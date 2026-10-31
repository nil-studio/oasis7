#!/usr/bin/env python3
"""Source, inventory, provenance and recovery contracts for full CI consumers."""
import copy
from datetime import datetime, timedelta, timezone
import importlib.util
import io
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
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

    def get(self, path):
        if path == "actions/workflows/rust.yml":
            return {"id": 7}
        if path.startswith("actions/runs/"):
            return next(run for run in self.runs if str(run["id"]) == path.rsplit("/", 1)[1])
        raise AssertionError("unexpected API object: " + path)

    def pages(self, path, key):
        parsed = urllib.parse.urlsplit(path)
        parts = parsed.path.split("/")
        if parts[:2] == ["actions", "workflows"] and parts[-1] == "runs":
            event = urllib.parse.parse_qs(parsed.query)["event"][0]
            return [run for run in self.runs if run["event"] == event]
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


class Collections(unittest.TestCase):
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
        with mock.patch.object(api.opener, "open", side_effect=redirect) as authenticated, \
             mock.patch.object(urllib.request, "urlopen", return_value=io.BytesIO(archive.getvalue())) as storage:
            self.assertEqual(api.artifact(9), {"schema": "fixture"})
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
        run = dict(id=42, event="schedule", head_branch="main", head_sha=A,
                   created_at="2026-10-30T11:00:00Z", updated_at="2026-10-30T11:30:00Z",
                   status="completed", conclusion="success")
        api = mock.Mock()
        api.pages.return_value = [run]
        with mock.patch.object(sys, "argv", ["ci-full-result.py", "health"]), \
             mock.patch.dict(os.environ, GITHUB_REPOSITORY="nil-studio/oasis7", GH_TOKEN="test-only"), \
             mock.patch.object(module, "GitHub", return_value=api), \
             mock.patch.object(module, "datetime", wraps=datetime) as clock, \
             mock.patch.object(module, "find_verified") as qualifier, mock.patch("builtins.print") as printed:
            clock.now.return_value = now
            module.main()
        query = urllib.parse.parse_qs(urllib.parse.urlsplit(api.pages.call_args.args[0]).query)
        self.assertEqual(query["created"], [">=2026-10-28T12:00:00Z"])
        self.assertEqual(query["event"], ["schedule"])
        self.assertEqual(query["branch"], ["main"])
        self.assertEqual(json.loads(printed.call_args.args[0])["last_successful_sha"], A)
        qualifier.assert_not_called()
        api.artifact.assert_not_called()

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
