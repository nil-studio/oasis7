"""Patch/closure contracts. Synthetic QL fixture is not a Rust semantic test."""
import hashlib
from pathlib import Path
import tempfile
import time
import unittest
import random
from unittest.mock import patch

import codeql_rust_successor_demand as candidate
from test_codeql_rust_successor_partition import official_fixture, dependency_edges


def pstate(edge):
    return edge[2:4] == ("internal", None)


def transfer(edges, forwards, uses, externs, aliases):
    out = set()
    for owner, target in forwards | externs:
        out |= {(owner, n, k, u, r) for src, n, k, u, r in edges if src == target}
    for owner, use in uses:
        out |= {(owner, n, k, use, r) for src, n, k, u, r in edges if src == use}
    for owner, target in aliases:
        out |= {(owner, n, k, u, r) for src, n, k, u, r in edges
                if src == target and k in ("external", "both")}
    return out


def full_closure(base, forwards, uses, externs, aliases):
    result = set(base)
    while True:
        expanded = result | transfer(result, forwards, uses, externs, aliases)
        if expanded == result:
            return result
        result = expanded


def split_closure(base, forwards, uses, externs, aliases, seeds, *, enum_forwarding=True):
    reach = {node for _, node in uses | externs}
    demand = set(seeds)
    while True:
        more = reach | ({q for p, q in forwards if p in reach} if enum_forwarding else set())
        keys = demand | {(q, n) for p, n in demand for a, q in forwards if a == p}
        if more == reach and keys == demand:
            break
        reach, demand = more, keys
    q = {e for e in base if not pstate(e)}
    enumerated = {e for e in base if pstate(e) and e[0] in reach}
    requested = {e for e in base if pstate(e) and e[:2] in demand}
    while True:
        nq = q | {e for e in transfer(q | enumerated, forwards, uses, externs, aliases) if not pstate(e)}
        ne = enumerated | {e for e in transfer(enumerated, forwards, uses, externs, aliases)
                          if pstate(e) and e[0] in reach}
        np = requested | {e for e in transfer(requested | enumerated, forwards, uses, externs, aliases)
                         if pstate(e) and e[:2] in demand}
        if (nq, ne, np) == (q, enumerated, requested):
            return q, enumerated, requested, reach, demand
        q, enumerated, requested = nq, ne, np


class EnumeratedProjectionTests(unittest.TestCase):
    def test_typed_enumeration_roots_never_reach_module_or_source_file(self):
        nodes = ("use", "extern", "block", "module", "file")
        possible_macro = [(owner, "block") for owner in nodes]
        for bits in range(1 << len(possible_macro)):
            macro = {edge for i, edge in enumerate(possible_macro) if bits & (1 << i)}
            for file_edge in (set(), {("module", "file")}):
                def reach(edges):
                    values = {"use", "extern"}
                    while True:
                        more = values | {target for owner, target in edges if owner in values}
                        if more == values:
                            return values
                        values = more
                self.assertEqual(reach(macro | file_edge), reach(macro))
                self.assertFalse(reach(macro | file_edge) & {"module", "file"})
                base = dependency_edges({"file"}, set(), set(), {("dep", "c-file", 1)})
                base |= {("extern", "ext", "internal", None, "c-extern")}
                uses, externs = {("module", "use")}, {("use", "extern"), ("block", "extern")}
                full = full_closure(base, macro | file_edge, uses, externs, set())
                q, enum, requested, domain, demand = split_closure(base, macro | file_edge, uses, externs, set(), {("file", "dep")})
                self.assertEqual(enum, {e for e in full if pstate(e) and e[0] in reach(macro)})
                self.assertIn(("file", "dep", "internal", None, "c-file"), requested)
                self.assertNotIn(("file", "dep", "internal", None, "c-file"), enum)

    def test_use_to_extern_to_forwarded_file_keeps_unmentioned_names(self):
        base = dependency_edges({"file"}, set(), set(), {("syntax", "c1", 1), ("unmentioned", "c2", 1)})
        forwards, uses, externs = {("extern", "block"), ("block", "file")}, {("scope", "use")}, {("use", "extern")}
        q, enum, requested, reach, demand = split_closure(base, forwards, uses, externs, set(), {("scope", "syntax")})
        expected = ("scope", "unmentioned", "internal", "use", "c2")
        self.assertIn(expected, q)
        self.assertIn("file", reach)
        self.assertNotIn(("file", "unmentioned"), demand)
        self.assertNotIn(expected, split_closure(base, forwards, uses, externs, set(), {("scope", "syntax")}, enum_forwarding=False)[0])

    def test_positive_recursive_projection_matches_full_oracle(self):
        rng = random.Random(22876)
        nodes = list(range(8))
        for _ in range(256):
            base = dependency_edges({0, 1}, set(), {(1, "a")}, {("a", 10, 1), ("b", 11, 1)})
            base |= {(rng.choice(nodes), rng.choice(("a", "b")), rng.choice(("internal", "external", "both")),
                      rng.choice((None, 5, 6)), rng.choice((10, 11))) for _ in range(10)}
            forwards = {(rng.choice(nodes), rng.choice(nodes)) for _ in range(6)}
            uses = {(rng.choice(nodes), rng.choice(nodes)) for _ in range(4)}
            externs = {(rng.choice(nodes), rng.choice(nodes)) for _ in range(4)}
            aliases = {(rng.choice(nodes), rng.choice(nodes)) for _ in range(3)}
            seeds = {(rng.choice(nodes), rng.choice(("a", "b"))) for _ in range(4)}
            full = full_closure(base, forwards, uses, externs, aliases)
            q, enum, requested, reach, demand = split_closure(base, forwards, uses, externs, aliases, seeds)
            self.assertEqual(q, {e for e in full if not pstate(e)})
            self.assertEqual(enum, {e for e in full if pstate(e) and e[0] in reach})
            self.assertEqual(requested, {e for e in full if pstate(e) and e[:2] in demand})


def consumer_fixture():
    clauses = [
        b"    result = getAChildSuccessor(this, name, kind) and useOpt.isNone()\n",
        b"    exists(SourceFileItemNode f | fileImport(this.(Module), f) and\n"
        b"      result = f.getASuccessorCrate(name, kind, useOpt))\n",
        b"    useImportEdge(this, name, result, kind) and useOpt.isNone()\n",
        b"    result = this.(CrateItemNode).getSourceFile().getASuccessorCrate(name, kind, useOpt) and kind.isExternalOrBoth()\n",
        b"    crateDependencyEdge(this, name, result) and kind.isInternal() and useOpt.isNone()\n",
        b"    externCrateEdge(this, name, kind, result) and useOpt.isNone()\n",
        b"    useOpt.asSome() = any(UseItemNode use_ | use_ = this.getASuccessorNonCrate(_, _, _) and\n"
        b"      result = use_.getASuccessorCrate(name, kind, _))\n",
        b"    exists(ExternCrateItemNode ec | result = ec.getASuccessorCrate(name, kind, useOpt) |\n"
        b"      ec = this.getASuccessorNonCrate(_, _, _)\n"
        b"      or\n"
        b"      // if the extern crate appears in the crate root, then the crate name is also added\n"
        b"      // to the 'extern prelude', see https://doc.rust-lang.org/reference/items/extern-crates.html\n"
        b"      exists(Crate c |\n"
        b"        ec = c.getSourceFile().(ItemNode).getASuccessorNonCrate(_, _, _) and\n"
        b"        this = c.getASourceFile()\n      )\n    )\n",
        b"    implEdge(this, name, result) and kind.isExternal() and useOpt.isNone()\n",
        b"    result = this.(TypeAliasItemNodeImpl).resolveAliasCand().getASuccessorCrate(name, kind, useOpt) and kind.isExternalOrBoth()\n",
        b"    exists(BlockExprItemNode be | this = be.getImmediateParent() and\n"
        b"      be = any(MacroCall mc).getMacroCallExpansion() and\n"
        b"      result = be.getASuccessorCrate(name, kind, useOpt))\n",
        b'    name = "self" and useOpt.isNone() and kind.isBoth() and result = this\n',
        b'    kind.isInternal() and useOpt.isNone() and (preludeEdge(this, name, result)\n'
        b'      or builtin(name, result)\n      or name = "crate" and this = result.(CrateItemNode).getASourceFile())\n',
    ]
    raw = candidate.C_HEADER + b"    result instanceof CrateItemNode and\n    (\n" + b"    or\n".join(clauses) + b"    )\n  }\n\n"
    rest = candidate.END + b"\n  ItemNode getASuccessor(string name) {\n"
    rest += b"    result = this.getASuccessor(name, kind, _) and\n      kind.isExternalOrBoth()\n  }\n"
    rest += candidate.NAMESPACE
    rest += b"result = getASuccessor(ancestor, pragma[only_bind_into](name), ns, kind, _)\n"
    rest += b"result = mid.getASuccessor(pragma[only_bind_into](name), kind, useOpt)\n"
    rest += b"result = q.getASuccessor(name, kind, useOpt)\n"
    rest += b"imported = m.(ExternCrateItemNode).getASuccessorNonCrate(_, _, _)\n    or\n"
    rest += b"imported = m.(ExternCrateItemNode).getASuccessorCrate(_, _, _)\n"
    rest += b"    crateDependencyEdge(f, s, crate)\n"
    rest += b'    or s = "self" and ec.getFile() = crate.getASourceFile().getFile()\n'
    return raw + rest, raw


def method(text, name):
    start = text.index(b"  final ItemNode " + name.encode() + candidate.PARAMS)
    return text[start:text.index(b"\n  }\n", start) + len(b"\n  }\n")]


class KeyedSourceTests(unittest.TestCase):
    def test_cold_public_method_and_hot_routes(self):
        base, raw = consumer_fixture()
        out = candidate.translate(base)
        self.assertIn(raw, out, "complete original crate enumerator must be retained verbatim")
        self.assertIn(b"getASuccessorUnqualified(ancestor", out)
        self.assertIn(b"mid.getASuccessorQualified(pragma[only_bind_into](name)", out)
        self.assertIn(b"q.getASuccessorQualified(name", out)
        self.assertIn(b"pred.getASuccessorQualified(name", out)
        self.assertIn(b"this.getASuccessorQualified(name, kind, _) and\n      kind.isExternalOrBoth()", out)
        for name in ("getASuccessorCrateQualified", "getASuccessorCrateRequested", "getASuccessorCrateEnumerated", "getASuccessorQualified"):
            self.assertNotIn(b".getASuccessorCrate(", method(out, name))

    def test_enumerator_domain_cannot_contain_module_or_source_file_terms(self):
        out = candidate.translate(consumer_fixture()[0])
        enum = method(out, "getASuccessorCrateEnumerated")
        self.assertNotIn(b"this.(Module)", enum)
        self.assertNotIn(b"enumeratedCrateDependencyEdge", enum)
        self.assertNotIn(b"preludeEdge(this", enum)
        self.assertNotIn(b"builtin(name", enum)
        self.assertNotIn(b"this = c.getASourceFile()", enum)
        requested = method(out, "getASuccessorCrateRequested")
        self.assertIn(b"this.(Module)", requested)
        self.assertIn(b"requestedCrateDependencyEdge", requested)
        self.assertIn(b"preludeEdge(this", requested)

    def test_inverse_use_and_extern_leaf_preserve_requested_inputs(self):
        out = candidate.translate(consumer_fixture()[0])
        q = method(out, "getASuccessorCrateQualified")
        p = method(out, "getASuccessorCrateRequested")
        self.assertIn(b"useOpt.asSome()", q)
        e = method(out, "getASuccessorCrateEnumerated")
        self.assertIn(b"use_.getASuccessorCrateQualified(name, kind, _)", q)
        self.assertIn(b"use_.getASuccessorCrateEnumerated(name, kind, _)", q)
        self.assertIn(b"ec.getASuccessorCrateQualified(name, kind, useOpt)", q)
        self.assertIn(b"ec.getASuccessorCrateEnumerated(name, kind, useOpt)", p)
        self.assertIn(b"ec.getASuccessorCrateEnumerated(name, kind, useOpt)", e)
        self.assertIn(b"imported = m.(ExternCrateItemNode).getASuccessorNonCrate(_, _, _)", out)
        self.assertIn(b"imported = m.(ExternCrateItemNode).getASuccessorCrateQualified(_, _, _)", out)
        self.assertIn(b"imported = m.(ExternCrateItemNode).getASuccessorCrateEnumerated(_, _, _)", out)
        self.assertNotIn(b"crateDependencyEdge(this", q)
        self.assertNotIn(b"preludeEdge(this", q)
        self.assertIn(b"requestedCrateDependencyEdge(this, name, result)", p)
        self.assertIn(b"crateInternalDemand(this, name)", p)
        self.assertIn(b"preludeEdge(this", p)
        self.assertIn(b'"crate"', p)
        self.assertNotIn(b"useOpt.asSome()", p)
        self.assertIn(b"bindingset[file, name]\npragma[inline_late]", out)
        self.assertIn(b"crateEnumerationReach(this)", e)
        self.assertNotIn(b"enumeratedCrateDependencyEdge", out)
        self.assertIn(b"requestedCrateDependencyEdge(f, s, crate)", out)
        self.assertIn(b'or s = "self" and ec.getFile() = crate.getASourceFile().getFile()', out)
        self.assertIn(b"item instanceof UseItemNode", out)
        self.assertIn(b"item instanceof ExternCrateItemNode", out)
        self.assertIn(b"crateEnumerationReach(owner)", out)
        self.assertNotIn(b"item instanceof SourceFile", candidate.ROOTS)

    def test_forward_demand_and_nested_disjunction(self):
        base, _ = consumer_fixture()
        out = candidate.translate(base)
        self.assertIn(b"unqualifiedPathLookup(item, name, _, _)", out)
        self.assertIn(b"keywordLookup(item, name, _)", out)
        self.assertIn(b"fileImport(owner.(Module), item.(SourceFileItemNode))", out)
        self.assertIn(b"item = be", out)
        self.assertIn(b"be.getASuccessorCrateRequested(name, kind, useOpt)", out)
        self.assertIn(b"f.getASuccessorCrateRequested(name, kind, useOpt)", out)
        # An indented nested 'or' is not a new top-level crate clause.
        nested = base.replace(b"getAChildSuccessor(this, name, kind)",
            b"(getAChildSuccessor(this, name, kind)\n      or none())")
        self.assertIn(b"      or none()", candidate.translate(nested))

    def test_unknown_anchors_or_reapplication_fail_closed(self):
        base, _ = consumer_fixture()
        for bad in (base.replace(b"useOpt.asSome()", b"unrecognizedUse"),
                    base + candidate.NAMESPACE, candidate.translate(base)):
            with self.assertRaises((RuntimeError, ValueError)):
                candidate.translate(bad)


class BoundaryTests(unittest.TestCase):
    def setup_closure(self, root):
        original, _ = official_fixture()
        p = root / "codeql/rust/internal/PathResolution.qll"
        p.parent.mkdir(parents=True); p.write_bytes(original)
        schema = {}
        for path in candidate.SCHEMA:
            p = root / path; p.parent.mkdir(parents=True, exist_ok=True)
            p.write_text("synthetic schema " + path)
            schema[path] = hashlib.sha256(p.read_bytes()).hexdigest()
        return {"codeql/rust-all": {"root": str(root)}}, schema

    def test_lexical_arity_guard_does_not_invent_comment_or_string_calls(self):
        text = '// getASuccessor(a,b,c)\nnode.getASuccessor("a,b"); /* ignored(x,y,z) */\nnode.getASuccessor("a", other(1,2), u);'
        code = candidate.code_without_comments(text)
        matches = list(__import__("re").finditer(r"getASuccessor\s*\(", code))
        self.assertEqual([candidate.arity(code, m.end()) for m in matches], [1, 3])
        self.assertEqual(code.count("\n"), text.count("\n"))

    def test_boundary_rejects_unclassified_public_enumerator_before_compile(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp); closure, schema = self.setup_closure(root)
            q = root / "outside.qll"; q.write_text('node.getASuccessor("x");')
            with patch.object(candidate, "SCHEMA", schema):
                result = candidate.boundary(closure, time.monotonic() + 10)
                self.assertEqual(result["outside_file_three_argument_calls"], 0)
                self.assertEqual(result["outside_file_successor_expressions"][0]["arity"], 1)
                q.write_text('node.getASuccessor("x", k, u);')
                with self.assertRaisesRegex(RuntimeError, "unclassified"):
                    candidate.boundary(closure, time.monotonic() + 10)
                q.write_text('predicate alias = getASuccessor/3;')
                with self.assertRaisesRegex(RuntimeError, "symbol reference"):
                    candidate.boundary(closure, time.monotonic() + 10)

    def test_schema_and_new_name_identity_fail_closed(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp); closure, schema = self.setup_closure(root)
            with self.assertRaisesRegex(RuntimeError, "schema identity"):
                candidate.boundary(closure, time.monotonic() + 10)
            with patch.object(candidate, "SCHEMA", schema):
                (root / "outside.qll").write_text("predicate crateInternalDemand() { any() }")
                with self.assertRaisesRegex(RuntimeError, "collision"):
                    candidate.boundary(closure, time.monotonic() + 10)

    def test_closure_calls_are_classified_but_three_args_and_bare_refs_still_stop(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp); closure, schema = self.setup_closure(root)
            p = root / "outside.qll"
            p.write_text('predicate inLoop() { this.getASuccessor+() = this }\nnode.getASuccessor * ();')
            with patch.object(candidate, "SCHEMA", schema):
                value = candidate.boundary(closure, time.monotonic() + 10)
                self.assertEqual([(c["arity"], c["closure_operator"]) for c in value["outside_file_successor_expressions"]], [(0, "+"), (0, "*")])
                for text, message, count in (
                    ('node.getASuccessor+("x", k, u);', "three-argument", 3),
                    ('predicate alias = getASuccessor/3;', "symbol reference", None),
                ):
                    p.write_text(text)
                    with self.assertRaisesRegex(candidate.BoundaryError, message) as failed:
                        candidate.boundary(closure, time.monotonic() + 10)
                    context = failed.exception.context
                    self.assertEqual(context["path"], "outside.qll")
                    self.assertEqual(context["line"], 1)
                    self.assertEqual(context["arity"], count)
                    self.assertEqual(context["source_sha256"], hashlib.sha256(p.read_bytes()).hexdigest())

    def test_reviewed_stage_rewrite_is_exact_not_a_whole_file_exception(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp); closure, schema = self.setup_closure(root)
            p = root / candidate.STAGES_PATH
            p.parent.mkdir(parents=True, exist_ok=True)
            raw = b"module Stage {\n" + candidate.partition.OLD_BACKREF + b"\n}\n"
            patched = raw.replace(candidate.partition.OLD_BACKREF, candidate.partition.NEW_BACKREF)
            p.write_bytes(raw)
            with patch.object(candidate, "SCHEMA", schema), \
                    patch.object(candidate, "STAGES_SOURCE_SHA", hashlib.sha256(raw).hexdigest()), \
                    patch.object(candidate, "STAGES_CANDIDATE_SHA", hashlib.sha256(patched).hexdigest()):
                result = candidate.boundary(closure, time.monotonic() + 10)
                self.assertEqual(result["outside_file_three_argument_calls"], 1)
                self.assertEqual(result["unclassified_outside_file_three_argument_calls"], 0)
                self.assertEqual(len(result["reviewed_call_rewrites"]), 1)
                self.assertEqual(result["reviewed_call_rewrites"][0]["candidate_sha256"], hashlib.sha256(patched).hexdigest())
                other = root / "unreviewed.qll"; other.write_bytes(raw)
                with self.assertRaises(candidate.BoundaryError):
                    candidate.boundary(closure, time.monotonic() + 10)
                other.unlink()
                p.write_bytes(b"// different source\n" + raw)
                with self.assertRaises(candidate.BoundaryError):
                    candidate.boundary(closure, time.monotonic() + 10)
                p.write_bytes(raw)
                with patch.object(candidate, "STAGES_CANDIDATE_SHA", "0" * 64):
                    with self.assertRaises(candidate.BoundaryError):
                        candidate.boundary(closure, time.monotonic() + 10)
                extra = raw + b"other.getASuccessor(a, b, c);\n"
                p.write_bytes(extra)
                # Even a future reviewed source fingerprint must not exempt
                # a second call outside the unique replacement anchor.
                with patch.object(candidate, "STAGES_SOURCE_SHA", hashlib.sha256(extra).hexdigest()), \
                        patch.object(candidate, "STAGES_CANDIDATE_SHA", hashlib.sha256(extra.replace(candidate.partition.OLD_BACKREF, candidate.partition.NEW_BACKREF)).hexdigest()):
                    with self.assertRaises(candidate.BoundaryError) as failed:
                        candidate.boundary(closure, time.monotonic() + 10)
                    self.assertEqual(failed.exception.context["line"], 4)


if __name__ == "__main__":
    unittest.main()
