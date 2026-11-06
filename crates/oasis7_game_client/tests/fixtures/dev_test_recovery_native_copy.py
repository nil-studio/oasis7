"""Trusted private metadata-copy recovery; only task and original lookup, never Context/Move.
Run in the existing isolated fixture, not against production. Native keys/world/cache stay current.
"""
import base64, copy, hashlib, json, os, pathlib, secrets, select, socket, subprocess, sys, time

def main():
    assert len(sys.argv)==6 and sys.argv[5]=='--execute-trusted-copy'
    root=pathlib.Path(sys.argv[1]);operator,server,native=sys.argv[2:5]
    work=root/('recovery-5537-'+secrets.token_hex(6));work.mkdir(mode=0o700)
    private_result=work/'result.json';processes=[];step='prepare'
    unlock=(root/'unlock.bin').read_bytes()
    env=os.environ.copy();env['SSL_CERT_FILE']=json.loads((root/'runtime-private.json').read_text())['environment']['SSL_CERT_FILE']
    result=dict(real_external_agent=False,world_action_sent=False,source_head='ea49a254dff312cfc00d9c61596f217e82e96884',dependency_head='60e12bf9294137f1c6ecdf445ae7bbd372273214',max_calls=16,max_inflight=1,timeout_ms=30000,source_root=str(root/'write-service-5536'),private_directory=str(work))
    def write(path,value):path.write_text(json.dumps(value));path.chmod(0o600)
    def invoke(args):
        completed=subprocess.run([operator]+list(map(str,args)),capture_output=True,timeout=40)
        assert completed.returncode==0,'operator_failed_'+step
    def native_read(config,command):
        completed=subprocess.run([native,'--executor-config',str(config)]+command,input=unlock,capture_output=True,env=env,timeout=35)
        if completed.returncode:return None
        return json.loads(completed.stdout)
    def start(policy):
        config=json.loads(policy.read_text());mapping={g['binding']['account_ref']:base64.b64encode(unlock).decode('ascii') for g in config['grants']}
        process=subprocess.Popen([server,'--serve',str(policy)],stdin=subprocess.PIPE,stdout=subprocess.DEVNULL,stderr=subprocess.PIPE,env=env);processes.append(process)
        process.stdin.write(json.dumps(mapping).encode());process.stdin.close()
        assert select.select([process.stderr],[],[],5)[0] and b'read_session_service_ready' in process.stderr.readline(),'service_start_failed'
        return process
    def stop(process,policy):
        process.terminate();process.wait(timeout=4);pathlib.Path(json.loads(policy.read_text())['service']['socket']).unlink(missing_ok=True)
    def call(policy,credential,**request):
        stream=socket.socket(socket.AF_UNIX);stream.settimeout(35);stream.connect(json.loads(policy.read_text())['service']['socket'])
        stream.sendall(json.dumps(dict(credential=credential,**request)).encode());stream.shutdown(socket.SHUT_WR);body=b''
        while chunk:=stream.recv(4096):body+=chunk
        stream.close();return json.loads(body)
    def budgeted_candidate(policy,label):
        config=json.loads(policy.read_text());now=int(time.time());token=secrets.token_hex(32);grant_id='recovery-5537-'+label+'-'+secrets.token_hex(4);caller='trusted-recovery-reader'
        config['service'].update(grant_id=grant_id,max_calls=16,timeout_ms=30000,max_request_bytes=8192,credentials=[dict(caller_id=caller,token=token,expires_at_unix=now+600,revoked=False)])
        for index,grant in enumerate(config['grants']):
            grant.update(grant_ref='recovery-'+label+'-'+str(index),grant_id=grant_id,caller_id=caller,expires_at_unix=now+540,enabled=True,methods=['describe','task','capabilities','lookup'],move_targets=[])
            grant['binding']['authorization_version']+=1
        candidate=policy.parent/('candidate-'+label+'.json');write(candidate,config)
        review=policy.parent/('review-'+label+'.json');write(review,dict(current_identity_and_permissions_checked=True,effects_and_pending_originals_checked=True,missing_usage_is_unknown=True))
        invoke(['reauthorize',policy,candidate,review]);return token,config
    def open_sessions(policy,token,config):
        sessions={}
        for grant in config['grants']:
            reply=call(policy,token,method='open',grant_ref=grant['grant_ref'],expires_at_unix=int(time.time())+180)
            assert reply['status']=='ok','current_read_scope_unavailable'
            sessions[grant['binding']['account_ref']]=reply['result']['session_ref']
        return sessions
    def usage(policy):
        directory=pathlib.Path(json.loads(policy.read_text())['service']['usage_directory']);records=[json.loads(p.read_text()) for p in directory.glob('*.json') if p.name!='recovery-state.json'];return sum(r['used'] for r in records)
    try:
        source=root/'write-service-5536/policy.json';source_config=json.loads(source.read_text())
        source_ledger_bytes={p.name:p.read_bytes() for p in pathlib.Path(source_config['service']['usage_directory']).glob('*.json')}
        original_records={}
        for data in source_ledger_bytes.values():original_records.update(json.loads(data).get('operations',{}))
        prepared=json.loads((root/'read-5535-result.json').read_text())['accounts'];world=prepared[0]['fence']['world_id'];genesis=prepared[0]['fence']['genesis_digest']
        result.update(world=world,genesis_digest=genesis,source_used=sum(json.loads(v)['used'] for v in source_ledger_bytes.values()),native_prechecks={})
        step='current_native_task_and_originals'
        moves=[];partial=[]
        for label,account in zip(['A','B'],prepared):
            fence=native_read(account['executor_config'],['task']);assert fence is not None,'current_native_task_unavailable'
            assert fence==account['fence'],'current_native_fence_changed'
            candidates=[(rid,record) for rid,record in original_records.items() if record['scope']['account_ref']==account['account_ref'] and record['arguments']['kind']=='move']
            assert candidates,'known_move_original_missing'
            rid,record=sorted(candidates)[0]
            receipt=native_read(account['executor_config'],['lookup','--context',record['native_context'],'--kind','action']);assert receipt is not None,'current_native_original_unavailable'
            status=receipt['outcome']['status'];assert status==('committed' if label=='A' else 'rejected'),'native_original_outcome_changed'
            moves.append((label,account['account_ref'],rid,status));result['native_prechecks'][label]=dict(request_id=rid,native_context=record['native_context'],outcome=status)
            if label=='B':
                contexts=[(key,op) for key,op in original_records.items() if op['scope']['account_ref']==account['account_ref'] and op['arguments']['kind']=='context' and op.get('native_context_identity') is None]
                partial=sorted(contexts)
        # Current native reads above justify the operator review; no new context was made.
        step='original_metadata_snapshot';invoke(['backup',source,work/'original-snapshot']);invoke(['restore',work/'original-snapshot',work/'read-source'])
        read_policy=work/'read-source/policy.json';source_token,read_config=budgeted_candidate(read_policy,'source')
        read_process=start(read_policy);sessions=open_sessions(read_policy,source_token,read_config)
        source_used_before=usage(read_policy);assert source_used_before==len(read_config['grants'])
        step='active_read_profile_snapshot';invoke(['backup',read_policy,work/'active-read-snapshot'])
        assert call(read_policy,source_token,method='accounts')['status']=='ok';assert usage(read_policy)==source_used_before+1
        old_session=next(iter(sessions.values()));stop(read_process,read_policy)
        step='restore_retired_profile';invoke(['restore',work/'active-read-snapshot',work/'restored'])
        target=work/'restored/policy.json';inactive=start(target)
        assert call(target,source_token,method='accounts')['status']=='unauthorized';stop(inactive,target)
        # Ordinary apply cannot turn a restored authorization back on.
        revived=json.loads(target.read_text());revived['service']['credentials'][0]['revoked']=False
        for grant in revived['grants']:grant['enabled']=True
        write(work/'revive.json',revived)
        assert subprocess.run([server,'--apply',str(target),str(work/'revive.json')],capture_output=True,timeout=4).returncode!=0
        step='explicit_new_read_authorization';new_token,new_config=budgeted_candidate(target,'final');active=start(target);new_sessions=open_sessions(target,new_token,new_config)
        assert call(target,new_token,method='close',session_ref=old_session)['status']=='session_unavailable'
        result['recovered_originals']={}
        for label,account_ref,rid,expected in moves:
            reply=call(target,new_token,method='lookup',session_ref=new_sessions[account_ref],operation=dict(request_id='restore-lookup-'+label,arguments=dict(kind='lookup',original_request_id=rid)))
            assert reply['status']=='ok','historical_lookup_denied'
            observation=reply['result']['observation'];assert observation['outcome']['status']==expected,'historical_native_receipt_unavailable'
            result['recovered_originals'][label]=dict(original_request_id=rid,outcome=expected,correlation=observation['correlation'])
        if partial:
            rid,record=partial[-1];account_ref=record['scope']['account_ref']
            unknown=call(target,new_token,method='lookup',session_ref=new_sessions[account_ref],operation=dict(request_id='restore-partial-lookup',arguments=dict(kind='lookup',original_request_id=rid)))
            assert unknown['status']=='ok' and unknown['result'].get('status',unknown['result'].get('observation',{}).get('status'))=='unknown','partial_original_must_stay_unknown'
            result['partial_original']=dict(request_id=rid,status='unknown')
        result.update(status='pass',restored_source_used_in_snapshot=source_used_before,source_read_used_after_snapshot=usage(read_policy),new_grant_used=usage(target),old_authorizations_reactivated=False,old_epoch_rejected=True,native_cache_rolled_back=False)
        assert all(p.read_bytes()==source_ledger_bytes[p.name] for p in pathlib.Path(source_config['service']['usage_directory']).glob('*.json')),'original_tool_ledger_changed'
        result['source_ledger_unchanged']=True
    except Exception as failure:
        reason=str(failure) if isinstance(failure,AssertionError) else type(failure).__name__
        result.update(status='blocked',phase=step,reason=reason)
        raise
    finally:
        for process in processes:
            if process.poll() is None:process.terminate();process.wait(timeout=4)
        write(private_result,result)
        print(json.dumps({k:v for k,v in result.items() if k not in ['private_directory','source_root']}))
if __name__=='__main__':main()
