"""Root-only Linux process fixture; synthetic credentials only, no external model calls."""
import hashlib, json, os, pathlib, shutil, signal, subprocess, sys, tempfile, time


def subject(uid):
    def isolate():
        os.setgroups([]); os.setgid(uid); os.setuid(uid)
    return isolate


def interrupted(signum, _frame):
    raise SystemExit(128 + signum)


def binary_hash(path):
    with path.open('rb') as source:
        return hashlib.file_digest(source, 'sha256').hexdigest()


client = r"""
import json,pathlib,socket,sys,time,subprocess
sock,secret,config,mode,binary=sys.argv[1:]
for path in [secret,config]:
    try: pathlib.Path(path).read_bytes()
    except PermissionError: pass
    else: raise AssertionError('model could read secret fixture')
request={'profile':'dev-test-accounts-v1','audience':'oasis7-dev-test-tools','method':'capabilities'}
if mode=='cli':
    result=subprocess.run([binary,'dev-test-local-call','--socket',sock],input=json.dumps(request),capture_output=True,text=True,timeout=4)
    assert result.returncode==0,result.stderr
    print(result.stdout);sys.exit(0)
if mode=='wrong-audience': request['audience']='manager'
if mode=='wrong-version': request['profile']='unknown'
s=socket.socket(socket.AF_UNIX);s.settimeout(2);s.connect(sock)
body=json.dumps(request).encode();started=time.monotonic()
if mode=='slowdrip':
    try:
        for byte in body:
            s.sendall(bytes([byte]));time.sleep(.02)
    except OSError: pass
else: s.sendall(body)
try: s.shutdown(socket.SHUT_WR)
except OSError: pass
response=b''
while True:
    try: chunk=s.recv(4096)
    except ConnectionResetError: break
    if not chunk: break
    response+=chunk
if mode=='slowdrip':
    assert time.monotonic()-started<1.5
    assert not response or json.loads(response)['status']=='local_request_denied'
else: print(response.decode())
"""
def main():
    if sys.platform != 'linux' or os.geteuid() != 0:
        raise SystemExit('cross-UID fixture requires root in an isolated Linux environment')
    if len(sys.argv) != 2:
        raise SystemExit('usage: dev_test_local_linux.py <built Linux CLI>')
    source = pathlib.Path(sys.argv[1]).resolve(strict=True)
    signal.signal(signal.SIGTERM, interrupted)
    signal.signal(signal.SIGINT, interrupted)
    with tempfile.TemporaryDirectory(prefix='oasis7-local-', dir='/tmp') as temporary:
        root = pathlib.Path(temporary)
        processes = []
        try:
            os.chmod(root, 0o755)
            # Distinct callers must execute the exact tested binary without
            # needing traversal permission on the checkout's parent directories.
            binary = root / 'oasis7_game_client'
            expected_hash = binary_hash(source)
            shutil.copyfile(source, binary)
            os.chmod(binary, 0o755)
            assert binary_hash(binary) == expected_hash
            print('Linux CLI sha256=' + expected_hash, flush=True)
            private = root / 'private'
            private.mkdir(mode=0o700)
            (private / 'usage').mkdir(mode=0o700)
            public = root / 'public'
            public.mkdir(mode=0o755)
            config_path = private / 'config.json'
            config = dict(profile='dev-test-accounts-v1', socket=str(private / 'service.sock'), usage_directory=str(private / 'usage'), grant_id='linux-fixture', max_request_bytes=4096, timeout_ms=250, max_calls=20, credentials=[dict(caller_id='model', token='synthetic-fixture-token-never-model-visible', expires_at_unix=int(time.time())+120, revoked=False)])
            config_path.write_text(json.dumps(config)); os.chmod(config_path, 0o600)
            secret = private / 'future-credential-fixture'
            secret.write_text('synthetic-private-material'); os.chmod(secret, 0o600)

            def start(args, socket):
                process = subprocess.Popen([binary] + args, stdout=subprocess.DEVNULL,
                                           stderr=subprocess.PIPE)
                processes.append(process)
                for _ in range(100):
                    if socket.exists():
                        return
                    if process.poll() is not None:
                        raise AssertionError(process.stderr.read().decode())
                    time.sleep(.02)
                raise AssertionError('service startup timeout')

            def call(uid, mode='normal'):
                # All children retain timeout's process group so root can stop
                # every UID on cancellation, including a CLI grandchild.
                result = subprocess.run([sys.executable, '-c', client, str(public/'tools.sock'),
                                         str(secret), str(config_path), mode, str(binary)],
                                        preexec_fn=subject(uid), capture_output=True,
                                        text=True, timeout=5)
                assert result.returncode == 0, result.stderr
                return json.loads(result.stdout) if result.stdout.strip() else None

            start(['dev-test-serve', '--profile', str(config_path)], private/'service.sock')
            start(['dev-test-local-serve', '--socket', str(public/'tools.sock'), '--profile',
                   str(config_path), '--caller-uid', '1001', '--caller-id', 'model'],
                  public/'tools.sock')
            assert (result := call(1001))['status'] == 'ok', result
            assert call(1001, 'cli')['status'] == 'ok'
            assert call(1002)['status'] == 'local_request_denied'
            assert call(1001, 'wrong-audience')['status'] == 'wrong_audience'
            assert call(1001, 'wrong-version')['status'] == 'unsupported_profile'
            call(1001, 'slowdrip')
            # An independent connection succeeds after the timed out request.
            assert (result := call(1001))['status'] == 'ok', result
            config['credentials'][0]['revoked'] = True
            config_path.write_text(json.dumps(config))
            assert call(1001)['status'] == 'local_request_denied'
            config['credentials'][0]['revoked'] = False
            config['credentials'][0]['expires_at_unix'] = int(time.time()) - 1
            config_path.write_text(json.dumps(config))
            assert call(1001)['status'] == 'local_request_denied'
        finally:
            cleanup_errors = []
            for process in reversed(processes):
                try:
                    process.terminate()
                    try:
                        process.wait(timeout=1)
                    except subprocess.TimeoutExpired:
                        process.kill()
                        process.wait(timeout=1)
                except (OSError, subprocess.TimeoutExpired) as error:
                    cleanup_errors.append(str(error))
                finally:
                    process.stderr.close()
            if cleanup_errors:
                raise RuntimeError('owned service cleanup failed: ' + '; '.join(cleanup_errors))
    print('PASS: distinct UID success/denial, private fixtures unreadable, wrong purpose/version, slowdrip/reconnect, revocation/expiry')


if __name__ == '__main__':
    main()
