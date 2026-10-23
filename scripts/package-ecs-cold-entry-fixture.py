"""Concrete isolated actual-binary DevLocal fixture for package-ecs-cold-entry-smoke."""
import http.client
import json
import os
from pathlib import Path
import secrets
import signal
import socket
import ssl
import time


def write(root, name, value):
    path = root / name
    with path.open("x") as output:
        json.dump(value, output)
    os.chmod(path, 0o600)
    return str(path)


def port():
    with socket.socket() as listener:
        listener.bind(("127.0.0.1", 0))
        return listener.getsockname()[1]


def launcher_transport_args(identity_http_port, runtime_tcp_port, runtime_websocket_port):
    """Identity HTTP uses viewer-port; web-bind belongs to runtime WebSocket."""
    ports = (identity_http_port, runtime_tcp_port, runtime_websocket_port)
    if len(set(ports)) != 3 or any(not isinstance(value, int) or not 1 <= value <= 65535 for value in ports):
        raise ValueError("distinct actual fixture transport ports required")
    return ["--live-bind", "127.0.0.1:" + str(runtime_tcp_port),
            "--web-bind", "127.0.0.1:" + str(runtime_websocket_port),
            "--viewer-host", "127.0.0.1", "--viewer-port", str(identity_http_port)]


def hosted_startup_isolation(root):
    """Independent fixture-only legacy issuer; never a participant policy key."""
    issuer = root / "hosted-registration-issuer.key"
    with issuer.open("xb") as output:
        output.write(secrets.token_bytes(32))
    os.chmod(issuer, 0o600)
    return {
        "OASIS7_HOSTED_REGISTRATION_ISSUER_PRIVATE_KEY": issuer.read_bytes().hex(),
        "OASIS7_HOSTED_REGISTRATION_REPLAY_LEDGER_PATH": str(root / "hosted-registration-replay.json"),
        "OASIS7_HOSTED_PLAYER_SESSION_LEDGER_PATH": str(root / "hosted-player-sessions.json"),
    }



def private_goal_storage_environment(root):
    """Provision one private encrypted-store namespace, never reuse or overwrite."""
    root = Path(root).resolve(strict=True)
    directory = root / "private-goal-store"
    keyfile = root / "private-goal-key.bin"
    directory.mkdir(mode=0o700)
    descriptor = os.open(keyfile, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600)
    with os.fdopen(descriptor, "wb") as output:
        output.write(secrets.token_bytes(32))
        output.flush()
        os.fsync(output.fileno())
    return {"OASIS7_PRIVATE_GOAL_DIR": str(directory),
            "OASIS7_PRIVATE_GOAL_KEY_FILE": str(keyfile)}


def fixture(smoke, bundle, probe_binary):
    root = smoke.root
    bundle = Path(bundle).resolve()
    chain = str(bundle / "bin/oasis7_chain_runtime")
    launcher = str(bundle / "bin/oasis7_game_launcher")
    client = str(bundle / "bin/oasis7_game_client")
    for binary in (chain, launcher, client, probe_binary, str(bundle / "bin/oasis7_viewer_live")):
        if not Path(binary).is_file():
            raise RuntimeError("built actual fixture binary unavailable")
    identity_http_port = smoke.fixture["identity_upstream"][1]
    ports = {identity_http_port, int(smoke.variables["origin"].rsplit(":", 1)[1])}
    allocated = []
    while len(allocated) < 3:
        candidate = port()
        if candidate not in ports:
            ports.add(candidate)
            allocated.append(candidate)
    chain_port, runtime_tcp_port, runtime_websocket_port = allocated
    world_root = root / "preview"
    keys = root / "node-keys"
    keys.mkdir(mode=0o700)
    def command(argv):
        phases = {"provision-identity", "initialize-preview-world", "devlocal-participant", "--identity-snapshot-export"}
        if len(argv) > 1 and argv[1] in phases:
            smoke.stage = "fixture_" + argv[1].lstrip("-").replace("-", "_")
            if argv[1] == "devlocal-participant" and len(argv) > 2 and argv[2] in {"inspect-head", "prepare", "finalize"}:
                smoke.stage += "_" + argv[2].replace("-", "_")
        else:
            smoke.stage = "fixture_tls_key_generation"
        if smoke.command(argv)[0]:
            raise RuntimeError("actual fixture setup command failed; inspect private process log")
    command([chain, "provision-identity", "--config-dir", str(keys), "--node-id", "cold-entry-smoke"])
    command([chain, "initialize-preview-world", "--root", str(world_root), "--world-id", "cold-entry-smoke",
             "--node-id", "cold-entry-smoke", "--key-dir", str(keys), "--genesis-nonce", secrets.token_hex(32),
             "--status-bind", "127.0.0.1:" + str(chain_port)])
    public = json.loads((world_root / "viewer.public.json").read_text())
    world, service = public["world"], public["service_public_key"]
    namespace = {"service_id": "cold-entry-smoke", "environment_id": "isolated-test", "instance_epoch": "1"}
    origin = smoke.variables["origin"]
    identity_db = root / "identity.sqlite"
    environment = {
        "OASIS7_HOSTED_IDENTITY_V2_ENABLED": "1", "OASIS7_HOSTED_ACCOUNT_REGISTRATION_ENABLED": "1",
        "OASIS7_HOSTED_MACHINE_REGISTRATION_ENABLED": "1", "OASIS7_HOSTED_ACCOUNT_STORE_BACKEND": "file",
        "OASIS7_HOSTED_WEBAUTHN_RP_ID": "localhost", "OASIS7_HOSTED_WEBAUTHN_ORIGINS": origin,
        "OASIS7_HOSTED_IDENTITY_SQLITE_PATH": str(identity_db), "OASIS7_HOSTED_IDENTITY_SESSION_KEY": secrets.token_hex(32),
        "OASIS7_HOSTED_IDENTITY_SERVICE_ID": namespace["service_id"], "OASIS7_HOSTED_IDENTITY_ENVIRONMENT": namespace["environment_id"],
        "OASIS7_HOSTED_IDENTITY_INSTANCE_EPOCH": "1", "OASIS7_HOSTED_WORLD_IDENTITY_SCOPE": "cold-entry-fixture-scope",
        "OASIS7_HOSTED_IDENTITY_TRUST_LOOPBACK_PROXY": "1", "OASIS7_HOSTED_PARTICIPANT_ENTRY_ENABLED": "0",
        "OASIS7_VIEWER_LIVE_BIN": str(bundle / "bin/oasis7_viewer_live"),
    }
    environment.update(hosted_startup_isolation(root))
    smoke.env.update(environment)
    # Reuse genuine initializer-produced consumer credentials without printing.
    import shlex
    for line in (world_root / "private/viewer.env").read_text().splitlines():
        if line.strip() and not line.lstrip().startswith("#"):
            key, raw = line.removeprefix("export ").split("=", 1)
            smoke.env[key] = shlex.split(raw)[0] if shlex.split(raw) else ""
    # Private goals and task contexts use the formal opt-in store. Keep these
    # sibling paths outside preview world/records/CAS, and reuse the same key
    # for the bootstrap and authorized real-service starts.
    smoke.env.update(private_goal_storage_environment(root))
    chain_args = [chain, "--node-id", "cold-entry-smoke", "--world-id", world["world_id"],
        "--config", str(keys / "node-keypair.toml"), "--runtime-root", str(world_root / "node"),
        "--execution-world-dir", str(world_root / "world"), "--execution-records-dir", str(world_root / "records"),
        "--storage-root", str(world_root / "store"), "--execution-bridge-state", str(world_root / "state.json"),
        "--replication-root", str(world_root / "runtime-replication"), "--status-bind", "127.0.0.1:" + str(chain_port),
        "--storage-profile", "dev_local", "--node-role", "sequencer", "--p2p-user-mode", "private_safe",
        "--chain-local-standalone-test", "--node-validator", "cold-entry-smoke:100",
        "--replication-network-listen", "/ip4/127.0.0.1/tcp/0"]
    launcher_args = [launcher, "--deployment-mode", "hosted_public_join", "--chain-disable",
        "--chain-world-id", world["world_id"], "--chain-status-bind", "127.0.0.1:" + str(chain_port),
        "--chain-execution-world-dir", str(world_root / "world"), "--chain-link-policy", "enforcing",
        "--viewer-static-dir", str(bundle / "web-dist"), "--no-auto-play", "--no-open-browser"]
    launcher_args += launcher_transport_args(identity_http_port, runtime_tcp_port, runtime_websocket_port)
    smoke.stage = "fixture_identity_database_bootstrap"
    original_servers = [smoke.command(chain_args, start=True), smoke.command(launcher_args, start=True)]
    ready = False
    for _ in range(60):
        try:
            connection = http.client.HTTPSConnection("localhost", int(origin.rsplit(":", 1)[1]),
                context=ssl.create_default_context(cafile=str(root / "root.crt")), timeout=2)
            connection.request("GET", "/api/public/hosted-account/v2/capabilities")
            response = connection.getresponse()
            ready = response.status == 200 and json.loads(response.read())["ok"] is True
            connection.close()
            if ready:
                break
        except (OSError, ValueError, KeyError, http.client.HTTPException):
            pass
        time.sleep(min(0.25, smoke.remaining()))
    if not ready:
        raise RuntimeError("actual identity database bootstrap did not become ready")
    for process in reversed(original_servers):
        try:
            os.killpg(process.pid, signal.SIGTERM)
        except ProcessLookupError:
            pass
        process.wait(timeout=5)
    key_public = []
    for role in ("issuer", "operator"):
        command(["openssl", "genpkey", "-algorithm", "Ed25519", "-out", str(root / (role + ".pem"))])
        command(["openssl", "pkey", "-in", str(root / (role + ".pem")), "-outform", "DER", "-out", str(root / (role + ".der"))])
        command(["openssl", "pkey", "-in", str(root / (role + ".pem")), "-pubout", "-outform", "DER", "-out", str(root / (role + ".pub"))])
        seed = (root / (role + ".der")).read_bytes()[-32:]
        (root / (role + ".key")).write_bytes(seed)
        os.chmod(root / (role + ".key"), 0o600)
        key_public.append((root / (role + ".pub")).read_bytes()[-32:].hex())
    issuer, operator = key_public
    config = {"world": world, "world_directory": str(world_root / "world"), "origin_root": str(world_root),
        "execution_records_directory": str(world_root / "records"), "storage_root": str(world_root / "store"),
        "node_id": "cold-entry-smoke", "node_key_directory": str(keys), "service_public_key": service,
        "operator_public_key": operator, "identity_namespace": namespace, "world_identity_scope": "cold-entry-fixture-scope"}
    source = write(root, "tool-source.json", config)
    head = root / "canonical-head.json"
    command([chain, "devlocal-participant", "inspect-head", "--config", source, "--output", str(head)])
    config["expected_head"] = json.loads(head.read_text())
    tool = write(root, "tool.json", config)
    command([chain, "devlocal-participant", "prepare", "--config", tool, "--output", str(root / "prepared.json")])
    export = write(root, "export-request.json", {"identity_namespace": {"identity_service_id": namespace["service_id"], "environment_id": namespace["environment_id"]},
        "rp_config": {"rp_id": "localhost", "allowed_origins": [origin], "version": 1, "identity_instance_epoch": 1}})
    command([launcher, "--identity-snapshot-export", str(identity_db), export, str(root / "snapshot.json")])
    policy = write(root, "policy.json", {"api_rev": 1, "world_id": world["world_id"], "genesis_digest": world["genesis_digest"],
        "identity_namespace": namespace, "issuer_public_key": issuer, "generation": 1, "enabled": True,
        "max_grant_ticks": 256, "max_participants": 10, "registry_complete": True})
    entries = write(root, "entries.json", [])
    command([chain, "devlocal-participant", "finalize", "--config", tool, "--prepared", str(root / "prepared.json"),
        "--identity-snapshot", str(root / "snapshot.json"), "--policy", policy, "--entries", entries,
        "--issuer-key", str(root / "issuer.key"), "--operator-key", str(root / "operator.key"), "--output", str(root / "authorization.json")])
    # cli_participant_authority::validate rejects every explicit replication
    # listener, including loopback, once local operator authority is enabled.
    # Keep the original bootstrap startup unchanged; the authorized startup is
    # the isolated single-node path and carries no replication listen argument.
    listen_index = chain_args.index("--replication-network-listen")
    del chain_args[listen_index:listen_index + 2]
    chain_args += ["--devlocal-participant-origin-root", str(world_root), "--devlocal-participant-operator-public-key", operator,
                   "--devlocal-participant-policy-file", str(root / "authorization.json")]
    smoke.env.update({"OASIS7_HOSTED_PARTICIPANT_ENTRY_ENABLED": "1", "OASIS7_HOSTED_OWNER_CONTROL_ENABLED": "1",
        "OASIS7_HOSTED_EXECUTOR_CONTROL_ENABLED": "1", "OASIS7_HOSTED_PARTICIPANT_ENTRY_TRUST": json.dumps({"endpoint": "http://127.0.0.1:" + str(chain_port), "service_public_key": service, "world": world}),
        "OASIS7_HOSTED_PARTICIPANT_ISSUER_PRIVATE_KEY": (root / "issuer.key").read_bytes().hex()})
    identity = {"endpoint": origin, "identity_service_id": namespace["service_id"], "environment_id": namespace["environment_id"],
        "audience": origin, "state_directory": str(root / "login-vault")}
    return {"client_binary": client, "setup_commands": [], "server_commands": [chain_args, launcher_args],
        "restart_committed_chain": True,
        "initial_prepared": str(root / "prepared.json"), "canonical_probe": [probe_binary, str(root / "orchestration"), str(root / "executor-config/executor.json")],
        "binding_response_path": "/api/public/hosted-account/v2/entry-owner/control", "plan_template": {"owner_approved": True,
        "entry": {"identity": identity, "identity_instance_epoch": "1", "world": world, "trusted_world_service_key": service, "owner_state_directory": str(root / "owner-vault")},
        "orchestration_directory": str(root / "orchestration"), "executor_vault_directory": str(root / "executor-vault"),
        "executor_state_directory": str(root / "executor-state"), "executor_config_directory": str(root / "executor-config"),
        "display_name": "Actual cold entry fixture", "starter": "claim", "executor_id": "smoke-executor",
        "delegation_generation": "1", "delegation_nonce": "1", "goal": {"goal_text": "Move once and verify the canonical result", "soft_constraints": []}}}
