"""Consumer relation proof checks, not real Rust semantics or performance."""
import hashlib
import random
import unittest

import codeql_rust_successor_partition as candidate
from test_codeql_rust_successor_partition import official_fixture


def original_macro(edges, modules, externs, macros, attrs):
    result = set()
    for owner, _, _, provenance, imported in edges:
        if imported not in attrs:
            continue
        if imported in modules:
            result |= {(owner, name, kind, provenance, target)
                for source, name, kind, _, target in edges if source == imported and target in macros}
        if imported in externs:
            middle = {target for source, _, _, _, target in edges if source == imported}
            result |= {(owner, name, kind, provenance, target)
                for source, name, kind, _, target in edges if source in middle and target in macros}
    return result


def consumer_macro(edges, crates, modules, externs, macros, attrs, middle_mode="both"):
    noncrate = {edge for edge in edges if edge[4] not in crates}
    crate = edges - noncrate
    result = set()
    for owner, _, _, provenance, imported in noncrate:
        if imported not in attrs:
            continue
        if imported in modules:
            result |= {(owner, name, kind, provenance, target)
                for source, name, kind, _, target in noncrate if source == imported and target in macros}
        if imported in externs:
            candidates = noncrate | crate if middle_mode == "both" else (crate if middle_mode == "crate" else noncrate)
            middle = {target for source, _, _, _, target in candidates if source == imported}
            result |= {(owner, name, kind, provenance, target)
                for source, name, kind, _, target in noncrate if source in middle and target in macros}
    return result


class ConsumerProof(unittest.TestCase):
    def test_extern_middle_must_keep_crate_and_parent_module_with_provenance(self):
        edges = {(0, "extern", "both", 17, 1), (1, "dep", "internal", None, 2),
                 (1, "self", "internal", None, 3), (2, "from_crate", "both", None, 4),
                 (3, "from_parent", "internal", 99, 5), (2, "not_macro", "both", None, 6)}
        crates, modules, externs, macros, attrs = {2}, {0, 3}, {1}, {4, 5}, {1}
        expected = {(0, "from_crate", "both", 17, 4), (0, "from_parent", "internal", 17, 5)}
        self.assertEqual(original_macro(edges, modules, externs, macros, attrs), expected)
        self.assertEqual(consumer_macro(edges, crates, modules, externs, macros, attrs), expected)
        self.assertNotEqual(consumer_macro(edges, crates, modules, externs, macros, attrs, "crate"), expected)
        self.assertNotEqual(consumer_macro(edges, crates, modules, externs, macros, attrs, "noncrate"), expected)
        self.assertEqual(consumer_macro(edges, crates, modules, externs, macros, set()), set())

    def test_trait_target_can_have_crate_receiver_and_same_text_as_crate_target(self):
        edges = {(0, "same", "both", None, 2), (0, "same", "both", None, 4),
                 (2, "exported", "both", None, 4)}
        crates, traits = {2}, {4}
        original = {(target, source) for source, _, _, _, target in edges if target in traits}
        narrowed = {(target, source) for source, _, _, _, target in edges if target not in crates and target in traits}
        self.assertEqual(original, {(4, 0), (4, 2)})
        self.assertEqual(narrowed, original)

    def test_seeded_pointwise_and_mutually_recursive_macro_closure(self):
        rng = random.Random(2287604)
        crates, modules, externs, macros = {2}, {0, 3}, {1}, {4, 5}
        for _ in range(1024):
            edges = {(rng.randrange(7), rng.choice(("a", "b")), rng.choice(("internal", "external", "both")),
                      rng.choice((None, 17)), rng.randrange(7)) for _ in range(20)}
            attrs = {x for x in range(7) if rng.random() < .5}
            self.assertEqual(original_macro(edges, modules, externs, macros, attrs),
                consumer_macro(edges, crates, modules, externs, macros, attrs))
            old, new = set(edges), set(edges)
            while True:
                old_next = old | original_macro(old, modules, externs, macros, attrs)
                new_next = new | consumer_macro(new, crates, modules, externs, macros, attrs)
                self.assertEqual(old_next, new_next)
                if old_next == old and new_next == new:
                    break
                old, new = old_next, new_next

    def test_exact_reversible_consumer_patch_keeps_both_middle_variants(self):
        data, stages = official_fixture()
        path_sha, stage_sha = hashlib.sha256(data).hexdigest(), hashlib.sha256(stages).hexdigest()
        result, stage = candidate.consumer_sources(data, stages, path_sha, stage_sha)
        self.assertIn(b"t1 = i2.getASuccessorNonCrate(_, _, _)", result)
        self.assertIn(b"imported = m.(ExternCrateItemNode).getASuccessorNonCrate(_, _, _)", result)
        self.assertIn(b"imported = m.(ExternCrateItemNode).getASuccessorCrate(_, _, _)", result)
        restored = result
        for old, new in reversed(candidate.CONSUMER_PATCHES):
            self.assertEqual(restored.count(new), 1)
            restored = restored.replace(new, old, 1)
        partition, expected_stage = candidate.partition_sources(data, stages, path_sha, stage_sha)
        self.assertEqual(restored, partition)
        self.assertEqual(stage, expected_stage)
        with self.assertRaises(RuntimeError):
            candidate.consumer_sources(data + b"unknown", stages, path_sha, stage_sha)


if __name__ == "__main__":
    unittest.main()
