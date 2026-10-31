#!/usr/bin/env python3
"""Local Linux fixture supervisor. Private stdin only; no production authority.
Docker root only creates fixture OS policy and drops all CLI children to H/M/N.
No Native SID, Owner key, purpose token or wrapping key is returned by this API.
"""
import hashlib, json, os, pathlib, secrets, signal, subprocess, sys, time
H, M, N, SHARED = 21001, 21002, 21003, 41000
ROOT = pathlib.Path('/tmp/oasis7-host-v2-component')
BINARY = pathlib.Path('/artifacts/host-cli')
SOCKET = ROOT / 'socket' / 'control.sock'
VAULT = ROOT / 'private' / 'vault'
wrapping = secrets.token_hex(32)
server = None
relay = None
initialized = False
registry_created = False

def deny(code):
    raise RuntimeError(code)

def drop(uid):
    def apply():
        os.setgroups([SHARED] if uid in (H, M) else [])
        os.setgid(uid)
        os.setuid(uid)
        os.umask(0o077)
    return apply

def mkdir(path, mode, uid=0, gid=0):
    path.mkdir()
    # Root has CHOWN but not FOWNER: chmod while the fresh inode is root-owned.
    os.chmod(path, mode)
    os.chown(path, uid, gid)

def setup():
    if os.geteuid() != 0 or os.getpid() != 1 or ROOT.exists() or not BINARY.is_file():
        deny('fixture_setup_rejected')
    mkdir(ROOT, 0o755)
    mkdir(ROOT / 'private', 0o700, H, H)
    mkdir(ROOT / 'socket', 0o2710, H, SHARED)
    load_skill()

def load_skill():
    # Exactly the existing same-source ECS package format, no parallel manifest.
    root = pathlib.Path('/formal-skill')
    raw = (root / 'manifest.json').read_bytes()
    if len(raw) > 32768:
        deny('fixture_skill_manifest_limit')
    manifest = json.loads(raw)
    if manifest.get('schema') != 1 or manifest.get('canonical_path') != '/skills/oasis7-play/SKILL.md':
        deny('fixture_skill_manifest_rejected')
    parts = []
    for name in ['SKILL.md','references/custody-native.md']:
        data = (root / name).read_bytes()
        record = manifest['files'][name]
        if len(data) > 512 * 1024 or record['bytes'] != len(data) or record['sha256'] != hashlib.sha256(data).hexdigest():
            deny('fixture_skill_content_rejected')
        parts.append(data.decode())
    return '\n'.join(parts)

def model_pids():
    found = []
    for path in pathlib.Path('/proc').iterdir():
        if not path.name.isdecimal():
            continue
        try:
            lines = (path / 'status').read_text().splitlines()
        except FileNotFoundError:
            continue
        uid = next(line.split()[1:] for line in lines if line.startswith('Uid:'))
        if any(int(value) == M for value in uid):
            found.append(int(path.name))
    return found

def retire_model():
    # Only this fresh PID namespace (supervisor PID1), never host accounts.
    for pid in model_pids():
        try: os.kill(pid, signal.SIGKILL)
        except ProcessLookupError: pass
    until = time.monotonic() + 5
    while time.monotonic() < until:
        while True:
            try:
                pid, _ = os.waitpid(-1, os.WNOHANG)
                if pid == 0: break
            except ChildProcessError: break
        if not model_pids():
            return
        time.sleep(0.02)
    deny('fixture_model_retirement_failed')

def cli(uid, command, body, socket=False):
    args = [str(BINARY), command]
    if command == 'custody-control-call':
        args += ['--socket', str(SOCKET)]
    else:
        args += ['--vault', str(VAULT)]
        if socket:
            args += ['--socket', str(SOCKET)]
    env = {'PATH': '/usr/bin:/bin', 'HOME': '/nonexistent'}
    # The exact public localhost fixture certificate is mounted by trusted root.
    # Linux rustls-platform-verifier uses native-certs SSL_CERT_FILE, no insecure TLS.
    if pathlib.Path('/fixture-ca/localhost.crt').is_file():
        env['SSL_CERT_FILE'] = '/fixture-ca/localhost.crt'
    result = subprocess.run(args, input=json.dumps(body).encode(),
        stdout=subprocess.PIPE, stderr=subprocess.PIPE, env=env,
        preexec_fn=drop(uid), timeout=35, check=False)
    if result.returncode != 0 or len(result.stdout) > 32768:
        deny('fixture_cli_rejected')
    try:
        return json.loads(result.stdout)
    except (ValueError, UnicodeError):
        deny('fixture_cli_shape_rejected')

def private_denied(uid):
    result = subprocess.run([sys.executable, '-c',
        'import pathlib,sys; p=pathlib.Path(sys.argv[1]); p.read_bytes()',
        str(VAULT / 'host.state')], stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL, preexec_fn=drop(uid), timeout=3)
    return result.returncode != 0

def registry(binding):
    global registry_created
    directory = pathlib.Path('/var/lib/oasis7-custody/model-bindings')
    for path in [directory.parent, directory]:
        if not path.exists():
            path.mkdir(); os.chmod(path, 0o755)
        info = path.lstat()
        if info.st_uid != 0 or info.st_mode & 0o022 or path.is_symlink():
            deny('fixture_registry_rejected')
    allowed = ['api_version','transport_profile','identity_digest','descriptor_digest',
        'session_digest','resource_id','restricted_runtime_identity','isolation']
    record = {key: binding[key] for key in allowed}
    path = directory / f'{M}.json'
    # One fixture lifetime, no replacement/rebind or old-M retirement assertion.
    with path.open('x') as stream:
        json.dump(record, stream)
    os.chmod(path, 0o644)
    registry_created = True

def handle(request):
    global initialized, server
    if not isinstance(request, dict) or set(request) != {'operation','input'}:
        deny('fixture_request_rejected')
    operation, data = request['operation'], request['input']
    if operation == 'initialize' and not initialized:
        if not isinstance(data, dict) or set(data) != {'identity','request_id'}:
            deny('fixture_request_rejected')
        identity = data['identity']
        # Real Native is remote on the developer host. N21003 is only an
        # independent negative-access witness, never a fake local Native service.
        isolation = identity['isolation']
        if isolation['host_uid'] != H or isolation['model_uid'] != M:
            deny('fixture_principal_rejected')
        if set(isolation['native']) != {'kind','https_origin'} or isolation['native']['kind'] != 'remote':
            deny('fixture_native_placement_rejected')
        identity['skill_manifest_path'] = '/formal-skill/manifest.json'
        result = cli(H, 'custody-host-init', {**data,'wrapping_key':wrapping})
        initialized = True
        readable = subprocess.run([sys.executable,'-c',
            'import pathlib,sys; assert pathlib.Path(sys.argv[1]).read_bytes()',
            str(VAULT / 'host.state')], stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL, preexec_fn=drop(H), timeout=3)
        if readable.returncode != 0:
            deny('fixture_host_private_read_rejected')
        if not private_denied(M) or not private_denied(N):
            deny('fixture_private_access_rejected')
        return result
    if not initialized:
        deny('fixture_not_initialized')
    if operation == 'enroll' and data == {}:
        return cli(H, 'custody-host-enroll', {'wrapping_key':wrapping})
    if operation == 'bind':
        return cli(H, 'custody-host-bind', {'wrapping_key':wrapping,'binding':data})
    if operation == 'claim':
        return cli(H, 'custody-host-claim', {'wrapping_key':wrapping,'request':data})
    if operation == 'serve' and data == {} and server is None:
        binding = cli(H, 'custody-host-profile', {'wrapping_key':wrapping}, socket=True)
        registry(binding)
        server = subprocess.Popen([str(BINARY),'custody-host-serve','--vault',str(VAULT),
            '--socket',str(SOCKET)], stdin=subprocess.PIPE, stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL, preexec_fn=drop(H), env={'PATH':'/usr/bin:/bin',
                'HOME':'/nonexistent','SSL_CERT_FILE':'/fixture-ca/localhost.crt'})
        server.stdin.write(json.dumps({'wrapping_key':wrapping}).encode());server.stdin.close()
        for _ in range(150):
            if SOCKET.is_socket():
                return {'ready':True,'private_denied_m':private_denied(M),
                    'private_denied_n_witness':private_denied(N)}
            if server.poll() is not None:
                deny('fixture_server_exited')
            time.sleep(0.1)
        deny('fixture_server_not_ready')
    if operation == 'model_probe' and server is not None:
        if not isinstance(data, dict) or set(data) != {'resource_id'} or model_pids():
            deny('fixture_model_launch_rejected')
        text = load_skill()
        payload = {'skill_text':text,'skill_sha256':hashlib.sha256(text.encode()).hexdigest(),
            'socket':str(SOCKET),'resource_id':data['resource_id']}
        result = subprocess.run([sys.executable,'/fixtures/host_model_probe.py'],
            input=json.dumps(payload).encode(),stdout=subprocess.PIPE,stderr=subprocess.DEVNULL,
            preexec_fn=drop(M),env={'PATH':'/usr/bin:/bin','HOME':'/nonexistent'},timeout=150)
        if result.returncode != 0 or len(result.stdout) > 32768:
            deny('fixture_model_probe_rejected')
        retire_model()
        reply = json.loads(result.stdout)
        reply['model_namespace_retired'] = not model_pids()
        return reply
    if operation == 'control' and server is not None:
        # M enters the actual public CLI; no direct Rust call or role claim.
        return cli(M, 'custody-control-call', data)
    deny('fixture_operation_rejected')

def main():
    global relay
    setup()
    port = os.environ.get('OASIS7_LOCAL_RP_PORT', '')
    if not port.isdecimal() or not 1024 <= int(port) <= 65535:
        deny('fixture_port_rejected')
    relay = subprocess.Popen([sys.executable,'/fixtures/localhost_relay.py',port],
        stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
    try:
        while True:
            line = sys.stdin.buffer.readline(65537)
            if not line:
                break
            if len(line) > 65536:
                deny('fixture_request_limit')
            try:
                response = {'ok':True,'result':handle(json.loads(line))}
            except (RuntimeError, ValueError, KeyError, TypeError, OSError,
                    subprocess.SubprocessError):
                # Never print exception strings, stderr, request bodies or secrets.
                response = {'ok':False,'error_code':'host_component_fixture_rejected'}
            print(json.dumps(response), flush=True)
    finally:
        retire_model()
        if registry_created:
            pathlib.Path(f'/var/lib/oasis7-custody/model-bindings/{M}.json').unlink()
        if relay is not None:
            relay.terminate();relay.wait(timeout=5)
        if server is not None:
            server.terminate()
            try: server.wait(timeout=5)
            except subprocess.TimeoutExpired: server.kill();server.wait()

if __name__ == '__main__':
    main()
