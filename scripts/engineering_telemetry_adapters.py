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


# The v2 contract is deliberately separate from the legacy projections above.
# Provenance proves a projection against supplied bytes, not source authenticity.
CONTRACT_INPUT = "oasis7-engineering-telemetry-input/v2"
CONTRACT_SNAPSHOT = "oasis7-engineering-telemetry-snapshot/v2"
SOURCE_SCHEMAS = {
    "github-actions-run/v1": "actions",
    "github-actions-jobs/v1": "actions",
    "oasis7-workflow-plan/v1": "workflow-plan",
    "oasis7-artifact-binding/v1": "artifact-binding",
    "oasis7-local-invocation/v1": "local-invocation",
    "oasis7-build-manifest/v2": "build-manifest",
    "oasis7-build-report/v1": "build-report",
    "oasis7-process-jsonl/v1": "process-jsonl",
    "oasis7-compile-metrics/v2": "compile-metrics",
    "oasis7-telemetry-fixture/v1": "fixture",
}
CONTEXT_FIELDS = tuple("repository run_id run_attempt source_sha tested_sha policy_sha config_digest local_invocation_id dirty_inputs_digest input_capture created_at gate_id job_ids pagination_complete dependencies_complete conditions_complete".split())
RECORD_FIELDS = tuple("kind job_id step_number parent_id depends_on job_key matrix source_sha tested_sha policy_sha config_digest started_at completed_at elapsed_seconds clock execution_result condition target profile features default_features toolchain runner producer exit_code timeout cancelled passed count_ok cleanup_result".split())
IDENTITY_FIELDS = ("source_sha", "tested_sha", "policy_sha", "config_digest")
COVERAGE_FIELDS = ("pagination_complete", "dependencies_complete", "conditions_complete")
_TOKEN = re.compile(r"^[A-Za-z0-9][A-Za-z0-9_.:-]{0,127}$")
_DIGEST = re.compile(r"^[0-9a-f]{64}$")
_REPOSITORY = re.compile(r"^[A-Za-z0-9_.-]{1,100}/[A-Za-z0-9_.-]{1,100}$")
_SECRET = re.compile(r"(?i)(?:gh[pousr]_[A-Za-z0-9]|github_pat_|AKIA[0-9A-Z]|bearer|password|secret|authorization|access.token|private.key)")
_POINTER_KEYS = frozenset(CONTEXT_FIELDS + RECORD_FIELDS + (
    "id", "contexts", "records", "jobs", "steps", "phases", "tests", "run", "plan",
    "binding", "invocation", "manifest", "report", "metrics", "processes", "rows",
    "identity", "consumer", "producer", "configuration", "build", "head_sha",
    "head_commit", "sha", "conclusion", "status", "number", "attempt", "full_name",
    "commit_oid", "producer_run_id", "producer_attempt", "producer_job_id", "matrix",
    "os", "arch", "package", "packages", "shard", "variant", "target_triple",
))


def _json_bytes(value):
    try:
        return json.dumps(value, ensure_ascii=True, sort_keys=True,
                          separators=(",", ":"), allow_nan=False).encode("ascii")
    except (TypeError, ValueError, RecursionError) as exc:
        raise ValueError("non-JSON contract value") from exc


def _token(value):
    return isinstance(value, str) and _TOKEN.fullmatch(value) is not None and not _SECRET.search(value)


def _reference(value):
    return (isinstance(value, str) and len(value) <= 256 and not value.startswith("/")
            and all(_token(part) and part not in (".", "..") for part in value.split("/")))


def _require(ok, message):
    if not ok:
        raise ValueError(message)


def _pointer(pointer):
    _require(isinstance(pointer, str) and len(pointer) <= 512 and pointer.startswith("/"), "invalid source pointer")
    parts = pointer[1:].split("/")
    _require(len(parts) <= 16 and all(p in _POINTER_KEYS or (p.isascii() and p.isdigit() and len(p) <= 8) for p in parts), "unsafe source pointer")
    return parts


def _resolve(payload, pointer):
    value = payload
    for part in _pointer(pointer):
        if isinstance(value, dict) and part in value:
            value = value[part]
        elif isinstance(value, list) and part.isdigit() and str(int(part)) == part and int(part) < len(value):
            value = value[int(part)]
        else:
            raise ValueError("source pointer does not resolve")
    return value


def _same(left, right):
    # JSON numeric equality must not turn true into 1 or 1.0 into 1.
    if type(left) is not type(right):
        return False
    if isinstance(left, list):
        return len(left) == len(right) and all(_same(a, b) for a, b in zip(left, right))
    if isinstance(left, dict):
        return left.keys() == right.keys() and all(_same(left[k], right[k]) for k in left)
    return left == right


def _field_value(field, value):
    if value is None:
        return
    if field in ("source_sha", "tested_sha", "policy_sha"):
        valid = sha(value) is not None
    elif field in ("config_digest", "dirty_inputs_digest"):
        valid = isinstance(value, str) and _DIGEST.fullmatch(value)
    elif field == "repository":
        valid = isinstance(value, str) and _REPOSITORY.fullmatch(value) and not _SECRET.search(value)
    elif field in ("run_id", "run_attempt", "job_id", "step_number"):
        valid = type(value) is int and 0 < value <= 2**63 - 1
    elif field in ("pagination_complete", "dependencies_complete", "conditions_complete", "default_features", "timeout", "cancelled", "passed", "count_ok"):
        valid = type(value) is bool
    elif field in ("created_at", "started_at", "completed_at"):
        valid = stamp(value) is not None
    elif field == "elapsed_seconds":
        valid = type(value) in (int, float) and math.isfinite(value) and 0 <= value <= 1e9
    elif field == "exit_code":
        valid = type(value) is int and -(2**31) <= value <= 2**31 - 1
    elif field in ("features", "job_ids", "depends_on"):
        valid = isinstance(value, list) and len(value) <= 10000 and all(_token(v) for v in value) and len(value) == len(set(value))
    elif field == "matrix":
        valid = isinstance(value, dict) and len(value) <= 32 and all(
            _token(k) and (type(v) is bool or (type(v) is int and abs(v) <= 2**31) or _token(v)) for k, v in value.items())
    elif field == "producer":
        required = {"repository", "run_id", "run_attempt", "job_id", "source_sha", "tested_sha", "config_digest"}
        _require(isinstance(value, dict) and set(value) == required and all(v is not None for v in value.values()), "incomplete producer identity")
        for key, item in value.items():
            _field_value(key, item)
        valid = True
    elif field in ("kind", "clock", "execution_result", "condition", "input_capture", "cleanup_result"):
        choices = {"kind": {"job", "step", "phase", "test", "run"},
                   "clock": {"wall", "monotonic", "estimated", "unknown"},
                   "execution_result": RESULTS | {"unknown", "in_progress"}, "condition": {"true", "false", "unknown"},
                   "input_capture": {"at_measurement", "unknown"},
                   "cleanup_result": {"clean", "unresolved"}}
        valid = isinstance(value, str) and value in choices[field]
    else:
        valid = _token(value)
    _require(valid, "invalid or unsafe field: " + field)


def _provenance(prov, value, sources, payloads=None):
    _require(isinstance(prov, dict), "invalid field provenance")
    state = prov.get("state")
    if state == "known":
        _require(set(prov) == {"state", "reference", "pointer"} and value is not None, "invalid known provenance")
        reference = prov["reference"]
        _require(_reference(reference) and reference in sources, "unknown or ambiguous source reference")
        _pointer(prov["pointer"])
        if payloads is not None:
            _require(_same(value, _resolve(payloads[reference], prov["pointer"])), "field differs from source")
    else:
        _require(state in ("unknown", "not_applicable") and set(prov) == {"state", "reason"} and _token(prov["reason"]) and value is None, "invalid unavailable provenance")
    return dict(prov)


def _entity(raw, fields, sources, payloads=None, snapshot=False):
    _require(isinstance(raw, dict), "invalid contract entity")
    internal = {"id", "field_sources"} | ({"context_id"} if fields == RECORD_FIELDS else set())
    derived = {"identity_status", "identity_missing_reasons", "identity_conflict", "aliases", "provenance_missing_reasons"} if snapshot else set()
    _require(set(raw) <= set(fields) | internal | derived, "unexpected entity field")
    _require(_token(raw.get("id")), "invalid entity id")
    result = {"id": raw["id"]}
    if fields == RECORD_FIELDS:
        _require(_token(raw.get("context_id")), "invalid context reference")
        result["context_id"] = raw["context_id"]
    provenance = raw.get("field_sources", {})
    _require(isinstance(provenance, dict) and set(provenance) <= set(fields), "unexpected provenance field")
    result["field_sources"] = {}
    for field in fields:
        value = raw.get(field)
        _field_value(field, value)
        entry = provenance.get(field)
        if entry is None:
            _require(not snapshot and value is None, "missing field provenance")
            entry = {"state": "unknown", "reason": "not_supplied"}
        entries = entry if isinstance(entry, list) else [entry]
        _require(not snapshot or isinstance(entry, list), "snapshot provenance must be a list")
        _require(isinstance(entries, list) and 0 < len(entries) <= 10000, "invalid provenance list")
        result[field] = value
        result["field_sources"][field] = [_provenance(p, value, sources, payloads) for p in entries]
    return result


def _context_status(context, origin):
    github = ("repository", "run_id", "run_attempt")
    local = ("local_invocation_id", "dirty_inputs_digest", "input_capture")
    inactive = local if origin == "github" else github
    for field in inactive:
        _require(context[field] is None and all(p["state"] == "not_applicable" for p in context["field_sources"][field]), "inapplicable identity must be explicit")
    required = IDENTITY_FIELDS + (github if origin == "github" else local)
    missing = ["missing_" + field for field in required if context[field] is None]
    if origin == "local" and context["input_capture"] != "at_measurement":
        missing.append("measurement_inputs_not_captured")
    return sorted(set(missing))


def _identity(record, context, origin):
    if record["kind"] is None:
        return None
    if origin == "github":
        if any(context[f] is None for f in ("repository", "run_id", "run_attempt")):
            return None
        base = (origin, context["repository"], str(context["run_id"]), str(context["run_attempt"]))
        kind = record["kind"]
        if kind != "run" and record["job_id"] is None:
            return None
        if kind == "step" and record["step_number"] is None:
            return None
        return base + (kind, str(record["job_id"]), str(record["step_number"]) if kind == "step" else record["id"] if kind in ("phase", "test") else "")
    if context["local_invocation_id"] is None:
        return None
    return (origin, context["local_invocation_id"], record["kind"], record["id"])


def _record_missing(record, context, origin):
    missing = list(context["identity_missing_reasons"])
    if _identity(record, context, origin) is None:
        missing.append("missing_record_identity")
    for field in IDENTITY_FIELDS:
        if record[field] is None:
            missing.append("missing_" + field)
        elif context[field] is not None and record[field] != context[field]:
            missing.append("context_" + field + "_mismatch")
    return sorted(set(missing))


def _source_index(summaries):
    index, ambiguous = {}, set()
    for source in summaries:
        _require(isinstance(source, dict) and set(source) == {"reference", "kind", "source_schema", "content_sha256"}, "invalid source summary")
        _require(_reference(source["reference"]) and _token(source["kind"]), "invalid source reference")
        schema = source["source_schema"]
        _require(isinstance(schema, str) and len(schema) <= 128 and re.fullmatch(r"[a-z0-9][a-z0-9-]*/v[0-9]+", schema), "invalid source schema")
        _require(isinstance(source["content_sha256"], str) and _DIGEST.fullmatch(source["content_sha256"]), "invalid source digest")
        ref = source["reference"]
        if ref in index and index[ref] != source:
            ambiguous.add(ref)
        index[ref] = source
    return {ref: s for ref, s in index.items() if ref not in ambiguous and SOURCE_SCHEMAS.get(s["source_schema"]) == s["kind"]}, ambiguous


def _envelope(document, schema):
    _require(isinstance(document, dict) and document.get("schema_version") == schema, "unsupported contract schema")
    _require(_token(document.get("collection_id")) and stamp(document.get("collected_at")) is not None, "invalid collection identity")
    _require(document.get("origin") in ("github", "local") and document.get("sample_kind") in ("synthetic", "recorded"), "invalid collection kind")
    for field in (("sources" if schema == CONTRACT_INPUT else "evidence"), "contexts", "records"):
        _require(isinstance(document.get(field), list) and len(document[field]) <= 100000, "invalid collection array")
    digest(_json_bytes(document))


def collect_contract(document):
    """Verify exact source projections and emit a bounded, payload-free snapshot.

    Unknown schemas and conflicting source references remain evidence, but cannot
    establish a known fact. Missing facts stay null with explicit provenance.
    """
    _envelope(document, CONTRACT_INPUT)
    _require(set(document) == {"schema_version", "collection_id", "collected_at", "origin", "sample_kind", "sources", "contexts", "records"}, "unexpected input field")
    summaries, payloads = [], {}
    for source in document["sources"]:
        _require(isinstance(source, dict) and set(source) == {"reference", "kind", "source_schema", "payload"}, "invalid input source")
        summary = {k: source[k] for k in ("reference", "kind", "source_schema")}
        summary["content_sha256"] = digest(_json_bytes(source["payload"]))
        _source_index([summary])
        summaries.append(summary)
        payloads[source["reference"]] = source["payload"]
    sources, ambiguous = _source_index(summaries)
    _require(document["sample_kind"] == "synthetic" or all(s["kind"] != "fixture" for s in summaries), "fixture requires synthetic sample")
    reasons = []
    if ambiguous:
        reasons.append("conflicting_source_reference")
    if any(SOURCE_SCHEMAS.get(s["source_schema"]) != s["kind"] for s in summaries):
        reasons.append("unsupported_source_schema")
    # Downgrade references to unsupported/ambiguous evidence without copying their
    # proposed values. Other malformed provenance remains an explicit error.
    def normalize(raw, fields):
        _require(isinstance(raw, dict), "invalid contract entity")
        raw = dict(raw)
        fs = raw.get("field_sources", {})
        _require(isinstance(fs, dict) and set(fs) <= set(fields), "invalid field provenance")
        fs = {k: v if isinstance(v, list) else [v] for k, v in fs.items()}
        raw["field_sources"] = fs
        failures = set()
        for field, entries in fs.items():
            _require(bool(entries) and len(entries) <= 10000, "invalid provenance list")
            good, failed = [], []
            for prov in entries:
                _require(isinstance(prov, dict), "invalid field provenance")
                if prov.get("state") != "known":
                    good.append(prov)
                    continue
                _require(set(prov) == {"state", "reference", "pointer"} and _reference(prov["reference"]), "invalid source provenance")
                _pointer(prov["pointer"])
                _require(any(s["reference"] == prov["reference"] for s in summaries), "missing source reference")
                failure = None
                if prov["reference"] not in sources:
                    failure = "unusable_source"
                else:
                    try:
                        resolved = _resolve(payloads[prov["reference"]], prov["pointer"])
                        if not _same(raw.get(field), resolved):
                            failure = "source_value_mismatch"
                    except ValueError:
                        failure = "source_pointer_missing"
                if failure:
                    failures.add(failure)
                    failed.append(failure)
                else:
                    good.append(prov)
            if failed and not good:
                raw[field] = None
                fs[field] = [{"state": "unknown", "reason": failed[0]}]
            else:
                fs[field] = good
        entity = _entity(raw, fields, sources, payloads)
        entity["provenance_missing_reasons"] = sorted(failures)
        if failures:
            reasons.append("incomplete_provenance")
        return entity
    contexts, context_map = [], {}
    for raw in document["contexts"]:
        context = normalize(raw, CONTEXT_FIELDS)
        _require(context["id"] not in context_map, "duplicate context id")
        missing = _context_status(context, document["origin"])
        context.update(identity_status="partial" if missing else "complete", identity_missing_reasons=missing, identity_conflict=False)
        contexts.append(context)
        context_map[context["id"]] = context
        if missing:
            reasons.append("partial_context_identity")
        if any(context[field] is not True for field in COVERAGE_FIELDS):
            reasons.append("incomplete_context_coverage")
    for group in _context_groups(contexts, document["origin"]).values():
        if any(not all(_same(group[0][field], other[field]) for field in CONTEXT_FIELDS) for other in group[1:]):
            reasons.append("conflicting_context_identity")
            for context in group:
                context["identity_conflict"] = True
    records, groups = [], {}
    for raw in document["records"]:
        record = normalize(raw, RECORD_FIELDS)
        _require(record["context_id"] in context_map, "missing record context")
        context = context_map[record["context_id"]]
        missing = _record_missing(record, context, document["origin"])
        record.update(identity_status="partial" if missing else "complete", identity_missing_reasons=missing,
                      identity_conflict=False, aliases=[record["id"]])
        if missing:
            reasons.append("partial_record_identity")
        identity = _identity(record, context, document["origin"])
        # Run/job identity establishes the conflict group even if revision facts
        # are incomplete or disagree. Only complete observations may collapse.
        group = groups.setdefault(identity, []) if identity is not None else []
        duplicate = next((old for old in group if not missing and not old["identity_missing_reasons"] and old["context_id"] == record["context_id"] and all(_same(old[f], record[f]) for f in RECORD_FIELDS)
                          and all(_same(context_map[old["context_id"]][f], context[f]) for f in IDENTITY_FIELDS)), None)
        if duplicate:
            if record["id"] not in duplicate["aliases"]:
                duplicate["aliases"].append(record["id"])
            duplicate["provenance_missing_reasons"] = sorted(set(duplicate["provenance_missing_reasons"] + record["provenance_missing_reasons"]))
            for field in RECORD_FIELDS:
                for prov in record["field_sources"][field]:
                    if prov not in duplicate["field_sources"][field]:
                        duplicate["field_sources"][field].append(prov)
            continue
        if any(not _record_facts_equal(old, record, context_map) for old in group):
            reasons.append("conflicting_record_identity")
            record["identity_conflict"] = True
            for old in group:
                old["identity_conflict"] = True
        group.append(record)
        records.append(record)
    if _ambiguous_aliases(records, context_map, document["origin"]):
        reasons.append("ambiguous_record_alias")
    if not contexts or not records:
        reasons.append("empty_collection")
    result = {key: document[key] for key in ("collection_id", "collected_at", "origin", "sample_kind")}
    result.update(schema_version=CONTRACT_SNAPSHOT, evidence=summaries, contexts=contexts, records=records,
                  collection_status="partial" if reasons else "complete", missing_reasons=sorted(set(reasons)))
    validate_contract_snapshot(result)
    return result


def validate_contract_snapshot(snapshot):
    """Revalidate the report boundary, without claiming raw-source authentication."""
    _envelope(snapshot, CONTRACT_SNAPSHOT)
    _require(set(snapshot) == {"schema_version", "collection_id", "collected_at", "origin", "sample_kind", "evidence", "contexts", "records", "collection_status", "missing_reasons"}, "unexpected snapshot field")
    sources, ambiguous = _source_index(snapshot["evidence"])
    _require(snapshot["sample_kind"] == "synthetic" or all(s["kind"] != "fixture" for s in snapshot["evidence"]), "fixture requires synthetic sample")
    _require(snapshot["collection_status"] in ("complete", "partial"), "invalid collection status")
    reasons = snapshot["missing_reasons"]
    _require(isinstance(reasons, list) and len(reasons) <= 1000 and all(_token(r) for r in reasons), "invalid missing reasons")
    must_partial = bool(ambiguous) or len(sources) < len({s["reference"] for s in snapshot["evidence"]})
    contexts = {}
    for raw in snapshot["contexts"]:
        context = _entity(raw, CONTEXT_FIELDS, sources, snapshot=True)
        _require(set(raw) == set(context) | {"identity_status", "identity_missing_reasons", "provenance_missing_reasons", "identity_conflict"}, "invalid context summary")
        _require(context["id"] not in contexts, "duplicate context id")
        missing = _context_status(context, snapshot["origin"])
        _require(raw["identity_missing_reasons"] == missing and raw["identity_status"] == ("partial" if missing else "complete"), "incorrect context identity status")
        _validate_provenance_reasons(raw)
        _require(type(raw["identity_conflict"]) is bool, "invalid context conflict flag")
        must_partial |= bool(raw["provenance_missing_reasons"]) or raw["identity_conflict"]
        contexts[context["id"]] = raw
        incomplete_coverage = any(context[field] is not True for field in COVERAGE_FIELDS)
        _require(not incomplete_coverage or "incomplete_context_coverage" in reasons, "omitted incomplete coverage reason")
        must_partial |= bool(missing) or incomplete_coverage
    for group in _context_groups(list(contexts.values()), snapshot["origin"]).values():
        conflicting = any(not all(_same(group[0][field], other[field]) for field in CONTEXT_FIELDS) for other in group[1:])
        _require(all(c["identity_conflict"] == conflicting for c in group), "incorrect context identity conflict flag")
        _require(not conflicting or "conflicting_context_identity" in reasons, "omitted context conflict reason")
    groups = {}
    for raw in snapshot["records"]:
        record = _entity(raw, RECORD_FIELDS, sources, snapshot=True)
        _require(set(raw) == set(record) | {"identity_status", "identity_missing_reasons", "identity_conflict", "aliases", "provenance_missing_reasons"}, "invalid record summary")
        _validate_provenance_reasons(raw)
        must_partial |= bool(raw["provenance_missing_reasons"])
        aliases = raw["aliases"]
        _require(isinstance(aliases, list) and 0 < len(aliases) <= 100000 and aliases[0] == record["id"] and all(_token(a) for a in aliases) and len(set(aliases)) == len(aliases), "invalid record aliases")
        _require(record["context_id"] in contexts, "missing record context")
        context = contexts[record["context_id"]]
        missing = _record_missing(record, context, snapshot["origin"])
        _require(raw["identity_missing_reasons"] == missing and raw["identity_status"] == ("partial" if missing else "complete"), "incorrect record identity status")
        _require(type(raw["identity_conflict"]) is bool, "invalid conflict flag")
        identity = _identity(record, context, snapshot["origin"])
        if identity is not None:
            groups.setdefault(identity, []).append(raw)
        else:
            _require(not raw["identity_conflict"], "unidentified record cannot claim identity conflict")
        if (identity is None or missing) and len(aliases) != 1:
            raise ValueError("partial identities cannot have aliases")
        must_partial |= bool(missing) or raw["identity_conflict"]
    for group in groups.values():
        for index, record in enumerate(group):
            for other in group[index + 1:]:
                _require(record["context_id"] != other["context_id"] or record["identity_missing_reasons"]
                         or other["identity_missing_reasons"] or not _record_facts_equal(record, other, contexts),
                         "duplicate complete observation was not merged")
        conflicting = any(not _record_facts_equal(group[0], other, contexts) for other in group[1:])
        _require(all(r["identity_conflict"] == conflicting for r in group), "incorrect identity conflict flag")
    ambiguous_aliases = _ambiguous_aliases(snapshot["records"], contexts, snapshot["origin"])
    _require(not ambiguous_aliases or "ambiguous_record_alias" in reasons, "omitted ambiguous alias reason")
    must_partial |= ambiguous_aliases or not contexts or not snapshot["records"]
    _require(snapshot["collection_status"] == ("partial" if reasons else "complete") and (not must_partial or bool(reasons)), "incorrect collection status")


def _validate_provenance_reasons(entity):
    reasons = entity.get("provenance_missing_reasons")
    allowed = {"unusable_source", "source_value_mismatch", "source_pointer_missing"}
    _require(isinstance(reasons, list) and len(reasons) <= len(allowed)
             and all(isinstance(r, str) and r in allowed for r in reasons)
             and len(set(reasons)) == len(reasons), "invalid provenance missing reasons")
    for entries in entity["field_sources"].values():
        for entry in entries:
            if entry.get("reason") in allowed:
                _require(entry["reason"] in reasons, "omitted provenance failure")


def _record_facts_equal(left, right, contexts):
    return (all(_same(left[f], right[f]) for f in RECORD_FIELDS)
            and all(_same(contexts[left["context_id"]][f], contexts[right["context_id"]][f]) for f in IDENTITY_FIELDS))


def _context_groups(contexts, origin):
    fields = ("repository", "run_id", "run_attempt") if origin == "github" else ("local_invocation_id",)
    groups = {}
    for context in contexts:
        identity = tuple(context[field] for field in fields)
        if any(value is None for value in identity):
            _require(not context["identity_conflict"], "unidentified context cannot claim identity conflict")
        else:
            groups.setdefault(identity, []).append(context)
    return groups


def _ambiguous_aliases(records, contexts, origin):
    owners = {}
    ambiguous = False
    for record in records:
        identity = _identity(record, contexts[record["context_id"]], origin)
        for alias in record["aliases"]:
            key = (record["context_id"], alias)
            if key in owners and (identity is None or owners[key] != identity):
                ambiguous = True
            owners[key] = identity
    return ambiguous
