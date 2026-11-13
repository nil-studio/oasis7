#!/usr/bin/env python3
"""Source, inventory, provenance and recovery contracts for full CI consumers."""
import copy
from datetime import datetime, timedelta, timezone
from http.server import BaseHTTPRequestHandler, HTTPServer
import importlib.util
import io
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import threading
import unittest
from unittest import mock
import urllib.error
import urllib.parse
import urllib.request
import zipfile

ROOT = Path(__file__).resolve().parent.parent
spec = importlib.util.spec_from_file_location("full_result_subject", ROOT / "scripts/ci-full-result.py")
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
A, B = "a" * 40, "b" * 40
CONFIG = json.loads((ROOT / "scripts/ci-required-scope.json").read_text())


def fixture():
    plan = dict(scope="explicit-full", execution_profile="periodic-full",
                check_inventory_version=1, source_sha=A, tested_sha=A,
                policy_sha=A, groups=list(CONFIG["groups"]))
    needs = {name.replace("_", "-"): {"result": "success"} for name in CONFIG["groups"]}
    needs.update(select={"result": "success"}, **{
        "oasis7-test-build": {"result": "success"}, "full-regression": {"result": "success"},
        "candidate-full-regression": {"result": "skipped"},
    })
    context = dict(repository="nil-studio/oasis7", event_name="schedule", ref="refs/heads/main",
                   workflow_sha=A, workflow_ref="nil-studio/oasis7/.github/workflows/rust.yml@refs/heads/main",
                   run_id=42, run_attempt=2)
    receipt = module.make_receipt(plan, needs, CONFIG, context, "definition")
    run = dict(id=42, run_attempt=2, workflow_id=7, path=module.WORKFLOW,
               head_sha=A, head_branch="main", event="schedule", status="completed", conclusion="success")
    jobs = [dict(name="explicit-full-gate", status="completed", conclusion="success", run_attempt=2)]
    return receipt, run, jobs


class Qualification(unittest.TestCase):
    def verify(self, receipt, run, jobs):
        return module.verify_receipt(receipt, run, jobs, source=A, repository="nil-studio/oasis7",
                                     workflow_id=7, config=CONFIG, digest="definition")

    def test_complete_scheduled_result(self):
        self.verify(*fixture())

    def test_main_policy_can_validate_different_fixed_candidate(self):
        receipt, run, jobs = fixture()
        run.update(event="workflow_dispatch", head_sha=B)
        receipt["context"].update(event_name="workflow_dispatch", workflow_sha=B)
        receipt["plan"].update(execution_profile="candidate-full", policy_sha=B)
        receipt["needs"]["full-regression"]["result"] = "skipped"
        receipt["needs"]["candidate-full-regression"]["result"] = "success"
        self.verify(receipt, run, jobs)

    def test_source_policy_attempt_and_repository_are_bound(self):
        for section, key, value in [
            ("plan", "source_sha", B), ("plan", "tested_sha", B), ("plan", "policy_sha", B),
            ("context", "run_id", 43), ("context", "run_attempt", 1),
            ("context", "workflow_sha", B), ("context", "repository", "other/repo"),
            ("context", "workflow_ref", "nil-studio/oasis7/.github/workflows/rust.yml@refs/heads/candidate"),
        ]:
            with self.subTest(section=section, key=key):
                receipt, run, jobs = fixture()
                receipt[section][key] = value
                with self.assertRaises(ValueError):
                    self.verify(receipt, run, jobs)

    def test_missing_platform_result_or_inventory_entry_fails(self):
        for kind in ("skipped", "missing", "inventory"):
            receipt, run, jobs = fixture()
            if kind == "inventory":
                receipt["plan"]["groups"].remove("command_center")
            elif kind == "missing":
                del receipt["needs"]["fleet-health"]
            else:
                receipt["needs"]["fleet-health"]["result"] = "skipped"
            with self.assertRaises(ValueError):
                self.verify(receipt, run, jobs)

    def test_failed_cancelled_missing_ambiguous_and_old_actual_gate(self):
        for kind in ("failure", "cancelled", "missing", "duplicate", "old_attempt"):
            with self.subTest(kind=kind):
                receipt, run, jobs = fixture()
                if kind == "missing":
                    jobs = []
                elif kind == "duplicate":
                    jobs += copy.deepcopy(jobs)
                elif kind == "old_attempt":
                    jobs[0]["run_attempt"] = 1
                else:
                    jobs[0]["conclusion"] = kind
                with self.assertRaises(ValueError):
                    self.verify(receipt, run, jobs)

    def test_candidate_bootstrap_old_definition_and_wrong_event_fail(self):
        for kind in ("branch", "bootstrap", "definition", "inventory", "pull_request"):
            receipt, run, jobs = fixture()
            if kind == "branch":
                run["head_branch"] = "candidate"
            elif kind == "bootstrap":
                receipt["plan"]["bootstrap_full"] = True
            elif kind == "definition":
                receipt["definition_sha256"] = "older-definition"
            elif kind == "inventory":
                receipt["plan"]["check_inventory_version"] = 0
            else:
                run["event"] = "pull_request"
            with self.assertRaises(ValueError):
                self.verify(receipt, run, jobs)

    def test_duplicate_json_key_is_invalid(self):
        with self.assertRaises(ValueError):
            module.decode_json('{"source":"a","source":"b"}')


class DefinitionDigest(unittest.TestCase):
    RUNNER = 'scripts/ci-python-control-contracts.py'
    CONTRACT = 'scripts/ci-required-result.test.py'

    def setUp(self):
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        self.root = Path(directory.name)
        self.git('init', '-q')
        for path in sorted(set(module.POLICY_FILES) | {self.RUNNER, self.CONTRACT}):
            target = self.root / path
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_text('# original control definition\n')
        self.config = dict(python_control_contract_version=1, python_control_contracts={
            'scripts/ci-required-result.py': dict(group='workflow_governance', tests=[self.CONTRACT])})
        self.write_config()

    def git(self, *arguments):
        return subprocess.check_output(['git', '-C', str(self.root), *arguments], text=True).strip()

    def write_config(self):
        (self.root / 'scripts/ci-required-scope.json').write_text(json.dumps(self.config))

    def commit(self):
        self.git('add', '.')
        self.git('-c', 'user.name=CI fixture', '-c', 'user.email=fixture@example.invalid',
                 'commit', '-qm', 'definition fixture')
        return self.git('rev-parse', 'HEAD')

    def assert_rejects_previous_definition(self, path):
        previous = module.definition_digest(self.root, self.commit())
        (self.root / path).write_text('# changed trusted contract execution\n')
        current = module.definition_digest(self.root, self.commit())
        self.assertNotEqual(previous, current)
        receipt, run, jobs = fixture()
        receipt['definition_sha256'] = previous
        with self.assertRaises(ValueError):
            module.verify_receipt(receipt, run, jobs, source=A, repository='nil-studio/oasis7',
                                  workflow_id=7, config=CONFIG, digest=current)

    def test_trusted_python_runner_change_invalidates_previous_definition(self):
        self.assert_rejects_previous_definition(self.RUNNER)

    def test_registered_baseline_assertion_change_invalidates_previous_definition(self):
        self.assert_rejects_previous_definition(self.CONTRACT)

    def test_complete_governance_assertions_are_hashed_at_the_fixed_revision(self):
        path = 'crates/fixture/scripts/governance.test.py'
        destination = self.root / path
        destination.parent.mkdir(parents=True)
        destination.write_text('# complete governance fixture\n')
        self.config['workflow_governance_python_tests'] = [self.CONTRACT, path]
        self.write_config()
        self.assert_rejects_previous_definition(path)

    def test_complete_governance_inventory_cannot_escape_or_duplicate_files(self):
        for paths in ([], [self.CONTRACT, self.CONTRACT], ['scripts/../outside.test.py'],
                      ['/outside.test.py'], ['scripts/*.test.py']):
            self.config['workflow_governance_python_tests'] = paths
            self.write_config()
            with self.subTest(paths=paths), self.assertRaises(ValueError):
                module.definition_digest(self.root, self.commit())

    def test_digest_reads_contracts_at_fixed_revision_not_worktree(self):
        revision = self.commit()
        previous = module.definition_digest(self.root, revision)
        self.config['python_control_contracts'] = {}
        self.write_config()
        (self.root / self.CONTRACT).write_text('# uncommitted candidate assertions\n')
        self.assertEqual(module.definition_digest(self.root, revision), previous)

    def test_unknown_python_is_not_an_implicit_policy_input(self):
        previous = module.definition_digest(self.root, self.commit())
        (self.root / 'scripts/ordinary-helper.py').write_text('VALUE=7\n')
        self.assertEqual(module.definition_digest(self.root, self.commit()), previous)

    def test_registered_worker_tool_definition_invalidates_previous_receipt(self):
        path = 'scripts/ci-worker-build.py'
        (self.root / path).write_text('# pinned tool preparation\n')
        self.config['python_control_contracts'][path] = dict(
            group='workflow_governance', tests=[self.CONTRACT])
        self.write_config()
        self.assert_rejects_previous_definition(path)

    def test_registered_worker_tool_definition_must_exist_at_fixed_revision(self):
        self.config['python_control_contracts']['scripts/ci-worker-build.py'] = dict(
            group='workflow_governance', tests=[self.CONTRACT])
        self.write_config()
        with self.assertRaises(subprocess.CalledProcessError):
            module.definition_digest(self.root, self.commit())

    def test_invalid_trusted_contract_paths_fail_before_reading_unowned_data(self):
        for path in ('../outside.test.py', '/tmp/outside.test.py', 'scripts/*.test.py',
                     'scripts/../outside.test.py', 'scripts/ordinary.py'):
            self.config['python_control_contracts']['scripts/ci-required-result.py']['tests'] = [path]
            self.write_config()
            with self.assertRaises(ValueError):
                module.definition_digest(self.root, self.commit())

    def test_old_configuration_without_python_contract_capability_remains_readable(self):
        self.config = {}
        self.write_config()
        self.assertEqual(len(module.definition_digest(self.root, self.commit())), 64)

    def test_custody_capability_binds_its_actual_execution_helper(self):
        path = 'scripts/ci-custody-platform.py'
        self.config['custody_execution_version'] = 1
        self.write_config()
        (self.root / path).write_text('# original custody producer/consumer\n')
        self.assert_rejects_previous_definition(path)

    def test_old_policy_does_not_require_new_custody_helper(self):
        revision = self.commit()
        previous = module.definition_digest(self.root, revision)
        (self.root / 'scripts/ci-custody-platform.py').write_text('# not enabled by old policy\n')
        self.assertEqual(previous, module.definition_digest(self.root, self.commit()))

    def test_custody_capability_fails_when_helper_missing_or_version_is_not_exact(self):
        self.config['custody_execution_version'] = 1
        self.write_config()
        with self.assertRaises(subprocess.CalledProcessError):
            module.definition_digest(self.root, self.commit())
        for version in (None, False, True, 0, 2, '1'):
            self.config['custody_execution_version'] = version
            self.write_config()
            with self.subTest(version=version), self.assertRaises(ValueError):
                module.definition_digest(self.root, self.commit())


class Response(io.BytesIO):
    def __init__(self, data, link=""):
        super().__init__(json.dumps(data).encode())
        self.headers = {"Link": link}


class History:
    """Fixed Actions facts; the real selector and receipt validator decide."""
    repository = "nil-studio/oasis7"

    def __init__(self, runs, artifacts=None, payloads=None, jobs=None):
        self.runs = runs
        self.artifacts = artifacts or {}
        self.payloads = payloads or {}
        self.jobs = jobs or {}
        self.downloads = []
        self.queries = []

    def get(self, path):
        if path == "actions/workflows/rust.yml":
            return {"id": 7}
        if path.startswith("actions/runs/"):
            return next(run for run in self.runs if str(run["id"]) == path.rsplit("/", 1)[1])
        raise AssertionError("unexpected API object: " + path)

    def pages(self, path, key):
        self.queries.append(path)
        parsed = urllib.parse.urlsplit(path)
        parts = parsed.path.split("/")
        if parts[:2] == ["actions", "workflows"] and parts[-1] == "runs":
            query = urllib.parse.parse_qs(parsed.query)
            result = [run for run in self.runs if run["event"] == query["event"][0]]
            for parameter, field in (("status", "status"), ("head_sha", "head_sha"), ("branch", "head_branch")):
                if parameter in query:
                    result = [run for run in result if run[field] == query[parameter][0]]
            if "created" in query:
                window = query["created"][0]
                if ".." in window:
                    begin, end = map(module.timestamp, window.split(".."))
                    result = [run for run in result if begin <= module.timestamp(run["created_at"]) <= end]
                else:
                    result = [run for run in result if module.timestamp(run["created_at"]) >=
                              module.timestamp(window.removeprefix(">="))]
            return result
        if parts[:2] == ["actions", "runs"] and parts[-1] == "artifacts":
            return self.artifacts.get(int(parts[2]), [])
        if parts[:2] == ["actions", "runs"] and parts[-1] == "jobs":
            return self.jobs.get((int(parts[2]), int(parts[4])), [])
        raise AssertionError("unexpected API collection: " + path)

    def artifact(self, artifact_id, filename="full-result.json"):
        self.downloads.append((artifact_id, filename))
        return copy.deepcopy(self.payloads[artifact_id])


class Attempts(unittest.TestCase):
    def setUp(self):
        self.receipt, self.green, self.gate = fixture()
        self.green.update(created_at="2026-10-30T10:00:00Z", run_started_at="2026-10-30T10:00:00Z",
                          updated_at="2026-10-30T10:30:00Z")
        self.api = History([self.green],
                           artifacts={42: [{"id": 4202, "name": "ci-full-result-42-2", "expired": False}]},
                           payloads={4202: self.receipt}, jobs={(42, 2): self.gate})

    def verify(self):
        return module.find_verified(self.api, A, CONFIG, "definition")

    def test_complete_matching_run_is_discovered(self):
        self.assertEqual(self.verify()["context"]["run_id"], 42)
        self.assertEqual(self.api.downloads, [(4202, "full-result.json")])

    def test_later_failed_rerun_of_an_older_run_blocks_a_newer_created_green(self):
        for attempt_clock in ("run_started_at", "updated_at"):
            with self.subTest(attempt_clock=attempt_clock):
                failed = dict(self.green, id=41, run_attempt=3, conclusion="failure",
                              created_at="2026-10-30T09:00:00Z", run_started_at="2026-10-30T09:00:00Z",
                              updated_at="2026-10-30T09:30:00Z")
                failed[attempt_clock] = "2026-10-30T11:00:00Z"
                self.api.runs = [self.green, failed]
                self.api.artifacts[41] = [{"id": 4101, "name": "ci-full-result-41-1", "expired": False}]
                self.api.downloads.clear()
                with self.assertRaises(ValueError):
                    self.verify()
                self.assertEqual(self.api.downloads, [])

    def test_newer_matching_attempt_in_progress_or_queued_blocks_old_green(self):
        for status in ("in_progress", "queued"):
            with self.subTest(status=status):
                pending = dict(self.green, id=43, run_attempt=1, status=status, conclusion=None,
                               created_at="2026-10-30T11:00:00Z", updated_at="2026-10-30T11:00:00Z")
                self.api.runs = [self.green, pending]
                self.api.downloads.clear()
                with self.assertRaises(ValueError):
                    self.verify()
                self.assertEqual(self.api.downloads, [])

    def test_unknown_pending_manual_source_cannot_fall_back_to_old_green(self):
        pending = dict(self.green, id=43, run_attempt=1, event="workflow_dispatch", head_sha=B,
                       status="in_progress", conclusion=None, created_at="2026-10-30T11:00:00Z",
                       updated_at="2026-10-30T11:00:00Z")
        self.api.runs = [self.green, pending]
        with self.assertRaises(ValueError):
            self.verify()
        self.assertEqual(self.api.downloads, [])

    def diagnostic_attempt(self, name="newapi-bridge-linux-x86-64"):
        run = dict(self.green, id=43, event="workflow_dispatch", head_sha=B,
                   created_at="2026-10-30T11:00:00Z", run_started_at="2026-10-30T11:00:00Z",
                   updated_at="2026-10-30T11:30:00Z")
        skipped = dict(run_attempt=2, status="completed", conclusion="skipped", started_at=None)
        other = ("codeql-rust-model-diagnostic" if name == "newapi-bridge-linux-x86-64"
                 else "newapi-bridge-linux-x86-64")
        jobs = [dict(skipped, id=4300, name="select"),
                dict(skipped, id=4301, name="explicit-full-gate"),
                dict(skipped, id=4302, name=name, conclusion="success",
                     started_at="2026-10-30T11:00:00Z"),
                dict(skipped, id=4303, name=other)]
        self.api.runs = [self.green, run]
        self.api.jobs[(43, 2)] = jobs
        self.api.downloads.clear()
        return run, jobs

    def test_started_known_diagnostic_does_not_block_older_full_result(self):
        for name in ("newapi-bridge-linux-x86-64", "codeql-rust-model-diagnostic"):
            for status, conclusion in (("in_progress", None), ("completed", "success"),
                                       ("completed", "failure"), ("completed", "cancelled")):
                with self.subTest(name=name, status=status, conclusion=conclusion):
                    run, jobs = self.diagnostic_attempt(name)
                    run.update(status=status, conclusion=conclusion)
                    jobs[2].update(status=status, conclusion=conclusion)
                    self.assertEqual(self.verify()["context"]["run_id"], 42)
                    self.assertEqual(self.api.downloads, [(4202, "full-result.json")])

    def test_unproven_or_ambiguous_diagnostic_cannot_fall_back_to_old_green(self):
        for kind in ("unknown", "not_started", "queued", "skipped", "missing", "duplicate",
                     "both_started", "old_diagnostic_attempt", "old_select_attempt", "old_gate_attempt",
                     "missing_select", "duplicate_select", "select_failure", "select_cancelled",
                     "select_in_progress", "missing_gate", "duplicate_gate", "gate_success",
                     "gate_failure", "gate_cancelled", "gate_in_progress"):
            with self.subTest(kind=kind):
                _, jobs = self.diagnostic_attempt()
                if kind == "unknown":
                    jobs[2]["name"] = "unknown-diagnostic"
                elif kind == "not_started":
                    jobs[2]["started_at"] = None
                elif kind == "queued":
                    jobs[2].update(status="queued", conclusion=None)
                elif kind == "skipped":
                    jobs[2]["conclusion"] = "skipped"
                elif kind == "missing":
                    del jobs[2]
                elif kind == "duplicate":
                    jobs.append(dict(jobs[2], id=4399))
                elif kind == "both_started":
                    jobs[3].update(conclusion="success", started_at="2026-10-30T11:00:00Z")
                elif kind.startswith("old_"):
                    index = {"old_diagnostic_attempt": 2, "old_select_attempt": 0,
                             "old_gate_attempt": 1}[kind]
                    jobs[index]["run_attempt"] = 1
                else:
                    index = 0 if "select" in kind else 1
                    if kind.startswith("missing_"):
                        del jobs[index]
                    elif kind.startswith("duplicate_"):
                        jobs.append(dict(jobs[index], id=4399))
                    elif kind.endswith("in_progress"):
                        jobs[index].update(status="in_progress", conclusion=None)
                    else:
                        jobs[index]["conclusion"] = kind.rsplit("_", 1)[1]
                with self.assertRaisesRegex(ValueError, "no source selection"):
                    self.verify()
                self.assertEqual(self.api.downloads, [])

    def test_diagnostic_evidence_cannot_qualify_an_exact_run_or_hide_a_schedule(self):
        run, _ = self.diagnostic_attempt()
        with self.assertRaisesRegex(ValueError, "current attempt"):
            module.find_verified(self.api, A, CONFIG, "definition", run_id=43)
        run.update(event="schedule", head_sha=A)
        with self.assertRaisesRegex(ValueError, "newer full attempt"):
            self.verify()
        self.assertEqual(self.api.downloads, [])

    def test_failed_manual_selection_binds_candidate_not_workflow_head(self):
        failed = dict(self.green, id=43, run_attempt=1, event="workflow_dispatch", head_sha=B,
                      conclusion="failure", created_at="2026-10-30T11:00:00Z", updated_at="2026-10-30T11:00:00Z")
        self.api.runs = [self.green, failed]
        self.api.artifacts[43] = [{"id": 4301, "name": "ci-selection-43-1", "expired": False}]
        self.api.jobs[(43, 1)] = [dict(name="select", run_attempt=1, status="completed", conclusion="success")]
        selected = dict(scope="explicit-full", execution_profile="candidate-full",
                        source_sha=A, tested_sha=A, policy_sha=B)
        self.api.payloads[4301] = selected
        with self.assertRaises(ValueError):
            self.verify()
        self.assertEqual(self.api.downloads, [(4301, "selection.json")])
        # A known different candidate does not become a repository-wide gate.
        selected.update(source_sha="c" * 40, tested_sha="c" * 40)
        self.assertEqual(self.verify()["context"]["run_id"], 42)

    def test_no_run_and_exact_run_without_current_receipt_fail_closed(self):
        with self.assertRaisesRegex(ValueError, "no complete trusted full result"):
            module.find_verified(History([]), A, CONFIG, "definition")
        self.green["run_attempt"] = 3
        with self.assertRaisesRegex(ValueError, "current attempt"):
            module.find_verified(self.api, A, CONFIG, "definition", run_id=42)
        self.assertEqual(self.api.downloads, [])

    def test_selection_reuse_requires_successful_producer_and_no_current_select(self):
        run = dict(self.green, id=99, run_attempt=3, event="workflow_dispatch", head_sha=B)
        selection = {"id": 9902, "name": "ci-selection-99-2", "expired": False}
        producer = dict(name="select", run_attempt=2, status="completed", conclusion="success")
        api = History([run], payloads={9902: dict(scope="explicit-full", execution_profile="candidate-full",
                                                source_sha=A, tested_sha=A, policy_sha=B)},
                      jobs={(99, 2): [producer], (99, 3): [
                          dict(name="candidate-full-regression", run_attempt=3, status="completed", conclusion="failure")
                      ]})
        self.assertEqual(module.selected_source_for_attempt(api, run, [selection]), A)
        for conclusion in ("success", "failure", "cancelled"):
            api.jobs[(99, 3)] = [dict(producer, run_attempt=3, conclusion=conclusion)]
            with self.subTest(current_select=conclusion), self.assertRaisesRegex(ValueError, "select was rerun"):
                module.selected_source_for_attempt(api, run, [selection])
        api.jobs[(99, 3)] = []
        for producers in ([], [dict(producer, conclusion="failure")],
                          [dict(producer, status="in_progress", conclusion=None)], [producer, producer],
                          [dict(producer, run_attempt=1)]):
            api.jobs[(99, 2)] = producers
            with self.subTest(producer_jobs=producers), self.assertRaisesRegex(ValueError, "producing select"):
                module.selected_source_for_attempt(api, run, [selection])
        api.jobs[(99, 2)] = [producer]
        api.jobs[(99, 3)] = [dict(producer, run_attempt=3)]
        current = dict(selection, id=9903, name="ci-selection-99-3")
        api.payloads[9903] = dict(api.payloads[9902], source_sha="c" * 40, tested_sha="c" * 40)
        self.assertEqual(module.selected_source_for_attempt(api, run, [selection, current]), "c" * 40)
        with self.assertRaisesRegex(ValueError, "expired selection"):
            module.selected_source_for_attempt(api, run, [selection, dict(current, expired=True)])

    def test_malformed_selection_is_not_proof_of_an_unrelated_diagnostic(self):
        failed = dict(self.green, id=43, run_attempt=1, conclusion="failure",
                      created_at="2026-10-30T11:00:00Z", updated_at="2026-10-30T11:00:00Z")
        self.api.runs = [self.green, failed]
        self.api.artifacts[43] = [{"id": 4301, "name": "ci-selection-43-1", "expired": False}]
        self.api.jobs[(43, 1)] = [dict(name="select", run_attempt=1, status="completed", conclusion="success")]
        for selection in ({}, dict(scope="targeted", execution_profile="periodic-full",
                                   source_sha=A, tested_sha=A, policy_sha=A)):
            with self.subTest(selection=selection):
                self.api.payloads[4301] = selection
                with self.assertRaises(ValueError):
                    self.verify()


class PeriodicAdmission(unittest.TestCase):
    def setUp(self):
        self.now = datetime(2026, 10, 30, 12, tzinfo=timezone.utc)
        receipt, run, _ = fixture()
        self.green = dict(run, id=40, run_attempt=1, created_at="2026-10-30T08:00:00Z",
                          run_started_at="2026-10-30T08:00:00Z", updated_at="2026-10-30T10:00:00Z")
        receipt["context"].update(run_id=40, run_attempt=1)
        self.api = History([self.green],
                           artifacts={40: [{"id": 4001, "name": "ci-full-result-40-1", "expired": False}]},
                           payloads={4001: receipt}, jobs={(40, 1): [
                               self.job("select", "success", started_at=self.green["created_at"]),
                               self.job("explicit-full-gate", "success"),
                           ]})

    def job(self, name, conclusion, **extra):
        return dict(name=name, status="completed", conclusion=conclusion, run_attempt=1, **extra)

    def context(self, run):
        return dict(repository=self.api.repository, event_name="schedule", ref="refs/heads/main",
                    workflow_sha=run["head_sha"], source_sha=run["head_sha"],
                    workflow_ref=f"{self.api.repository}/{module.WORKFLOW}@refs/heads/main",
                    run_id=run["id"], run_attempt=run["run_attempt"])

    def add_run(self, run_id, when=None, **extra):
        when = when or self.now
        run = dict(self.green, id=run_id, created_at=when.isoformat(), run_started_at=when.isoformat(),
                   updated_at=(when + timedelta(seconds=5)).isoformat())
        run.update(extra)
        self.api.runs.append(run)
        return run

    def full(self, run):
        return module.classify_periodic_attempt(self.api, run, 7)

    def add_observation(self, run_id=41, when=None, previous=None):
        when = when or self.now - timedelta(minutes=20)
        run = self.add_run(run_id, when)
        history = [self.full(self.green)] if previous is None else previous
        decision = module.periodic_decision(history, self.context(run), when)
        self.api.jobs[(run_id, 1)] = [self.job("periodic-admission", "success"),
                                    self.job("select", "skipped"), self.job("explicit-full-gate", "skipped")]
        self.api.artifacts[run_id] = [{"id": run_id * 100, "name": f"ci-periodic-admission-{run_id}-1", "expired": False}]
        self.api.payloads[run_id * 100] = decision
        return run, decision

    def admit(self, now=None, run_id=90):
        now = now or self.now
        run = self.add_run(run_id, now, status="in_progress", conclusion=None)
        return module.admit_periodic(self.api, self.context(run), now)

    def test_first_start_and_exact_four_hour_due_keep_unchanged_source(self):
        self.assertTrue(self.admit()["run_full"])
        self.api.runs.remove(next(run for run in self.api.runs if run["id"] == 90))
        before = self.admit(self.now - timedelta(seconds=1), 91)
        self.assertFalse(before["run_full"])
        self.assertEqual(before["reason"], "not_due")
        self.api = History([])
        first = self.admit(run_id=92)
        self.assertTrue(first["run_full"])
        self.assertIsNone(first["previous_full"])

    def test_many_missed_intervals_admit_one_current_full_without_replaying_slots(self):
        decision = self.admit(self.now + timedelta(hours=12))
        self.assertTrue(decision["run_full"])
        self.assertEqual(decision["missed_intervals"], 3)
        self.assertEqual(decision["active_full_run_ids"], [])

    def test_failed_full_keeps_start_anchor_and_is_not_retried_on_each_tick(self):
        self.green.update(conclusion="failure")
        self.api.jobs[(40, 1)][1]["conclusion"] = "failure"
        self.assertFalse(self.admit(self.now - timedelta(minutes=20))["run_full"])
        self.api.runs = [self.green]
        decision = self.admit()
        self.assertTrue(decision["run_full"])
        self.assertEqual(decision["previous_full"]["conclusion"], "failure")

    def test_active_full_older_than_history_window_blocks_overlap(self):
        old = self.now - timedelta(days=3)
        self.green.update(created_at=old.isoformat(), run_started_at=old.isoformat(),
                          status="in_progress", conclusion=None)
        self.api.jobs[(40, 1)][0]["started_at"] = old.isoformat()
        decision = self.admit()
        self.assertFalse(decision["run_full"])
        self.assertEqual(decision["reason"], "full_active")
        self.assertEqual(decision["active_full_run_ids"], [40])

    def test_published_admission_reserves_slot_before_select_starts(self):
        first = self.admit()
        self.api.jobs[(90, 1)] = [self.job("periodic-admission", "success")]
        self.api.artifacts[90] = [{"id": 9000, "name": "ci-periodic-admission-90-1", "expired": False}]
        self.api.payloads[9000] = first
        second = self.admit(self.now + timedelta(minutes=20), 91)
        self.assertFalse(second["run_full"])
        self.assertEqual(second["active_full_run_ids"], [90])
        self.assertEqual(second["previous_full"]["reserved_at"], self.now.isoformat())
        self.assertIsNone(second["previous_full"]["full_started_at"])

    def test_queued_observation_without_mutex_does_not_deadlock_admission(self):
        queued = self.add_run(41, self.now - timedelta(seconds=1), status="queued", conclusion=None)
        self.api.jobs[(41, 1)] = [dict(name="periodic-admission", status="queued", conclusion=None,
                                      run_attempt=1, started_at=None)]
        self.assertEqual(self.full(queued)["kind"], "pending_observation")
        self.assertTrue(self.admit()["run_full"])

    def test_unpublished_running_or_successful_admission_cannot_release_a_slot(self):
        for status, conclusion in (("in_progress", None), ("completed", "success")):
            with self.subTest(status=status):
                self.api.runs = [self.green]
                self.add_run(41, self.now - timedelta(minutes=20), status="in_progress", conclusion=None)
                self.api.jobs[(41, 1)] = [dict(name="periodic-admission", status=status, conclusion=conclusion,
                                              run_attempt=1, started_at="2026-10-30T11:40:00Z")]
                with self.assertRaisesRegex(ValueError, "admission evidence"):
                    self.admit()

    def test_failed_upload_remains_control_failure_but_next_due_tick_can_compensate(self):
        failed = self.add_run(41, self.now - timedelta(minutes=20), conclusion="failure")
        self.api.jobs[(41, 1)] = [self.job("periodic-admission", "failure"),
                                self.job("select", "skipped"), self.job("explicit-full-gate", "skipped")]
        self.assertEqual(self.full(failed)["kind"], "control_failure")
        decision = self.admit()
        self.assertTrue(decision["run_full"])
        self.assertEqual(decision["control_failure_run_ids"], [41])
        self.api.runs = [self.green, failed]
        with self.assertRaisesRegex(ValueError, "newer full attempt"):
            module.find_verified(self.api, A, CONFIG, "definition")

    def test_noop_requires_actual_skips_and_no_full_execution_artifacts(self):
        for kind in ("select_ran", "gate_ran", "other_ran", "missing_select", "missing_gate",
                     "duplicate_select", "old_select", "selection", "full-result", "unknown_run"):
            with self.subTest(kind=kind):
                run, _ = self.add_observation()
                jobs = self.api.jobs[(41, 1)]
                if kind in {"select_ran", "gate_ran"}:
                    jobs[1 if kind == "select_ran" else 2]["conclusion"] = "success"
                elif kind == "other_ran":
                    jobs.append(self.job("full-regression", "success"))
                elif kind in {"missing_select", "missing_gate"}:
                    del jobs[1 if kind == "missing_select" else 2]
                elif kind == "duplicate_select":
                    jobs.append(dict(jobs[1]))
                elif kind == "old_select":
                    jobs[1]["run_attempt"] = 0
                elif kind in {"selection", "full-result"}:
                    self.api.artifacts[41].append({"id": 4199, "name": f"ci-{kind}-41-1", "expired": False})
                else:
                    run["conclusion"] = "unknown"
                with self.assertRaises(ValueError):
                    self.full(run)
                self.api.runs.remove(run)

    def test_noop_rejects_missing_expired_duplicate_or_cross_attempt_decisions(self):
        for kind in ("missing", "expired", "duplicate", "old_attempt", "job_failure", "job_missing", "duplicate_job"):
            with self.subTest(kind=kind):
                run, _ = self.add_observation()
                artifacts = self.api.artifacts[41]
                if kind == "missing":
                    artifacts.clear()
                elif kind == "expired":
                    artifacts[0]["expired"] = True
                elif kind == "duplicate":
                    artifacts.append(dict(artifacts[0], id=4199))
                elif kind == "old_attempt":
                    run["run_attempt"] = 2
                    self.api.jobs[(41, 2)] = [dict(job, run_attempt=2) for job in self.api.jobs[(41, 1)]]
                elif kind == "job_failure":
                    self.api.jobs[(41, 1)][0]["conclusion"] = "failure"
                elif kind == "job_missing":
                    del self.api.jobs[(41, 1)][0]
                else:
                    self.api.jobs[(41, 1)].append(dict(self.api.jobs[(41, 1)][0]))
                if kind == "job_failure":
                    self.assertEqual(self.full(run)["kind"], "control_failure")
                else:
                    with self.assertRaises(ValueError):
                        self.full(run)
                self.api.runs.remove(run)

    def test_decision_binds_actual_context_schema_and_cadence(self):
        mutations = [("schema", "unknown"), ("run_full", "false"), ("cadence_seconds", 1200),
                     ("reason", "due"), ("observed_at", "2026-10-29T00:00:00Z"),
                     ("start_gap_seconds", -1), ("missed_intervals", 3), ("previous_full", None),
                     ("active_full_run_ids", [41]), ("control_failure_run_ids", [True])]
        for key, value in mutations:
            with self.subTest(field=key):
                run, receipt = self.add_observation()
                receipt[key] = value
                with self.assertRaises(ValueError):
                    self.full(run)
                self.api.runs.remove(run)
        for key, value in (("run_id", 99), ("run_attempt", 2), ("source_sha", B), ("workflow_sha", B),
                           ("ref", "refs/heads/candidate"), ("event_name", "workflow_dispatch"),
                           ("workflow_ref", "nil-studio/oasis7/other.yml@refs/heads/main"),
                           ("repository", "other/repo")):
            with self.subTest(context=key):
                run, receipt = self.add_observation()
                receipt["context"][key] = value
                with self.assertRaises(ValueError):
                    self.full(run)
                self.api.runs.remove(run)

    def test_only_proven_noop_can_be_ignored_by_release_qualification(self):
        run, _ = self.add_observation()
        receipt = module.find_verified(self.api, A, CONFIG, "definition")
        self.assertEqual(receipt["context"]["run_id"], 40)
        self.assertEqual(module.selected_source_for_attempt(self.api, run, self.api.artifacts[41], 7), "")
        with self.assertRaisesRegex(ValueError, "current attempt"):
            module.find_verified(self.api, A, CONFIG, "definition", run_id=41)
        query = next(urllib.parse.parse_qs(urllib.parse.urlsplit(path).query) for path in self.api.queries
                     if "workflow_runs" not in path and "head_sha=" in path)
        self.assertEqual(query["head_sha"], [A])
        self.api.artifacts[41].clear()
        with self.assertRaisesRegex(ValueError, "admission evidence"):
            module.find_verified(self.api, A, CONFIG, "definition")

    def test_observation_green_cannot_clear_full_failure_or_refresh_full_start(self):
        failed = self.add_run(41, self.now - timedelta(hours=3), conclusion="failure",
                              updated_at="2026-10-30T11:00:00Z")
        self.api.jobs[(41, 1)] = [self.job("select", "success", started_at=failed["created_at"]),
                                self.job("explicit-full-gate", "failure")]
        observation, _ = self.add_observation(42, previous=[self.full(failed)])
        with self.assertRaisesRegex(ValueError, "scheduled full did not succeed") as caught:
            module.periodic_health(self.api, self.now)
        report = json.loads(str(caught.exception).split("; schedule facts: ", 1)[1])
        self.assertEqual(report["latest_start"]["run_id"], 41)
        self.assertEqual(report["latest_terminal"]["run_id"], 41)
        self.assertEqual(report["last_successful_run"]["run_id"], 40)
        self.assertEqual(report["latest_observation"]["run_id"], observation["id"])

    def test_admission_failure_is_visible_and_noop_does_not_restore_health(self):
        failed = self.add_run(41, self.now - timedelta(minutes=40), conclusion="failure")
        self.api.jobs[(41, 1)] = [self.job("periodic-admission", "failure"),
                                self.job("select", "skipped"), self.job("explicit-full-gate", "skipped")]
        self.add_observation(42)
        with self.assertRaisesRegex(ValueError, "periodic admission failed") as caught:
            module.periodic_health(self.api, self.now)
        report = json.loads(str(caught.exception).split("; schedule facts: ", 1)[1])
        self.assertEqual(report["control_failure_run_ids"], [failed["id"]])
        self.assertEqual(report["last_successful_sha"], A)

    def test_success_needs_actual_gate_and_current_full_artifact(self):
        self.api.artifacts[40].clear()
        with self.assertRaisesRegex(ValueError, "did not succeed"):
            module.periodic_health(self.api, self.now)

    def test_reserved_slot_does_not_claim_a_full_has_started(self):
        receipt = self.admit()
        self.api.jobs[(90, 1)] = [self.job("periodic-admission", "success")]
        self.api.artifacts[90] = [{"id": 9000, "name": "ci-periodic-admission-90-1", "expired": False}]
        self.api.payloads[9000] = receipt
        report = module.periodic_health(self.api, self.now + timedelta(minutes=20))
        self.assertEqual(report["latest_start"]["run_id"], 40)
        self.assertEqual(report["start_age_seconds"], 4 * 3600 + 20 * 60)
        self.assertEqual(report["reserved_full_run_ids"], [90])

    def test_partial_rerun_cannot_reuse_old_noop_proof_or_clear_an_old_failure(self):
        run, _ = self.add_observation()
        run.update(run_attempt=2, conclusion="failure", run_started_at="2026-10-30T11:50:00Z",
                   updated_at="2026-10-30T11:55:00Z")
        self.api.jobs[(41, 2)] = [dict(self.job("full-regression", "failure", started_at="2026-10-30T11:50:00Z"),
                                     run_attempt=2)]
        classified = self.full(run)
        self.assertEqual(classified["kind"], "full")
        self.assertEqual(classified["full_started_at"], "2026-10-30T11:50:00Z")
        with self.assertRaisesRegex(ValueError, "newer full attempt"):
            module.find_verified(self.api, A, CONFIG, "definition")

    def test_cancelled_admission_can_be_compensated_without_hiding_cancellation(self):
        run = self.add_run(41, self.now - timedelta(minutes=20), conclusion="cancelled")
        self.api.jobs[(41, 1)] = [self.job("periodic-admission", "skipped"),
                                self.job("select", "skipped"), self.job("explicit-full-gate", "skipped")]
        self.assertEqual(self.full(run)["kind"], "control_failure")
        self.assertTrue(self.admit()["run_full"])

    def test_failed_noop_after_successful_admission_allows_later_due_ticks(self):
        for conclusion in ("cancelled", "failure"):
            with self.subTest(conclusion=conclusion):
                self.setUp()
                run, _ = self.add_observation()
                run["conclusion"] = conclusion
                classified = self.full(run)
                self.assertEqual(classified["kind"], "control_failure")
                self.assertFalse(classified["successful"])
                self.assertIsNone(classified["full_started_at"])
                self.assertIsNone(classified["reserved_at"])
                for index, seconds in enumerate((-1, 0, 1200)):
                    self.api.runs = [self.green, run]
                    decision = self.admit(self.now + timedelta(seconds=seconds), 90 + index)
                    self.assertEqual(decision["run_full"], seconds >= 0)
                    self.assertEqual(decision["previous_full"]["id"], self.green["id"])
                    self.assertEqual(decision["control_failure_run_ids"], [run["id"]])
                self.api.runs = [self.green, run]
                with self.assertRaisesRegex(ValueError, "periodic admission failed") as caught:
                    module.periodic_health(self.api, self.now)
                report = json.loads(str(caught.exception).split("; schedule facts: ", 1)[1])
                self.assertEqual(report["control_failure_run_ids"], [run["id"]])
                self.assertEqual(report["last_successful_run"]["run_id"], self.green["id"])
                self.assertEqual(module.selected_source_for_attempt(self.api, run, self.api.artifacts[41], 7), A)
                with self.assertRaisesRegex(ValueError, "newer full attempt"):
                    module.find_verified(self.api, A, CONFIG, "definition")

    def test_cancelled_noop_recovery_cannot_ignore_incomplete_or_conflicting_evidence(self):
        for kind in ("active", "unknown_conclusion", "select_cancelled", "other_ran", "selection", "full-result",
                     "missing_decision", "wrong_attempt", "wrong_source"):
            with self.subTest(kind=kind):
                self.setUp()
                run, receipt = self.add_observation()
                run["conclusion"] = "cancelled"
                if kind == "active":
                    run["status"] = "in_progress"
                elif kind == "unknown_conclusion":
                    run["conclusion"] = "timed_out"
                elif kind == "select_cancelled":
                    self.api.jobs[(41, 1)][1]["conclusion"] = "cancelled"
                elif kind == "other_ran":
                    self.api.jobs[(41, 1)].append(self.job("full-regression", "success"))
                elif kind in {"selection", "full-result"}:
                    self.api.artifacts[41].append({"id": 4199, "name": f"ci-{kind}-41-1", "expired": False})
                elif kind == "missing_decision":
                    self.api.artifacts[41].clear()
                elif kind == "wrong_attempt":
                    receipt["context"]["run_attempt"] = 2
                else:
                    receipt["context"]["source_sha"] = B
                with self.assertRaises(ValueError):
                    self.admit()

    def test_later_complete_full_restores_health_after_control_failure(self):
        failed = self.add_run(41, self.now - timedelta(hours=2), conclusion="failure")
        self.api.jobs[(41, 1)] = [self.job("periodic-admission", "failure"),
                                self.job("select", "skipped"), self.job("explicit-full-gate", "skipped")]
        recovered = self.add_run(42, self.now - timedelta(hours=1), updated_at="2026-10-30T11:59:00Z")
        self.api.jobs[(42, 1)] = [self.job("select", "success", started_at=recovered["created_at"]),
                                self.job("explicit-full-gate", "success")]
        self.api.artifacts[42] = [{"id": 4200, "name": "ci-full-result-42-1", "expired": False}]
        self.assertEqual(module.periodic_health(self.api, self.now)["last_successful_run"]["run_id"], 42)

    def test_actual_start_order_wins_over_completion_order(self):
        self.green["updated_at"] = "2026-10-30T11:30:00Z"
        newer = self.add_run(41, self.now - timedelta(hours=2), updated_at="2026-10-30T11:00:00Z")
        self.api.jobs[(41, 1)] = [self.job("select", "success", started_at=newer["created_at"]),
                                self.job("explicit-full-gate", "failure")]
        decision = self.admit()
        self.assertFalse(decision["run_full"])
        self.assertEqual(decision["previous_full"]["id"], 41)

    def test_old_created_completed_rerun_failure_is_not_hidden_by_recent_green(self):
        old = self.add_run(41, self.now - timedelta(days=3), run_attempt=2, conclusion="failure",
                           run_started_at="2026-10-30T11:00:00Z", updated_at="2026-10-30T11:30:00Z")
        self.api.jobs[(41, 2)] = [dict(self.job("select", "success", started_at="2026-10-30T11:00:00Z"), run_attempt=2),
                                 dict(self.job("explicit-full-gate", "failure"), run_attempt=2)]
        with self.assertRaisesRegex(ValueError, "did not succeed") as caught:
            module.periodic_health(self.api, self.now)
        report = json.loads(str(caught.exception).split("; schedule facts: ", 1)[1])
        self.assertEqual(report["latest_terminal"]["run_id"], old["id"])
        self.assertEqual(report["last_successful_run"]["run_id"], 40)
        decision = self.admit()
        self.assertFalse(decision["run_full"])
        self.assertEqual(decision["previous_full"]["id"], old["id"])
        self.assertEqual(decision["previous_full"]["run_attempt"], 2)

    def test_old_created_successful_rerun_updates_current_start_and_success(self):
        self.green["conclusion"] = "failure"
        old = self.add_run(41, self.now - timedelta(days=31), head_sha=B, run_attempt=2, conclusion="success",
                           run_started_at="2026-10-29T11:00:00Z", updated_at="2026-10-30T11:30:00Z")
        self.api.jobs[(41, 2)] = [dict(self.job("select", "success", started_at="2026-10-29T11:00:00Z"), run_attempt=2),
                                 dict(self.job("explicit-full-gate", "success"), run_attempt=2)]
        self.api.artifacts[41] = [{"id": 4102, "name": "ci-full-result-41-2", "expired": False}]
        # The rerun can start within GitHub's 30-day eligibility and finish in
        # the two-day observation window even though creation is 31 days old.
        history, _, _ = module.periodic_history(self.api, self.now)
        self.assertTrue(next(item for item in history if item["run"]["id"] == old["id"])["successful"])
        old["created_at"] = (self.now - timedelta(days=3)).isoformat()
        old["run_started_at"] = "2026-10-30T11:00:00Z"
        self.api.jobs[(41, 2)][0]["started_at"] = old["run_started_at"]
        report = module.periodic_health(self.api, self.now)
        self.assertEqual(report["last_successful_sha"], B)
        self.assertEqual(report["latest_start"]["run_id"], 41)

    def test_history_page_overlap_requires_immutable_identity_and_attempt(self):
        original = self.api.pages
        for mutation in ({}, {"run_attempt": 2}, {"head_sha": B}, {"created_at": "2026-10-30T07:00:00Z"}):
            def overlapping(path, key):
                result = original(path, key)
                if "created=" in path and ".." in path:
                    result = [dict(self.green, **mutation)]
                return result
            with self.subTest(mutation=mutation), mock.patch.object(self.api, "pages", side_effect=overlapping):
                if mutation:
                    with self.assertRaisesRegex(ValueError, "attempt changed"):
                        module.periodic_history(self.api, self.now)
                else:
                    history, _, _ = module.periodic_history(self.api, self.now)
                    self.assertEqual([item["run"]["id"] for item in history], [40])

    def test_old_workflow_that_finishes_within_35_days_is_kept_in_two_day_window(self):
        self.api.runs = []
        created = self.now - timedelta(days=36)
        started = self.now - timedelta(days=2)
        finished = self.now - timedelta(days=1, hours=1)
        old = self.add_run(41, created, conclusion="failure", run_started_at=created.isoformat(),
                           updated_at=finished.isoformat())
        self.api.jobs[(41, 1)] = [self.job("select", "success", started_at=started.isoformat()),
                                self.job("explicit-full-gate", "failure")]
        # Total workflow age at completion is 34d23h; the terminal result is
        # recent even though neither a two-day nor a 32-day created query sees it.
        history, _, _ = module.periodic_history(self.api, self.now)
        self.assertEqual([item["run"]["id"] for item in history], [old["id"]])
        decision = self.admit()
        self.assertTrue(decision["run_full"])
        self.assertEqual(decision["previous_full"]["id"], old["id"])

    def test_two_days_of_noops_and_full_37_day_metadata_fit_the_request_budget(self):
        old = self.now - timedelta(days=3)
        self.green.update(created_at=old.isoformat(), run_started_at=old.isoformat(),
                          updated_at=(self.now - timedelta(seconds=1)).isoformat())
        self.api.jobs[(40, 1)][0]["started_at"] = old.isoformat()
        active = self.full(self.green)
        active["run"] = dict(self.green, status="in_progress", conclusion=None)
        for index in range(144):
            self.add_observation(100 + index, self.now - timedelta(minutes=20 * (index + 1)), previous=[active])
        for index in range(144, 37 * 72):
            self.add_run(1000 + index, self.now - timedelta(minutes=20 * (index + 1)))
        class MeteredHistory(History):
            def __init__(self, history):
                super().__init__(history.runs, history.artifacts, history.payloads, history.jobs)
                self.requests, self.lock = 0, threading.Lock()
            def charge(self, amount):
                with self.lock:
                    self.requests += amount
                    if self.requests > module.MAX_API_REQUESTS:
                        raise ValueError("API verification budget exhausted; history is unknown")
            def get(self, path):
                self.charge(1)
                return super().get(path)
            def pages(self, path, key):
                result = super().pages(path, key)
                self.charge(max(1, (len(result) + 99) // 100))
                return result
            def artifact(self, artifact_id, filename="full-result.json"):
                self.charge(2)  # API redirect plus token-free storage download.
                return super().artifact(artifact_id, filename)
        self.api = MeteredHistory(self.api)
        self.assertTrue(self.admit()["run_full"])
        self.assertGreater(self.api.requests, 400)
        self.assertLess(self.api.requests, module.MAX_API_REQUESTS)

    def test_untrusted_workflow_or_current_context_fails_before_writing_a_decision(self):
        run = self.add_run(90, status="in_progress", conclusion=None)
        for field, value in (("path", ".github/workflows/other.yml"), ("workflow_id", 8),
                             ("head_branch", "candidate"), ("event", "workflow_dispatch")):
            original = run[field]
            run[field] = value
            with self.subTest(field=field), self.assertRaises(ValueError):
                module.admit_periodic(self.api, self.context(run), self.now)
            run[field] = original
        context = dict(self.context(run), source_sha=B)
        with self.assertRaises(ValueError):
            module.admit_periodic(self.api, context, self.now)

    def test_cli_writes_decision_before_exposing_boolean_and_never_outputs_on_api_failure(self):
        run = self.add_run(90, status="in_progress", conclusion=None)
        context = self.context(run)
        environment = {"GITHUB_" + key.upper(): str(value) for key, value in context.items()}
        environment["GITHUB_SHA"] = environment.pop("GITHUB_SOURCE_SHA")
        environment["GH_TOKEN"] = "test-only"
        with tempfile.TemporaryDirectory() as directory:
            output, github_output = Path(directory) / "admission.json", Path(directory) / "outputs"
            argv = ["ci-full-result.py", "admit-periodic", "--output", str(output), "--github-output", str(github_output)]
            with mock.patch.dict(os.environ, environment), mock.patch.object(sys, "argv", argv), \
                 mock.patch.object(module, "GitHub", return_value=self.api), mock.patch("builtins.print"), \
                 mock.patch.object(module, "datetime", wraps=datetime) as clock:
                clock.now.return_value = self.now
                module.main()
                self.assertEqual(github_output.read_text(), "run_full=true\n")
                self.assertTrue(json.loads(output.read_text())["run_full"])
                output.unlink()
                github_output.unlink()
                with mock.patch.object(self.api, "pages", side_effect=OSError("history unavailable")):
                    with self.assertRaisesRegex(OSError, "history unavailable"):
                        module.main()
                self.assertFalse(output.exists())
                self.assertFalse(github_output.exists())


class ReadRetries(unittest.TestCase):
    def error(self, api, code, path="actions/workflows/rust.yml"):
        return urllib.error.HTTPError(api.root + path, code, "fixture HTTP failure", {}, io.BytesIO(b"failure"))

    def test_transient_get_recovers_and_counts_every_attempt(self):
        for code in (502, 503, 504):
            for failures in (1, 2):
                with self.subTest(code=code, failures=failures):
                    api = module.GitHub("nil-studio/oasis7", "test-only")
                    deadline = api.deadline
                    errors = [self.error(api, code) for _ in range(failures)]
                    with mock.patch.object(api.opener, "open", side_effect=errors + [Response({"id": 7})]) as opened, \
                         mock.patch.object(module.time, "sleep") as slept:
                        self.assertEqual(api.get("actions/workflows/rust.yml"), {"id": 7})
                    self.assertEqual(api.requests, failures + 1)
                    self.assertEqual(opened.call_count, failures + 1)
                    self.assertEqual(slept.call_args_list, [mock.call(0.5), mock.call(1.0)][:failures])
                    self.assertEqual(api.deadline, deadline)
                    self.assertTrue(all(error.fp.closed for error in errors))
                    for call in opened.call_args_list:
                        self.assertEqual(call.args[0].get_method(), "GET")
                        self.assertEqual(call.args[0].full_url, api.root + "actions/workflows/rust.yml")

    def test_persistent_transient_failure_stops_after_three_attempts(self):
        for code in (502, 503, 504):
            with self.subTest(code=code):
                api = module.GitHub("nil-studio/oasis7", "test-only")
                errors = [self.error(api, code) for _ in range(3)]
                with mock.patch.object(api.opener, "open", side_effect=errors + [Response({"id": 7})]) as opened, \
                     mock.patch.object(module.time, "sleep") as slept:
                    with self.assertRaises(urllib.error.HTTPError) as caught:
                        api.get("actions/workflows/rust.yml")
                self.assertIs(caught.exception, errors[-1])
                self.assertEqual(api.requests, 3)
                self.assertEqual(opened.call_count, 3)
                self.assertEqual(slept.call_args_list, [mock.call(0.5), mock.call(1.0)])

    def test_other_http_transport_and_assertion_errors_are_not_retried(self):
        api = module.GitHub("nil-studio/oasis7", "test-only")
        failures = [self.error(api, code) for code in (301, 302, 307, 400, 401, 403, 404, 408, 429, 500, 501)]
        failures += [urllib.error.URLError("unknown transport failure"), TimeoutError("read timed out"),
                     OSError("unknown read failure"), AssertionError("contract failed")]
        for failure in failures:
            with self.subTest(failure=str(failure)):
                before = api.requests
                with mock.patch.object(api.opener, "open", side_effect=failure) as opened, \
                     mock.patch.object(module.time, "sleep") as slept:
                    with self.assertRaises(type(failure)) as caught:
                        api.get("actions/workflows/rust.yml")
                self.assertIs(caught.exception, failure)
                self.assertEqual(api.requests, before + 1)
                opened.assert_called_once()
                slept.assert_not_called()

    def test_malformed_or_incomplete_success_responses_are_not_retried(self):
        for response, collection in ((io.BytesIO(b"{"), False),
                                      (io.BytesIO(b'{"id":7,"id":8}'), False),
                                      (Response({"total_count": 1}), True),
                                      (Response({"total_count": 2, "jobs": [{"id": 1}]}), True)):
            with self.subTest(collection=collection):
                api = module.GitHub("nil-studio/oasis7", "test-only")
                with mock.patch.object(api.opener, "open", return_value=response) as opened, \
                     mock.patch.object(module.time, "sleep") as slept:
                    with self.assertRaises(ValueError):
                        api.pages("jobs", "jobs") if collection else api.get("workflow")
                self.assertEqual(api.requests, 1)
                opened.assert_called_once()
                slept.assert_not_called()

    def test_retry_cannot_reset_exhausted_request_budget(self):
        api = module.GitHub("nil-studio/oasis7", "test-only")
        api.requests = module.MAX_API_REQUESTS - 1
        with mock.patch.object(api.opener, "open", side_effect=self.error(api, 503)) as opened, \
             mock.patch.object(module.time, "sleep") as slept:
            with self.assertRaisesRegex(ValueError, "budget exhausted; history is unknown"):
                api.get("actions/workflows/rust.yml")
        self.assertEqual(api.requests, module.MAX_API_REQUESTS)
        opened.assert_called_once()
        slept.assert_not_called()

    def test_retry_backoff_must_fit_remaining_shared_deadline(self):
        api = module.GitHub("nil-studio/oasis7", "test-only")
        now = [api.deadline - 5]
        def failed(*args, **kwargs):
            now[0] = api.deadline - 0.25
            raise self.error(api, 502)
        with mock.patch.object(module.time, "monotonic", side_effect=lambda: now[0]), \
             mock.patch.object(api.opener, "open", side_effect=failed) as opened, \
             mock.patch.object(module.time, "sleep") as slept:
            with self.assertRaisesRegex(ValueError, "budget exhausted; history is unknown"):
                api.get("actions/workflows/rust.yml")
        opened.assert_called_once()
        self.assertEqual(api.requests, 1)
        slept.assert_not_called()

    def test_retry_timeout_uses_original_deadline_after_backoff(self):
        api = module.GitHub("nil-studio/oasis7", "test-only")
        now = [api.deadline - 2]
        deadline = api.deadline
        def delay(seconds):
            now[0] += seconds
        with mock.patch.object(module.time, "monotonic", side_effect=lambda: now[0]), \
             mock.patch.object(api.opener, "open", side_effect=[self.error(api, 504), Response({"id": 7})]) as opened, \
             mock.patch.object(module.time, "sleep", side_effect=delay):
            self.assertEqual(api.get("actions/workflows/rust.yml"), {"id": 7})
        self.assertEqual([call.kwargs["timeout"] for call in opened.call_args_list], [2, 1.5])
        self.assertEqual(api.deadline, deadline)
        self.assertEqual(api.requests, 2)

    def test_retry_rechecks_deadline_after_wait_before_opening(self):
        api = module.GitHub("nil-studio/oasis7", "test-only")
        now = [api.deadline - 2]
        def delayed_wakeup(seconds):
            now[0] = api.deadline
        with mock.patch.object(module.time, "monotonic", side_effect=lambda: now[0]), \
             mock.patch.object(api.opener, "open", side_effect=self.error(api, 502)) as opened, \
             mock.patch.object(module.time, "sleep", side_effect=delayed_wakeup) as slept:
            with self.assertRaisesRegex(ValueError, "budget exhausted; history is unknown"):
                api.get("actions/workflows/rust.yml")
        opened.assert_called_once()
        slept.assert_called_once_with(0.5)
        self.assertEqual(api.requests, 1)

    def test_parallel_retries_share_budget_without_holding_its_lock_during_io(self):
        api = module.GitHub("nil-studio/oasis7", "test-only")
        api.requests = module.MAX_API_REQUESTS - 6
        barrier, lock, attempts = threading.Barrier(4), threading.Lock(), {}
        def failed(request, **kwargs):
            with lock:
                attempts[request.full_url] = attempts.get(request.full_url, 0) + 1
                first = attempts[request.full_url] == 1
            if first:
                barrier.wait(timeout=5)
            raise self.error(api, 503)
        def read(index):
            try:
                api.get(f"actions/runs/{index}")
                return True
            except (ValueError, urllib.error.HTTPError):
                return False
        with mock.patch.object(api.opener, "open", side_effect=failed) as opened, \
             mock.patch.object(module.time, "sleep"), module.ThreadPoolExecutor(max_workers=4) as reader:
            self.assertEqual(list(reader.map(read, range(4))), [False] * 4)
        self.assertEqual(api.requests, module.MAX_API_REQUESTS)
        self.assertEqual(opened.call_count, 6)
        self.assertEqual(len(attempts), 4)
        self.assertLessEqual(max(attempts.values()), 3)

    def test_later_page_can_recover_without_restarting_complete_earlier_pages(self):
        api = module.GitHub("nil-studio/oasis7", "test-only")
        responses = [Response({"total_count": 2, "jobs": [{"id": 1}]},
                              f'<{api.root}page2>; rel="next"'),
                     self.error(api, 503, "page2"), Response({"total_count": 2, "jobs": [{"id": 2}]})]
        with mock.patch.object(api.opener, "open", side_effect=responses) as opened, \
             mock.patch.object(module.time, "sleep"):
            self.assertEqual(api.pages("page1", "jobs"), [{"id": 1}, {"id": 2}])
        self.assertEqual([call.args[0].full_url for call in opened.call_args_list],
                         [api.root + path for path in ("page1", "page2", "page2")])
        self.assertEqual(api.requests, 3)

    def test_failed_later_page_never_qualifies_a_green_prefix(self):
        api = module.GitHub("nil-studio/oasis7", "test-only")
        _, green, _ = fixture()
        responses = [Response({"id": 7}),
                     Response({"total_count": 2, "workflow_runs": [green]}, f'<{api.root}page2>; rel="next"'),
                     *[self.error(api, 502, "page2") for _ in range(3)]]
        with mock.patch.object(api.opener, "open", side_effect=responses) as opened, \
             mock.patch.object(module.time, "sleep"), mock.patch.object(api, "artifact") as download:
            with self.assertRaises(urllib.error.HTTPError):
                module.find_verified(api, A, CONFIG, "definition")
        download.assert_not_called()
        self.assertEqual(opened.call_count, 5)
        self.assertEqual(api.requests, 5)
        self.assertEqual([call.args[0].full_url for call in opened.call_args_list[-3:]], [api.root + "page2"] * 3)

    def test_retry_failure_cannot_publish_admission_files_or_success_output(self):
        receipt, _, _ = fixture()
        environment = {"GITHUB_" + key.upper(): str(value) for key, value in receipt["context"].items()}
        environment.update(GITHUB_SHA=A, GH_TOKEN="test-only")
        for exhausted in (False, True):
            with self.subTest(exhausted=exhausted), tempfile.TemporaryDirectory() as directory:
                api = module.GitHub("nil-studio/oasis7", "test-only")
                if exhausted:
                    api.requests = module.MAX_API_REQUESTS - 1
                output, github_output = Path(directory) / "admission.json", Path(directory) / "outputs"
                argv = ["ci-full-result.py", "admit-periodic", "--output", str(output), "--github-output", str(github_output)]
                with mock.patch.dict(os.environ, environment), mock.patch.object(sys, "argv", argv), \
                     mock.patch.object(module, "GitHub", return_value=api), \
                     mock.patch.object(api.opener, "open", side_effect=[self.error(api, 502) for _ in range(3)]), \
                     mock.patch.object(module.time, "sleep"), mock.patch("builtins.print") as printed:
                    with self.assertRaises(ValueError if exhausted else urllib.error.HTTPError):
                        module.main()
                self.assertFalse(output.exists())
                self.assertFalse(github_output.exists())
                printed.assert_not_called()
                self.assertEqual(api.requests, module.MAX_API_REQUESTS if exhausted else 3)

    def test_artifact_storage_transient_failure_is_not_retried(self):
        api = module.GitHub("nil-studio/oasis7", "test-only")
        location = "https://artifacts.example.invalid/receipt.zip?signature=fixture"
        redirect = urllib.error.HTTPError(api.root, 302, "Found", {"Location": location}, None)
        storage_error = urllib.error.HTTPError(location, 503, "fixture storage failure", {}, None)
        with mock.patch.object(api.opener, "open", side_effect=redirect) as authenticated, \
             mock.patch.object(module.urllib.request, "urlopen", side_effect=storage_error) as storage, \
             mock.patch.object(module.time, "sleep") as slept:
            with self.assertRaises(urllib.error.HTTPError) as caught:
                api.artifact(9)
        self.assertIs(caught.exception, storage_error)
        authenticated.assert_called_once()
        storage.assert_called_once()
        slept.assert_not_called()
        self.assertEqual(api.requests, 2)

    def test_real_http_get_recovers_from_transient_status_and_keeps_identity(self):
        observed = []
        class Handler(BaseHTTPRequestHandler):
            def do_GET(self):
                observed.append((self.command, self.path, self.headers.get("Authorization")))
                status = (502, 504, 200)[min(len(observed) - 1, 2)]
                body = b'{"id":7}' if status == 200 else b"temporary failure"
                self.send_response(status)
                self.send_header("Content-Length", str(len(body)))
                self.end_headers()
                self.wfile.write(body)
            def log_message(self, *args):
                pass
        server = HTTPServer(("127.0.0.1", 0), Handler)
        thread = threading.Thread(target=server.serve_forever, kwargs={"poll_interval": 0.01}, daemon=True)
        thread.start()
        try:
            api = module.GitHub("nil-studio/oasis7", "test-only")
            api.root = f"http://127.0.0.1:{server.server_port}/"
            api.opener = urllib.request.build_opener(urllib.request.ProxyHandler({}), module.NoRedirect)
            self.assertEqual(api.get("actions/workflows/rust.yml"), {"id": 7})
            self.assertEqual(api.requests, 3)
            self.assertEqual(observed, [("GET", "/actions/workflows/rust.yml", "Bearer test-only")] * 3)
        finally:
            server.shutdown()
            thread.join(timeout=5)
            server.server_close()
        self.assertFalse(thread.is_alive())


class Collections(unittest.TestCase):
    def test_read_budget_bounds_requests_and_total_wall_time(self):
        api = module.GitHub("nil-studio/oasis7", "test-only")
        with mock.patch.object(api.opener, "open") as opened:
            api.requests = module.MAX_API_REQUESTS
            with self.assertRaisesRegex(ValueError, "budget exhausted"):
                api.request("actions/workflows/rust.yml")
            opened.assert_not_called()
            api.requests = 0
            with mock.patch.object(module.time, "monotonic", return_value=api.deadline + 1):
                with self.assertRaisesRegex(ValueError, "budget exhausted"):
                    api.request("actions/workflows/rust.yml")
            opened.assert_not_called()

    def test_parallel_readers_share_one_request_budget(self):
        api = module.GitHub("nil-studio/oasis7", "test-only")
        api.requests = module.MAX_API_REQUESTS - 3
        def read():
            try:
                api.request_timeout()
                return True
            except ValueError:
                return False
        with module.ThreadPoolExecutor(max_workers=4) as reader:
            results = list(reader.map(lambda _: read(), range(20)))
        self.assertEqual(sum(results), 3)
        self.assertEqual(api.requests, module.MAX_API_REQUESTS)

    def test_response_that_crosses_total_deadline_cannot_be_used(self):
        api = module.GitHub("nil-studio/oasis7", "test-only")
        with mock.patch.object(api, "request", return_value=Response({"id": 7})), \
             mock.patch.object(module.time, "monotonic", return_value=api.deadline + 1):
            with self.assertRaisesRegex(ValueError, "budget exhausted"):
                api.get("actions/workflows/rust.yml")

    def test_total_count_changes_cannot_publish_a_partial_history(self):
        api = module.GitHub("nil-studio/oasis7", "test-only")
        pages = [Response({"total_count": 2, "workflow_runs": [{"id": 1}]},
                          '<https://api.github.com/repos/nil-studio/oasis7/page2>; rel="next"'),
                 Response({"total_count": 3, "workflow_runs": [{"id": 2}]})]
        with mock.patch.object(api, "request", side_effect=pages):
            with self.assertRaisesRegex(ValueError, "total_count"):
                api.pages("page1", "workflow_runs")

    def test_all_pages_are_read(self):
        api = module.GitHub("nil-studio/oasis7", "test-only")
        pages = [
            Response({"total_count": 2, "jobs": [{"id": 1}]},
                     '<https://api.github.com/repos/nil-studio/oasis7/page2>; rel="next"'),
            Response({"total_count": 2, "jobs": [{"id": 2}]}),
        ]
        with mock.patch.object(api, "request", side_effect=pages):
            self.assertEqual([1, 2], [job["id"] for job in api.pages("page1", "jobs")])

    def test_missing_page_is_unknown_not_complete(self):
        api = module.GitHub("nil-studio/oasis7", "test-only")
        with mock.patch.object(api, "request", return_value=Response({"total_count": 2, "jobs": [{"id": 1}]})):
            with self.assertRaises(ValueError):
                list(api.pages("page1", "jobs"))

    def test_duplicate_ids_are_rejected(self):
        api = module.GitHub("nil-studio/oasis7", "test-only")
        with mock.patch.object(api, "request", return_value=Response({"total_count": 2, "jobs": [{"id": 1}, {"id": 1}]})):
            with self.assertRaises(ValueError):
                list(api.pages("page1", "jobs"))

    def test_unrelated_next_page_and_later_error_are_rejected(self):
        api = module.GitHub("nil-studio/oasis7", "test-only")
        response = Response({"total_count": 2, "jobs": [{"id": 1}]},
                            '<https://example.com/page2>; rel="next"')
        with mock.patch.object(api, "request", return_value=response):
            with self.assertRaises(ValueError):
                list(api.pages("page1", "jobs"))
        response = Response({"total_count": 2, "jobs": [{"id": 1}]},
                            '<https://api.github.com/repos/nil-studio/oasis7/page2>; rel="next"')
        with mock.patch.object(api, "request", side_effect=[response, OSError("later page unavailable")]):
            with self.assertRaises(OSError):
                list(api.pages("page1", "jobs"))

    def test_artifact_has_one_named_file_and_is_not_extracted(self):
        api = module.GitHub("nil-studio/oasis7", "test-only")
        for name in ("full-result.json", "../full-result.json"):
            archive = io.BytesIO()
            with zipfile.ZipFile(archive, "w") as bundle:
                bundle.writestr(name, '{"schema":"fixture"}')
            with mock.patch.object(api, "request", return_value=io.BytesIO(archive.getvalue())):
                if name == "full-result.json":
                    self.assertEqual(api.artifact(1), {"schema": "fixture"})
                else:
                    with self.assertRaises(ValueError):
                        api.artifact(1)

    def test_artifact_redirect_does_not_forward_the_api_bearer_token(self):
        api = module.GitHub("nil-studio/oasis7", "fixture-api-secret")
        location = "https://artifacts.example.invalid/receipt.zip?signature=fixture"
        archive = io.BytesIO()
        with zipfile.ZipFile(archive, "w") as bundle:
            bundle.writestr("full-result.json", '{"schema":"fixture"}')
        redirect = urllib.error.HTTPError(api.root + "actions/artifacts/9/zip", 302, "Found",
                                          {"Location": location}, None)
        transient = urllib.error.HTTPError(api.root, 503, "Service Unavailable", {}, io.BytesIO(b"temporary failure"))
        with mock.patch.object(api.opener, "open", side_effect=[transient, redirect]) as authenticated, \
             mock.patch.object(urllib.request, "urlopen", return_value=io.BytesIO(archive.getvalue())) as storage, \
             mock.patch.object(module.time, "sleep") as slept:
            self.assertEqual(api.artifact(9), {"schema": "fixture"})
        self.assertEqual(authenticated.call_count, 2)
        self.assertEqual(api.requests, 3)
        slept.assert_called_once_with(0.5)
        api_request = authenticated.call_args.args[0]
        self.assertEqual(api_request.get_header("Authorization"), "Bearer fixture-api-secret")
        storage_request = storage.call_args.args[0]
        self.assertEqual(storage_request, location)
        self.assertNotIn("fixture-api-secret", storage_request)
        self.assertEqual(storage.call_args.kwargs, {"timeout": 30})
        self.assertIsNone(module.NoRedirect().redirect_request(None, None, 302, "Found", {}, location))

    def test_unsafe_artifact_redirects_never_open_storage(self):
        api = module.GitHub("nil-studio/oasis7", "test-only")
        for location in ("http://artifacts.example.invalid/receipt.zip", "https://user:pass@example.invalid/a", ""):
            redirect = urllib.error.HTTPError(api.root, 302, "Found", {"Location": location}, None)
            with self.subTest(location=location), mock.patch.object(api, "request", side_effect=redirect), \
                 mock.patch.object(urllib.request, "urlopen") as storage:
                with self.assertRaisesRegex(ValueError, "unsafe artifact"):
                    api.artifact(9)
                storage.assert_not_called()

    def test_later_run_page_failure_cannot_expose_a_green_first_page(self):
        api = module.GitHub("nil-studio/oasis7", "test-only")
        _, green, _ = fixture()
        responses = [
            Response({"id": 7}),
            Response({"total_count": 2, "workflow_runs": [green]},
                     '<https://api.github.com/repos/nil-studio/oasis7/page2>; rel="next"'),
            OSError("later workflow run page unavailable"),
        ]
        with mock.patch.object(api, "request", side_effect=responses), mock.patch.object(api, "artifact") as download:
            with self.assertRaisesRegex(OSError, "later workflow run page"):
                module.find_verified(api, A, CONFIG, "definition")
            download.assert_not_called()


class PublicationSource(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        def git(*args):
            return subprocess.check_output(["git", *args], cwd=self.root, text=True, stderr=subprocess.PIPE).strip()
        git("init", "-q")
        (self.root / "source").write_text("main\n")
        git("add", ".")
        git("-c", "user.name=Fixture", "-c", "user.email=fixture@example.invalid",
            "-c", "commit.gpgsign=false", "commit", "-qm", "main")
        self.main = git("rev-parse", "HEAD")
        git("update-ref", "refs/remotes/origin/main", self.main)
        (self.root / "source").write_text("candidate\n")
        git("add", ".")
        git("-c", "user.name=Fixture", "-c", "user.email=fixture@example.invalid",
            "-c", "commit.gpgsign=false", "commit", "-qm", "candidate")
        self.candidate = git("rev-parse", "HEAD")
        self.environment = dict(GITHUB_REPOSITORY="nil-studio/oasis7", GITHUB_WORKSPACE=str(self.root),
                                GITHUB_WORKFLOW_REF="nil-studio/oasis7/.github/workflows/testnet-packages.yml@refs/heads/main",
                                GITHUB_WORKFLOW_SHA=self.main, GH_TOKEN="test-only")

    def test_only_trusted_main_testnet_can_opt_in_to_a_fixed_unmerged_source(self):
        module.validate_publication_source(self.root, self.candidate, True, self.environment)
        module.validate_publication_source(self.root, self.main, False, self.environment)
        with self.assertRaisesRegex(ValueError, "protected main history"):
            module.validate_publication_source(self.root, self.candidate, False, self.environment)
        for workflow_ref in (
            "nil-studio/oasis7/.github/workflows/testnet-packages.yml@refs/heads/candidate",
            "nil-studio/oasis7/.github/workflows/release-packages.yml@refs/heads/main",
            "nil-studio/oasis7/.github/workflows/mainnet-packages.yml@refs/heads/main",
            "nil-studio/oasis7/.github/workflows/ecs-packages.yml@refs/heads/main",
        ):
            with self.subTest(workflow_ref=workflow_ref), self.assertRaisesRegex(ValueError, "trusted main Testnet"):
                module.validate_publication_source(self.root, self.candidate, True,
                                                   dict(self.environment, GITHUB_WORKFLOW_REF=workflow_ref))

    def test_candidate_cli_still_requires_main_policy_and_full_result(self):
        receipt, _, _ = fixture()
        argv = ["ci-full-result.py", "verify", "--source", self.candidate, "--allow-candidate-source"]
        with mock.patch.dict(os.environ, self.environment), mock.patch.object(sys, "argv", argv), \
             mock.patch.object(module, "GitHub"), mock.patch.object(module, "definition_digest", return_value="definition"), \
             mock.patch.object(module, "find_verified", return_value=receipt) as full, mock.patch("builtins.print"):
            module.main()
            self.assertEqual(full.call_args.args[1], self.candidate)
            full.reset_mock()
            with mock.patch.dict(os.environ, GITHUB_WORKFLOW_SHA=self.candidate):
                with self.assertRaisesRegex(ValueError, "protected main history"):
                    module.main()
            full.assert_not_called()
            with mock.patch.object(module, "find_verified", side_effect=ValueError("no complete trusted result")):
                with self.assertRaisesRegex(ValueError, "no complete trusted result"):
                    module.main()


class HealthAndConsumers(unittest.TestCase):
    def test_no_run_stale_and_failed_are_visible(self):
        now = datetime(2026, 10, 30, 12, tzinfo=timezone.utc)
        run = dict(id=42, event="schedule", head_branch="main", head_sha=A,
                   created_at=(now - timedelta(minutes=10)).isoformat(), status="completed", conclusion="success")
        self.assertEqual(42, module.health([run], now)["run_id"])
        for runs in ([], [dict(run, conclusion="failure")],
                     [dict(run, created_at=(now - timedelta(hours=8)).isoformat())]):
            with self.assertRaises(ValueError):
                module.health(runs, now)

    def test_health_reports_recent_start_terminal_and_last_success_separately(self):
        now = datetime(2026, 10, 30, 12, tzinfo=timezone.utc)
        success = dict(id=40, event="schedule", head_branch="main", head_sha=A,
                       created_at="2026-10-30T08:00:00Z", updated_at="2026-10-30T09:00:00Z",
                       status="completed", conclusion="success")
        active = dict(success, id=41, head_sha=B, created_at="2026-10-30T11:50:00Z",
                      updated_at="2026-10-30T11:51:00Z", status="in_progress", conclusion=None)
        report = module.health([success, active], now)
        self.assertEqual(report["latest_start"]["run_id"], 41)
        self.assertEqual(report["latest_terminal"]["run_id"], 40)
        self.assertEqual(report["last_successful_run"]["run_id"], 40)
        self.assertEqual(report["last_successful_sha"], A)
        self.assertEqual(report["commit_range"], {"base": A, "head": B})
        self.assertEqual(report["overdue_run_ids"], [])
        with self.assertRaisesRegex(ValueError, "no successful scheduled run"):
            module.health([active], now)

    def test_new_pending_run_cannot_hide_an_older_overdue_scheduled_run(self):
        now = datetime(2026, 10, 30, 12, tzinfo=timezone.utc)
        success = dict(id=40, event="schedule", head_branch="main", head_sha=A,
                       created_at="2026-10-30T08:00:00Z", updated_at="2026-10-30T09:00:00Z",
                       status="completed", conclusion="success")
        pending = dict(success, id=42, head_sha=B, created_at="2026-10-30T11:50:00Z",
                       updated_at="2026-10-30T11:50:00Z", status="queued", conclusion=None)
        for status in ("in_progress", "queued"):
            overdue = dict(success, id=39, created_at="2026-10-30T06:00:00Z",
                           updated_at="2026-10-30T11:59:00Z", status=status, conclusion=None)
            with self.subTest(status=status), self.assertRaisesRegex(ValueError, "execution budget") as caught:
                module.health([success, overdue, pending], now)
            report = json.loads(str(caught.exception).split("; schedule facts: ", 1)[1])
            self.assertEqual(report["overdue_run_ids"], [39])
            self.assertEqual(report["latest_start"]["run_id"], 42)

    def test_failed_terminal_remains_visible_until_a_later_success(self):
        now = datetime(2026, 10, 30, 12, tzinfo=timezone.utc)
        success = dict(id=40, event="schedule", head_branch="main", head_sha=A,
                       created_at="2026-10-30T04:00:00Z", updated_at="2026-10-30T05:00:00Z",
                       status="completed", conclusion="success")
        failed = dict(success, id=41, head_sha=B, created_at="2026-10-30T08:00:00Z",
                      updated_at="2026-10-30T10:00:00Z", conclusion="failure")
        pending = dict(success, id=42, head_sha="c" * 40, created_at="2026-10-30T11:50:00Z",
                       updated_at="2026-10-30T11:50:00Z", status="queued", conclusion=None)
        with self.assertRaisesRegex(ValueError, "has not restored health") as caught:
            module.health([success, failed, pending], now)
        report = json.loads(str(caught.exception).split("; schedule facts: ", 1)[1])
        self.assertEqual(report["latest_terminal"]["run_id"], 41)
        self.assertEqual(report["last_successful_sha"], A)
        recovered = dict(pending, status="completed", conclusion="success", updated_at="2026-10-30T11:59:00Z")
        restored = module.health([success, failed, recovered], now)
        self.assertEqual(restored["last_successful_run"]["run_id"], 42)
        self.assertEqual(restored["last_successful_sha"], recovered["head_sha"])

    def test_health_cli_limits_history_and_does_not_qualify_a_release(self):
        now = datetime(2026, 10, 30, 12, tzinfo=timezone.utc)
        _, run, jobs = fixture()
        run.update(
                   created_at="2026-10-30T11:00:00Z", updated_at="2026-10-30T11:30:00Z",
                   status="completed", conclusion="success")
        api = History([run], artifacts={42: [{"id": 4202, "name": "ci-full-result-42-2", "expired": False}]},
                      jobs={(42, 2): jobs})
        with mock.patch.object(sys, "argv", ["ci-full-result.py", "health"]), \
             mock.patch.dict(os.environ, GITHUB_REPOSITORY="nil-studio/oasis7", GH_TOKEN="test-only"), \
             mock.patch.object(module, "GitHub", return_value=api), \
             mock.patch.object(module, "datetime", wraps=datetime) as clock, \
             mock.patch.object(module, "find_verified") as qualifier, mock.patch("builtins.print") as printed:
            clock.now.return_value = now
            module.main()
        query = urllib.parse.parse_qs(urllib.parse.urlsplit(api.queries[0]).query)
        self.assertEqual(query["created"], [">=2026-10-28T12:00:00Z"])
        self.assertEqual(query["event"], ["schedule"])
        self.assertEqual(query["branch"], ["main"])
        self.assertEqual(json.loads(printed.call_args.args[0])["last_successful_sha"], A)
        qualifier.assert_not_called()
        self.assertEqual(api.downloads, [])

    def test_retained_rollback_keeps_recovery_and_normal_promotion_checks_retained_source(self):
        sys.path.insert(0, str(ROOT / "scripts"))
        spec = importlib.util.spec_from_file_location("ecs_cli_for_full_tests", ROOT / "scripts/ecs-preview-release.py")
        ecs = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(ecs)
        with mock.patch.object(ecs.subprocess, "run") as runner:
            ecs.verify_full_for_publish(B, rollback=True)
            runner.assert_not_called()
            ecs.verify_full_for_publish(B)
            self.assertEqual(["verify", "--source", B], runner.call_args.args[0][-3:])
            self.assertTrue(runner.call_args.kwargs["check"])
        with mock.patch.object(ecs.subprocess, "run", side_effect=subprocess.CalledProcessError(1, [])):
            with self.assertRaises(subprocess.CalledProcessError):
                ecs.verify_full_for_publish(B)

    def test_ecs_publisher_checks_retained_manifest_before_any_publish(self):
        sys.path.insert(0, str(ROOT / "scripts"))
        spec = importlib.util.spec_from_file_location("ecs_publish_boundary", ROOT / "scripts/ecs-preview-release.py")
        ecs = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(ecs)
        release = B + "/10-1"
        manifest = dict(schema_version=1, release_id=release, source_sha=B,
                        archive_key="preview/releases/" + release + "/bundle.tar.gz", archive_sha256="c" * 64,
                        platform="linux-x64", components=["player"],
                        compatibility=dict(program_contract="v1", persistence_contract="v1", identity_contract="v1"))
        environment = dict(GITHUB_EVENT_NAME="workflow_dispatch", GITHUB_REF="refs/heads/main",
                           GITHUB_WORKFLOW_SHA=A, GITHUB_SHA=A, GITHUB_REPOSITORY="nil-studio/oasis7",
                           GITHUB_RUN_ID="100", GITHUB_RUN_NUMBER="100")
        argv = ["ecs-preview-release.py", "--endpoint", "https://oss.example.invalid", "--bucket", "fixture",
                "--prefix", "preview", "--retained-release", release]
        store = mock.Mock(spec=ecs.OssStore)
        store.get.return_value = json.dumps(manifest)
        events = []
        def verified(command, *, check):
            self.assertEqual(command[-3:], ["verify", "--source", B])
            self.assertTrue(check)
            events.append("verified retained source")
        def published(*args):
            self.assertEqual(args[1]["source_sha"], B)
            events.append("published")
            return {}
        with mock.patch.dict(os.environ, environment), mock.patch.object(sys, "argv", argv), \
             mock.patch.object(ecs, "OssStore", return_value=store), mock.patch("builtins.print"), \
             mock.patch.object(ecs, "publish", side_effect=published) as publish, \
             mock.patch.object(ecs.subprocess, "run", side_effect=verified) as verify:
            ecs.main()
            self.assertEqual(events, ["verified retained source", "published"])
            publish.reset_mock()
            verify.side_effect = subprocess.CalledProcessError(1, [])
            with self.assertRaises(subprocess.CalledProcessError):
                ecs.main()
            publish.assert_not_called()
            verify.reset_mock()
            with mock.patch.object(sys, "argv", argv + ["--rollback"]):
                ecs.main()
            verify.assert_not_called()
            self.assertTrue(publish.call_args.args[-1])


if __name__ == "__main__":
    unittest.main()
