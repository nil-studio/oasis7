#!/usr/bin/env python3
"""Explicit bounded HTTP profile child and read-time diagnostic projection.

The enclosing observability attempt owns ordering/publication. No raw producer
DTO, endpoint, credentials or error body is saved in the report.
"""
import argparse
import asyncio
import hashlib
import http.client
import importlib.util
import json
import os
import re
from pathlib import Path
import stat
import subprocess
import sys
import time
import unicodedata
from urllib.parse import urlsplit

_spec = importlib.util.spec_from_file_location("http_profile_adapter", Path(__file__).with_name("p2p-profile-readiness.py"))
adapter = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(adapter)
LABELS = ("local_node", "sequencer_ecs", "storage_ecs")
CAP = 4 * 1024 * 1024
TIMEOUT = 5
CONTROLLED_CAP = 16 * 1024
CONTROLLED_TIMEOUT = 6
CONTROLLED_OWNER_TIMEOUT = 12
CONTROLLED_OUTPUT_CAP = 64 * 1024
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


def decode_object(pairs):
    result = {}
    for name, value in pairs:
        if name in result:
            raise ValueError("duplicate JSON field")
        result[name] = value
    return result


def hex_digest(value):
    return isinstance(value, str) and re.fullmatch(r"[0-9a-fA-F]{64}", value) is not None


def valid_pins(pins):
    required = {"status_url", "service_public_key", "expected_world", "expected_node_id"}
    if not isinstance(pins, dict) or set(pins) - {"owner_config"} != required:
        return False
    if "owner_config" in pins:
        owner = pins["owner_config"]
        if not isinstance(owner, str) or not Path(owner).is_absolute():
            return False
    if not isinstance(pins["status_url"], str):
        return False
    world = pins["expected_world"]
    node = adapter.text(pins["expected_node_id"])
    address = pins["status_url"]
    url = endpoint(address)
    return (url.path == "/v1/chain/status" and not any(delimiter in address for delimiter in ("?", "#"))
            and isinstance(world, dict) and set(world) == {"world_id", "genesis_digest"}
            and adapter.text(world["world_id"]) is not None and hex_digest(world["genesis_digest"])
            and hex_digest(pins["service_public_key"]) and node is not None
            and len(node.encode("utf-8")) <= 256 and all(unicodedata.category(c) != "Cc" for c in node))


def controlled_inventory(path):
    descriptor = os.open(path, os.O_RDONLY | getattr(os, "O_NONBLOCK", 0) | getattr(os, "O_NOFOLLOW", 0))
    with os.fdopen(descriptor, "rb") as source:
        if not stat.S_ISREG(os.fstat(source.fileno()).st_mode):
            raise ValueError("controlled status inventory is not a regular file")
        data = source.read(CONTROLLED_CAP + 1)
    if len(data) > CONTROLLED_CAP:
        raise ValueError("controlled status inventory exceeds bound")
    inventory = json.loads(data.decode("utf-8"), object_pairs_hook=decode_object)
    if (not isinstance(inventory, dict) or set(inventory) != {"api_version", "runtime_bin", "slots"}
            or inventory["api_version"] != "oasis7.controlled-status-inventory/v1"
            or not isinstance(inventory["runtime_bin"], str)):
        raise ValueError("invalid controlled inventory schema")
    runtime = Path(inventory["runtime_bin"])
    slots = inventory["slots"]
    if (not runtime.is_absolute() or not runtime.is_file() or not os.access(runtime, os.X_OK)
            or not isinstance(slots, dict) or not slots or not set(slots) <= set(LABELS)
            or not all(valid_pins(pins) for pins in slots.values())):
        raise ValueError("invalid controlled profile pins")
    return runtime, slots, hashlib.sha256(data).hexdigest()


def project_owner_pair(value, position, record, received):
    if not isinstance(value, dict):
        return {"status": "unavailable"}
    state = value.get("status")
    if state in ("unavailable", "identity_mismatch", "cut_mismatch"):
        return {"status": state}
    started = adapter.uint(value.get("sample_started_at_unix_ms"))
    matches = value.get("both_heads_match_unchanged_cut")
    finalizer = value.get("finalizing_authority_id")
    if (state != "observed" or started is None or started > received
            or position is None or not hex_digest(finalizer) or type(matches) is not bool):
        return {"status": "invalid_observation"}
    result = dict(status="observed", sample_started_at_unix_ms=started,
                  finalizing_authority_id=finalizer, both_heads_match_unchanged_cut=matches)
    for role in ("primary", "replica"):
        endpoint = value.get(role)
        if not isinstance(endpoint, dict):
            return {"status": "invalid_observation"}
        if endpoint.get("status") == "unavailable" and not matches:
            result[role] = {"status": "unavailable"}
            continue
        head = endpoint.get("head")
        if (endpoint.get("status") != "authenticated" or not isinstance(head, dict)
                or adapter.uint(head.get("position")) is None or not hex_digest(head.get("record_id"))
                or not hex_digest(endpoint.get("instance_digest"))
                or any(adapter.uint(endpoint.get(key)) is None for key in ("checkpoint_revision", "epoch_floor"))
                or any(type(endpoint.get(key)) is not bool for key in ("sealed", "native_request_pending"))
                or (matches and (head["position"] != position or head["record_id"] != record))):
            return {"status": "invalid_observation"}
        result[role] = dict(
            status="authenticated", head={"position": head["position"], "record_id": head["record_id"]},
            **{key: endpoint[key] for key in (
                "instance_digest", "checkpoint_revision", "epoch_floor", "sealed", "native_request_pending"
            )}
        )
    return result



async def _bounded_controlled_output(args, payload):
    """Concurrent stdin/output, with a hard parent-side output and time cap."""
    process = await asyncio.create_subprocess_exec(
        *args, stdin=asyncio.subprocess.PIPE, stdout=asyncio.subprocess.PIPE,
        stderr=asyncio.subprocess.DEVNULL, limit=CONTROLLED_OUTPUT_CAP,
    )

    async def send_request():
        try:
            process.stdin.write(payload)
            await process.stdin.drain()
        except (BrokenPipeError, ConnectionResetError):
            pass
        finally:
            process.stdin.close()

    sender = asyncio.create_task(send_request())
    try:
        output = bytearray()
        while True:
            # Read at most one byte beyond the cap, even if the child floods
            # stdout. asyncio pauses its pipe transport at the bounded limit.
            chunk = await process.stdout.read(min(8192, CONTROLLED_OUTPUT_CAP + 1 - len(output)))
            if not chunk:
                break
            output.extend(chunk)
            if len(output) > CONTROLLED_OUTPUT_CAP:
                return None, "output_limit"
        await sender
        if await process.wait() != 0:
            return None, "controlled_observation_failed"
        return bytes(output), "success"
    finally:
        sender.cancel()
        if process.returncode is None:
            process.kill()
        # A StreamReader may pause its subprocess pipe at the high-water
        # mark. Drain and discard after killing the exact child so wait()
        # observes EOF; retaining the buffered data would defeat the cap.
        while await process.stdout.read(8192):
            pass
        await process.wait()
        await asyncio.gather(sender, return_exceptions=True)


def run_controlled_bounded(args, payload, timeout):
    try:
        return asyncio.run(asyncio.wait_for(_bounded_controlled_output(args, payload), timeout))
    except asyncio.TimeoutError:
        return None, "timeout"
    except OSError:
        return None, "invalid_status"


def observe_controlled(runtime, pins):
    try:
        # The selected verifier is the operator's trusted shipping binary. Pins
        # go through stdin and raw stdout/error are never published.
        args = [str(runtime), "op", "formal-status-observe", "--request", "-"]
        owner = pins.get("owner_config")
        if owner is not None:
            args += ["--owner-config", owner]
        status_pins = {key: value for key, value in pins.items() if key != "owner_config"}
        output, result = run_controlled_bounded(
            args, json.dumps(status_pins).encode("utf-8"),
            CONTROLLED_OWNER_TIMEOUT if owner is not None else CONTROLLED_TIMEOUT,
        )
        if result != "success":
            return None, result
        status = json.loads(output.decode("utf-8"), object_pairs_hook=decode_object)
    except (OSError, ValueError, UnicodeError):
        return None, "invalid_status"
    if (not isinstance(status, dict)
            or status.get("api_version") != "oasis7.formal-live-status-observation/v1"
            or status.get("observation_scope") != "authenticated_host_diagnostics_only"
            or status.get("authority_qualification") != "unknown"
            or status.get("node_id") != pins["expected_node_id"] or status.get("world") != pins["expected_world"]
            or status.get("host_state") not in ("starting", "stopped", "degraded", "ready", "stopping")
            or any(type(status.get(key)) is not bool for key in
                   ("node_running", "host_reports_ready", "runtime_error_present"))
            or status["host_reports_ready"] != (status["host_state"] == "ready")
            or (status["host_reports_ready"] and not status["node_running"])):
        return None, "invalid_status"
    position, record = status.get("committed_position"), status.get("committed_record_id")
    if ((position is None) != (record is None)
            or (position is not None and (adapter.uint(position) is None or not hex_digest(record)))
            or (status["host_reports_ready"] and position is None)):
        return None, "invalid_status"
    received = now()
    profile = unknown("controlled_authority_qualification_unobserved")
    profile.update(source="verified_controlled_status", runtime_profile="formal_live_v1",
                   observation_scope="authenticated_host_diagnostics_only",
                   authority_qualification="unknown", protected_services_readiness="unknown",
                   sample_time_basis="local_authenticated_receipt", observed_at_unix_ms=received,
                   world_id=pins["expected_world"]["world_id"], genesis_digest=pins["expected_world"]["genesis_digest"],
                   node_id=pins["expected_node_id"],
                   committed_position=position, committed_record_id=record,
                   **{key: status[key] for key in
                      ("host_state", "node_running", "host_reports_ready", "runtime_error_present")})
    if owner is not None:
        profile["protected_pair"] = project_owner_pair(
            status.get("protected_pair"), position, record, received,
        )
    return profile, "success"


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
        # read1() permits EOF before the declared Content-Length and does not
        # raise IncompleteRead like read() without an amount. A parseable JSON
        # prefix is not a complete HTTP observation.
        if response.length not in (None, 0):
            return None, "invalid_status"
        # Do not let a duplicate producer field silently shadow identity,
        # time, readiness, or BFT checkpoint facts.
        status = json.loads(chunks.decode("utf-8"), object_pairs_hook=decode_object)
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
    pair = profile.get("protected_pair")
    if isinstance(pair, dict) and pair.get("status") == "observed":
        started = adapter.uint(pair.get("sample_started_at_unix_ms"))
        age = as_of - started if started is not None and started <= as_of else None
        profile["protected_pair"] = dict(
            pair, age_ms=age,
            freshness="unknown" if age is None else "fresh" if age <= adapter.MAX_AGE_MS else "stale",
        )
    observed = profile.get("observed_at_unix_ms")
    valid_observed = type(observed) is int and 0 <= observed <= as_of
    profile.update(as_of_unix_ms=as_of, age_ms=as_of - observed if valid_observed else None)
    runtime_profile = profile.get("runtime_profile")
    freshness_scope = ({"bft/version=1": "bft", "formal_live_v1": "controlled_host"}.get(runtime_profile)
                       if isinstance(runtime_profile, str) else None)
    if freshness_scope:
        profile["source_freshness"] = ("unknown" if not valid_observed else
            "fresh" if as_of-observed <= adapter.MAX_AGE_MS else "stale")
        if not valid_observed:
            # A null timestamp carries the adapter's more specific failure reason.
            profile["status"] = "unknown"
            if observed is not None:
                profile["reason"] = freshness_scope + "_observation_clock_uncertain"
        elif as_of-observed > adapter.MAX_AGE_MS:
            profile.update(status="stale", reason=freshness_scope + "_observation_expired")
        elif (collection.get("result") != "success" or collection.get("complete") is not True
              or collection["freshness"] != "fresh"):
            profile.update(status="unknown", reason="collection_unavailable")
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
    parser.add_argument("--controlled-inventory")
    parser.add_argument("--expected-controlled-inventory-digest")
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
    runtime, controlled, digest = None, {}, None
    inventory_failed = False
    if args.controlled_inventory:
        try:
            runtime, controlled, digest = controlled_inventory(args.controlled_inventory)
            if any(urls[label] is not None for label in controlled):
                raise ValueError("controlled and HTTP profile inputs share a slot")
            if not args.validate_only and digest != args.expected_controlled_inventory_digest:
                raise ValueError("controlled profile inventory changed after preflight")
        except (OSError, ValueError, UnicodeError):
            if args.validate_only:
                parser.error("invalid controlled status inventory or conflicting profile slot")
            controlled = {}
            inventory_failed = True
    if args.validate_only:
        if digest is not None:
            print(digest)
        return 0
    if not args.summary_json:
        parser.error("--summary-json is required")
    nodes = {}
    failed = inventory_failed
    for label, value in urls.items():
        started = now()
        mode = "http"
        scope = hashlib.sha256(value.encode()).hexdigest() if value else None
        if label in controlled:
            mode = "controlled_verified_http"
            pins = controlled[label]
            scope = hashlib.sha256(json.dumps(pins, sort_keys=True, separators=(",", ":")).encode("utf-8")).hexdigest()
            profile, result = observe_controlled(runtime, pins)
        elif value is not None:
            status, result = fetch(value)
            profile = adapter.project(status, "http", now()) if result == "success" else None
        else:
            result = "controlled_inventory_unavailable" if inventory_failed else "not_selected"
            profile = None
        finished = now()
        failed |= result not in ("success", "not_selected")
        if profile is None:
            profile = unknown(result, "unsupported" if result == "not_selected" else "unknown")
        nodes[label] = refresh_node({"profile_collection": {
            "mode": mode, "result": result, "complete": result == "success",
            "started_at_unix_ms": started, "finished_at_unix_ms": finished,
            "endpoint_scope_digest": scope,
        }, "profile_readiness": profile}, finished)
    Path(args.summary_json).write_text(json.dumps({"api_version": "oasis7.triad-http-profile/v1", "nodes": nodes}) + "\n")
    return 1 if failed else 0

if __name__ == "__main__":
    raise SystemExit(main())
