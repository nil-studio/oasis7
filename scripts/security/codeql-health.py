#!/usr/bin/env python3
"""Read-only CodeQL health. API gaps remain unknown, never zero findings.

Fixture input uses the same API-shaped records as the bounded live reader.
This report is diagnostic data, not a merge receipt or scanning attestation.
"""
import argparse
import datetime as dt
import json
import re
import subprocess
import sys
import importlib.util
from pathlib import Path

_association_spec = importlib.util.spec_from_file_location(
    "codeql_upload_association", Path(__file__).with_name("codeql_upload_association.py"))
association = importlib.util.module_from_spec(_association_spec)
_association_spec.loader.exec_module(association)

UNITS = {"actions-repo": "actions", "python-repo": "python",
         "javascript-repo": "javascript-typescript", "rust-repo": "rust"}


def timestamp(value):
    try:
        result = dt.datetime.fromisoformat(value.replace("Z", "+00:00"))
        return result if result.tzinfo else None
    except (ValueError, TypeError, AttributeError):
        return None


def report(data, now, max_age_hours=24):
    errors = list(data.get("errors", []))
    identities = data.get("identity", {})
    ref, sha = identities.get("ref"), identities.get("sha")
    identity_valid = isinstance(ref, str) and ref.startswith("refs/") and bool(re.fullmatch(r"[0-9a-f]{40}", str(sha)))
    if not identity_valid:
        errors.append("missing or invalid requested ref/sha identity")
    analyses = data.get("analyses")
    jobs = data.get("jobs")
    alerts = data.get("alerts")
    units = []
    for profile in ("default", "extended"):
        for unit, language in UNITS.items():
            category = f"oasis7/{unit}/{profile}"
            matches = [a for a in (analyses or []) if identity_valid and
                       a.get("category") == category and a.get("ref") == ref and
                       a.get("commit_sha") == sha and a.get("tool", {}).get("name") == "CodeQL"]
            matches.sort(key=lambda a: a.get("created_at", ""), reverse=True)
            previous_analysis = matches[0] if matches else None
            job_matches = [j for j in (jobs or []) if j.get("name") == f"CodeQL / {unit} / {profile}"
                           and j.get("head_sha") == sha and j.get("ref") == ref]
            job_matches.sort(key=lambda j: (j.get("run_started_at") or j.get("started_at") or "",
                                           j.get("run_id", 0), j.get("run_attempt", 0)), reverse=True)
            job = job_matches[0] if job_matches else None
            # SHA/ref/category alone also match older reruns. Accept only an explicit
            # SARIF link to this exact run attempt; API records without that link
            # remain unknown rather than borrowing a previous attempt's success.
            attempt_bound = job and all(isinstance(job.get(k), int) and not isinstance(job.get(k), bool)
                                        and job[k] > 0 for k in ("run_id", "run_attempt"))
            associated = [a for a in matches if attempt_bound and job.get("upload_sarif_id")
                          and a.get("sarif_id") == job["upload_sarif_id"]]
            analysis = associated[0] if len(associated) == 1 else None
            execution = "unknown"
            if job:
                raw_steps = job.get("steps", [])
                try:
                    execution = association.execution_status(job)
                except RuntimeError as error:
                    errors.append(str(error))
                    raw_steps = []
            upload = "unknown"
            upload_step_status = "unknown"
            if job:
                upload_steps = [s for s in raw_steps if s.get("name") == "CodeQL SARIF upload"]
                if len(upload_steps) == 1:
                    upload_step_status = upload_steps[0].get("conclusion") or upload_steps[0].get("status") or "unknown"
                if upload_step_status in ("failure", "cancelled", "timed_out", "skipped"):
                    upload = "failed" if upload_step_status == "failure" else upload_step_status
                elif upload_step_status == "success" and analysis:
                    upload = "failed" if analysis.get("error") else "accepted"
                elif job.get("status") in ("queued", "in_progress", "waiting", "pending"):
                    upload = job["status"]
                elif job.get("conclusion") in ("cancelled", "timed_out"):
                    upload = job["conclusion"]
            # Job success includes upload and does not prove a clean finding result.
            relevant_alerts = [a for a in (alerts or []) if any(
                i.get("category") == category and i.get("commit_sha") == sha and i.get("ref") == ref
                for i in a.get("instances", []))]
            findings = ("unknown" if alerts is None or not analysis else "open" if relevant_alerts else
                        "no_open_findings" if data.get("alert_instances_complete") is True else "unknown")
            completed = timestamp(analysis.get("created_at")) if analysis else None
            age = (now - completed).total_seconds() / 3600 if completed else None
            previous_completed = timestamp(previous_analysis.get("created_at")) if previous_analysis else None
            previous_age = (now - previous_completed).total_seconds() / 3600 if previous_completed else None
            coverage = data.get("coverage", {}).get(category)
            units.append({"unit": unit, "language": language, "profile": profile, "category": category,
                          "execution_status": execution, "upload_status": upload, "finding_status": findings,
                          "upload_step_status": upload_step_status,
                          "run_id": job.get("run_id") if job else None,
                          "run_attempt": job.get("run_attempt") if job else None,
                          "observed_job_identity": job.get("observed_job_identity") if job else None,
                          "analysis_association": "sarif_id" if analysis else "unknown",
                          "previous_analysis_id": previous_analysis.get("id") if previous_analysis else None,
                          "previous_analysis_age_hours": previous_age,
                          "job_status": job.get("conclusion") or job.get("status") if job else "unknown",
                          "open_findings": len(relevant_alerts) if findings != "unknown" else None,
                          "coverage_age_hours": age, "fresh": age is not None and 0 <= age <= max_age_hours,
                          "missing_manifests": coverage.get("missing_manifests") if isinstance(coverage, dict) else None,
                          "coverage_status": "observed" if isinstance(coverage, dict) else "unknown",
                          "analysis_id": analysis.get("id") if analysis else None,
                          "tool_version": analysis.get("tool", {}).get("version") if analysis else None,
                          "duration_seconds": job.get("duration_seconds") if job else None})
    healthy = all(u["fresh"] and u["upload_status"] == "accepted" and u["execution_status"] == "success" for u in units)
    return {"schema": "oasis7-codeql-health/v1", "identity": identities,
            "status": "unknown" if errors else "healthy" if healthy else "incomplete_or_stale",
            "errors": errors, "units": units,
            "finding_trend": "unknown", "note": "fixture/API observations are not hosted coverage proof"}


def read_pages(repo, endpoint, limit=10):
    records = []
    for page in range(1, limit + 1):
        separator = "&" if "?" in endpoint else "?"
        result = subprocess.run(["gh", "api", f"repos/{repo}/{endpoint}{separator}per_page=100&page={page}"],
                                text=True, capture_output=True, timeout=30)
        if result.returncode:
            raise RuntimeError(f"{endpoint}: API read failed ({result.returncode})")
        value = json.loads(result.stdout)
        batch = value if isinstance(value, list) else value.get("workflow_runs", value.get("jobs"))
        if not isinstance(batch, list) or not all(isinstance(row, dict) for row in batch):
            raise RuntimeError(f"{endpoint}: unexpected API schema")
        records.extend(batch)
        if len(batch) < 100:
            return records
    raise RuntimeError(f"{endpoint}: page budget exceeded; incomplete data discarded")


def live(repo, ref, sha):
    from urllib.parse import quote
    data = {"identity": {"repository": repo, "ref": ref, "sha": sha}, "errors": []}
    for key, endpoint in (("analyses", f"code-scanning/analyses?ref={quote(ref, safe='')}"),
                          ("runs", "actions/workflows/codeql.yml/runs"),
                          ("alerts", f"code-scanning/alerts?state=open&ref={quote(ref, safe='')}")):
        try:
            data[key] = read_pages(repo, endpoint)
        except (RuntimeError, subprocess.TimeoutExpired, json.JSONDecodeError) as error:
            data[key] = None
            data["errors"].append(str(error))
    jobs = []
    reader = association.Reader(repo, ref, sha)
    branch = ref.removeprefix("refs/heads/") if ref.startswith("refs/heads/") else None
    relevant_runs = [r for r in (data.get("runs") or []) if branch and r.get("head_sha") == sha
                     and r.get("head_branch") == branch and r.get("path") == ".github/workflows/codeql.yml"]
    run_ids = [run.get("id") for run in relevant_runs]
    duplicate_runs = len(run_ids) != len(set(run_ids))
    if len(relevant_runs) > 20:
        data["errors"].append("run budget exceeded")
    else:
        for run in relevant_runs:
            try:
                attempt = run.get("run_attempt")
                if not isinstance(attempt, int) or isinstance(attempt, bool) or attempt < 1:
                    raise RuntimeError("run attempt identity unavailable")
                attempt_jobs = read_pages(repo, f"actions/runs/{run['id']}/attempts/{attempt}/jobs")
                for job in attempt_jobs:
                    # Platform outputs are unavailable in jobs API. Ignore any
                    # caller-shaped field until trusted artifact validation.
                    job.pop("upload_sarif_id", None)
                try:
                    if duplicate_runs:
                        raise RuntimeError("duplicate run identity")
                    errors = reader.associate(run, attempt_jobs)
                    data["errors"].extend("upload association: " + error for error in errors)
                except (RuntimeError, subprocess.TimeoutExpired, ValueError, TypeError, KeyError) as error:
                    for job in attempt_jobs:
                        job.pop("upload_sarif_id", None)
                    data["errors"].append("upload association: " + str(error))
                for job in attempt_jobs:
                    expected_identity = {"head_sha": run["head_sha"], "run_id": run["id"], "run_attempt": attempt}
                    if any(type(job.get(key)) is not type(value) or job.get(key) != value
                           for key, value in expected_identity.items()):
                        # An exact-attempt endpoint may return reused jobs from a
                        # previous attempt. Preserve that observation separately;
                        # the current-attempt diagnostic has no observed steps.
                        observed_identity = {key: job.get(key) for key in expected_identity}
                        data["errors"].append("diagnostic job identity missing or differs from current run attempt")
                        job = {"name": job.get("name"), "status": "unknown", "conclusion": None,
                               "steps": [], "observed_job_identity": observed_identity}
                    job["head_sha"], job["ref"] = run["head_sha"], ref
                    job["run_id"], job["run_attempt"] = run["id"], attempt
                    job["run_started_at"] = run.get("run_started_at") or run.get("created_at")
                    start, end = timestamp(job.get("started_at")), timestamp(job.get("completed_at"))
                    job["duration_seconds"] = (end - start).total_seconds() if start and end else None
                    jobs.append(job)
            except (RuntimeError, subprocess.TimeoutExpired, json.JSONDecodeError) as error:
                data["errors"].append(str(error))
    data["jobs"] = jobs
    # Alerts API supplies most_recent_instance, not a complete historical scan.
    if data.get("alerts") is not None:
        for alert in data["alerts"]:
            alert["instances"] = [alert.get("most_recent_instance", {})]
    return data


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--fixture")
    parser.add_argument("--repo")
    parser.add_argument("--ref", default="refs/heads/main")
    parser.add_argument("--sha")
    parser.add_argument("--now")
    args = parser.parse_args()
    if args.fixture:
        with open(args.fixture, encoding="utf-8") as source:
            data = json.load(source)
    else:
        if not args.repo or not re.fullmatch(r"[\w.-]+/[\w.-]+", args.repo) or not re.fullmatch(r"[0-9a-f]{40}", args.sha or ""):
            parser.error("live reads require --repo owner/name and exact --sha")
        data = live(args.repo, args.ref, args.sha)
    now = timestamp(args.now) if args.now else dt.datetime.now(dt.timezone.utc)
    if now is None:
        parser.error("--now must be a timezone-aware ISO timestamp")
    print(json.dumps(report(data, now), indent=2))
    return 0


if __name__ == "__main__":
    sys.exit(main())
