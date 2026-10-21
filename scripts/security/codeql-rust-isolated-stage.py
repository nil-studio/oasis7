"""One annotation-only, owned-copy experiment; never edits the official library."""
import csv
import hashlib
import json
import os
from pathlib import Path
import re
import stat
import shutil
import sys
import time

STAGE = "codeql/rust/internal/CachedStages.qll"
BEFORE_SHA = "e9ec32dedd08185b23787efb12879032dc7a7c0cc59db2b48dd49202d54b5754"
OLD = "  cached\n  module AstStage {"
NEW = "  module AstStage {"
LOCATABLE = "codeql/rust/elements/internal/LocatableImpl.qll"
LOCATION_SHA = "8c6f8f3cb1fb5cbaa85a5f61a5d4d50e1a15bb70948f5e2508076fbce97a0da9"
LOCATION_OLD = "    cached\n    Location getLocation() {"
LOCATION_NEW = "    Location getLocation() {"
MACRO_MODULE = "codeql/rust/elements/internal/ElementImpl.qll"
MACRO_SHA = "b5648da97c3efedd78184d032086603e54f8173f5e85147f37660fb18ad2a8fb"
MACRO_OLD = "  cached\n  module MacroExpansion {"
MACRO_NEW = "  module MacroExpansion {"


def inventory(root):
    result = {}
    total = 0
    for path in sorted(root.rglob("*")):
        mode = path.lstat().st_mode
        if stat.S_ISDIR(mode):
            continue
        if not stat.S_ISREG(mode) or path.suffix in (".qlx", ".qlo") or ".cache" in path.parts:
            raise RuntimeError("non-source/linked official pack input")
        total += path.stat().st_size
        if total > 256 * 1024**2 or len(result) >= 10000:
            raise RuntimeError("library copy source budget exceeded")
        result[str(path.relative_to(root))] = hashlib.sha256(path.read_bytes()).hexdigest()
    if not result:
        raise RuntimeError("empty library")
    return result


def copy_and_patch(original, candidate, uncache_location=False, ungroup_macro=False):
    before = inventory(original)
    if before.get(STAGE) != BEFORE_SHA:
        raise RuntimeError("official CachedStages differs from frozen bundle")
    candidate.mkdir(exist_ok=False)
    for name, digest in before.items():
        target = candidate / name
        target.parent.mkdir(parents=True, exist_ok=True)
        data = (original / name).read_bytes()
        if hashlib.sha256(data).hexdigest() != digest:
            raise RuntimeError("library source changed during copy")
        target.write_bytes(data)
    stage = candidate / STAGE
    source = stage.read_text()
    if source.count(OLD) != 1:
        raise RuntimeError("AstStage annotation patch is not unique")
    stage.write_text(source.replace(OLD, NEW, 1))
    changed = {STAGE}
    if uncache_location:
        if before.get(LOCATABLE) != LOCATION_SHA:
            raise RuntimeError("official Locatable differs from frozen bundle")
        location = candidate / LOCATABLE
        text = location.read_text()
        if text.count(LOCATION_OLD) != 1:
            raise RuntimeError("cached public location annotation is not unique")
        location.write_text(text.replace(LOCATION_OLD, LOCATION_NEW, 1))
        if location.read_text().replace(LOCATION_NEW, LOCATION_OLD, 1) != text:
            raise RuntimeError("location patch changes logical expressions")
        changed.add(LOCATABLE)
    if ungroup_macro:
        if not uncache_location or before.get(MACRO_MODULE) != MACRO_SHA:
            raise RuntimeError("macro grouping candidate lacks frozen prerequisites")
        macro = candidate / MACRO_MODULE
        text = macro.read_text()
        if text.count(MACRO_OLD) != 1:
            raise RuntimeError("macro module annotation is not unique")
        macro.write_text(text.replace(MACRO_OLD, MACRO_NEW, 1))
        if macro.read_text().replace(MACRO_NEW, MACRO_OLD, 1) != text:
            raise RuntimeError("macro patch changes logical expressions")
        changed.add(MACRO_MODULE)
    after = inventory(candidate)
    if {name for name in before if before[name] != after.get(name)} != changed or set(before) != set(after):
        raise RuntimeError("candidate changes more than declared annotation files")
    if stage.read_text().replace(NEW, OLD, 1) != source:
        raise RuntimeError("candidate changes logical source expressions")
    return before, after


def inspect_location_plans(driver, production, args, env, ungroup_macro=False):
    cli = str(args.codeql)
    official = args.codeql.parent / "qlpacks/codeql/rust-all/0.2.23"
    before = inventory(official)
    save = lambda name, value: production.evidence_write(args.output / name, (json.dumps(value, indent=2)+"\n").encode(), args.output)
    error = None
    try:
        help_path = args.output / "isolated-query-compile-help.log"
        driver.run([cli, "query", "compile", "--help", "-v"], args.work, help_path, args.deadline - 90, 20, args.output, env)
        if "--[no-]dump-ra" not in help_path.read_text():
            raise RuntimeError("exact CLI RA output support unknown")
        if args.deadline - time.monotonic() < 450 or shutil.disk_usage(args.work).free < 4 * 1024**3:
            raise RuntimeError("insufficient original compile/space budget")
        end = min(args.deadline - 90, time.monotonic() + 360)
        plans = {}
        variants = (("ungroup-macro", True),) if ungroup_macro else (("stage-only", False), ("demand-location", True))
        for name, uncache_location in variants:
            copy = args.work / ("library-plan-" + name)
            original_hashes, copied = copy_and_patch(official, copy, uncache_location, ungroup_macro)
            save("library-plan-" + name + ".json", {"official": original_hashes, "candidate": copied,
                "annotation_deletions": 3 if ungroup_macro else 2 if uncache_location else 1})
            query = args.output / ("query-plan-" + name)
            driver.write(query / "qlpack.yml", "name: oasis7/rust-model-diagnostic\nversion: 0.0.0\ndependencies:\n  codeql/rust-all: 0.2.23\n")
            driver.write(query / "SourceAst.ql", production.QUERIES["SourceAst"])
            roots = [copy if key == "rust-all" else args.codeql.parent / "qlpacks/codeql" / key / version for key, version in driver.LIBRARY_VERSIONS.items()]
            packs = os.pathsep.join(map(str, roots))
            path = args.output / ("plan-resolver-" + name + ".json")
            driver.run([cli, "resolve", "library-path", "--query=" + str(query / "SourceAst.ql"), "--format=json", "--additional-packs=" + packs],
                args.work, path, end, 20, args.output, env)
            verify_resolver(json.loads(path.read_text()), roots, query, "SourceAst", copy)
            cache = args.work / ("plan-cache-" + name)
            cache.mkdir(exist_ok=False)
            driver.run([cli, "query", "compile", "--dump-ra", "--compilation-cache=" + str(cache), "--no-default-compilation-cache",
                "--common-caches=" + str(args.work / ("plan-common-" + name)), "--additional-packs=" + packs,
                "--threads=2", "--ram=3072", str(query / "SourceAst.ql")], args.work,
                args.output / ("plan-ra-" + name + ".log"), end, 360, args.output, env)
            plans[name] = {"query_sha256": hashlib.sha256((query / "SourceAst.ql").read_bytes()).hexdigest(),
                           "library_root": str(copy), "compilation_cache": str(cache)}
        save("location-plan-result.json", {"disposition": "REQUIRES_INDEPENDENT_PLAN_REVIEW", "plans": plans,
            "database_downloads": 0, "extractions": 0, "query_evaluations": 0, "coverage": "HOLD",
            "fixed_wall_deadline": getattr(args, "fixed_wall_deadline", None),
            "remaining_cached_roots": ["enclosing-macro", "TElement", "TLocation", "Element.toString"],
            "cut_scope": "identified forced-location module group, not every cache root"})
    except BaseException as exc:
        error = exc
    finally:
        try:
            after = inventory(official)
            save("location-plan-official-after.json", {"unchanged": before == after, "hashes": after})
            if before != after:
                raise RuntimeError("official library changed during plan inspection")
        except BaseException as exc:
            if error is None:
                error = exc
            else:
                print("secondary official-library verification failure: " + repr(exc), file=sys.stderr)
    if error is not None:
        raise error


def verify_resolver(value, roots, query, name, library):
    keys = {"libraryPath", "dbscheme", "compilationCache", "relativeName", "qlPackName"}
    if not isinstance(value, dict) or set(value) != keys:
        raise RuntimeError("unknown library-path shape")
    paths = value["libraryPath"]
    if not isinstance(paths, list) or len(paths) != len(set(paths)) or set(paths) != set(map(str, roots)) | {str(query)}:
        raise RuntimeError("isolated dependency roots differ")
    if value["dbscheme"] != str(library / "rust.dbscheme") or value["qlPackName"] != "oasis7/rust-model-diagnostic" or value["relativeName"] != f"oasis7/rust-model-diagnostic/{name}.ql":
        raise RuntimeError("isolated source/schema identity differs")


def tables(root, variant):
    result = {}
    for name in ("Macros", "Calls", "Closures"):
        with (root / f"isolated-{variant}-{name}.csv").open() as stream:
            rows = list(csv.reader(stream))
        if not rows:
            raise RuntimeError("reference table missing header")
        result[name] = {"header": rows[0], "rows": sorted(rows[1:])}
    calls, macros, closures = (result[n]["rows"] for n in ("Calls", "Macros", "Closures"))
    if not any(r[0].endswith("src/lib.rs") and r[2] == "version_one_sink" and int(r[4]) > 0 for r in calls):
        raise RuntimeError("reference direct call/flow positive absent")
    if not any(r[0].endswith("src/lib.rs") and int(r[3]) > 0 and int(r[4]) > 0 for r in macros):
        raise RuntimeError("reference generated expansion/flow absent")
    orphan = [r for r in macros if r[0].endswith("orphan.rs")]
    if not orphan or any(int(r[3]) or int(r[4]) for r in orphan):
        raise RuntimeError("reference orphan zero control differs")
    for line, name in (("4", "format"), ("5", "assert"), ("5", "format"), ("5", "...::panic_2021")):
        if not any(r[0].endswith("panic_shape.rs") and r[1] == line and r[2] == name
                   and int(r[3]) > 0 and int(r[4]) > 0 for r in macros):
            raise RuntimeError("reference nested expansion/flow positive absent")
    if len(closures) != 1 or closures[0][0] != "panic-closure" or int(closures[0][1]) < 1 or int(closures[0][2]) < 1:
        raise RuntimeError("reference closure AST/flow positive absent")
    return result


def experiment(driver, production, args, env):
    run = driver.run
    cli = str(args.codeql)
    official = args.codeql.parent / "qlpacks/codeql/rust-all/0.2.23"
    copy = args.work / "isolated-rust-all"
    if shutil.disk_usage(args.work).free < 4 * 1024**3:
        raise RuntimeError("insufficient owned space for isolated library/DB/cache")
    before = inventory(official)
    save = lambda name, value: production.evidence_write(args.output / name, (json.dumps(value, indent=2)+"\n").encode(), args.output)
    failure = None
    try:
        for toolchain in ("1.96.0", "1.98.1"):
            run(["rustup", "run", toolchain, "rustc", "-vV"], args.work,
                args.output / ("isolated-rustc-" + toolchain + ".log"), args.deadline - 90, 20, args.output, env)
        save("isolated-inputs.json", {"helper_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
            "production_queries": {name: hashlib.sha256(text.encode()).hexdigest() for name, text in production.QUERIES.items()},
            "reference_queries": {name: hashlib.sha256(text.encode()).hexdigest() for name, text in
                (("Macros", driver.MACROS), ("Calls", driver.CALLS), ("Closures", driver.CLOSURES))},
            "ram_requested_mib": 3072, "hard_rss_cap": False})
        before, copied = copy_and_patch(official, copy)
        save("isolated-library.json", {"official_before": before, "candidate": copied,
            "only_change": "remove AstStage module cached annotation", "official": str(official), "candidate_root": str(copy)})
        root = args.work / "reference-source"
        driver.fixture(root, False)
        for child in (".", "candidate", "legacy"):
            run(["cargo", "generate-lockfile", "--offline", "--manifest-path", str(root / child / "Cargo.toml")], root,
                args.output / ("reference-lock-" + child.replace(".", "root") + ".log"), args.deadline - 90, 20, args.output, env)
        source_hashes = driver.hashes(root)
        save("isolated-reference-source.json", source_hashes)
        compile_end = min(args.deadline - 90, time.monotonic() + 360)
        variants = {}
        for variant, library in (("original", official), ("candidate", copy)):
            query = args.output / ("isolated-queries-" + variant)
            driver.write(query / "qlpack.yml", "name: oasis7/rust-model-diagnostic\nversion: 0.0.0\ndependencies:\n  codeql/rust-all: 0.2.23\n")
            texts = {"Macros": driver.MACROS, "Calls": driver.CALLS, "Closures": driver.CLOSURES}
            if variant == "candidate":
                texts["SourceAst"] = production.QUERIES["SourceAst"]
            roots = [library if key == "rust-all" else args.codeql.parent / "qlpacks/codeql" / key / version for key, version in driver.LIBRARY_VERSIONS.items()]
            packs = os.pathsep.join(map(str, roots))
            cache = args.work / ("isolated-cache-" + variant)
            cache.mkdir(exist_ok=False)
            for name, text in texts.items():
                driver.write(query / (name + ".ql"), text)
                path = args.output / f"isolated-resolve-{variant}-{name}.json"
                run([cli, "resolve", "library-path", "--query=" + str(query / (name + ".ql")), "--format=json", "--additional-packs=" + packs], args.work,
                    path, args.deadline - 90, 20, args.output, env)
                verify_resolver(json.loads(path.read_text()), roots, query, name, library)
            run([cli, "query", "compile", "--keep-going", "--compilation-cache=" + str(cache), "--no-default-compilation-cache",
                 "--common-caches=" + str(args.work / ("isolated-common-" + variant)), "--additional-packs=" + packs, "--threads=2", "--ram=3072",
                 *[str(query / (name + ".ql")) for name in texts]], args.work,
                args.output / ("isolated-compile-" + variant + ".log"), compile_end, 360, args.output, env)
            variants[variant] = (query, packs, cache)
        for variant in variants:
            if args.deadline - time.monotonic() < 270:
                raise RuntimeError("insufficient original budget for reference DB/query plus90 reserve")
            if shutil.disk_usage(args.work).free < 4 * 1024**3:
                raise RuntimeError("insufficient owned reference DB/cache space")
            database = args.work / ("reference-db-" + variant)
            run([cli, "database", "create", str(database), "--language=rust", "--build-mode=none", "--source-root=" + str(root), "--threads=2", "--ram=3072"], root,
                args.output / ("isolated-create-" + variant + ".log"), args.deadline - 90, 90, args.output, env, file_limit=1024**3)
            query, packs, cache = variants[variant]
            run([cli, "database", "run-queries", "--additional-packs=" + packs, "--compilation-cache=" + str(cache), "--no-default-compilation-cache",
                 "--common-caches=" + str(args.work / ("isolated-common-" + variant)), "--max-disk-cache=1024", "--threads=2", "--ram=3072", "--", str(database),
                 *[str(query / (name + ".ql")) for name in ("Macros", "Calls", "Closures")]], args.work,
                args.output / ("isolated-reference-" + variant + ".log"), args.deadline - 90, 90, args.output, env, file_limit=1024**3)
            for name in ("Macros", "Calls", "Closures"):
                matches = list((database / "results").rglob(name + ".bqrs"))
                if len(matches) != 1 or matches[0].is_symlink():
                    raise RuntimeError("reference BQRS missing or ambiguous")
                run([cli, "bqrs", "decode", str(matches[0]), "--format=csv", "--output=" + str(args.output / f"isolated-{variant}-{name}.csv")], args.work,
                    args.output / f"isolated-decode-{variant}-{name}.log", args.deadline - 90, 20, args.output, env)
        original, candidate = tables(args.output, "original"), tables(args.output, "candidate")
        if original != candidate or {name: hashlib.sha256((root / name).read_bytes()).hexdigest() for name in source_hashes} != source_hashes:
            raise RuntimeError("reference semantic tuples/source differ")
        save("isolated-reference-comparison.json", {"equal": True, "tables": original, "scope": "representative controls; not full production equivalence"})
        if args.deadline - time.monotonic() < 420:
            raise RuntimeError("insufficient original budget for production identity/download/evaluation plus reserve")
        query, packs, cache = variants["candidate"]
        args.isolated_library_source_ast = True
        production.execute(run, args, env, query, packs, cache)
    except BaseException as exc:
        failure = exc
    finally:
        try:
            after = inventory(official)
            save("isolated-official-after.json", {"unchanged": before == after, "sha256": after})
            if before != after:
                raise RuntimeError("official library changed during isolated experiment")
        except BaseException as exc:
            if failure is None:
                failure = exc
            else:
                print("secondary official-library verification failure: " + repr(exc), file=sys.stderr)
    if failure is not None:
        raise failure
