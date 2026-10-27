"""Fixed-source successor result partition; no database or evaluator operations."""
import hashlib
from pathlib import Path
import re
import time

PARAMS = b"(string name, SuccessorKind kind, UseOption useOpt)"
METHOD_START = b"  cached\n  ItemNode getASuccessor" + PARAMS + b" {"
METHOD_END = b"  /** Gets an _external_ successor named `name`, if any. */"
DEPENDENCY_HEADER = b"pragma[nomagic]\nprivate predicate crateDependencyEdge("
INLINE_DEPENDENCY = b"pragma[inline]\nprivate predicate crateDependencyEdge("
OLD_BACKREF = b"exists(any(ItemNode i).getASuccessor(_, _, _))"
NEW_BACKREF = b"exists(any(ItemNode i).getASuccessorNonCrate(_, _, _))"
NAMES = ("getASuccessorNonCrate", "getASuccessorCrate")


def once(data, old, new):
    if data.count(old) != 1:
        raise RuntimeError("partition patch anchor is not unique")
    return data.replace(old, new, 1)


def specialize(body, target):
    body = once(body, b"fileImportEdge(this, name, result, kind, useOpt)",
        b"exists(SourceFileItemNode f |\n      fileImport(this.(Module), f) and\n"
        b"      result = f." + target + b"(name, kind, useOpt)\n    )")
    body = once(body, b"crateDefEdge(this, name, result, kind, useOpt)",
        b"result = this.(CrateItemNode).getSourceFile()." + target +
        b"(name, kind, useOpt) and\n    kind.isExternalOrBoth()")
    # These three intermediates are explicitly UseItemNode or ExternCrateItemNode,
    # both disjoint from Crate in the fixed schema. Terminal results may be crates.
    body = body.replace(b".getASuccessor(_, _, _)", b".getASuccessorNonCrate(_, _, _)")
    body = body.replace(b".getASuccessor(name, kind, useOpt)", b"." + target + b"(name, kind, useOpt)")
    body = body.replace(b".getASuccessor(name, kind, _)", b"." + target + b"(name, kind, _)")
    if target == b"getASuccessorNonCrate":
        # The full branch remains in the crate partition; never remove fallback
        # based on missing metadata or an inferred owner.
        body = once(body, b"    or\n    crateDependencyEdge(this, name, result) and\n"
            b"    kind.isInternal() and\n    useOpt.isNone()\n", b"")
    return body


def partition_sources(data, stages, path_sha, stages_sha):
    if hashlib.sha256(data).hexdigest() != path_sha or hashlib.sha256(stages).hexdigest() != stages_sha:
        raise RuntimeError("fixed official partition/stage bytes differ")
    if any(name.encode() in data or name.encode() in stages for name in NAMES):
        raise RuntimeError("partition method name already exists")
    if data.count(METHOD_START) != 1:
        raise RuntimeError("partition method anchor differs")
    start = data.index(METHOD_START)
    end = data.index(METHOD_END, start)
    old = data[start:end]
    if not old.endswith(b"  }\n\n"):
        raise RuntimeError("partition method boundary differs")
    body = old.split(b"{\n", 1)[1][:-len(b"  }\n\n")]
    body = once(body, b"    Stages::PathResolutionStage::ref() and\n", b"")
    new = (b"  ItemNode getASuccessor" + PARAMS + b" {\n"
        b"    result = this.getASuccessorNonCrate(name, kind, useOpt)\n    or\n"
        b"    result = this.getASuccessorCrate(name, kind, useOpt)\n  }\n\n")
    new += (b"  cached\n  final ItemNode getASuccessorNonCrate" + PARAMS + b" {\n"
        b"    Stages::PathResolutionStage::ref() and\n    not result instanceof CrateItemNode and\n    (\n" +
        specialize(body, b"getASuccessorNonCrate") + b"    )\n  }\n\n")
    # Keep the declared range ItemNode: branches that are empty for crates can
    # still contain casts to AssocItemNode. The positive guard fixes the range.
    new += (b"  final ItemNode getASuccessorCrate" + PARAMS + b" {\n"
        b"    result instanceof CrateItemNode and\n    (\n" +
        specialize(body, b"getASuccessorCrate") + b"    )\n  }\n\n")
    candidate = once(once(data, old, new), DEPENDENCY_HEADER, INLINE_DEPENDENCY)
    stage_candidate = once(stages, OLD_BACKREF, NEW_BACKREF)
    if once(once(candidate, new, old), INLINE_DEPENDENCY, DEPENDENCY_HEADER) != data:
        raise RuntimeError("partition patch is not exactly reversible")
    if once(stage_candidate, NEW_BACKREF, OLD_BACKREF) != stages:
        raise RuntimeError("partition stage patch is not exactly reversible")
    return candidate, stage_candidate


def check_extension_boundary(closure, deadline):
    """Reject the fixed experiment if direct partition calls could bypass overrides."""
    inspected = 0
    override = re.compile(r"\boverride\b[^{};]*?\bgetASuccessor\s*\(([^()]*)\)", re.S)
    for name, pack in closure.items():
        for path in sorted(Path(pack["root"]).rglob("*")):
            if time.monotonic() >= deadline:
                raise RuntimeError("partition extension-check deadline exhausted")
            if not path.is_file() or path.suffix not in (".ql", ".qll"):
                continue
            if path.is_symlink() or path.stat().st_size > 64 * 1024**2 or inspected >= 20000:
                raise RuntimeError("partition extension-check input bound exceeded")
            text = path.read_text(encoding="utf-8")
            inspected += 1
            if any(re.search(r"\b" + method + r"\b", text) for method in NAMES):
                raise RuntimeError("partition method collision in " + name + "/" + path.name)
            # Conservatively include comments; a false match stops this candidate,
            # rather than pretending a full QL class/dispatch parser exists here.
            if any(len(match.group(1).split(",")) == 3 for match in override.finditer(text)):
                raise RuntimeError("three-argument successor override in " + name + "/" + path.name)
    return {"status": "NO_SELECTED_CLOSURE_COLLISION_OR_OVERRIDE", "ql_files_inspected": inspected,
        "new_methods_final": True, "outside_selected_closure": "NOT_COVERED"}
