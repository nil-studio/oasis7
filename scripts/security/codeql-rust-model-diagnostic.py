#!/usr/bin/env python3
"""Manual, bounded CodeQL model experiment; never uploads Code Scanning results."""
import argparse
import importlib.util
import csv
import hashlib
import json
import os
from pathlib import Path
import platform
import resource
import re
import selectors
import shutil
import signal
import subprocess
import sys
import time

CAP = 64 * 1024 * 1024
PEER = '''pub fn peer_flow(value: i32) -> i32 { model_dep::route!(value) }
fn main() { let _ = peer_flow(7); }
'''
MACROS = '''import rust
import codeql.rust.dataflow.DataFlow
from MacroCall mc
where mc.getLocation().getFile().getRelativePath().regexpMatch(".*(peer|lib|orphan|panic_shape)[.]rs")
select mc.getLocation().getFile().getRelativePath(), mc.getLocation().getStartLine(),
  mc.getPath().toString(), count(AstNode n | mc.getMacroCallExpansion() = n.getParentNode*()),
  count(DataFlow::Node n | mc.getMacroCallExpansion() = n.asExpr().getParentNode*())
'''
CALLS = '''import rust
import codeql.rust.dataflow.DataFlow
from Call c
where c.getTargetName() = ["version_one_sink", "version_two_sink"]
select c.getLocation().getFile().getRelativePath(), c.getLocation().getStartLine(),
  c.getTargetName(), c.getStaticTarget().getLocation().getFile().getRelativePath(),
  count(DataFlow::Node a, DataFlow::Node b |
    a.asParameter().getEnclosingCfgScope() = c.getEnclosingCfgScope() and
    b.asExpr() = c.getPositionalArgument(0) and DataFlow::localFlow(a, b))
'''
CLOSURES = '''import rust
import codeql.rust.dataflow.DataFlow
from string layer
where layer = "panic-closure"
select layer,
  count(ClosureExpr c | c.getLocation().getFile().getRelativePath().matches("%/panic_shape.rs")),
  count(DataFlow::Node n | exists(ClosureExpr c |
    c.getLocation().getFile().getRelativePath().matches("%/panic_shape.rs") and
    c = n.asExpr().getParentNode*()))
'''


def write(path, text):
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("x") as stream:
        stream.write(text)


def fixture(root, wrappers):
    write(root / "Cargo.toml", '''[package]
name = "model-root"
version = "0.1.0"
edition = "2021"
[workspace]
exclude = ["candidate", "legacy", "deps/v1", "deps/v2"]
[dependencies]
model-dep = { path = "deps/v1" }
''')
    write(root / "src/lib.rs", '''pub fn direct_positive(value: i32) -> i32 { model_dep::route!(value) }
#[cfg(test)] mod panic_shape;
''')
    write(root / "src/panic_shape.rs", '''#[test]
fn nested_assert_shape() {
    let original_digest = "digest";
    let remainder = format!("image-child-digest:{original_digest}");
    assert!(remainder.lines().any(|line| line == format!("image-child-digest:{original_digest}")), "{remainder}");
}
''')
    write(root / "orphan.rs", "pub fn absent_negative(value: i32) -> i32 { model_dep::route!(value) }\n")
    write(root / "peer.rs", PEER)
    for version, word in [("1", "one"), ("2", "two")]:
        write(root / f"deps/v{version}/Cargo.toml", f'''[package]
name = "model-dep"
version = "{version}.0.0"
edition = "2021"
[workspace]
''')
        write(root / f"deps/v{version}/src/lib.rs", f'''pub fn version_{word}_sink(value: i32) -> i32 {{ value }}
#[macro_export]
macro_rules! route {{ ($value:expr) => {{ $crate::version_{word}_sink($value) }}; }}
''')
    for child, version in [("candidate", "1"), ("legacy", "2")]:
        write(root / child / "Cargo.toml", f'''[package]
name = "model-{child}"
version = "0.1.0"
edition = "2021"
[workspace]
[[bin]]
name = "peer"
path = "../peer.rs"
[dependencies]
model-dep = {{ path = "../deps/v{version}" }}
''')
        if wrappers:
            write(root / child / "src/bin/codeql_coverage.rs", 'include!("../../../peer.rs");\n')


def hashes(root):
    return {str(p.relative_to(root)): hashlib.sha256(p.read_bytes()).hexdigest()
            for p in sorted(root.rglob("*")) if p.is_file() and p.name != "evidence-hashes.json"}


def output_bytes(root):
    return sum(p.stat().st_size for p in root.rglob("*") if p.is_file())


def check_tables(root):
    """Fixed controls: two dependency contexts, a real flow, and no orphan edge."""
    tables = {}
    for layout in ("baseline", "wrappers"):
        for query in ("Macros", "Calls", "Closures"):
            with (root / f"{layout}-{query}.csv").open() as stream:
                tables[layout, query] = list(csv.reader(stream))[1:]
    for layout in ("baseline", "wrappers"):
        positive = [r for r in tables[layout, "Calls"]
                    if r[0].endswith("src/lib.rs") and r[2] == "version_one_sink" and int(r[4]) > 0]
        macro_positive = [r for r in tables[layout, "Macros"] if r[0].endswith("src/lib.rs")
                          and int(r[3]) > 0 and int(r[4]) > 0]
        negative = [r for r in tables[layout, "Macros"] if r[0].endswith("orphan.rs")]
        if not positive or not macro_positive or not negative or any(int(r[3]) or int(r[4]) for r in negative):
            raise RuntimeError("direct-positive/orphan-negative model control failed")
        for line, macro in [("4", "format"), ("5", "assert"), ("5", "format")]:
            if not any(r[0].endswith("panic_shape.rs") and r[1] == line and r[2] == macro
                       for r in tables[layout, "Macros"]):
                raise RuntimeError("panic outer macro row missing; no mechanism conclusion")
        if not tables[layout, "Closures"] or int(tables[layout, "Closures"][0][1]) < 1:
            raise RuntimeError("panic closure AST witness missing")
    baseline = [r for r in tables["baseline", "Calls"] if r[0].endswith("peer.rs")]
    candidate = [r for r in tables["wrappers", "Calls"] if r[0].endswith("peer.rs")]
    if baseline:
        raise RuntimeError("baseline did not reproduce the shared-source missing-call mechanism")
    for target, dependency in [("version_one_sink", "deps/v1/src/lib.rs"),
                               ("version_two_sink", "deps/v2/src/lib.rs")]:
        if not any(r[2] == target and r[3].endswith(dependency) and int(r[4]) > 0 for r in candidate):
            raise RuntimeError("wrapper failed to preserve both actual dependency call/flow contexts")
    peer_macros = [r for r in tables["wrappers", "Macros"] if r[0].endswith("peer.rs")]
    if not any(int(r[3]) > 0 and int(r[4]) > 0 for r in peer_macros):
        raise RuntimeError("wrapper produced no original-peer expansion/dataflow witness")
    layers = {}
    for layout in ("baseline", "wrappers"):
        layers[layout] = {}
        for name, line, macro in [("initial_format", "4", "format"), ("outer_assert", "5", "assert"),
                                 ("nested_format", "5", "format"), ("nested_panic", "5", "panic")]:
            found = [r for r in tables[layout, "Macros"] if r[0].endswith("panic_shape.rs")
                     and r[1] == line and (r[2] == macro or (macro == "panic" and "panic" in r[2]))]
            layers[layout][name] = {"rows": len(found), "expansion_nodes": sum(int(r[3]) for r in found),
                                    "dataflow_nodes": sum(int(r[4]) for r in found),
                                    "observation": "macro-row-observed" if found else "no-macro-row-observed"}
        layers[layout]["closure"] = tables[layout, "Closures"]
    return {"shared_target_controls": "PASS", "production_coverage": "NOT_ESTABLISHED",
            "panic_layers": layers,
            "panic_disposition": "requires independent layer-by-layer model interpretation"}


def run(argv, cwd, output, end, timeout, artifact_root, env, file_limit=CAP):
    """Own one process group; a timeout/error cannot leave its children running."""
    if not isinstance(file_limit, int) or not 0 < file_limit <= 8 * 1024**3:
        raise ValueError("invalid owned child file-size limit")
    process = None
    error = None
    cleanup = []
    started = time.time()
    selector = selectors.DefaultSelector()
    try:
        remaining = min(timeout, end - time.monotonic())
        if remaining <= 0:
            raise TimeoutError("fixed diagnostic deadline exhausted before spawn")
        with output.open("xb") as stream:
            process = subprocess.Popen(
                argv, cwd=cwd, env=env, stdout=subprocess.PIPE,
                stderr=subprocess.STDOUT, start_new_session=True,
                preexec_fn=lambda: resource.setrlimit(resource.RLIMIT_FSIZE, (file_limit, file_limit)))
            os.set_blocking(process.stdout.fileno(), False)
            selector.register(process.stdout, selectors.EVENT_READ)
            phase_end = time.monotonic() + remaining
            while process.poll() is None or selector.get_map():
                if time.monotonic() >= phase_end:
                    raise TimeoutError("diagnostic phase deadline exhausted")
                if output_bytes(artifact_root) > CAP - 1024 * 1024:
                    raise RuntimeError("combined artifact budget exceeded")
                for key, _ in selector.select(0.1):
                    chunk = os.read(key.fd, 65536)
                    if not chunk:
                        selector.unregister(key.fileobj)
                        continue
                    if output_bytes(artifact_root) + len(chunk) > CAP - 1024 * 1024:
                        raise RuntimeError("combined log budget exceeded before write")
                    stream.write(chunk)
                    stream.flush()
            if process.returncode:
                raise RuntimeError(f"command exit {process.returncode}")
    except BaseException as exc:
        error = repr(exc)
    finally:
        try:
            selector.close()
        except Exception as exc:
            cleanup.append("selector close: " + repr(exc))
        if process is not None:
            for sig in (signal.SIGTERM, signal.SIGKILL):
                try:
                    os.killpg(process.pid, sig)
                except ProcessLookupError:
                    pass
                except OSError as exc:
                    cleanup.append(repr(exc))
                try:
                    process.wait(timeout=2)
                except subprocess.TimeoutExpired:
                    pass
                except Exception as exc:
                    cleanup.append(repr(exc))
            if process.stdout:
                try:
                    process.stdout.close()
                except Exception as exc:
                    cleanup.append("stdout close: " + repr(exc))
            gone_end = time.monotonic() + 2
            while time.monotonic() < gone_end:
                try:
                    os.killpg(process.pid, 0)
                except ProcessLookupError:
                    break
                except OSError as exc:
                    cleanup.append("group readback: " + repr(exc))
                    break
                time.sleep(0.02)
            try:
                os.killpg(process.pid, 0)
                cleanup.append("owned process group remains")
            except ProcessLookupError:
                pass
            except OSError as exc:
                cleanup.append(repr(exc))
        result = {"argv": argv, "cwd": str(cwd), "started": started,
                  "elapsed": time.time() - started, "pid": process.pid if process else None,
                  "exit": process.returncode if process else None, "error": error,
                  "cleanup_errors": cleanup, "file_size_limit_bytes": file_limit}
        try:
            output.with_suffix(output.suffix + ".result.json").write_text(json.dumps(result, indent=2) + "\n")
        except Exception as exc:
            cleanup.append("result save: " + repr(exc))
            print(json.dumps(result), file=sys.stderr)
    if error or cleanup:
        raise RuntimeError(json.dumps(result))


def bundle_search_root(cli):
    root = cli.parent / "qlpacks"
    packs = list(root.glob("codeql/rust-all/*/qlpack.yml"))
    if len(packs) != 1:
        raise RuntimeError("official Rust library pack identity ambiguous")
    text = packs[0].read_text()
    if not re.search(r"(?m)^name: codeql/rust-all$", text) or not re.search(r"(?m)^version: 0\.2\.23$", text):
        raise RuntimeError("official Rust library pack identity mismatch")
    return root.resolve(), {"path": str(packs[0].relative_to(cli.parent)),
                            "sha256": hashlib.sha256(packs[0].read_bytes()).hexdigest(),
                            "name": "codeql/rust-all", "version": "0.2.23"}


LIBRARY_VERSIONS = {
    "concepts": "0.0.33", "controlflow": "2.0.43", "dataflow": "2.1.15",
    "mad": "1.0.59", "namebinding": "0.0.8", "regex": "1.0.59",
    "rust-all": "0.2.23", "ssa": "2.0.35", "threat-models": "1.0.59",
    "tutorial": "1.0.59", "typeinference": "0.0.40", "typetracking": "2.0.43",
    "util": "2.0.46"}


def verify_library_path(value, bundle, query, name):
    expected_keys = {"libraryPath", "dbscheme", "compilationCache", "relativeName", "qlPackName"}
    if not isinstance(value, dict) or set(value) != expected_keys:
        raise RuntimeError("unknown actual library-path schema")
    roots = {str(bundle / "qlpacks/codeql" / key / version) for key, version in LIBRARY_VERSIONS.items()}
    paths = value["libraryPath"]
    if not isinstance(paths, list) or not all(isinstance(p, str) for p in paths) or len(paths) != len(set(paths)):
        raise RuntimeError("invalid actual library paths")
    if set(paths) != roots | {str(query)}:
        raise RuntimeError("actual library dependency closure differs from pinned bundle")
    if value["dbscheme"] != str(bundle / "qlpacks/codeql/rust-all/0.2.23/rust.dbscheme"):
        raise RuntimeError("actual dbscheme differs from pinned bundle")
    if value["qlPackName"] != "oasis7/rust-model-diagnostic" or value["relativeName"] != f"oasis7/rust-model-diagnostic/{name}.ql":
        raise RuntimeError("actual source query pack identity mismatch")
    if not isinstance(value["compilationCache"], list) or not all(isinstance(p, str) for p in value["compilationCache"]):
        raise RuntimeError("unknown compilation cache schema")


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--codeql", type=Path, required=True)
    parser.add_argument("--inspect-packs-only", action="store_true")
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument("--real-peer-projection", action="store_true")
    mode.add_argument("--production-macro-database", action="store_true")
    mode.add_argument("--inspect-evaluator-evidence", action="store_true")
    mode.add_argument("--compile-anchored-parent", action="store_true")
    mode.add_argument("--isolated-ast-stage", action="store_true")
    mode.add_argument("--inspect-isolated-location-plans", action="store_true")
    parser.add_argument("--evaluator-cost-probes", action="store_true")
    parser.add_argument("--evaluator-batch", action="store_true")
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--work", type=Path, required=True)
    parser.add_argument("--deadline", type=float, required=True)
    args = parser.parse_args()
    if args.evaluator_cost_probes and not args.production_macro_database:
        parser.error("cost probes require the production database mode")
    if args.evaluator_batch and (not args.production_macro_database or args.evaluator_cost_probes):
        parser.error("batch requires production database mode without cost probes")
    args.owned_output = False
    terminal = {"status": "failed"}
    try:
        diagnose(args)
        terminal["status"] = "completed"
    except BaseException as exc:
        terminal["error"] = repr(exc)[:4096]
        raise
    finally:
        if args.owned_output:
            (args.output / "terminal.json").write_text(json.dumps(terminal, indent=2) + "\n")
            (args.output / "evidence-hashes.json").write_text(json.dumps(hashes(args.output), indent=2) + "\n")


def diagnose(args):
    args.deadline = time.monotonic() + max(0, args.deadline - time.time())
    args.output.mkdir(exist_ok=False)
    args.owned_output = True
    args.work.mkdir(exist_ok=False)
    write(args.output / "identity.json", json.dumps({
        "source": os.environ.get("GITHUB_SHA"), "run": os.environ.get("GITHUB_RUN_ID"),
        "attempt": os.environ.get("GITHUB_RUN_ATTEMPT"), "platform": platform.platform(),
        "script_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
        "bundle_sha256": "acf2050189aeede5b9aa566f8fb382b02ac1ace72f534778f68b06fce5371cea"}, indent=2) + "\n")
    write(args.output / "expectations.json", json.dumps({
        "baseline": "direct_positive has version_one call+localflow; shared peer has no resolved calls; orphan has no expansion/dataflow",
        "wrappers": "unchanged direct/orphan controls; peer has separate version_one and version_two resolved targets+localflow and expansion nodes",
        "panic": "record every layer and dataframe witness; no automatic coverage PASS or warning waiver"}, indent=2) + "\n")
    env = dict(os.environ, CARGO_NET_OFFLINE="true", RUSTUP_TOOLCHAIN="1.96.0",
               CODEQL_EXTRACTOR_RUST_OPTION_CARGO_ALL_TARGETS="true")
    env.pop("GH_TOKEN", None)
    env.pop("GITHUB_TOKEN", None)
    cli = str(args.codeql.resolve())
    run([cli, "version", "--format=json"], args.work, args.output / "codeql-version.json",
        args.deadline, 20, args.output, env)
    version = json.loads((args.output / "codeql-version.json").read_text())
    if version["version"] != "2.27.2":
        raise RuntimeError("unexpected CodeQL version")
    if args.isolated_ast_stage or args.inspect_isolated_location_plans:
        import types
        modules = []
        for label, filename in (("production", "codeql-rust-production-macros.py"),
                                ("isolated", "codeql-rust-isolated-stage.py")):
            spec = importlib.util.spec_from_file_location(label, Path(__file__).with_name(filename))
            module = importlib.util.module_from_spec(spec)
            spec.loader.exec_module(module)
            modules.append(module)
        action = modules[1].inspect_location_plans if args.inspect_isolated_location_plans else modules[1].experiment
        action(types.SimpleNamespace(run=run, fixture=fixture, write=write, hashes=hashes,
            LIBRARY_VERSIONS=LIBRARY_VERSIONS, MACROS=MACROS, CALLS=CALLS, CLOSURES=CLOSURES), modules[0], args, env)
        return
    if args.inspect_evaluator_evidence:
        helper = Path(__file__).with_name("codeql-rust-production-macros.py")
        specification = importlib.util.spec_from_file_location("evaluator_inspection", helper)
        module = importlib.util.module_from_spec(specification)
        specification.loader.exec_module(module)
        module.inspect_saved_evaluator_evidence(run, args, env)
        return
    for toolchain in ("1.96.0", "1.98.1"):
        run(["rustup", "run", toolchain, "rustc", "-vV"], args.work,
            args.output / f"rustc-{toolchain}.log", args.deadline, 20, args.output, env)
    search_root, pack_identity = bundle_search_root(args.codeql.resolve())
    write(args.output / "rust-library-pack.json", json.dumps(pack_identity, indent=2) + "\n")
    query = args.output / "queries"
    write(query / "qlpack.yml", "name: oasis7/rust-model-diagnostic\nversion: 0.0.0\ndependencies:\n  codeql/rust-all: 0.2.23\n")
    pack_files = sorted(search_root.glob("*/*/*/qlpack.yml"))
    inventory = []
    for manifest in pack_files:
        text = manifest.read_text()
        names = re.findall(r"(?m)^name: (.+)$", text)
        versions = re.findall(r"(?m)^version: (.+)$", text)
        if len(names) != 1 or len(versions) != 1:
            raise RuntimeError("ambiguous bundle pack manifest")
        lock = manifest.parent / "codeql-pack.lock.yml"
        inventory.append({"name": names[0], "version": versions[0],
                          "root": str(manifest.parent.resolve()), "manifest": text,
                          "sha256": hashlib.sha256(manifest.read_bytes()).hexdigest(),
                          "lock": lock.read_text() if lock.exists() else None,
                          "lock_sha256": hashlib.sha256(lock.read_bytes()).hexdigest() if lock.exists() else None})
    if not inventory or len({p["name"] for p in inventory}) != len(inventory):
        raise RuntimeError("empty or duplicate bundle pack identities")
    bundle_manifests = {}
    for path in (args.codeql.parent / ".codeqlmanifest.json", search_root / ".codeqlmanifest.json"):
        bundle_manifests[str(path)] = path.read_text() if path.exists() else None
    write(args.output / "bundle-pack-inputs.json", json.dumps({
        "packs": inventory, "bundle_manifests": bundle_manifests,
        "source_pack": "new owned source pack without copied compiled lock",
        "ram_requested_mib": 3072, "hard_rss_cap": False}, indent=2) + "\n")
    search_root = os.pathsep.join(p["root"] for p in inventory)
    peer_mode = None
    query_texts = {"Macros": MACROS, "Calls": CALLS, "Closures": CLOSURES}
    if args.real_peer_projection or args.production_macro_database or args.compile_anchored_parent:
        helper = Path(__file__).with_name("codeql-rust-production-macros.py" if args.production_macro_database or args.compile_anchored_parent else "codeql-rust-peer-inputs.py")
        specification = importlib.util.spec_from_file_location("real_peer_inputs", helper)
        peer_mode = importlib.util.module_from_spec(specification)
        specification.loader.exec_module(peer_mode)
        query_texts = (peer_mode.ANCHOR_COMPILE_QUERIES if args.compile_anchored_parent else
                       peer_mode.COST_QUERIES if args.evaluator_cost_probes else peer_mode.QUERIES)
        write(args.output / "real-peer-helper-sha256.txt", hashlib.sha256(helper.read_bytes()).hexdigest() + "\n")
    names = tuple(query_texts)
    for name, text in query_texts.items():
        write(query / (name + ".ql"), text)
    run([cli, "resolve", "packs", "--help"], args.work,
        args.output / "resolve-packs-help.log", args.deadline, 20, args.output, env)
    run([cli, "resolve", "packs", "--search-path=" + search_root], args.work,
        args.output / "resolve-packs.log", args.deadline, 20, args.output, env)
    run([cli, "resolve", "qlpacks", "--format=json", "--additional-packs=" + search_root], args.work,
        args.output / "resolve-qlpacks.json", args.deadline, 20, args.output, env)
    run([cli, "resolve", "library-path", "--help"], args.work,
        args.output / "resolve-library-help.log", args.deadline, 20, args.output, env)
    for name in names:
        run([cli, "resolve", "library-path", "--query=" + str(query / (name + ".ql")),
             "--format=json", "--additional-packs=" + search_root], args.work,
            args.output / f"library-path-{name}.json", args.deadline, 20, args.output, env)
    if args.inspect_packs_only:
        write(args.output / "inspection-result.json", json.dumps({
            "disposition": "INSPECTION_REQUIRES_REVIEW", "databases": 0,
            "model_queries": 0, "query_preflight": "NOT_RUN",
            "resolver_json_schema": "requires independent exact-shape review",
            "production_coverage": "NOT_ESTABLISHED"}, indent=2) + "\n")
        return
    versions = {p["name"]: p["version"] for p in inventory}
    for name, version in LIBRARY_VERSIONS.items():
        if versions.get("codeql/" + name) != version:
            raise RuntimeError("pinned dependency manifest mismatch: " + name)
    for name in names:
        verify_library_path(json.loads((args.output / f"library-path-{name}.json").read_text()),
                            args.codeql.resolve().parent, query.resolve(), name)
    run([cli, "query", "compile", "--help", "-v"], args.work,
        args.output / "query-compile-help.log", args.deadline, 20, args.output, env)
    help_text = (args.output / "query-compile-help.log").read_text()
    for option in ("--check-only", "--keep-going", "--additional-packs", "--compilation-cache"):
        if option not in help_text and option.replace("--", "--[no-]", 1) not in help_text:
            raise RuntimeError("exact CLI missing preflight option: " + option)
    run([cli, "query", "compile", "--check-only", "--keep-going",
         "--additional-packs=" + str(search_root), "--threads=2", "--ram=3072",
         *[str(query / (name + ".ql")) for name in names]],
        args.work, args.output / "query-preflight.log", args.deadline, 60, args.output, env)
    run([cli, "query", "run", "--help", "-v"], args.work,
        args.output / "query-run-help.log", args.deadline, 20, args.output, env)
    if "--compilation-cache" not in (args.output / "query-run-help.log").read_text():
        raise RuntimeError("exact query run missing compilation cache option")
    cache = args.work / "query-cache"
    cache.mkdir(exist_ok=False)
    write(args.output / "compiled-query-inputs.json", json.dumps({
        "cache": str(cache), "shared_compile_seconds": 360,
        "queries": {name: hashlib.sha256((query / (name + ".ql")).read_bytes()).hexdigest()
                    for name in names}}, indent=2) + "\n")
    if args.compile_anchored_parent:
        if "--[no-]dump-ra" not in help_text:
            raise RuntimeError("exact CLI missing RA dump option")
        if args.deadline - time.monotonic() < 450:
            raise RuntimeError("insufficient original budget for shared360 plus90 reserve")
        run([cli, "query", "compile", "--keep-going", "--dump-ra",
             "--compilation-cache=" + str(cache), "--no-default-compilation-cache",
             "--common-caches=" + str(args.work / "common-caches"),
             "--additional-packs=" + str(search_root), "--threads=2", "--ram=3072",
             *[str(query / (name + ".ql")) for name in names]], args.work,
            args.output / "anchored-parent-compile-ra.log", args.deadline - 90, 360, args.output, env)
        write(args.output / "anchored-parent-result.json", json.dumps({
            "disposition": "COMPILE_PLAN_REQUIRES_INDEPENDENT_REVIEW", "database_downloads": 0,
            "extractions": 0, "query_evaluations": 0, "coverage": "NOT_ESTABLISHED",
            "original_and_candidate": {name: hashlib.sha256((query / (name + ".ql")).read_bytes()).hexdigest()
                                       for name in names}}, indent=2)+"\n")
        return
    run([cli, "query", "compile", "--keep-going", "--compilation-cache=" + str(cache),
         "--additional-packs=" + str(search_root), "--threads=2", "--ram=3072",
         *[str(query / (name + ".ql")) for name in names]],
        args.work, args.output / "query-compile.log", args.deadline, 360, args.output, env)
    if peer_mode is not None:
        peer_mode.execute(run, args, env, query, search_root, cache)
        return
    before = {}
    for name, wrappers in [("baseline", False), ("wrappers", True)]:
        source = args.output / name
        fixture(source, wrappers)
        protected = ["peer.rs", "candidate/Cargo.toml", "legacy/Cargo.toml"]
        before[name] = {f: hashlib.sha256((source / f).read_bytes()).hexdigest() for f in protected}
        for child in (".", "candidate", "legacy"):
            manifest = source / child / "Cargo.toml"
            label = "root" if child == "." else child
            run(["cargo", "generate-lockfile", "--offline", "--manifest-path", str(manifest)],
                source, args.output / f"{name}-{label}-lock.log", args.deadline, 20, args.output, env)
            run(["cargo", "metadata", "--locked", "--offline", "--format-version=1", "--manifest-path", str(manifest)],
                source, args.output / f"{name}-{label}-metadata.json", args.deadline, 20, args.output, env)
            metadata = json.loads((args.output / f"{name}-{label}-metadata.json").read_text())
            if child != ".":
                wanted = "1.0.0" if child == "candidate" else "2.0.0"
                versions = [p["version"] for p in metadata["packages"] if p["name"] == "model-dep"]
                if versions != [wanted]:
                    raise RuntimeError("standalone dependency version changed")
        database = args.work / name
        run([cli, "database", "create", str(database), "--language=rust", "--build-mode=none",
             "--source-root=" + str(source), "--threads=2", "--ram=3072"], source,
            args.output / f"{name}-extract.log", args.deadline, 180, args.output, env)
        for q in ("Macros", "Calls", "Closures"):
            bqrs = args.output / f"{name}-{q}.bqrs"
            run([cli, "query", "run", str(query / (q + ".ql")), "--database=" + str(database),
                 "--output=" + str(bqrs), "--threads=2", "--ram=3072",
                 "--additional-packs=" + str(search_root), "--compilation-cache=" + str(cache)], source,
                args.output / f"{name}-{q}.log", args.deadline, 60, args.output, env)
            run([cli, "bqrs", "decode", str(bqrs), "--format=csv", "--output=" + str(args.output / f"{name}-{q}.csv")],
                source, args.output / f"{name}-{q}-decode.log", args.deadline, 20, args.output, env)
        after = {f: hashlib.sha256((source / f).read_bytes()).hexdigest() for f in protected}
        if after != before[name]:
            raise RuntimeError("original fixture source or manifest changed")
    if before["baseline"] != before["wrappers"]:
        raise RuntimeError("comparison does not preserve original source/manifests")
    if output_bytes(args.output) > CAP:
        raise RuntimeError("combined artifact budget exceeded")
    (args.output / "model-result.json").write_text(json.dumps(check_tables(args.output), indent=2) + "\n")


if __name__ == "__main__":
    main()
