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
MACRO_OLD = b"""private predicate macroUseEdge(
  ItemNode i, string name, SuccessorKind kind, UseOption useOpt, MacroItemNode macro
) {
  exists(ItemNode m |
    m = i.getASuccessor(_, _, useOpt) and
    m.hasAttr("macro_use")
  |
    macro = m.(ModuleItemNode).getASuccessor(name, kind, _)
    or
    macro = m.(ExternCrateItemNode).getASuccessor(_, _, _).getASuccessor(name, kind, _)
  )
}
"""
MACRO_NEW = b"""private predicate macroUseEdge(
  ItemNode i, string name, SuccessorKind kind, UseOption useOpt, MacroItemNode macro
) {
  exists(ItemNode m |
    m = i.getASuccessorNonCrate(_, _, useOpt) and
    m.hasAttr("macro_use")
  |
    macro = m.(ModuleItemNode).getASuccessorNonCrate(name, kind, _)
    or
    exists(ItemNode imported |
      (
        imported = m.(ExternCrateItemNode).getASuccessorNonCrate(_, _, _)
        or
        imported = m.(ExternCrateItemNode).getASuccessorCrate(_, _, _)
      ) and
      macro = imported.getASuccessorNonCrate(name, kind, _)
    )
  )
}
"""
CONSUMER_PATCHES = (
    (b"t1 = i2.getASuccessor(_, _, _)", b"t1 = i2.getASuccessorNonCrate(_, _, _)"),
    (b"pragma[nomagic]\nprivate ItemNode getASuccessor(\n  ItemNode pred, string name, Namespace ns, SuccessorKind kind, UseOption useOpt",
     b"private ItemNode getASuccessor(\n  ItemNode pred, string name, Namespace ns, SuccessorKind kind, UseOption useOpt"),
    (b"  pragma[nomagic]\n  ItemNode getASuccessor(string name) {",
     b"  ItemNode getASuccessor(string name) {"),
    (MACRO_OLD, MACRO_NEW),
)


def once(data, old, new):
    if data.count(old) != 1:
        raise RuntimeError("partition patch anchor is not unique")
    return data.replace(old, new, 1)


def remove_empty_branches(body, target):
    # This split uses the exact frozen source's top-level indentation, not a
    # general QL parser. QL rejects statically empty terms instead of retaining
    # them as false disjuncts (even when the declared return type is ItemNode).
    clauses = re.split(rb"(?m)^    or\n", body)
    if target == b"getASuccessorNonCrate":
        kept = [c for c in clauses if b"externCrateEdge(this, name, kind, result)" not in c]
        if len(clauses) - len(kept) != 1:
            raise RuntimeError("crate-only extern branch differs")
        return once(b"    or\n".join(kept), b'      or\n      name = "crate" and\n'
            b"      this = result.(CrateItemNode).getASourceFile()\n", b"")
    impossible = (b"macroExportEdge(", b"macroUseEdge(", b"any(TraitItemNodeImpl trait |",
        b"typeImplEdge(", b"this.(TypeParamItemNodeImpl)", b"this.(ImplTraitTypeReprItemNodeImpl)",
        b'name = "super" and')
    kept = [c for c in clauses if not any(marker in c for marker in impossible)]
    if len(clauses) - len(kept) != 7:
        raise RuntimeError("non-crate-only result branches differ")
    self_clauses = [c for c in kept if b'name = "self" and' in c]
    if len(self_clauses) != 1:
        raise RuntimeError("self successor branch differs")
    # Under the crate output guard, result=this selects the original Crate
    # arm. Parent modules and the Module/Enum/Trait arms cannot return a crate.
    return b"    or\n".join(b'    name = "self" and\n    useOpt.isNone() and\n'
        b"    kind.isBoth() and\n    result = this\n" if c == self_clauses[0] else c for c in kept)


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
    return remove_empty_branches(body, target)


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
    # The guard defines the crate range without adding an ItemNode subclass.
    # Statically impossible QL terms were removed above; declaring ItemNode
    # does not prevent type narrowing by this positive guard.
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


def consumer_sources(data, stages, path_sha, stages_sha):
    base, stage_candidate = partition_sources(data, stages, path_sha, stages_sha)
    candidate = base
    for old, new in CONSUMER_PATCHES:
        candidate = once(candidate, old, new)
    restored = candidate
    for old, new in reversed(CONSUMER_PATCHES):
        restored = once(restored, new, old)
    if restored != base:
        raise RuntimeError("consumer patch is not exactly reversible")
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
