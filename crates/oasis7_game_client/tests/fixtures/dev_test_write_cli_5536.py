"""Explicitly authorized bounded A/B native CLI write fixture; never runs on import.

Uses the pre-existing private Linux fixture, no model credentials or world reset.
Requires the separate canonical observer and its trusted Python dependencies.
"""
import base64
import hashlib
import json
import os
from pathlib import Path
import secrets
import socket
import subprocess
import sys
import time


def main():
    assert len(sys.argv) == 4 and sys.argv[3] == "--execute-authorized"
    root, binary = Path(sys.argv[1]), sys.argv[2]
    prepared = json.loads((root / "read-5535-result.json").read_text())["accounts"]
    work = root / "write-service-5536"
    # Never reuse a prior fixture's operation namespace automatically.
    resuming = work.exists()
    work.mkdir(mode=0o700, exist_ok=True)
    (work / "usage").mkdir(mode=0o700, exist_ok=True)
    now, token = int(time.time()), secrets.token_hex(32)
    denied_token = secrets.token_hex(32)
    catalog = work / "catalog"
    catalog.mkdir(mode=0o700, exist_ok=True)
    all_records = json.loads((root / "catalog/catalog.json").read_text())
    account_refs = {a["account_ref"] for a in prepared}
    records = [a for a in all_records if a["account_ref"] in account_refs]
    assert len(records) == 2
    if not resuming:
        (catalog / "catalog.json").write_text(json.dumps(records))
    (catalog / "catalog.json").chmod(0o600)
    config = {
        "service": {"profile": "dev-test-accounts-v1", "socket": str(work / "socket"),
                    "usage_directory": str(work / "usage"), "grant_id": "dev-test-5536-native-v1",
                    "max_request_bytes": 8192, "timeout_ms": 30000, "max_calls": 64,
                    "credentials": [{"caller_id": "bounded-write-tester", "token": token,
                                     "expires_at_unix": now + 600, "revoked": False},
                                    {"caller_id": "ungranted-tester", "token": denied_token,
                                     "expires_at_unix": now + 600, "revoked": False}]},
        "catalog_directory": str(catalog), "grants": [], "resources": {},
    }
    for label, account in zip(["A", "B"], prepared):
        fence, resource = account["fence"], label + "-write"
        binding = {"account_ref": account["account_ref"], "environment": account["environment"],
                   "world": fence["world_id"], "genesis_digest": fence["genesis_digest"],
                   "agent_id": fence["agent_id"], "resource": resource,
                   "executor_config_digest": account["executor_config_digest"], "authorization_version": 1}
        config["grants"].append({"grant_ref": resource, "grant_id": config["service"]["grant_id"],
                                 "caller_id": "bounded-write-tester", "binding": binding,
                                 "expires_at_unix": now + 540, "enabled": True,
                                 "methods": ["describe", "task", "capabilities", "context", "move", "lookup"],
                                 "move_targets": [[1, 0, 0]]})
        config["resources"][resource] = {"account_ref": account["account_ref"],
                                          "executor_config": account["executor_config"]}
    policy = work / "policy.json"
    previous_steps = []
    if resuming:
        # Only a pre-context harness failure may resume; preserve the original budget.
        previous = json.loads((root / "write-5536-result.json").read_text())
        assert not any(key.endswith("_context") or key.endswith("_move") for key in previous)
        for path in (work / "usage").glob("*.json"):
            assert not json.loads(path.read_text()).get("operations"), "original_exists_manual_reconcile_only"
        config = json.loads(policy.read_text())
        token, denied_token = [c["token"] for c in config["service"]["credentials"]]
        now = config["service"]["credentials"][0]["expires_at_unix"] - 600
        assert int(time.time()) < now + 360, "existing_authorization_too_short_stop"
        previous_steps = previous["steps"]
    else:
        policy.write_text(json.dumps(config))
    policy.chmod(0o600)
    unlock = base64.b64encode((root / "unlock.bin").read_bytes()).decode("ascii")
    unlocks = {a["account_ref"]: unlock for a in prepared}
    env = os.environ.copy()
    env["SSL_CERT_FILE"] = json.loads((root / "runtime-private.json").read_text())["environment"]["SSL_CERT_FILE"]
    env["PYTHONPATH"] = "/tmp/oasis7-5536-observer"
    evidence = {"head": "4273a7239adb60b0bcf5cae181ea10660646d039", "real_external_agent": False,
                "max_calls": 64, "target": [1, 0, 0], "steps": [], "prior_pre_context_attempt_steps": previous_steps,
                "run_input": {"source": "4273a7239adb60b0bcf5cae181ea10660646d039", "dependency_source": "7193eb838ba088c0d0e3e138770c50d5755317a9",
                              "target": "aarch64-unknown-linux-gnu", "max_inflight": 1, "timeout_ms": 30000,
                              "max_request_bytes": 8192, "credential_expires_at_unix": now + 600,
                              "grant_expires_at_unix": now + 540, "session_lifetime_seconds": 180,
                              "binary_sha256": hashlib.sha256(Path(binary).read_bytes()).hexdigest(),
                              "fixture_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
                              "observer_sha256": hashlib.sha256(Path("/tmp/oasis7-5536-observer/world_observer.py").read_bytes()).hexdigest(),
                              "policy_sha256": hashlib.sha256(policy.read_bytes()).hexdigest(),
                              "account_bindings": [g["binding"] for g in config["grants"]]}}

    def observer():
        result = subprocess.run(["python3", "/tmp/oasis7-5536-observer/world_observer.py", str(root)],
                                env=env, capture_output=True, timeout=20, check=True)
        return json.loads(result.stdout)

    def persist():
        output = root / "write-5536-result.json"
        output.write_text(json.dumps(evidence))
        output.chmod(0o600)

    def start():
        assert not Path(config["service"]["socket"]).exists()
        proc = subprocess.Popen([binary, "--serve", str(policy)], stdin=subprocess.PIPE,
                                stderr=(work / "daemon.log").open("ab"), env=env)
        proc.stdin.write(json.dumps(unlocks).encode())
        proc.stdin.close()
        until = time.monotonic() + 10
        while not Path(config["service"]["socket"]).exists():
            assert proc.poll() is None, "daemon_exited"
            assert time.monotonic() < until, "daemon_start_deadline"
            time.sleep(0.05)
        return proc

    def stop(proc):
        proc.terminate()
        proc.wait(timeout=30)
        Path(config["service"]["socket"]).unlink(missing_ok=True)

    def request(method, session=None, grant=None, operation=None):
        return {"credential": token, "method": method, "session_ref": session, "grant_ref": grant,
                "expires_at_unix": int(time.time()) + 180 if method == "open" else None,
                "operation": operation}

    def call(method, session=None, grant=None, operation=None):
        result = subprocess.run([binary, "--call", config["service"]["socket"]],
                                input=json.dumps(request(method, session, grant, operation)).encode(),
                                capture_output=True, env=env, timeout=35, check=True)
        return json.loads(result.stdout)

    def ok(response, step):
        assert response["status"] == "ok", (step, response["status"])
        evidence["steps"].append(step)
        persist()
        return response["result"]

    def operation(rid, kind, **kwargs):
        return {"request_id": rid, "arguments": {"kind": kind, **kwargs}}

    def committed(value):
        native = value.get("operation", value)
        assert native["outcome"]["status"] == "committed", "original_not_committed_stop"
        return native

    baseline = observer()
    assert baseline["payload_hashes_verified"] and not baseline["active_modules"]
    assert all(pos == {"x_cm": 0, "y_cm": 0, "z_cm": 0} for pos in baseline["positions"].values())
    assert not baseline["movement_events"], "existing_movement_requires_manual_reconciliation"
    evidence["before"] = baseline
    persist()
    proc = start()
    try:
        refs = [ok(call("open", grant=label + "-write"), "open_" + label)["session_ref"] for label in ["A", "B"]]
        denied = request("move", refs[0], operation=operation("denied-move", "move", context_request_id="A-context", to=[1, 0, 0]))
        denied["credential"] = denied_token
        out = subprocess.run([binary, "--call", config["service"]["socket"]], input=json.dumps(denied).encode(), capture_output=True, env=env, timeout=35, check=True)
        assert json.loads(out.stdout)["status"] == "grant_unavailable"
        assert observer()["movement_events"] == baseline["movement_events"]
        evidence["steps"].append("ungranted_caller_move_rejected_without_effect")
        for index, (label, ref) in enumerate(zip(["A", "B"], refs)):
            context = ok(call("context", ref, operation=operation(label + "-context", "context")), "context_" + label)
            for attempt in range(1, 4):
                if "context" in context.get("observation", {}):
                    break
                # A new lookup invocation is counted; the original context ID never changes.
                time.sleep(2)
                context = ok(call("lookup", ref, operation=operation(label + "-context-proof-" + str(attempt), "lookup", original_request_id=label + "-context")), "context_original_lookup_" + label + str(attempt))
            assert "context" in context.get("observation", {}), "context_not_verified_stop"
            authority = context["observation"]["native_authority"]
            assert authority["authority_mode"] == "native_executor_v1"
            assert authority["fence"] == prepared[index]["fence"]
            assert not authority["signer_delegation"]["revoked"]
            assert authority["logical_tick"] < authority["valid_until_tick"]
            committed(context["observation"])
            evidence[label + "_context"] = context
            persist()
        move_a = operation("A-move", "move", context_request_id="A-context", to=[1, 0, 0])
        a = ok(call("move", refs[0], operation=move_a), "move_A")
        evidence["A_first_response"] = a
        persist()
        for attempt in range(1, 4):
            observation = a.get("observation", {})
            if observation.get("outcome", {}).get("status") != "unknown":
                break
            time.sleep(2)
            a = ok(call("lookup", refs[0], operation=operation("A-move-result-" + str(attempt), "lookup", original_request_id="A-move")), "move_A_original_lookup_" + str(attempt))
        evidence["A_move"] = committed(a["observation"])
        persist()
        move_b = operation("B-move", "move", context_request_id="B-context", to=[1, 0, 0])
        # Drop only the tool response, after delivering the original tool request.
        with socket.socket(socket.AF_UNIX) as connection:
            connection.settimeout(5)
            connection.connect(config["service"]["socket"])
            connection.sendall(json.dumps(request("move", refs[1], operation=move_b)).encode())
            connection.shutdown(socket.SHUT_WR)
        until = time.monotonic() + 35
        while True:
            state = observer()
            if len(state["movement_events"]) == 2:
                break
            assert len(state["movement_events"]) < 2 and time.monotonic() < until, "lost_reply_original_not_observed_stop"
            time.sleep(0.1)
        evidence["after_lost_reply"] = state
        persist()
        # Management changes only the fixture's own catalog, while B has no tool response.
        before_usage = {p.name: p.read_bytes() for p in (work / "usage").glob("*.json")}
        policy_bytes = policy.read_bytes()
        native = "/work/target/aarch64-unknown-linux-gnu/debug/oasis7_game_client"
        for record, alias in [(records[0], "renamed-during-original-recovery"),
                              (next(r for r in all_records if r["account_ref"] not in account_refs), "C")]:
            management = subprocess.run([native, "dev-test-catalog-register", "--directory", str(catalog),
                                         "--identity-config", record["credential_ref"], "--operation", record["login_operation"],
                                         "--alias", alias], input=(root / "unlock.bin").read_bytes(), capture_output=True, env=env, timeout=35)
            assert management.returncode == 0, "catalog_management_failed_stop"
        assert policy.read_bytes() == policy_bytes
        assert before_usage == {p.name: p.read_bytes() for p in (work / "usage").glob("*.json")}
        evidence["steps"].append("alias_and_C_registration_preserve_pending_original_and_usage")
        persist()
    finally:
        stop(proc)
    proc = start()
    try:
        assert call("task", refs[1])["status"] != "ok"
        bnew = ok(call("open", grant="B-write"), "restart_B_new_session")["session_ref"]
        lookup = ok(call("lookup", bnew, operation=operation("B-lookup", "lookup", original_request_id="B-move")), "recover_B_original")
        replay = ok(call("move", bnew, operation=move_b), "replay_B_same_request")
        recovered = committed(lookup["observation"])
        assert recovered == committed(replay["observation"])
        assert lookup["native_context"] == replay["native_context"] == evidence["B_context"]["native_context"]
        conflict = operation("B-move", "move", context_request_id="different-context", to=[1, 0, 0])
        assert call("move", bnew, operation=conflict)["status"] == "request_body_conflict"
        competing = operation("B-second-move", "move", context_request_id="B-context", to=[1, 0, 0])
        assert call("move", bnew, operation=competing)["status"] == "context_action_already_reserved"
        assert call("lookup", bnew, operation=operation("B-foreign", "lookup", original_request_id="A-move"))["status"] == "original_scope_denied"
        evidence["B_recovered"] = recovered
        evidence["after"] = observer()
        assert len(evidence["after"]["movement_events"]) == 2
        for label, account, native in zip(["A", "B"], prepared, [evidence["A_move"], recovered]):
            identity = evidence[label + "_context"]["observation"]["context"]
            record = native["outcome"]["receipt"]["commit_record"]
            assert record["agent_id"] == account["fence"]["agent_id"]
            assert record["decision_request_id"] == identity["decision_request_id"]
            assert record["request_digest"] == identity["request_commitment"]
            assert native["correlation"]["key"]["verified_subject"] == account["fence"]["executor_binding"]["executor_public_key"]
            action_id = int(record["action_id"].removeprefix("action:"))
            events = [e for e in evidence["after"]["movement_events"] if e["caused_by"] == {"type": "Action", "data": action_id}]
            assert len(events) == 1
            moved = events[0]["body"]["payload"]["data"]
            assert moved["agent_id"] == record["agent_id"]
            assert moved["from"] == {"x_cm": 0, "y_cm": 0, "z_cm": 0}
            assert moved["to"] == {"x_cm": 1, "y_cm": 0, "z_cm": 0}
        assert all(pos == {"x_cm": 1, "y_cm": 0, "z_cm": 0} for pos in evidence["after"]["positions"].values())
        evidence["persistent_tool_calls"] = sum(json.loads(path.read_text())["used"] for path in (work / "usage").glob("*.json"))
        evidence["state"] = "passed"
        persist()
        print(json.dumps({"state": "passed", "steps": evidence["steps"], "persistent_tool_calls": evidence["persistent_tool_calls"], "real_external_agent": False}))
    finally:
        stop(proc)


if __name__ == "__main__":
    main()
