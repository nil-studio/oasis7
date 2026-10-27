#!/usr/bin/env python3
"""Finite-graph equivalence checks, not Rust semantic/performance acceptance."""
import importlib.util
import hashlib
import itertools
from pathlib import Path
import random
import json
import tempfile
import time
import sys
from types import SimpleNamespace
import unittest

from test_codeql_rust_successor_partition import official_fixture
from unittest.mock import patch

P = Path(__file__).with_name("codeql_rust_trait_scope_plan.py")
spec = importlib.util.spec_from_file_location("scope_plan", P)
m = importlib.util.module_from_spec(spec)
spec.loader.exec_module(m)


def reachable(graph, seed):
    seen = {seed}
    todo = [seed]
    while todo:
        for nxt in graph.get(todo.pop(), ()):
            if nxt not in seen:
                seen.add(nxt)
                todo.append(nxt)
    return seen


def visibility(scopes, imports, outer, owners, relevant):
    graph = {}
    def edge(a, b):
        graph.setdefault(a, set()).add(b)
    traits = {t for e, t in relevant}
    elements = {e for e, t in relevant}
    for t, i in imports:
        if t in traits and i in scopes:
            edge(("trait", t), ("item", i))
    for parent, inner in outer:
        if parent in scopes and inner in scopes:
            edge(("item", parent), ("item", inner))
    for i, e in owners:
        if i in scopes and e in elements:
            edge(("item", i), ("element", e))
    return {(e, t) for e, t in relevant if ("element", e) in reachable(graph, ("trait", t))}


def live_scopes(outer, owners, relevant):
    elements = {e for e, t in relevant}
    live = {i for i, e in owners if e in elements}
    while True:
        new = live | {parent for parent, inner in outer if inner in live}
        if new == live:
            return live
        live = new


class ScopePlan(unittest.TestCase):
    def test_root_jump_and_branching_cycle_keep_witnesses_but_drop_dead_scope(self):
        scopes = set(range(5))
        outer = {(0, 1), (1, 2), (2, 1), (3, 2)}
        owners = {(2, 0)}
        imports = {(0, 0), (1, 3), (2, 4)}
        relevant = {(0, 0), (0, 1), (0, 2)}
        expected = {(0, 0), (0, 1)}
        self.assertEqual(visibility(scopes, imports, outer, owners, relevant), expected)
        live = live_scopes(outer, owners, relevant)
        self.assertEqual(live, {0, 1, 2, 3})
        self.assertEqual(visibility(live, imports, outer, owners, relevant), expected)
        self.assertNotEqual(visibility({2}, imports, outer, owners, relevant), expected)

    def test_exhaustive_two_scopes_including_cycles_and_no_relevant_pairs(self):
        def subsets(values):
            values = list(values)
            for bits in range(1 << len(values)):
                yield {v for i, v in enumerate(values) if bits & (1 << i)}
        cases = 0
        for outer, imports, owners, relevant in itertools.product(
            subsets(itertools.product(range(2), repeat=2)),
            subsets([(0, 0), (0, 1)]),
            subsets([(0, 0), (1, 0)]),
            [set(), {(0, 0)}],
        ):
            self.assertEqual(visibility({0, 1}, imports, outer, owners, relevant),
                visibility(live_scopes(outer, owners, relevant), imports, outer, owners, relevant))
            cases += 1
        self.assertEqual(cases, 512)

    def test_multiple_traits_elements_owners_and_disconnected_scopes(self):
        rng = random.Random(22876)
        for _ in range(4096):
            outer = {(p, i) for p in range(6) for i in range(6) if rng.random() < .2}
            imports = {(t, i) for t in range(3) for i in range(6) if rng.random() < .3}
            owners = {(i, e) for i in range(6) for e in range(4) if rng.random() < .2}
            relevant = {(e, t) for e in range(4) for t in range(3) if rng.random() < .3}
            self.assertEqual(visibility(set(range(6)), imports, outer, owners, relevant),
                visibility(live_scopes(outer, owners, relevant), imports, outer, owners, relevant))

    def test_exact_patch_and_unknown_source_fail_closed(self):
        source = b"fixture-prefix\n" + m.OLD + b"fixture-suffix\n"
        with patch.object(m, "PATH_SHA", hashlib.sha256(source).hexdigest()):
            candidate = m.patched_source(source)
            self.assertIn(b"TItemNode(ItemNode i) { relevantItemNode(i) }", candidate)
            self.assertEqual(candidate.replace(m.NEW, m.OLD, 1), source)
            for invalid in (source + b"\n", candidate):
                with self.assertRaises(RuntimeError):
                    m.patched_source(invalid)


    def test_compile_only_commands_and_failure_stops_before_candidate(self):
        for probe, fail in itertools.product(m.COMPILE_PROBES, (False, True)):
            with self.subTest(probe=probe, fail=fail), tempfile.TemporaryDirectory() as tmp:
                root = Path(tmp)
                official, out, work = root / "official", root / "out", root / "work"
                (official / m.PATH).parent.mkdir(parents=True)
                source, stages = official_fixture()
                source = b"prefix\n" + m.OLD + source + b"suffix\n"
                (official / m.PATH).write_bytes(source)
                (official / m.STAGES).write_bytes(stages)
                (official / "qlpack.yml").write_text("official fixture")
                out.mkdir()
                work.mkdir()
                args = SimpleNamespace(probe=probe, bundle_version="2.27.3", output=out, work=work,
                    codeql=root / "codeql", deadline=time.monotonic() + 600)
                commands, ends, verified = [], [], []
                def run(argv, cwd, output, end, timeout, output_root, env):
                    commands.append(argv)
                    self.assertEqual(output_root, out)
                    self.assertEqual(cwd, work)
                    if argv[1:4] == ["query", "compile", "--help"]:
                        output.write_text("--dump-ra")
                    elif argv[1:3] == ["resolve", "library-path"]:
                        output.write_text("{}")
                    elif argv[1:3] == ["query", "compile"]:
                        self.assertIn("--dump-ra", argv)
                        self.assertIn("--threads=2", argv)
                        self.assertIn("--ram=3072", argv)
                        self.assertIn("--no-default-compilation-cache", argv)
                        ends.append(end)
                        if fail and len(ends) == 2:
                            raise RuntimeError("baseline compile failed")
                        output.write_text("complete RA fixture")
                    else:
                        self.fail("unexpected command (no database/extraction/evaluation permitted): " + str(argv))
                def verify(value, query, name, selected, pack_name):
                    self.assertEqual(pack_name, m.PACK)
                    self.assertEqual((query / (name + ".ql")).read_text(), m.QUERIES[name])
                    verified.append((query.name, name, selected["codeql/rust-all"]["root"]))
                with patch.object(m, "PATH_SHA", hashlib.sha256(source).hexdigest()), \
                        patch.object(m, "STAGES_SHA", hashlib.sha256(stages).hexdigest()), \
                        patch.object(m.runtime, "run", side_effect=run):
                    if fail:
                        with self.assertRaisesRegex(RuntimeError, "baseline compile failed"):
                            m.execute(args, {}, {"codeql/rust-all": {"root": str(official)}}, verify)
                    else:
                        m.execute(args, {}, {"codeql/rust-all": {"root": str(official)}}, verify)
                self.assertEqual(len(ends), 2 if fail else 4)
                self.assertEqual(len(set(ends)), 1, "all variants/queries share one compiler deadline")
                self.assertEqual((official / m.PATH).read_bytes(), source)
                self.assertEqual((official / m.STAGES).read_bytes(), stages)
                changed = json.loads((out / "isolated-source.json").read_text())["changed_files"]
                self.assertEqual(set(changed), {m.PATH, m.STAGES} if probe == "successor-partition-plan" else {m.PATH})
                self.assertEqual(json.loads((out / "official-after.json").read_text()),
                    json.loads((out / "official-before.json").read_text()))
                if fail:
                    self.assertTrue(all(label == "original-query" for label, _, _ in verified))
                    self.assertFalse((out / "compile-plan-terminal.json").exists())
                else:
                    self.assertEqual([name for _, name, _ in verified], ["Flow", "Callee", "Flow", "Callee"])
                    terminal = json.loads((out / "compile-plan-terminal.json").read_text())
                    self.assertFalse(terminal["runtime_go"])
                    self.assertEqual(terminal["database_operations"], 0)

    def test_wrong_bundle_never_spawns(self):
        with patch.object(m.runtime, "run") as run:
            with self.assertRaisesRegex(RuntimeError, "official 2.27.3"):
                m.execute(SimpleNamespace(bundle_version="2.27.2"), {}, {}, None)
            run.assert_not_called()

    def test_unknown_candidate_never_spawns(self):
        with patch.object(m.runtime, "run") as run:
            with self.assertRaisesRegex(RuntimeError, "unknown compile-only"):
                m.execute(SimpleNamespace(bundle_version="2.27.3", probe="unknown"), {}, {}, None)
            run.assert_not_called()

    def test_manual_probe_routes_to_compile_only_without_business_input(self):
        spec = importlib.util.spec_from_file_location("diagnostic_entry", P.with_name("codeql-rust-model-diagnostic.py"))
        entry = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(entry)
        for probe in m.COMPILE_PROBES:
            with self.subTest(probe=probe), tempfile.TemporaryDirectory() as tmp:
                root = Path(tmp)
                argv = ["diagnostic", "--codeql", str(root / "missing-cli"), "--bundle-version", "2.27.3",
                    "--business-source", str(root / "missing-business"), "--probe", probe,
                    "--output", str(root / "out"), "--work", str(root / "work"), "--deadline", str(time.time() + 600)]
                def version(argv, cwd, output, *args, **kwargs):
                    self.assertEqual(argv[1:], ["version", "--format=json"])
                    output.write_text(json.dumps({"version": "2.27.3"}))
                with patch.object(sys, "argv", argv), patch.dict(sys.modules, {"codeql_rust_trait_scope_plan": m}), \
                    patch.object(entry, "bundle_closure", return_value={}), \
                    patch.object(entry.runtime, "run", side_effect=version) as run, patch.object(m, "execute") as execute:
                    entry.main()
                self.assertEqual(run.call_count, 1)
                execute.assert_called_once()
                self.assertEqual(execute.call_args.args[0].probe, probe)
                terminal = json.loads((root / "out/terminal.json").read_text())
                self.assertEqual(terminal["status"], "COMPILE_ONLY_REQUIRES_PLAN_REVIEW")
                self.assertFalse((root / "missing-business").exists())

    def test_workflow_selection_keeps_manual_gate_and_existing_budget(self):
        root = P.parents[2]
        text = (root / ".github/workflows/rust.yml").read_text()
        job = text.split("  codeql-rust-model-diagnostic:\n", 1)[1].split("\n  select:", 1)[0]
        for probe in m.COMPILE_PROBES:
            self.assertIn(probe, text.split("permissions:", 1)[0])
        self.assertIn("MODEL_PROBE: ${{ inputs.model_probe }}", job)
        self.assertIn('trait-scope-plan|successor-partition-plan) test "$BUNDLE_VERSION" = 2.27.3 ;;', job)
        self.assertIn('trait-scope-plan|successor-partition-plan) ;;', job)
        self.assertIn("timeout-minutes: 20", job)
        self.assertIn(" + 1080 ", job)
        self.assertIn("64 * 1024**2", job)
        self.assertIn("permissions:\n      contents: read", job)
        self.assertIn("run python3 ./scripts/security/" + P.with_name("test_codeql_rust_trait_scope_plan.py").name,
            (root / "scripts/ci-tests.sh").read_text())


if __name__ == "__main__":
    unittest.main()
