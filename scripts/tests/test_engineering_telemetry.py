import copy
import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts"))
spec = importlib.util.spec_from_file_location("engineering_telemetry", ROOT / "scripts/engineering_telemetry.py")
t = importlib.util.module_from_spec(spec)
spec.loader.exec_module(t)
FIXTURE = ROOT / "scripts/tests/fixtures/engineering-telemetry/input.json"

LEGACY_INPUT = {'collected_at': '2026-01-01T00:00:00Z',
 'collection_id': 'fixture-001',
 'origin': 'github',
 'schema_version': 'oasis7-engineering-telemetry-input/v1',
 'sources': [{'kind': 'actions',
              'records': [{'coverage': 'partial',
                           'execution_result': 'success',
                           'id': 'select',
                           'job_id': 10,
                           'kind': 'job',
                           'missing_reasons': ['fixture-partial'],
                           'repository': 'nil-studio/oasis7',
                           'run_attempt': 1,
                           'run_id': 1},
                          {'coverage': 'partial',
                           'depends_on': ['select'],
                           'execution_result': 'failure',
                           'id': 'build',
                           'job_id': 11,
                           'kind': 'job',
                           'missing_reasons': ['fixture-partial'],
                           'repository': 'nil-studio/oasis7',
                           'run_attempt': 1,
                           'run_id': 1}],
              'reference': 'fixture/actions.json',
              'source_schema': 'oasis7-engineering-telemetry-input/v1'}]}

class TelemetryTests(unittest.TestCase):
    def setUp(self):
        self.input = copy.deepcopy(LEGACY_INPUT)
        self.input["sources"][0]["records"][0].pop("field_sources", None)
        self.input["sources"][0]["records"][0].pop("missing_reasons")

    def test_cli_fixed_snapshot_and_report(self):
        with tempfile.TemporaryDirectory() as d:
            d = Path(d)
            script = ROOT / "scripts/engineering_telemetry.py"
            (d / "input.json").write_text(json.dumps(self.input))
            proc = subprocess.run([sys.executable, str(script), "collect", str(d / "input.json"), str(d / "snapshot.json")])
            self.assertEqual(proc.returncode, 1)
            subprocess.run([sys.executable, str(script), "report", str(d / "snapshot.json"), str(d / "report.json"), str(d / "report.md")], check=True)
            result = json.loads((d / "report.json").read_text())
            self.assertEqual(result["schema_version"], t.REPORT)
            self.assertEqual(result["analysis"]["status"], "unavailable")
            self.assertEqual({r["execution_result"] for r in result["records"]}, {"success", "failure"})
            self.assertIn("unavailable", (d / "report.md").read_text())

    def test_raw_source_to_report(self):
        with tempfile.TemporaryDirectory() as d:
            d = Path(d)
            source = d / "raw.json"
            source.write_text('{"jobs":[{"id":4,"started_at":"2026-01-01T00:00:00Z","completed_at":"2026-01-01T00:01:00Z","conclusion":"failure"}]}')
            script = ROOT / "scripts/engineering_telemetry.py"
            proc = subprocess.run([sys.executable, str(script), "collect-source", "actions-jobs", str(source), str(d / "snapshot.json"), "--reference", "fixture/actions.json", "--collection-id", "fixture", "--collected-at", "2026-01-01T00:00:00Z", "--origin", "github"])
            self.assertEqual(proc.returncode, 1)
            snap = json.loads((d / "snapshot.json").read_text())
            self.assertEqual(snap["records"][0]["execution_result"], "failure")
            self.assertIn("pagination, workflow dependencies and run identity not verified", snap["missing_reasons"])
            import hashlib
            self.assertEqual(snap["evidence"][0]["raw_sha256"], hashlib.sha256(source.read_bytes()).hexdigest())
            subprocess.run([sys.executable, str(script), "report", str(d / "snapshot.json"), str(d / "report.json"), str(d / "report.md")], check=True)

    def test_output_aliases_do_not_clobber_inputs(self):
        with tempfile.TemporaryDirectory() as d:
            d = Path(d)
            source = d / "source.json"
            source.write_text(json.dumps(self.input))
            script = ROOT / "scripts/engineering_telemetry.py"
            before = source.read_bytes()
            proc = subprocess.run([sys.executable, str(script), "collect", str(source), str(source)], capture_output=True)
            self.assertEqual(proc.returncode, 1)
            self.assertEqual(source.read_bytes(), before)
            snapshot = d / "snapshot.json"
            subprocess.run([sys.executable, str(script), "collect", str(source), str(snapshot)])
            original = snapshot.read_bytes()
            alias = d / "alias"; alias.hardlink_to(snapshot)
            proc = subprocess.run([sys.executable, str(script), "report", str(snapshot), str(alias), str(d / "report.md")], capture_output=True)
            self.assertEqual(proc.returncode, 1)
            self.assertEqual(snapshot.read_bytes(), original)
            output = d / "report"
            proc = subprocess.run([sys.executable, str(script), "report", str(snapshot), str(output), str(output)], capture_output=True)
            self.assertEqual(proc.returncode, 1)
            self.assertFalse(output.exists())

    def test_duplicate_and_conflict_preserve_attempts(self):
        s = self.input["sources"][0]
        duplicate = copy.deepcopy(s); duplicate["reference"] = "fixture/duplicate.json"
        self.input["sources"].append(duplicate)
        result = t.collect(self.input)
        self.assertEqual(len(result["records"]), 2)
        self.assertEqual(len(result["records"][0]["evidence"]), 2)
        duplicate["records"][0]["execution_result"] = "failure"
        result = t.collect(self.input)
        self.assertEqual(result["collection_status"], "partial")
        self.assertEqual(sum(r["identity_conflict"] for r in result["records"]), 2)
        duplicate["records"][0]["run_attempt"] = 2
        result = t.collect(self.input)
        self.assertNotIn("identity conflict", result["missing_reasons"])

    def test_job_alias_cannot_evade_identity_conflict(self):
        source = self.input["sources"][0]
        other = copy.deepcopy(source)
        other["reference"] = "fixture/other.json"
        other["records"][0]["id"] = "different-alias"
        other["records"][0]["execution_result"] = "failure"
        self.input["sources"].append(other)
        result = t.collect(self.input)
        conflicts = [record for record in result["records"] if record["identity_conflict"]]
        self.assertEqual(len(conflicts), 2)
        self.assertEqual({r["execution_result"] for r in conflicts}, {"success", "failure"})
        self.assertIn("identity conflict", result["missing_reasons"])

    def test_job_aliases_same_values_deduplicate_with_aliases(self):
        other = copy.deepcopy(self.input["sources"][0])
        other["reference"] = "fixture/other.json"
        other["records"][0]["id"] = "different-alias"
        self.input["sources"].append(other)
        result = t.collect(self.input)
        self.assertEqual(len(result["records"]), 2)
        record = next(r for r in result["records"] if r["job_id"] == 10)
        self.assertEqual(record["aliases"], ["different-alias", "select"])
        self.assertFalse(record["identity_conflict"])
        t.report(result)

    def test_job_step_number_is_rejected(self):
        other = copy.deepcopy(self.input["sources"][0])
        other["reference"] = "fixture/other.json"
        other["records"][0]["step_number"] = 2
        other["records"][0]["execution_result"] = "failure"
        self.input["sources"].append(other)
        result = t.collect(self.input)
        self.assertEqual(len([r for r in result["records"] if r.get("job_id") == 10]), 1)
        self.assertIn("invalid or unsafe record: fixture/other.json", result["missing_reasons"])

    def test_report_rejects_orphan_or_empty_evidence(self):
        result = t.collect(self.input)
        for references in ([], ["fixture/missing.json"]):
            bad = copy.deepcopy(result)
            bad["records"][0]["evidence"] = references
            with self.assertRaises(ValueError): t.report(bad)

    def test_unverified_identities_are_not_deduplicated(self):
        source = self.input["sources"][0]
        for record in source["records"]:
            record.pop("repository")
        duplicate = copy.deepcopy(source)
        duplicate["reference"] = "fixture/duplicate.json"
        self.input["sources"].append(duplicate)
        result = t.collect(self.input)
        self.assertEqual(len(result["records"]), 4)
        self.assertFalse(any(record["identity_conflict"] for record in result["records"]))

    def test_conflicting_evidence_reference_is_rejected(self):
        other = copy.deepcopy(self.input["sources"][0])
        other["records"][0]["execution_result"] = "failure"
        self.input["sources"].append(other)
        result = t.collect(self.input)
        self.assertEqual(result["records"], [])
        self.assertEqual(result["evidence"], [])
        self.assertIn("conflicting evidence reference", result["missing_reasons"])
        t.report(result)

    def test_unknown_schema_and_unapproved_fields_are_partial(self):
        self.input["sources"][0]["records"][0]["secret"] = "do not copy"
        result = t.collect(self.input)
        self.assertEqual(result["collection_status"], "partial")
        self.assertNotIn("do not copy", json.dumps(result))
        self.input["sources"][0]["source_schema"] = "future"
        result = t.collect(self.input)
        self.assertEqual(result["records"], [])
        self.assertTrue(any("unsupported source schema" in x for x in result["missing_reasons"]))
        with self.assertRaises(ValueError): t.report({"schema_version":"future"})

    def test_invalid_reference_does_not_escape(self):
        self.input["sources"][0]["reference"] = "../private"
        self.assertEqual(t.collect(self.input)["records"], [])

    def test_nested_and_path_data_are_rejected(self):
        for field, value in (("field_sources", {"token": "secret"}),
                             ("scope", "/home/user/private"),
                             ("runner", {"private": "data"})):
            data = copy.deepcopy(self.input)
            data["sources"][0]["records"][0][field] = value
            result = t.collect(data)
            self.assertEqual(len(result["records"]), 1)
            self.assertNotIn("private", json.dumps(result))

    def test_snapshot_rejects_injected_data(self):
        snapshot = t.collect(self.input)
        snapshot["records"][0]["runner"] = {"secret": "value"}
        with self.assertRaises((ValueError, TypeError)):
            t.report(snapshot)

    def test_input_budget_and_partial_status(self):
        snapshot = t.collect(self.input)
        self.assertEqual(snapshot["collection_status"], "partial")
        with tempfile.TemporaryDirectory() as d:
            path = Path(d) / "large.json"
            with path.open("wb") as f:
                f.truncate(t.MAX_INPUT_BYTES + 1)
            with self.assertRaisesRegex(ValueError, "budget"):
                t.read_json(path)



class ProvenanceDagTests(unittest.TestCase):
    METRICS = {
        "feedback_wall_seconds": 21,
        "job_execution_seconds": 25,
        "execution_longest_chain_seconds": 19,
        "unattributed_wait_seconds": 2,
        "observed_wall_chain_seconds": 21,
        "first_job_start_delay_seconds": 0,
    }

    def setUp(self):
        self.input = json.loads(FIXTURE.read_text())

    def refresh(self):
        """Update synthetic evidence together with deliberately changed facts."""
        payload = {}
        for group in ("contexts", "records"):
            payload[group] = []
            for index, item in enumerate(self.input[group]):
                facts = {k: copy.deepcopy(v) for k, v in item.items() if k != "field_sources"}
                payload[group].append(facts)
                old = item.get("field_sources", {})
                item["field_sources"] = {
                    key: ({"state": "known", "reference": "fixture/facts.json",
                           "pointer": f"/{group}/{index}/{key}"} if value is not None else
                          copy.deepcopy(old[key]) if old.get(key, {}).get("state") in ("unknown", "not_applicable") else
                          {"state": "unknown", "reason": "missing-fact"})
                    for key, value in facts.items() if key not in ("id", "context_id")
                }
        self.input["sources"][0]["payload"] = payload

    def result(self):
        return t.report(t.collect(self.input))

    def context(self):
        return self.result()["analysis"]["contexts"][0]

    def assert_unavailable(self):
        result = self.result()
        self.assertNotEqual(result["analysis"]["status"], "available")
        for context in result["analysis"].get("contexts", []):
            self.assertEqual(context["status"], "unavailable")
            self.assertTrue(context["missing_reasons"])
            for name in self.METRICS:
                self.assertEqual(context["metrics"][name]["status"], "unavailable")
                self.assertIsNone(context["metrics"][name]["value"])
        return result

    def test_fixed_parallel_dag_and_traceable_formulas(self):
        snapshot = t.collect(self.input)
        self.assertEqual(snapshot["collection_status"], "complete")
        self.assertNotIn('"payload"', json.dumps(snapshot))
        self.assertEqual(snapshot["sample_kind"], "synthetic")
        result = t.report(snapshot)
        self.assertEqual(result["analysis"]["status"], "available")
        self.assertEqual(result["analysis"]["formula_version"], "oasis7-engineering-dag/v1")
        context = result["analysis"]["contexts"][0]
        for name, expected in self.METRICS.items():
            with self.subTest(metric=name):
                metric = context["metrics"][name]
                self.assertEqual(metric["status"], "available")
                self.assertEqual(metric["value"], expected)
                self.assertEqual(metric["unit"], "seconds")
                self.assertTrue(metric["formula"])
                self.assertTrue(metric["input_ids"])
        shard = next(j for j in context["job_timings"] if j["id"] == "shard-b")
        self.assertEqual(shard["unattributed_wait_seconds"], 2)
        self.assertEqual(shard["execution_seconds"], 8)
        for record in snapshot["records"]:
            self.assertIsInstance(record["field_sources"]["started_at"], list)

    def test_evidence_digest_changes_with_facts_without_exposing_payload(self):
        first = t.collect(self.input)
        self.input["records"][3]["execution_result"] = "failure"
        self.refresh()
        second = t.collect(self.input)
        self.assertNotEqual(first["evidence"], second["evidence"])
        self.assertNotIn("payload", second["evidence"][0])

    def test_unknown_policy_is_not_inferred_from_source(self):
        self.input["contexts"][0]["policy_sha"] = None
        for record in self.input["records"]:
            record["policy_sha"] = None
        self.refresh()
        snapshot = t.collect(self.input)
        self.assertEqual(snapshot["collection_status"], "partial")
        self.assertTrue(all(record["policy_sha"] is None for record in snapshot["records"]))
        self.assert_unavailable()

    def test_cli_v2_reproduces_fixed_metrics(self):
        with tempfile.TemporaryDirectory() as directory:
            directory = Path(directory)
            script = ROOT / "scripts/engineering_telemetry.py"
            snapshot = directory / "snapshot.json"
            subprocess.run([sys.executable, str(script), "collect", str(FIXTURE), str(snapshot)], check=True)
            subprocess.run([sys.executable, str(script), "report", str(snapshot),
                            str(directory / "report.json"), str(directory / "report.md")], check=True)
            result = json.loads((directory / "report.json").read_text())
            self.assertEqual(result["analysis"]["contexts"][0]["metrics"]["job_execution_seconds"]["value"], 25)

    def test_removed_slow_shard_dependency_changes_fixed_answer(self):
        self.input["records"][4]["depends_on"] = ["shard-a"]
        self.refresh()
        context = self.context()
        self.assertNotEqual(context["metrics"]["execution_longest_chain_seconds"]["value"], 19)

    def test_cycle_missing_job_and_incomplete_coverage_fail_closed(self):
        original = copy.deepcopy(self.input)
        for defect in ("cycle", "job", "pagination_complete", "dependencies_complete", "conditions_complete"):
            with self.subTest(defect=defect):
                self.input = copy.deepcopy(original)
                if defect == "cycle":
                    self.input["records"][0]["depends_on"] = ["gate"]
                elif defect == "job":
                    self.input["records"].pop(3)
                else:
                    self.input["contexts"][0][defect] = False
                self.refresh()
                self.assert_unavailable()

    def test_unknown_terminal_condition_and_reversed_time_fail_closed(self):
        original = copy.deepcopy(self.input)
        for field, value in (("completed_at", None), ("condition", None),
                             ("completed_at", "2026-01-01T00:00:01Z")):
            with self.subTest(field=field, value=value):
                self.input = copy.deepcopy(original)
                self.input["records"][3][field] = value
                self.refresh()
                self.assert_unavailable()

    def test_mixed_source_tested_policy_or_config_identity_fail_closed(self):
        original = copy.deepcopy(self.input)
        for field, size in (("source_sha", 40), ("tested_sha", 40), ("policy_sha", 40), ("config_digest", 64)):
            with self.subTest(field=field):
                self.input = copy.deepcopy(original)
                self.input["records"][3][field] = "9" * size
                self.refresh()
                self.assert_unavailable()

    def test_failure_and_cancelled_are_not_rewritten_as_success(self):
        for outcome in ("failure", "cancelled"):
            with self.subTest(outcome=outcome):
                self.input["records"][3]["execution_result"] = outcome
                self.refresh()
                result = self.result()
                self.assertEqual(result["analysis"]["contexts"][0]["metrics"]["feedback_wall_seconds"]["value"], 21)
                shard = next(r for r in result["records"] if r["id"] == "shard-b")
                self.assertEqual(shard["execution_result"], outcome)

    def test_monotonic_elapsed_cannot_supply_cross_job_boundaries(self):
        for record in self.input["records"]:
            record["clock"] = "monotonic"
            record["elapsed_seconds"] = 1
        self.refresh()
        self.assert_unavailable()

    def test_step_duration_does_not_double_count_job_resource(self):
        step = copy.deepcopy(self.input["records"][1])
        step.update(id="build-step", kind="step", parent_id="build", step_number=1)
        self.input["records"].append(step)
        self.refresh()
        self.assertEqual(self.context()["metrics"]["job_execution_seconds"]["value"], 25)

    def test_producer_identity_does_not_replace_consumer(self):
        self.input["records"][1]["producer"] = {
            "repository": "nil-studio/oasis7", "run_id": 42, "run_attempt": 7,
            "job_id": 123, "source_sha": "5" * 40, "tested_sha": "6" * 40,
            "config_digest": "7" * 64,
        }
        self.refresh()
        result = self.result()
        self.assertEqual(result["analysis"]["contexts"][0]["metrics"]["feedback_wall_seconds"]["value"], 21)
        record = next(r for r in result["records"] if r["id"] == "build")
        self.assertEqual(record["producer"]["run_id"], 42)
        self.assertEqual(record["producer"]["run_attempt"], 7)
        self.assertEqual(record["context_id"], "attempt-1")

    def test_unsupported_schema_and_unverified_pointer_fail_closed(self):
        original = copy.deepcopy(self.input)
        for defect in ("schema", "pointer", "value"):
            with self.subTest(defect=defect):
                self.input = copy.deepcopy(original)
                if defect == "schema":
                    self.input["sources"][0]["source_schema"] = "future/v99"
                elif defect == "pointer":
                    self.input["records"][3]["field_sources"]["completed_at"]["pointer"] = "/records/99/completed_at"
                else:
                    self.input["records"][3]["completed_at"] = "2026-01-01T00:00:19Z"
                self.assert_unavailable()

    def test_same_reference_different_payload_is_not_silently_accepted(self):
        other = copy.deepcopy(self.input["sources"][0])
        other["payload"]["records"][3]["execution_result"] = "failure"
        self.input["sources"].append(other)
        self.assert_unavailable()

    def test_duplicate_sources_preserve_provenance_without_double_count(self):
        other = copy.deepcopy(self.input["sources"][0])
        other["reference"] = "fixture/copy.json"
        self.input["sources"].append(other)
        duplicate = copy.deepcopy(self.input["records"][3])
        duplicate["id"] = "shard-b-alias"
        for source in duplicate["field_sources"].values():
            if source["state"] == "known":
                source["reference"] = "fixture/copy.json"
        self.input["records"].append(duplicate)
        snapshot = t.collect(self.input)
        self.assertEqual(len(snapshot["records"]), 5)
        record = next(r for r in snapshot["records"] if r["job_id"] == 103)
        self.assertEqual(set(record["aliases"]), {"shard-b", "shard-b-alias"})
        self.assertEqual(len(record["field_sources"]["completed_at"]), 2)
        self.assertEqual(t.report(snapshot)["analysis"]["contexts"][0]["metrics"]["job_execution_seconds"]["value"], 25)

    def test_conflicting_same_job_identity_preserves_both_outcomes(self):
        other = copy.deepcopy(self.input["records"][3])
        other.update(id="shard-b-conflict", execution_result="failure")
        self.input["records"].append(other)
        self.refresh()
        result = self.assert_unavailable()
        conflicts = [r for r in result["records"] if r.get("identity_conflict")]
        self.assertEqual({r["execution_result"] for r in conflicts}, {"success", "failure"})

    def test_attempts_remain_independent_and_old_success_cannot_mask_failure(self):
        context = copy.deepcopy(self.input["contexts"][0])
        context.update(id="attempt-2", run_attempt=2)
        self.input["contexts"].append(context)
        for record in copy.deepcopy(self.input["records"]):
            record["context_id"] = "attempt-2"
            record["job_id"] += 1000
            if record["id"] == "gate":
                record["execution_result"] = "failure"
            self.input["records"].append(record)
        self.refresh()
        result = self.result()
        self.assertEqual(len(result["analysis"]["contexts"]), 2)
        for context in result["analysis"]["contexts"]:
            self.assertEqual(context["metrics"]["job_execution_seconds"]["value"], 25)
        gates = [r for r in result["records"] if r["id"] == "gate"]
        self.assertEqual({r["execution_result"] for r in gates}, {"success", "failure"})

    def test_execution_and_observed_wall_chains_can_differ(self):
        # A has more execution, B starts later and determines observed completion.
        self.input["records"][2]["completed_at"] = "2026-01-01T00:00:19Z"
        self.input["records"][3]["started_at"] = "2026-01-01T00:00:18Z"
        self.refresh()
        metrics = self.context()["metrics"]
        self.assertEqual(metrics["execution_longest_chain_seconds"]["value"], 20)
        self.assertEqual(metrics["observed_wall_chain_seconds"]["value"], 21)
        self.assertIn("shard-a", metrics["execution_longest_chain_seconds"]["input_ids"])
        self.assertIn("shard-b", metrics["observed_wall_chain_seconds"]["input_ids"])

    def test_skipped_false_branch_has_zero_execution_without_becoming_success(self):
        skipped = copy.deepcopy(self.input["records"][1])
        skipped.update(id="build-alternative", job_id=205, job_key="build-alternative",
                       condition="false", execution_result="skipped",
                       started_at=None, completed_at=None)
        self.input["records"].append(skipped)
        self.input["contexts"][0]["job_ids"].append(skipped["id"])
        self.input["records"][4]["depends_on"].append(skipped["id"])
        self.refresh()
        result = self.result()
        context = result["analysis"]["contexts"][0]
        self.assertEqual(context["metrics"]["job_execution_seconds"]["value"], 25)
        self.assertEqual(context["metrics"]["feedback_wall_seconds"]["value"], 21)
        record = next(r for r in result["records"] if r["id"] == skipped["id"])
        self.assertEqual(record["execution_result"], "skipped")
        timing = next(r for r in context["job_timings"] if r["id"] == skipped["id"])
        self.assertEqual(timing["execution_seconds"], 0)

    def test_nanosecond_utc_precision_is_not_truncated_to_microseconds(self):
        self.input["records"][4]["completed_at"] = "2026-01-01T00:00:21.000000009Z"
        self.refresh()
        metrics = self.context()["metrics"]
        self.assertAlmostEqual(metrics["feedback_wall_seconds"]["value"], 21.000000009, places=10)
        self.assertAlmostEqual(metrics["job_execution_seconds"]["value"], 25.000000009, places=10)
        self.assertGreater(metrics["feedback_wall_seconds"]["value"], 21)

    def test_report_rejects_tampered_snapshot(self):
        snapshot = t.collect(self.input)
        for defect in ("evidence", "provenance", "unsafe-field"):
            with self.subTest(defect=defect):
                tampered = copy.deepcopy(snapshot)
                if defect == "evidence":
                    tampered["evidence"] = []
                elif defect == "provenance":
                    tampered["records"][0]["field_sources"]["started_at"][0]["reference"] = "fixture/missing.json"
                else:
                    tampered["records"][0]["runner"] = {"secret": "private"}
                with self.assertRaises((ValueError, TypeError)):
                    t.report(tampered)

    def duplicate_context(self):
        context = copy.deepcopy(self.input["contexts"][0])
        context["id"] = "same-attempt-observation"
        self.input["contexts"].append(context)
        for record in copy.deepcopy(self.input["records"]):
            record["context_id"] = context["id"]
            self.input["records"].append(record)
        return context

    def test_conflicting_context_creation_time_invalidates_both_observations(self):
        context = self.duplicate_context()
        context["created_at"] = "2025-12-31T23:59:59Z"
        self.refresh()
        snapshot = t.collect(self.input)
        self.assertEqual(snapshot["collection_status"], "partial")
        self.assertEqual(len(snapshot["contexts"]), 2)
        self.assertTrue(all(c["identity_conflict"] for c in snapshot["contexts"]))
        self.assertEqual(len(snapshot["records"]), 10)
        self.assert_unavailable()
        for index in range(2):
            with self.subTest(tampered_context=index):
                tampered = copy.deepcopy(snapshot)
                tampered["contexts"][index]["identity_conflict"] = False
                with self.assertRaises(ValueError):
                    t.report(tampered)

    def test_same_context_facts_with_different_alias_preserve_both_graphs(self):
        self.duplicate_context()
        self.refresh()
        snapshot = t.collect(self.input)
        self.assertEqual(snapshot["collection_status"], "complete")
        self.assertEqual(len(snapshot["contexts"]), 2)
        self.assertFalse(any(c["identity_conflict"] for c in snapshot["contexts"]))
        self.assertEqual(len(snapshot["records"]), 10)
        for context in t.report(snapshot)["analysis"]["contexts"]:
            self.assertEqual(context["status"], "available")
            self.assertEqual(context["metrics"]["job_execution_seconds"]["value"], 25)
            self.assertEqual(len(context["job_timings"]), 5)
        tampered = copy.deepcopy(snapshot)
        tampered["contexts"][0]["identity_conflict"] = True
        with self.assertRaises(ValueError):
            t.report(tampered)

    def test_incomplete_coverage_is_partial_at_collection_not_only_analysis(self):
        original = copy.deepcopy(self.input)
        for field in ("pagination_complete", "dependencies_complete", "conditions_complete"):
            for value in (False, None):
                with self.subTest(field=field, value=value):
                    self.input = copy.deepcopy(original)
                    self.input["contexts"][0][field] = value
                    self.refresh()
                    snapshot = t.collect(self.input)
                    self.assertEqual(snapshot["collection_status"], "partial")
                    self.assertTrue(snapshot["missing_reasons"])
                    self.assert_unavailable()
                    tampered = copy.deepcopy(snapshot)
                    tampered["collection_status"] = "complete"
                    tampered["missing_reasons"] = []
                    with self.assertRaises(ValueError):
                        t.report(tampered)

    def test_incomplete_revision_identity_cannot_escape_job_conflict_group(self):
        original = copy.deepcopy(self.input)
        for field, size in (("source_sha", 40), ("tested_sha", 40),
                            ("policy_sha", 40), ("config_digest", 64)):
            with self.subTest(field=field):
                self.input = copy.deepcopy(original)
                conflict = copy.deepcopy(self.input["records"][3])
                conflict.update(id="new-failing-observation", execution_result="failure")
                conflict[field] = "9" * size
                self.input["records"].append(conflict)
                self.refresh()
                snapshot = t.collect(self.input)
                self.assertEqual(snapshot["collection_status"], "partial")
                observations = [r for r in snapshot["records"] if r["job_id"] == 103]
                self.assertEqual(len(observations), 2)
                self.assertTrue(all(r["identity_conflict"] for r in observations))
                self.assertEqual({r["execution_result"] for r in observations}, {"success", "failure"})
                self.assertEqual({r["identity_status"] for r in observations}, {"complete", "partial"})
                self.assert_unavailable()
                tampered = copy.deepcopy(snapshot)
                for record in tampered["records"]:
                    if record["job_id"] == 103:
                        record["identity_conflict"] = False
                with self.assertRaises(ValueError):
                    t.report(tampered)

    def test_no_provable_intervals_is_unknown_not_zero_execution(self):
        for record in self.input["records"]:
            record["completed_at"] = None
        self.refresh()
        result = self.assert_unavailable()
        known = result["analysis"]["contexts"][0]["known_job_execution_seconds"]
        self.assertEqual(known["status"], "unavailable")
        self.assertIsNone(known["value"])
        self.assertEqual(known["input_ids"], [])

    def test_duplicate_page_same_record_id_deduplicates_without_new_alias(self):
        self.input["records"].append(copy.deepcopy(self.input["records"][3]))
        self.refresh()
        snapshot = t.collect(self.input)
        self.assertEqual(snapshot["collection_status"], "complete")
        self.assertEqual(len(snapshot["records"]), 5)
        shard = next(r for r in snapshot["records"] if r["job_id"] == 103)
        self.assertEqual(shard["aliases"], ["shard-b"])
        self.assertFalse(shard["identity_conflict"])
        self.assertEqual(len(shard["field_sources"]["completed_at"]), 2)
        result = t.report(snapshot)
        self.assertEqual(result["analysis"]["contexts"][0]["metrics"]["job_execution_seconds"]["value"], 25)

    def test_conflicting_page_same_record_id_preserves_both_versions(self):
        conflict = copy.deepcopy(self.input["records"][3])
        conflict["execution_result"] = "failure"
        self.input["records"].append(conflict)
        self.refresh()
        snapshot = t.collect(self.input)
        self.assertEqual(snapshot["collection_status"], "partial")
        observations = [r for r in snapshot["records"] if r["job_id"] == 103]
        self.assertEqual(len(observations), 2)
        self.assertEqual({r["execution_result"] for r in observations}, {"success", "failure"})
        self.assertTrue(all(r["identity_conflict"] for r in observations))
        self.assertTrue(all(r["aliases"] == ["shard-b"] for r in observations))
        self.assert_unavailable()
        tampered = copy.deepcopy(snapshot)
        for record in tampered["records"]:
            record["identity_conflict"] = False
        with self.assertRaises(ValueError):
            t.report(tampered)

    def test_distinct_jobs_reusing_record_alias_are_partial_and_ambiguous(self):
        other = copy.deepcopy(self.input["records"][3])
        other["job_id"] = 999
        self.input["records"].append(other)
        self.refresh()
        snapshot = t.collect(self.input)
        self.assertEqual(snapshot["collection_status"], "partial")
        self.assertIn("ambiguous_record_alias", snapshot["missing_reasons"])
        observations = [r for r in snapshot["records"] if r["id"] == "shard-b"]
        self.assertEqual({r["job_id"] for r in observations}, {103, 999})
        self.assertFalse(any(r["identity_conflict"] for r in observations))
        self.assert_unavailable()
        tampered = copy.deepcopy(snapshot)
        tampered["missing_reasons"] = [reason for reason in tampered["missing_reasons"]
                                      if reason != "ambiguous_record_alias"]
        tampered["collection_status"] = "complete"
        with self.assertRaises(ValueError):
            t.report(tampered)

    def test_local_missing_dirty_identity_cannot_impersonate_github(self):
        self.input["origin"] = "local"
        context = self.input["contexts"][0]
        for field in ("repository", "run_id", "run_attempt"):
            context[field] = None
            context["field_sources"][field] = {"state": "not_applicable", "reason": "local-context"}
        context["local_invocation_id"] = "local-measurement-001"
        context["input_capture"] = "at_measurement"
        context["dirty_inputs_digest"] = None
        context["field_sources"]["dirty_inputs_digest"] = {"state": "unknown", "reason": "missing-capture"}
        self.refresh()
        snapshot = t.collect(self.input)
        self.assertEqual(snapshot["origin"], "local")
        self.assertEqual(snapshot["collection_status"], "partial")
        self.assert_unavailable()




class BoundedCollectionTests(unittest.TestCase):
    def setUp(self):
        import io, zipfile
        self.run = {"id": 7, "run_attempt": 2, "head_sha": "a" * 40, "event": "pull_request",
                    "created_at": "2026-01-01T00:00:00Z", "repository": {"full_name": "owner/repo"}}
        self.job = {"id": 10, "run_id": 7, "run_attempt": 2, "head_sha": "a" * 40,
                    "conclusion": "failure", "started_at": "2026-01-01T00:00:01Z",
                    "completed_at": "2026-01-01T00:00:03Z", "name": "password=NEVER_COPY",
                    "steps": [{"number": 1, "conclusion": "cancelled"}]}
        self.plan = {"fast_plan_version": 1, "check_inventory_version": 1,
                     "source_sha": "a" * 40, "tested_sha": "b" * 40, "policy_sha": "c" * 40}
        self.archive = self.zip({"selection.json": json.dumps(self.plan).encode()})
        self.artifact = {"id": 20, "name": "ci-selection-7-2", "expired": False,
                         "size_in_bytes": len(self.archive), "workflow_run": {"id": 7, "head_sha": "a"*40},
                         "digest": "sha256:" + __import__('hashlib').sha256(self.archive).hexdigest()}
        self.responses = {"runs/7/attempts/2": self.run,
                          "runs/7/attempts/2/jobs?per_page=100&page=1": {"total_count": 1, "jobs": [self.job]},
                          "runs/7/artifacts?per_page=100&page=1": {"total_count": 1, "artifacts": [self.artifact]},
                          "artifacts/20/zip": self.archive}
        self.calls = []

    @staticmethod
    def zip(files):
        import io, zipfile
        out = io.BytesIO()
        with zipfile.ZipFile(out, "w", compression=zipfile.ZIP_DEFLATED) as z:
            for name, data in files.items(): z.writestr(name, data)
        return out.getvalue()

    def fetch(self, url, limit, timeout):
        self.calls.append(url)
        self.assertGreater(timeout, 0)
        key = url.split('/actions/', 1)[1]
        value = self.responses.get(key)
        if value is None: return 503, None, b''
        raw = value if isinstance(value, bytes) else json.dumps(value).encode()
        return 200, None, raw[:limit + 1]

    def collect(self, **kwargs):
        budget = kwargs.pop('budget', t.CollectionBudget())
        return t.collect_actions('owner/repo', 7, 2, budget, self.fetch, **kwargs)

    def test_real_shapes_preserve_identity_result_without_guessing_graph(self):
        snapshot, receipt = self.collect()
        ctx = snapshot['contexts'][0]
        self.assertEqual([ctx[k] for k in ('source_sha','tested_sha','policy_sha')], ['a'*40,'b'*40,'c'*40])
        self.assertEqual([r['execution_result'] for r in snapshot['records']], ['failure','cancelled'])
        self.assertEqual(snapshot['records'][0]['job_id'], 10)
        self.assertIsNone(snapshot['records'][0]['tested_sha'])
        self.assertIsNone(snapshot['records'][0]['depends_on'])
        self.assertIsNone(ctx['config_digest'])
        self.assertTrue(ctx['pagination_complete'])
        self.assertEqual(receipt['budget']['requests'], 4)
        self.assertNotIn('NEVER_COPY', json.dumps((snapshot,receipt)))
        self.assertEqual(t.report(snapshot)['analysis']['status'], 'unavailable')
        self.assertEqual(receipt['snapshot_sha256'], __import__('hashlib').sha256(t.adapters_bytes(snapshot)).hexdigest())

    def test_second_page_failure_keeps_first_page(self):
        jobs = [dict(self.job, id=i) for i in range(1,101)]
        self.responses['runs/7/attempts/2/jobs?per_page=100&page=1'] = {'total_count':101,'jobs':jobs}
        snapshot, receipt = self.collect()
        self.assertEqual(len([r for r in snapshot['records'] if r['kind']=='job']),100)
        self.assertFalse(snapshot['contexts'][0]['pagination_complete'])
        self.assertIn('http_status_503',receipt['missing_reasons'])

    def test_full_pagination_and_repeated_or_conflicting_page(self):
        key='runs/7/attempts/2/jobs?per_page=100&page='
        self.responses[key+'1']={'total_count':101,'jobs':[dict(self.job,id=i) for i in range(1,101)]}
        self.responses[key+'2']={'total_count':101,'jobs':[dict(self.job,id=101)]}
        snapshot,_=self.collect()
        self.assertTrue(snapshot['contexts'][0]['pagination_complete'])
        self.assertEqual(len(snapshot['contexts'][0]['job_ids']),101)
        for item in (dict(self.job,id=1),dict(self.job,id=1,conclusion='success')):
            self.responses[key+'2']={'total_count':101,'jobs':[item]}
            snapshot,receipt=self.collect()
            self.assertFalse(snapshot['contexts'][0]['pagination_complete'])
            self.assertIn('repeated_page_identity',receipt['missing_reasons'])
            self.assertTrue(all(r['execution_result']=='failure' for r in snapshot['records'] if r['kind']=='job'))

    def test_run_job_plan_and_artifact_mismatch(self):
        for target,field,value,reason in [(self.run,'run_attempt',3,'run_identity_mismatch_or_missing'),
                                        (self.job,'run_attempt',1,'job_identity_mismatch'),
                                        (self.artifact,'expired',True,'artifact_expired_or_unknown')]:
            old=target[field];target[field]=value
            snapshot,receipt=self.collect()
            self.assertIn(reason,receipt['missing_reasons'])
            self.assertEqual(snapshot['collection_status'],'partial')
            target[field]=old
        self.plan['source_sha']='d'*40
        self.archive=self.zip({'selection.json':json.dumps(self.plan).encode()})
        self.responses['artifacts/20/zip']=self.archive
        self.artifact.update(size_in_bytes=len(self.archive),digest='sha256:'+__import__('hashlib').sha256(self.archive).hexdigest())
        snapshot,receipt=self.collect()
        self.assertIn('selection_identity_or_schema_mismatch',receipt['missing_reasons'])
        self.assertIsNone(snapshot['contexts'][0]['tested_sha'])

    def test_request_byte_and_clock_exhaustion(self):
        snapshot,receipt=self.collect(budget=t.CollectionBudget(requests=2))
        self.assertEqual(receipt['budget']['requests'],2)
        self.assertEqual(snapshot['records'][0]['execution_result'],'failure')
        self.assertIn('request_budget_exhausted',receipt['missing_reasons'])
        snapshot,receipt=self.collect(budget=t.CollectionBudget(byte_limit=2))
        self.assertIn('response_byte_budget_exhausted',receipt['missing_reasons'])
        self.assertEqual(snapshot['records'],[])
        snapshot,receipt=self.collect(budget=t.CollectionBudget(seconds=0))
        self.assertEqual(receipt['budget']['requests'],0)
        self.assertIn('time_budget_exhausted',receipt['missing_reasons'])
        ticks=iter([0,0,121])
        budget=t.CollectionBudget(clock=lambda:next(ticks))
        budget.request()
        with self.assertRaisesRegex(t.CollectionStopped,'time_budget'):budget.charge(1)

    def test_redirect_never_sends_to_other_host_or_retries(self):
        reader=t.ActionsReader('owner/repo',t.CollectionBudget(),lambda *args:(302,'https://attacker.example/x',b''))
        with self.assertRaisesRegex(t.CollectionStopped,'unsafe_download'):reader.get('artifacts/20/zip','artifacts/20.zip',archive=True)
        self.assertEqual(reader.budget.requests,1)
        for url in ('http://api.github.com/repos/owner/repo/actions/runs/7/attempts/2',
                    'https://api.github.com@attacker.example/x','https://api.github.com/repos/owner/repo/issues',
                    'https://api.github.com:password=NEVER_COPY/x','https://[broken'):
            with self.assertRaises(t.CollectionStopped):t._download_url(url)

    def test_archive_paths_duplicates_links_and_bombs(self):
        import io, zipfile, stat
        for files in ({'../selection.json':b'{}'},{'/selection.json':b'{}'},{'a\\selection.json':b'{}'},
                      {'selection.json':b'{}','./selection.json':b'{}'}):
            with self.assertRaises(t.CollectionStopped):t.archive_document(self.zip(files),'selection.json',t.CollectionBudget())
        raw=self.zip({'selection.json':b' '*10000})
        with self.assertRaisesRegex(t.CollectionStopped,'decoded_byte_budget'):t.archive_document(raw,'selection.json',t.CollectionBudget(byte_limit=50))
        out=io.BytesIO()
        with zipfile.ZipFile(out,'w') as z:
            info=zipfile.ZipInfo('selection.json');info.create_system=3;info.external_attr=(stat.S_IFLNK|0o777)<<16
            z.writestr(info,'elsewhere')
        with self.assertRaisesRegex(t.CollectionStopped,'unsupported_archive'):t.archive_document(out.getvalue(),'selection.json',t.CollectionBudget())

    def test_nested_tar_reads_manifest_without_extracting_or_running_binary(self):
        import io, tarfile
        for bad in (False,True):
            out=io.BytesIO()
            with tarfile.open(fileobj=out,mode='w:gz',format=tarfile.USTAR_FORMAT) as archive:
                info=tarfile.TarInfo('./bin/tool');info.size=16;archive.addfile(info,io.BytesIO(b'do not execute!!'))
                info=tarfile.TarInfo('../manifest.json' if bad else './manifest.json');data=b'{"schema_version":2}';info.size=len(data);archive.addfile(info,io.BytesIO(data))
            raw=self.zip({'bundle.tar.gz':out.getvalue()})
            if bad:
                with self.assertRaisesRegex(t.CollectionStopped,'unsafe_archive'):t.archive_document(raw,'manifest.json',t.CollectionBudget())
            else:
                doc,data=t.archive_document(raw,'manifest.json',t.CollectionBudget())
                self.assertEqual(doc,{'schema_version':2});self.assertEqual(data,b'{"schema_version":2}')

    def test_build_producer_is_not_consumer(self):
        manifest={'schema_version':2,'source_sha':'a'*40,'tested_sha':'b'*40,'config_sha256':'e'*64,
                  'producer_run_id':'6','producer_attempt':'1','execution_plan':self.plan,
                  'build_timings':[{'elapsed_seconds':2.5}]}
        raw=self.zip({'manifest.json':json.dumps(manifest).encode()})
        artifact=dict(self.artifact,id=30,name='build',size_in_bytes=len(raw),digest='sha256:'+__import__('hashlib').sha256(raw).hexdigest())
        self.responses['runs/7/artifacts?per_page=100&page=1']={'total_count':2,'artifacts':[self.artifact,artifact]}
        self.responses['artifacts/30/zip']=raw
        snapshot,receipt=self.collect(build_artifacts=[30])
        observed=receipt['build_observations'][0]
        self.assertEqual((observed['producer_run_id'],observed['producer_attempt'],observed['consumer_run_id'],observed['consumer_attempt']),(6,1,7,2))
        self.assertIn('different_or_unknown_producer_attempt',observed['missing_reasons'])
        self.assertIsNone(snapshot['records'][-2]['producer'])
        self.assertEqual(snapshot['records'][-1]['elapsed_seconds'],2.5)

    def test_metrics_never_signs_dirty_inputs_or_infers_success(self):
        with tempfile.TemporaryDirectory() as directory:
            path=Path(directory)/'current.metrics.json'
            path.write_text(json.dumps({'schema_version':2,'commit_oid':'a'*40,'package':'crate','check_only':True,
                                        'no_default_features':True,'warm_check_enabled':True,'cargo_check_seconds':4,
                                        'cargo_check_warm_seconds':.5,'checkout_path':'/private/password=NEVER_COPY'}))
            snapshot,receipt=t.collect_metrics(path,t.CollectionBudget())
            self.assertEqual([r['elapsed_seconds'] for r in snapshot['records']],[4,.5])
            self.assertTrue(all(r['execution_result'] is None for r in snapshot['records']))
            self.assertIsNone(snapshot['contexts'][0]['source_sha'])
            self.assertIsNone(snapshot['contexts'][0]['local_invocation_id'])
            self.assertIsNone(snapshot['contexts'][0]['dirty_inputs_digest'])
            self.assertNotIn('NEVER_COPY',json.dumps((snapshot,receipt)))
            self.assertEqual(receipt['budget']['requests'],0)
            self.assertEqual(t.report(snapshot)['analysis']['status'],'unavailable')

    def test_cli_replay_and_receipt_failure_do_not_modify_input(self):
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory);exports=root/'exports';exports.mkdir()
            (exports/'run.json').write_text(json.dumps(self.run))
            (exports/'jobs-1.json').write_text(json.dumps(self.responses['runs/7/attempts/2/jobs?per_page=100&page=1']))
            (exports/'artifacts-1.json').write_text(json.dumps(self.responses['runs/7/artifacts?per_page=100&page=1']))
            (exports/'artifact-20.zip').write_bytes(self.archive)
            args=['collect-github','owner/repo','7','2',str(root/'snapshot.json'),str(root/'receipt.json'),'--recorded-responses',str(exports)]
            self.assertEqual(t.main(args),1)
            self.assertEqual(json.loads((root/'receipt.json').read_text())['transport'],'recorded')
            args[4]=str(exports/'run.json')
            self.assertEqual(t.main(args),1)
            self.assertEqual(json.loads((exports/'run.json').read_text()),self.run)
            args[4]=str(root/'snapshot.json');args[5]=str(root/'missing'/'receipt.json')
            self.assertEqual(t.main(args),1)
            self.assertTrue((root/'snapshot.json').is_file())


    def test_bad_schema_and_nonfinite_json_preserve_partial(self):
        for repository in (None, [], {'full_name':12}):
            self.run['repository']=repository
            snapshot,receipt=self.collect()
            self.assertIn('run_identity_mismatch_or_missing',receipt['missing_reasons'])
            self.assertEqual(snapshot['collection_status'],'partial')
        self.run['repository']={'full_name':'owner/repo'}
        self.responses['runs/7/artifacts?per_page=100&page=1']=b'{"total_count":NaN,"artifacts":[]}'
        snapshot,receipt=self.collect()
        self.assertIn('invalid_response_json',receipt['missing_reasons'])
        self.assertEqual(snapshot['records'][0]['execution_result'],'failure')

    def test_unused_unicode_metadata_does_not_amplify_contract(self):
        self.run['unused']='\u6c34'*(6*1024*1024)
        self.responses['runs/7/attempts/2']=json.dumps(self.run,ensure_ascii=False).encode()
        snapshot,receipt=self.collect()
        self.assertEqual(snapshot['records'][0]['execution_result'],'failure')
        self.assertLess(len(json.dumps(snapshot)),50000)
        self.assertLess(receipt['budget']['response_bytes'],t.MAX_INPUT_BYTES)

    def test_corrupt_deflate_is_partial_after_jobs(self):
        import struct
        raw=bytearray(self.archive)
        size=30+struct.unpack_from('<H',raw,26)[0]+struct.unpack_from('<H',raw,28)[0]
        raw[size]=(raw[size]&~7)|7  # reserved DEFLATE block type
        self.responses['artifacts/20/zip']=bytes(raw)
        self.artifact['digest']='sha256:'+__import__('hashlib').sha256(raw).hexdigest()
        snapshot,receipt=self.collect()
        self.assertIn('invalid_archive_document',receipt['missing_reasons'])
        self.assertEqual(snapshot['records'][0]['execution_result'],'failure')

    def test_projection_and_normalization_check_deadline(self):
        self.job['steps']=[{'number':i+1} for i in range(3000)]
        ticks=[0.0]
        def clock():
            ticks[0]+=.0001
            return ticks[0]
        snapshot,receipt=self.collect(budget=t.CollectionBudget(seconds=.03,clock=clock))
        self.assertIn('time_budget_exhausted',receipt['missing_reasons'])
        self.assertLess(len(snapshot['records']),512)
        self.assertEqual(snapshot['collection_status'],'partial')
        self.assertEqual(t.report(snapshot)['analysis']['status'],'unavailable')

    @unittest.skipUnless(__import__('os').name=='posix','POSIX signal cancellation')
    def test_cli_cancellation_and_timeout_reap_only_request_child(self):
        import os, signal, time
        for cancel in (True,False):
            with self.subTest(cancel=cancel),tempfile.TemporaryDirectory() as directory:
                root=Path(directory);pidfile=root/'child.pid'
                code="""
import sys, subprocess
from pathlib import Path
sys.path.insert(0,sys.argv[1])
import engineering_telemetry as t
original=subprocess.Popen
def sleeper(*args,**kwargs):
    child=original([sys.executable,'-c','import time;time.sleep(30)'],**kwargs)
    Path(sys.argv[2]).write_text(str(child.pid))
    return child
subprocess.Popen=sleeper
raise SystemExit(t.main(['collect-github','owner/repo','7','2',sys.argv[3],sys.argv[4],'--max-seconds',sys.argv[5]]))
"""
                proc=subprocess.Popen([sys.executable,'-c',code,str(ROOT/'scripts'),str(pidfile),str(root/'snapshot.json'),str(root/'receipt.json'),'10' if cancel else '.2'],stdout=subprocess.PIPE,stderr=subprocess.PIPE)
                try:
                    until=time.monotonic()+5
                    while not pidfile.exists() and time.monotonic()<until:time.sleep(.01)
                    self.assertTrue(pidfile.exists())
                    child=int(pidfile.read_text())
                    if cancel:os.kill(proc.pid,signal.SIGTERM)
                    stdout,stderr=proc.communicate(timeout=5)
                    self.assertEqual(proc.returncode,1,stderr.decode())
                    receipt=json.loads((root/'receipt.json').read_text())
                    self.assertIn('collection_cancelled' if cancel else 'time_budget_exhausted',receipt['missing_reasons'])
                    with self.assertRaises(ProcessLookupError):os.kill(child,0)
                finally:
                    if proc.poll() is None:proc.kill();proc.communicate()

if __name__ == "__main__": unittest.main()
