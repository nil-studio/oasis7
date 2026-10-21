"""Read the existing production DB; never extract or upload a new database."""
import hashlib
import json
from pathlib import Path, PurePosixPath
import re
import shutil
import stat
import time
import urllib.request
import urllib.error
import zipfile

DB_ID = 587211101
DB_COMMIT = "8957026492b2b2e3851c99a8f9908ef61a68295d"
DB_BYTES = 610635853
API = "https://api.github.com/repos/nil-studio/oasis7/code-scanning/codeql/databases/rust"
TARGET = "crates/oasis7_chain_runtime/src/host/executable_image_tests.rs"
SOURCES = {
 "crates/oasis7_chain_runtime/src/lib.rs": "cb4f444f1daa62b146537a4b016cbd2967cee90dada9bf3eefea5372b7f2f9de",
 "crates/oasis7_chain_runtime/src/host.rs": "4000f4eb86bb2bfa730c7467ddd90e8a9173f8aaad2529af5e6a608051e5b20b",
 "crates/oasis7_chain_runtime/src/host/executable_image.rs": "304e886d1f5d3aace4528714ca333d65c1ae1b08063b53962c16c8356bfff166",
 TARGET: "8b73e2270486bd2bb1e54b5421b4d2a2fff3cfbd56b46f8e8cdc3a86a96d641b"}

# Predicates are fixed before the run. Existence is exactly count > 0, not a
# filter that drops generated descendants from a previously accepted scope.
QUERIES = {
"SourceAst": '''import rust
from MacroCall mc
where (mc.getLocation().getFile().getRelativePath() = "crates/oasis7_chain_runtime/src/host/executable_image_tests.rs" or
 mc.getLocation().getFile().getRelativePath().regexpMatch(".*/crates/oasis7_chain_runtime/src/host/executable_image_tests[.]rs")) and
 mc.getLocation().getStartLine() = [100, 102, 104, 107, 110, 111]
select mc.getLocation().getFile().getRelativePath(), mc.getLocation().getStartLine(),
 mc.getLocation().getEndLine(), mc.getPath().toString()
''',
"MacroLayers": '''import rust
from File file, string layer, int line, string path, int entity, int expanded
where (file.getRelativePath() = "crates/oasis7_chain_runtime/src/host/executable_image_tests.rs" or file.getRelativePath().regexpMatch(".*/crates/oasis7_chain_runtime/src/host/executable_image_tests[.]rs")) and ((layer = "unconditional" and line = 100 and path = "assert") or
 (layer = "mac-outer" and line = 102 and path = "assert") or
 (layer = "mac-panic" and line = 104 and path = "panic") or
 (layer = "linux-outer" and line = 107 and path = "assert") or
 (layer = "linux-format" and line = 110 and path = "format") or
 (layer = "linux-panic" and line = 111 and path = "panic"))
 and ((entity = 1 and exists(MacroCall mc | mc.getLocation().getFile() = file and
 mc.getLocation().getStartLine() = line and mc.getPath().toString().matches("%" + path + "%"))) or
 (entity = 0 and not exists(MacroCall mc | mc.getLocation().getFile() = file and
 mc.getLocation().getStartLine() = line and mc.getPath().toString().matches("%" + path + "%")))) and
 ((expanded = 1 and exists(MacroCall mc | mc.getLocation().getFile() = file and
 mc.getLocation().getStartLine() = line and mc.getPath().toString().matches("%" + path + "%") and exists(mc.getMacroCallExpansion()))) or
 (expanded = 0 and not exists(MacroCall mc | mc.getLocation().getFile() = file and
 mc.getLocation().getStartLine() = line and mc.getPath().toString().matches("%" + path + "%") and exists(mc.getMacroCallExpansion()))))
select file.getRelativePath(), layer, line, entity, expanded
''',
"MacroFlow": '''import rust
import codeql.rust.dataflow.DataFlow
from File file, int line, int present, string path
where (file.getRelativePath() = "crates/oasis7_chain_runtime/src/host/executable_image_tests.rs" or file.getRelativePath().regexpMatch(".*/crates/oasis7_chain_runtime/src/host/executable_image_tests[.]rs")) and ((line = [100, 102, 107] and path = "assert") or
 (line = [104, 111] and path = "panic") or (line = 110 and path = "format")) and
 ((present = 1 and exists(MacroCall mc, DataFlow::Node n |
 mc.getLocation().getFile() = file and
 mc.getLocation().getStartLine() = line and mc.getPath().toString().matches("%" + path + "%") and
 mc.getMacroCallExpansion() = n.asExpr().getParentNode*())) or
 (present = 0 and not exists(MacroCall mc, DataFlow::Node n |
 mc.getLocation().getFile() = file and
 mc.getLocation().getStartLine() = line and mc.getPath().toString().matches("%" + path + "%") and
 mc.getMacroCallExpansion() = n.asExpr().getParentNode*())))
select file.getRelativePath(), line, path, present
''',
"ClosureFlow": '''import rust
import codeql.rust.dataflow.DataFlow
from File file, int present
where (file.getRelativePath() = "crates/oasis7_chain_runtime/src/host/executable_image_tests.rs" or file.getRelativePath().regexpMatch(".*/crates/oasis7_chain_runtime/src/host/executable_image_tests[.]rs")) and (
 (present = 1 and exists(DataFlow::Node source, DataFlow::Node sink |
 source.asParameter().getLocation().getFile() = file and
 source.asParameter().getLocation().getStartLine() = 110 and sink.asExpr() instanceof PathExpr and
 sink.asExpr().getLocation().getFile() = file and
 sink.asExpr().getLocation().getStartLine() = 110 and DataFlow::localFlow(source, sink))) or
 (present = 0 and not exists(DataFlow::Node source, DataFlow::Node sink |
 source.asParameter().getLocation().getFile() = file and
 source.asParameter().getLocation().getStartLine() = 110 and sink.asExpr() instanceof PathExpr and
 sink.asExpr().getLocation().getFile() = file and
 sink.asExpr().getLocation().getStartLine() = 110 and DataFlow::localFlow(source, sink)))
)
select file.getRelativePath(), "linux-closure-parameter-use", present
'''}


class NoCredentialRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, request, fp, code, message, headers, url):
        if not url.startswith("https://"):
            raise RuntimeError("non-HTTPS database redirect")
        redirected = super().redirect_request(request, fp, code, message, headers, url)
        if redirected is not None:
            redirected.remove_header("Authorization")
        return redirected


def request(opener, token, accept, end):
    remaining = end - time.monotonic()
    if remaining <= 0:
        raise TimeoutError("database download budget expired before request")
    req = urllib.request.Request(API, headers={"Accept": accept, "X-GitHub-Api-Version": "2022-11-28"})
    req.add_unredirected_header("Authorization", "Bearer " + token)
    try:
        return opener.open(req, timeout=min(15, remaining))
    except urllib.error.HTTPError as error:
        raise RuntimeError("official database HTTP status " + str(error.code)) from None


def identity(opener, token, end):
    with request(opener, token, "application/vnd.github+json", end) as response:
        raw = response.read(1024 * 1024 + 1)
    if len(raw) > 1024 * 1024:
        raise RuntimeError("database metadata too large")
    value = json.loads(raw)
    if value.get("id") != DB_ID or value.get("commit_oid") != DB_COMMIT or value.get("size") != DB_BYTES or value.get("language") != "rust":
        raise RuntimeError("official database identity changed")
    return value


def zip_inputs(archive, nested=False):
    infos = archive.infolist()
    if len(infos) > 100000 or sum(info.file_size for info in infos) > (4 if nested else 8) * 1024**3:
        raise RuntimeError("database ZIP expansion budget exceeded")
    names = set()
    for info in infos:
        original = info.filename
        # Inner source keys may be absolute archival identities; never extract
        # them or join them onto filesystem destinations.
        name = original.lstrip("/") if nested else original
        parts = PurePosixPath(name).parts
        canonical = str(PurePosixPath(name))
        if not name or original.startswith("/") and not nested or "\\" in name or "\x00" in name or ".." in parts or canonical != name.rstrip("/"):
            raise RuntimeError("unsafe database ZIP member")
        kind = stat.S_IFMT(info.external_attr >> 16)
        if canonical in names or kind not in (0, stat.S_IFREG, stat.S_IFDIR):
            raise RuntimeError("duplicate/link database ZIP member")
        names.add(canonical)
    return infos


def execute(run, args, env, query, packs, cache):
    import csv
    import os
    token = os.environ.get("GH_TOKEN")
    if not token:
        raise RuntimeError("read-only database token unavailable")
    if "GH_TOKEN" in env or "GITHUB_TOKEN" in env:
        raise RuntimeError("database token would reach CodeQL subprocess")
    end = min(args.deadline, time.monotonic() + 180)
    opener = urllib.request.build_opener(NoCredentialRedirect())
    before = identity(opener, token, end)
    (args.output / "database-before.json").write_text(json.dumps(before, indent=2) + "\n")
    if shutil.disk_usage(args.work).free < 2 * 1024**3:
        raise RuntimeError("insufficient owned CI disk before database download")
    bundle = args.work / "production-database.zip"
    digest = hashlib.sha256(); total = 0
    with request(opener, token, "application/zip", end) as response, bundle.open("xb") as stream:
        while True:
            if time.monotonic() >= end:
                raise TimeoutError("single database download expired")
            chunk = response.read(1024 * 1024)
            if not chunk:
                break
            total += len(chunk)
            if total > DB_BYTES or total > 768 * 1024**2:
                raise RuntimeError("database download byte cap")
            stream.write(chunk); digest.update(chunk)
    if total != DB_BYTES:
        raise RuntimeError("database download incomplete")
    after = identity(opener, token, end)
    (args.output / "database-after.json").write_text(json.dumps(after, indent=2) + "\n")
    with zipfile.ZipFile(bundle) as archive:
        infos = zip_inputs(archive)
        max_member_bytes = max(info.file_size for info in infos)
        member_limit = max(64 * 1024**2, max_member_bytes)
        required = sum(info.file_size for info in infos) + 1024**3
        if shutil.disk_usage(args.work).free < required:
            raise RuntimeError("insufficient owned disk for database plus query cache")
        (args.output / "database-archive.json").write_text(json.dumps({"bytes": total, "sha256": digest.hexdigest(), "uncompressed_bytes": required - 1024**3, "entries": len(infos), "max_member_bytes": max_member_bytes, "unbundle_file_limit_bytes": member_limit, "members": [{"name": info.filename, "bytes": info.file_size} for info in infos]}, indent=2) + "\n")
    cli = str(args.codeql)
    run([cli, "database", "unbundle", "--help"], args.work, args.output / "unbundle-help.log", args.deadline, 20, args.output, env)
    if "--target" not in (args.output / "unbundle-help.log").read_text():
        raise RuntimeError("exact CLI unbundle target option unavailable")
    database = args.work / "production-database"
    if database.exists():
        raise RuntimeError("owned unbundle destination already exists")
    run([cli, "database", "unbundle", "--target=" + str(args.work), "--name=production-database", str(bundle)], args.work,
        args.output / "database-unbundle.log", args.deadline, 120, args.output, env, file_limit=member_limit)
    descriptor = database / "codeql-database.yml"
    raw = descriptor.read_text()
    (args.output / "production-database.yml").write_text(raw)
    if not re.search(r"(?m)^\s*cliVersion: ['\"]?2[.]27[.]2['\"]?\s*$", raw):
        raise RuntimeError("production DB CLI version unknown or mismatched")
    schemas = list(database.rglob("rust.dbscheme"))
    expected = args.codeql.parent / "qlpacks/codeql/rust-all/0.2.23/rust.dbscheme"
    if len(schemas) != 1 or schemas[0].read_bytes() != expected.read_bytes():
        raise RuntimeError("production DB schema differs from pinned library")
    with zipfile.ZipFile(database / "src.zip") as archive:
        infos = zip_inputs(archive, nested=True)
        source_hashes = {}
        for path, expected_sha in SOURCES.items():
            matches = [info for info in infos if info.filename.lstrip("/").endswith("/" + path) or info.filename == path]
            if len(matches) != 1 or matches[0].file_size > 512 * 1024:
                raise RuntimeError("production archived source identity ambiguous")
            source = archive.read(matches[0])
            actual = hashlib.sha256(source).hexdigest()
            if actual != expected_sha:
                raise RuntimeError("production archived module/source changed")
            source_hashes[path] = {"archive_key": matches[0].filename, "sha256": actual}
    (args.output / "production-source-hashes.json").write_text(json.dumps(source_hashes, indent=2) + "\n")
    # Save independent source/closure observations before the previously costly layer query.
    for name in ("SourceAst", "ClosureFlow", "MacroFlow", "MacroLayers"):
        bqrs = args.output / f"production-{name}.bqrs"
        run([cli, "query", "run", str(query / (name + ".ql")), "--database=" + str(database), "--output=" + str(bqrs),
             "--additional-packs=" + str(packs), "--compilation-cache=" + str(cache), "--threads=2", "--ram=3072"],
            args.work, args.output / f"production-{name}.log", args.deadline, 120, args.output, env)
        run([cli, "bqrs", "decode", str(bqrs), "--format=csv", "--output=" + str(args.output / f"production-{name}.csv")],
            args.work, args.output / f"production-{name}-decode.log", args.deadline, 20, args.output, env)
    (args.output / "production-macro-result.json").write_text(json.dumps({
        "disposition": "REQUIRES_INDEPENDENT_LAYER_REVIEW", "database_id": DB_ID, "source": DB_COMMIT,
        "reextractions": 0, "uploads": 0, "rust_baseline": "HOLD"}, indent=2) + "\n")
