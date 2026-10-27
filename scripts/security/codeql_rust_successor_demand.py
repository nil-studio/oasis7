"""Frozen-source keyed P / complete Q successor experiment; compile-only."""
import hashlib
from pathlib import Path
import re
import time

import codeql_rust_successor_partition as partition


PARAMS = partition.PARAMS
C_HEADER = b"  final ItemNode getASuccessorCrate" + PARAMS + b" {\n"
END = b"  /** Gets an _external_ successor named `name`, if any. */"
NEW_NAMES = ("getASuccessorCrateQualified", "getASuccessorCrateRequested",
    "getASuccessorQualified", "getASuccessorUnqualified", "crateInternalDemand",
    "requestedCrateDependencyEdge", "getASuccessorCrateEnumerated",
    "crateEnumerationReach", "crateInternalForward", "enumeratedCrateDependencyEdge")
SCHEMA = {
    "codeql/rust/elements/internal/generated/Synth.qll":
        "1a1bfe65be484d44b08c27447b40bb7a31e6011690e5b97f493ac920b7828691",
    "codeql/rust/elements/internal/generated/Crate.qll":
        "649a3d7cd7ee99f95f8a4d3d3c41ea2fa848ce7d8415ccbac62977dfc9a49d35",
}
NAMESPACE = b"""private ItemNode getASuccessor(
  ItemNode pred, string name, Namespace ns, SuccessorKind kind, UseOption useOpt
) {
  result = pred.getASuccessor(name, kind, useOpt) and
  ns = result.getNamespace()
}
"""
ROOTS = b"""// Internal/None crate requests originate only in unqualified AST lookups.
// Qualified/use-tree/glob consumers use complete Q, retaining visibility checks.
private predicate crateInternalForward(ItemNode owner, ItemNode item) {
  fileImport(owner.(Module), item.(SourceFileItemNode))
  or
  exists(BlockExprItemNode be |
    owner = be.getImmediateParent() and
    be = any(MacroCall mc).getMacroCallExpansion() and
    item = be
  )
}

private predicate crateInternalDemand(ItemNode item, string name) {
  unqualifiedPathLookup(item, name, _, _)
  or
  keywordLookup(item, name, _)
  or
  exists(ItemNode owner |
    crateInternalDemand(owner, name) and
    crateInternalForward(owner, item)
  )
}

// Full wildcard inputs of a use/extern consumer remain complete, even for
// unusual AST forwarding. Only receivers reachable from those roots enumerate P.
private predicate crateEnumerationReach(ItemNode item) {
  item instanceof UseItemNode
  or
  item instanceof ExternCrateItemNode
  or
  exists(ItemNode owner |
    crateEnumerationReach(owner) and
    crateInternalForward(owner, item)
  )
}

bindingset[file, name]
pragma[inline_late]
private predicate requestedCrateDependencyEdge(
  SourceFileItemNode file, string name, CrateItemNode dep
) {
  crateDependencyEdge(file, name, dep)
}

bindingset[file]
pragma[inline_late]
private predicate enumeratedCrateDependencyEdge(
  SourceFileItemNode file, string name, CrateItemNode dep
) {
  crateDependencyEdge(file, name, dep)
}

private ItemNode getASuccessorUnqualified(
  ItemNode pred, string name, Namespace ns, SuccessorKind kind, UseOption useOpt
) {
  (
    result = pred.getASuccessorQualified(name, kind, useOpt)
    or
    result = pred.getASuccessorCrateRequested(name, kind, useOpt)
  ) and
  ns = result.getNamespace()
}

"""


def once(data, old, new):
    if data.count(old) != 1:
        raise RuntimeError("keyed successor anchor differs")
    return data.replace(old, new, 1)


def translate(base):
    """Translate the already checked N/C consumer representation reversibly."""
    if any(name.encode() in base for name in NEW_NAMES):
        raise RuntimeError("keyed successor name collision")
    start = base.index(C_HEADER)
    end = base.index(END, start)
    raw = base[start:end]
    prefix = C_HEADER + b"    result instanceof CrateItemNode and\n    (\n"
    suffix = b"    )\n  }\n\n"
    if not raw.startswith(prefix) or not raw.endswith(suffix):
        raise RuntimeError("crate method boundary differs")
    clauses = re.split(rb"(?m)^    or\n", raw[len(prefix):-len(suffix)])
    markers = (b"getAChildSuccessor", b"fileImport(", b"useImportEdge(",
        b"this.(CrateItemNode)", b"crateDependencyEdge(", b"externCrateEdge(",
        b"useOpt.asSome()", b"exists(ExternCrateItemNode ec", b"implEdge(",
        b"this.(TypeAliasItemNodeImpl)", b"exists(BlockExprItemNode be",
        b'name = "self"', b"preludeEdge(")
    if len(clauses) != len(markers) or any(m not in c for m, c in zip(markers, clauses)):
        raise RuntimeError("crate clause mapping differs")

    # Q is a complete enumerator: no names are pruned. Its direct P base is
    # impossible, but Use edges may still need P at their actual syntax keys.
    q = []
    for index, clause in enumerate(clauses):
        if index in (4, 12):
            continue
        if index == 6:
            clause = once(clause, b"result = use_.getASuccessorCrate(name, kind, _)",
                b"(\n          result = use_.getASuccessorCrateQualified(name, kind, _)\n"
                b"          or\n          result = use_.getASuccessorCrateEnumerated(name, kind, _)\n        )")
        clause = clause.replace(b".getASuccessorCrate(", b".getASuccessorCrateQualified(")
        q.append(clause)
    qualified = (b"  final ItemNode getASuccessorCrateQualified" + PARAMS + b" {\n"
        b"    result instanceof CrateItemNode and\n"
        b"    (kind.isExternalOrBoth() or exists(useOpt.asSome())) and\n    (\n" +
        b"    or\n".join(q) + suffix)

    # P has exactly one kind/provenance state. File/macro forwarding preserves
    # its receiver/name key; use paths and extern identifiers retain their own
    # exact syntactic name mappings instead of enumerating the fallback domain.
    p = []
    for index in (0, 1, 2, 4, 5, 7, 10, 12):
        clause = clauses[index]
        if index == 4:
            clause = once(clause, b"crateDependencyEdge(this, name, result)",
                b"requestedCrateDependencyEdge(this, name, result)")
        if index == 7:
            clause = clause.replace(b".getASuccessorCrate(", b".getASuccessorCrateEnumerated(")
        clause = clause.replace(b".getASuccessorCrate(", b".getASuccessorCrateRequested(")
        p.append(clause)
    requested = (b"  final ItemNode getASuccessorCrateRequested" + PARAMS + b" {\n"
        b"    result instanceof CrateItemNode and\n"
        b"    kind.isInternal() and useOpt.isNone() and\n"
        b"    crateInternalDemand(this, name) and\n    (\n" + b"    or\n".join(p) + suffix)
    enumerated = requested.replace(b"getASuccessorCrateRequested", b"getASuccessorCrateEnumerated")
    enumerated = once(enumerated, b"crateInternalDemand(this, name)", b"crateEnumerationReach(this)")
    enumerated = once(enumerated, b"requestedCrateDependencyEdge(this, name, result)",
        b"enumeratedCrateDependencyEdge(this, name, result)")
    helper = (b"  final ItemNode getASuccessorQualified" + PARAMS + b" {\n"
        b"    result = this.getASuccessorNonCrate(name, kind, useOpt)\n    or\n"
        b"    result = this.getASuccessorCrateQualified(name, kind, useOpt)\n  }\n\n")

    changes = [(raw, raw + qualified + requested + enumerated + helper),
        (NAMESPACE, ROOTS + once(NAMESPACE, b"pred.getASuccessor(", b"pred.getASuccessorQualified(")),
        (b"result = getASuccessor(ancestor, pragma[only_bind_into](name), ns, kind, _)",
         b"result = getASuccessorUnqualified(ancestor, pragma[only_bind_into](name), ns, kind, _)"),
        (b"result = this.getASuccessor(name, kind, _) and\n      kind.isExternalOrBoth()",
         b"result = this.getASuccessorQualified(name, kind, _) and\n      kind.isExternalOrBoth()"),
        (b"result = mid.getASuccessor(pragma[only_bind_into](name), kind, useOpt)",
         b"result = mid.getASuccessorQualified(pragma[only_bind_into](name), kind, useOpt)"),
        (b"result = q.getASuccessor(name, kind, useOpt)",
         b"result = q.getASuccessorQualified(name, kind, useOpt)"),
        (b"imported = m.(ExternCrateItemNode).getASuccessorCrate(_, _, _)",
         b"(\n          imported = m.(ExternCrateItemNode).getASuccessorCrateQualified(_, _, _)\n"
         b"          or\n          imported = m.(ExternCrateItemNode).getASuccessorCrateEnumerated(_, _, _)\n        )")]
    result = base
    for old, new in changes:
        result = once(result, old, new)
    restored = result
    for old, new in reversed(changes):
        restored = once(restored, new, old)
    if restored != base:
        raise RuntimeError("keyed patch is not reversible")
    return result


def sources(data, stages, path_sha, stages_sha):
    base, stage_candidate = partition.consumer_sources(data, stages, path_sha, stages_sha)
    return translate(base), stage_candidate


def code_without_comments(text):
    # Preserve offsets/newlines; literals/comments cannot create a call. This
    # is a conservative lexical guard, not a receiver-type or full QL parser.
    def blank(match):
        out = "".join("\n" if c == "\n" else " " for c in match[0])
        return "x" + out[1:] if match[0].startswith('"') else out
    return re.sub(r'"(?:\\.|[^"\\])*"|//[^\n]*|/\*.*?\*/', blank, text, flags=re.S)


def arity(code, start):
    depth, commas, content = 0, 0, False
    for char in code[start:]:
        if char in "([{":
            depth += 1
        elif char in ")]}":
            if depth == 0:
                return commas + 1 if content else 0
            depth -= 1
        elif char == "," and depth == 0:
            commas += 1
        if not char.isspace():
            content = True
    raise RuntimeError("unterminated successor expression")


def boundary(closure, deadline):
    result = partition.check_extension_boundary(closure, deadline)
    official = Path(closure["codeql/rust-all"]["root"])
    identities = {}
    for path, expected in SCHEMA.items():
        actual = hashlib.sha256((official / path).read_bytes()).hexdigest()
        if actual != expected:
            raise RuntimeError("keyed schema identity differs: " + path)
        identities[path] = actual
    calls = []
    for name, pack in closure.items():
        root = Path(pack["root"])
        for path in sorted(root.rglob("*")):
            if time.monotonic() >= deadline:
                raise RuntimeError("keyed closure boundary deadline")
            if not path.is_file() or path.suffix not in (".ql", ".qll"):
                continue
            text = path.read_text()
            code = code_without_comments(text)
            if any(re.search(r"\b" + method + r"\b", code) for method in NEW_NAMES):
                raise RuntimeError("keyed method collision: " + name + "/" + str(path.relative_to(root)))
            if path == official / "codeql/rust/internal/PathResolution.qll":
                continue
            for match in re.finditer(r"\bgetASuccessor\b\s*", code):
                if code[match.end():match.end()+1] != "(":
                    raise RuntimeError("unclassified outside-file successor symbol reference")
                count = arity(code, match.end() + 1)
                calls.append({"pack": name, "path": str(path.relative_to(root)),
                    "line": code.count("\n", 0, match.start()) + 1, "arity": count})
                if len(calls) > 4096 or count == 3:
                    raise RuntimeError("unclassified outside-file three-argument successor call")
    return {**result, "keyed_schema": identities, "outside_file_successor_expressions": calls,
        "outside_file_three_argument_calls": 0, "guard_scope": "selected closure only; conservative arity, not receiver types",
        "recursive_negation": "no new negative rule; original complete AST/dependency negatives retained; actual QL compiler stratification still required"}
