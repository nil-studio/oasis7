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
from pathlib import Path
import secrets
import signal
import ssl
import subprocess
import sys
import threading
import time


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

    def command(self, argv, *, private=False, start=False):
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
            process.communicate(self.password if private else None, timeout=min(60, self.remaining()))
        except subprocess.TimeoutExpired:
            os.killpg(process.pid, signal.SIGKILL)
            process.wait()
            raise RuntimeError("bounded child command timed out") from None
        data = Path(log.name).read_bytes()
        return process.returncode, data

    def probe(self, name, expected):
        code, output = self.command(self.fixture[name], private=name == "canonical_probe")
        if code or len(output) > 65536:
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
        for argv in fixture["server_commands"]:
            smoke.command(argv, start=True)
        smoke.stage = "zero_resource_discovery"
        initial = None
        for _ in range(30):
            try:
                if "initial_prepared" in fixture:
                    import urllib.request
                    request = urllib.request.Request(smoke.variables["origin"] + "/api/public/hosted-account/v2/entry/discovery")
                    with urllib.request.urlopen(request, context=ssl.create_default_context(cafile=str(smoke.root / "root.crt")), timeout=3) as response:
                        payload = response.read(65537)
                        if len(payload) > 65536:
                            raise RuntimeError("bounded discovery exceeded")
                        discovery = json.loads(payload)
                    if discovery.get("world") != fixture["plan_template"]["entry"]["world"] or not all(discovery.get("phase_capabilities", {}).get(phase) for phase in ("machine_registration", "participant_registration", "owner_control", "executor_control")):
                        raise RuntimeError("actual entry service not ready")
                    prepared = json.loads(Path(fixture["initial_prepared"]).read_text())
                    initial = prepared.get("resource_counts")
                    if initial != {"agents": 0, "tasks": 0, "goals": 0, "grants": 0}:
                        raise RuntimeError("actual prepared world must contain zero entry resources")
                else:
                    initial = smoke.probe("initial_probe", {"agents": 0, "tasks": 0, "goals": 0, "grants": 0})
                break
            except (RuntimeError, OSError, ValueError):
                time.sleep(min(0.5, smoke.remaining()))
        if any(child.poll() is not None for child in smoke.children[-len(fixture["server_commands"]):]):
            raise RuntimeError("actual server exited before baseline verification")
        if initial is None:
            raise RuntimeError("fresh actual services did not become ready")
        plan = smoke.expand(fixture["plan_template"])
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
        completed = False
        smoke.stage = "cold_entry_response_loss_recovery"
        for _ in range(30):
            code, output = smoke.command([client, "cold-entry", "run", "--directory", directory], private=True)
            if not code:
                try:
                    completed = json.loads(output).get("stage") == "complete"
                except ValueError:
                    pass
            if completed:
                break
            time.sleep(min(0.2, smoke.remaining()))
        if not completed or not all(f["hit"] for f in smoke.faults):
            raise RuntimeError("cold entry or required real response-loss recovery incomplete")
        smoke.stage = "canonical_sdk_action"
        before = smoke.probe("canonical_probe", {"canonical_verified": True, "agents": 1, "tasks": 1,
                            "goals": 1, "active_executors": 1, "sdk_task_context_verified": True,
                            "action_committed": True})
        smoke.stage = "completed_workflow_restart"
        if smoke.command([client, "cold-entry", "run", "--directory", directory], private=True)[0]:
            raise RuntimeError("completed workflow restart failed")
        smoke.stage = "canonical_original_recheck"
        after = smoke.probe("canonical_probe", {"canonical_verified": True, "agents": 1, "tasks": 1,
                           "goals": 1, "active_executors": 1, "sdk_task_context_verified": True,
                           "action_committed": True})
        for key in ("participant_operation_id", "executor_operation_id", "task_id", "agent_id"):
            if not before.get(key) or before.get(key) != after.get(key):
                raise RuntimeError("canonical original identity changed on restart: " + key)
        receipt = {"state": "passed", "kind": "actual_service_integration_fixture",
                   "model_runtime_playtest": False, "response_losses_recovered": 2,
                   "canonical_originals_unchanged": True}
        (smoke.root / "receipt.json").write_text(json.dumps(receipt, indent=2) + "\n")
        smoke.stage = "passed"
        print(json.dumps(receipt))
    finally:
        if smoke.stage != "passed":
            print(json.dumps({"state": "failed", "failure_stage": smoke.stage, "private_details_omitted": True}), file=sys.stderr)
        if server:
            server.shutdown()
            server.server_close()
        smoke.close()


if __name__ == "__main__":
    main()
