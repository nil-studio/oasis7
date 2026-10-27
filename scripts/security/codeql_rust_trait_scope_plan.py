"""Fixed compile-only library candidates; never creates/evaluates a DB."""
import hashlib
import json
import os
from pathlib import Path
import shutil
import time

import codeql_rust_diagnostic_runtime as runtime
import codeql_rust_successor_partition as partition

PATH = "codeql/rust/internal/PathResolution.qll"
PATH_SHA = "c83ebb64d9a10628ec37afaac3543ad0e2ac36f1297cffd4e85e29ea66f9fa8d"
STAGES = "codeql/rust/internal/CachedStages.qll"
STAGES_SHA = "e9ec32dedd08185b23787efb12879032dc7a7c0cc59db2b48dd49202d54b5754"
COMPILE_PROBES = ("trait-scope-plan", "successor-partition-plan")
OLD = b"""module TraitIsVisible<relevantTraitVisibleSig/2 relevantTraitVisible> {
  private newtype TNode =
    TTrait(Trait t) { relevantTraitVisible(_, t) } or
    TItemNode(ItemNode i) or
    TElement(Element e) { relevantTraitVisible(e, _) }
"""
NEW = b"""module TraitIsVisible<relevantTraitVisibleSig/2 relevantTraitVisible> {
  // Every accepting path ends at a relevant element's owner; include all
  // outer scopes on that path, including module-root jumps and cycles.
  private predicate relevantItemNode(ItemNode item) {
    exists(Element e | relevantTraitVisible(e, _) and item.getADescendant() = e)
    or
    exists(ItemNode inner | relevantItemNode(inner) and item = getOuterScope(inner))
  }

  private newtype TNode =
    TTrait(Trait t) { relevantTraitVisible(_, t) } or
    TItemNode(ItemNode i) { relevantItemNode(i) } or
    TElement(Element e) { relevantTraitVisible(e, _) }
"""
QUERIES = {
    "Flow": "import rust\nimport codeql.rust.dataflow.DataFlow\nfrom DataFlow::Node a, DataFlow::Node b\nwhere DataFlow::localFlow(a, b)\nselect a, b\n",
    "Callee": "import rust\nfrom Call c\nselect c, c.getStaticTarget()\n",
}
PACK = "oasis7/rust-trait-scope-plan"


def patched_source(data):
    if hashlib.sha256(data).hexdigest() != PATH_SHA or data.count(OLD) != 1:
        raise RuntimeError("fixed official PathResolution bytes differ")
    result = data.replace(OLD, NEW, 1)
    if result.count(NEW) != 1 or result.replace(NEW, OLD, 1) != data:
        raise RuntimeError("scope patch is not exactly reversible")
    return result


def successor_sources(data, stages):
    candidate, stage_candidate = partition.partition_sources(data, stages, PATH_SHA, STAGES_SHA)
    return {PATH: candidate, STAGES: stage_candidate}


def check_time(end):
    if time.monotonic() >= end:
        raise RuntimeError("fixed compile-plan deadline exhausted")


def inventory(closure, end):
    result = {}
    for name, pack in closure.items():
        root = Path(pack["root"])
        for path in sorted(root.rglob("*")):
            check_time(end)
            if path.is_symlink() or not (path.is_file() or path.is_dir()):
                raise RuntimeError("nonregular official library input")
            if path.is_file():
                if len(result) >= 20000 or path.stat().st_size > 64 * 1024**2:
                    raise RuntimeError("official inventory bound exceeded")
                result[name + "/" + str(path.relative_to(root))] = runtime.digest(path)
    return result


def omitted_cache(path):
    return path.suffix in (".qlx", ".bqrs") or any(part in (
        ".cache", ".codeql", "compilation-cache", "query-cache", "common-cache"
    ) for part in path.parts[:-1])


def execute(args, env, closure, verify_library):
    if args.bundle_version != "2.27.3":
        raise RuntimeError("library plan requires complete official 2.27.3")
    probe = getattr(args, "probe", "trait-scope-plan")
    if probe not in COMPILE_PROBES:
        raise RuntimeError("unknown compile-only candidate")
    out, work = args.output, args.work
    official = Path(closure["codeql/rust-all"]["root"])
    before = inventory(closure, args.deadline - 90)
    runtime.save(out / "terminal-size-admission.json", runtime.admit_terminal_snapshots({}, before), out)
    runtime.save(out / "official-before.json", before, out)
    primary = None
    try:
        if probe == "successor-partition-plan":
            boundary = partition.check_extension_boundary(closure, args.deadline - 90)
            runtime.save(out / "extension-boundary.json", boundary, out)
        sources = (successor_sources((official / PATH).read_bytes(), (official / STAGES).read_bytes())
            if probe == "successor-partition-plan" else {PATH: patched_source((official / PATH).read_bytes())})
        copy = work / "isolated-rust-all"
        copy.mkdir(exist_ok=False)
        copied, omitted = {}, []
        for path in sorted(official.rglob("*")):
            check_time(args.deadline - 90)
            relative = path.relative_to(official)
            if path.is_dir():
                continue
            if omitted_cache(relative):
                omitted.append(str(relative))
                continue
            target = copy / relative
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(path, target)
            copied[str(relative)] = runtime.digest(target)
        for path, patched in sources.items():
            (copy / path).write_bytes(patched)
            copied[path] = hashlib.sha256(patched).hexdigest()
        expected = {name.removeprefix("codeql/rust-all/"): value for name, value in before.items()
            if name.startswith("codeql/rust-all/") and not omitted_cache(Path(name.removeprefix("codeql/rust-all/")))}
        expected.update({path: copied[path] for path in sources})
        actual = {str(path.relative_to(copy)): runtime.digest(path) for path in copy.rglob("*") if path.is_file()}
        if actual != expected or actual != copied:
            raise RuntimeError("isolated copy contains unexpected differences")
        runtime.save(out / "isolated-source.json", {"probe": probe, "original_sha256": PATH_SHA,
            "candidate_sha256": copied[PATH], "changed_path": PATH,
            "changed_files": {path: {"original_sha256": before["codeql/rust-all/" + path],
                "candidate_sha256": copied[path]} for path in sources},
            "copied_regular_files": actual, "omitted_precompiled_or_cache": omitted}, out)
        for path, patched in sources.items():
            runtime.evidence_write(out / ("candidate-" + Path(path).name), patched, out)
        runtime.save(out / "query-inputs.json", {name: hashlib.sha256(text.encode()).hexdigest()
            for name, text in QUERIES.items()}, out)
        runtime.run([str(args.codeql), "query", "compile", "--help", "-v"], work,
            out / "compile-help.log", args.deadline - 90, 20, out, env)
        help_text = (out / "compile-help.log").read_text()
        if "--dump-ra" not in help_text and "--[no-]dump-ra" not in help_text:
            raise RuntimeError("actual CLI does not advertise dump-ra")
        if args.deadline - time.monotonic() < 450:
            raise RuntimeError("NOT_REACHED insufficient shared compile budget")
        compile_end = min(args.deadline - 90, time.monotonic() + 360)
        for label, root in (("original", official), ("candidate", copy)):
            selected = {name: dict(pack) for name, pack in closure.items()}
            selected["codeql/rust-all"]["root"] = str(root)
            query = out / (label + "-query")
            query.mkdir(exist_ok=False)
            runtime.evidence_write(query / "qlpack.yml", ("name: " + PACK + "\nversion: 0.0.0\ndependencies:\n  codeql/rust-all: 0.2.24\n").encode(), out)
            packs = os.pathsep.join(pack["root"] for pack in selected.values())
            for name, text in QUERIES.items():
                runtime.evidence_write(query / (name + ".ql"), text.encode(), out)
                resolved = out / (label + "-" + name + "-library.json")
                runtime.run([str(args.codeql), "resolve", "library-path", "--query=" + str(query / (name + ".ql")),
                    "--format=json", "--additional-packs=" + packs], work, resolved, compile_end, 20, out, env)
                verify_library(json.loads(resolved.read_text()), query, name, selected, pack_name=PACK)
                cache, common = work / (label + name + "-compile"), work / (label + name + "-common")
                cache.mkdir(exist_ok=False)
                common.mkdir(exist_ok=False)
                runtime.run([str(args.codeql), "query", "compile", "--dump-ra", "--additional-packs=" + packs,
                    "--compilation-cache=" + str(cache), "--no-default-compilation-cache", "--common-caches=" + str(common),
                    "--threads=2", "--ram=3072", str(query / (name + ".ql"))], work,
                    out / (label + "-" + name + "-ra.log"), compile_end, 360, out, env)
        runtime.save(out / "compile-plan-terminal.json", {"status": "COMPILED_REQUIRES_INDEPENDENT_PLAN_REVIEW",
            "database_operations": 0, "extractions": 0, "query_evaluations": 0,
            "business_source_consumed": False, "runtime_go": False, "full_rust_coverage": "HOLD"}, out)
    except BaseException as exc:
        primary = exc
        raise
    finally:
        try:
            after = inventory(closure, args.deadline)
            runtime.save(out / "official-after.json", after, out, terminal=True)
            if after != before:
                raise RuntimeError("official library contents changed")
        except BaseException as exc:
            runtime.save(out / "official-after-error.json", {"status": "UNKNOWN_OR_CHANGED", "error": repr(exc)[:2048]}, out, terminal=True)
            if primary is None:
                raise
