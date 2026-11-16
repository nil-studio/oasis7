#!/usr/bin/env python3
"""Observe one completed main Rust/default upload using read-only GitHub APIs.

This supplies platform evidence, not coverage or continuous-enablement acceptance.
The shared reader remains the authority for producer and upload provenance.
"""
import argparse
from contextlib import contextmanager
import copy
import datetime as dt
import importlib.util
import json
import os
from pathlib import Path
import re
import signal
import subprocess
import sys
from urllib.parse import quote


_spec = importlib.util.spec_from_file_location("codeql_health", Path(__file__).with_name("codeql-health.py"))
health = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(health)
association = health.association

REPOSITORY = "nil-studio/oasis7"
REF = "refs/heads/main"
CATEGORY = "oasis7/rust-repo/default"
JOB = "CodeQL / rust-repo / default"
WORKFLOW = ".github/workflows/codeql-platform-observation.yml"
API_BUDGET_SECONDS = 240
PAGE_LIMIT = 10
READ_ERRORS = (RuntimeError, ValueError, TypeError, KeyError, OSError, subprocess.TimeoutExpired)


def require(value, message):
    if not value:
        raise ValueError(message)


def target(run_id, attempt, sha):
    require(all(isinstance(value, str) and re.fullmatch(r"[1-9][0-9]{0,19}", value)
                for value in (run_id, attempt)), "producer run/attempt must be positive decimal IDs")
    require(isinstance(sha, str) and re.fullmatch(r"[0-9a-f]{40}", sha),
            "producer SHA must be a full lowercase Git OID")
    return {"repository": REPOSITORY, "run_id": int(run_id), "run_attempt": int(attempt),
            "sha": sha, "ref": REF, "category": CATEGORY, "unit": "rust-repo", "profile": "default"}


def collector_context(root, environment):
    require(environment.get("GITHUB_EVENT_NAME") == "workflow_dispatch"
            and environment.get("GITHUB_REPOSITORY") == REPOSITORY
            and environment.get("GITHUB_REF") == REF
            and environment.get("GITHUB_WORKFLOW_REF") == REPOSITORY + "/" + WORKFLOW + "@" + REF,
            "collector requires its own repository's trusted main dispatch")
    sha = environment.get("GITHUB_SHA", "")
    require(re.fullmatch(r"[0-9a-f]{40}", sha) and environment.get("GITHUB_WORKFLOW_SHA") == sha,
            "collector workflow/source SHA mismatch")
    event = json.loads(Path(environment["GITHUB_EVENT_PATH"]).read_text(encoding="utf-8"))
    require(isinstance(event, dict), "collector event schema invalid")
    repository = event.get("repository", {})
    require(isinstance(repository, dict) and repository.get("full_name") == REPOSITORY
            and repository.get("default_branch") == "main", "collector repository/default branch mismatch")
    checkout = subprocess.run(["git", "-C", str(root), "rev-parse", "HEAD"],
                              text=True, capture_output=True, timeout=10)
    require(checkout.returncode == 0 and checkout.stdout.strip() == sha,
            "collector checkout differs from its workflow SHA")
    identity = target(environment.get("GITHUB_RUN_ID", ""), environment.get("GITHUB_RUN_ATTEMPT", ""), sha)
    return {key: identity[key] for key in ("repository", "run_id", "run_attempt", "sha", "ref")} | {
        "workflow_path": WORKFLOW}


class ReadBudgetExpired(Exception):
    """Not swallowed by the shared reader's per-endpoint error handling."""


@contextmanager
def read_budget(seconds):
    require(0 < seconds <= API_BUDGET_SECONDS, "invalid read budget")
    require(signal.getitimer(signal.ITIMER_REAL) == (0.0, 0.0), "another read timer is already active")
    previous_handler = signal.getsignal(signal.SIGALRM)

    def expired(_signum, _frame):
        raise ReadBudgetExpired("absolute API read budget exhausted; remaining observations unknown")

    signal.signal(signal.SIGALRM, expired)
    signal.setitimer(signal.ITIMER_REAL, seconds)
    try:
        yield
    finally:
        signal.setitimer(signal.ITIMER_REAL, 0)
        signal.signal(signal.SIGALRM, previous_handler)


def initial_receipt(requested, collector, now, budget_seconds):
    errors = []
    return {"schema": "oasis7-codeql-platform-observation/v1", "status": "unknown", "target": requested,
            "collector": collector, "observed_at": now.isoformat(), "read_budget_seconds": budget_seconds,
            "producer_run": None, "producer_job": None, "analyses": None,
            "analysis_results_count": None, "analysis_errors": [], "warnings": [], "errors": errors,
            "findings": {"status": "unknown", "read_status": "not_started", "alerts_read": None,
                         "observed_open_findings": None, "alert_instances_complete": False,
                         "page_limit": PAGE_LIMIT, "per_page": 100,
                         "scope": "current open alerts whose most_recent_instance matches target SHA/ref/category; "
                                  "not new findings attributed to this run attempt; no complete-instance or zero-findings proof"},
            "_health": {"identity": {"repository": REPOSITORY, "ref": REF, "sha": requested.get("sha")},
                        "errors": errors, "jobs": [], "analyses": None, "alerts": None},
            "note": "Read-only platform evidence; manifest/feature/target/macro coverage and continuous enablement remain separate."}


def validate_run(run, requested):
    require(isinstance(run, dict) and run.get("status") == "completed", "producer run must be completed")
    expected = {"id": requested["run_id"], "run_attempt": requested["run_attempt"],
                "head_sha": requested["sha"], "head_branch": "main", "path": association.WORKFLOW}
    require(all(type(run.get(key)) is type(value) and run.get(key) == value for key, value in expected.items()),
            "producer run identity differs from requested run/attempt/SHA/main/workflow")


def validate_analysis(rows, requested, sarif_id):
    require(len(rows) == 1, "target SARIF must have exactly one analysis")
    analysis = rows[0]
    expected = {"sarif_id": sarif_id, "ref": REF, "commit_sha": requested["sha"], "category": CATEGORY}
    require(all(analysis.get(key) == value for key, value in expected.items())
            and isinstance(analysis.get("tool"), dict) and analysis["tool"].get("name") == "CodeQL",
            "analysis identity differs from target SARIF/ref/SHA/category/tool")
    require(association.integer(analysis.get("id")) and isinstance(analysis.get("error"), str)
            and health.timestamp(analysis.get("created_at")) is not None
            and type(analysis.get("results_count")) is int and analysis["results_count"] >= 0,
            "analysis ID/error/created_at/results_count unavailable or invalid")


def error(data, stage, failure):
    # Existing API helpers report the endpoint and status, never gh stderr/token.
    data["errors"].append(stage + ": " + str(failure))


def gather(data):
    requested, health_data = data["target"], data["_health"]
    prefix = "repos/" + REPOSITORY
    run_endpoint = f"{prefix}/actions/runs/{requested['run_id']}"
    run = association.api(run_endpoint)
    data["producer_run"] = run
    validate_run(run, requested)
    jobs = association.pages(f"{run_endpoint}/attempts/{requested['run_attempt']}/jobs", "jobs")
    selected = [job for job in jobs if job.get("name") == JOB]
    require(len(selected) == 1, "Rust/default producer job missing or duplicate")
    job = copy.deepcopy(selected[0])
    job.pop("upload_sarif_id", None)
    data["producer_job"] = job
    require(job.get("status") == "completed", "Rust/default producer job must be completed")
    expected = {"run_id": requested["run_id"], "run_attempt": requested["run_attempt"], "head_sha": requested["sha"]}
    require(all(type(job.get(key)) is type(value) and job.get(key) == value for key, value in expected.items()),
            "Rust/default job identity differs from exact producer attempt")
    job["ref"] = REF
    job["run_started_at"] = run.get("run_started_at") or run.get("created_at")
    start, end = health.timestamp(job.get("started_at")), health.timestamp(job.get("completed_at"))
    job["duration_seconds"] = (end - start).total_seconds() if start and end else None
    health_data["jobs"] = [job]
    reader = association.Reader(REPOSITORY, REF, requested["sha"])
    try:
        errors = reader.associate(run, [job])
        data["errors"].extend("upload association: " + item for item in errors)
        if errors or not job.get("upload_sarif_id"):
            job.pop("upload_sarif_id", None)
            return
    except READ_ERRORS as failure:
        job.pop("upload_sarif_id", None)
        error(data, "upload association", failure)
        return

    try:
        endpoint = ("code-scanning/analyses?sarif_id=" + quote(job["upload_sarif_id"], safe="")
                    + "&ref=" + quote(REF, safe="") + "&tool_name=CodeQL")
        rows = health.read_pages(REPOSITORY, endpoint, limit=PAGE_LIMIT)
        data["analyses"] = rows  # Keep raw errors/warnings, even if identity validation fails.
        for analysis in rows:
            for field in ("error", "errors"):
                if analysis.get(field):
                    data["analysis_errors"].append({"analysis_id": analysis.get("id"), "field": field,
                                                    "value": analysis[field]})
            for field in ("warning", "warnings"):
                if analysis.get(field):
                    data["warnings"].append({"analysis_id": analysis.get("id"), "field": field,
                                             "value": analysis[field]})
        validate_analysis(rows, requested, job["upload_sarif_id"])
        health_data["analyses"] = rows
        data["analysis_results_count"] = rows[0]["results_count"]
    except READ_ERRORS as failure:
        error(data, "analysis read", failure)

    try:
        alerts = health.read_pages(REPOSITORY, "code-scanning/alerts?state=open&ref="
                                   + quote(REF, safe="") + "&tool_name=CodeQL", limit=PAGE_LIMIT)
        ids = [alert.get("number") for alert in alerts]
        require(all(association.integer(number) for number in ids) and len(ids) == len(set(ids)),
                "open alert identity missing or duplicate across pages")
        require(all(alert.get("state") == "open" and isinstance(alert.get("most_recent_instance"), dict)
                    and isinstance(alert.get("tool"), dict) and alert["tool"].get("name") == "CodeQL"
                    for alert in alerts), "open alert API schema invalid")
        matched = []
        for alert in alerts:
            instance = alert["most_recent_instance"]
            require(isinstance(instance.get("commit_sha"), str)
                    and re.fullmatch(r"[0-9a-f]{40}", instance["commit_sha"])
                    and isinstance(instance.get("ref"), str) and instance["ref"].startswith("refs/")
                    and isinstance(instance.get("category"), str) and instance["category"]
                    and instance.get("state") in ("open", "fixed", "dismissed"),
                    "most_recent_instance identity/state unavailable or invalid")
            if (instance.get("commit_sha") == requested["sha"] and instance.get("ref") == REF
                    and instance.get("category") == CATEGORY and instance.get("state") == "open"):
                matched.append(alert)
        data["findings"].update(read_status="completed", alerts_read=len(alerts), observed_open_findings=matched)
        health_data["alerts"] = [dict(alert, instances=[alert["most_recent_instance"]]) for alert in matched]
    except READ_ERRORS as failure:
        data["findings"]["read_status"] = "unknown"
        error(data, "findings read", failure)

    try:
        current = association.api(run_endpoint)
        reader.same_run(run, current)
        validate_run(current, requested)
    except READ_ERRORS as failure:
        # A new attempt invalidates the association, not the retained raw observations.
        job.pop("upload_sarif_id", None)
        error(data, "final producer identity", failure)


def finish(data, now):
    diagnostic = health.report(data.pop("_health"), now)
    data["health"] = diagnostic
    for item in diagnostic["errors"]:
        if item not in data["errors"]:
            data["errors"].append(item)
    selected = next(row for row in diagnostic["units"] if row["category"] == CATEGORY)
    data["target_health"] = selected
    # Preserve any additional platform errors alongside the official singular error.
    if selected["analysis_association"] == "sarif_id" and data["analysis_errors"]:
        selected["upload_status"] = "failed"
    data["findings"]["status"] = selected["finding_status"]
    if selected["upload_status"] == "failed" or selected["execution_status"] in ("failure", "cancelled", "timed_out"):
        data["status"] = "failed"
    elif (not data["errors"] and selected["execution_status"] == "success"
          and selected["upload_status"] == "accepted"):
        data["status"] = "observed"
    return data


def observe(requested, collector=None, now=None, budget_seconds=API_BUDGET_SECONDS):
    now = now or dt.datetime.now(dt.timezone.utc)
    data = initial_receipt(requested, collector, now, budget_seconds)
    try:
        with read_budget(budget_seconds):
            gather(data)
    except (ReadBudgetExpired, *READ_ERRORS) as failure:
        # No final run revalidation completed after an interrupted collection.
        for job in data["_health"]["jobs"]:
            job.pop("upload_sarif_id", None)
        error(data, "collection", failure)
    except Exception as failure:
        # Unexpected API shape/code failures still produce an honest failure artifact.
        for job in data["_health"]["jobs"]:
            job.pop("upload_sarif_id", None)
        data["errors"].append("collection: unexpected " + type(failure).__name__)
    return finish(data, now)


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--producer-run-id", required=True)
    parser.add_argument("--producer-run-attempt", required=True)
    parser.add_argument("--producer-sha", required=True)
    parser.add_argument("--output", required=True)
    args = parser.parse_args(argv)
    now = dt.datetime.now(dt.timezone.utc)
    try:
        requested = target(args.producer_run_id, args.producer_run_attempt, args.producer_sha)
        context = collector_context(Path(__file__).resolve().parents[2], os.environ)
    except READ_ERRORS as failure:
        result = initial_receipt({}, None, now, API_BUDGET_SECONDS)
        error(result, "collector admission", failure)
        result = finish(result, now)
    else:
        result = observe(requested, context, now)
    output = Path(args.output)
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(result, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    summary = ("## CodeQL platform observation\n\n"
               f"Status: `{result['status']}`. Upload: `{result['target_health']['upload_status']}`. "
               f"Findings: `{result['findings']['status']}`. Errors: {len(result['errors'])}.\n\n"
               "Read platform-observation.json for exact producer identity, analysis warnings and findings scope. "
               "This observation does not accept source coverage or continuous enablement.\n")
    if os.environ.get("GITHUB_STEP_SUMMARY"):
        with open(os.environ["GITHUB_STEP_SUMMARY"], "a", encoding="utf-8") as destination:
            destination.write(summary)
    print(summary)
    return 0 if result["status"] == "observed" else 1


if __name__ == "__main__":
    sys.exit(main())
