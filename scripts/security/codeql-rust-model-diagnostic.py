#!/usr/bin/env python3
"""Manual, bounded CodeQL model experiment; never uploads Code Scanning results."""
import argparse
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
where mc.getLocation().getFile().getRelativePath().regexpMatch(".*(peer|lib|orphan|panic_shape)\\.rs")
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


def run(argv, cwd, output, end, timeout, artifact_root, env):
    """Own one process group; a timeout/error cannot leave its children running."""
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
                preexec_fn=lambda: resource.setrlimit(resource.RLIMIT_FSIZE, (CAP, CAP)))
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
                  "cleanup_errors": cleanup}
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


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--codeql", type=Path, required=True)
    parser.add_argument("--inspect-packs-only", action="store_true")
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--work", type=Path, required=True)
    parser.add_argument("--deadline", type=float, required=True)
    args = parser.parse_args()
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
    cli = str(args.codeql.resolve())
    run([cli, "version", "--format=json"], args.work, args.output / "codeql-version.json",
        args.deadline, 20, args.output, env)
    version = json.loads((args.output / "codeql-version.json").read_text())
    if version["version"] != "2.27.2":
        raise RuntimeError("unexpected CodeQL version")
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
    write(query / "Macros.ql", MACROS)
    write(query / "Calls.ql", CALLS)
    write(query / "Closures.ql", CLOSURES)
    run([cli, "resolve", "packs", "--help"], args.work,
        args.output / "resolve-packs-help.log", args.deadline, 20, args.output, env)
    run([cli, "resolve", "packs", "--search-path=" + search_root], args.work,
        args.output / "resolve-packs.log", args.deadline, 20, args.output, env)
    run([cli, "resolve", "qlpacks", "--format=json", "--additional-packs=" + search_root], args.work,
        args.output / "resolve-qlpacks.json", args.deadline, 20, args.output, env)
    run([cli, "resolve", "library-path", "--help"], args.work,
        args.output / "resolve-library-help.log", args.deadline, 20, args.output, env)
    for name in ("Macros", "Calls", "Closures"):
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
    run([cli, "query", "compile", "--help"], args.work,
        args.output / "query-compile-help.log", args.deadline, 20, args.output, env)
    help_text = (args.output / "query-compile-help.log").read_text()
    for option in ("--check-only", "--keep-going", "--additional-packs"):
        if option not in help_text and option.replace("--", "--[no-]", 1) not in help_text:
            raise RuntimeError("exact CLI missing preflight option: " + option)
    run([cli, "query", "compile", "--check-only", "--keep-going",
         "--additional-packs=" + str(search_root), "--threads=2", "--ram=3072",
         *[str(query / (name + ".ql")) for name in ("Macros", "Calls", "Closures")]],
        args.work, args.output / "query-preflight.log", args.deadline, 60, args.output, env)
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
                 "--additional-packs=" + str(search_root)], source,
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
