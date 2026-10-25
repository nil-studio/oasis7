#!/usr/bin/env python3
"""Linux-only actual-service cold entry smoke; never a model Runtime playtest.

The private fixture describes actual binary argv, not synthetic HTTP responses.
See package-ecs-cold-entry-smoke.md. No command is executed through a shell.
"""
import argparse
import importlib.util
import ipaddress
import http.client
import http.server
import json
import os
import re
from pathlib import Path
import secrets
import signal
import ssl
import subprocess
import sys
import threading
import time


# Public diagnostics accept exact known control enums, never arbitrary CLI text.
COLD_ENTRY_ENUMS = {
    "stage": frozenset(("registration", "login", "participant", "starter", "task",
                        "goal_prepare", "goal_publish", "delegation", "executor_bind",
                        "complete", "login_refresh")),
    "state": frozenset(("not_started", "pending", "confirmed", "action_required", "blocked", "unknown")),
    "next_action": frozenset((
        "explicit_fresh_login_same_operation", "explicit_fresh_login",
        "reconcile_original_login_or_explicit_fresh_login", "reconcile_original_registration",
        "query_original_participant", "read_canonical_starter_eligibility",
        "inspect_canonical_starter_eligibility", "reconcile_original_goal_prepare",
        "query_original_owner_operation", "query_original", "inspect_original_rejection",
        "verify_canonical_owner_view", "await_authorized_registration",
        "revalidate_session_and_query_original", "replay_original", "resume_original",
        "explicit_start", "login", "world_admission",
        "explicit_validated_runtime_profile_required")),
}
FAILURE_STAGES = frozenset((
    "initialization", "temporary_tls", "actual_fixture_setup", "authorized_service_startup",
    "zero_resource_discovery", "cold_entry_init", "cold_entry_response_loss_recovery",
    "committed_policy_chain_process_restart", "canonical_sdk_action",
    "completed_workflow_restart", "canonical_original_recheck"))


# Match the entire fixed probe diagnostic, never a substring of SDK/server text.
PROBE_FIXED_ERRORS = {
    "private unlock unavailable": "unlock_unavailable",
    "private unlock required": "unlock_required",
    "original plan missing": "original_plan_missing",
    "original operations missing": "original_operations_missing",
    "unexpected operation manifest": "operation_manifest_mismatch",
    "invalid operation identity": "operation_identity_invalid",
    "invalid original participant config": "participant_config_invalid",
    "canonical task unavailable": "canonical_task_unavailable",
    "participant original mismatch": "participant_original_mismatch",
    "task original mismatch": "task_original_mismatch",
    "task not active": "task_not_active",
    "canonical private goal missing": "private_goal_missing",
    "goal original mismatch": "goal_original_mismatch",
    "goal encoding failed": "goal_encoding_failed",
    "goal content mismatch": "goal_content_mismatch",
    "executor id missing": "executor_id_missing",
    "binding missing": "executor_binding_missing",
    "bind original is not canonically confirmed": "bind_original_unconfirmed",
    "SDK task binding mismatch": "sdk_task_binding_mismatch",
    "private context resource mismatch": "context_resource_mismatch",
    "original action unavailable; retain original operation": "original_action_unavailable",
    "actual action not canonically committed": "action_uncommitted",
    "post-action task not authenticated": "post_action_task_unauthenticated",
    "committed move did not update authenticated world position": "world_position_mismatch",
    "probe recovery deadline exhausted; retain original operation": "recovery_deadline",
}
for _phase in ("context_prefix", "action"):
    for _outcome in ("expired", "history_unavailable") + tuple("rejected:" + value for value in (
            "unsupported_contract", "unsupported_capability", "unauthorized", "conflict",
            "unavailable", "assurance_unavailable", "read_not_caught_up", "history_unavailable",
            "cursor_expired", "resync_required")):
        PROBE_FIXED_ERRORS[_phase + " " + _outcome + "; retain original operation"] = (
            _phase + "_" + _outcome.replace(":", "_"))
    for _category in ("configuration", "transport", "assurance", "http_rejected", "cooldown"):
        PROBE_FIXED_ERRORS[_phase + " query " + _category + "; retain original operation"] = (
            _phase + "_query_" + _category)
    for _outcome in ("unobserved", "received", "pending", "unknown"):
        PROBE_FIXED_ERRORS[_phase + " recovery deadline exhausted outcome=" + _outcome
                           + "; retain original operation"] = _phase + "_recovery_deadline_" + _outcome


def probe_error_category(output):
    if len(output) > 65536:
        return "output_limit"
    if len(output) > 256:
        return "unavailable"
    try:
        message = output.decode("ascii").rstrip("\r\n")
    except UnicodeError:
        return "unavailable"
    return PROBE_FIXED_ERRORS.get(message, "unavailable")


def safe_enum(value, allowed):
    return value if isinstance(value, str) and value in allowed else "unavailable"


class Smoke:
    def __init__(self, fixture, root, deadline):
        self.fixture, self.root, self.deadline = fixture, root, deadline
        self.children = []
        self.password = secrets.token_bytes(48)
        self.env = {key: value for key, value in os.environ.items() if not key.startswith("OASIS7_")}
        self.variables = {"root": str(root)}
        self.faults = []
        self.lock = threading.Lock()
        self.stage = "initialization"
        self.retry_not_before = 0.0
        self.attempts = 0
        self.probe_failure = None
        self.last_result = {"rc": None, **{key: "unavailable" for key in COLD_ENTRY_ENUMS}}

    def observe_retry_after(self, status, header):
        if status != 429:
            return
        # IdentityService's fixed 429 contract is Retry-After: 60. Missing or
        # malformed delta-seconds must not turn throttling into a fast retry.
        seconds = 60
        if isinstance(header, str):
            value = header.strip()
            if value and value.isascii() and value.isdecimal():
                seconds = int(value) if len(value) <= 10 else float("inf")
        with self.lock:
            self.retry_not_before = max(self.retry_not_before, time.monotonic() + seconds)

    def wait_recovery(self, backoff):
        target = time.monotonic() + backoff
        while True:
            with self.lock:
                target = max(target, self.retry_not_before)
            if target >= self.deadline:
                raise RuntimeError("cold entry retry exceeds smoke deadline")
            delay = target - time.monotonic()
            if delay <= 0:
                break
            time.sleep(min(60, delay))
        self.remaining()

    def recover_cold_entry(self, argv):
        backoff = 1
        while True:
            self.wait_recovery(0)
            self.begin_cold_entry_attempt()
            code, output = self.command(argv, private=True)
            self.record_cold_entry_result(code, output)
            if self.last_result["state"] in ("blocked", "action_required"):
                raise RuntimeError("cold entry requires explicit authorized action")
            if self.last_result["stage"] == "complete" and self.last_result["state"] == "confirmed":
                return
            self.wait_recovery(backoff)
            backoff = min(4, backoff * 2)

    def begin_cold_entry_attempt(self):
        self.attempts += 1
        self.last_result = {"rc": None, **{key: "unavailable" for key in COLD_ENTRY_ENUMS}}

    def record_cold_entry_result(self, code, output):
        parsed = {}
        if code == 0:
            try:
                value = json.loads(output)
                if isinstance(value, dict):
                    parsed = value
            except (ValueError, UnicodeError):
                pass
        self.last_result = {"rc": code if type(code) is int and -255 <= code <= 255 else None,
                            **{key: safe_enum(parsed.get(key), allowed)
                               for key, allowed in COLD_ENTRY_ENUMS.items()}}

    def write_failure_receipt(self):
        receipt = {"state": "failed", "failure_stage": safe_enum(self.stage, FAILURE_STAGES),
                   "private_details_omitted": True, "attempts": self.attempts,
                   "last_result": dict(self.last_result),
                   "response_loss": {name: len(self.faults) > index and self.faults[index].get("hit") is True
                                     for index, name in enumerate(("registration", "executor_bind"))}}
        if self.probe_failure is not None:
            receipt["probe_failure"] = dict(self.probe_failure)
        (self.root / "receipt.json").write_text(json.dumps(receipt, indent=2) + "\n")
        return receipt

    def remaining(self):
        value = self.deadline - time.monotonic()
        if value <= 0:
            raise RuntimeError("smoke deadline exceeded")
        return value

    def expand(self, value):
        if isinstance(value, str):
            for key, replacement in self.variables.items():
                value = value.replace("${" + key + "}", replacement)
            return value
        if isinstance(value, list):
            return [self.expand(item) for item in value]
        if isinstance(value, dict):
            return {key: self.expand(item) for key, item in value.items()}
        return value

    def command(self, argv, *, private=False, start=False, timeout_seconds=60):
        argv = self.expand(argv)
        if not isinstance(argv, list) or not argv or not all(isinstance(a, str) for a in argv):
            raise RuntimeError("fixture commands must be nonempty argv arrays")
        # Files can contain private response material; never echo their contents.
        log = (self.root / ("process-" + str(len(list(self.root.glob("process-*")))) + ".log")).open("xb")
        os.chmod(log.name, 0o600)
        process = subprocess.Popen(argv, stdin=subprocess.PIPE if private else subprocess.DEVNULL,
                                   stdout=log, stderr=log, env=self.env, start_new_session=True)
        self.children.append(process)
        log.close()
        if start:
            return process
        try:
            process.communicate(self.password if private else None, timeout=min(timeout_seconds, self.remaining()))
        except subprocess.TimeoutExpired:
            os.killpg(process.pid, signal.SIGKILL)
            process.wait()
            raise RuntimeError("bounded child command timed out") from None
        data = Path(log.name).read_bytes()
        return process.returncode, data

    def probe(self, name, expected):
        self.probe_failure = None
        code, output = self.command(self.fixture[name], private=name == "canonical_probe",
                                    timeout_seconds=130 if name == "canonical_probe" else 60)
        if code or len(output) > 65536:
            self.probe_failure = {
                "rc": code if type(code) is int and -255 <= code <= 255 else None,
                "output_bytes": len(output), "category": probe_error_category(output)}
            raise RuntimeError(name + " failed; inspect private process log")
        try:
            value = json.loads(output)
        except (ValueError, UnicodeError):
            raise RuntimeError(name + " must emit one JSON proof") from None
        for key, wanted in expected.items():
            if value.get(key) != wanted:
                raise RuntimeError(name + " missing required evidence: " + key)
        return value

    def close(self):
        for child in reversed(self.children):
            # A reaped leader's numeric PID may now identify another group.
            # Never poll/reap an unknown leader immediately before signalling.
            if child.returncode is not None:
                continue
            try:
                os.killpg(child.pid, signal.SIGTERM)
            except ProcessLookupError:
                pass
        until = time.monotonic() + 5
        for child in reversed(self.children):
            if child.poll() is None:
                try:
                    child.wait(timeout=max(0.01, until - time.monotonic()))
                except subprocess.TimeoutExpired:
                    os.killpg(child.pid, signal.SIGKILL)
                    child.wait()


def generate_tls(smoke):
    """Actual one-day CA and distinct server leaf; no CA-as-end-entity shortcut."""
    extension = smoke.root / "server.ext"
    extension.write_text("basicConstraints=critical,CA:FALSE\nkeyUsage=critical,digitalSignature,keyEncipherment\nextendedKeyUsage=serverAuth\nsubjectAltName=DNS:localhost,IP:127.0.0.1\n")
    os.chmod(extension, 0o600)
    commands = [
        ["openssl", "req", "-x509", "-newkey", "rsa:2048", "-nodes", "-sha256",
         "-keyout", "${root}/root.key", "-out", "${root}/root.crt", "-days", "1",
         "-subj", "/CN=Oasis7 isolated cold entry fixture CA", "-addext", "basicConstraints=critical,CA:TRUE,pathlen:0",
         "-addext", "keyUsage=critical,keyCertSign,cRLSign"],
        ["openssl", "req", "-new", "-newkey", "rsa:2048", "-nodes", "-sha256",
         "-keyout", "${root}/server.key", "-out", "${root}/server.csr", "-subj", "/CN=localhost"],
        ["openssl", "x509", "-req", "-in", "${root}/server.csr", "-CA", "${root}/root.crt",
         "-CAkey", "${root}/root.key", "-CAcreateserial", "-out", "${root}/server.crt",
         "-days", "1", "-sha256", "-extfile", "${root}/server.ext"],
        ["openssl", "verify", "-purpose", "sslserver", "-CAfile", "${root}/root.crt", "${root}/server.crt"],
    ]
    for command in commands:
        if smoke.command(command)[0]:
            raise RuntimeError("temporary CA and server leaf generation failed")
    for name in ("root.key", "server.key"):
        os.chmod(smoke.root / name, 0o600)
    smoke.env["SSL_CERT_FILE"] = str(smoke.root / "root.crt")


def trusted_forward_headers(incoming, client_address):
    """Fixture TLS termination supplies one actual peer IP, never a caller claim."""
    real_ip = str(ipaddress.ip_address(client_address))
    headers = {key: value for key, value in incoming.items()
               if key.lower() not in ("connection", "transfer-encoding", "x-real-ip", "x-forwarded-proto")}
    headers["X-Real-IP"] = real_ip
    headers["X-Forwarded-Proto"] = "https"
    return headers


def forwarder(smoke, address):
    class Handler(http.server.BaseHTTPRequestHandler):
        def setup(self):
            super().setup()
            self.connection.settimeout(15)

        def log_message(self, *_):
            pass

        def do_GET(self):
            self.forward()

        def do_POST(self):
            self.forward()

        def forward(self):
            connection = None
            try:
                length = int(self.headers.get("Content-Length", "0"))
                if not 0 <= length <= 2 * 1024 * 1024:
                    raise ValueError("bounded request required")
                body = self.rfile.read(length)
                connection = http.client.HTTPConnection(*address, timeout=min(15, smoke.remaining()))
                headers = trusted_forward_headers(self.headers, self.client_address[0])
                headers["Host"] = self.headers.get("Host", "localhost")
                connection.request(self.command, self.path, body, headers)
                response = connection.getresponse()
                smoke.observe_retry_after(response.status, response.getheader("Retry-After"))
                payload = response.read(2 * 1024 * 1024 + 1)
                if len(payload) > 2 * 1024 * 1024:
                    raise ValueError("bounded response required")
                is_bind = False
                try:
                    request = json.loads(body)
                    operation = request.get("operation", {})
                    signed = operation.get("request", {}).get("signed_payload", {})
                    is_bind = (operation.get("kind") == "submit" and signed.get("kind") == "game_executor_control"
                               and signed.get("payload", {}).get("request", {}).get("operation", {}).get("operation") == "bind")
                except (ValueError, AttributeError):
                    pass
                with smoke.lock:
                    fault = next((f for f in smoke.faults if not f["hit"] and
                                  self.path == f["path"] and self.command == "POST" and
                                  response.status == 200 and
                                  (not f.get("bind_only") or is_bind)), None)
                    if fault:
                        fault["hit"] = True
                if fault:
                    # The actual upstream completed and generated this response.
                    # Drop it without inventing or altering an authority receipt.
                    self.close_connection = True
                    self.connection.shutdown(2)
                    return
                self.send_response(response.status)
                for key, value in response.getheaders():
                    if key.lower() not in ("connection", "transfer-encoding", "content-length"):
                        self.send_header(key, value)
                self.send_header("Content-Length", str(len(payload)))
                self.send_header("Connection", "close")
                self.end_headers()
                self.wfile.write(payload)
            except (OSError, ValueError, RuntimeError, http.client.HTTPException):
                self.close_connection = True
            finally:
                if connection:
                    connection.close()
    server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Handler)
    server.daemon_threads = True
    context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
    context.load_cert_chain(smoke.root / "server.crt", smoke.root / "server.key")
    server.socket = context.wrap_socket(server.socket, server_side=True)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    return server


def read_initial_discovery(smoke, fixture):
    import urllib.request
    request = urllib.request.Request(smoke.variables["origin"] + "/api/public/hosted-account/v2/entry/discovery")
    with urllib.request.urlopen(request, context=ssl.create_default_context(cafile=str(smoke.root / "root.crt")),
                                timeout=min(3, smoke.remaining())) as response:
        payload = response.read(65537)
    if len(payload) > 65536:
        raise RuntimeError("bounded discovery exceeded")
    return json.loads(payload)


def wait_initial_resources(smoke, fixture, read_discovery):
    servers = smoke.children[-len(fixture["server_commands"]):] if fixture["server_commands"] else []

    def require_live_servers():
        if any(child.poll() is not None for child in servers):
            raise RuntimeError("actual server exited before baseline verification")

    for _ in range(60):
        smoke.remaining()
        require_live_servers()
        try:
            if "initial_prepared" in fixture:
                discovery = read_discovery(smoke, fixture)
                entry = fixture["plan_template"]["entry"]
                identity = entry["identity"]
                if (not isinstance(discovery, dict) or discovery.get("world") != entry["world"]
                        or discovery.get("world_service_public_key") != entry["trusted_world_service_key"]
                        or discovery.get("identity") != {"service_id": identity["identity_service_id"],
                            "environment_id": identity["environment_id"], "instance_epoch": entry["identity_instance_epoch"]}
                        or not isinstance(discovery.get("phase_capabilities"), dict)
                        or not all(discovery["phase_capabilities"].get(phase) is True for phase in
                            ("machine_registration", "participant_registration", "owner_control", "executor_control"))):
                    raise RuntimeError("actual entry service not ready")
                prepared = json.loads(Path(fixture["initial_prepared"]).read_text())
                initial = prepared.get("resource_counts")
                expected = {"agents": 1, "tasks": 0, "goals": 0, "grants": 0}
                if (type(initial) is not dict or set(initial) != set(expected)
                        or any(type(initial[key]) is not int or initial[key] != value for key, value in expected.items())):
                    raise RuntimeError("actual prepared world must match seeded fixture baseline")
            else:
                # External fixture contract remains an actual zero-resource probe.
                initial = smoke.probe("initial_probe", {"agents": 0, "tasks": 0, "goals": 0, "grants": 0})
        except (RuntimeError, OSError, ValueError):
            time.sleep(min(1, smoke.remaining()))
            continue
        require_live_servers()
        return initial
    raise RuntimeError("fresh actual services did not become ready")


def validate_restart_commits(before, after):
    """CLI already verified both signatures; preserve the complete history fence."""
    fields = {"world", "binding", "position", "execution_block_hash", "state_root_ref"}
    if (not isinstance(before, dict) or not isinstance(after, dict)
            or set(before) != fields or set(after) != fields
            or any(type(value.get("position")) is not int or value["position"] < 0 for value in (before, after))
            or before["world"] != after["world"] or before["binding"] != after["binding"]
            or after["position"] < before["position"]
            or (after["position"] == before["position"] and after != before)):
        raise RuntimeError("canonical chain restart changed or regressed the committed history")


def restart_committed_chain(smoke, fixture, chain_process, plan, read_discovery=read_initial_discovery):
    """Real production binary restart; same world, argv, env, vault and operations."""
    if fixture.get("restart_committed_chain") is not True:
        return None
    client = fixture["client_binary"]
    directory = plan["orchestration_directory"]
    code, raw = smoke.command([client, "cold-entry", "status", "--directory", directory], private=True)
    if code or len(raw) > 65536:
        raise RuntimeError("restart original status unavailable")
    status = json.loads(raw)
    refs = [status.get("participant_operation_id"), status.get("active_login_operation_id")]
    if not all(isinstance(value, str) and re.fullmatch(r"[A-Za-z0-9_-]{43}", value) for value in refs):
        raise RuntimeError("restart requires original participant and login references")
    config = smoke.root / "restart-owner-entry.json"
    with config.open("x") as output:
        os.chmod(config, 0o600)
        json.dump(plan["entry"], output)
    argv = [client, "--entry-config", str(config), "entry-owner-view",
            "--operation", refs[0], "--login-operation", refs[1]]
    def owner_view():
        code, raw = smoke.command(argv, private=True)
        if code or len(raw) > 65536:
            raise RuntimeError("fresh canonical restart owner view unavailable")
        view = json.loads(raw)
        if not isinstance(view, dict) or not isinstance(view.get("version"), dict):
            raise RuntimeError("invalid canonical restart owner observation")
        return view
    before = owner_view()
    if chain_process not in smoke.children or chain_process.poll() is not None:
        raise RuntimeError("original chain process unavailable before restart")
    expected_argv = smoke.expand(fixture["server_commands"][0])
    if chain_process.args != expected_argv or smoke.env != smoke.server_environment:
        raise RuntimeError("original chain argv or environment changed before restart")
    original_argv = list(chain_process.args)
    os.killpg(chain_process.pid, signal.SIGTERM)
    chain_process.wait(timeout=min(10, smoke.remaining()))
    restarted = smoke.command(original_argv, start=True)
    if restarted.args != original_argv:
        raise RuntimeError("restarted chain argv differs from original process")
    deadline = min(time.monotonic() + 45, smoke.deadline)
    while time.monotonic() < deadline:
        smoke.remaining()
        if restarted.poll() is not None:
            raise RuntimeError("production chain exited restoring committed participant policy")
        try:
            discovery = read_discovery(smoke, fixture)
            entry = plan["entry"]
            identity = entry["identity"]
            if (discovery.get("identity") != {"service_id": identity["identity_service_id"],
                    "environment_id": identity["environment_id"], "instance_epoch": entry["identity_instance_epoch"]}
                    or discovery.get("world") != entry["world"]
                    or discovery.get("world_service_public_key") != entry["trusted_world_service_key"]
                    or not all(discovery.get("phase_capabilities", {}).get(phase) is True for phase in
                        ("machine_registration", "participant_registration", "owner_control", "executor_control"))):
                raise RuntimeError("restarted policy service not ready")
            after = owner_view()  # a new official nonce/signature each attempt, never reuse old success
            validate_restart_commits(before["version"]["commit"], after["version"]["commit"])
            if (before.get("receipt") != after.get("receipt") or before.get("owner") != after.get("owner")
                    or before.get("task") != after.get("task")):
                raise RuntimeError("canonical participant or task changed across process restart")
            return {"production_chain_restarted": True, "original_argv_preserved": True,
                    "canonical_owner_read_verified": True,
                    "before_position": before["version"]["commit"]["position"],
                    "after_position": after["version"]["commit"]["position"]}
        except (RuntimeError, OSError, ValueError, KeyError):
            time.sleep(min(0.25, smoke.remaining()))
    raise RuntimeError("production chain restart canonical verification timed out")


def require_fresh_subject_paths(plan):
    paths = [plan[key] for key in ("orchestration_directory", "executor_vault_directory", "executor_state_directory", "executor_config_directory")]
    paths += [plan["entry"]["identity"]["state_directory"], plan["entry"]["owner_state_directory"]]
    if any(Path(path).exists() or Path(path).is_symlink() for path in paths):
        raise RuntimeError("cold entry subject directories must not exist before initialization")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--fixture", type=Path)
    parser.add_argument("--bundle", type=Path)
    parser.add_argument("--probe-binary", type=Path)
    parser.add_argument("--evidence-directory", required=True, type=Path)
    parser.add_argument("--timeout-seconds", type=int, default=300, metavar="30..900")
    args = parser.parse_args()
    if not 30 <= args.timeout_seconds <= 900:
        parser.error("timeout must be from 30 to 900 seconds")
    if sys.platform != "linux":
        parser.error("Linux only: temporary CA uses SSL_CERT_FILE; no system trust changes")
    if bool(args.fixture) == bool(args.bundle) or (args.bundle and not args.probe_binary):
        parser.error("provide either private --fixture or --bundle with --probe-binary")
    if args.fixture and (not args.fixture.is_absolute() or args.fixture.stat().st_mode & 0o077):
        parser.error("fixture must be an absolute private 0600 file")
    if args.bundle:
        spec = importlib.util.spec_from_file_location("ecs_cold_entry_fixture", Path(__file__).with_name("package-ecs-cold-entry-fixture.py"))
        fixture_module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(fixture_module)
        fixture = {"identity_upstream": ["127.0.0.1", fixture_module.port()]}
    else:
        fixture = json.loads(args.fixture.read_text())
    upstream = fixture.get("identity_upstream")
    if not isinstance(upstream, list) or len(upstream) != 2 or upstream[0] != "127.0.0.1" or not isinstance(upstream[1], int) or not 1 <= upstream[1] <= 65535:
        parser.error("identity upstream must be a concrete IPv4 loopback port")
    for required in ("client_binary", "setup_commands", "server_commands", "plan_template",
                     "identity_upstream", "initial_probe", "canonical_probe", "binding_response_path"):
        if not args.bundle and required not in fixture:
            parser.error("fixture missing " + required)
    if not args.bundle and not fixture["server_commands"]:
        parser.error("actual server commands are required")
    if not args.bundle and fixture["binding_response_path"] != "/api/public/hosted-account/v2/entry-owner/control":
        parser.error("binding fault requires the actual owner control route")
    os.umask(0o077)
    args.evidence_directory.mkdir(mode=0o700, parents=False, exist_ok=False)
    smoke = Smoke(fixture, args.evidence_directory.resolve(), time.monotonic() + args.timeout_seconds)
    server = None
    try:
        smoke.stage = "temporary_tls"
        generate_tls(smoke)
        server = forwarder(smoke, tuple(fixture["identity_upstream"]))
        smoke.variables["origin"] = "https://localhost:" + str(server.server_port)
        if args.bundle:
            smoke.stage = "actual_fixture_setup"
            fixture.update(fixture_module.fixture(smoke, args.bundle, str(args.probe_binary.resolve())))
            smoke.fixture = fixture
        smoke.env.update(smoke.expand(fixture.get("environment", {})))
        smoke.env["SSL_CERT_FILE"] = str(smoke.root / "root.crt")
        for relative, content in fixture.get("private_files", {}).items():
            target = smoke.root / relative
            if Path(relative).is_absolute() or ".." in Path(relative).parts:
                raise RuntimeError("fixture private files must remain beneath fresh root")
            target.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
            with target.open("x") as output:
                json.dump(smoke.expand(content), output)
            os.chmod(target, 0o600)
        for argv in fixture["setup_commands"]:
            if smoke.command(argv)[0]:
                raise RuntimeError("actual fixture setup failed; inspect private process log")
        smoke.stage = "authorized_service_startup"
        smoke.server_environment = dict(smoke.env)
        actual_servers = []
        for argv in fixture["server_commands"]:
            actual_servers.append(smoke.command(argv, start=True))
        smoke.stage = "zero_resource_discovery"
        initial = wait_initial_resources(smoke, fixture, read_initial_discovery)
        plan = smoke.expand(fixture["plan_template"])
        if "initial_prepared" in fixture:
            require_fresh_subject_paths(plan)
        path = smoke.root / "plan.json"
        path.write_text(json.dumps(plan))
        os.chmod(path, 0o600)
        client = fixture["client_binary"]
        smoke.stage = "cold_entry_init"
        if smoke.command([client, "cold-entry", "init", "--plan", str(path)], private=True)[0]:
            raise RuntimeError("actual cold entry initialization failed")
        smoke.faults = [{"path": "/api/public/hosted-account/v2/machine-registration/complete", "hit": False},
                        {"path": fixture["binding_response_path"], "hit": False, "bind_only": True}]
        directory = plan["orchestration_directory"]
        smoke.stage = "cold_entry_response_loss_recovery"
        smoke.recover_cold_entry([client, "cold-entry", "run", "--directory", directory])
        if not all(f["hit"] for f in smoke.faults):
            raise RuntimeError("cold entry required real response-loss recovery incomplete")
        smoke.stage = "committed_policy_chain_process_restart"
        restart_evidence = restart_committed_chain(smoke, fixture, actual_servers[0], plan)
        smoke.stage = "canonical_sdk_action"
        before = smoke.probe("canonical_probe", {"canonical_verified": True, "agents": 1, "tasks": 1,
                            "goals": 1, "active_executors": 1, "sdk_task_context_verified": True,
                            "action_committed": True})
        smoke.stage = "completed_workflow_restart"
        smoke.recover_cold_entry([client, "cold-entry", "run", "--directory", directory])
        smoke.stage = "canonical_original_recheck"
        after = smoke.probe("canonical_probe", {"canonical_verified": True, "agents": 1, "tasks": 1,
                           "goals": 1, "active_executors": 1, "sdk_task_context_verified": True,
                           "action_committed": True})
        for key in ("participant_operation_id", "executor_operation_id", "task_id", "agent_id"):
            if not before.get(key) or before.get(key) != after.get(key):
                raise RuntimeError("canonical original identity changed on restart: " + key)
        receipt = {"state": "passed", "kind": "actual_service_integration_fixture",
                   "model_runtime_playtest": False, "response_losses_recovered": 2,
                   "canonical_originals_unchanged": True, "committed_policy_restart": restart_evidence}
        (smoke.root / "receipt.json").write_text(json.dumps(receipt, indent=2) + "\n")
        smoke.stage = "passed"
        print(json.dumps(receipt))
    finally:
        try:
            if smoke.stage != "passed":
                print(json.dumps(smoke.write_failure_receipt()), file=sys.stderr)
        finally:
            try:
                if server:
                    try:
                        server.shutdown()
                    finally:
                        server.server_close()
            finally:
                smoke.close()


if __name__ == "__main__":
    main()
