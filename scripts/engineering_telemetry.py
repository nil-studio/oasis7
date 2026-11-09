#!/usr/bin/env python3
"""Read-only bounded engineering telemetry collection and offline reports; no builds."""
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
    if item.get("kind") == "job" and "step_number" in item:
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
    if isinstance(document, dict) and document.get("schema_version") == CONTRACT_INPUT:
        import engineering_telemetry_adapters as adapters
        return adapters.collect_contract(document)
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
    references = {}
    for source in inputs:
        if not isinstance(source, dict) or not token(source.get("reference")):
            continue
        if not isinstance(source.get("records"), list) or not isinstance(source.get("kind"), str) or not isinstance(source.get("source_schema"), str):
            continue
        identity = (source.get("kind"), source.get("source_schema"), canonical(source["records"]))
        references.setdefault(source["reference"], set()).add(identity)
    conflicting_references = {ref for ref, versions in references.items() if len(versions) > 1}
    if conflicting_references:
        missing.append("conflicting evidence reference")
    seen_evidence = set()
    for source in inputs:
        if not isinstance(source, dict) or source.get("kind") not in SOURCES:
            missing.append("unsupported source kind")
            continue
        # Raw source content is not copied to output. References must be relative and cannot escape.
        reference = source.get("reference")
        if not token(reference) or Path(reference).is_absolute() or ".." in Path(reference).parts:
            missing.append("invalid evidence reference")
            continue
        if reference in conflicting_references:
            continue
        payload = source.get("records")
        if not isinstance(payload, list):
            missing.append("invalid source records: " + reference)
            continue
        digest = hashlib.sha256(canonical(payload).encode()).hexdigest()
        ev = {"kind": source["kind"], "reference": reference,
              "source_schema": INPUT if source.get("source_schema") == INPUT else "unknown", "records_sha256": digest}
        if reference not in seen_evidence:
            evidence.append(ev)
            seen_evidence.add(reference)
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
                and record.get("job_id") and record.get("kind") in ("job", "step")
                and (record.get("kind") != "step" or record.get("step_number") is not None)):
            ident = ("github", record["repository"], record["run_id"], record["run_attempt"],
                     record["job_id"], record["kind"], record.get("step_number"))
        elif record.get("local_invocation_id") and origin == "local":
            ident = ("local", record["local_invocation_id"], record["id"])
        else:
            # Unverified identity cannot establish duplicates or conflicts.
            ident = ("unidentified", occurrence)
        body = {k: v for k, v in record.items() if k not in ("evidence", "id")}
        record["aliases"] = [record["id"]]
        grouped.setdefault(ident, {}).setdefault(canonical(body), record)
        target = grouped[ident][canonical(body)]
        target["evidence"] = sorted(set(target["evidence"] + record["evidence"]))
        target["aliases"] = sorted(set(target["aliases"] + record["aliases"]))
        target["id"] = target["aliases"][0]
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
    if isinstance(snapshot, dict) and snapshot.get("schema_version") == CONTRACT_SNAPSHOT:
        return report_contract(snapshot)
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
    evidence_by_reference = {}
    for ev in snapshot["evidence"]:
        if (not isinstance(ev, dict) or set(ev) not in ({"kind", "reference", "source_schema", "records_sha256"}, {"kind", "reference", "source_schema", "records_sha256", "raw_sha256"})
                or ev["kind"] not in SOURCES or not token(ev["reference"])
                or ev["source_schema"] not in (INPUT, "unknown")
                or not re.fullmatch(r"[0-9a-f]{64}", ev["records_sha256"])
                or ("raw_sha256" in ev and not re.fullmatch(r"[0-9a-f]{64}", ev["raw_sha256"]))):
            raise ValueError("invalid snapshot evidence")
        previous = evidence_by_reference.setdefault(ev["reference"], canonical(ev))
        if previous != canonical(ev):
            raise ValueError("conflicting snapshot evidence reference")
    for record in records:
        if not isinstance(record, dict) or set(record) - (FIELDS | {"evidence", "identity_conflict", "aliases"}):
            raise ValueError("invalid snapshot record")
        body = {k: v for k, v in record.items() if k not in ("evidence", "identity_conflict", "aliases")}
        if (not valid_record(body) or type(record.get("identity_conflict")) is not bool
                or not isinstance(record.get("evidence"), list) or not record["evidence"]
                or not all(token(x) and x in evidence_by_reference for x in record["evidence"])
                or not isinstance(record.get("aliases"), list) or not record["aliases"]
                or not all(token(x) for x in record["aliases"])
                or record["id"] not in record["aliases"]):
            raise ValueError("invalid snapshot record")
    # Only collector-generated reason patterns are accepted, never arbitrary text.
    reasons = snapshot["missing_reasons"]
    prefixes = ("invalid source records: ", "unsupported source schema: ", "invalid or unsafe record: ")
    adapter_reasons = {"invalid job", "job fields unavailable", "reversed job timestamps",
        "pagination, workflow dependencies and run identity not verified", "invalid process record",
        "invalid process elapsed", "invalid or missing process started_at", "missing process exit code", "process artifact outer identity not verified",
        "configuration and producer-consumer binding not verified", "measurement invocation and dirty inputs not verified"}
    fixed = adapter_reasons | {"no sources", "unsupported source kind", "invalid evidence reference", "identity conflict", "source completeness not verified", "raw source identity and completeness not verified", "conflicting evidence reference"}
    if not all(isinstance(x, str) and (x in fixed or any(x.startswith(p) and token(x[len(p):]) for p in prefixes)) for x in reasons):
        raise ValueError("invalid snapshot reasons")
    # No inference about execution success, timing, or complete DAG from this initial slice.
    return {"schema_version": REPORT, "snapshot_schema": SNAPSHOT,
            "collection_id": snapshot.get("collection_id"), "origin": snapshot.get("origin"),
            "collection_status": snapshot.get("collection_status", "unknown"),
            "missing_reasons": snapshot.get("missing_reasons", []), "records": records,
            "evidence": snapshot.get("evidence", []),
            "analysis": {"status": "unavailable", "reason": "timing and DAG analysis not implemented"}}


# v2 is additive at the CLI boundary; older snapshot readers cannot mistake a
# provenance-backed calculation for the deliberately partial v1 projection.
CONTRACT_INPUT = "oasis7-engineering-telemetry-input/v2"
CONTRACT_SNAPSHOT = "oasis7-engineering-telemetry-snapshot/v2"
CONTRACT_REPORT = "oasis7-engineering-telemetry-report/v2"
DAG_FORMULA = "oasis7-engineering-dag/v1"
METRIC_FORMULAS = {
    "feedback_wall_seconds": "gate.completed_at - context.created_at",
    "job_execution_seconds": "sum(job.completed_at - job.started_at), excluding skipped",
    "execution_longest_chain_seconds": "max dependency-chain sum of job execution durations to gate",
    "unattributed_wait_seconds": "sum(job.started_at - max(context.created_at, dependency completion))",
    "observed_wall_chain_seconds": "max dependency-chain sum of execution and unattributed wait to gate",
    "first_job_start_delay_seconds": "min(executed job.started_at) - context.created_at",
}
TERMINAL_RESULTS = {"success", "failure", "cancelled", "timed_out", "neutral",
                    "action_required", "stale", "startup_failure"}


def precise_timestamp(value):
    """UTC wall-clock seconds as Decimal, retaining sub-microsecond precision."""
    from datetime import datetime, timezone
    from decimal import Decimal
    if not isinstance(value, str):
        raise ValueError("missing timestamp")
    match = re.fullmatch(r"(\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d)(\.\d{1,18})?(?:Z|\+00:00)", value)
    if not match:
        raise ValueError("invalid UTC timestamp")
    instant = datetime.fromisoformat(match[1]).replace(tzinfo=timezone.utc)
    delta = instant - datetime(1970, 1, 1, tzinfo=timezone.utc)
    return Decimal(delta.days * 86400 + delta.seconds) + Decimal(match[2] or "0")


def _number(value):
    # JSON has no Decimal. Do arithmetic before converting the final value, so
    # close timestamps are never subtracted as large binary floating-point times.
    return int(value) if value == value.to_integral_value() else float(value)


def _known(record, field):
    entries = record.get("field_sources", {}).get(field, [])
    return bool(entries) and all(entry.get("state") == "known" for entry in entries)


def analyze_context(context, records, origin):
    """Compute a complete gate-required DAG, or expose only known job intervals.

    parent_id means containment, not scheduling. Only explicit depends_on edges
    between the declared job instances affect the graph. No test/step/phase is
    added to job resource time, and no successful result is inferred here.
    """
    from decimal import Decimal
    import heapq
    zero = Decimal(0)
    reasons = []
    jobs = [record for record in records if record.get("kind") == "job"]
    result = {"context_id": context["id"], "status": "unavailable", "coverage": "partial",
              "missing_reasons": reasons, "metrics": {}, "job_timings": [],
              "execution_results": [{"id": record["id"], "result": record.get("execution_result", "unknown")}
                                    for record in jobs]}
    for key, formula in METRIC_FORMULAS.items():
        result["metrics"][key] = {"status": "unavailable", "value": None, "unit": "seconds",
                                  "formula": formula, "input_ids": []}
    def fail(reason):
        reasons.append(reason)
    if origin != "github":
        fail("github_job_dag_not_applicable_to_local")
    if context.get("identity_status") != "complete":
        fail("incomplete_context_identity")
    if context.get("identity_conflict"):
        fail("context_identity_conflict")
    if context.get("provenance_missing_reasons"):
        fail("incomplete_context_provenance")
    for flag in ("pagination_complete", "dependencies_complete", "conditions_complete"):
        if context.get(flag) is not True or not _known(context, flag):
            fail(flag + "_not_proven")
    required = context.get("job_ids")
    if not isinstance(required, list) or not required or len(set(required)) != len(required) or not _known(context, "job_ids"):
        fail("invalid_declared_job_set")
        required = []
    try:
        created = precise_timestamp(context.get("created_at"))
        if not _known(context, "created_at"):
            raise ValueError("missing creation provenance")
    except ValueError:
        created = None
        fail("run_creation_unavailable")
    aliases, ambiguous_ids = {}, set()
    for record in jobs:
        for alias in record.get("aliases", [record["id"]]):
            if alias in aliases and aliases[alias] is not record:
                fail("ambiguous_job_alias")
                ambiguous_ids.update((aliases[alias]["id"], record["id"]))
            aliases[alias] = record
    selected = {}
    for alias in required:
        if alias not in aliases:
            fail("missing_required_job:" + alias)
        else:
            selected[aliases[alias]["id"]] = aliases[alias]
    if set(selected) != {record["id"] for record in jobs}:
        fail("undeclared_or_conflicting_job")
    if len(selected) != len(required):
        fail("declared_jobs_not_unique_instances")
    gate_alias = context.get("gate_id")
    gate = aliases.get(gate_alias, {}).get("id")
    if gate not in selected or not _known(context, "gate_id"):
        fail("missing_final_gate")
    dependencies, durations, intervals, skipped = {}, {}, {}, set()
    # Even with an incomplete DAG, known unconflicted job intervals remain useful
    # as a partial resource observation, never as the complete job total.
    for job_id, record in selected.items():
        if record.get("identity_status") != "complete":
            fail("incomplete_job_identity:" + job_id)
        if record.get("identity_conflict"):
            fail("job_identity_conflict:" + job_id)
        if record.get("provenance_missing_reasons"):
            fail("incomplete_job_provenance:" + job_id)
        for field in ("source_sha", "tested_sha", "policy_sha", "config_digest"):
            if not _known(record, field) or record.get(field) != context.get(field):
                fail("job_context_identity_mismatch:" + job_id + ":" + field)
        if not _known(record, "depends_on") or not isinstance(record.get("depends_on"), list):
            fail("dependencies_unavailable:" + job_id)
            dependencies[job_id] = []
        else:
            raw = record["depends_on"]
            resolved = [aliases.get(alias, {}).get("id") for alias in raw]
            if len(set(raw)) != len(raw) or any(dep not in selected for dep in resolved) or len(set(resolved)) != len(resolved):
                fail("invalid_dependency:" + job_id)
            dependencies[job_id] = sorted({dep for dep in resolved if dep in selected})
        condition = record.get("condition")
        execution = record.get("execution_result")
        if not _known(record, "condition") or condition not in ("true", "false"):
            fail("condition_unavailable:" + job_id)
        if not _known(record, "execution_result"):
            fail("terminal_result_unavailable:" + job_id)
        if execution == "skipped":
            if condition != "false":
                fail("unresolved_skipped_job:" + job_id)
            # A proven false branch has no execution interval. Its virtual
            # dependency boundary is calculated below, not read as zero elapsed.
            skipped.add(job_id)
            durations[job_id] = zero
            continue
        if condition == "false":
            fail("executed_false_condition:" + job_id)
        if execution not in TERMINAL_RESULTS:
            fail("terminal_result_unavailable:" + job_id)
        try:
            if record.get("clock") != "wall" or not all(_known(record, field) for field in ("started_at", "completed_at", "clock")):
                raise ValueError("inexact boundary")
            start = precise_timestamp(record.get("started_at"))
            end = precise_timestamp(record.get("completed_at"))
            if end < start or (created is not None and start < created):
                raise ValueError("reversed interval")
            if execution not in TERMINAL_RESULTS or job_id in ambiguous_ids or record.get("identity_conflict") or record.get("identity_status") != "complete" or record.get("provenance_missing_reasons"):
                raise ValueError("unverified interval")
            durations[job_id] = end - start
            intervals[job_id] = (start, end)
        except ValueError:
            fail("job_interval_unavailable:" + job_id)
    known_total = sum((durations[job] for job in intervals), zero)
    result["known_job_execution_seconds"] = {"status": "partial" if intervals else "unavailable",
                                             "value": _number(known_total) if intervals else None,
                                             "unit": "seconds", "input_ids": sorted(intervals)}
    if gate in skipped:
        fail("final_gate_not_executed")
    # Iterative topological traversal also handles large input without recursion.
    outgoing = {job: [] for job in selected}
    indegree = {job: len(dependencies[job]) for job in selected}
    for job, needs in dependencies.items():
        for dep in needs:
            outgoing[dep].append(job)
    pending = [job for job, degree in indegree.items() if degree == 0]
    heapq.heapify(pending)
    order = []
    while pending:
        job = heapq.heappop(pending)
        order.append(job)
        for child in outgoing[job]:
            indegree[child] -= 1
            if indegree[child] == 0:
                heapq.heappush(pending, child)
    if len(order) != len(selected):
        fail("cyclic_dependencies")
    if gate in selected:
        ancestors, stack = set(), [gate]
        while stack:
            job = stack.pop()
            if job not in ancestors:
                ancestors.add(job)
                stack.extend(dependencies[job])
        if ancestors != set(selected):
            fail("gate_does_not_cover_declared_jobs")
    # Defects stop complete aggregation but never overwrite source conclusions.
    if reasons:
        result["missing_reasons"] = sorted(set(reasons))
        return result
    boundaries, waits, execution_dp, observed_dp = {}, {}, {}, {}
    execution_paths, observed_paths = {}, {}
    for job in order:
        needs = dependencies[job]
        ready = max([created] + [boundaries[dep] for dep in needs])
        if job in skipped:
            waits[job], boundaries[job] = zero, ready
        else:
            start, end = intervals[job]
            if start < ready:
                fail("job_started_before_dependencies:" + job)
            waits[job], boundaries[job] = start - ready, end
        # Select a deterministic path for ties. These paths can differ: pure
        # execution does not include any scheduler/concurrency/approval waiting.
        ep = max(needs, key=lambda dep: (execution_dp[dep], dep)) if needs else None
        op = max(needs, key=lambda dep: (observed_dp[dep], dep)) if needs else None
        execution_dp[job] = durations[job] + (execution_dp[ep] if ep else zero)
        observed_dp[job] = durations[job] + waits[job] + (observed_dp[op] if op else zero)
        execution_paths[job] = (execution_paths[ep] if ep else []) + [job]
        observed_paths[job] = (observed_paths[op] if op else []) + [job]
        result["job_timings"].append({"id": job, "execution_seconds": _number(durations[job]),
                                      "unattributed_wait_seconds": _number(waits[job]),
                                      "skipped": job in skipped, "depends_on": needs})
    if reasons:
        result["job_timings"] = []
        result["missing_reasons"] = sorted(set(reasons))
        return result
    values = {
        "feedback_wall_seconds": boundaries[gate] - created,
        "job_execution_seconds": sum(durations.values(), zero),
        "execution_longest_chain_seconds": execution_dp[gate],
        "unattributed_wait_seconds": sum(waits.values(), zero),
        "observed_wall_chain_seconds": observed_dp[gate],
        "first_job_start_delay_seconds": min(start for start, _ in intervals.values()) - created,
    }
    for key, value in values.items():
        result["metrics"][key].update(status="available", value=_number(value), input_ids=sorted(selected))
    result.update(status="available", coverage="complete", execution_longest_chain=execution_paths[gate],
                  observed_wall_chain=observed_paths[gate],
                  execution_chain_wait_seconds=_number(sum((waits[job] for job in execution_paths[gate]), zero)))
    result["known_job_execution_seconds"]["status"] = "complete"
    return result


def report_contract(snapshot):
    import engineering_telemetry_adapters as adapters
    adapters.validate_contract_snapshot(snapshot)
    contexts = [analyze_context(context, [row for row in snapshot["records"] if row["context_id"] == context["id"]], snapshot["origin"])
                for context in snapshot["contexts"]]
    available = sum(context["status"] == "available" for context in contexts)
    status = "available" if contexts and available == len(contexts) else "partial" if available else "unavailable"
    return {"schema_version": CONTRACT_REPORT, "snapshot_schema": CONTRACT_SNAPSHOT,
            "collection_id": snapshot["collection_id"], "origin": snapshot["origin"],
            "sample_kind": snapshot["sample_kind"], "collected_at": snapshot["collected_at"],
            "collection_status": snapshot["collection_status"], "missing_reasons": snapshot["missing_reasons"],
            "contexts": snapshot["contexts"], "records": snapshot["records"], "evidence": snapshot["evidence"],
            "analysis": {"status": status, "formula_version": DAG_FORMULA, "contexts": contexts}}



def markdown(result):
    lines = ["# Engineering telemetry (offline)", "",
             "- Collection: " + str(result["collection_id"]),
             "- Origin: " + str(result["origin"]),
             "- Collection status: " + str(result["collection_status"]),
             "- Records: " + str(len(result["records"])),
             "- Analysis: " + result["analysis"]["status"],
             "- Missing reasons: " + (", ".join(result["missing_reasons"]) or "none reported")]
    if result.get("sample_kind"):
        lines.append("- Sample kind: " + result["sample_kind"] + " (not a performance or validation certificate)")
    for context in result["analysis"].get("contexts", []):
        lines.extend(["", "## " + context["context_id"], "", "- Coverage: " + context["coverage"]])
        for key, metric in context["metrics"].items():
            value = str(metric["value"]) + " seconds" if metric["status"] == "available" else "unavailable"
            lines.append("- " + key + ": " + value)
        if context["missing_reasons"]:
            lines.append("- Reasons: " + ", ".join(context["missing_reasons"]))
    return "\n".join(lines) + "\n"


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



class CollectionStopped(Exception):
    """A safe, fixed failure code, never an exception containing a URL or token."""


class CollectionBudget:
    def __init__(self, requests=100, seconds=120, byte_limit=MAX_INPUT_BYTES, clock=None):
        import time
        if type(requests) is not int or not 0 <= requests <= 100 or type(byte_limit) is not int or not 0 <= byte_limit <= MAX_INPUT_BYTES or type(seconds) not in (int, float) or not math.isfinite(seconds) or not 0 <= seconds <= 120:
            raise ValueError("collection limits must be within 100 requests, 120 seconds and 32 MiB")
        self.clock = clock or time.monotonic
        self.started = self.clock()
        self.limits = dict(requests=requests, seconds=seconds, bytes=byte_limit)
        self.requests = self.response_bytes = self.decoded_bytes = 0
        self.cancelled = False

    def remaining(self):
        if self.cancelled:
            raise CollectionStopped("collection_cancelled")
        remaining = self.limits["seconds"] - (self.clock() - self.started)
        if remaining <= 0:
            raise CollectionStopped("time_budget_exhausted")
        return remaining

    def request(self):
        self.remaining()
        if self.requests >= self.limits["requests"]:
            raise CollectionStopped("request_budget_exhausted")
        if self.response_bytes >= self.limits["bytes"]:
            raise CollectionStopped("response_byte_budget_exhausted")
        self.requests += 1

    def stop_reason(self):
        try:
            self.remaining()
        except CollectionStopped as exc:
            return str(exc)
        return None

    def charge(self, size, decoded=False):
        self.remaining()
        key = "decoded_bytes" if decoded else "response_bytes"
        setattr(self, key, getattr(self, key) + size)
        if getattr(self, key) > self.limits["bytes"]:
            raise CollectionStopped("decoded_byte_budget_exhausted" if decoded else "response_byte_budget_exhausted")

    def summary(self):
        return dict(limits=self.limits, requests=self.requests, response_bytes=self.response_bytes,
                    decoded_bytes=self.decoded_bytes, elapsed_seconds=max(0, self.clock() - self.started))


def _download_url(url):
    from urllib.parse import urlsplit
    try:
        parsed = urlsplit(url)
        host = parsed.hostname or ""
        port = parsed.port
    except (ValueError, TypeError) as exc:
        raise CollectionStopped("unsafe_download_destination") from exc
    # Only the API and GitHub's artifact storage. Never forward API credentials to storage.
    trusted = host == "api.github.com" or host.endswith(".blob.core.windows.net") or host == "productionresultssa0.blob.core.windows.net" or host.endswith(".actions.githubusercontent.com")
    if not trusted or parsed.scheme != "https" or port not in (None, 443) or parsed.username or parsed.password or parsed.fragment:
        raise CollectionStopped("unsafe_download_destination")
    if host == "api.github.com":
        actions = re.fullmatch(r"/repos/[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+/actions/(?:runs/[0-9]+(?:/attempts/[0-9]+(?:/jobs)?|/artifacts)|artifacts/[0-9]+/zip|jobs/[0-9]+/logs)", parsed.path)
        workflow = (re.fullmatch(r"/repos/[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+/contents/\.github/workflows/rust\.yml", parsed.path)
                    and re.fullmatch(r"ref=[0-9a-f]{40}", parsed.query))
        if not actions and not workflow:
            raise CollectionStopped("unsupported_api_endpoint")
    return host


def http_worker():
    """One GET in a disposable child; the parent owns the hard wall deadline."""
    import base64
    import urllib.error
    import urllib.request
    try:
        request = json.loads(sys.stdin.buffer.read(8192))
        url, limit, timeout = request["url"], request["limit"], request["timeout"]
        host = _download_url(url)
        if type(limit) is not int or not 0 <= limit <= MAX_INPUT_BYTES or not 0 < timeout <= 120:
            raise ValueError("invalid request budget")
        class NoRedirect(urllib.request.HTTPRedirectHandler):
            def redirect_request(self, *args, **kwargs):
                return None
        headers = {"Accept": "application/vnd.github+json", "X-GitHub-Api-Version": "2022-11-28"}
        credential = os.environ.get("GH_TOKEN") or os.environ.get("GITHUB_TOKEN")
        if credential and host == "api.github.com":
            headers["Authorization"] = "Bearer " + credential
        opener = urllib.request.build_opener(NoRedirect())
        try:
            response = opener.open(urllib.request.Request(url, headers=headers, method="GET"), timeout=timeout)
        except urllib.error.HTTPError as error:
            response = error
        with response:
            status = response.status
            # Error bodies can contain private request context; do not return them.
            raw = response.read(limit + 1) if status == 200 else b""
            result = dict(status=status, location=response.headers.get("Location"),
                          body=base64.b64encode(raw).decode("ascii"))
        print(json.dumps(result))
        return 0
    except Exception:
        print('{"error":"http_request_failed"}')
        return 1


class LiveTransport:
    def __init__(self, budget=None):
        self.budget, self.child = budget, None

    def cancel(self):
        if self.child is not None:
            self.child.kill()

    def __call__(self, url, limit, timeout):
        import base64
        import subprocess
        _download_url(url)
        try:
            child = subprocess.Popen([sys.executable, str(Path(__file__).resolve()), "_http-get"],
                                     stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
            self.child = child
            try:
                if self.budget:
                    self.budget.remaining()
                output, _ = child.communicate(json.dumps(dict(url=url, limit=limit, timeout=timeout)).encode(), timeout=timeout)
            except BaseException:
                # Includes SIGTERM/SIGINT translated by the CLI, not just timeouts.
                child.kill()
                child.communicate()
                raise
            finally:
                self.child = None
        except subprocess.TimeoutExpired as exc:
            # subprocess.run kills and reaps only this request child on timeout.
            raise CollectionStopped("time_budget_exhausted") from exc
        except OSError as exc:
            raise CollectionStopped("http_request_failed") from exc
        try:
            data = json.loads(output)
            if child.returncode or "error" in data:
                raise ValueError("failed request")
            return data["status"], data.get("location"), base64.b64decode(data["body"], validate=True)
        except (KeyError, ValueError, TypeError) as exc:
            raise CollectionStopped("http_request_failed") from exc


class RecordedTransport:
    """Replay an explicitly supplied export; never describe it as a live fetch."""
    def __init__(self, directory):
        self.directory = Path(directory).resolve()

    def __call__(self, url, limit, timeout):
        from urllib.parse import urlsplit, parse_qs
        parsed = urlsplit(url)
        parts = parsed.path.split("/")
        page = parse_qs(parsed.query).get("page", ["1"])[0]
        if parts[-1] == "jobs":
            name = f"jobs-{page}.json"
        elif parts[-1] == "artifacts":
            name = f"artifacts-{page}.json"
        elif parts[-1] == "zip":
            name = f"artifact-{parts[-2]}.zip"
        else:
            name = "run.json"
        path = self.directory / name
        if path.is_symlink() or not path.is_file() or path.parent.resolve() != self.directory:
            raise CollectionStopped("recorded_response_missing")
        with path.open("rb") as stream:
            return 200, None, stream.read(limit + 1)


class ActionsReader:
    def __init__(self, repository, budget, transport):
        self.base = "https://api.github.com/repos/" + repository + "/actions/"
        self.budget, self.transport = budget, transport
        self.sources = []

    def get(self, path, reference, archive=False):
        url = self.base + path
        for redirect in range(2):
            _download_url(url)
            self.budget.request()
            status, location, raw = self.transport(url, self.budget.limits["bytes"] - self.budget.response_bytes,
                                                    self.budget.remaining())
            self.budget.charge(len(raw))
            if status == 200:
                self.sources.append(dict(reference=reference, resource=path,
                                         raw_sha256=hashlib.sha256(raw).hexdigest(), bytes=len(raw)))
                return raw
            if archive and redirect == 0 and status in (301, 302, 303, 307, 308) and isinstance(location, str):
                # Signed locations remain transient; they are never persisted in a receipt.
                _download_url(location)
                url = location
                continue
            raise CollectionStopped("http_status_" + str(status) if type(status) is int and 100 <= status <= 599 else "invalid_http_response")
        raise CollectionStopped("redirect_limit")

    def json(self, path, reference):
        raw = self.get(path, reference)
        self.budget.charge(len(raw), decoded=True)
        try:
            return json.loads(raw, parse_constant=lambda value: (_ for _ in ()).throw(ValueError("non-finite JSON")))
        except (UnicodeError, ValueError, RecursionError) as exc:
            raise CollectionStopped("invalid_response_json") from exc

    def pages(self, path, key, prefix):
        pages, seen, total = [], set(), None
        try:
            for page in range(1, 102):
                obj = self.json(path + f"?per_page=100&page={page}", f"actions/{prefix}-{page}.json")
                if not isinstance(obj, dict) or type(obj.get("total_count")) is not int or not 0 <= obj["total_count"] <= 10000 or not isinstance(obj.get(key), list) or len(obj[key]) > 100:
                    raise CollectionStopped("invalid_pagination_schema")
                if total is not None and total != obj["total_count"]:
                    raise CollectionStopped("pagination_total_changed")
                total = obj["total_count"]
                ids = [item.get("id") if isinstance(item, dict) else None for item in obj[key]]
                if any(type(value) is not int or value <= 0 for value in ids):
                    raise CollectionStopped("invalid_page_identity")
                if len(set(ids)) != len(ids) or seen.intersection(ids):
                    # Do not merge conflicting or repeated pages into a successful collection.
                    raise CollectionStopped("repeated_page_identity")
                if len(seen) + len(ids) > total:
                    raise CollectionStopped("pagination_count_mismatch")
                pages.append(obj)
                seen.update(ids)
                if len(seen) == total:
                    return pages, True, []
                if len(ids) < 100:
                    raise CollectionStopped("pagination_truncated")
            raise CollectionStopped("pagination_page_limit")
        except CollectionStopped as exc:
            return pages, False, [str(exc)]


def archive_document(raw, filename, budget):
    """Read selected JSON only, never extract files or execute candidate code.

    ZIP and one gzip/tar layer are bounded independently of compressed size. All
    entries, including unused binaries, are checked and charged. PAX, links and
    special files are deliberately unsupported rather than partially interpreted.
    """
    import io
    import gzip
    import stat
    import zipfile
    import zlib
    found = []
    def safe_name(name, directory=False):
        if not isinstance(name, str) or "\\" in name or name.startswith("/") or "\0" in name:
            raise CollectionStopped("unsafe_archive_path")
        parts = name.split("/")
        if ".." in parts or any(":" in part for part in parts):
            raise CollectionStopped("unsafe_archive_path")
        clean = "/".join(part for part in parts if part not in ("", "."))
        if not clean and not directory:
            raise CollectionStopped("unsafe_archive_path")
        return clean
    def read(stream, size):
        out = bytearray()
        while len(out) < size:
            budget.remaining()
            chunk = stream.read(min(65536, size - len(out)))
            if not chunk:
                break
            budget.charge(len(chunk), decoded=True)
            out.extend(chunk)
        return bytes(out)
    def tar_member(data):
        seen = set()
        with gzip.GzipFile(fileobj=io.BytesIO(data)) as stream:
            for _ in range(257):
                header = read(stream, 512)
                if header == b"\0" * 512:
                    tail = read(stream, 512)
                    if tail != b"\0" * 512:
                        raise CollectionStopped("invalid_tar_end")
                    while True:
                        tail = read(stream, 65536)
                        if any(tail):
                            raise CollectionStopped("trailing_tar_data")
                        if not tail:
                            return
                if len(header) != 512:
                    raise CollectionStopped("truncated_tar")
                try:
                    size = int(header[124:136].strip(b"\0 ") or b"0", 8)
                    checksum = int(header[148:156].strip(b"\0 "), 8)
                    name = header[:100].split(b"\0", 1)[0].decode("utf-8")
                    prefix = header[345:500].split(b"\0", 1)[0].decode("utf-8")
                except (ValueError, UnicodeError) as exc:
                    raise CollectionStopped("invalid_tar_header") from exc
                if checksum != sum(header[:148]) + 256 + sum(header[156:]) or size < 0:
                    raise CollectionStopped("invalid_tar_header")
                kind = header[156:157]
                if kind not in (b"0", b"\0", b"5") or (kind == b"5" and size):
                    raise CollectionStopped("unsupported_archive_member")
                name = safe_name((prefix + "/" if prefix else "") + name, kind == b"5")
                if name in seen:
                    raise CollectionStopped("duplicate_archive_member")
                seen.add(name)
                if size > budget.limits["bytes"] - budget.decoded_bytes:
                    raise CollectionStopped("decoded_byte_budget_exhausted")
                selected = name == filename
                payload = bytearray()
                remaining = (size + 511) // 512 * 512
                while remaining:
                    block = read(stream, min(65536, remaining))
                    if not block:
                        raise CollectionStopped("truncated_tar")
                    if selected:
                        payload.extend(block[:max(0, size - len(payload))])
                    remaining -= len(block)
                if selected:
                    found.append(bytes(payload))
            raise CollectionStopped("archive_member_limit")
    try:
        with zipfile.ZipFile(io.BytesIO(raw)) as archive:
            members = archive.infolist()
            if len(members) > 256:
                raise CollectionStopped("archive_member_limit")
            names = set()
            for item in members:
                name = safe_name(item.filename, item.is_dir())
                if name in names:
                    raise CollectionStopped("duplicate_archive_member")
                names.add(name)
                kind = stat.S_IFMT(item.external_attr >> 16)
                if kind not in (0, stat.S_IFREG, stat.S_IFDIR) or item.flag_bits & 1:
                    raise CollectionStopped("unsupported_archive_member")
                if item.file_size > budget.limits["bytes"] - budget.decoded_bytes:
                    raise CollectionStopped("decoded_byte_budget_exhausted")
                with archive.open(item) as stream:
                    data = read(stream, item.file_size + 1)
                if len(data) != item.file_size:
                    raise CollectionStopped("archive_size_mismatch")
                if name == filename:
                    found.append(data)
                elif name.endswith(".tar.gz"):
                    tar_member(data)
        if len(found) != 1:
            raise CollectionStopped("archive_document_missing_or_ambiguous")
        return json.loads(found[0], parse_constant=lambda value: (_ for _ in ()).throw(ValueError("non-finite JSON"))), found[0]
    except CollectionStopped:
        raise
    except (OSError, ValueError, EOFError, UnicodeError, RecursionError, zipfile.BadZipFile, NotImplementedError, zlib.error) as exc:
        raise CollectionStopped("invalid_archive_document") from exc


def collect_actions(repository, run_id, attempt, budget, transport, build_artifacts=()):
    import uuid
    from datetime import datetime, timezone
    import engineering_telemetry_adapters as adapters
    if not adapters._REPOSITORY.fullmatch(repository) or adapters._SECRET.search(repository) or any(type(v) is not int or not 0 < v <= 2**63-1 for v in (run_id, attempt, *build_artifacts)):
        raise ValueError("invalid repository/run/attempt/artifact identity")
    reader = ActionsReader(repository, budget, transport)
    reasons, run, pages, plan, builds = [], None, [], None, []
    complete = False
    collected_at = datetime.now(timezone.utc).isoformat()
    try:
        run = reader.json(f"runs/{run_id}/attempts/{attempt}", "actions/run.json")
        pages, complete, errors = reader.pages(f"runs/{run_id}/attempts/{attempt}/jobs", "jobs", "jobs")
        reasons += errors
        artifact_pages, artifacts_complete, errors = reader.pages(f"runs/{run_id}/artifacts", "artifacts", "artifacts")
        reasons += errors
        artifacts = [item for page in artifact_pages for item in page["artifacts"]]
        selection = [item for item in artifacts if item.get("name") == f"ci-selection-{run_id}-{attempt}"]
        selected = [(selection[0], "selection.json")] if artifacts_complete and len(selection) == 1 else []
        if not selected:
            reasons.append("selection_artifact_unavailable_or_ambiguous")
        for artifact_id in dict.fromkeys(build_artifacts):
            matches = [item for item in artifacts if item["id"] == artifact_id]
            if artifacts_complete and len(matches) == 1:
                selected.append((matches[0], "manifest.json"))
            else:
                reasons.append("build_artifact_unavailable_or_ambiguous")
        for artifact, member in selected:
            try:
                binding = artifact.get("workflow_run")
                if not isinstance(binding, dict) or not isinstance(run, dict) or binding.get("id") != run_id or binding.get("head_sha") != run.get("head_sha"):
                    raise CollectionStopped("artifact_run_binding_mismatch")
                if artifact.get("expired") is not False:
                    raise CollectionStopped("artifact_expired_or_unknown")
                size = artifact.get("size_in_bytes")
                if type(size) is not int or not 0 <= size <= budget.limits["bytes"] - budget.response_bytes:
                    raise CollectionStopped("artifact_exceeds_byte_budget")
                ref = f"artifacts/{artifact['id']}.zip"
                raw = reader.get(f"artifacts/{artifact['id']}/zip", ref, archive=True)
                checksum = artifact.get("digest")
                if isinstance(checksum, str) and checksum.startswith("sha256:"):
                    if checksum != "sha256:" + hashlib.sha256(raw).hexdigest():
                        raise CollectionStopped("artifact_digest_mismatch")
                else:
                    reasons.append("artifact_digest_unavailable")
                payload, member_raw = archive_document(raw, member, budget)
                reference = "artifacts/selection.json" if member == "selection.json" else f"artifacts/build-{artifact['id']}.json"
                reader.sources.append(dict(reference=reference, artifact_id=artifact["id"], member=member,
                                           raw_sha256=hashlib.sha256(member_raw).hexdigest(), bytes=len(member_raw),
                                           expires_at=adapters.stamp(artifact.get("expires_at"))))
                if member == "selection.json":
                    plan = payload
                else:
                    builds.append((reference, payload))
            except CollectionStopped as exc:
                reasons.append(str(exc))
    except CollectionStopped as exc:
        reasons.append(str(exc))
    doc, adapter_reasons, build_observations = adapters.actions_document(repository, run_id, attempt, run, pages, complete,
                                                                       plan, builds, str(uuid.uuid4()), collected_at, budget)
    return finish_collection(doc, reasons + adapter_reasons, budget, reader.sources,
                             "recorded" if isinstance(transport, RecordedTransport) else "live", build_observations)


def finish_collection(doc, reasons, budget, sources, mode, build_observations=None):
    try:
        budget.remaining()
    except CollectionStopped as exc:
        reasons.append(str(exc))
    snapshot = doc.finish(reasons)
    try:
        budget.remaining()
    except CollectionStopped as exc:
        if str(exc) not in snapshot["missing_reasons"]:
            snapshot["missing_reasons"].append(str(exc))
            snapshot["missing_reasons"].sort()
            snapshot["collection_status"] = "partial"
    receipt = dict(schema_version="oasis7-engineering-collection/v1", collection_id=snapshot["collection_id"],
                   collected_at=snapshot["collected_at"], transport=mode, budget=budget.summary(),
                   collection_status=snapshot["collection_status"], missing_reasons=snapshot["missing_reasons"],
                   snapshot_sha256=hashlib.sha256(adapters_bytes(snapshot)).hexdigest(), sources=sources,
                   projection=doc.projection, build_observations=build_observations or [],
                   retention=dict(recommended_report_days=7, raw_payloads_retained=False, automatic_deletion=False))
    return snapshot, receipt


def adapters_bytes(value):
    import engineering_telemetry_adapters as adapters
    return adapters._json_bytes(value)


def collect_metrics(path, budget):
    import uuid
    from datetime import datetime, timezone
    import engineering_telemetry_adapters as adapters
    reasons, metrics, sources = [], None, []
    try:
        budget.remaining()
        if Path(path).is_symlink() or not Path(path).is_file():
            raise CollectionStopped("metrics_file_unavailable")
        with open(path, "rb") as stream:
            raw = stream.read(budget.limits["bytes"] + 1)
        budget.charge(len(raw), decoded=True)
        metrics = json.loads(raw, parse_constant=lambda value: (_ for _ in ()).throw(ValueError("non-finite JSON")))
        sources.append(dict(reference="local/current.metrics.json", bytes=len(raw), raw_sha256=hashlib.sha256(raw).hexdigest()))
    except CollectionStopped as exc:
        reasons.append(str(exc))
    except (OSError, ValueError, RecursionError) as exc:
        reasons.append("invalid_metrics_file")
    doc, adapter_reasons = adapters.metrics_document(metrics, str(uuid.uuid4()), datetime.now(timezone.utc).isoformat())
    return finish_collection(doc, reasons + adapter_reasons, budget, sources, "local")



# Forward runtime bindings remain in the registered telemetry implementation.
import engineering_telemetry_adapters as adapters

SCHEMA = 'oasis7-engineering-bindings/v1'
MARKER = 'OASIS7_ENGINEERING_JOB_V1='
STEP = 'Record engineering job observation'
# This standalone body is embedded once as a YAML step anchor. It must also work
# in controller jobs that deliberately have no checkout or candidate executable.
EMITTER = r'''import gzip, hashlib, json, os, pathlib, re, subprocess
E = os.environ
limit = 1024 * 1024
missing = []
def digest(raw):
    return hashlib.sha256(raw).hexdigest()
def bounded(path):
    path = pathlib.Path(path)
    if any(p.is_symlink() for p in (path, *path.parents)):
        raise ValueError('symlink')
    with path.open('rb') as stream:
        raw = stream.read(limit + 1)
    if len(raw) > limit:
        raise ValueError('size')
    return raw
def sha(value):
    return value if isinstance(value, str) and re.fullmatch('[0-9a-f]{40}', value) else None
def token(value):
    return isinstance(value, str) and re.fullmatch('[A-Za-z0-9][A-Za-z0-9_.:-]{0,127}', value) and not re.search(r'(?i)(password|secret|authorization|bearer|access.token|gh[pousr]_|github_pat_)', value)
def safe_json(name, fallback):
    raw = E.get(name, '')
    if len(raw) > 65536:
        raise ValueError('context_size')
    return json.loads(raw) if raw else fallback
needs = safe_json('TELEMETRY_NEEDS', {})
matrix = safe_json('TELEMETRY_MATRIX', {}) or {}
if not isinstance(needs, dict) or not isinstance(matrix, dict):
    raise ValueError('context_type')
if len(needs) > 100 or len(matrix) > 32 or any(not token(k) or not isinstance(v, dict) or v.get('result') not in ('success', 'failure', 'cancelled', 'skipped') for k, v in needs.items()):
    raise ValueError('needs_values')
if any(not token(k) or not (type(v) is bool or (type(v) is int and abs(v) <= 2**31) or token(v)) for k, v in matrix.items()):
    raise ValueError('matrix_values')
needs = {key: value.get('result') for key, value in needs.items()}
workspace = pathlib.Path(E['GITHUB_WORKSPACE'])
checkout = None
if (workspace / '.git').exists():
    try:
        checkout = sha(subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=workspace,
                         timeout=5, stderr=subprocess.DEVNULL, text=True).strip())
    except (OSError, subprocess.SubprocessError):
        missing.append('checkout_unavailable')
temp = pathlib.Path(E['RUNNER_TEMP'])
configs = {}
for label, path in [('checkout-test-config', workspace / 'scripts/ci-formal-process-suites.json'),
                    ('checkout-scope-config', workspace / 'scripts/ci-required-scope.json'),
                    ('formal-authority', temp / 'ci-authority/ci-formal-process-suites.json')]:
    if path.exists():
        try:
            configs[label] = digest(bounded(path))
        except (OSError, ValueError):
            missing.append('configuration_unavailable')
job = E['GITHUB_JOB']
locations = []
if job in ('formal-debug-build', 'formal-debug-build-trusted'):
    locations = [('produced', 'debug', temp / 'formal-debug-tests/manifest.json', False)]
elif job in ('formal-endpoint-build', 'formal-endpoint-build-trusted'):
    locations = [('produced', 'endpoint', temp / 'formal-endpoint-tests/manifest.json', False)]
elif job in ('formal-test-build', 'formal-test-build-trusted'):
    locations = [('observed-input', 'debug', temp / 'formal-debug-tests/manifest.json', False),
                 ('observed-input', 'endpoint', temp / 'formal-endpoint-tests/manifest.json', False)]
    for shard in ('runtime-protocol', 'world-history', 'healthy-set', 'storage-recovery', 'lifecycle-cli'):
        locations.append(('produced', shard, temp / 'formal-archives' / (shard + '.tar.gz'), True))
elif job == 'formal-process-tests':
    locations = [('observed-input', matrix.get('shard'), temp / 'native-tests/manifest.json', False)]
manifests = []
for direction, label, path, archived in locations:
    if not path.exists():
        continue
    try:
        if archived:
            if any(p.is_symlink() for p in (path, *path.parents)):
                raise ValueError('symlink')
            with gzip.open(path, 'rb') as stream:
                header = stream.read(512)
                if (len(header) != 512 or header[:100].split(b'\0', 1)[0] != b'manifest.json'
                        or header[156:157] not in (b'0', b'\0')
                        or int(header[148:156].strip(b'\0 '), 8) != sum(header[:148]) + 256 + sum(header[156:])):
                    raise ValueError('manifest_header')
                size = int(header[124:136].strip(b'\0 '), 8)
                if not 0 <= size <= limit:
                    raise ValueError('manifest_size')
                raw = stream.read(size)
                if len(raw) != size:
                    raise ValueError('manifest_truncated')
        else:
            raw = bounded(path)
        manifest = json.loads(raw)
        if type(manifest.get('schema_version')) is not int or manifest['schema_version'] != 2:
            raise ValueError('manifest_schema')
        if any(not isinstance(manifest.get(k), str) or not re.fullmatch('[1-9][0-9]{0,18}', manifest[k]) or int(manifest[k]) > 2**63-1 for k in ('producer_run_id', 'producer_attempt')):
            raise ValueError('manifest_producer')
        if not isinstance(manifest.get('config_sha256'), str) or not re.fullmatch('[0-9a-f]{64}', manifest['config_sha256']) or not token(label):
            raise ValueError('manifest_config')
        manifests.append(dict(direction=direction, label=label, manifest_sha256=digest(raw),
            source_sha=sha(manifest.get('source_sha')), tested_sha=sha(manifest.get('tested_sha')),
            producer_run_id=manifest.get('producer_run_id'), producer_attempt=manifest.get('producer_attempt'),
            config_digest=manifest.get('config_sha256')))
    except (OSError, ValueError, EOFError, KeyError, TypeError):
        missing.append('manifest_observation_unavailable')
row = dict(schema_version='oasis7-engineering-job/v1', repository=E['GITHUB_REPOSITORY'],
    run_id=int(E['GITHUB_RUN_ID']), run_attempt=int(E['GITHUB_RUN_ATTEMPT']), job_key=job,
    workflow_sha=sha(E.get('TELEMETRY_WORKFLOW_SHA')), matrix=matrix, needs=needs,
    target_source_sha=sha(E.get('TELEMETRY_SOURCE_SHA')), target_tested_sha=sha(E.get('TELEMETRY_TESTED_SHA')),
    target_policy_sha=sha(E.get('TELEMETRY_POLICY_SHA')), checkout_sha=checkout,
    configuration_digests=configs, manifests=manifests, missing_reasons=sorted(set(missing)))
print('OASIS7_' + 'ENGINEERING_JOB_V1=' + json.dumps(row, sort_keys=True, separators=(',', ':')))
'''


def workflow_step():
    return "      - &engineering-job-observation\n        name: Record engineering job observation\n        if: always() && !cancelled()\n        continue-on-error: true\n        timeout-minutes: 1\n        shell: bash\n        env:\n          TELEMETRY_WORKFLOW_SHA: ${{ github.workflow_sha }}\n          TELEMETRY_SOURCE_SHA: ${{ needs.select.outputs.source_sha || steps.select.outputs.source_sha }}\n          TELEMETRY_TESTED_SHA: ${{ needs.select.outputs.tested_sha || steps.select.outputs.tested_sha }}\n          TELEMETRY_POLICY_SHA: ${{ needs.select.outputs.policy_sha || steps.select.outputs.policy_sha }}\n          TELEMETRY_NEEDS: ${{ toJSON(needs) }}\n          TELEMETRY_MATRIX: ${{ toJSON(matrix) }}\n        run: |\n          python3 - <<'PY_OBSERVATION' # oasis7-engineering-job-observation/v1\n" + ''.join('          ' + line + '\n' for line in EMITTER.splitlines()) + '          PY_OBSERVATION\n'


def validate_observation(row, repository, run_id, attempt):
    fields = {'schema_version', 'repository', 'run_id', 'run_attempt', 'job_key', 'workflow_sha',
              'matrix', 'needs', 'target_source_sha', 'target_tested_sha', 'target_policy_sha',
              'checkout_sha', 'configuration_digests', 'manifests', 'missing_reasons'}
    if not isinstance(row, dict) or set(row) != fields or row['schema_version'] != 'oasis7-engineering-job/v1':
        raise ValueError('invalid_observation_schema')
    if (row['repository'] != repository or type(row['run_id']) is not int or row['run_id'] != run_id
            or type(row['run_attempt']) is not int or row['run_attempt'] != attempt):
        raise ValueError('observation_identity_mismatch')
    adapters._field_value('job_key', row['job_key'])
    adapters._field_value('matrix', row['matrix'])
    for field in ('workflow_sha', 'target_source_sha', 'target_tested_sha', 'target_policy_sha', 'checkout_sha'):
        if row[field] is not None and adapters.sha(row[field]) is None:
            raise ValueError('invalid_observation_sha')
    needs = row['needs']
    if (not isinstance(needs, dict) or len(needs) > 100 or
            any(not adapters._token(key) or value not in ('success', 'failure', 'cancelled', 'skipped')
                for key, value in needs.items())):
        raise ValueError('invalid_observation_dependencies')
    configurations = row['configuration_digests']
    if (not isinstance(configurations, dict) or set(configurations) - {'checkout-test-config', 'checkout-scope-config', 'formal-authority'}
            or any(not isinstance(v, str) or not adapters._DIGEST.fullmatch(v) for v in configurations.values())):
        raise ValueError('invalid_observation_configuration')
    if (not isinstance(row['missing_reasons'], list) or len(row['missing_reasons']) > 8 or
            any(v not in ('checkout_unavailable', 'configuration_unavailable', 'manifest_observation_unavailable') for v in row['missing_reasons'])):
        raise ValueError('invalid_observation_reasons')
    if not isinstance(row['manifests'], list) or len(row['manifests']) > 8:
        raise ValueError('invalid_observation_manifests')
    seen = set()
    for item in row['manifests']:
        if (not isinstance(item, dict) or set(item) != {'direction', 'label', 'manifest_sha256', 'source_sha', 'tested_sha',
                                                      'producer_run_id', 'producer_attempt', 'config_digest'} or
                item['direction'] not in ('produced', 'observed-input') or not adapters._token(item['label'])):
            raise ValueError('invalid_manifest_observation')
        for key in ('manifest_sha256', 'config_digest'):
            if not isinstance(item[key], str) or not adapters._DIGEST.fullmatch(item[key]):
                raise ValueError('invalid_manifest_digest')
        for key in ('source_sha', 'tested_sha'):
            if item[key] is not None and adapters.sha(item[key]) is None:
                raise ValueError('invalid_manifest_sha')
        for key in ('producer_run_id', 'producer_attempt'):
            if not isinstance(item[key], str) or not re.fullmatch(r'[1-9][0-9]{0,18}', item[key]) or int(item[key]) > 2**63-1:
                raise ValueError('invalid_manifest_producer')
        identity = (item['direction'], item['label'])
        if identity in seen:
            raise ValueError('duplicate_manifest_observation')
        seen.add(identity)
    return row


def parse_job_log(raw, job, repository, run_id, attempt):
    """Read exactly one marker emitted inside the dedicated step's time window.

    Raw logs are never persisted in a report. API step number/time and the marker
    offset are retained. Equal-second API boundaries require dedicated log-group
    and next-group/cleanup fences; their subsecond timing is not inferred.
    """
    steps = [s for s in job.get('steps', []) if s.get('name') == STEP]
    if len(steps) != 1 or steps[0].get('conclusion') != 'success':
        raise ValueError('observation_step_unavailable')
    step = steps[0]
    start, end = (precise_timestamp(step.get(k)) for k in ('started_at', 'completed_at'))
    # Actions API timestamps have whole-second precision, including a next
    # step that begins in the observation's completion second.
    next_starts = [precise_timestamp(s['started_at']) for s in job.get('steps', [])
                   if type(s.get('number')) is int and s['number'] > step['number'] and s.get('started_at')]
    # Coarse equal-second API boundaries are resolved by the dedicated log
    # group and cleanup/next-group fences, not by inventing subsecond times.
    next_starts = [value for value in next_starts if value >= start]
    stop = min([end + 1] + [value + 1 for value in next_starts])
    candidates = []
    active = False
    groups = 0
    header_stamp = None
    fences = []
    for number, line in enumerate(raw.decode('utf-8').splitlines(), 1):
        cleanup = re.match(r'^\d{4}-\d\d-\d\dT\S+ (?:Post job cleanup\.|Cleaning up orphan processes$)', line)
        if active and (cleanup or '##[group]' in line):
            fences.append(precise_timestamp(line.split(' ', 1)[0]))
        if cleanup:
            active = False
        if '##[group]' in line:
            active = "##[group]Run python3 - <<'PY_OBSERVATION' # oasis7-engineering-job-observation/v1" in line
            if active:
                header_stamp = precise_timestamp(line.split(' ', 1)[0])
                if not start <= header_stamp < stop:
                    raise ValueError('observation_header_outside_step')
            groups += int(active)
        match = re.match(r'^(\d{4}-\d\d-\d\dT\S+) ' + MARKER + r'(\{.*\})$', line)
        if not match:
            continue
        if not active:
            raise ValueError('observation_outside_dedicated_step')
        stamp = precise_timestamp(match[1])
        if not start <= stamp < stop or stop <= start or header_stamp is None or stamp < header_stamp:
            raise ValueError('observation_outside_step')
        candidates.append((number, json.loads(match[2]), stamp))
    if len(candidates) != 1 or groups != 1:
        raise ValueError('observation_missing_or_ambiguous')
    number, row, stamp = candidates[0]
    # A marker in a coarse next-step second is accepted only with a later
    # exact log fence in that second, never from timestamps alone or truncation.
    if next_starts and stamp >= min(next_starts) and not any(stamp <= fence < stop for fence in fences):
        raise ValueError('observation_boundary_ambiguous')
    validate_observation(row, repository, run_id, attempt)
    return row, dict(step_number=step['number'], log_line=number, raw_sha256=hashlib.sha256(raw).hexdigest())


def workflow_dependencies(raw):
    """Recognize only this repository's static top-level needs syntax, fail closed."""
    text = raw.decode('utf-8')
    if text.count('\njobs:\n') != 1:
        raise ValueError('unsupported_workflow_jobs')
    jobs = re.findall(r'^  ([a-z][a-z0-9-]*):\n(.*?)(?=^  [a-z][a-z0-9-]*:|\Z)', text.split('\njobs:\n')[1], re.M | re.S)
    declarations = re.findall(r'^  [^ #\n][^\n]*:\s*$', text.split('\njobs:\n')[1], re.M)
    if len(declarations) != len(jobs):
        raise ValueError('unsupported_workflow_job_declaration')
    result = {}
    for key, body in jobs:
        if key in result:
            raise ValueError('unsupported_workflow_job')
        header = body.split('    steps:', 1)[0]
        declarations = re.findall(r'''^    ["']?needs["']?\s*:.*$''', body, re.M)
        lines = re.findall(r'^    needs: (.+)$', header, re.M)
        if len(declarations) != len(lines) or re.search(r'^    <<\s*:', body, re.M):
            raise ValueError('unsupported_workflow_needs')
        if len(lines) > 1:
            raise ValueError('ambiguous_workflow_needs')
        value = lines[0] if lines else ''
        if value.startswith('[') and value.endswith(']'):
            values = [v.strip() for v in value[1:-1].split(',')]
        else:
            values = [value] if value else []
        if any(not re.fullmatch('[a-z][a-z0-9-]*', v) for v in values) or len(values) != len(set(values)):
            raise ValueError('unsupported_workflow_needs')
        result[key] = values
    if not result or any(dep not in result for needs in result.values() for dep in needs):
        raise ValueError('missing_workflow_dependency')
    return result


def bind(run, pages, logs, workflow_raw=None, workflow_sha=None, pagination_complete=False, scope='formal-process'):
    """Return facts and limitations, not a fabricated v2 complete graph."""
    repository = run.get('repository', {}).get('full_name')
    run_id, attempt = run.get('id'), run.get('run_attempt')
    if (not isinstance(repository, str) or not adapters._REPOSITORY.fullmatch(repository)
            or any(type(v) is not int or v <= 0 for v in (run_id, attempt)) or not adapters.sha(run.get('head_sha'))):
        raise ValueError('invalid_run_identity')
    result = dict(schema_version=SCHEMA, repository=repository, run_id=run_id, run_attempt=attempt,
                  scope=scope, source_sha=run['head_sha'], jobs=[], dependency_bindings=[], artifact_bindings=[],
                  planned_unexecuted=[], missing_reasons=[], analysis=dict(status='unavailable', reason='complete_execution_input_contract_not_proven'))
    missing = result['missing_reasons']
    if not pagination_complete:
        missing.append('incomplete_jobs_pagination')
    jobs, seen, duplicate_ids = [], set(), set()
    for page in pages:
        for job in page.get('jobs', []):
            jid = job.get('id')
            if (type(jid) is not int or jid <= 0 or jid in seen or job.get('run_id') != run_id
                    or job.get('run_attempt') != attempt or job.get('head_sha') != run['head_sha']):
                if jid in seen:
                    duplicate_ids.add(jid)
                missing.append('job_identity_mismatch_or_duplicate'); continue
            seen.add(jid)
            jobs.append(job)
    jobs = [job for job in jobs if job['id'] not in duplicate_ids]
    observations, keyed, conflicting_keys = {}, {}, set()
    for job in jobs:
        jid = job['id']
        if job.get('conclusion') == 'skipped':
            continue  # Never assign a symbolic identity from its display name.
        if jid not in logs:
            continue
        try:
            row, evidence = parse_job_log(logs[jid], job, repository, run_id, attempt)
            if row['workflow_sha'] != workflow_sha or adapters.sha(workflow_sha) is None:
                raise ValueError('workflow_identity_unverified')
            if row['target_source_sha'] is not None and row['target_source_sha'] != run['head_sha']:
                raise ValueError('target_source_mismatch')
            if row['target_source_sha'] is None:
                missing.append('target_source_unavailable:' + str(jid))
            key = (row['job_key'], json.dumps(row['matrix'], sort_keys=True))
            if key in keyed or key in conflicting_keys:
                if key in keyed:
                    observations.pop(keyed.pop(key), None)
                conflicting_keys.add(key)
                raise ValueError('duplicate_runtime_instance')
            keyed[key] = jid
            observations[jid] = dict(job_id=jid, execution_result=job.get('conclusion'),
                                     started_at=adapters.stamp(job.get('started_at')),
                                     completed_at=adapters.stamp(job.get('completed_at')),
                                     observation=row, evidence=evidence)
        except (ValueError, TypeError, KeyError, UnicodeError):
            missing.append('job_observation_unusable:' + str(jid))
    dependencies = {}
    if workflow_raw is not None and adapters.sha(workflow_sha):
        try:
            dependencies = workflow_dependencies(workflow_raw)
            result['workflow'] = dict(sha=workflow_sha, path='.github/workflows/rust.yml',
                                     raw_sha256=hashlib.sha256(workflow_raw).hexdigest())
        except ValueError:
            missing.append('workflow_dependencies_unavailable')
    else:
        missing.append('workflow_source_unavailable')
    closure, pending = set(), [scope]
    while pending:
        key = pending.pop()
        if key not in closure:
            closure.add(key)
            if key not in dependencies:
                missing.append('scope_dependency_unavailable:' + key)
            pending.extend(dependencies.get(key, []))
    groups = {}
    for job in observations.values():
        key = job['observation']['job_key']
        if key in closure:
            groups.setdefault(key, []).append(job)
    for key in sorted(closure):
        if key not in groups:
            skipped = [job['job_id'] for job in observations.values() if job['observation']['needs'].get(key) == 'skipped']
            if skipped:
                result['planned_unexecuted'].append(dict(job_key=key, job_id=None, result='skipped', observed_by=skipped))
            else:
                missing.append('runtime_job_binding_unavailable:' + key)
    for key, group in sorted(groups.items()):
        for job in group:
            row = job['observation']
            result['jobs'].append(job)
            if set(row['needs']) != set(dependencies.get(key, [])):
                missing.append('runtime_dependency_mismatch:' + str(job['job_id'])); continue
            for dep, execution in row['needs'].items():
                upstream = groups.get(dep, [])
                ids = [item['job_id'] for item in upstream]
                if any(item['execution_result'] != execution for item in upstream) and len(upstream) == 1:
                    missing.append('dependency_result_mismatch:' + str(job['job_id']))
                    ids = []
                result['dependency_bindings'].append(dict(consumer_job_id=job['job_id'], producer_job_key=dep,
                    producer_job_ids=ids, observed_result=execution, completeness='unknown' if len(ids) != 1 or (upstream and upstream[0]['observation']['matrix']) else 'single-instance'))
                if len(ids) != 1 or (upstream and upstream[0]['observation']['matrix']):
                    missing.append('dependency_instance_completeness_unverified:' + dep)
    produced = {}
    for job in result['jobs']:
        for item in job['observation']['manifests']:
            if item['direction'] == 'produced':
                if (item['producer_run_id'] == str(run_id) and item['producer_attempt'] == str(attempt)):
                    produced.setdefault(item['manifest_sha256'], []).append((job, item))
    for job in result['jobs']:
        for item in job['observation']['manifests']:
            if item['direction'] != 'observed-input':
                continue
            matches = produced.get(item['manifest_sha256'], [])
            good = [(producer, manifest) for producer, manifest in matches if all(manifest[k] == item[k] for k in
                    ('source_sha', 'tested_sha', 'producer_run_id', 'producer_attempt', 'config_digest'))]
            bound = len(good) == 1 and good[0][0]['job_id'] != job['job_id']
            result['artifact_bindings'].append(dict(manifest_sha256=item['manifest_sha256'],
                producer_job_id=good[0][0]['job_id'] if bound else None, consumer_job_id=job['job_id'],
                content_binding='bound' if bound else 'unknown', artifact_id=None, validation_result='unknown'))
            if not bound:
                missing.append('manifest_producer_binding_unavailable:' + str(job['job_id']))
    # Configuration observations and manifest equality do not prove every actual
    # input, matrix cardinality, artifact transport or consumer validation.
    missing += ['matrix_cardinality_not_proven', 'artifact_transport_and_validation_not_proven',
                'complete_execution_input_contract_not_proven']
    result['missing_reasons'] = sorted(set(missing))
    result['collection_status'] = 'partial'
    return result


def collect_live(repository, run_id, attempt, budget, transport):
    """Reuse bounded read-only transport. No login, workflow dispatch or retries."""
    import base64
    if (not isinstance(repository, str) or not adapters._REPOSITORY.fullmatch(repository)
            or adapters._SECRET.search(repository) or any(type(v) is not int or v <= 0 for v in (run_id, attempt))):
        raise ValueError('invalid_collection_identity')
    reader = ActionsReader(repository, budget, transport)
    try:
        run = reader.json(f'runs/{run_id}/attempts/{attempt}', 'actions/run.json')
    except CollectionStopped as exc:
        return dict(schema_version=SCHEMA, repository=repository, run_id=run_id, run_attempt=attempt,
                    collection_status='partial', transport='live', jobs=[], dependency_bindings=[],
                    artifact_bindings=[], planned_unexecuted=[], missing_reasons=[str(exc)],
                    analysis=dict(status='unavailable', reason='run_source_unavailable'),
                    sources=reader.sources, budget=budget.summary())
    if (not isinstance(run, dict) or run.get('id') != run_id or type(run.get('id')) is not int
            or run.get('run_attempt') != attempt or type(run.get('run_attempt')) is not int
            or not isinstance(run.get('repository'), dict) or run['repository'].get('full_name') != repository
            or adapters.sha(run.get('head_sha')) is None):
        return dict(schema_version=SCHEMA, repository=repository, run_id=run_id, run_attempt=attempt,
                    collection_status='partial', transport='live', jobs=[], dependency_bindings=[],
                    artifact_bindings=[], planned_unexecuted=[], missing_reasons=['run_identity_mismatch'],
                    analysis=dict(status='unavailable', reason='run_identity_mismatch'),
                    sources=reader.sources, budget=budget.summary())
    pages, complete, reasons = reader.pages(f'runs/{run_id}/attempts/{attempt}/jobs', 'jobs', 'jobs')
    logs, revisions = {}, set()
    for page in pages:
        for job in page['jobs']:
            if not any(s.get('name') == STEP for s in job.get('steps', []) if isinstance(s, dict)):
                continue
            if (job.get('run_id') != run_id or job.get('run_attempt') != attempt
                    or job.get('head_sha') != run.get('head_sha')):
                reasons.append('job_identity_mismatch'); continue
            try:
                jid = job['id']
                raw = reader.get(f'jobs/{jid}/logs', f'actions/job-{jid}.log', archive=True)
                budget.charge(len(raw), decoded=True)
                row, _ = parse_job_log(raw, job, repository, run_id, attempt)
                logs[jid] = raw
                if row['workflow_sha']:
                    revisions.add(row['workflow_sha'])
            except (CollectionStopped, ValueError, TypeError, KeyError, UnicodeError):
                reasons.append('job_log_binding_unavailable:' + str(job['id']))
    workflow, revision = None, next(iter(revisions)) if len(revisions) == 1 else None
    if revision:
        try:
            reader.base = 'https://api.github.com/repos/' + repository + '/'
            data = reader.json('contents/.github/workflows/rust.yml?ref=' + revision, 'workflow/rust.json')
            if data.get('encoding') != 'base64' or not isinstance(data.get('content'), str):
                raise ValueError('invalid_workflow_response')
            workflow = base64.b64decode(data['content'].replace('\n', ''), validate=True)
            budget.charge(len(workflow), decoded=True)
        except (CollectionStopped, ValueError, TypeError):
            reasons.append('workflow_source_unavailable')
    result = bind(run, pages, logs, workflow, revision, complete)
    result['missing_reasons'] = sorted(set(result['missing_reasons'] + reasons))
    result['transport'] = 'live'
    result['sources'] = reader.sources
    result['budget'] = budget.summary()
    return result


def live_main(argv):
    parser = argparse.ArgumentParser(description='Collect numeric-job-bound observations using existing read-only permissions.')
    parser.add_argument('repository'); parser.add_argument('run_id', type=int); parser.add_argument('attempt', type=int)
    parser.add_argument('output', type=Path)
    parser.add_argument('--max-seconds', type=float, default=120)
    parser.add_argument('--max-requests', type=int, default=100)
    parser.add_argument('--max-bytes', type=int, default=MAX_INPUT_BYTES)
    args = parser.parse_args(argv)
    budget = CollectionBudget(args.max_requests, args.max_seconds, args.max_bytes)
    transport = LiveTransport(budget)
    import signal
    def cancel(signum, frame):
        budget.cancelled = True
        transport.cancel()
    previous = {sig: signal.signal(sig, cancel) for sig in (signal.SIGINT, signal.SIGTERM)}
    try:
        result = collect_live(args.repository, args.run_id, args.attempt, budget, transport)
        write_json(args.output, result)
        return 1 if result['collection_status'] != 'complete' else 0
    except (OSError, ValueError, TypeError, KeyError, UnicodeError, CollectionStopped):
        print('engineering bindings: collection unavailable', file=sys.stderr)
        return 1
    finally:
        for sig, handler in previous.items():
            signal.signal(sig, handler)


def bindings_main(argv=None):
    argv = sys.argv[1:] if argv is None else argv
    if argv and argv[0] == 'collect-live':
        return live_main(argv[1:])
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('directory', nargs='?', type=Path, help='fixed run.json/jobs-N.json/job-ID.log/workflow-SHA.yml exports')
    parser.add_argument('output', nargs='?', type=Path)
    parser.add_argument('--print-emitter', action='store_true')
    parser.add_argument('--workflow-sha')
    parser.add_argument('--scope', default='formal-process', choices=('formal-process', 'required-gate'))
    args = parser.parse_args(argv)
    if args.print_emitter:
        print(EMITTER, end=''); return 0
    if not args.directory or not args.output:
        parser.error('directory and output required')
    root = args.directory.resolve()
    if root == args.output.resolve() or root in args.output.resolve().parents:
        parser.error('output must be outside input directory')
    budget = CollectionBudget()
    try:
        def read(path):
            if path.is_symlink() or not path.is_file() or path.parent.resolve() != root:
                raise ValueError('input_unavailable')
            budget.remaining()
            raw = path.read_bytes() if path.stat().st_size <= budget.limits['bytes'] - budget.decoded_bytes else b''
            if not raw:
                raise ValueError('input_empty_or_over_budget')
            budget.charge(len(raw), decoded=True)
            return raw
        run = json.loads(read(root / 'run.json'))
        pages, ids, total = [], set(), None
        for number in range(1, 101):
            page = json.loads(read(root / ('jobs-' + str(number) + '.json')))
            count = page.get('total_count')
            rows = page.get('jobs')
            if type(count) is not int or not 0 <= count <= 10000 or not isinstance(rows, list) or len(rows) > 100 or (total is not None and total != count):
                raise ValueError('invalid_jobs_page')
            current = [job.get('id') for job in rows]
            if len(set(current)) != len(current) or ids.intersection(current):
                raise ValueError('duplicate_jobs_page')
            total = count; ids.update(current); pages.append(page)
            if len(ids) == total or len(rows) < 100:
                break
        logs = {jid: read(root / ('job-' + str(jid) + '.log')) for jid in ids if (root / ('job-' + str(jid) + '.log')).exists()}
        workflow = read(root / ('workflow-' + args.workflow_sha + '.yml')) if adapters.sha(args.workflow_sha) else None
        result = bind(run, pages, logs, workflow, args.workflow_sha, len(ids) == total, args.scope)
        result['transport'] = 'recorded'
        result['budget'] = budget.summary()
        write_json(args.output, result)
        return 1 if result['collection_status'] != 'complete' else 0
    except (OSError, ValueError, TypeError, KeyError, UnicodeError) as exc:
        print('engineering bindings: input or observation unavailable', file=sys.stderr)
        return 1

def main(argv=None):
    incoming = sys.argv[1:] if argv is None else argv
    if incoming and incoming[0] == 'bindings':
        return bindings_main(incoming[1:])
    if (sys.argv[1:] if argv is None else argv) == ["_http-get"]:
        return http_worker()
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)
    live = sub.add_parser("collect-github", help="read one explicit Actions attempt, without triggering work")
    live.add_argument("repository"); live.add_argument("run_id", type=int); live.add_argument("attempt", type=int)
    live.add_argument("--recorded-responses", type=Path, help="replay run.json, jobs-N.json, artifacts-N.json and artifact-ID.zip exports")
    live.add_argument("--build-artifact", action="append", type=int, default=[])
    local = sub.add_parser("collect-metrics", help="read an existing compile metrics JSON; never run Cargo")
    local.add_argument("input")
    for command in (live, local):
        command.add_argument("output"); command.add_argument("receipt")
        command.add_argument("--max-requests", type=int, default=100)
        command.add_argument("--max-seconds", type=float, default=120)
        command.add_argument("--max-bytes", type=int, default=MAX_INPUT_BYTES)
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
        if args.command in ("collect-github", "collect-metrics"):
            distinct_paths(args.output, args.receipt, *([args.input] if args.command == "collect-metrics" else []))
            if args.command == "collect-github" and args.recorded_responses:
                root = args.recorded_responses.resolve()
                if any(root == Path(p).resolve() or root in Path(p).resolve().parents for p in (args.output, args.receipt)):
                    raise ValueError("outputs must be outside recorded response directory")
            budget = CollectionBudget(args.max_requests, args.max_seconds, args.max_bytes)
            import signal
            transport = None
            def cancel(signum, frame):
                budget.cancelled = True
                # During CPU projection, the next checkpoint preserves the work
                # already normalized. During HTTP, terminate only our own child.
                if isinstance(transport, LiveTransport):
                    transport.cancel()
            previous = {sig: signal.signal(sig, cancel) for sig in (signal.SIGINT, signal.SIGTERM)}
            try:
                if args.command == "collect-github":
                    transport = RecordedTransport(args.recorded_responses) if args.recorded_responses else LiveTransport(budget)
                    result, receipt = collect_actions(args.repository, args.run_id, args.attempt, budget, transport, args.build_artifact)
                else:
                    result, receipt = collect_metrics(args.input, budget)
            finally:
                for sig, handler in previous.items():
                    signal.signal(sig, handler)
            write_json(args.output, result)
            # The receipt's digest detects an old receipt if the second atomic write fails.
            write_json(args.receipt, receipt)
            return 1 if result["collection_status"] != "complete" else 0
        elif args.command == "collect":
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
