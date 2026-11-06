"""Real Linux UID/stdio transport; synthetic backend only, never world requests."""
import json, os, pathlib, shutil, socket, subprocess, sys, tempfile, threading, time
binary = str(pathlib.Path(sys.argv[1]).resolve())
root = pathlib.Path(tempfile.mkdtemp(prefix='oasis7-business-', dir='/run')); root.chmod(0o755)
private = root/'private'; private.mkdir(mode=0o700)
public = root/'public'; public.mkdir(mode=0o755)
policy = private/'policy.json'; backend_path = private/'backend.sock'
token = 'synthetic-private-token-01234567890123456789'
config = dict(service=dict(profile='dev-test-accounts-v1',socket=str(backend_path),usage_directory=str(private/'usage'),grant_id='fixture',max_request_bytes=65536,timeout_ms=1000,max_calls=100,credentials=[dict(caller_id='model',token=token,expires_at_unix=int(time.time())+120,revoked=False)]),catalog_directory=str(private/'catalog'),grants=[],resources={})
def save():
    policy.write_text(json.dumps(config)); policy.chmod(0o600)
save(); records=[]; stop=False
listener=socket.socket(socket.AF_UNIX); listener.bind(str(backend_path)); backend_path.chmod(0o600); listener.listen(); listener.settimeout(.1)
def backend():
    while not stop:
        try: stream,_=listener.accept()
        except socket.timeout: continue
        with stream:
            body=b''
            while chunk:=stream.recv(4096): body+=chunk
            value=json.loads(body); assert value.pop('credential')==token; records.append(value)
            stream.sendall(json.dumps(dict(status='ok',result=dict(status='unknown',echo=value))).encode())
thread=threading.Thread(target=backend);thread.start()
def identity(uid):
    def enter(): os.setgroups([]);os.setgid(uid);os.setuid(uid)
    return enter
process=subprocess.Popen([binary,'serve',str(public/'tools.sock'),str(policy),'1001','model'],stderr=subprocess.PIPE)
def call(value,uid=1001,stdio=False):
    command=[binary,'stdio' if stdio else 'call',str(public/'tools.sock')]
    data=json.dumps(value)+'\n'
    result=subprocess.run(command,input=data,text=True,capture_output=True,preexec_fn=identity(uid),timeout=4)
    assert token not in result.stdout+result.stderr
    return result
try:
    for _ in range(100):
        if (public/'tools.sock').exists():break
        if process.poll() is not None:raise AssertionError(process.stderr.read().decode())
        time.sleep(.02)
    for method in ['accounts','open','close','describe','task','capabilities','context','move','lookup']:
        value=dict(method=method,session_ref='A')
        if method in ['context','move','lookup']:
            args={'kind':method}
            if method=='move':args.update(context_request_id='context-A',to=[1,0,0])
            if method=='lookup':args['original_request_id']='move-A'
            value['operation']=dict(request_id=method+'-A',arguments=args)
        result=call(value); assert result.returncode==0,result.stderr
        assert json.loads(result.stdout)['result']['echo']==value
    assert len(records)==9
    denied=call(dict(method='accounts'),1002); assert not records[9:]
    assert denied.returncode!=0 or json.loads(denied.stdout)['status']=='local_request_denied'
    assert call(dict(method='accounts',credential='forbidden')).returncode!=0
    request=dict(jsonrpc='2.0',id=7,method='tools/call',params=dict(name='dev_test_accounts_call',arguments=dict(method='lookup',session_ref='A',operation=dict(request_id='lookup-A',arguments=dict(kind='lookup',original_request_id='move-A')))))
    reply=call(request,stdio=True);assert reply.returncode==0,reply.stderr
    assert 'unknown' in json.loads(reply.stdout)['result']['content'][0]['text'];assert len(records)==10
    for mutation in ['revoked','expired']:
        config['service']['credentials'][0]['revoked']=mutation=='revoked'
        if mutation=='expired':config['service']['credentials'][0]['expires_at_unix']=1
        save(); assert json.loads(call(dict(method='accounts')).stdout)['status']=='local_request_denied';assert len(records)==10
    check=subprocess.run([sys.executable,'-c','import pathlib,sys; pathlib.Path(sys.argv[1]).read_bytes()',str(policy)],preexec_fn=identity(1001),capture_output=True)
    assert check.returncode!=0
    print('PASS: nine closed methods, exact IDs/body, stdio unknown, cross-UID denial, secret rejection, expiry/revocation, private policy inaccessible; synthetic backend only')
finally:
    process.terminate();process.wait(timeout=3);stop=True;thread.join(timeout=2);listener.close();shutil.rmtree(root)
