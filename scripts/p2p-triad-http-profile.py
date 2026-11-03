#!/usr/bin/env python3
"""Explicit bounded HTTP profile child and read-time diagnostic projection.

The enclosing observability attempt owns ordering/publication. No raw producer
DTO, endpoint, credentials or error body is saved in the report.
"""
import argparse
import hashlib
import http.client
import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import time
from urllib.parse import urlsplit

_spec = importlib.util.spec_from_file_location("http_profile_adapter", Path(__file__).with_name("p2p-profile-readiness.py"))
adapter = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(adapter)
LABELS = ("local_node", "sequencer_ecs", "storage_ecs")
CAP = 4 * 1024 * 1024
TIMEOUT = 5
START = "<!-- triad-profile-start -->"
END = "<!-- triad-profile-end -->"


def now():
    return time.time_ns() // 1_000_000


def endpoint(value):
    try:
        value.encode("utf-8")
        url = urlsplit(value)
        if (len(value) > 2048 or url.scheme not in ("http", "https") or not url.hostname
                or url.username is not None or url.password is not None or url.fragment
                or any(ord(c) <= 32 or ord(c) == 127 for c in value)):
            raise ValueError()
        _ = url.port
        return url
    except (ValueError, TypeError, UnicodeError):
        raise ValueError("invalid profile endpoint") from None


def unknown(reason, state="unknown"):
    result = adapter.project(None, "http", now())
    result.update(status=state, reason=reason)
    return result


def _fetch(url, deadline, owned):
    cls = http.client.HTTPSConnection if url.scheme == "https" else http.client.HTTPConnection
    connection = cls(url.hostname, url.port, timeout=TIMEOUT)
    owned.append(connection)
    try:
        if time.monotonic() >= deadline:
            return None, "timeout"
        connection.request("GET", (url.path or "/") + ("?" + url.query if url.query else ""), headers={"Accept": "application/json"})
        response = connection.getresponse()
        if response.status != 200:
            return None, "http_failed"  # Includes redirects: never cross scope.
        chunks = bytearray()
        while True:
            if response.isclosed():
                break
            left = deadline - time.monotonic()
            if left <= 0:
                return None, "timeout"
            # HTTP/1.1 and connection-close responses retain the live socket
            # through HTTPResponse.fp even after HTTPConnection releases it.
            sock = connection.sock or response.fp.raw._sock
            sock.settimeout(left)
            chunk = response.read1(min(65536, CAP + 1 - len(chunks)))
            if time.monotonic() >= deadline:
                return None, "timeout"
            if not chunk:
                break
            chunks.extend(chunk)
            if len(chunks) > CAP:
                return None, "output_limit"
        status = json.loads(chunks.decode("utf-8"))
        if not isinstance(status, dict):
            return None, "invalid_status"
        return status, "success"
    except (TimeoutError,):
        return None, "timeout"
    except (OSError, http.client.HTTPException, UnicodeError, ValueError, AttributeError):
        return None, "timeout" if time.monotonic() >= deadline else "invalid_status"
    finally:
        connection.close()



def fetch(value):
    endpoint(value)
    # A separate owned process bounds DNS/TLS/headers as well as body reads.
    # Timeout kills and waits for that exact worker; it cannot issue a late
    # probe after the collector has returned. Raw DTO bytes remain pipe-local.
    worker = """import importlib.util,json,sys,time
spec=importlib.util.spec_from_file_location('profile_http_worker',sys.argv[1])
m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m)
m.TIMEOUT=float(sys.argv[3])
print(json.dumps(m._fetch(m.endpoint(sys.argv[2]),time.monotonic()+m.TIMEOUT,[])))
"""
    try:
        process = subprocess.Popen([sys.executable, "-B", "-c", worker, str(Path(__file__).resolve()), value, str(TIMEOUT)],
                                   stdout=subprocess.PIPE, stderr=subprocess.DEVNULL)
    except OSError:
        return None, "spawn_failed"
    try:
        output, _ = process.communicate(timeout=TIMEOUT)
        if process.returncode != 0:
            return None, "invalid_status"
        return tuple(json.loads(output))
    except subprocess.TimeoutExpired:
        process.kill()
        process.communicate()
        return None, "timeout"
    except (ValueError, UnicodeError):
        return None, "invalid_status"


def refresh_node(node, as_of):
    node = dict(node)
    collection = dict(node.get("profile_collection") or {})
    profile = dict(node.get("profile_readiness") or unknown("profile_unavailable"))
    started, finished = collection.get("started_at_unix_ms"), collection.get("finished_at_unix_ms")
    valid = (type(started) is int and type(finished) is int and 0 <= started <= finished <= as_of)
    collection_age = as_of - finished if valid else None
    collection["age_ms"] = collection_age
    collection["freshness"] = "fresh" if valid and collection_age <= adapter.MAX_AGE_MS else "stale" if valid else "unknown"
    observed = profile.get("observed_at_unix_ms")
    valid_observed = type(observed) is int and 0 <= observed <= as_of
    profile.update(as_of_unix_ms=as_of, age_ms=as_of - observed if valid_observed else None)
    if profile.get("status") == "ready":
        if not valid_observed:
            profile.update(status="unknown", reason="observation_clock_uncertain")
        elif as_of - observed > adapter.MAX_AGE_MS:
            profile.update(status="stale", reason="observation_expired")
        elif (collection.get("result") != "success" or collection.get("complete") is not True
              or collection["freshness"] != "fresh"):
            profile.update(status="stale" if valid else "unknown", reason="collection_unavailable")
    node.update(profile_collection=collection, profile_readiness=profile)
    return node


def refresh_report(report, as_of):
    report = dict(report)
    report["nodes"] = {label: refresh_node(node, as_of) if "profile_readiness" in node else node
                       for label, node in report.get("nodes", {}).items()}
    return report


def markdown(report):
    lines = [START, "## HTTP Profile Observations", "", "Producer observations only; legacy fleet health and role readiness are separate."]
    for label, node in report.get("nodes", {}).items():
        if "profile_readiness" not in node:
            continue
        lines += ["", "### " + label]
        lines += adapter.markdown({"collection": node["profile_collection"], **node["profile_readiness"]})
    return "\n".join(lines + [END, ""])


def replace_markdown(original, report):
    if START in original:
        original = original.split(START, 1)[0]
    return original.rstrip() + "\n\n" + markdown(report)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for label in LABELS:
        parser.add_argument("--" + label.replace("_", "-") + "-url", action="append", default=[])
    parser.add_argument("--summary-json")
    parser.add_argument("--validate-only", action="store_true")
    args = parser.parse_args()
    supplied = {label: getattr(args, label + "_url") for label in LABELS}
    try:
        for values in supplied.values():
            for value in values:
                endpoint(value)
    except ValueError:
        parser.error("invalid profile endpoint")
    urls = {label: values[-1] if values else None for label, values in supplied.items()}
    if args.validate_only:
        return 0
    if not args.summary_json:
        parser.error("--summary-json is required")
    nodes = {}
    failed = False
    for label, value in urls.items():
        started = now()
        status, result = fetch(value) if value is not None else (None, "not_selected")
        finished = now()
        failed |= result not in ("success", "not_selected")
        profile = adapter.project(status, "http", finished) if result == "success" else unknown(result, "unsupported" if value is None else "unknown")
        nodes[label] = refresh_node({"profile_collection": {
            "mode": "http", "result": result, "complete": result == "success",
            "started_at_unix_ms": started, "finished_at_unix_ms": finished,
            "endpoint_scope_digest": hashlib.sha256(value.encode()).hexdigest() if value else None,
        }, "profile_readiness": profile}, finished)
    Path(args.summary_json).write_text(json.dumps({"api_version": "oasis7.triad-http-profile/v1", "nodes": nodes}) + "\n")
    return 1 if failed else 0

if __name__ == "__main__":
    raise SystemExit(main())
