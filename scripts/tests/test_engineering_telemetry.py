import copy
import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location("engineering_telemetry", ROOT / "scripts/engineering_telemetry.py")
t = importlib.util.module_from_spec(spec)
spec.loader.exec_module(t)
FIXTURE = ROOT / "scripts/tests/fixtures/engineering-telemetry/input.json"

class TelemetryTests(unittest.TestCase):
    def setUp(self):
        self.input = json.loads(FIXTURE.read_text())
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
            source.write_bytes(FIXTURE.read_bytes())
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

if __name__ == "__main__": unittest.main()
