#!/usr/bin/env python3
"""Bind a complete CI result to its source, approved policy and Actions attempt."""
from __future__ import annotations

import argparse
from concurrent.futures import ThreadPoolExecutor
from datetime import datetime, timedelta, timezone
import hashlib
import importlib.util
import io
import json
import math
import os
from pathlib import Path
import re
import subprocess
import sys
import threading
import time
import urllib.error
import urllib.parse
import urllib.request
import zipfile


SCHEMA = "oasis7.ci-full-result/v1"
ADMISSION_SCHEMA = "oasis7.ci-periodic-admission/v1"
WORKFLOW = ".github/workflows/rust.yml"
MAX_BYTES = 2 * 1024 * 1024
CADENCE_SECONDS = 4 * 3600
API_BUDGET_SECONDS = 120
MAX_API_REQUESTS = 800
ACTIVE_STATUSES = {"queued", "in_progress", "waiting", "pending", "requested"}
POLICY_FILES = (
    WORKFLOW, ".github/workflows/command-center.yml",
    "scripts/ci-required-scope.json", "scripts/plan-rust-required-scope.py",
    "scripts/ci-tests.sh", "scripts/ci-test-artifacts.py",
    "scripts/ci-formal-process-suites.json", "scripts/ci-formal-plan.py",
    "scripts/ci_process_runner.py", "scripts/ci-required-result.py",
    "scripts/ci-full-result.py", "scripts/ci-python-control-contracts.py",
)
ROOT = Path(__file__).resolve().parent.parent


def decode_json(raw):
    def pairs(items):
        value = {}
        for key, item in items:
            if key in value:
                raise ValueError(f"duplicate JSON key: {key}")
            value[key] = item
        return value
    return json.loads(raw, object_pairs_hook=pairs)


def commit(value):
    if not isinstance(value, str) or not re.fullmatch(r"[0-9a-f]{40}", value):
        raise ValueError("expected an immutable 40-character commit SHA")
    return value


def definition_digest(root, revision):
    revision = commit(revision)

    def read(path):
        return subprocess.check_output(
            ["git", "-C", str(root), "show", f"{revision}:{path}"],
            stderr=subprocess.PIPE,
        )

    contents = {path: read(path) for path in POLICY_FILES}
    config = decode_json(contents["scripts/ci-required-scope.json"])
    if not isinstance(config, dict):
        raise ValueError("invalid trusted scope definition")
    if "custody_execution_version" in config:
        if type(config["custody_execution_version"]) is not int or config["custody_execution_version"] != 1:
            raise ValueError("invalid trusted custody execution definition")
        contents["scripts/ci-custody-platform.py"] = read("scripts/ci-custody-platform.py")
    contract_tests = set()
    if "workflow_governance_python_tests" in config:
        governed = config["workflow_governance_python_tests"]
        if (not isinstance(governed, list) or not governed or len(governed) > 256
                or any(not isinstance(path, str) or not path.startswith(("scripts/", "crates/"))
                       or not path.endswith(".test.py")
                       or any(part in ("", ".", "..") for part in path.split("/"))
                       or any(character in path for character in ("\\", "*", "?", "[", "]"))
                       or any(ord(character) < 32 for character in path) for path in governed)
                or len(governed) != len(set(governed))):
            raise ValueError("invalid complete governance Python definition")
        contract_tests.update(governed)
    if {"python_control_contract_version", "python_control_contracts"}.intersection(config):
        contracts = config.get("python_control_contracts")
        if (type(config.get("python_control_contract_version")) is not int
                or config["python_control_contract_version"] != 1
                or not isinstance(contracts, dict) or not contracts):
            raise ValueError("invalid trusted Python contract definition")
        for contract in contracts.values():
            tests = contract.get("tests") if isinstance(contract, dict) else None
            if not isinstance(tests, list) or not tests:
                raise ValueError("missing trusted Python contract tests")
            for path in tests:
                if (not isinstance(path, str) or not path.startswith("scripts/")
                        or not path.endswith(".test.py")
                        or any(part in ("", ".", "..") for part in path.split("/"))
                        or any(character in path for character in ("\\", "*", "?", "[", "]"))
                        or any(ord(character) < 32 for character in path)):
                    raise ValueError("invalid trusted Python contract test path")
                contract_tests.add(path)
        # New tool preparation affects every custody WASM producer. Bind its
        # implementation only when that immutable policy registers it, so old
        # revisions remain readable and cannot claim the newer definition.
        if "scripts/ci-worker-build.py" in contracts:
            contents["scripts/ci-worker-build.py"] = read("scripts/ci-worker-build.py")
    # These assertions are now executable policy inputs, read from the same
    # immutable revision as the runner and scope. Never load candidate Python.
    for path in sorted(contract_tests.difference(contents)):
        contents[path] = read(path)
    digest = hashlib.sha256()
    for path, content in contents.items():
        digest.update(path.encode() + b"\0" + hashlib.sha256(content).digest())
    return digest.hexdigest()


def validator():
    path = Path(__file__).with_name("ci-required-result.py")
    spec = importlib.util.spec_from_file_location("ci_required_full_result", path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module.validate_full


def validate_plan(plan, needs, config, context):
    jobs = {name: name.replace("_", "-") for name in config["groups"]}
    validator()(
        plan, needs, jobs, event_name=context["event_name"], ref=context["ref"],
        workflow_sha=context["workflow_sha"], source_sha=plan["source_sha"],
        run_mode="full" if context["event_name"] == "workflow_dispatch" else "",
    )


def make_receipt(plan, needs, config, context, digest):
    validate_plan(plan, needs, config, context)
    if context["run_id"] < 1 or context["run_attempt"] < 1:
        raise ValueError("invalid Actions run identity")
    return {
        "schema": SCHEMA, "coverage_version": 1, "context": context,
        "definition_sha256": digest, "plan": plan, "needs": needs,
    }


class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        return None


class GitHub:
    """Read-only API; never forward the API bearer token to artifact storage."""

    def __init__(self, repository, token):
        if not re.fullmatch(r"[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+", repository):
            raise ValueError("invalid repository")
        if not token:
            raise ValueError("GH_TOKEN or GITHUB_TOKEN is required for CI verification")
        self.repository = repository
        self.root = f"https://api.github.com/repos/{repository}/"
        self.token = token
        self.opener = urllib.request.build_opener(NoRedirect)
        self.deadline = time.monotonic() + API_BUDGET_SECONDS
        self.requests = 0
        self.budget_lock = threading.Lock()

    def request_timeout(self):
        with self.budget_lock:
            remaining = self.deadline - time.monotonic()
            if remaining <= 0 or self.requests >= MAX_API_REQUESTS:
                raise ValueError("API verification budget exhausted; history is unknown")
            self.requests += 1
            return min(30, remaining)

    def check_deadline(self):
        if time.monotonic() >= self.deadline:
            raise ValueError("API verification budget exhausted; history is unknown")

    def request(self, path):
        url = path if path.startswith("https://") else self.root + path
        if not url.startswith(self.root):
            raise ValueError("API pagination escaped the repository")
        req = urllib.request.Request(url, headers={
            "Authorization": f"Bearer {self.token}", "Accept": "application/vnd.github+json",
            "X-GitHub-Api-Version": "2022-11-28", "User-Agent": "oasis7-ci-full-result",
        })
        return self.opener.open(req, timeout=self.request_timeout())

    def get(self, path):
        with self.request(path) as response:
            raw = response.read(MAX_BYTES + 1)
        self.check_deadline()
        if len(raw) > MAX_BYTES:
            raise ValueError("API response exceeds bounded verification budget")
        return decode_json(raw)

    def pages(self, path, key):
        next_url = path
        visited, identities, collected = set(), set(), []
        total = None
        for _ in range(20):
            if next_url in visited:
                raise ValueError("API pagination cycle")
            visited.add(next_url)
            with self.request(next_url) as response:
                raw = response.read(MAX_BYTES + 1)
                links = response.headers.get("Link", "")
            self.check_deadline()
            if len(raw) > MAX_BYTES:
                raise ValueError("API page exceeds bounded verification budget")
            payload = decode_json(raw)
            if not isinstance(payload, dict) or not isinstance(payload.get(key), list):
                raise ValueError(f"incomplete API collection: {key}")
            count = payload.get("total_count")
            if type(count) is not int or count < 0 or (total is not None and count != total):
                raise ValueError("API total_count missing or changed during pagination")
            total = count
            for item in payload[key]:
                if not isinstance(item, dict) or type(item.get("id")) is not int or item["id"] in identities:
                    raise ValueError("invalid or duplicate API collection identity")
                identities.add(item["id"])
                collected.append(item)
            next_links = re.findall(r'<([^>]+)>;\s*rel="next"', links)
            if not next_links:
                if len(collected) != total:
                    raise ValueError("API collection incomplete")
                # Validate the whole collection before exposing any result.
                return collected
            if len(next_links) != 1 or not next_links[0].startswith(self.root):
                raise ValueError("invalid API next page")
            next_url = next_links[0]
        raise ValueError("API pagination incomplete: specify an exact --run-id")

    def artifact(self, artifact_id, filename="full-result.json"):
        if type(artifact_id) is not int or artifact_id < 1:
            raise ValueError("invalid artifact ID")
        try:
            response = self.request(f"actions/artifacts/{artifact_id}/zip")
        except urllib.error.HTTPError as exc:
            if exc.code != 302:
                raise
            location = exc.headers.get("Location", "")
            parsed = urllib.parse.urlsplit(location)
            if parsed.scheme != "https" or not parsed.hostname or parsed.username or parsed.password:
                raise ValueError("unsafe artifact download redirect") from exc
            # This separate request has no Authorization header.
            response = urllib.request.urlopen(location, timeout=self.request_timeout())
        with response:
            archive = response.read(MAX_BYTES + 1)
        self.check_deadline()
        if len(archive) > MAX_BYTES:
            raise ValueError("full result artifact too large")
        with zipfile.ZipFile(io.BytesIO(archive)) as bundle:
            entries = bundle.infolist()
            if (len(entries) != 1 or entries[0].filename != filename
                    or entries[0].file_size > MAX_BYTES
                    or (entries[0].external_attr >> 16) & 0o170000 == 0o120000):
                raise ValueError(f"artifact must contain only bounded {filename}")
            return decode_json(bundle.read(entries[0]))


def verify_receipt(receipt, run, jobs, *, source, repository, workflow_id, config, digest):
    if (not isinstance(receipt, dict) or receipt.get("schema") != SCHEMA
            or receipt.get("coverage_version") != 1):
        raise ValueError("unsupported full result")
    context, plan, needs = receipt.get("context"), receipt.get("plan"), receipt.get("needs")
    if not isinstance(context, dict) or not isinstance(plan, dict):
        raise ValueError("missing full result context or plan")
    if plan.get("bootstrap_full"):
        raise ValueError("bootstrap validation cannot qualify a release")
    if (run.get("workflow_id") != workflow_id or run.get("path") != WORKFLOW
            or run.get("head_branch") != "main" or run.get("event") not in {"schedule", "workflow_dispatch"}
            or run.get("status") != "completed" or run.get("conclusion") != "success"):
        raise ValueError("run is not a successful trusted full workflow")
    if (context.get("repository") != repository or context.get("ref") != "refs/heads/main"
            or context.get("workflow_ref") != f"{repository}/{WORKFLOW}@refs/heads/main"
            or context.get("workflow_sha") != run.get("head_sha")
            or context.get("event_name") != run.get("event")
            or context.get("run_id") != run.get("id")
            or context.get("run_attempt") != run.get("run_attempt")):
        raise ValueError("result does not belong to the actual trusted Actions attempt")
    if (plan.get("source_sha") != commit(source) or plan.get("tested_sha") != source
            or plan.get("policy_sha") != commit(run.get("head_sha"))):
        raise ValueError("full result source or policy does not match")
    if receipt.get("definition_sha256") != digest:
        raise ValueError("full result uses a different check/workflow definition")
    validate_plan(plan, needs, config, context)
    gates = [job for job in jobs if job.get("name") == "explicit-full-gate"]
    if (len(gates) != 1 or gates[0].get("status") != "completed"
            or gates[0].get("conclusion") != "success"
            or gates[0].get("run_attempt") != run.get("run_attempt")):
        raise ValueError("missing, ambiguous or unsuccessful full gate in actual jobs")
    return receipt


def require_default_ancestor(root, source):
    commit(source)
    if subprocess.run(["git", "-C", str(root), "merge-base", "--is-ancestor", source,
                       "refs/remotes/origin/main"], capture_output=True).returncode:
        raise ValueError("published source is not in the fetched protected main history")


def validate_publication_source(root, source, allow_candidate, environment):
    commit(source)
    if allow_candidate:
        expected = f"{environment['GITHUB_REPOSITORY']}/.github/workflows/testnet-packages.yml@refs/heads/main"
        if environment.get("GITHUB_WORKFLOW_REF") != expected:
            raise ValueError("candidate source is only supported by the trusted main Testnet entry")
    else:
        require_default_ancestor(root, source)


def timestamp(value):
    if not isinstance(value, str):
        raise ValueError("missing Actions timestamp")
    result = datetime.fromisoformat(value.replace("Z", "+00:00"))
    if result.tzinfo is None:
        raise ValueError("Actions timestamp has no timezone")
    return result


def attempt_clock(run):
    return max(timestamp(run[key]) for key in ("created_at", "run_started_at", "updated_at")
               if run.get(key))


def trusted_schedule(run, workflow_id):
    if (type(workflow_id) is not int or run.get("workflow_id") != workflow_id
            or run.get("path") != WORKFLOW or run.get("event") != "schedule"
            or run.get("head_branch") != "main"
            or type(run.get("id")) is not int or run["id"] < 1
            or type(run.get("run_attempt")) is not int or run["run_attempt"] < 1
            or run.get("status") not in ACTIVE_STATUSES | {"completed"}):
        raise ValueError("periodic observation is not a canonical main Actions attempt")
    commit(run.get("head_sha"))
    timestamp(run.get("created_at"))


def admission_context(environment):
    context = {key: environment[variable] for key, variable in {
        "repository": "GITHUB_REPOSITORY", "event_name": "GITHUB_EVENT_NAME",
        "ref": "GITHUB_REF", "workflow_sha": "GITHUB_WORKFLOW_SHA",
        "workflow_ref": "GITHUB_WORKFLOW_REF", "source_sha": "GITHUB_SHA",
    }.items()}
    context.update(run_id=int(environment["GITHUB_RUN_ID"]),
                   run_attempt=int(environment["GITHUB_RUN_ATTEMPT"]))
    return context


def validate_admission_context(context, run, repository):
    if (not isinstance(context, dict) or context.get("repository") != repository
            or context.get("event_name") != "schedule" or context.get("ref") != "refs/heads/main"
            or context.get("workflow_ref") != f"{repository}/{WORKFLOW}@refs/heads/main"
            or context.get("workflow_sha") != run.get("head_sha")
            or context.get("source_sha") != run.get("head_sha")
            or type(context.get("run_id")) is not int or context["run_id"] != run["id"]
            or type(context.get("run_attempt")) is not int
            or context["run_attempt"] != run["run_attempt"]):
        raise ValueError("admission does not belong to the actual trusted Actions attempt")


def validate_admission(receipt, run, repository):
    if (not isinstance(receipt, dict) or receipt.get("schema") != ADMISSION_SCHEMA
            or type(receipt.get("run_full")) is not bool
            or type(receipt.get("cadence_seconds")) is not int
            or receipt["cadence_seconds"] != CADENCE_SECONDS
            or receipt.get("reason") not in ({"due"} if receipt["run_full"] else {"not_due", "full_active"})):
        raise ValueError("unsupported or contradictory periodic admission")
    validate_admission_context(receipt.get("context"), run, repository)
    observed = timestamp(receipt.get("observed_at"))
    if observed < timestamp(run["created_at"]):
        raise ValueError("admission predates its Actions run")
    for key in ("active_full_run_ids", "control_failure_run_ids"):
        values = receipt.get(key)
        if (not isinstance(values, list) or any(type(value) is not int or value < 1 for value in values)
                or len(set(values)) != len(values) or run["id"] in values):
            raise ValueError("invalid periodic admission history identities")
    previous, gap = receipt.get("previous_full"), receipt.get("start_gap_seconds")
    if previous is None:
        if gap is not None or receipt["active_full_run_ids"]:
            raise ValueError("admission has no prior full for its reported gap or reservation")
    else:
        if (not isinstance(previous, dict) or type(previous.get("id")) is not int or previous["id"] < 1
                or previous["id"] == run["id"] or type(previous.get("run_attempt")) is not int
                or previous["run_attempt"] < 1 or previous.get("status") not in ACTIVE_STATUSES | {"completed"}
                or type(gap) not in {int, float} or not math.isfinite(gap) or gap < 0):
            raise ValueError("invalid periodic admission prior full or gap")
        commit(previous.get("head_sha"))
        anchor = timestamp(previous.get("full_started_at") or previous.get("reserved_at") or previous.get("created_at"))
        if abs((observed - anchor).total_seconds() - gap) > 0.001:
            raise ValueError("periodic admission gap does not match its prior full")
    active = bool(receipt["active_full_run_ids"])
    due = gap is None or gap >= CADENCE_SECONDS
    expected_reason = "full_active" if active else "due" if due else "not_due"
    if (receipt["reason"] != expected_reason or receipt["run_full"] != (due and not active)
            or type(receipt.get("missed_intervals")) is not int
            or receipt["missed_intervals"] != (max(0, int(gap // CADENCE_SECONDS) - 1) if gap is not None else 0)):
        raise ValueError("periodic admission does not obey the four-hour cadence")
    return receipt


def classify_periodic_attempt(api, run, workflow_id, artifacts=None):
    """Exclude a no-op only with its immutable decision AND actual skipped jobs.

    Missing evidence never means a successful observation. Legacy schedules stay
    full attempts, including their failures. A failed admission with every full
    entry skipped is a control failure: it can be compensated by a later tick,
    but cannot qualify a release or restore health.
    """
    trusted_schedule(run, workflow_id)
    jobs = api.pages(f"actions/runs/{run['id']}/attempts/{run['run_attempt']}/jobs?per_page=100", "jobs")
    current = [job for job in jobs if job.get("run_attempt") == run["run_attempt"]]
    admission_jobs = [job for job in current if job.get("name") == "periodic-admission"]
    select = [job for job in current if job.get("name") == "select"]
    gates = [job for job in current if job.get("name") == "explicit-full-gate"]
    if len(admission_jobs) > 1 or len(select) > 1 or len(gates) > 1:
        raise ValueError("ambiguous periodic admission, select or full gate")
    skipped = all(len(found) == 1 and found[0].get("status") == "completed"
                  and found[0].get("conclusion") == "skipped" for found in (select, gates))
    other_skipped = all(job.get("status") == "completed" and job.get("conclusion") == "skipped"
                        for job in current if job.get("name") != "periodic-admission")
    result = {"run": run, "kind": "full", "full_started_at": None, "reserved_at": None,
              "successful": False, "decision": None}
    if select and select[0].get("started_at") and select[0].get("conclusion") != "skipped":
        timestamp(select[0]["started_at"])
        result["full_started_at"] = select[0]["started_at"]
    if admission_jobs:
        job = admission_jobs[0]
        if (run.get("status") == "completed" and job.get("status") == "completed"
                and job.get("conclusion") not in {"success", None}
                and (job.get("conclusion") != "skipped" or run.get("conclusion") != "success")
                and skipped and other_skipped):
            return dict(result, kind="control_failure")
        if (job.get("status") in ACTIVE_STATUSES - {"in_progress"} and not job.get("started_at")
                and not result["full_started_at"] and not any(
                    item.get("conclusion") != "skipped" and item.get("status") == "completed"
                    for item in gates)):
            # This tick has not acquired the short admission job's mutex. It
            # cannot already own a full slot and will inspect our decision later.
            return dict(result, kind="pending_observation")
    if artifacts is None:
        artifacts = api.pages(f"actions/runs/{run['id']}/artifacts?per_page=100", "artifacts")
    admission_name = f"ci-periodic-admission-{run['id']}-{run['run_attempt']}"
    matches = [item for item in artifacts if item.get("name") == admission_name]
    if not admission_jobs and not matches:
        # Before admission was deployed every schedule was a full attempt. A
        # failed/cancelled legacy attempt must still block an older green.
        # Partial reruns can also omit admission/select, but cannot reuse an
        # earlier no-op proof. Conservatively retain them as full attempts.
        started = [job["started_at"] for job in current if job.get("started_at")
                   and job.get("conclusion") != "skipped"]
        result["full_started_at"] = (result["full_started_at"] or
                                     min(started, key=timestamp, default=run.get("run_started_at") or run["created_at"]))
    else:
        if len(admission_jobs) != 1 or len(matches) != 1 or matches[0].get("expired"):
            raise ValueError("missing, ambiguous or expired periodic admission evidence")
        decision = validate_admission(api.artifact(matches[0]["id"], "admission.json"), run, api.repository)
        result["decision"] = decision
        if admission_jobs[0].get("status") != "completed" or admission_jobs[0].get("conclusion") != "success":
            raise ValueError("periodic decision is not bound to a successful admission job")
        if not decision["run_full"]:
            conflicts = [item for item in artifacts if re.fullmatch(
                rf"ci-(selection|full-result)-{run['id']}-[1-9][0-9]*", item.get("name", ""))]
            if (not skipped or not other_skipped or conflicts or run.get("status") != "completed"
                    or run.get("conclusion") not in {"success", "failure", "cancelled"}):
                raise ValueError("no-op admission conflicts with actual full execution or incomplete jobs")
            if run["conclusion"] != "success":
                # A terminal no-full attempt can be recovered by a later tick.
                # Keep it unhealthy and release-blocking without treating its
                # successful admission as a full start or active reservation.
                return dict(result, kind="control_failure")
            return dict(result, kind="observation")
        result["reserved_at"] = decision["observed_at"]
    full_artifacts = [item for item in artifacts
                      if item.get("name") == f"ci-full-result-{run['id']}-{run['run_attempt']}"]
    result["successful"] = (run.get("status") == "completed" and run.get("conclusion") == "success"
                             and len(gates) == 1 and gates[0].get("status") == "completed"
                             and gates[0].get("conclusion") == "success"
                             and len(full_artifacts) == 1 and not full_artifacts[0].get("expired"))
    return result


def periodic_history(api, now, *, current_run=None, for_admission=False):
    """Collect complete run pages, then only inspect the relevant history suffix.

    All active runs are inspected even if older than the two-day reporting
    window. GitHub allows a workflow to last 35 days, including waiting, so
    inspect 37 days of metadata to retain completed old attempts in the two-day
    window (also covering the 30-day rerun eligibility). Two-day created ranges
    avoid the API's 1,000-result search cap.
    Completed observations stop at the last full admission (or the last full
    success for health), so ordinary ticks do not download the whole history.
    """
    workflow_id = api.get("actions/workflows/rust.yml").get("id")
    if type(workflow_id) is not int or workflow_id < 1:
        raise ValueError("unknown canonical Rust workflow")
    if current_run is not None:
        trusted_schedule(current_run, workflow_id)
    recent = now - timedelta(days=2)
    query = {"event": "schedule", "branch": "main", "per_page": 100,
             "created": ">=" + recent.strftime("%Y-%m-%dT%H:%M:%SZ")}
    runs = api.pages(f"actions/workflows/{workflow_id}/runs?{urllib.parse.urlencode(query)}", "workflow_runs")
    by_id = {run["id"]: run for run in runs}
    def merge(run):
        trusted_schedule(run, workflow_id)
        previous = by_id.get(run["id"])
        identity = ("head_sha", "run_attempt", "created_at", "workflow_id", "path", "head_branch", "event")
        if previous and (any(previous.get(key) != run.get(key) for key in identity)
                         or (previous.get("status") == "completed"
                             and (run.get("status") != "completed"
                                  or previous.get("conclusion") != run.get("conclusion")))):
            raise ValueError("Actions attempt changed during periodic history collection")
        by_id[run["id"]] = run
    end = recent
    oldest = now - timedelta(days=37)
    while end > oldest:
        begin = max(oldest, end - timedelta(days=2))
        older_query = dict(query, created=begin.strftime("%Y-%m-%dT%H:%M:%SZ") + ".." +
                           (end - timedelta(seconds=1)).strftime("%Y-%m-%dT%H:%M:%SZ"))
        for run in api.pages(f"actions/workflows/{workflow_id}/runs?{urllib.parse.urlencode(older_query)}", "workflow_runs"):
            if attempt_clock(run) >= recent:
                merge(run)
        end = begin
    # The window must not hide a still queued/active full that exceeds its budget.
    for status in sorted(ACTIVE_STATUSES):
        active_query = dict(event="schedule", branch="main", per_page=100, status=status)
        for run in api.pages(f"actions/workflows/{workflow_id}/runs?{urllib.parse.urlencode(active_query)}", "workflow_runs"):
            merge(run)
    observed, classified, completed = [], {}, []
    for run in sorted(by_id.values(), key=lambda item: (attempt_clock(item), item["id"]), reverse=True):
        trusted_schedule(run, workflow_id)
        if current_run is not None and run["id"] == current_run["id"]:
            continue
        if timestamp(run["created_at"]) > now:
            raise ValueError("periodic history contains a future run")
        observed.append(run)
        if run["status"] != "completed":
            classified[run["id"]] = classify_periodic_attempt(api, run, workflow_id)
        else:
            completed.append(run)
    if for_admission and any(item["kind"] == "full" for item in classified.values()):
        # An active full already decides "full_active". Do not spend the read
        # budget proving two days of no-ops while an old full remains blocked.
        completed = []
    with ThreadPoolExecutor(max_workers=4) as reader:
        for index in range(0, len(completed), 4):
            batch = completed[index:index + 4]
            items = list(reader.map(lambda run: classify_periodic_attempt(api, run, workflow_id), batch))
            for run, item in zip(batch, items):
                classified[run["id"]] = item
            full = [value for value in classified.values() if value["kind"] == "full"]
            anchor = max((full_anchor(value) for value in full), default=None)
            enough = bool(full) if for_admission else any(value["successful"] for value in full)
            # Completion order is not start order, especially for reruns. Do
            # not stop at a slow full while a newer start remains unseen.
            remaining = completed[index + len(batch):]
            if enough and (not remaining or attempt_clock(remaining[0]) <= anchor):
                break
    if any(item["decision"] and timestamp(item["decision"]["observed_at"]) > now
           for item in classified.values()):
        raise ValueError("periodic decision is in the future")
    return list(classified.values()), observed, workflow_id


def full_anchor(item):
    return timestamp(item["full_started_at"] or item["reserved_at"] or item["run"]["created_at"])


def periodic_decision(history, context, now):
    full = [item for item in history if item["kind"] == "full"]
    active = [item for item in full if item["run"]["status"] != "completed"]
    previous = max(full, key=full_anchor, default=None)
    gap = (now - full_anchor(previous)).total_seconds() if previous else None
    if gap is not None and gap < 0:
        raise ValueError("periodic full start is in the future")
    due = gap is None or gap >= CADENCE_SECONDS
    run_full = due and not active
    record = None
    if previous:
        run = previous["run"]
        record = {key: run.get(key) for key in ("id", "run_attempt", "head_sha", "created_at", "status", "conclusion")}
        record.update(full_started_at=previous["full_started_at"], reserved_at=previous["reserved_at"])
    return {
        "schema": ADMISSION_SCHEMA, "context": context, "observed_at": now.isoformat(),
        "run_full": run_full, "reason": "full_active" if active else "due" if due else "not_due",
        "cadence_seconds": CADENCE_SECONDS, "previous_full": record,
        "active_full_run_ids": sorted(item["run"]["id"] for item in active),
        "start_gap_seconds": gap, "missed_intervals": max(0, int(gap // CADENCE_SECONDS) - 1) if gap is not None else 0,
        "control_failure_run_ids": sorted(item["run"]["id"] for item in history if item["kind"] == "control_failure"),
    }


def admit_periodic(api, context, now):
    run = api.get(f"actions/runs/{context['run_id']}")
    validate_admission_context(context, run, api.repository)
    if run.get("status") != "in_progress":
        raise ValueError("periodic admission must execute inside its active Actions run")
    history, _, _ = periodic_history(api, now, current_run=run, for_admission=True)
    return validate_admission(periodic_decision(history, context, now), run, api.repository)


def selected_source_for_attempt(api, run, artifacts, workflow_id=None):
    if run.get("event") == "schedule":
        kind = classify_periodic_attempt(api, run, workflow_id or run.get("workflow_id"), artifacts)["kind"]
        if kind == "observation":
            return ""
    selections = []
    for item in artifacts:
        match = re.fullmatch(rf"ci-selection-{run['id']}-([1-9][0-9]*)", item.get("name", ""))
        if match and int(match[1]) <= run["run_attempt"]:
            selections.append((int(match[1]), item))
    if not selections:
        if run.get("event") == "schedule":
            return run.get("head_sha")
        # The same trusted workflow also hosts two explicit non-full diagnostic
        # modes. Their actual started job and skipped full entry prove that this
        # run did not attempt full validation. Missing select evidence alone is
        # never enough to exclude a failed or cancelled full run.
        jobs = api.pages(f"actions/runs/{run['id']}/attempts/{run['run_attempt']}/jobs?per_page=100", "jobs")
        current = [job for job in jobs if job.get("run_attempt") == run["run_attempt"]]
        skipped = all(
            len(found := [job for job in current if job.get("name") == name]) == 1
            and found[0].get("status") == "completed" and found[0].get("conclusion") == "skipped"
            for name in ("select", "explicit-full-gate")
        )
        diagnostic = [job for job in current if job.get("name") in
                      {"newapi-bridge-linux-x86-64", "codeql-rust-model-diagnostic"}
                      and job.get("started_at") and job.get("status") in {"in_progress", "completed"}
                      and job.get("conclusion") != "skipped"]
        return "" if skipped and len(diagnostic) == 1 else None
    attempt = max(value for value, _ in selections)
    current = [item for value, item in selections if value == attempt]
    if len(current) != 1 or current[0].get("expired"):
        raise ValueError("ambiguous or expired selection for a full run")
    current_jobs = api.pages(f"actions/runs/{run['id']}/attempts/{run['run_attempt']}/jobs?per_page=100", "jobs")
    if attempt != run["run_attempt"] and any(
        job.get("name") == "select" and job.get("run_attempt") == run["run_attempt"]
        for job in current_jobs
    ):
        raise ValueError("select was rerun without a current successful selection; source is unknown")
    producing_jobs = (current_jobs if attempt == run["run_attempt"] else
                      api.pages(f"actions/runs/{run['id']}/attempts/{attempt}/jobs?per_page=100", "jobs"))
    selecting = [job for job in producing_jobs if job.get("name") == "select" and job.get("run_attempt") == attempt]
    if (len(selecting) != 1 or selecting[0].get("status") != "completed"
            or selecting[0].get("conclusion") != "success"):
        raise ValueError("selection is not bound to a successful producing select job")
    selected = api.artifact(current[0]["id"], "selection.json")
    if selected.get("scope") != "explicit-full":
        # Non-full diagnostics never execute select in this workflow. A
        # selection with missing or conflicting scope is incomplete evidence,
        # not permission to fall back to a previous green result.
        raise ValueError("selection does not identify a complete full attempt")
    if (selected.get("source_sha") != selected.get("tested_sha")
            or selected.get("policy_sha") != run.get("head_sha")
            or selected.get("execution_profile") not in {"periodic-full", "candidate-full"}):
        raise ValueError("selection does not bind the trusted run and source")
    return commit(selected.get("source_sha"))


def find_verified(api, source, config, digest, run_id=None):
    workflow = api.get("actions/workflows/rust.yml")
    workflow_id = workflow.get("id")
    if type(workflow_id) is not int:
        raise ValueError("unknown canonical Rust workflow")
    if run_id:
        runs = [api.get(f"actions/runs/{run_id}")]
    else:
        runs = []
        cutoff = (datetime.now(timezone.utc) - timedelta(days=30)).strftime("%Y-%m-%dT%H:%M:%SZ")
        for event in ("schedule", "workflow_dispatch"):
            query_args = {"branch": "main", "per_page": 100, "event": event, "created": ">=" + cutoff}
            if event == "schedule":
                # Every scheduled source is its immutable workflow head. Avoid
                # paging through a month of unrelated lightweight observations.
                query_args["head_sha"] = source
            query = urllib.parse.urlencode(query_args)
            runs.extend(api.pages(f"actions/workflows/{workflow_id}/runs?{query}", "workflow_runs"))
        # A rerun keeps the old creation time. Its latest attempt must sort by
        # its real execution/update time, ahead of an earlier successful run.
        runs.sort(key=lambda run: (max(run.get(key) or "" for key in
                                      ("run_started_at", "updated_at", "created_at")),
                                  run.get("id", 0)), reverse=True)
    for run in runs:
        if run.get("event") not in {"schedule", "workflow_dispatch"}:
            continue
        artifacts = list(api.pages(f"actions/runs/{run['id']}/artifacts?per_page=100", "artifacts"))
        name = f"ci-full-result-{run['id']}-{run['run_attempt']}"
        matches = [item for item in artifacts if item.get("name") == name]
        if not matches:
            if run_id:
                raise ValueError("specified run has no full result for its current attempt")
            selected_source = selected_source_for_attempt(api, run, artifacts, workflow_id)
            if selected_source == source:
                raise ValueError("newer full attempt for this source has no successful complete result")
            if selected_source is None:
                raise ValueError("manual attempt has no source selection; use an exact --run-id or run a new full validation")
            continue
        if len(matches) != 1 or matches[0].get("expired"):
            raise ValueError("ambiguous or expired full result artifact")
        receipt = api.artifact(matches[0]["id"])
        if receipt.get("plan", {}).get("source_sha") != source:
            if run_id:
                raise ValueError("specified run tested a different source")
            continue
        # A matching newer attempt must qualify on its own; do not mask it by
        # falling back to an older artifact or an earlier successful attempt.
        jobs = list(api.pages(f"actions/runs/{run['id']}/attempts/{run['run_attempt']}/jobs?per_page=100", "jobs"))
        return verify_receipt(receipt, run, jobs, source=source, repository=api.repository,
                              workflow_id=workflow_id, config=config, digest=digest)
    raise ValueError("no complete trusted full result for this source; run main manual full with source_ref")


def health(runs, now, start_gap=5 * 3600, run_budget=4 * 3600, *, observations=None, control_failures=()):
    scheduled = [run for run in runs if run.get("event") == "schedule" and run.get("head_branch") == "main"]
    started_runs = [run for run in scheduled if run.get("_full_started_at", run["created_at"]) is not None]
    if not started_runs:
        raise ValueError("no scheduled full run; health is unknown")
    def started_at(run):
        return run.get("_full_started_at", run["created_at"]) or run.get("_reserved_at") or run["created_at"]
    latest = max(started_runs, key=lambda run: timestamp(started_at(run)))
    started = timestamp(started_at(latest))
    age = (now - started).total_seconds()
    completed = [run for run in scheduled if run.get("status") == "completed"]
    successes = [run for run in completed if run.get("conclusion") == "success" and run.get("_full_success", True)]
    terminal = max(completed, key=lambda run: run.get("updated_at", run["created_at"]), default=None)
    successful = max(successes, key=lambda run: run.get("updated_at", run["created_at"]), default=None)
    def record(run, full_start=True):
        return ({"run_id": run["id"], "source_sha": run["head_sha"],
                 "created_at": run["created_at"], "updated_at": run.get("updated_at"),
                 **({"full_started_at": run.get("_full_started_at", run["created_at"])} if full_start else {}),
                 "status": run["status"], "conclusion": run.get("conclusion")}
                if run else None)
    report = {
        "run_id": latest["id"], "status": latest["status"], "source_sha": latest["head_sha"],
        "latest_start": record(latest), "latest_terminal": record(terminal),
        "last_successful_run": record(successful),
        "last_successful_sha": successful["head_sha"] if successful else None,
        "commit_range": {"base": successful["head_sha"] if successful else None,
                         "head": latest["head_sha"]},
        "start_age_seconds": age, "start_gap_budget_seconds": start_gap,
        "execution_budget_seconds": run_budget,
        "latest_observation": record(max(observations or scheduled, key=attempt_clock), full_start=False),
        "control_failure_run_ids": [run["id"] for run in control_failures],
        "reserved_full_run_ids": [run["id"] for run in scheduled if run.get("_full_started_at", run["created_at"]) is None],
    }
    report["overdue_run_ids"] = [
        run["id"] for run in scheduled
        if run.get("status") != "completed"
        and (now - timestamp(started_at(run))).total_seconds() > run_budget
    ]
    def fail(reason):
        raise ValueError(reason + "; schedule facts: " + json.dumps(report, sort_keys=True))
    if age > start_gap or age < 0:
        fail("scheduled full start is missing or stale")
    if report["overdue_run_ids"]:
        fail("scheduled full exceeded its execution budget")
    if latest.get("status") == "completed" and (latest.get("conclusion") != "success" or not latest.get("_full_success", True)):
        fail("latest scheduled full did not succeed")
    if not successful:
        fail("no successful scheduled run in the observation window; health is unknown")
    if terminal and (terminal.get("conclusion") != "success" or not terminal.get("_full_success", True)):
        fail("last completed scheduled full did not succeed; the next run has not restored health")
    if any(attempt_clock(run) > attempt_clock(successful) for run in control_failures):
        fail("periodic admission failed; a later complete full has not restored health")
    # These are scheduling facts. Release consumers must still verify the exact
    # source's complete receipt, inventory and actual gate with `verify`.
    return report


def periodic_health(api, now):
    history, observations, _ = periodic_history(api, now)
    full = [dict(item["run"], _full_started_at=item["full_started_at"], _reserved_at=item["reserved_at"],
                 _full_success=item["successful"]) for item in history if item["kind"] == "full"]
    failures = [item["run"] for item in history if item["kind"] == "control_failure"]
    return health(full, now, observations=observations, control_failures=failures)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    produce = commands.add_parser("produce")
    produce.add_argument("--plan", required=True)
    produce.add_argument("--needs", required=True)
    produce.add_argument("--output", type=Path, required=True)
    verify = commands.add_parser("verify")
    verify.add_argument("--source", required=True)
    verify.add_argument("--run-id", type=int)
    verify.add_argument("--allow-candidate-source", action="store_true",
                        help="Keep the trusted main Testnet entry's fixed-candidate packaging contract")
    commands.add_parser("health")
    admission = commands.add_parser("admit-periodic")
    admission.add_argument("--output", type=Path, required=True)
    admission.add_argument("--github-output", type=Path, required=True)
    args = parser.parse_args()
    root = Path(os.environ.get("GITHUB_WORKSPACE", os.getcwd())).resolve()
    config = decode_json(Path(__file__).with_name("ci-required-scope.json").read_bytes())
    if args.command == "produce":
        context = {key: os.environ[variable] for key, variable in {
            "repository": "GITHUB_REPOSITORY", "event_name": "GITHUB_EVENT_NAME",
            "ref": "GITHUB_REF", "workflow_sha": "GITHUB_WORKFLOW_SHA",
            "workflow_ref": "GITHUB_WORKFLOW_REF",
        }.items()}
        context.update(run_id=int(os.environ["GITHUB_RUN_ID"]), run_attempt=int(os.environ["GITHUB_RUN_ATTEMPT"]))
        plan, needs = decode_json(args.plan), decode_json(args.needs)
        digest = None if plan.get("bootstrap_full") else definition_digest(root, plan["policy_sha"])
        receipt = make_receipt(plan, needs, config, context, digest)
        args.output.write_text(json.dumps(receipt, sort_keys=True, indent=2) + "\n")
        print(f"Complete CI result recorded for {plan['source_sha']}.")
        return
    api = GitHub(os.environ["GITHUB_REPOSITORY"], os.environ.get("GH_TOKEN") or os.environ.get("GITHUB_TOKEN"))
    if args.command == "admit-periodic":
        receipt = admit_periodic(api, admission_context(os.environ), datetime.now(timezone.utc))
        args.output.write_text(json.dumps(receipt, sort_keys=True, indent=2) + "\n")
        with args.github_output.open("a") as output:
            output.write(f"run_full={str(receipt['run_full']).lower()}\n")
        print(json.dumps(receipt, sort_keys=True))
        return
    if args.command == "health":
        print(json.dumps(periodic_health(api, datetime.now(timezone.utc)), sort_keys=True))
        return
    if args.run_id is not None and args.run_id < 1:
        raise ValueError("run ID must be positive")
    validate_publication_source(root, args.source, args.allow_candidate_source, os.environ)
    policy = commit(os.environ["GITHUB_WORKFLOW_SHA"])
    require_default_ancestor(root, policy)
    receipt = find_verified(api, args.source, config, definition_digest(root, policy), args.run_id)
    context = receipt["context"]
    print(f"Qualified source {args.source}: https://github.com/{api.repository}/actions/runs/{context['run_id']}/attempts/{context['run_attempt']}")


if __name__ == "__main__":
    try:
        main()
    except (ValueError, KeyError, OSError, subprocess.SubprocessError, zipfile.BadZipFile) as exc:
        print(f"ci-full-result: {exc}", file=sys.stderr)
        raise SystemExit(1)
