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


COST_QUERIES = {
    "Engine": "select 1\n",
    "ExactFile": '''import rust
from File file
where file.getRelativePath() = "crates/oasis7_chain_runtime/src/host/executable_image_tests.rs" or
 file.getRelativePath().regexpMatch(".*/crates/oasis7_chain_runtime/src/host/executable_image_tests[.]rs")
select file.getRelativePath()
''',
    "MacroPopulation": "import rust\nselect count(MacroCall mc)\n",
    "SourceAst": QUERIES["SourceAst"],
}


def evidence_write(path, data, artifact_root):
    """A direct evidence writer shares the parent's combined prewrite cap."""
    total = sum(p.stat().st_size for p in artifact_root.rglob("*") if p.is_file())
    if total + len(data) > 63 * 1024**2:
        raise RuntimeError("combined evidence budget before direct write")
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("xb") as output:
        output.write(data)


def frame_complete_events(raw, destination, artifact_root=None):
    """Frame first JSON object prefixes; no unstable event fields are interpreted."""
    decoder = json.JSONDecoder()
    records = []
    partial = []
    output = bytearray()
    byte_offset = 0
    ignored_text_lines = 0
    for number, line in enumerate(raw.read_bytes().splitlines(keepends=True), 1):
        stripped = line.lstrip(b" \t")
        prefix_offset = len(line) - len(stripped)
        if not stripped.startswith(b"{"):
            ignored_text_lines += 1
            byte_offset += len(line)
            continue
        try:
            text = stripped.decode("utf-8")
            value, end = decoder.raw_decode(text)
            prefix = text[:end].encode("utf-8")
        except (ValueError, UnicodeError) as error:
            partial.append({"line": number, "byte_offset": byte_offset + prefix_offset,
                            "bytes": len(stripped), "sha256": hashlib.sha256(stripped).hexdigest(),
                            "error": str(error)[:160], "disposition": "UNKNOWN partial JSON; raw preserved"})
            byte_offset += len(line)
            continue
        if isinstance(value, dict):
            output.extend(prefix + b"\n")
            tail = stripped[len(prefix):]
            records.append({"line": number, "byte_offset": byte_offset + prefix_offset,
                            "object_bytes": len(prefix), "trailing_bytes": len(tail),
                            "trailing_sha256": hashlib.sha256(tail).hexdigest()})
        byte_offset += len(line)
    if artifact_root is None:
        with destination.open("xb") as stream:
            stream.write(output)
    else:
        evidence_write(destination, output, artifact_root)
    return {"complete_object_lines": len(records), "partial_or_non_object_lines": len(partial),
            "prefix_records": records, "partial_records": partial, "ignored_text_lines": ignored_text_lines,
            "event_schema_interpreted": False, "full_raw_preserved": True}


def artifact_request(opener, token, packet, end, archive=False):
    remaining = end - time.monotonic()
    if remaining <= 0:
        raise TimeoutError("fixed artifact inspection download expired")
    url = "https://api.github.com/repos/nil-studio/oasis7/actions/artifacts/" + str(packet["artifact_id"])
    if archive:
        url += "/zip"
    req = urllib.request.Request(url, headers={"Accept": "application/vnd.github+json", "X-GitHub-Api-Version": "2022-11-28"})
    req.add_unredirected_header("Authorization", "Bearer " + token)
    try:
        return opener.open(req, timeout=min(15, remaining))
    except urllib.error.HTTPError as error:
        raise RuntimeError("frozen artifact HTTP status " + str(error.code)) from None


def artifact_identity(opener, token, packet, end):
    with artifact_request(opener, token, packet, end) as response:
        raw = response.read(1024**2 + 1)
    if len(raw) > 1024**2:
        raise RuntimeError("artifact metadata cap")
    value = json.loads(raw)
    if (value.get("id") != packet["artifact_id"] or value.get("name") != packet["name"] or
        value.get("size_in_bytes") != packet["zip_bytes"] or value.get("digest") != "sha256:" + packet["zip_sha256"] or
        value.get("expired") is not False or value.get("workflow_run", {}).get("id") != packet["run"] or
        value.get("workflow_run", {}).get("head_sha") != packet["source"]):
        raise RuntimeError("frozen artifact identity changed")
    return value


def inspect_saved_evaluator_evidence(run, args, env):
    import os
    packet_path = Path(__file__).with_name("codeql-rust-evaluator-input.json")
    packet = json.loads(packet_path.read_text())
    if "GH_TOKEN" in env or "GITHUB_TOKEN" in env:
        raise RuntimeError("inspection token reached CLI env")
    token = os.environ.get("GH_TOKEN")
    if not token:
        raise RuntimeError("artifact read token missing")
    if shutil.disk_usage(args.work).free < 2 * 1024**3:
        raise RuntimeError("insufficient owned disk for bounded artifact inspection")
    end = min(args.deadline, time.monotonic() + 90)
    opener = urllib.request.build_opener(NoCredentialRedirect())
    before = artifact_identity(opener, token, packet, end)
    evidence_write(args.output / "artifact-before.json", (json.dumps(before, indent=2)+"\n").encode(), args.output)
    archive_path = args.work / "frozen-evaluator-artifact.zip"
    digest = hashlib.sha256(); total = 0
    with artifact_request(opener, token, packet, end, archive=True) as response, archive_path.open("xb") as stream:
        while True:
            if time.monotonic() >= end:
                raise TimeoutError("single artifact download expired")
            chunk = response.read(65536)
            if not chunk:
                break
            total += len(chunk)
            if total > packet["zip_bytes"] or total > 16 * 1024**2:
                raise RuntimeError("artifact download cap")
            stream.write(chunk); digest.update(chunk)
    if total != packet["zip_bytes"] or digest.hexdigest() != packet["zip_sha256"]:
        raise RuntimeError("artifact ZIP size/hash mismatch")
    after = artifact_identity(opener, token, packet, end)
    evidence_write(args.output / "artifact-after.json", (json.dumps(after, indent=2)+"\n").encode(), args.output)
    raw_names = ["cost-" + name + ".log" for name in COST_QUERIES]
    with zipfile.ZipFile(archive_path) as archive:
        infos = zip_inputs(archive)
        files = {info.filename: info for info in infos if not info.is_dir()}
        if set(files) != set(packet["members"]) or sum(info.file_size for info in infos) > 64 * 1024**2:
            raise RuntimeError("artifact member closure changed")
        for name, info in files.items():
            expected = packet["members"][name]
            if info.file_size != expected["bytes"]:
                raise RuntimeError("artifact member size mismatch")
            data = archive.read(info)
            if hashlib.sha256(data).hexdigest() != expected["sha256"]:
                raise RuntimeError("artifact member SHA mismatch")
            if name in raw_names:
                evidence_write(args.output / "original" / name, data, args.output)
    recoveries = {}
    for name in COST_QUERIES:
        source = args.output / "original" / f"cost-{name}.log"
        events = args.output / f"recovered-{name}.jsonl"
        recoveries[name] = frame_complete_events(source, events, args.output)
    evidence_write(args.output / "framing-offsets.json", (json.dumps(recoveries, indent=2)+"\n").encode(), args.output)
    library = args.codeql.resolve().parent / "qlpacks/codeql/rust-all/0.2.23"
    manifest = (library / "qlpack.yml").read_text()
    if re.findall(r"(?m)^name: (.+)$", manifest) != ["codeql/rust-all"] or re.findall(r"(?m)^version: (.+)$", manifest) != ["0.2.23"]:
        raise RuntimeError("bundled source pack identity mismatch")
    sources = {}
    source_bytes = 0
    for relative, tag_blob in packet["tag_git_blob_sha1"].items():
        source = library / "codeql/rust" / relative
        source.resolve().relative_to(args.codeql.resolve().parent)
        if source.is_symlink() or not source.is_file() or source.stat().st_size > 2 * 1024**2:
            raise RuntimeError("bundled source type/size rejected")
        data = source.read_bytes(); source_bytes += len(data)
        if source_bytes > 4 * 1024**2:
            raise RuntimeError("combined bundled source cap")
        blob = hashlib.sha1(b"blob " + str(len(data)).encode() + b"\0" + data).hexdigest()
        sources[relative] = {"sha256": hashlib.sha256(data).hexdigest(), "git_blob_sha1": blob,
                             "tag_git_blob_sha1": tag_blob, "matches_tag": blob == tag_blob}
        evidence_write(args.output / "bundle-source" / relative, data, args.output)
    evidence_write(args.output / "bundle-source-identity.json", (json.dumps(sources, indent=2)+"\n").encode(), args.output)
    cli = str(args.codeql)
    run([cli, "generate", "log-summary", "--help"], args.work,
        args.output / "official-summary-help.log", args.deadline, 20, args.output, env)
    help_text = (args.output / "official-summary-help.log").read_text()
    if "--format=" not in help_text or "text" not in help_text:
        raise RuntimeError("exact CLI summary text option absent")
    summaries = {}
    for name in COST_QUERIES:
        if args.deadline - time.monotonic() < 40:
            summaries[name] = "UNKNOWN: insufficient final remaining budget"
            break
        try:
            run([cli, "generate", "log-summary", "--format=text", str(args.output / f"recovered-{name}.jsonl")],
                args.work, args.output / f"official-{name}-summary.log", args.deadline - 15, 20, args.output, env)
            summaries[name] = "official command completed; interpretation requires review"
        except RuntimeError:
            summaries[name] = "UNKNOWN: official summary failed; raw retained; no further subprocess"
            break
    evidence_write(args.output / "inspection-result.json", (json.dumps({
        "disposition": "REQUIRES_INDEPENDENT_EVIDENCE_REVIEW", "artifact_id": packet["artifact_id"],
        "packet_sha256": hashlib.sha256(packet_path.read_bytes()).hexdigest(), "summaries": summaries,
        "bundle_sources_match_tag": all(item["matches_tag"] for item in sources.values()),
        "database_downloads": 0, "repository_extractions": 0, "query_compiles": 0, "query_evaluations": 0,
        "coverage": "NOT_EVALUATED", "rust_baseline": "HOLD"}, indent=2)+"\n").encode(), args.output)
    if len(summaries) != 4 or any(value.startswith("UNKNOWN") for value in summaries.values()):
        raise RuntimeError("postprocess retained incomplete summary; no query rerun")


def cost_probes(run, args, env, query, packs, cache, database):
    cli = str(args.codeql)
    help_text = (args.output / "query-run-help.log").read_text()
    for option in ("--evaluator-log=", "--evaluator-log-minify", "--[no-]tuple-counting", "--verbosity="):
        if option not in help_text:
            raise RuntimeError("exact CLI cost logging option unavailable: " + option)
    run([cli, "generate", "log-summary", "--help"], args.work,
        args.output / "log-summary-help.log", args.deadline, 20, args.output, env)
    summary_help = (args.output / "log-summary-help.log").read_text()
    if "--format=" not in summary_help or "text" not in summary_help:
        raise RuntimeError("exact CLI text evaluator summary unavailable")
    results = []
    timed_out = False
    stop = None
    summary_errors = []
    unsafe_stop = False
    try:
        for name in COST_QUERIES:
            if args.deadline - time.monotonic() < 210:
                stop = "insufficient original remaining budget for next120 plus90 reserve"
                break
            log = args.output / f"cost-{name}.log"
            command = [cli, "query", "run", str(query / (name + ".ql")),
                "--database=" + str(database), "--output=" + str(args.output / f"cost-{name}.bqrs"),
                "--additional-packs=" + str(packs), "--compilation-cache=" + str(cache),
                "--threads=2", "--ram=3072", "--verbosity=progress+++", "--tuple-counting",
                "--evaluator-log=/dev/stderr", "--evaluator-log-minify"]
            try:
                run(command, args.work, log, args.deadline - 90, 120, args.output, env)
            except RuntimeError:
                result = json.loads(log.with_suffix(".log.result.json").read_text())
                results.append({"probe": name, "result": result})
                if result["error"] != "TimeoutError('diagnostic phase deadline exhausted')" or result["cleanup_errors"]:
                    raise
                timed_out = True
            else:
                results.append({"probe": name, "result": json.loads(log.with_suffix(".log.result.json").read_text())})
                run([cli, "bqrs", "decode", str(args.output / f"cost-{name}.bqrs"), "--format=csv",
                    "--output=" + str(args.output / f"cost-{name}.csv")], args.work,
                    args.output / f"cost-{name}-decode.log", args.deadline - 90, 20, args.output, env)
    except BaseException:
        unsafe_stop = True
        raise
    finally:
        # Raw remains authoritative even if framing/official summary cannot finish.
        for item in results:
            name = item["probe"]
            if unsafe_stop:
                item["summary"] = "not launched: non-allowlisted failure or cleanup uncertainty"
                continue
            if args.deadline - time.monotonic() < 25:
                item["summary"] = "not launched: final time reserve"
                continue
            events = args.work / f"cost-{name}-events.jsonl"
            try:
                item["framing"] = frame_complete_events(args.output / f"cost-{name}.log", events)
            except (OSError, ValueError) as error:
                item["framing_error"] = repr(error)
                summary_errors.append(repr(error))
                break
            if not item["framing"]["complete_object_lines"]:
                item["summary"] = "UNKNOWN: no complete structured object lines; full raw retained"
                continue
            try:
                run([cli, "generate", "log-summary", "--format=text", str(events)], args.work,
                    args.output / f"cost-{name}-summary.log", args.deadline - 15, 10, args.output, env)
                item["summary"] = "official text summary produced; no custom event-schema inference"
            except RuntimeError:
                item["summary"] = "UNKNOWN: official summary failed or partial; raw/result preserved"
                summary_errors.append("official summary failed; no further subprocess launched")
                break
        (args.output / "cost-probe-results.json").write_text(json.dumps({
            "probes": results, "stop": stop, "unsafe_stop": unsafe_stop, "summary_errors": summary_errors, "coverage": "NOT_EVALUATED", "rust_baseline": "HOLD",
            "query_predicate_changed": False, "structured_log_transport": "/dev/stderr via bounded parent pipe"
        }, indent=2) + "\n")
    if timed_out or stop or summary_errors:
        raise RuntimeError("cost diagnostic retained timeout/budget evidence; no coverage conclusion")


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
    if getattr(args, "evaluator_cost_probes", False):
        cost_probes(run, args, env, query, packs, cache, database)
        return
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
