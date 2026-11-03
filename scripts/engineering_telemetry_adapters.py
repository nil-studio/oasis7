"""Conservative field projections of existing evidence; no execution or network access.

These projections deliberately do not associate records across artifacts. Callers must
supply verified outer identity separately; a test name cannot establish that identity.
"""
import hashlib
from datetime import datetime
import math
import json
import re

MAX_BYTES = 32 * 1024 * 1024
SHA = re.compile(r"^[0-9a-f]{40}$")
UTC = re.compile(r"^\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d(?:\.\d+)?(?:Z|\+00:00)$")
RESULTS = frozenset(("success", "failure", "cancelled", "skipped", "timed_out", "action_required", "neutral", "stale", "startup_failure"))


def digest(raw):
    if len(raw) > MAX_BYTES:
        raise ValueError("source exceeds byte budget")
    return hashlib.sha256(raw).hexdigest()


def integer(value):
    return value if type(value) is int and 0 <= value <= 2**63 - 1 else None


def stamp(value):
    if not isinstance(value, str) or not UTC.fullmatch(value):
        return None
    try:
        datetime.fromisoformat(value.replace("Z", "+00:00"))
    except ValueError:
        return None
    return value


def sha(value):
    return value if isinstance(value, str) and SHA.fullmatch(value) else None


def project(kind, raw):
    """Return (safe projected records, missing reasons, raw-byte SHA-256).

    Unknown, malformed or partial input cannot imply full coverage. No raw strings,
    paths, command lines, environment, or arbitrary nested fields are copied.
    """
    checksum = digest(raw)
    try:
        text = raw.decode("utf-8")
        obj = [json.loads(line) for line in text.splitlines() if line.strip()] if kind == "process-jsonl" else json.loads(text)
    except (UnicodeError, json.JSONDecodeError, RecursionError) as exc:
        raise ValueError("invalid source encoding or JSON") from exc
    result, missing = [], []
    if kind == "actions-jobs":
        if not isinstance(obj, dict) or not isinstance(obj.get("jobs"), list):
            raise ValueError("invalid Actions jobs response")
        for job in obj["jobs"]:
            if not isinstance(job, dict):
                missing.append("invalid job"); continue
            conclusion = job.get("conclusion")
            record = {"kind": "job", "job_id": integer(job.get("id")),
                      "started_at": stamp(job.get("started_at")), "completed_at": stamp(job.get("completed_at")),
                      "execution_result": conclusion if isinstance(conclusion, str) and conclusion in RESULTS else None}
            if None in (record["job_id"], record["started_at"], record["completed_at"], record["execution_result"]):
                missing.append("job fields unavailable")
            if record["started_at"] and record["completed_at"] and datetime.fromisoformat(record["completed_at"].replace("Z", "+00:00")) < datetime.fromisoformat(record["started_at"].replace("Z", "+00:00")):
                missing.append("reversed job timestamps")
            result.append(record)
        missing.append("pagination, workflow dependencies and run identity not verified")
    elif kind == "process-jsonl":
        for row in obj:
            if not isinstance(row, dict):
                missing.append("invalid process record"); continue
            elapsed = row.get("elapsed_seconds")
            if type(elapsed) not in (int, float) or not math.isfinite(elapsed) or not 0 <= elapsed <= 1e9:
                elapsed = None; missing.append("invalid process elapsed")
            code = row.get("exit_code")
            if type(code) is not int or abs(code) > 2**31:
                code = None; missing.append("missing process exit code")
            started_at = stamp(row.get("started_at"))
            if started_at is None:
                missing.append("invalid or missing process started_at")
            result.append({"kind": "test", "started_at": started_at,
                           "elapsed_seconds": elapsed, "exit_code": code,
                           "timeout": row.get("timeout") if type(row.get("timeout")) is bool else None,
                           "cancelled": row.get("cancelled") if type(row.get("cancelled")) is bool else None,
                           "passed": row.get("passed") if type(row.get("passed")) is bool else None,
                           "count_ok": row.get("count_ok") if type(row.get("count_ok")) is bool else None,
                           "cleanup_result": row.get("cleanup_result") if isinstance(row.get("cleanup_result"), str) and row.get("cleanup_result") in ("clean", "unresolved") else None})
        missing.append("process artifact outer identity not verified")
    elif kind == "build-manifest":
        if not isinstance(obj, dict) or obj.get("schema_version") != 2:
            raise ValueError("unsupported build manifest schema")
        result.append({"kind": "build", "source_sha": sha(obj.get("source_sha")),
                       "tested_sha": sha(obj.get("tested_sha")),
                       "producer_run_id": integer(int(obj["producer_run_id"])) if str(obj.get("producer_run_id", "")).isdigit() else None,
                       "producer_attempt": integer(int(obj["producer_attempt"])) if str(obj.get("producer_attempt", "")).isdigit() else None})
        missing.append("configuration and producer-consumer binding not verified")
    elif kind == "compile-metrics":
        if not isinstance(obj, dict) or obj.get("schema_version") != 2:
            raise ValueError("unsupported compile metrics schema")
        result.append({"kind": "local-measurement", "tested_sha": sha(obj.get("commit_oid"))})
        missing.append("measurement invocation and dirty inputs not verified")
    else:
        raise ValueError("unsupported source kind")
    return result, sorted(set(missing)), checksum
