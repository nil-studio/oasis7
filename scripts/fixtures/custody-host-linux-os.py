#!/usr/bin/env python3
"""Disposable Linux UID/IPC component fixture. Never run on a host OS."""
import argparse, ctypes, errno, hashlib, json, os, pathlib, secrets, signal, socket, subprocess, sys, time
P = pathlib.Path
ROOT = P("/run/custody-os")
REG = P("/var/lib/oasis7-custody/model-bindings/21002.json")
TEST = "custody_tool_host::ipc::linux_os_tests::linux_os_component_worker"
H, M, N, GROUP = 21001, 21002, 21003, 22000
libc = ctypes.CDLL(None, use_errno=True)
children = []
diagnostic = {"stage": "startup", "child_role": None, "child_exit": None}
RUNTIME_CODES = frozenset({
    "bounding_cap_drop_failed", "capset_failed", "ambient_cap_clear_failed", "no_new_privileges_failed",
    "dumpability_failed", "pdeathsig_failed", "supervisor_disappeared", "component_child_timeout",
    "component_child_unexpected_exit", "component_readiness_failed", "metadata_reply_invalid",
    "safe_probe_result_missing", "disposable_linux_container_root_required", "fixture_must_start_empty",
    "canonical_linux_artifacts_required", "wrong_server_received_request_before_peer_check",
    "actual_kernel_wire_counts_invalid",
})

def stage(code):
    diagnostic.update(stage=code, child_role=None, child_exit=None)

def safe_error(error):
    kind = type(error).__name__
    if kind not in {"RuntimeError", "OSError", "PermissionError", "FileNotFoundError", "FileExistsError",
                    "SubprocessError", "TimeoutExpired", "ValueError", "TypeError", "JSONDecodeError",
                    "KeyError", "AttributeError", "IndexError", "AssertionError"}:
        kind = "Exception"
    number = error.errno if isinstance(error, OSError) and isinstance(error.errno, int) and 0 < error.errno < 4096 else None
    code = error.args[0] if isinstance(error, RuntimeError) and len(error.args) == 1 and error.args[0] in RUNTIME_CODES else None
    return {**diagnostic, "exception_kind": kind, "errno": number, "runtime_code": code}



LEAF_STAGES = frozenset({
    "pins.current_exe_hash_begin", "pins.current_exe_hash_finished",
    "host.entry", "host.key_read", "host.vault_initialize_begin", "host.vault_initialized",
    "host.binding_read_begin", "host.binding_read_finished", "host.vault_reopen_begin", "host.vault_reopened",
})

def collect_leaves(proc):
    # Failure-only, bounded, nonblocking parse. Never forward raw child output.
    os.set_blocking(proc.stdout.fileno(), False)
    try:
        raw = os.read(proc.stdout.fileno(), 65536)
    except BlockingIOError:
        raw = b""
    leaves = []
    for line in raw.splitlines():
        if b"CUSTODY_OS_LEAF " not in line:
            continue
        try:
            value = json.loads(line.split(b"CUSTODY_OS_LEAF ", 1)[1])
        except (ValueError, UnicodeError):
            continue
        if isinstance(value, dict) and set(value) == {"stage", "elapsed_ms"} and isinstance(value["stage"], str) and value["stage"] in LEAF_STAGES \
                and type(value["elapsed_ms"]) is int and 0 <= value["elapsed_ms"] <= 3600000:
            leaves.append(value)
    diagnostic["child_leaves"] = leaves[-16:]


def require(value, code):
    if not value:
        raise RuntimeError(code)


def lower(uid, groups, mask):
    supervisor = os.getpid()
    def child():
        os.umask(mask)
        # No inherited descriptors except Popen's private stdio; all execs fixed.
        last = int(P("/proc/sys/kernel/cap_last_cap").read_text())
        for cap in range(last + 1):
            require(libc.prctl(24, cap, 0, 0, 0) == 0, "bounding_cap_drop_failed")
        os.setgroups(groups)
        os.setresgid(uid, uid, uid)
        os.setresuid(uid, uid, uid)
        class Header(ctypes.Structure):
            _fields_ = [("version", ctypes.c_uint32), ("pid", ctypes.c_int)]
        class Data(ctypes.Structure):
            _fields_ = [("effective", ctypes.c_uint32), ("permitted", ctypes.c_uint32), ("inheritable", ctypes.c_uint32)]
        require(libc.capset(ctypes.byref(Header(0x20080522, 0)), ctypes.byref((Data * 2)())) == 0, "capset_failed")
        require(libc.prctl(47, 4, 0, 0, 0) == 0, "ambient_cap_clear_failed")
        require(libc.prctl(38, 1, 0, 0, 0) == 0, "no_new_privileges_failed")
        require(libc.prctl(4, 0, 0, 0, 0) == 0, "dumpability_failed")
        # Credential changes clear PDEATHSIG; install after setresuid/setresgid.
        require(libc.prctl(1, signal.SIGKILL, 0, 0, 0) == 0, "pdeathsig_failed")
        require(os.getppid() == supervisor, "supervisor_disappeared")
    return child


def spawn(argv, uid, groups=(), mask=0o077, role=None, private_input=None):
    env = {"PATH": "/usr/local/bin:/usr/bin:/bin", "OASIS7_CUSTODY_OS_ROOT": str(ROOT)}
    if role:
        env["OASIS7_CUSTODY_OS_ROLE"] = role
    diagnostic.update(child_role=role or "cli", child_exit=None)
    proc = subprocess.Popen(argv, env=env, stdin=subprocess.PIPE if private_input is not None else subprocess.DEVNULL,
                            stdout=subprocess.PIPE, stderr=subprocess.PIPE, close_fds=True,
                            start_new_session=True, preexec_fn=lower(uid, groups, mask))
    proc.custody_fixture_role = role or "cli"
    children.append(proc)
    if private_input is not None:
        proc.stdin.write(private_input)
        proc.stdin.close()
        proc.stdin = None
    return proc


def worker(exe, role, uid, groups=(), mask=0o077, private_input=None):
    return spawn([exe, TEST, "--exact", "--ignored", "--nocapture", "--test-threads=1"], uid, groups, mask, role, private_input)


def finish(proc, expected=True):
    diagnostic.update(child_role=proc.custody_fixture_role, child_exit=proc.poll())
    try:
        out, _ = proc.communicate(timeout=8)
    except subprocess.TimeoutExpired:
        proc.kill()
        proc.communicate()
        raise RuntimeError("component_child_timeout") from None
    diagnostic["child_exit"] = proc.returncode
    require((proc.returncode == 0) == expected, "component_child_unexpected_exit")
    return out


def wait_file(path, proc):
    end = time.monotonic() + 15
    while not path.exists():
        diagnostic.update(child_role=proc.custody_fixture_role, child_exit=proc.poll())
        if proc.poll() is not None or time.monotonic() >= end:
            collect_leaves(proc)
        require(proc.poll() is None and time.monotonic() < end, "component_readiness_failed")
        time.sleep(0.01)


def mkdir(path, uid, gid, mode):
    path.mkdir()
    # Supervisor has CHOWN, not FOWNER: set mode while still the actual owner.
    path.chmod(mode)
    os.chown(path, uid, gid)


def request(binary, uid, socket_path, expected=True):
    body = b'{"api_version":1,"resource_id":"fixture-resource","request_id":"fixture-request","method":"tools_list"}'
    proc = spawn([binary, "custody-control-call", "--socket", str(socket_path)], uid, [GROUP], private_input=body)
    out = finish(proc, expected)
    if expected:
        value = json.loads(out)
        require(value == {"api_version": 1, "request_id": "fixture-request", "status": "unavailable", "close_requested": False}, "metadata_reply_invalid")
    return proc.returncode == 0


def probe(exe, uid, groups=()):
    out = finish(worker(exe, "probe", uid, groups))
    lines = [line.split(b"CUSTODY_OS_FACTS ", 1)[1] for line in out.splitlines() if b"CUSTODY_OS_FACTS " in line]
    require(len(lines) == 1, "safe_probe_result_missing")
    return json.loads(lines[0])


def file_sha256(path):
    digest = hashlib.sha256()
    with P(path).open("rb") as source:
        for block in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def main():
    stage("arguments")
    parser = argparse.ArgumentParser()
    parser.add_argument("--host-umask", choices=["022", "077"], required=True)
    args = parser.parse_args()
    stage("container_guard")
    require(sys.platform == "linux" and os.geteuid() == 0 and P("/.dockerenv").exists(), "disposable_linux_container_root_required")
    require(not ROOT.exists() and not REG.exists(), "fixture_must_start_empty")
    exe, binary = "/artifacts/host-tests", "/artifacts/game-client"
    require(P(exe).is_file() and P(binary).is_file(), "canonical_linux_artifacts_required")
    stage("filesystem_setup")
    os.umask(0o022)
    ROOT.mkdir(mode=0o755)
    for path, uid, gid, mode in [("host-private", H, H, 0o700), ("native-private", N, N, 0o700),
                                 ("host-public", H, H, 0o755), ("native-public", N, N, 0o755), ("socket", H, GROUP, 0o2710)]:
        mkdir(ROOT / path, uid, gid, mode)
    (ROOT / "skill.json").write_text('{"fixture":"OS-component-only-not-Skill-execution"}')
    (ROOT / "skill.json").chmod(0o444)
    stage("native_spawn")
    native = worker(exe, "native", N)
    stage("native_ready")
    wait_file(ROOT / "native-public/ready.json", native)
    stage("host_spawn")
    host = worker(exe, "host", H, [GROUP], int(args.host_umask, 8), secrets.token_bytes(32))
    (ROOT / "host.pid").write_text(str(host.pid)); (ROOT / "host.pid").chmod(0o644)
    stage("host_public_binding_ready")
    wait_file(ROOT / "host-public/binding.json", host)
    binding = json.loads((ROOT / "host-public/binding.json").read_text())
    stage("root_registry_publish")
    REG.parent.mkdir(mode=0o755, parents=True, exist_ok=False)
    REG.parent.chmod(0o755)
    REG.write_text(json.dumps({key: value for key, value in binding.items() if key != "socket_path"}))
    REG.chmod(0o644)
    stage("host_socket_ready")
    wait_file(ROOT / "host-public/ready.json", host)
    # Supervisor may traverse public H/M socket path, never gains vault access.
    stage("supervisor_public_group")
    os.setgroups([GROUP])
    side = ROOT / "socket/host.sock.custody.json"
    mode = side.stat().st_mode & 0o7777
    stage("model_privacy_probe")
    model_probe = probe(exe, M, [GROUP])
    stage("native_privacy_probe")
    native_probe = probe(exe, N)
    facts = {"kind": "linux_uid_kernel_metadata_component", "host_umask": args.host_umask,
             "actual_sidecar_mode": oct(mode), "model_probe": model_probe, "native_probe": native_probe,
             "native_authorization_tested": False, "provider_calls": 0, "native_http_calls": 0,
             "world_closed": False, "old_model_lifecycle_qualified": False}
    stage("model_public_metadata_readability")
    if mode != 0o644 or not model_probe["public_sidecar_readable"]:
        # The 077 failure is a real target RED; never count expected failure as PASS.
        request(binary, M, ROOT / "socket/host.sock", expected=False)
        print(json.dumps({**facts, "status": "failed", "failure": "public_metadata_unreadable_to_actual_model_uid"}))
        return 1
    stage("model_actual_cli_positive")
    request(binary, M, ROOT / "socket/host.sock")
    # Root and H may traverse/read metadata, but cannot impersonate configured M.
    stage("root_model_impersonation_negative")
    request(binary, 0, ROOT / "socket/host.sock", expected=False)
    stage("host_model_impersonation_negative")
    request(binary, H, ROOT / "socket/host.sock", expected=False)
    stage("wrong_kernel_peer_negative")
    finish(worker(exe, "wrong-peer", 21004, [GROUP]))
    original = REG.read_bytes()
    stage("writable_registry_negative")
    REG.chmod(0o664)
    request(binary, M, ROOT / "socket/host.sock", expected=False)
    REG.chmod(0o644)
    stage("missing_registry_negative")
    REG.unlink()
    request(binary, M, ROOT / "socket/host.sock", expected=False)
    REG.write_bytes(original); REG.chmod(0o644)
    stage("nonroot_registry_negative")
    os.chown(REG, M, M)
    request(binary, M, ROOT / "socket/host.sock", expected=False)
    stage("restored_registry_positive")
    os.chown(REG, 0, 0)
    request(binary, M, ROOT / "socket/host.sock")
    # A root-created listening socket is deliberately labelled owner H. The
    # actual client must reject kernel peer0 before writing any request frame.
    stage("wrong_server_peer_negative")
    fake_dir = ROOT / "wrong-server"; fake_dir.mkdir()
    fake_socket = fake_dir / "host.sock"
    listener = socket.socket(socket.AF_UNIX); listener.bind(str(fake_socket)); listener.listen(1); listener.settimeout(3)
    fake = {**binding, "socket_path": str(fake_socket)}
    fake_side = P(str(fake_socket) + ".custody.json"); fake_side.write_text(json.dumps(fake)); fake_side.chmod(0o644)
    # Same ownership ordering for the deliberately wrong-peer socket metadata.
    fake_socket.chmod(0o660)
    fake_dir.chmod(0o2710)
    os.chown(fake_side, H, GROUP); os.chown(fake_socket, H, GROUP)
    os.chown(fake_dir, H, GROUP)
    request(binary, M, fake_socket, expected=False)
    connection, _ = listener.accept(); connection.settimeout(3)
    require(connection.recv(1) == b"", "wrong_server_received_request_before_peer_check")
    connection.close(); listener.close()
    stage("workers_retire_and_count")
    (ROOT / "stop").touch()
    finish(host); finish(native)
    host_facts = json.loads((ROOT / "host-public/facts.json").read_text())
    require(host_facts["metadata_requests"] == 2 and host_facts["wrong_kernel_peer_rejected"] == 1, "actual_kernel_wire_counts_invalid")
    print(json.dumps({**facts, "status": "passed", "scope": "OS_kernel_metadata_only_no_Control_authorization", "host_facts": host_facts,
                      "root_and_host_model_impersonation_denied": True, "wrong_server_peer_denied_before_frame": True,
                      "registry_missing_nonroot_writable_denied": True,
                      "artifact_sha256": {"test": file_sha256(exe), "cli": file_sha256(binary)}}))
    return 0


if __name__ == "__main__":
    result = 1
    try:
        result = main()
    except Exception as error:
        print(json.dumps({"status": "failed", "failure": "closed_component_fixture_error", "native_authorization_tested": False, **safe_error(error)}))
    finally:
        if ROOT.exists():
            (ROOT / "stop").touch()
        for child in children:
            if child.poll() is None:
                try:
                    child.wait(timeout=3)
                except subprocess.TimeoutExpired:
                    os.killpg(child.pid, signal.SIGKILL)
                    child.wait(timeout=3)
    sys.exit(result)
