"""Finite relation model; not real Rust semantics or CodeQL performance."""
import itertools
import hashlib
from pathlib import Path
import tempfile
import time

import codeql_rust_successor_partition as candidate
import random
import unittest


def dependency_edges(files, explicit, declarations, versions):
    edges = {(f, name, "internal", None, c) for f, name, c in explicit}
    blocked = declarations | {(f, name) for f, name, _ in explicit}
    for name in {name for name, _, _ in versions}:
        latest = max(v for n, c, v in versions if n == name)
        crates = {c for n, c, v in versions if n == name and v == latest}
        if len(crates) == 1:
            c = next(iter(crates))
            edges |= {(f, name, "internal", None, c) for f in files if (f, name) not in blocked}
    return edges


def derive(edges, forwards, aliases):
    result = set()
    for owner, source in forwards:
        result |= {(owner, name, kind, use, target)
                   for src, name, kind, use, target in edges if src == source}
    for owner, output_name, start, first, second, output_kind, provenance in aliases:
        mids = {target for src, name, kind, use, target in edges
                if src == start and name == first and kind in ("internal", "both")}
        result |= {(owner, output_name, output_kind, provenance, target)
                   for src, name, kind, use, target in edges
                   if src in mids and name == second and kind in ("external", "both")}
    return result


def original(base, forwards, aliases):
    edges = set(base)
    while True:
        next_edges = edges | derive(edges, forwards, aliases)
        if next_edges == edges:
            return edges
        edges = next_edges


def partitioned(base, forwards, aliases, crates, *, omit_crate_intermediates=False):
    noncrate = {e for e in base if e[4] not in crates}
    crate = set(base) - noncrate
    while True:
        union = noncrate if omit_crate_intermediates else noncrate | crate
        # Two heads consume the same union equations. The production prototype
        # specializes terminal calls; this checks the partition fixed point.
        new_noncrate = noncrate | {e for e in derive(union, forwards, aliases) if e[4] not in crates}
        new_crate = crate | {e for e in derive(union, forwards, aliases) if e[4] in crates}
        if new_noncrate == noncrate and new_crate == crate:
            assert not noncrate & crate
            return noncrate | crate
        noncrate, crate = new_noncrate, new_crate


class PartitionModel(unittest.TestCase):
    def test_cross_crate_function_survives_unknown_owner_forwarding_and_cycle(self):
        base = dependency_edges({0, 1}, set(), {(1, "pkg")}, {("pkg", 2, 1)})
        base |= {(2, "make", "external", None, 3), (3, "value", "external", 7, 4)}
        forwards = {(5, 0), (0, 5)}
        aliases = {(0, "call", 5, "pkg", "make", "both", 9),
                   (1, "blocked", 1, "pkg", "make", "internal", 10),
                   (0, "again", 0, "call", "value", "external", 11)}
        expected = {(0, "call", "both", 9, 3), (0, "again", "external", 11, 4)}
        old = original(base, forwards, aliases)
        self.assertTrue(expected <= old)
        self.assertNotIn((1, "blocked", "internal", 10, 3), old)
        self.assertEqual(old, partitioned(base, forwards, aliases, {2}))
        mutant = partitioned(base, forwards, aliases, {2}, omit_crate_intermediates=True)
        self.assertFalse(expected <= mutant, "dropping crates must lose the real consumer witness")

    def test_rename_conflict_latest_tie_and_explicit_dependency_are_preserved(self):
        versions = {("pkg", 2, 1), ("pkg", 3, 2), ("tie", 2, 4), ("tie", 3, 4)}
        actual = dependency_edges({0, 1}, {(0, "renamed", 2), (0, "tie", 2)}, {(1, "pkg")}, versions)
        expected = {(0, "renamed", "internal", None, 2), (0, "tie", "internal", None, 2),
                    (0, "pkg", "internal", None, 3)}
        self.assertEqual(actual, expected)
        self.assertEqual(original(actual, set(), set()), partitioned(actual, set(), set(), {2, 3}))

    def test_exhaustive_two_node_relation_with_cycles_and_aliases(self):
        values = list(itertools.product(range(2), repeat=2))
        def subsets():
            for bits in range(16):
                yield {v for i, v in enumerate(values) if bits & (1 << i)}
        count = 0
        for links, forwards, alias, fallback in itertools.product(subsets(), subsets(), (False, True), (False, True)):
            base = {(a, "a", "both", None, b) for a, b in links}
            if fallback:
                base |= dependency_edges({0}, set(), set(), {("a", 1, 0)})
            aliases = {(0, "a", 0, "a", "a", "both", 1)} if alias else set()
            self.assertEqual(original(base, forwards, aliases), partitioned(base, forwards, aliases, {1}))
            count += 1
        self.assertEqual(count, 1024)

    def test_seeded_mixed_outputs_visibility_and_provenance(self):
        rng = random.Random(5166)
        for _ in range(1024):
            crates = {4, 5}
            versions = {(n, c, rng.randrange(2)) for n in ("a", "b") for c in crates}
            explicit = {(f, n, rng.choice(tuple(crates))) for f in (0, 1) for n in ("a", "b") if rng.random() < .3}
            declarations = {(f, n) for f in (0, 1) for n in ("a", "b") if rng.random() < .3}
            base = dependency_edges({0, 1}, explicit, declarations, versions)
            base |= {(rng.randrange(6), rng.choice(("a", "b", "c")), rng.choice(("internal", "external", "both")),
                      rng.choice((None, 7)), rng.randrange(6)) for _ in range(12)}
            forwards = {(rng.randrange(6), rng.randrange(6)) for _ in range(5)}
            aliases = {(rng.randrange(6), rng.choice(("a", "b", "c")), rng.randrange(6),
                        rng.choice(("a", "b", "c")), rng.choice(("a", "b", "c")),
                        rng.choice(("internal", "external", "both")), rng.choice((None, 9))) for _ in range(4)}
            self.assertEqual(original(base, forwards, aliases), partitioned(base, forwards, aliases, crates))


def official_fixture():
    path = b"""// Synthetic safety fixture, not compiled Rust QL.
  cached
  ItemNode getASuccessor(string name, SuccessorKind kind, UseOption useOpt) {
    Stages::PathResolutionStage::ref() and
    result = getAChildSuccessor(this, name, kind) and
    useOpt.isNone()
    or
    fileImportEdge(this, name, result, kind, useOpt)
    or
    useImportEdge(this, name, result, kind) and useOpt.isNone()
    or
    crateDefEdge(this, name, result, kind, useOpt)
    or
    crateDependencyEdge(this, name, result) and
    kind.isInternal() and
    useOpt.isNone()
    or
    externCrateEdge(this, name, kind, result) and useOpt.isNone()
    or
    exists(ExternCrateItemNode ec | result = ec.getASuccessor(name, kind, useOpt) |
      ec = this.getASuccessor(_, _, _)
    )
    or
    macroExportEdge(this, name, result) and kind.isBoth()
    or
    macroUseEdge(this, name, kind, useOpt, result)
    or
    this = any(TraitItemNodeImpl trait | result = trait.getAssocItem(name))
    or
    typeImplEdge(this, _, name, result)
    or
    implEdge(this, name, result)
    or
    result = this.(TypeParamItemNodeImpl).resolveABoundCand().getASuccessor(name, kind, useOpt).(AssocItemNode)
    or
    result = this.(ImplTraitTypeReprItemNodeImpl).resolveABoundCand().getASuccessor(name, kind, useOpt).(AssocItemNode)
    or
    result = this.(TypeAliasItemNodeImpl).resolveAliasCand().getASuccessor(name, kind, useOpt)
    or
    name = "super" and result = this.getImmediateParentModule()
    or
    name = "self" and (if this instanceof Crate then result = this else result = this.getImmediateParentModule())
    or
    kind.isInternal() and useOpt.isNone() and
    (
      preludeEdge(this, name, result)
      or
      this instanceof SourceFile and builtin(name, result)
      or
      name = "crate" and
      this = result.(CrateItemNode).getASourceFile()
    )
  }

  /** Gets an _external_ successor named `name`, if any. */
pragma[nomagic]
private predicate crateDependencyEdge(SourceFileItemNode file, string name, CrateItemNode dep) {
  crateDependency(file, name, dep)
  or
  dep = unique(CrateItemNode dep0 | dep0.isLatestVersion(name)) and
  not hasDeclOrDep(file, name)
}
"""
    stages = b"""predicate ref() { 1 = 1 }
predicate backref() { 1 = 1 or exists(any(ItemNode i).getASuccessor(_, _, _)) }
"""
    return path, stages


class PartitionPatch(unittest.TestCase):
    def test_exact_partition_patch_preserves_fallback_and_class_population(self):
        data, stages = official_fixture()
        result, stage = candidate.partition_sources(data, stages,
            hashlib.sha256(data).hexdigest(), hashlib.sha256(stages).hexdigest())
        wrapper = result.split(b"  cached\n  final ItemNode getASuccessorNonCrate", 1)[0]
        self.assertIn(b"result = this.getASuccessorNonCrate(name, kind, useOpt)", wrapper)
        self.assertIn(b"result = this.getASuccessorCrate(name, kind, useOpt)", wrapper)
        n = result.split(b"  cached\n  final ItemNode getASuccessorNonCrate", 1)[1].split(b"  final ItemNode getASuccessorCrate", 1)[0]
        c = result.split(b"  final ItemNode getASuccessorCrate", 1)[1]
        self.assertIn(b"not result instanceof CrateItemNode", n)
        self.assertIn(b"if this instanceof Crate", n, "do not send crate receivers into the parent-module else arm")
        self.assertNotIn(b"crateDependencyEdge(this, name, result)", n)
        self.assertIn(b"result instanceof CrateItemNode", c)
        self.assertIn(b"crateDependencyEdge(this, name, result)", c)
        self.assertEqual(result.count(b"class "), data.count(b"class "))
        self.assertIn(b"not hasDeclOrDep(file, name)", result)
        self.assertIn(b"unique(CrateItemNode dep0 | dep0.isLatestVersion(name))", result)
        self.assertIn(candidate.NEW_BACKREF, stage)
        self.assertIn(b"1 = 1", stage)

    def test_crate_partition_emits_no_statically_impossible_output_rules(self):
        data, stages = official_fixture()
        result, _ = candidate.partition_sources(data, stages,
            hashlib.sha256(data).hexdigest(), hashlib.sha256(stages).hexdigest())
        crate = result.split(b"  final ItemNode getASuccessorCrate", 1)[1].split(candidate.METHOD_END, 1)[0]
        for impossible in (b"macroExportEdge", b"macroUseEdge", b"any(TraitItemNodeImpl", b"typeImplEdge",
                b"TypeParamItemNodeImpl", b"ImplTraitTypeReprItemNodeImpl", b'name = "super"', b"getImmediateParentModule"):
            self.assertNotIn(impossible, crate)
        for required in (b"implEdge", b"resolveAliasCand", b"crateDependencyEdge", b"externCrateEdge", b"useImportEdge", b"preludeEdge"):
            self.assertIn(required, crate)
        self.assertIn(b'name = "self"', crate)
        self.assertIn(b"result = this", crate)

    def test_changed_or_already_patched_source_fails_closed(self):
        data, stages = official_fixture()
        path_sha, stages_sha = hashlib.sha256(data).hexdigest(), hashlib.sha256(stages).hexdigest()
        result, stage = candidate.partition_sources(data, stages, path_sha, stages_sha)
        for d, s in ((data + b"x", stages), (data, stages + b"x"), (result, stages), (data, stage)):
            with self.assertRaises(RuntimeError):
                candidate.partition_sources(d, s, path_sha, stages_sha)

    def test_extension_guard_rejects_override_and_method_collision_before_compile(self):
        for text, reject in (("override Node getASuccessor() { none() }", False),
                ("override Node getASuccessor(string name) { none() }", False),
                ("override ItemNode getASuccessor(string name, SuccessorKind kind, UseOption useOpt) { none() }", True),
                ("final ItemNode getASuccessorNonCrate(string name) { none() }", True)):
            with self.subTest(text=text), tempfile.TemporaryDirectory() as tmp:
                root = Path(tmp)
                (root / "Example.qll").write_text(text)
                closure = {"codeql/rust-all": {"root": str(root)}}
                if reject:
                    with self.assertRaises(RuntimeError):
                        candidate.check_extension_boundary(closure, time.monotonic() + 10)
                else:
                    self.assertEqual(candidate.check_extension_boundary(closure, time.monotonic() + 10)["ql_files_inspected"], 1)
                with self.assertRaisesRegex(RuntimeError, "deadline"):
                    candidate.check_extension_boundary(closure, time.monotonic() - 1)


if __name__ == "__main__":
    unittest.main()
