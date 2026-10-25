"""Shared local profile report shape, validation and read-time freshness."""
import importlib.util
import json
from pathlib import Path
import re

_spec = importlib.util.spec_from_file_location("profile_adapter", Path(__file__).with_name("p2p-profile-readiness.py"))
adapter = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(adapter)
MAX_AGE_MS = 60_000
RESULTS = {"success", "spawn_failed", "command_failed", "timeout", "output_limit", "invalid_status", "interrupted"}


def unknown(reason):
    return {"api_version": "oasis7.profile-readiness/v1", "source": "local_cli", "status": "unknown",
            "reason": reason, "can_propose": False, "trust_boundary": "producer_observation_only",
            "observed_at_unix_ms": None, "age_ms": None, "max_sample_age_ms": MAX_AGE_MS}


def age(observed, now):
    if type(observed) is not int or observed < 0 or observed > now:
        return {"state": "unknown", "age_ms": None, "as_of_unix_ms": now}
    elapsed = now - observed
    return {"state": "fresh" if elapsed <= MAX_AGE_MS else "stale", "age_ms": elapsed, "as_of_unix_ms": now}


def refresh(report, now):
    report = dict(report)
    collection = report.get("collection") or {}
    freshness = age(collection.get("finished_at_unix_ms"), now)
    started = collection.get("started_at_unix_ms")
    finished = collection.get("finished_at_unix_ms")
    if type(started) is not int or started < 0 or type(finished) is not int or finished < started:
        freshness = {"state": "unknown", "age_ms": None, "as_of_unix_ms": now}
    report["collection_freshness"] = freshness
    report["current_measurement_available"] = collection.get("complete") is True and freshness["state"] == "fresh"
    profile = dict(report.get("profile_readiness") or unknown("profile_unavailable"))
    observed = age(profile.get("observed_at_unix_ms"), now)
    profile.update(as_of_unix_ms=now, age_ms=observed["age_ms"], observation_freshness=observed["state"])
    if profile.get("status") == "ready" and observed["state"] != "fresh":
        profile.update(status="stale" if observed["state"] == "stale" else "unknown",
                       reason="observation_expired" if observed["state"] == "stale" else "observation_clock_uncertain")
    if profile.get("status") == "ready" and not report["current_measurement_available"]:
        profile.update(status="stale" if freshness["state"] == "stale" else "unknown",
                       reason="collection_expired" if freshness["state"] == "stale" else "collection_unavailable")
    report["profile_readiness"] = profile
    ready = report["current_measurement_available"] and profile.get("status") == "ready"
    report["overall"] = {"status": "pass_candidate" if ready else "blocked", "alerts": [] if ready else ["profile_not_ready"],
                         "scope": "local_profile_observation_only"}
    return report


def build(profile, collection, now):
    return refresh({"schema_version": 2, "collection": collection, "profile_readiness": profile}, now)


def markdown(report):
    # The existing adapter renderer safely wraps arbitrary identifiers as JSON code spans.
    lines = ["# Local node profile observation", "", "Collection and role readiness are separate."]
    collection_lines = adapter.markdown({key: report.get(key) for key in ("overall", "current_measurement_available", "collection", "collection_freshness", "last_success")})
    lines += [line.replace("## Profile Readiness", "## Collection") for line in collection_lines]
    lines += adapter.markdown(report["profile_readiness"])
    return "\n".join(lines) + "\n"


def identity(profile):
    fields = ("runtime_profile", "node_id", "world_id", "chain_id", "incarnation", "generation", "config_digest")
    return {key: profile[key] for key in fields} if all(profile.get(key) is not None for key in fields) else None


def validate_source(source, directory):
    if not isinstance(source, dict) or source.get("schema_version") != 2:
        raise ValueError("invalid node-profile report schema")
    config = json.loads((directory / "config.json").read_text())
    c = source.get("collection")
    p = source.get("profile_readiness")
    if not isinstance(c, dict) or not isinstance(p, dict) or p.get("api_version") != "oasis7.profile-readiness/v1" or p.get("source") != "local_cli":
        raise ValueError("invalid node-profile report shape")
    if c.get("mode") != "local_cli" or c.get("result") not in RESULTS:
        raise ValueError("invalid node-profile collection mode or result")
    for key in ("binary_sha256", "data_dir_sha256"):
        if not isinstance(c.get(key), str) or not re.fullmatch(r"[0-9a-f]{64}", c[key]) or c[key] != config.get(key):
            raise ValueError("node-profile scope mismatch")
    if type(c.get("timeout_ms")) is not int or not 1 <= c["timeout_ms"] <= 60000 or c["timeout_ms"] != config.get("timeout_ms"):
        raise ValueError("invalid node-profile timeout")
    expected = {"expected_sample_count": 1, "attempted_count": 1,
                "failed_sample_count": 0 if c["result"] == "success" else 1,
                "complete": c["result"] == "success"}
    if any(type(c.get(k)) is not type(v) or c[k] != v for k,v in expected.items()):
        raise ValueError("invalid node-profile collection counts")
    for key in ("started_at_unix_ms", "finished_at_unix_ms"):
        if type(c.get(key)) is not int or not 0 <= c[key] < 2**64:
            raise ValueError("invalid node-profile collection timestamp")
    if p.get("status") not in ("ready", "blocked", "stale", "unknown", "unsupported") or p.get("can_propose") is not False:
        raise ValueError("invalid node-profile capability")
    if not c["complete"] and p.get("status") != "unknown":
        raise ValueError("failed CLI result claims profile readiness")
