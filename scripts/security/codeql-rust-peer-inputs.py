"""Scanner-owned projections of the unchanged real compatibility peer."""
import copy
import hashlib
import json
from pathlib import Path
import shutil
import subprocess

PEER = "tests/libp2p-compat/peer.rs"
VENDOR = "vendor-libp2p-swarm-0.48.0"
ORIGINALS = [PEER] + [f"tests/libp2p-compat/{kind}/{name}" for kind in ("candidate", "legacy") for name in ("Cargo.toml", "Cargo.lock")]


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def snapshot(repository, root):
    root.mkdir(exist_ok=False)
    tracked = subprocess.check_output(["git", "ls-files", "-z", "--", *ORIGINALS, VENDOR], cwd=repository).decode().split("\0")
    for relative in filter(None, tracked):
        source = repository / relative
        if source.is_symlink() or not source.is_file():
            raise RuntimeError("non-regular tracked peer input: " + relative)
        destination = root / relative
        destination.parent.mkdir(parents=True, exist_ok=True)
        used = sum(p.stat().st_size for p in root.parent.rglob("*") if p.is_file())
        if used + source.stat().st_size > 63 * 1024 * 1024:
            raise RuntimeError("combined evidence budget before source copy")
        shutil.copyfile(source, destination)
    if any(not (root / relative).is_file() for relative in ORIGINALS):
        raise RuntimeError("real peer input missing")
    lines = (root / PEER).read_text().splitlines()
    if not lines[9].startswith("struct Behaviour") or "with_existing_identity" not in lines[46] or "async fn run(args" not in lines[43] or 'let server = args[1]' not in lines[44]:
        raise RuntimeError("frozen original peer witness locations changed")
    (root / "src").mkdir()
    (root / "Cargo.toml").write_text('[package]\nname="codeql-peer-control"\nversion="0.0.0"\nedition="2024"\n[workspace]\nexclude=["tests/libp2p-compat/candidate","tests/libp2p-compat/legacy","vendor-libp2p-swarm-0.48.0"]\n')
    (root / "src/lib.rs").write_text('macro_rules! oracle { ($value:expr) => { oracle_sink($value) }; }\nfn oracle_sink(value:i32)->i32 {value}\npub fn direct_positive(value:i32)->i32 { oracle!(value) }\n')
    (root / "orphan.rs").write_text('pub fn negative(value:i32)->i32 { oracle!(value) }\n')
    return {relative: sha(root / relative) for relative in ORIGINALS}


def project(root, kind):
    if kind not in ("candidate", "legacy"):
        raise RuntimeError("unknown projection context")
    original = root / f"tests/libp2p-compat/{kind}/Cargo.toml"
    text = original.read_text()
    if text.count('path = "../peer.rs"') != 1:
        raise RuntimeError("unexpected original target")
    text = text.replace('path = "../peer.rs"', 'path = "peer.rs"')
    if kind == "candidate":
        old = 'path = "../../../vendor-libp2p-swarm-0.48.0"'
        if text.count(old) != 1:
            raise RuntimeError("unexpected original swarm patch")
        text = text.replace(old, 'path = "../../vendor-libp2p-swarm-0.48.0"')
    manifest = root / "tests/libp2p-compat/Cargo.toml"
    with manifest.open("x") as stream:
        stream.write(text)
    lock = root / "tests/libp2p-compat/Cargo.lock"
    with lock.open("xb") as stream:
        stream.write((original.parent / "Cargo.lock").read_bytes())
    return manifest, lock


def normalized_metadata(metadata, original_peer=None):
    """Alias only the root identity and the four explicitly relocated fields."""
    value = copy.deepcopy(metadata)
    root_id = value["resolve"]["root"] if value.get("resolve") else value["workspace_members"][0]
    root_packages = [package for package in value["packages"] if package["id"] == root_id]
    if len(root_packages) != 1:
        raise RuntimeError("metadata root package ambiguous")
    root_packages[0]["manifest_path"] = "<ROOT_MANIFEST>"
    if original_peer is not None:
        targets = root_packages[0]["targets"]
        peers = [target for target in targets if target["kind"] == ["bin"] and target["name"] == "peer"]
        if len(peers) != 1 or not Path(peers[0]["src_path"]).samefile(original_peer):
            raise RuntimeError("root peer target is not the same original source file")
        peers[0]["src_path"] = str(original_peer.resolve())
    if "build_directory" in value:
        if value["build_directory"] != value["target_directory"]:
            raise RuntimeError("unexpected independent Cargo build directory")
        value["build_directory"] = "<ROOT_TARGET_DIRECTORY>"
    value["workspace_root"] = "<ROOT_WORKSPACE>"
    value["target_directory"] = "<ROOT_TARGET_DIRECTORY>"
    def identity(item):
        if isinstance(item, str):
            return "<ROOT_PACKAGE_ID>" if item == root_id else item
        if isinstance(item, list):
            return [identity(x) for x in item]
        if isinstance(item, dict):
            return {key: identity(x) for key, x in item.items()}
        return item
    value = identity(value)
    value["packages"].sort(key=lambda package: package["id"])
    if value.get("resolve"):
        value["resolve"]["nodes"].sort(key=lambda node: node["id"])
    return value


def verify_metadata(original, projected, root, kind, complete):
    if complete:
        for metadata in (original, projected):
            resolve = metadata.get("resolve")
            if not isinstance(resolve, dict) or not resolve.get("root") or not isinstance(resolve.get("nodes"), list) or not any(node.get("id") == resolve["root"] for node in resolve["nodes"]):
                raise RuntimeError("complete metadata requires a real resolved root and graph")
    if normalized_metadata(original, root / PEER) != normalized_metadata(projected, root / PEER):
        raise RuntimeError("projected Cargo metadata changed beyond root identity/location")
    packages = projected["packages"]
    root_id = projected["resolve"]["root"] if projected.get("resolve") else projected["workspace_members"][0]
    package = next(package for package in packages if package["id"] == root_id)
    binaries = [target for target in package["targets"] if target["kind"] == ["bin"]]
    if len(binaries) != 1 or binaries[0]["name"] != "peer" or Path(binaries[0]["src_path"]) != root / PEER:
        raise RuntimeError("projected original peer target identity changed")
    dependency = next(d for d in package["dependencies"] if d["name"] == "libp2p")
    if kind == "candidate":
        if not dependency["source"].startswith("git+https://github.com/libp2p/rust-libp2p?rev=2d8497b2615086bd88018ef3a1fb32bc613a07ca"):
            raise RuntimeError("candidate Git identity changed")
    elif dependency["req"] != "=0.56.0" or not dependency["source"].startswith("registry+"):
        raise RuntimeError("legacy registry identity changed")
    if complete:
        libp2p = [p for p in packages if p["name"] == "libp2p"]
        if len(libp2p) != 1:
            raise RuntimeError("libp2p resolved package ambiguous")
        return {"source": libp2p[0]["source"], "version": libp2p[0]["version"], "root": str(Path(libp2p[0]["manifest_path"]).parent)}
    return {"closure": "NOT_CHECKED_NO_DEPS"}

QUERIES = {
"Macros": '''import rust
import codeql.rust.dataflow.DataFlow
from MacroCall mc
where mc.getLocation().getFile().getRelativePath() = ["tests/libp2p-compat/peer.rs", "src/lib.rs", "orphan.rs"]
select mc.getLocation().getFile().getRelativePath(), mc.getLocation().getStartLine(), mc.getPath().toString(),
 count(AstNode n | mc.getMacroCallExpansion() = n.getParentNode*()),
 count(DataFlow::Node n | mc.getMacroCallExpansion() = n.asExpr().getParentNode*())
''',
"Calls": '''import rust
from Call c
where c.getLocation().getFile().getRelativePath() = ["tests/libp2p-compat/peer.rs", "src/lib.rs"] and
 (c.getTargetName() = "oracle_sink" or
  c.getTargetName() = "with_existing_identity" and c.getLocation().getStartLine() = 47)
select c.getLocation().getFile().getRelativePath(), c.getLocation().getStartLine(), c.getTargetName(),
 c.getStaticTarget().getLocation().getFile().getAbsolutePath()
''',
"Derives": '''import rust
import codeql.rust.dataflow.DataFlow
from TypeItem item
where item instanceof Struct and item.getLocation().getFile().getRelativePath() = "tests/libp2p-compat/peer.rs" and
 item.getName().toString() = "Behaviour" and item.getName().getLocation().getStartLine() = 10
select item.getLocation().getFile().getRelativePath(), item.getName().getLocation().getStartLine(),
 item.getNumberOfDeriveMacroExpansions(),
 count(AstNode n | item.getADeriveMacroExpansion() = n.getParentNode*()),
 count(DataFlow::Node n | item.getADeriveMacroExpansion() = n.asExpr().getParentNode*())
''',
"Flow": '''import rust
import codeql.rust.dataflow.DataFlow
from string witness
where witness = "run-args-to-line45-path"
select witness, count(DataFlow::Node source, DataFlow::Node sink |
 source.asParameter().getLocation().getFile().getRelativePath() = "tests/libp2p-compat/peer.rs" and
 source.asParameter().getLocation().getStartLine() = 44 and
 sink.asExpr() instanceof PathExpr and
 sink.asExpr().getLocation().getFile().getRelativePath() = "tests/libp2p-compat/peer.rs" and
 sink.asExpr().getLocation().getStartLine() = 45 and DataFlow::localFlow(source, sink))
'''}


def metadata_read(run, argv, source, output, args, env, seconds):
    import shlex
    command = "set -C; exec " + shlex.join(argv) + " > " + shlex.quote(str(output))
    run(["/bin/sh", "-c", command], source, output.with_suffix(".command.log"),
        args.deadline, seconds, args.output, env)
    return json.loads(output.read_text())


def execute(run, args, env, query, packs, cache):
    """One baseline and two distinct real target contexts; never mix DB entities."""
    import csv
    import os
    repository = Path(__file__).resolve().parents[2]
    network_env = dict(env, CARGO_NET_OFFLINE="false", CARGO_HOME=str(args.work / "cargo-home"))
    network_env.pop("RUSTC_WRAPPER", None)
    Path(network_env["CARGO_HOME"]).mkdir(exist_ok=False)
    extraction_env = dict(network_env, CARGO_NET_OFFLINE="true")
    evidence = {}
    for kind in ("baseline", "candidate", "legacy"):
        source = args.output / ("real-" + kind)
        before = snapshot(repository, source)
        vendor_before = {str(p.relative_to(source)): sha(p) for p in (source / VENDOR).rglob("*") if p.is_file()}
        (args.output / f"{kind}-protected-before.json").write_text(json.dumps({"original": before, "vendor": vendor_before}, indent=2) + "\n")
        projected_lock = None
        try:
            if kind != "baseline":
                original_manifest = source / f"tests/libp2p-compat/{kind}/Cargo.toml"
                original_metadata = {}
                for label, no_deps in (("direct", True), ("full", False)):
                    output = args.output / f"{kind}-original-{label}.json"
                    argv = ["cargo", "metadata", "--locked", "--format-version=1", "--manifest-path", str(original_manifest)]
                    if no_deps:
                        argv.append("--no-deps")
                    original_metadata[label] = metadata_read(run, argv, source, output, args, network_env, 90)
                projected_manifest, projected_lock = project(source, kind)
                for label, no_deps in (("direct", True), ("full", False)):
                    output = args.output / f"{kind}-projected-{label}.json"
                    argv = ["cargo", "metadata", "--locked", "--offline", "--format-version=1", "--manifest-path", str(projected_manifest)]
                    if no_deps:
                        argv.append("--no-deps")
                    projected_metadata = metadata_read(run, argv, source, output, args, extraction_env, 30)
                    context = verify_metadata(original_metadata[label], projected_metadata, source, kind, not no_deps)
                evidence[kind] = {"metadata_equivalence": "PASS", "libp2p": context}
            else:
                # Materialize both exact dependency contexts before offline extraction.
                for target in ("candidate", "legacy"):
                    metadata_read(run, ["cargo", "metadata", "--locked", "--format-version=1", "--manifest-path",
                         str(source / f"tests/libp2p-compat/{target}/Cargo.toml")], source,
                        args.output / f"baseline-{target}-metadata.json", args, network_env, 90)
            (args.output / f"{kind}-metadata-equivalence.json").write_text(json.dumps(evidence.get(kind, {"closure": "baseline observation"}), indent=2) + "\n")
            database = args.work / ("real-" + kind)
            run([str(args.codeql), "database", "create", str(database), "--language=rust", "--build-mode=none",
                 "--source-root=" + str(source), "--threads=2", "--ram=3072"], source,
                args.output / f"{kind}-extract.log", args.deadline, 180, args.output, extraction_env)
            for name in QUERIES:
                bqrs = args.output / f"{kind}-{name}.bqrs"
                run([str(args.codeql), "query", "run", str(query / (name + ".ql")), "--database=" + str(database),
                     "--output=" + str(bqrs), "--additional-packs=" + str(packs),
                     "--compilation-cache=" + str(cache), "--threads=2", "--ram=3072"], source,
                    args.output / f"{kind}-{name}.log", args.deadline, 120, args.output, extraction_env)
                run([str(args.codeql), "bqrs", "decode", str(bqrs), "--format=csv",
                     "--output=" + str(args.output / f"{kind}-{name}.csv")], source,
                    args.output / f"{kind}-{name}-decode.log", args.deadline, 20, args.output, extraction_env)
        finally:
            after = {relative: sha(source / relative) if (source / relative).is_file() else None for relative in ORIGINALS}
            vendor_after = {relative: sha(source / relative) if (source / relative).is_file() else None for relative in vendor_before}
            lock_hash = sha(projected_lock) if projected_lock is not None and projected_lock.is_file() else None
            (args.output / f"{kind}-protected-after.json").write_text(json.dumps({"original": after, "vendor": vendor_after, "projected_lock": lock_hash}, indent=2) + "\n")
            if before != after or vendor_before != vendor_after:
                raise RuntimeError("protected original/vendor source changed")
            if projected_lock is not None and lock_hash != before[f"tests/libp2p-compat/{kind}/Cargo.lock"]:
                raise RuntimeError("projected original lock changed")
            evidence.setdefault(kind, {})["source_hashes"] = before
    (args.output / "real-peer-contexts.json").write_text(json.dumps(evidence, indent=2) + "\n")
    check_results(args.output, evidence)


def check_results(output, contexts):
    import csv
    def rows(kind, query):
        with (output / f"{kind}-{query}.csv").open() as stream:
            return list(csv.reader(stream))[1:]
    for kind in ("baseline", "candidate", "legacy"):
        macros = rows(kind, "Macros")
        positive = [r for r in macros if r[0] == "src/lib.rs" and int(r[3]) > 0 and int(r[4]) > 0]
        negative = [r for r in macros if r[0] == "orphan.rs"]
        if not positive or not negative or any(int(r[3]) or int(r[4]) for r in negative):
            raise RuntimeError("real-peer direct/negative controls failed: " + kind)
        peer = [r for r in macros if r[0] == PEER]
        calls = [r for r in rows(kind, "Calls") if r[0] == PEER and r[1] == "47" and r[2] == "with_existing_identity"]
        derives = rows(kind, "Derives")
        flow = rows(kind, "Flow")
        if kind == "baseline":
            if any(int(r[3]) or int(r[4]) for r in peer) or calls:
                raise RuntimeError("original shared-peer missing-semantic mechanism not reproduced")
            continue
        expected = Path(contexts[kind]["libp2p"]["root"])
        if not calls or not any(Path(r[3]).is_relative_to(expected) for r in calls):
            raise RuntimeError("real libp2p target provenance not established: " + kind)
        if not any(int(r[3]) > 0 and int(r[4]) > 0 for r in peer):
            raise RuntimeError("original peer macro expansion/dataflow missing: " + kind)
        if not any(r[0] == PEER and r[1] == "10" and int(r[2]) > 0 and int(r[3]) > 0 and int(r[4]) > 0 for r in derives):
            raise RuntimeError("original Behaviour derive model missing: " + kind)
        if len(flow) != 1 or flow[0][0] != "run-args-to-line45-path" or int(flow[0][1]) <= 0:
            raise RuntimeError("exact original run parameter/path flow missing: " + kind)
    (output / "real-peer-result.json").write_text(json.dumps({"fixed_controls": "PASS", "production_baseline": "NOT_ACCEPTED", "contexts": contexts}, indent=2) + "\n")
