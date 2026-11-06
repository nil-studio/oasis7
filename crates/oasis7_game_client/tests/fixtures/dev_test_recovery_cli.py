"""Private synthetic CLI recovery: real operator/session processes, no native/world calls.
Usage: script RECOVERY_BINARY SESSION_BINARY. Real original proofs need separate evidence.
"""
import hashlib, json, os, pathlib, select, shutil, socket, subprocess, sys, tempfile, time
operator,server=[str(pathlib.Path(p).resolve()) for p in sys.argv[1:]]
root=pathlib.Path(tempfile.mkdtemp(prefix='.r5537-',dir=str(pathlib.Path.home())));root.chmod(0o700)
source=root/'source';source.mkdir(mode=0o700)
for name in ['usage','catalog']:(source/name).mkdir(mode=0o700)
now=int(time.time());old_token='synthetic-old-token-01234567890123456789'
config=dict(recovery_required=False,service=dict(profile='dev-test-accounts-v1',socket=str(source/'s'),usage_directory=str(source/'usage'),grant_id='old-grant',max_request_bytes=4096,timeout_ms=1000,max_calls=8,credentials=[dict(caller_id='caller',token=old_token,expires_at_unix=now+120,revoked=False)]),catalog_directory=str(source/'catalog'),grants=[dict(grant_ref='old-ref',grant_id='old-grant',caller_id='caller',binding=dict(account_ref='a',environment='e',world='w',genesis_digest='g',agent_id='agent',resource='r',executor_config_digest='pin',authorization_version=1),expires_at_unix=now+120,enabled=True,methods=['lookup'],move_targets=[])],resources={})
def write(path,value):path.write_text(json.dumps(value));path.chmod(0o600)
write(source/'policy.json',config)
write(source/'catalog/catalog.json',[dict(account_ref='a',account_id='synthetic-account',environment='e',public_key='synthetic-key',alias='A',credential_ref='/unused-native',login_operation='unused')])
processes=[]
def invoke(args,ok=True):
    result=subprocess.run([operator]+list(map(str,args)),capture_output=True,text=True,timeout=35)
    assert old_token not in result.stdout+result.stderr
    if ok:assert result.returncode==0,result.stderr
    else:assert result.returncode!=0
    return result
def start(policy):
    process=subprocess.Popen([server,'--serve',str(policy)],stdin=subprocess.PIPE,stdout=subprocess.DEVNULL,stderr=subprocess.PIPE);processes.append(process)
    process.stdin.write(b'{}');process.stdin.close()
    assert select.select([process.stderr],[],[],4)[0]
    assert b'read_session_service_ready' in process.stderr.readline()
    return process
def stop(process,policy):
    process.terminate();process.wait(timeout=3);path=pathlib.Path(json.loads(policy.read_text())['service']['socket']);path.unlink(missing_ok=True)
def call(policy,token,**request):
    path=json.loads(policy.read_text())['service']['socket'];stream=socket.socket(socket.AF_UNIX);stream.settimeout(3);stream.connect(path)
    stream.sendall(json.dumps(dict(credential=token,**request)).encode());stream.shutdown(socket.SHUT_WR);body=b''
    while chunk:=stream.recv(4096):body+=chunk
    stream.close();assert token.encode() not in body;return json.loads(body)
try:
    active=start(source/'policy.json')
    assert call(source/'policy.json',old_token,method='accounts')['status']=='ok'
    invoke(['backup',source/'policy.json',root/'snapshot'])
    assert call(source/'policy.json',old_token,method='accounts')['status']=='ok'
    source_usage=[p for p in (source/'usage').glob('*.json')][0]
    assert json.loads(source_usage.read_text())['used']==2
    invoke(['restore',root/'snapshot',root/'restored'])
    restored=root/'restored/policy.json';restored_config=json.loads(restored.read_text());assert restored_config['recovery_required'] is True
    restored_process=start(restored)
    assert call(restored,old_token,method='accounts')['status']=='unauthorized'
    revival=json.loads(restored.read_text());revival['service']['credentials'][0]['revoked']=False;revival['grants'][0]['enabled']=True
    write(root/'revive.json',revival)
    denied=subprocess.run([server,'--apply',str(restored),str(root/'revive.json')],capture_output=True,timeout=4);assert denied.returncode!=0
    stop(restored_process,restored)
    state_path=root/'restored/usage/recovery-state.json';state=json.loads(state_path.read_text());state['restore_pending']=True;write(state_path,state)
    interrupted=subprocess.run([server,'--serve',str(restored)],input=b'{}',capture_output=True,timeout=4);assert interrupted.returncode!=0
    invoke(['restore',root/'snapshot',root/'restored'])
    new=json.loads(restored.read_text());new_token='synthetic-new-token-01234567890123456789'
    new['service']['grant_id']='explicit-new-grant';new['service']['max_calls']=4;new['service']['credentials'][0].update(token=new_token,revoked=False)
    new['grants'][0].update(grant_ref='explicit-new-ref',grant_id='explicit-new-grant',enabled=True)
    write(root/'candidate.json',new)
    write(root/'review.json',dict(current_identity_and_permissions_checked=True,effects_and_pending_originals_checked=True,missing_usage_is_unknown=True))
    invoke(['reauthorize',restored,root/'candidate.json',root/'review.json'])
    resumed=start(restored)
    assert call(restored,new_token,method='accounts')['status']=='ok'
    assert call(restored,new_token,method='close',session_ref='old-epoch-reference')['status']=='session_unavailable'
    assert call(restored,old_token,method='accounts')['status']=='unauthorized'
    assert call(restored,new_token,method='accounts')['status']=='ok'
    ledgers=[p for p in (root/'restored/usage').glob('*.json') if p.name!='recovery-state.json'];assert len(ledgers)==1 and json.loads(ledgers[0].read_text())['used']==2
    assert json.loads(source_usage.read_text())['used']==2
    history=list((root/'restored/history').glob('*.json'));assert len(history)==1 and json.loads(history[0].read_text())['used']==1
    print('PASS: actual private operator/session CLI, old authorization/enable denied, pending restart denied/resume, explicit new grant billed separately, old usage history not current, old epoch rejected; synthetic metadata only')
finally:
    for process in processes:
        if process.poll() is None:process.terminate();process.wait(timeout=3)
    shutil.rmtree(root)
