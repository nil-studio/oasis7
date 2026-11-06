"""Root-only Linux process fixture; synthetic credentials only, no external model calls."""
import json, os, pathlib, shutil, subprocess, sys, tempfile, time
binary = str(pathlib.Path(sys.argv[1]).resolve())
root = pathlib.Path(tempfile.mkdtemp(prefix="oasis7-local-", dir="/run"))
os.chmod(root, 0o755)
private = root / "private"
private.mkdir(mode=0o700)
(private / "usage").mkdir(mode=0o700)
public = root / "public"
public.mkdir(mode=0o755)
config_path = private / "config.json"
config = dict(profile="dev-test-accounts-v1", socket=str(private / "service.sock"), usage_directory=str(private / "usage"), grant_id="linux-fixture", max_request_bytes=4096, timeout_ms=250, max_calls=20, credentials=[dict(caller_id="model", token="synthetic-fixture-token-never-model-visible", expires_at_unix=int(time.time())+120, revoked=False)])
config_path.write_text(json.dumps(config)); os.chmod(config_path,0o600)
secret = private / "future-credential-fixture"
secret.write_text("synthetic-private-material"); os.chmod(secret,0o600)
processes=[]
def start(args, socket):
    process=subprocess.Popen([binary]+args, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE)
    processes.append(process)
    for _ in range(100):
        if socket.exists(): return
        if process.poll() is not None: raise AssertionError(process.stderr.read().decode())
        time.sleep(.02)
    raise AssertionError("service startup timeout")
def subject(uid):
    def isolate():
        os.setgroups([]); os.setgid(uid); os.setuid(uid)
    return isolate
client = r"""
import json,pathlib,socket,sys,time
sock,secret,config,mode=sys.argv[1:]
for path in [secret,config]:
    try: pathlib.Path(path).read_bytes()
    except PermissionError: pass
    else: raise AssertionError('model could read secret fixture')
request={'profile':'dev-test-accounts-v1','audience':'oasis7-dev-test-tools','method':'capabilities'}
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
    assert json.loads(response)['status']=='local_request_denied'
else: print(response.decode())
"""
def call(uid, mode="normal"):
    result=subprocess.run([sys.executable,"-c",client,str(public/"tools.sock"),str(secret),str(config_path),mode],preexec_fn=subject(uid),capture_output=True,text=True,timeout=5)
    assert result.returncode==0,result.stderr
    return json.loads(result.stdout) if result.stdout.strip() else None
try:
    start(["dev-test-serve","--profile",str(config_path)],private/"service.sock")
    start(["dev-test-local-serve","--socket",str(public/"tools.sock"),"--profile",str(config_path),"--caller-uid","1001","--caller-id","model"],public/"tools.sock")
    assert (result := call(1001))['status']=='ok', result
    assert call(1002)['status']=='local_request_denied'
    assert call(1001,'wrong-audience')['status']=='wrong_audience'
    assert call(1001,'wrong-version')['status']=='unsupported_profile'
    call(1001,'slowdrip')
    # A subsequent independently connected request still succeeds after the timed out request.
    assert (result := call(1001))['status']=='ok', result
    config['credentials'][0]['revoked']=True
    config_path.write_text(json.dumps(config))
    assert call(1001)['status']=='local_request_denied'
    config['credentials'][0]['revoked']=False
    config['credentials'][0]['expires_at_unix']=int(time.time())-1
    config_path.write_text(json.dumps(config))
    assert call(1001)['status']=='local_request_denied'
    print('PASS: distinct UID success/denial, private fixtures unreadable, wrong purpose/version, slowdrip/reconnect, revocation/expiry')
finally:
    for process in reversed(processes):
        process.terminate()
        try: process.wait(timeout=3)
        except subprocess.TimeoutExpired: process.kill(); process.wait()
    shutil.rmtree(root)
