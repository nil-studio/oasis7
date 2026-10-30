#!/usr/bin/env python3
"""Bind a complete CI result to its source, approved policy and Actions attempt."""
from __future__ import annotations

import argparse
from datetime import datetime, timedelta, timezone
import hashlib
import importlib.util
import io
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import urllib.error
import urllib.parse
import urllib.request
import zipfile


SCHEMA = "oasis7.ci-full-result/v1"
WORKFLOW = ".github/workflows/rust.yml"
MAX_BYTES = 2 * 1024 * 1024
POLICY_FILES = (
    WORKFLOW, ".github/workflows/command-center.yml",
    "scripts/ci-required-scope.json", "scripts/plan-rust-required-scope.py",
    "scripts/ci-tests.sh", "scripts/ci-test-artifacts.py",
    "scripts/ci-formal-process-suites.json", "scripts/ci-formal-plan.py",
    "scripts/ci_process_runner.py", "scripts/ci-required-result.py",
    "scripts/ci-full-result.py",
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
    digest = hashlib.sha256()
    for path in POLICY_FILES:
        content = subprocess.check_output(
            ["git", "-C", str(root), "show", f"{commit(revision)}:{path}"],
            stderr=subprocess.PIPE,
        )
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

    def request(self, path):
        url = path if path.startswith("https://") else self.root + path
        if not url.startswith(self.root):
            raise ValueError("API pagination escaped the repository")
        req = urllib.request.Request(url, headers={
            "Authorization": f"Bearer {self.token}", "Accept": "application/vnd.github+json",
            "X-GitHub-Api-Version": "2022-11-28", "User-Agent": "oasis7-ci-full-result",
        })
        return self.opener.open(req, timeout=30)

    def get(self, path):
        with self.request(path) as response:
            raw = response.read(MAX_BYTES + 1)
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
            response = urllib.request.urlopen(location, timeout=30)
        with response:
            archive = response.read(MAX_BYTES + 1)
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


def selected_source_for_attempt(api, run, artifacts):
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
            query = urllib.parse.urlencode({"branch": "main", "per_page": 100, "event": event, "created": ">=" + cutoff})
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
            selected_source = selected_source_for_attempt(api, run, artifacts)
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


def health(runs, now, start_gap=5 * 3600, run_budget=4 * 3600):
    scheduled = [run for run in runs if run.get("event") == "schedule" and run.get("head_branch") == "main"]
    if not scheduled:
        raise ValueError("no scheduled full run; health is unknown")
    latest = max(scheduled, key=lambda run: run["created_at"])
    started = datetime.fromisoformat(latest["created_at"].replace("Z", "+00:00"))
    age = (now - started).total_seconds()
    completed = [run for run in scheduled if run.get("status") == "completed"]
    successes = [run for run in completed if run.get("conclusion") == "success"]
    terminal = max(completed, key=lambda run: run.get("updated_at", run["created_at"]), default=None)
    successful = max(successes, key=lambda run: run.get("updated_at", run["created_at"]), default=None)
    def record(run):
        return ({"run_id": run["id"], "source_sha": run["head_sha"],
                 "created_at": run["created_at"], "updated_at": run.get("updated_at"),
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
    }
    report["overdue_run_ids"] = [
        run["id"] for run in scheduled
        if run.get("status") != "completed"
        and (now - datetime.fromisoformat(run["created_at"].replace("Z", "+00:00"))).total_seconds() > run_budget
    ]
    def fail(reason):
        raise ValueError(reason + "; schedule facts: " + json.dumps(report, sort_keys=True))
    if age > start_gap or age < 0:
        fail("scheduled full start is missing or stale")
    if report["overdue_run_ids"]:
        fail("scheduled full exceeded its execution budget")
    if latest.get("status") == "completed" and latest.get("conclusion") != "success":
        fail("latest scheduled full did not succeed")
    if not successful:
        fail("no successful scheduled run in the observation window; health is unknown")
    if terminal and terminal.get("conclusion") != "success":
        fail("last completed scheduled full did not succeed; the next run has not restored health")
    # These are scheduling facts. Release consumers must still verify the exact
    # source's complete receipt, inventory and actual gate with `verify`.
    return report


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
    if args.command == "health":
        cutoff = (datetime.now(timezone.utc) - timedelta(days=2)).strftime("%Y-%m-%dT%H:%M:%SZ")
        query = urllib.parse.urlencode({"event": "schedule", "branch": "main", "per_page": 100, "created": ">=" + cutoff})
        runs = api.pages(f"actions/workflows/rust.yml/runs?{query}", "workflow_runs")
        print(json.dumps(health(runs, datetime.now(timezone.utc)), sort_keys=True))
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
