#!/usr/bin/env python3
"""Offline engineering telemetry snapshot and report (no network or build execution)."""
import argparse
import hashlib
import json
import os
import re
import math
import tempfile
from pathlib import Path
import sys

SNAPSHOT = "oasis7-engineering-telemetry-snapshot/v1"
REPORT = "oasis7-engineering-telemetry-report/v1"
# Adapter input is deliberately explicit: source-specific parsing is future work.
INPUT = "oasis7-engineering-telemetry-input/v1"
FIELDS = frozenset(("id", "kind", "parent_id", "depends_on", "repository", "run_id",
    "run_attempt", "job_id", "step_number", "source_sha", "tested_sha", "policy_sha",
    "config_digest", "local_invocation_id", "dirty_inputs_digest", "scope", "target",
    "profile", "features", "toolchain", "runner", "phase", "started_at", "completed_at",
    "elapsed_seconds", "clock", "execution_result", "collection_status", "missing_reasons",
    "coverage", "field_sources", "exit_code", "timeout", "cancelled", "passed", "count_ok", "cleanup_result", "producer_run_id", "producer_attempt"))
SOURCES = frozenset(("actions", "process-jsonl", "build-manifest", "build-report", "compile-metrics"))


MAX_INPUT_BYTES = 32 * 1024 * 1024
# Free-form text and nested objects are intentionally excluded from this narrow
# interchange format. A producer must not place secrets in identifier fields.
TOKEN = re.compile(r"^[A-Za-z0-9][A-Za-z0-9_.:@+/-]{0,255}$")
ENUMS = {
    "kind": {"run", "job", "step", "phase", "test"},
    "coverage": {"complete", "partial", "unknown"},
    "collection_status": {"complete", "partial", "unknown"},
    "execution_result": {"success", "failure", "cancelled", "skipped", "timed_out", "unknown", "neutral", "action_required", "in_progress", "stale", "startup_failure"},
    "clock": {"wall", "monotonic", "unknown"},
    "cleanup_result": {"clean", "unresolved"},
}
NUMBERS = {"run_id", "run_attempt", "job_id", "step_number", "producer_run_id", "producer_attempt"}
BOOLEANS = {"timeout", "cancelled", "passed", "count_ok"}
LISTS = {"depends_on", "features", "missing_reasons"}
# Excluded until a safe, documented structured schema exists.
EXCLUDED = {"field_sources", "scope"}

def token(value):
    return (isinstance(value, str) and bool(TOKEN.fullmatch(value))
            and not value.startswith(("/", ".")) and ".." not in value.split("/")
            and not any(x in value.lower() for x in ("token", "secret", "password")))

def valid_record(item):
    if not isinstance(item, dict) or set(item) - FIELDS or set(item) & EXCLUDED:
        return False
    if not token(item.get("id")):
        return False
    for key, value in item.items():
        if key in ENUMS:
            if not isinstance(value, str) or value not in ENUMS[key]:
                return False
        elif key in NUMBERS:
            if type(value) is not int or value < 0:
                return False
        elif key in BOOLEANS:
            if type(value) is not bool:
                return False
        elif key == "exit_code":
            if type(value) is not int or abs(value) > 2**31:
                return False
        elif key == "elapsed_seconds":
            if type(value) not in (int, float) or not math.isfinite(value) or value < 0:
                return False
        elif key in LISTS:
            if not isinstance(value, list) or not all(token(v) for v in value):
                return False
        elif not token(value):
            return False
    return True

def read_json(path):
    with open(path, "rb") as stream:
        data = stream.read(MAX_INPUT_BYTES + 1)
    if len(data) > MAX_INPUT_BYTES:
        raise ValueError("input exceeds byte budget")
    return json.loads(data.decode("utf-8"))

def read_bytes(path):
    with open(path, "rb") as stream:
        data = stream.read(MAX_INPUT_BYTES + 1)
    if len(data) > MAX_INPUT_BYTES:
        raise ValueError("input exceeds byte budget")
    return data


def canonical(value):
    return json.dumps(value, sort_keys=True, ensure_ascii=False, separators=(",", ":"))


def collect(document):
    if not isinstance(document, dict) or document.get("schema_version") != INPUT:
        raise ValueError("unsupported input schema")
    origin = document.get("origin")
    if origin not in ("github", "local"):
        raise ValueError("origin must be github or local")
    for key in ("collection_id", "collected_at"):
        if not token(document.get(key)):
            raise ValueError("missing " + key)
    inputs = document.get("sources")
    if not isinstance(inputs, list):
        raise ValueError("sources must be a list")
    records, evidence, missing = [], [], ["source completeness not verified"]
    if not inputs:
        missing.append("no sources")
    for source in inputs:
        if not isinstance(source, dict) or source.get("kind") not in SOURCES:
            missing.append("unsupported source kind")
            continue
        # Raw source content is not copied to output. References must be relative and cannot escape.
        reference = source.get("reference")
        if not token(reference) or Path(reference).is_absolute() or ".." in Path(reference).parts:
            missing.append("invalid evidence reference")
            continue
        payload = source.get("records")
        if not isinstance(payload, list):
            missing.append("invalid source records: " + reference)
            continue
        digest = hashlib.sha256(canonical(payload).encode()).hexdigest()
        ev = {"kind": source["kind"], "reference": reference,
              "source_schema": INPUT if source.get("source_schema") == INPUT else "unknown", "records_sha256": digest}
        evidence.append(ev)
        if source.get("source_schema") != INPUT:
            missing.append("unsupported source schema: " + reference)
            continue
        for item in payload:
            if not valid_record(item):
                missing.append("invalid or unsafe record: " + reference)
                continue
            # This prepared-input format cannot prove source or DAG completeness.
            missing.append("source completeness not verified")
            record = dict(item)
            record["evidence"] = [reference]
            records.append(record)
    # Distinct versions of an identity survive; exact duplicates combine evidence.
    grouped = {}
    for occurrence, record in enumerate(records):
        if (record.get("repository") and record.get("run_id") and record.get("run_attempt")
                and record.get("job_id")):
            ident = ("github", record["repository"], record["run_id"], record["run_attempt"],
                     record["job_id"], record.get("step_number"), record["id"])
        elif record.get("local_invocation_id") and origin == "local":
            ident = ("local", record["local_invocation_id"], record["id"])
        else:
            # Unverified identity cannot establish duplicates or conflicts.
            ident = ("unidentified", occurrence)
        body = {k: v for k, v in record.items() if k != "evidence"}
        grouped.setdefault(ident, {}).setdefault(canonical(body), record)
        target = grouped[ident][canonical(body)]
        target["evidence"] = sorted(set(target["evidence"] + record["evidence"]))
    output = []
    for versions in grouped.values():
        conflict = len(versions) > 1
        for record in versions.values():
            record["identity_conflict"] = conflict
            output.append(record)
        if conflict:
            missing.append("identity conflict")
    output.sort(key=lambda x: (str(x.get("run_attempt", "")), x["id"], canonical(x)))
    return {"schema_version": SNAPSHOT, "origin": origin, "collection_id": document["collection_id"],
            "collected_at": document["collected_at"], "records": output, "evidence": evidence,
            "collection_status": "partial" if missing else "complete",
            "missing_reasons": sorted(set(missing))}


def report(snapshot):
    if not isinstance(snapshot, dict) or snapshot.get("schema_version") != SNAPSHOT:
        raise ValueError("unsupported snapshot schema")
    records = snapshot.get("records")
    if not isinstance(records, list):
        raise ValueError("invalid snapshot records")
    if (set(snapshot) != {"schema_version", "origin", "collection_id", "collected_at", "records", "evidence", "collection_status", "missing_reasons"}
            or snapshot["origin"] not in ("github", "local")
            or not token(snapshot["collection_id"]) or not token(snapshot["collected_at"])
            or snapshot["collection_status"] != "partial"
            or not isinstance(snapshot["evidence"], list)
            or not isinstance(snapshot["missing_reasons"], list)):
        raise ValueError("invalid snapshot metadata")
    for ev in snapshot["evidence"]:
        if (not isinstance(ev, dict) or set(ev) not in ({"kind", "reference", "source_schema", "records_sha256"}, {"kind", "reference", "source_schema", "records_sha256", "raw_sha256"})
                or ev["kind"] not in SOURCES or not token(ev["reference"])
                or ev["source_schema"] not in (INPUT, "unknown")
                or not re.fullmatch(r"[0-9a-f]{64}", ev["records_sha256"])
                or ("raw_sha256" in ev and not re.fullmatch(r"[0-9a-f]{64}", ev["raw_sha256"]))):
            raise ValueError("invalid snapshot evidence")
    for record in records:
        if not isinstance(record, dict) or set(record) - (FIELDS | {"evidence", "identity_conflict"}):
            raise ValueError("invalid snapshot record")
        body = {k: v for k, v in record.items() if k not in ("evidence", "identity_conflict")}
        if (not valid_record(body) or type(record.get("identity_conflict")) is not bool
                or not isinstance(record.get("evidence"), list)
                or not all(token(x) for x in record["evidence"])):
            raise ValueError("invalid snapshot record")
    # Only collector-generated reason patterns are accepted, never arbitrary text.
    reasons = snapshot["missing_reasons"]
    prefixes = ("invalid source records: ", "unsupported source schema: ", "invalid or unsafe record: ")
    adapter_reasons = {"invalid job", "job fields unavailable", "reversed job timestamps",
        "pagination, workflow dependencies and run identity not verified", "invalid process record",
        "invalid process elapsed", "missing process exit code", "process artifact outer identity not verified",
        "configuration and producer-consumer binding not verified", "measurement invocation and dirty inputs not verified"}
    fixed = adapter_reasons | {"no sources", "unsupported source kind", "invalid evidence reference", "identity conflict", "source completeness not verified", "raw source identity and completeness not verified"}
    if not all(isinstance(x, str) and (x in fixed or any(x.startswith(p) and token(x[len(p):]) for p in prefixes)) for x in reasons):
        raise ValueError("invalid snapshot reasons")
    # No inference about execution success, timing, or complete DAG from this initial slice.
    return {"schema_version": REPORT, "snapshot_schema": SNAPSHOT,
            "collection_id": snapshot.get("collection_id"), "origin": snapshot.get("origin"),
            "collection_status": snapshot.get("collection_status", "unknown"),
            "missing_reasons": snapshot.get("missing_reasons", []), "records": records,
            "evidence": snapshot.get("evidence", []),
            "analysis": {"status": "unavailable", "reason": "timing and DAG analysis not implemented"}}


def markdown(result):
    return ("# Engineering telemetry (offline)\n\n"
            + "- Collection: " + str(result["collection_id"]) + "\n"
            + "- Origin: " + str(result["origin"]) + "\n"
            + "- Collection status: " + str(result["collection_status"]) + "\n"
            + "- Records: " + str(len(result["records"])) + "\n"
            + "- Analysis: unavailable (timing and DAG analysis not implemented)\n"
            + "- Missing reasons: " + (", ".join(result["missing_reasons"]) or "none reported") + "\n")


def write_json(path, value):
    atomic_write(path, json.dumps(value, ensure_ascii=False, sort_keys=True, indent=2) + "\n")


def atomic_write(path, content):
    target = Path(path)
    fd, tmp = tempfile.mkstemp(dir=target.parent, prefix="." + target.name + ".")
    try:
        with os.fdopen(fd, "w", encoding="utf-8") as stream:
            stream.write(content)
        os.replace(tmp, target)
    finally:
        if os.path.exists(tmp):
            os.unlink(tmp)


def distinct_paths(*paths):
    resolved = [Path(p).resolve() for p in paths]
    for index, left in enumerate(resolved):
        for right in resolved[index + 1:]:
            if left == right or (left.exists() and right.exists() and os.path.samefile(left, right)):
                raise ValueError("input and output paths must be distinct")


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)
    c = sub.add_parser("collect", help="freeze explicitly prepared offline input")
    c.add_argument("input"); c.add_argument("output")
    source = sub.add_parser("collect-source", help="project one raw offline source as partial evidence")
    source.add_argument("kind", choices=("actions-jobs", "process-jsonl", "build-manifest", "compile-metrics"))
    source.add_argument("input"); source.add_argument("output")
    source.add_argument("--reference", required=True, help="safe relative evidence reference")
    source.add_argument("--collection-id", required=True); source.add_argument("--collected-at", required=True)
    source.add_argument("--origin", required=True, choices=("github", "local"))
    r = sub.add_parser("report", help="report a frozen snapshot offline")
    r.add_argument("snapshot"); r.add_argument("json_output"); r.add_argument("markdown_output")
    args = parser.parse_args(argv)
    try:
        if args.command == "collect":
            distinct_paths(args.input, args.output)
            result = collect(read_json(args.input))
            write_json(args.output, result)
            return 1 if result["collection_status"] != "complete" else 0
        elif args.command == "collect-source":
            import engineering_telemetry_adapters as adapters
            distinct_paths(args.input, args.output)
            raw = read_bytes(args.input)
            rows, reasons, raw_sha = adapters.project(args.kind, raw)
            records = []
            for index, row in enumerate(rows):
                row = {k: v for k, v in row.items() if v is not None}
                row["id"] = "record-" + str(index)
                if row["kind"] in ("build", "local-measurement"):
                    row["kind"] = "phase"
                records.append(row)
            kind = "actions" if args.kind == "actions-jobs" else args.kind
            doc = {"schema_version": INPUT, "origin": args.origin, "collection_id": args.collection_id,
                   "collected_at": args.collected_at, "sources": [{"kind": kind, "reference": args.reference,
                   "source_schema": INPUT, "records": records}]}
            result = collect(doc)
            if not result["evidence"]:
                raise ValueError("invalid evidence reference")
            result["evidence"][0]["raw_sha256"] = raw_sha
            result["missing_reasons"].extend(reasons)
            result["missing_reasons"].append("raw source identity and completeness not verified")
            result["missing_reasons"] = sorted(set(result["missing_reasons"]))
            write_json(args.output, result)
            return 1
        else:
            distinct_paths(args.snapshot, args.json_output, args.markdown_output)
            result = report(read_json(args.snapshot))
            write_json(args.json_output, result)
            atomic_write(args.markdown_output, markdown(result))
    except (OSError, ValueError, TypeError, RecursionError) as exc:
        print("telemetry: " + str(exc), file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
