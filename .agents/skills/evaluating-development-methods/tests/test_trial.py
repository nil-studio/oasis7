"""Offline contracts for the optional method-trial tool; synthetic records only."""
import copy
import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location("method_trial", ROOT / "scripts" / "trial.py")
trial = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(trial)


class TrialContracts(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.suite = {"schema_version": 1, "cases": [{
            "id": "scope", "split": "pilot", "family": "ci_scope", "prompt": "Select checks.",
            "input": {"changed": ["doc/example.md"]},
            "answer_format": {"groups": "set of strings", "formal": "boolean", "minutes": "number"},
            "expected": {"groups": ["baseline", "docs"], "formal": False, "minutes": 6},
            "provenance": {"kind": "contract_fixture", "source": "synthetic-test-only"},
        }]}
        self.plan = trial.make_plan(self.suite, "pilot", 1)
        self.write("plan.json", self.plan)

    def write(self, name, value):
        path = self.root / name
        path.write_text(json.dumps(value), encoding="utf-8")
        return path

    def results(self):
        return {"schema_version": 1, "plan_sha256": self.plan["plan_sha256"],
                "method_ref": "synthetic-unit-test-only",
                "context": {"model": "fake", "model_settings": {}, "tools": "fixture",
                            "environment": "unit-test", "memory_snapshot": "empty", "cache_state": "not-applicable",
                            "budget": {"max_seconds": 60, "max_tokens": 1000}},
                "runs": [{"case_id": "scope", "attempt": 1, "status": "completed",
                          "answer": copy.deepcopy(self.suite["cases"][0]["expected"]),
                          "wall_seconds": 10, "model_tokens": 100, "runner_seconds": 0,
                          "cost_usd": 0, "human_interventions": 0, "evidence": "synthetic-unit-test-only"}]}

    def compare(self, baseline=None, candidate=None):
        return trial.compare(self.suite, self.root / "plan.json",
                             self.write("baseline.json", self.results() if baseline is None else baseline),
                             self.write("candidate.json", self.results() if candidate is None else candidate))

    def test_export_has_no_answers_or_provenance_and_refuses_overwrite(self):
        output = self.root / "export"
        result = trial.prepare(self.suite, "pilot", 2, output)
        self.assertFalse(result["executed"])
        self.assertEqual(result["planned_attempts_per_method"], 2)
        exported = trial.read_json(output / "tasks" / "scope.json")
        self.assertEqual(set(exported), {"id", "family", "prompt", "input", "answer_format"})
        self.assertNotIn("expected", exported)
        self.assertNotIn("provenance", exported)
        self.assertEqual(trial.read_json(output / "results.template.json")["runs"], [])
        original = (output / "plan.json").read_bytes()
        with self.assertRaises(FileExistsError):
            trial.prepare(self.suite, "pilot", 1, output)
        self.assertEqual((output / "plan.json").read_bytes(), original)

    def test_historical_suite_or_plan_changes_are_rejected(self):
        self.write("baseline.json", self.results())
        self.write("candidate.json", self.results())
        self.suite["cases"][0]["expected"]["minutes"] = 7
        with self.assertRaisesRegex(ValueError, "fingerprint changed"):
            trial.compare(self.suite, self.root / "plan.json", self.root / "baseline.json", self.root / "candidate.json")
        self.suite["cases"][0]["expected"]["minutes"] = 6
        self.plan["schema_version"] = True
        self.write("plan.json", self.plan)
        with self.assertRaisesRegex(ValueError, "fingerprint changed"):
            self.compare()

    def test_missing_attempts_stay_in_denominator_and_cost_is_incomplete(self):
        self.plan = trial.make_plan(self.suite, "pilot", 2)
        self.write("plan.json", self.plan)
        report = self.compare()
        self.assertEqual(report["planned_pairs"], 2)
        self.assertEqual(report["outcomes"], {"win": 0, "loss": 0, "tie": 1, "neither_passed": 0, "inconclusive": 1})
        self.assertEqual(report["coverage"]["baseline"]["missing"], 1)
        self.assertEqual(report["metrics"]["baseline"]["wall_seconds"]["known_sum"], 10)
        self.assertIsNone(report["metrics"]["baseline"]["wall_seconds"]["total"])
        self.assertIsNone(report["metrics"]["candidate_minus_baseline"]["wall_seconds"])

    def test_regression_is_reported_and_never_adopted(self):
        candidate = self.results()
        candidate["runs"][0]["answer"]["formal"] = True
        report = self.compare(candidate=candidate)
        self.assertEqual(report["outcomes"]["loss"], 1)
        self.assertEqual(report["pairs"][0]["candidate"]["failed_fields"], ["formal"])
        self.assertEqual(report["adoption"], "not_decided")
        self.assertFalse(report["evidence_verified"])
        self.assertEqual(report["evaluation_scope"], "contract_fixture")

    def test_correct_answer_does_not_turn_failure_into_success(self):
        for status in ("failed", "timeout", "cancelled"):
            with self.subTest(status=status):
                candidate = self.results()
                candidate["runs"][0]["status"] = status
                report = self.compare(candidate=candidate)
                self.assertEqual(report["outcomes"]["loss"], 1)
                self.assertEqual(report["coverage"]["candidate"][status], 1)
                self.assertEqual(report["metrics"]["candidate"]["wall_seconds"]["total"], 10)
                both = self.compare(candidate, candidate)
                self.assertEqual(both["outcomes"]["neither_passed"], 1)
                self.assertEqual(both["outcomes"]["tie"], 0)

    def test_sets_ignore_order_but_preserve_type_and_duplicates(self):
        candidate = self.results()
        candidate["runs"][0]["answer"]["groups"].reverse()
        candidate["runs"][0]["answer"]["explanation"] = "Additional prose is reviewed separately."
        self.assertEqual(self.compare(candidate=candidate)["outcomes"]["tie"], 1)
        for field, value in (("formal", 0), ("minutes", True), ("groups", ["baseline", "docs", "docs"])):
            with self.subTest(field=field):
                candidate = self.results()
                candidate["runs"][0]["answer"][field] = value
                self.assertEqual(self.compare(candidate=candidate)["outcomes"]["loss"], 1)

    def test_budget_overrun_and_unknown_are_not_fair_wins(self):
        baseline = self.results()
        baseline["runs"][0]["status"] = "failed"
        for seconds, budget_status in ((61, "exceeded"), (None, "unknown")):
            with self.subTest(seconds=seconds):
                candidate = self.results()
                candidate["runs"][0]["wall_seconds"] = seconds
                report = self.compare(baseline, candidate)
                self.assertEqual(report["fixture_passes"]["candidate"], 1)
                self.assertEqual(report["outcomes"]["win"], 0)
                self.assertEqual(report["outcomes"]["inconclusive"], 1)
                self.assertEqual(report["budget"]["candidate"][budget_status], 1)
                self.assertEqual(report["both_pass_timing"]["pairs"], 0)

    def test_null_cost_is_unknown_and_zero_is_known(self):
        candidate = self.results()
        candidate["runs"][0]["cost_usd"] = None
        candidate["runs"][0]["wall_seconds"] = 7
        report = self.compare(candidate=candidate)
        self.assertEqual(report["metrics"]["baseline"]["cost_usd"]["total"], 0)
        self.assertIsNone(report["metrics"]["candidate"]["cost_usd"]["total"])
        self.assertIsNone(report["metrics"]["candidate_minus_baseline"]["cost_usd"])
        self.assertEqual(report["both_pass_timing"]["median_candidate_minus_baseline_seconds"], -3)

    def test_null_expected_answer_is_distinct_from_missing_and_text(self):
        self.suite["cases"][0]["expected"]["minutes"] = None
        self.plan = trial.make_plan(self.suite, "pilot", 1)
        self.write("plan.json", self.plan)
        self.assertEqual(self.compare()["outcomes"]["tie"], 1)
        candidate = self.results()
        del candidate["runs"][0]["answer"]["minutes"]
        self.assertEqual(self.compare(candidate=candidate)["outcomes"]["loss"], 1)
        candidate["runs"][0]["answer"]["minutes"] = "null"
        self.assertEqual(self.compare(candidate=candidate)["outcomes"]["loss"], 1)

    def test_unknown_duplicate_attempts_and_changed_context_are_rejected(self):
        mutations = [lambda r: r["runs"].append(copy.deepcopy(r["runs"][0])),
                     lambda r: r["runs"][0].update(case_id="unplanned"),
                     lambda r: r["runs"][0].update(attempt=2),
                     lambda r: r.update(plan_sha256="wrong"),
                     lambda r: r["context"].update(cache_state="warm"),
                     lambda r: r["context"].update(memory_snapshot=None)]
        for mutate in mutations:
            with self.subTest(mutation=mutate):
                candidate = self.results()
                mutate(candidate)
                with self.assertRaises(ValueError):
                    self.compare(candidate=candidate)

    def test_invalid_numeric_records_are_rejected(self):
        for field, value in (("attempt", True), ("attempt", 1.5), ("wall_seconds", -1),
                             ("model_tokens", True), ("human_interventions", 0.5),
                             ("cost_usd", float("nan")), ("runner_seconds", float("inf"))):
            with self.subTest(field=field, value=value):
                candidate = self.results()
                candidate["runs"][0][field] = value
                with self.assertRaises(ValueError):
                    self.compare(candidate=candidate)
        for value in (True, 0, -1, 1.5):
            with self.assertRaises(ValueError):
                trial.make_plan(self.suite, "pilot", value)

    def test_duplicate_keys_and_nonfinite_literals_are_rejected(self):
        for source in ('{"status":"failed","status":"completed"}', '{"x":NaN}', '{"x":1e999}'):
            path = self.root / "invalid.json"
            path.write_text(source, encoding="utf-8")
            with self.assertRaises(ValueError):
                trial.read_json(path)

    def test_cli_exports_shipped_public_fixtures_without_starting_an_experiment(self):
        result = subprocess.run([sys.executable, str(ROOT / "scripts" / "trial.py"), "prepare",
                                 "--split", "all", "--out", str(self.root / "shipped")],
                                text=True, capture_output=True, check=True)
        summary = json.loads(result.stdout)
        self.assertFalse(summary["executed"])
        self.assertEqual(summary["tasks"], 8)
        exported = list((self.root / "shipped" / "tasks").glob("*.json"))
        self.assertEqual(len(exported), 8)
        for path in exported:
            self.assertNotIn("expected", trial.read_json(path))
        invalid = subprocess.run([sys.executable, str(ROOT / "scripts" / "trial.py"), "compare",
                                  "--plan", str(self.root / "shipped" / "plan.json"),
                                  "--baseline", str(self.root / "shipped" / "results.template.json"),
                                  "--candidate", str(self.root / "shipped" / "results.template.json")],
                                 text=True, capture_output=True)
        self.assertEqual(invalid.returncode, 2)
        self.assertEqual(invalid.stdout, "")
        suite = trial.load_suite(trial.CASES)
        plan = trial.read_json(self.root / "shipped" / "plan.json")
        synthetic = self.results()
        synthetic["plan_sha256"] = plan["plan_sha256"]
        row = synthetic["runs"][0]
        synthetic["runs"] = [{**row, "case_id": case["id"], "answer": case["expected"]}
                             for case in suite["cases"]]
        records = self.write("synthetic-cli-records.json", synthetic)
        compared = subprocess.run([sys.executable, str(ROOT / "scripts" / "trial.py"), "compare",
                                   "--plan", str(self.root / "shipped" / "plan.json"),
                                   "--baseline", str(records), "--candidate", str(records)],
                                  text=True, capture_output=True, check=True)
        report = json.loads(compared.stdout)
        self.assertEqual(report["outcomes"]["tie"], 8)
        self.assertEqual(report["planned_pairs"], 8)
        self.assertFalse(report["evidence_verified"])
        self.assertEqual(report["adoption"], "not_decided")


if __name__ == "__main__":
    unittest.main()
