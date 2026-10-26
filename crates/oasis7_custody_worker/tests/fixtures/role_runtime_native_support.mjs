// Node-only setup for negative tests around actual Rust/native/workerd components.
import assert from 'node:assert/strict';
import {spawn, execFileSync} from 'node:child_process';
import {once} from 'node:events';
import {existsSync, readFileSync, writeFileSync, mkdtempSync, rmSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {dirname, resolve} from 'node:path';
import {fileURLToPath} from 'node:url';
import {setTimeout as delay} from 'node:timers/promises';
import {Miniflare, convertV4MiniflareOptions, Response as MiniflareResponse} from 'miniflare';
const root=resolve(dirname(fileURLToPath(import.meta.url)),'../..');
const world={world_id:'world-one',genesis_digest:'genesis-one',branch_id:'branch-one',reorg_epoch:'0',service_public_key:'ab'.repeat(32)};
const routes={environment:'test',identity_route:'native-one',world_route:'world-route-one',world_identity:world.world_id};
function roleBindings(role, wrongKek=false) {
  return {CUSTODY_PROVISION_KEYS_ENABLED:'true',CUSTODY_ROLE_KEY_ENVIRONMENT:'test',CUSTODY_SERVICE_EPOCH:'1',CUSTODY_ROLE_WRAPPING_KEY_VERSION:'1',
    CUSTODY_ROLE_KEY_KEK:Buffer.alloc(32,wrongKek?64:{identity:61,owner:62,executor:63}[role]).toString('hex')};
}
function options(persist, data, bridge, mode) {
  const native = mode.startsWith('native-');
  const nativeBindings = native ? {CUSTODY_NATIVE_PROVISION_ENABLED:'true',
    CUSTODY_NATIVE_PRIVATE_KEK:Buffer.alloc(32,94).toString('hex'),CUSTODY_NATIVE_PRIVATE_WRAPPING_KEY_VERSION:'1'} : {};
  const moduleOptions={modulesRoot:root,compatibilityDate:'2026-10-25',cf:false};
  const artifact=role=>[{type:'ESModule',path:resolve(root,`artifacts/${role}/entry.mjs`)},{type:'ESModule',path:resolve(root,`artifacts/${role}/index.js`)},{type:'CompiledWasm',path:resolve(root,`artifacts/${role}/index_bg.wasm`)}];
  return {...convertV4MiniflareOptions({workers:[{
    name:'identity',...moduleOptions,
    modules:[{type:'ESModule',path:resolve(root,'tests/fixtures/native_revocation_scheduler.mjs')},...artifact('identity')],
    bindings:{CUSTODY_NATIVE_ORIGIN:'https://localhost',CUSTODY_NATIVE_RP_ID:'localhost',CUSTODY_NATIVE_NAMESPACE:data.namespace,
      CUSTODY_NATIVE_INSTANCE_EPOCH:data.identity_instance_epoch,CUSTODY_NATIVE_ENVIRONMENT:'test',CUSTODY_NATIVE_AUDIENCE:data.audience,
      CUSTODY_NATIVE_APPROVAL_KEY_ID:data.key_id,CUSTODY_NATIVE_APPROVAL_PUBLIC_KEY:data.public_key,CUSTODY_NATIVE_ADAPTER_SECRET:data.adapter_secret,
      CUSTODY_NATIVE_REVOCATION_POLL_SECRET:data.revocation_poll_secret,CUSTODY_NATIVE_REVOCATION_ACK_SECRET:data.revocation_ack_secret,
      CUSTODY_PROVISION_APPROVAL_ENABLED:'true',...roleBindings('identity',mode==='wrong-kek'),...nativeBindings,...(mode==='lost'?{FIXTURE_DROP_ROLE_RESPONSE:'true'}:{})},
    outboundService:bridge,
    serviceBindings:{OWNER_REVOCATIONS:{name:'owner',entrypoint:'OwnerRevocations'},OWNER_MANAGEMENT:{name:'owner',entrypoint:'OwnerManagement'},
      ACCOUNT_KEY_PERMITS:((mode.startsWith('wrong-')&&mode!=='wrong-kek')||mode==='freeze-after-seal')?{name:'oracle-proxy',entrypoint:'IdentityPermitProxy'}:{name:'owner',entrypoint:'IdentityKeyPermits'},
      RECOVERY_FENCE:{name:'fence',entrypoint:'RecoveryFenceReader'},
      ...(native?{OWNER_NATIVE_PERMITS:{name:'owner',entrypoint:'OwnerNativePermits'}}:{})},
    durableObjects:{SESSIONS:{className:'NativeSession',useSQLite:true},NATIVE_SESSIONS:{className:'NativeSession',useSQLite:true},ROLE_KEYS:{className:'RoleKeyVault',useSQLite:true}},
  },{
    name:'owner',...moduleOptions,modules:[{type:'ESModule',path:resolve(root,'tests/fixtures/native_owner_result_loss.mjs')},...artifact('owner')],
    bindings:{CUSTODY_NATIVE_APPROVAL_KEY_ID:data.key_id,CUSTODY_NATIVE_APPROVAL_PUBLIC_KEY:data.public_key,CUSTODY_NATIVE_AUDIENCE:data.audience,
      CUSTODY_REVOCATION_OWNER_KEY_ID:'owner-source-test',CUSTODY_REVOCATION_OWNER_SIGNING_KEY:Buffer.alloc(32,92).toString('hex'),
      CUSTODY_NATIVE_NAMESPACE:data.namespace,CUSTODY_NATIVE_INSTANCE_EPOCH:data.identity_instance_epoch,CUSTODY_NATIVE_ENVIRONMENT:'test',CUSTODY_POLICY_VERSION:'1',
      ...(mode==='native-lost-account-result'?{FIXTURE_DROP_NATIVE_RESULT:'true'}:{}),
      ...(mode==='native-lost-missing-result'?{FIXTURE_DROP_MISSING_RESULT:'true'}:{}),
      CUSTODY_BOOTSTRAP_MANAGERS:JSON.stringify([{principal:{identity_namespace:data.namespace,native_account_id:'a'},environments:['test']}]),
      CUSTODY_PROVISION_ROUTES:JSON.stringify(routes),CUSTODY_PROVISION_WORLD_IDENTITY:JSON.stringify(world),CUSTODY_PROVISION_APPROVAL_ENABLED:'true',...roleBindings('owner'),
      ...(native?{CUSTODY_NATIVE_PROVISION_ENABLED:'true',CUSTODY_NATIVE_ORIGIN:'https://localhost',
        CUSTODY_NATIVE_BINDING_OWNER_PIN:JSON.stringify(data.managed_binding_owner_pin),CUSTODY_NATIVE_BINDING_SIGNING_SEED:Buffer.alloc(32,93).toString('hex')}:{})},
    serviceBindings:{IDENTITY_NATIVE_APPROVAL:{name:'identity',entrypoint:'IdentityNativeApproval'},RECOVERY_FENCE:{name:'fence',entrypoint:'RecoveryFenceReader'},
      ACCOUNT_KEY_PERMITS:{name:'owner',entrypoint:'OwnerKeyPermits'},IDENTITY_PROVISION_KEYS:{name:'identity',entrypoint:'RoleKeyProvision'},
      OWNER_PROVISION_KEYS:{name:'owner',entrypoint:'RoleKeyProvision'},EXECUTOR_PROVISION_KEYS:{name:'executor',entrypoint:'RoleKeyProvision'},
      ...(native?{IDENTITY_NATIVE_PROVISION:{name:'identity',entrypoint:'IdentityNativeProvision'},
        IDENTITY_NATIVE_HISTORY:{name:'identity',entrypoint:'NativeProvisionHistory'},
        IDENTITY_NATIVE_STATUS_RECOVERY:{name:'identity',entrypoint:'NativeProvisionRecovery'}}:{})},
    durableObjects:{ACCOUNTS:{className:'AccountAuthority',useSQLite:true},MANAGER_ASSOCIATIONS:{className:'ManagerAssociations',useSQLite:true},ROLE_KEYS:{className:'RoleKeyVault',useSQLite:true}},
  },{
    name:'executor',...moduleOptions,modules:artifact('executor'),bindings:roleBindings('executor'),
    serviceBindings:{ACCOUNT_KEY_PERMITS:{name:'owner',entrypoint:'ExecutorKeyPermits'},RECOVERY_FENCE:{name:'fence',entrypoint:'RecoveryFenceReader'}},
    durableObjects:{ROLE_KEYS:{className:'RoleKeyVault',useSQLite:true}},
  },{
    name:'fence',...moduleOptions,modules:[{type:'ESModule',path:resolve(root,'tests/fixtures/provision_fence_fixture.mjs')}],
  },{
    name:'oracle-proxy',...moduleOptions,modules:[{type:'ESModule',path:resolve(root,'tests/fixtures/role_runtime_oracle_proxy.mjs')}],bindings:{FIXTURE_MODE:mode},
    serviceBindings:{IDENTITY_REAL:{name:'owner',entrypoint:'IdentityKeyPermits'},EXECUTOR_REAL:{name:'owner',entrypoint:'ExecutorKeyPermits'},EXECUTOR_DEFAULT:'executor',FENCE:'fence'},
  }]}),resourcePersistencePath:persist,unsafeDevRegistryPath:resolve(persist,'registry')};
}
async function authority(directory, nativeMode=false) {
  const ready=resolve(directory,'native-ready.json'), stop=resolve(directory,'native-stop');
  const binary=readFileSync(resolve(root,'artifacts/native-test-binary.txt'),'utf8').trim();
  const child=spawn(binary,['serve_native_identity_http_fixture','--ignored','--nocapture'],{
    env:{...Object.fromEntries(Object.entries(process.env).filter(([name])=>!name.startsWith('OASIS7_'))),OASIS7_CUSTODY_NATIVE_HTTP_READY:ready,OASIS7_CUSTODY_NATIVE_HTTP_STOP:stop,
      ...(nativeMode?{OASIS7_CUSTODY_NATIVE_HTTP_MANAGED_BINDING:'true'}:{})},stdio:'ignore'});
  let exited=false;
  const exit=once(child,'exit').then(()=>{exited=true;});
  const close=async()=>{writeFileSync(stop,'stop');await Promise.race([exit,delay(5000,undefined,{ref:false})]);if(!exited){child.kill('SIGTERM');await exit;}};
  for(let attempt=0;attempt<300;attempt++) {
    if(exited) throw new Error('isolated native fixture exited before readiness');
    if(existsSync(ready)) {try{return {data:JSON.parse(readFileSync(ready,'utf8')),close};}catch{/* bounded ready file may still be completing */}}
    await delay(20);
  }
  await close();throw new Error('isolated native fixture did not become ready');
}
export async function withRoleRuntime(mode, run) {
  const directory=mkdtempSync(resolve(tmpdir(),'oasis7-role-runtime-negative-'));
  let native,mf;
  try {
    const nativeMode=mode.startsWith('native-');
    native=await authority(directory,nativeMode);
    const persist=resolve(directory,'workerd');
    let nativeVerifications=0;
    const nativeRequests={};
    let lostBindingReply=false;
    let lostLoginReply=false;
    const bridge=async request=>{
      const url=new URL(request.url);assert.equal(url.origin,'https://localhost');
      const suffix=url.pathname.replace('/api/public/hosted-account/v2','');
      const management=['/custody/management-session-status','/custody/approvals/verify'].includes(suffix);
      assert.ok(management || nativeMode && ['/capabilities','/machine-registration/options','/machine-registration/complete','/native-key/options','/native-key/complete',
        '/custody/managed-bindings/options','/custody/managed-bindings/complete','/custody/managed-bindings/lookup','/custody/managed-bindings/session-status'].includes(suffix));
      nativeRequests[suffix]=(nativeRequests[suffix]??0)+1;
      if(!management) {
        assert.equal(request.headers.get('authorization'),null,'ordinary native transport has no approval adapter credential');
        assert.notEqual(request.headers.get('cookie'),native.data.cookie,'manager cookie never becomes a managed account credential');
      }
      if(url.pathname.endsWith('/verify')) nativeVerifications++;
      const headers=new Headers();for(const name of ['authorization','cookie','origin','sec-fetch-site','x-oasis7-csrf','content-type']) {const value=request.headers.get(name);if(value!==null) headers.set(name,value);}
      const response=await fetch(native.data.address+url.pathname,{method:request.method,headers,...(request.method==='GET'?{}:{body:Buffer.from(await request.arrayBuffer())}),redirect:'manual'});
      if(mode==='native-lost-binding' && suffix==='/custody/managed-bindings/complete' && response.status===200 && !lostBindingReply) {
        await response.arrayBuffer(); lostBindingReply=true;
        throw new Error('fixture lost binding response after native SQL commit');
      }
      if(['native-lost-login','native-lost-missing-result'].includes(mode) && suffix==='/native-key/complete' && response.status===200 && !lostLoginReply) {
        await response.arrayBuffer(); lostLoginReply=true;
        throw new Error('fixture lost original login Cookie after native SQL commit');
      }
      const outputHeaders=new Headers({'Content-Type':'application/json'});
      if(nativeMode) for(const cookie of response.headers.getSetCookie()) outputHeaders.append('Set-Cookie',cookie);
      return new MiniflareResponse(await response.text(),{status:response.status,headers:outputHeaders});
    };
    const restart=async nextMode=>{await mf?.dispose();mf=new Miniflare(options(persist,native.data,bridge,nextMode));};
    await restart(mode);
    const headers={Cookie:native.data.cookie,Origin:'https://localhost','Content-Type':'application/json','X-Oasis7-CSRF':native.data.csrf};
    const draft={api_version:1,client_request_id:'negative-runtime',plan:{...routes,purpose:'isolated actual runtime negative cases',expires_at:String(Date.now()+(mode==='native-lost-login'?6000:180000)),transport_limit:'400'}};
    let identity=await mf.getWorker('identity');
    const response=await identity.fetch('https://localhost/v1/account-provisions',{method:'POST',headers,body:JSON.stringify(draft)});
    assert.equal(response.status,200);const original=await response.json();
    const request={approval_request_id:'negative-runtime-approval',context:{environment:'test',custody_account_id:original.entry.custody_account_id,audience:native.data.audience,policy_version:'1',security_version:'0',service_epoch:'1'},
      intent:{kind:'provision',provision_id:original.entry.provision_id,purpose:draft.plan.purpose,plan_digest:original.entry.plan_digest,bootstrap_config_version:'1',world_identity:world,identity_route:routes.identity_route,world_route:routes.world_route,
        native_identity_namespace:native.data.namespace,register_request_id:'negative-runtime-register',login_request_id:'negative-runtime-login',deadline:draft.plan.expires_at,transport_limit:'400'}};
    const completed=await (await fetch(native.data.address+'/_fixture/approve-intent',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(request)})).json();
    assert.ok(completed.approval,'actual isolated native UV must sign the explicit test plan');
    const base='https://localhost/v1/account-provisions/'+original.entry.provision_id;
    const accepted=await identity.fetch(base+'/approve',{method:'POST',headers,body:JSON.stringify({api_version:1,input:{operation_id:'negative-runtime-approval-operation',request,approval:completed.approval}})});
    assert.equal(accepted.status,200);
    await run({persist,account:original.entry.custody_account_id,planExpires:Number(draft.plan.expires_at),restart,suspend:async()=>{await mf?.dispose();mf=undefined;},
      approveManaged:async(blocked,limit='64')=>{
        const saved=persistedRuntime(persist,original.entry.custody_account_id);
        assert.ok(saved.native_binding);
        const request={approval_request_id:'fresh-managed-approval-one',context:{...completed.approval.proof.context,security_version:'1'},
          intent:{kind:'managed_login',native_account_id:saved.native_binding.native_account_id,login_key_ref:saved.native_binding.login_key_ref,
            native_identity_namespace:saved.native_binding.identity_namespace,blocked_operation_ref:blocked,login_request_id:'fresh-managed-login-one',transport_limit:limit,deadline:String(Date.now()+60000)}};
        const proofReply=await fetch(native.data.address+'/_fixture/approve-intent',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(request)});
        assert.equal(proofReply.status,200);const proof=await proofReply.json();assert.ok(proof.approval);
        identity=await mf.getWorker('identity');
        const reply=await identity.fetch(base+'/managed-logins/approve',{method:'POST',headers,body:JSON.stringify({api_version:1,input:{operation_id:'fresh-managed-operation-one',request,approval:proof.approval}})});
        assert.equal(reply.status,200);return reply.json();
      },
      continueManaged:async operation=>{identity=await mf.getWorker('identity');const reply=await identity.fetch(base+'/managed-logins/continue',{method:'POST',headers,body:JSON.stringify({api_version:1,operation_id:operation})});assert.equal(reply.status,200);return reply.json();},
      drive:async operation=>{identity=await mf.getWorker('identity');const reply=await identity.fetch(base+'/continue',{method:'POST',headers,body:JSON.stringify({api_version:1,operation_id:operation})});assert.equal(reply.status,200);return reply.json();},
      driveResponse:async operation=>{identity=await mf.getWorker('identity');return identity.fetch(base+'/continue',{method:'POST',headers,body:JSON.stringify({api_version:1,operation_id:operation})});},
      freezeFence:async()=>{const reply=await (await mf.getWorker('fence')).fetch('https://fixture.test/freeze');assert.equal(reply.status,200);},
      oracle:async(role,body)=>(await mf.getWorker('oracle-proxy')).fetch('https://fixture.test/'+role+'-permit',{method:'POST',body:JSON.stringify(body)}),
      defaultExecutor:async(kind,body)=>(await mf.getWorker('oracle-proxy')).fetch('https://fixture.test/executor-default-'+kind,{method:'POST',body:JSON.stringify(body)}),
      nativeCalls:()=>nativeVerifications,nativeRequests:()=>({...nativeRequests})});
  } finally {await mf?.dispose();await native?.close();rmSync(directory,{recursive:true,force:true});}
}

// Only non-secret facts leave the isolated SQLite files; no envelope/seed/session
// is printed. Corruption is injected only while the owning workerd is disposed.
export function persistedRuntime(persist, account, corruptIdentity=false) {
  return JSON.parse(execFileSync('python3',['-c',`
import hashlib,json,sqlite3,sys
from pathlib import Path
result={'slots':[],'dispatches':[],'inspections':0,'native_results':[],'native_reads':[],'native_binding':None,'cipher_records':0,'history_count':0,'status_recoveries':[]}
for p in Path(sys.argv[1]).rglob('*.sqlite'):
 try:
  db=sqlite3.connect(p.as_uri()+('' if sys.argv[3]=='yes' else '?mode=ro'),uri=True)
  row=db.execute("SELECT payload FROM custody_records WHERE id='custody-role-key-slot-v1'").fetchone()
  if row:
   value=json.loads(row[0]);slot=value['slot']
   if slot and slot['scope']['context']['identity']['account_id']==sys.argv[2]:
    role=slot['scope']['context']['identity']['role']
    if sys.argv[3]=='yes' and role=='identity':
     text=slot['envelope']['ciphertext'];slot['envelope']['ciphertext']=('1' if text[0]=='0' else '0')+text[1:]
     payload=json.dumps(value,separators=(',',':'));db.execute("UPDATE custody_records SET payload=? WHERE id='custody-role-key-slot-v1'",(payload,));db.commit()
    else: payload=row[0]
    result['slots'].append({'role':role,'public_key':slot['public_ref']['public_key'],'payload_digest':hashlib.sha256(payload.encode()).hexdigest()})
  row=db.execute("SELECT payload FROM custody_records WHERE id='custody-provision-approval-v1'").fetchone()
  if row:
   state=json.loads(row[0])['state']
   if state['draft']['entry']['custody_account_id']==sys.argv[2]:
    result['inspections']=len(state['origin_inspections'])
    result['dispatches']=[{'role':d['scope']['context']['identity']['role'],'reads':d['permit_reads'],'expires_at':d['expires_at'],'request':json.loads(d['request_bytes']),'has_result':d['result'] is not None} for d in state['role_key_dispatches'].values()]
    result['native_results']=[v['result']['kind'] for v in state.get('native_stages',{}).values() if v.get('result') is not None]
    result['native_reads']=[{'stage':v['request']['stage'],'reads':v['reads']} for v in state.get('native_stages',{}).values()]
    result['history_count']=len(state.get('native_history',{}))
    result['status_recoveries']=[{'reads':v['reads'],'has_result':v['result'] is not None,'nonce':v['nonce']} for v in state.get('session_status_recoveries',{}).values()]
    for v in state.get('native_stages',{}).values():
     if (v.get('result') or {}).get('kind')=='binding_receipt':
      receipt=v['result']['result']['receipt'];result['native_binding']={k:receipt[k] for k in ('native_account_id','login_key_ref','identity_namespace')}
  row=db.execute("SELECT payload FROM custody_records WHERE id='custody-native-cipher-v1'").fetchone()
  if row:
   cipher=json.loads(row[0])
   if cipher.get('envelope') is not None:
    assert '"cookie"' not in row[0] and '"csrf"' not in row[0], 'private records must retain envelopes only'
    result['cipher_records']+=1
  db.close()
 except sqlite3.Error: pass
print(json.dumps(result))
`,persist,account,corruptIdentity?'yes':'no'],{encoding:'utf8'}));
}
