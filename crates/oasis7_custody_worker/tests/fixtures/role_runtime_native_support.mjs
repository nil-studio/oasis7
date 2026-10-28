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
import {fixtureRecoveryPolicy,signFixtureRecoveryActivation} from './fence_activation_signer.mjs';
import {worldAuthority} from './world_runtime_support.mjs';
const root=resolve(dirname(fileURLToPath(import.meta.url)),'../..');
const defaultWorld={world_id:'world-one',genesis_digest:'genesis-one',branch_id:'branch-one',reorg_epoch:'0',service_public_key:'ab'.repeat(32)};
const defaultRoutes={environment:'test',identity_route:'native-one',world_route:'world-route-one',world_identity:defaultWorld.world_id};
const realFenceMode=mode=>mode==='native-real-fence'||mode.startsWith('native-participant-real-fence')||mode.startsWith('native-participant-owner-setup-real-fence');
function roleBindings(role, wrongKek=false) {
  return {CUSTODY_PROVISION_KEYS_ENABLED:'true',CUSTODY_ROLE_KEY_ENVIRONMENT:'test',CUSTODY_SERVICE_EPOCH:'1',CUSTODY_ROLE_WRAPPING_KEY_VERSION:'1',
    CUSTODY_ROLE_KEY_KEK:Buffer.alloc(32,wrongKek?64:{identity:61,owner:62,executor:63}[role]).toString('hex')};
}
function options(persist, data, bridge, mode, configureOptions) {
  const native = mode.startsWith('native-');
  const world=data.world??defaultWorld;
  const routes={...defaultRoutes,world_identity:world.world_id};
  const realFence = realFenceMode(mode);
  const nativeBindings = native ? {CUSTODY_NATIVE_PROVISION_ENABLED:'true',
    ...(mode.startsWith('native-participant-owner-setup-real-fence-gap')?{FIXTURE_SETUP_STAGE_GAP:mode}:{}),
    ...(mode.startsWith('native-participant-owner-setup')?{CUSTODY_OWNER_SETUP_ENABLED:'true'}:{}),
    ...(mode.startsWith('native-participant')?{CUSTODY_PARTICIPANT_PROVISION_ENABLED:'true'}:{}),
    CUSTODY_NATIVE_PRIVATE_KEK:Buffer.alloc(32,94).toString('hex'),CUSTODY_NATIVE_PRIVATE_WRAPPING_KEY_VERSION:'1'} : {};
  const moduleOptions={modulesRoot:root,compatibilityDate:'2026-10-25',cf:false};
  const artifact=role=>[{type:'ESModule',path:resolve(root,`artifacts/${role}/entry.mjs`)},{type:'ESModule',path:resolve(root,`artifacts/${role}/index.js`)},{type:'CompiledWasm',path:resolve(root,`artifacts/${role}/index_bg.wasm`)}];
  const rawOptions={workers:[{
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
    name:'owner',...moduleOptions,modules:[{type:'ESModule',path:resolve(root,mode==='native-participant-owner-setup-real-fence-reconcile-rollback'?'tests/fixtures/setup_reconcile_audit_fault.mjs':mode==='native-participant-role-capability'?'tests/fixtures/participant_role_capability_probe.mjs':mode==='native-participant-audit-rollback'?'tests/fixtures/participant_audit_fault.mjs':'tests/fixtures/native_owner_result_loss.mjs')},...(mode==='native-participant-owner-setup-real-fence-reconcile-rollback'?[{type:'ESModule',path:resolve(root,'tests/fixtures/native_owner_result_loss.mjs')}]:[]),...artifact('owner')],
    bindings:{CUSTODY_NATIVE_APPROVAL_KEY_ID:data.key_id,CUSTODY_NATIVE_APPROVAL_PUBLIC_KEY:data.public_key,CUSTODY_NATIVE_AUDIENCE:data.audience,
      CUSTODY_REVOCATION_OWNER_KEY_ID:'owner-source-test',CUSTODY_REVOCATION_OWNER_SIGNING_KEY:Buffer.alloc(32,92).toString('hex'),
      CUSTODY_NATIVE_NAMESPACE:data.namespace,CUSTODY_NATIVE_INSTANCE_EPOCH:data.identity_instance_epoch,CUSTODY_NATIVE_ENVIRONMENT:'test',CUSTODY_POLICY_VERSION:'1',
      ...(mode==='native-lost-account-result'?{FIXTURE_DROP_NATIVE_RESULT:'true'}:{}),
      ...(mode.startsWith('native-participant-owner-setup-real-fence-reconcile')?{FIXTURE_SETUP_ARCHIVE_EXPIRY:mode.endsWith('-bind')?'bind':mode.endsWith('-final')?'final_observation':'create_task'}:{}),
      ...(mode==='native-lost-missing-result'?{FIXTURE_DROP_MISSING_RESULT:'true'}:{}),
      ...(mode.startsWith('native-stage-gap-')?{FIXTURE_STAGE_GAP:'true'}:{}),
      CUSTODY_BOOTSTRAP_MANAGERS:JSON.stringify([{principal:{identity_namespace:data.namespace,native_account_id:'a'},environments:['test']}]),
      CUSTODY_PROVISION_ROUTES:JSON.stringify(routes),CUSTODY_PROVISION_WORLD_IDENTITY:JSON.stringify(world),CUSTODY_PROVISION_APPROVAL_ENABLED:'true',...roleBindings('owner'),
      ...(mode.startsWith('native-participant')?{CUSTODY_PARTICIPANT_PROVISION_ENABLED:'true'}:{}),
      ...(mode.startsWith('native-participant-owner-setup')?{CUSTODY_OWNER_SETUP_ENABLED:'true'}:{}),
      ...(native?{CUSTODY_NATIVE_PROVISION_ENABLED:'true',CUSTODY_NATIVE_ORIGIN:'https://localhost',
        CUSTODY_NATIVE_BINDING_OWNER_PIN:JSON.stringify(data.managed_binding_owner_pin),CUSTODY_NATIVE_BINDING_SIGNING_SEED:Buffer.alloc(32,93).toString('hex')}:{})},
    ...(mode.startsWith('native-stage-gap-')?{outboundService:bridge}:{}),
    serviceBindings:{...(mode==='native-participant-role-capability'?{PARTICIPANT_IDENTITY_REAL:{name:'owner',entrypoint:'ParticipantIdentityReal'}}:{}),IDENTITY_NATIVE_APPROVAL:{name:'identity',entrypoint:'IdentityNativeApproval'},RECOVERY_FENCE:{name:'fence',entrypoint:'RecoveryFenceReader'},
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
    name:'fence',...moduleOptions,modules:realFence?artifact('recovery-fence'):[{type:'ESModule',path:resolve(root,'tests/fixtures/provision_fence_fixture.mjs')}],
    ...(realFence?{bindings:{CUSTODY_INITIAL_SERVICE_EPOCH:'0',CUSTODY_RECOVERY_CONTROL_ENABLED:'true',CUSTODY_RECOVERY_AUTHORITY_POLICY:JSON.stringify(fixtureRecoveryPolicy)},durableObjects:{FENCES:{className:'RecoveryFence',useSQLite:true}}}:{}),
  },{
    name:'oracle-proxy',...moduleOptions,modules:[{type:'ESModule',path:resolve(root,'tests/fixtures/role_runtime_oracle_proxy.mjs')}],bindings:{FIXTURE_MODE:mode},
    serviceBindings:{IDENTITY_REAL:{name:'owner',entrypoint:'IdentityKeyPermits'},EXECUTOR_REAL:{name:'owner',entrypoint:'ExecutorKeyPermits'},EXECUTOR_DEFAULT:'executor',FENCE:'fence'},
  },...(realFence?[{name:'fence-controller',...moduleOptions,modules:[{type:'ESModule',path:resolve(root,'tests/fixtures/fence_controller.mjs')}],serviceBindings:{CONTROL:{name:'fence',entrypoint:'RecoveryFenceControl'},READER:{name:'fence',entrypoint:'RecoveryFenceReader'}}}]:[])]};
  return {...convertV4MiniflareOptions(configureOptions?configureOptions(rawOptions):rawOptions),resourcePersistencePath:persist,unsafeDevRegistryPath:resolve(persist,'registry')};
}
async function authority(directory, nativeMode=false, worldData) {
  const ready=resolve(directory,'native-ready.json'), stop=resolve(directory,'native-stop');
  const binary=readFileSync(resolve(root,'artifacts/native-test-binary.txt'),'utf8').trim();
  const child=spawn(binary,['serve_native_identity_http_fixture','--ignored','--nocapture'],{
    env:{...Object.fromEntries(Object.entries(process.env).filter(([name])=>!name.startsWith('OASIS7_'))),OASIS7_CUSTODY_NATIVE_HTTP_READY:ready,OASIS7_CUSTODY_NATIVE_HTTP_STOP:stop,
      ...(nativeMode?{OASIS7_CUSTODY_NATIVE_HTTP_MANAGED_BINDING:'true'}:{}),
      ...(worldData?{OASIS7_CUSTODY_NATIVE_WORLD_TRUST:JSON.stringify(worldData)}:{})},stdio:'ignore'});
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
export async function withRoleRuntime(mode, run, {configureOptions}={}) {
  const directory=mkdtempSync(resolve(tmpdir(),'oasis7-role-runtime-negative-'));
  let native,mf,worldProcess;
  try {
    const nativeMode=mode.startsWith('native-');
    if(mode.startsWith('native-participant')) worldProcess=await worldAuthority(directory);
    native=await authority(directory,nativeMode,worldProcess?{...worldProcess.data,owner_setup:mode.startsWith('native-participant-owner-setup')}:undefined);
    const world=worldProcess?.data.world??defaultWorld;
    const routes={...defaultRoutes,world_identity:world.world_id};
    if(worldProcess) native.data.world=world;
    const persist=resolve(directory,'workerd');
    let nativeVerifications=0;
    let nativeUnavailable=false;
    const nativeRequests={};
    const approvalFacts=[];
    const worldResults=[];
    let lostBindingReply=false;
    let lostLoginReply=false;
    let lostParticipantReply=false;
    const bridge=async request=>{
      const url=new URL(request.url);
      if(mode.startsWith('native-participant-owner-setup-real-fence-reconcile') && url.href==='https://fixture.test/_after-setup-submit-archive') {
        await delay(5100);return new MiniflareResponse(null,{status:204});
      }
      if(mode.startsWith('native-participant-owner-setup-real-fence-gap') && url.href==='https://fixture.test/_after-setup-create-commit') {
        if(mode.endsWith('-revoke')) {
          const revoked=await fetch(native.data.address+'/_fixture/revoke-manager',{method:'POST'});
          assert.equal(revoked.status,200);
        }
        await delay(31000);
        return new MiniflareResponse(null,{status:204});
      }
      if(mode.startsWith('native-stage-gap-') && url.href==='https://fixture.test/_after-binding-commit') {
        if(mode==='native-stage-gap-revoke'||mode==='native-stage-gap-expire') {
          const path=mode==='native-stage-gap-revoke'?'revoke-manager':'expire';
          const invalidated=await fetch(native.data.address+'/_fixture/'+path,{method:'POST'});
          assert.equal(invalidated.status,200);
        }
        await new Promise(resolve=>setTimeout(resolve,5200));
        return new MiniflareResponse(null,{status:204});
      }
      assert.equal(url.origin,'https://localhost');
      const suffix=url.pathname.replace('/api/public/hosted-account/v2','');
      const management=['/custody/management-session-status','/custody/approvals/verify'].includes(suffix);
      assert.ok(management || nativeMode && ['/capabilities','/machine-registration/options','/machine-registration/complete','/native-key/options','/native-key/complete',
        '/custody/managed-bindings/options','/custody/managed-bindings/complete','/custody/managed-bindings/lookup','/custody/managed-bindings/session-status','/custody/managed-bindings/session-lifetime',
        ...(worldProcess?['/participant/policy-original','/participant/authorize','/participant/submit','/participant/lookup','/participant/owner-view',...(mode.startsWith('native-participant-owner-setup')?['/entry-owner/control']:[])]:[])].includes(suffix));
      nativeRequests[suffix]=(nativeRequests[suffix]??0)+1;
      if(nativeUnavailable)throw new Error('fixed test native provider unavailable');
      if(!management) {
        assert.equal(request.headers.get('authorization'),null,'ordinary native transport has no approval adapter credential');
        assert.notEqual(request.headers.get('cookie'),native.data.cookie,'manager cookie never becomes a managed account credential');
      }
      if(url.pathname.endsWith('/verify')) nativeVerifications++;
      const headers=new Headers();for(const name of ['authorization','cookie','origin','sec-fetch-site','x-oasis7-csrf','content-type']) {const value=request.headers.get(name);if(value!==null) headers.set(name,value);}
      const response=await fetch(native.data.address+url.pathname,{method:request.method,headers,...(request.method==='GET'?{}:{body:Buffer.from(await request.arrayBuffer())}),redirect:'manual'});
      if(mode==='native-participant-lost-submit' && suffix==='/participant/submit' && response.status===200 && !lostParticipantReply) {
        await response.arrayBuffer();lostParticipantReply=true;
        throw new Error('fixture loses only the original submit response; canonical World may commit');
      }
      if(mode==='native-lost-binding' && suffix==='/custody/managed-bindings/complete' && response.status===200 && !lostBindingReply) {
        await response.arrayBuffer(); lostBindingReply=true;
        throw new Error('fixture lost binding response after native SQL commit');
      }
      if((['native-lost-login','native-lost-missing-result'].includes(mode)||mode==='native-participant-owner-setup-real-fence-test-session-lost-initial') && suffix==='/native-key/complete' && response.status===200 && !lostLoginReply) {
        await response.arrayBuffer(); lostLoginReply=true;
        throw new Error('fixture lost original login Cookie after native SQL commit');
      }
      const outputHeaders=new Headers({'Content-Type':'application/json'});
      if(nativeMode) for(const cookie of response.headers.getSetCookie()) outputHeaders.append('Set-Cookie',cookie);
      let body=await response.text();
      if(suffix==='/custody/approvals/verify' && response.status===200) {
        const fact=JSON.parse(body).verification;
        if(fact) approvalFacts.push(Object.fromEntries(['approval_request_id','purpose','proof_state','origin_state','observed_at','proof_expires_at'].map(key=>[key,fact[key]])));
      }
      if(mode==='native-stage-gap-stale-entry' && suffix==='/custody/management-session-status' && nativeRequests[suffix]>=3 && response.status===200) {
        const status=JSON.parse(body);status.server_time=String(Date.now()-5001);body=JSON.stringify(status);
      }
      if(suffix.startsWith('/participant/') || suffix==='/entry-owner/control') {
        try {const value=JSON.parse(body),payload=value.world_response?.payload,control=payload?.control?.view?.game_task_control,goal=control?.active_goal_reference,receipt=payload?.outcome?.receipt;
          worldResults.push({path:suffix,http:response.status,
            outcome:suffix==='/entry-owner/control' ? payload?.outcome?.status??null : payload?.outcome??value.payload?.outcome??null,error:value.error??null,
            ...(control?{task:{revision:control.task_revision,status:control.task_status,executor_epoch:control.executor_epoch,latest_goal_revision:control.latest_goal_revision,active_goal_revision:control.active_goal_revision,goal_height:goal?.activation_committed_height,prepare:goal?.prepare_operation_id,publish:goal?.publish_operation_id}}:{}),
            ...(suffix==='/entry-owner/control'&&receipt?{receipt:{task_revision:receipt.task_revision,goal_height:receipt.goal_reference?.activation_committed_height,goal_revision:receipt.goal_reference?.goal_revision,prepare:receipt.goal_reference?.prepare_operation_id,publish:receipt.goal_reference?.publish_operation_id}}:{})});} catch {worldResults.push({path:suffix,http:response.status,error:'invalid_json'});}
      }
      if(mode==='native-participant-tamper-policy' && suffix==='/participant/policy-original' && response.status===200) {
        const original=JSON.parse(body);original.response.signature_hex='00'.repeat(64);body=JSON.stringify(original);
      }
      if(mode==='native-participant-tamper-final' && ['/participant/submit','/participant/lookup'].includes(suffix) && response.status===200) {
        const original=JSON.parse(body);original.world_response.signature_hex='00'.repeat(64);body=JSON.stringify(original);
      }
      if(mode.startsWith('native-participant-wrapper-') && ['/participant/submit','/participant/lookup'].includes(suffix) && response.status===200) {
        const original=JSON.parse(body);
        switch(mode.slice('native-participant-wrapper-'.length)) {
          case 'version':original.api_version=3;break;
          case 'ok':original.ok=false;break;
          case 'unknown':original.caller_authorized=true;break;
          case 'upstream':delete original.world_response;original.upstream_read_error={status:409,code:'not_authority'};break;
        }
        body=JSON.stringify(original);
      }
      if(mode==='native-participant-session-revoke' && suffix==='/participant/policy-original' && response.status===200) {
        const logout=await fetch(native.data.address+'/api/public/hosted-account/v2/logout',{method:'POST',headers});
        assert.equal(logout.status,200,'actual managed native logout succeeds');
      }
      if(mode==='native-participant-real-fence-freeze' && suffix==='/participant/policy-original' && response.status===200) {
        const disabled=await (await mf.getWorker('fence-controller')).fetch('https://fixture.test/disable',{method:'POST'});
        assert.equal(disabled.status,200);
      }
      return new MiniflareResponse(body,{status:response.status,headers:outputHeaders});
    };
    const restart=async nextMode=>{await mf?.dispose();const configured=options(persist,native.data,bridge,nextMode,configureOptions);mf=new Miniflare(configured);};
    await restart(mode);
    if(realFenceMode(mode)) {
      const controller=await mf.getWorker('fence-controller');
      const description=await (await controller.fetch('https://fixture.test/describe',{method:'POST'})).json();
      const activation=await controller.fetch('https://fixture.test/activate',{method:'POST',body:JSON.stringify(signFixtureRecoveryActivation(description,'native-fixture-bootstrap'))});
      assert.equal(activation.status,200,'the real independent Fence must consume the synthetic offline quorum');
    }
    const headers={Cookie:native.data.cookie,Origin:'https://localhost','Content-Type':'application/json','X-Oasis7-CSRF':native.data.csrf};
    const draft={api_version:1,client_request_id:'negative-runtime',plan:{...routes,purpose:'isolated actual runtime negative cases',expires_at:String(Date.now()+(mode==='native-lost-login'?6000:180000)),transport_limit:mode==='native-participant-low-budget'?'399':'400'}};
    let identity=await mf.getWorker('identity');
    const response=await identity.fetch('https://localhost/v1/account-provisions',{method:'POST',headers,body:JSON.stringify(draft)});
    assert.equal(response.status,200);const original=await response.json();
    const request={approval_request_id:'negative-runtime-approval',context:{environment:'test',custody_account_id:original.entry.custody_account_id,audience:native.data.audience,policy_version:'1',security_version:'0',service_epoch:'1'},
      intent:{kind:'provision',provision_id:original.entry.provision_id,purpose:draft.plan.purpose,plan_digest:original.entry.plan_digest,bootstrap_config_version:'1',world_identity:world,identity_route:routes.identity_route,world_route:routes.world_route,
        native_identity_namespace:native.data.namespace,register_request_id:'negative-runtime-register',login_request_id:'negative-runtime-login',deadline:draft.plan.expires_at,transport_limit:draft.plan.transport_limit}};
    const completed=await (await fetch(native.data.address+'/_fixture/approve-intent',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(request)})).json();
    assert.ok(completed.approval,'actual isolated native UV must sign the explicit test plan');
    const base='https://localhost/v1/account-provisions/'+original.entry.provision_id;
    const accepted=await identity.fetch(base+'/approve',{method:'POST',headers,body:JSON.stringify({api_version:1,input:{operation_id:'negative-runtime-approval-operation',request,approval:completed.approval}})});
    assert.equal(accepted.status,200);
    await run({persist,account:original.entry.custody_account_id,provisionId:original.entry.provision_id,
      publicEnrollmentFetch:async(path,init)=>{assert.match(path,/^\/v1\/client-enrollments(?:\/[A-Za-z0-9._~-]+\/prove)?$/);const incoming=new Headers(init.headers);for(const key of ['cookie','authorization','x-oasis7-csrf'])assert.equal(incoming.get(key),null,'public enrollment never inherits manager credentials');return mf.dispatchFetch('https://localhost'+path,init);},
      activationReadDelayStatus:async()=>{const {ACCOUNTS}=await mf.getBindings('owner');const response=await ACCOUNTS.get(ACCOUNTS.idFromName(original.entry.custody_account_id)).fetch('https://account.internal/fixture/activation-read-delay-status',{method:'POST'});assert.equal(response.status,200,'fixed readonly delay status unavailable');return response.json();},
      accountAlarmFixture:async(kind,sessionId)=>{const paths={observe:'/fixture/account-alarm-observe',early:'/fixture/account-alarm-early',unknown:'/fixture/account-alarm-unknown'};assert.ok(Object.hasOwn(paths,kind));const {ACCOUNTS}=await mf.getBindings('owner');const response=await ACCOUNTS.get(ACCOUNTS.idFromName(original.entry.custody_account_id)).fetch('https://account.internal'+paths[kind],{method:'POST',headers:{'content-type':'application/json'},body:JSON.stringify({session_id:sessionId})});assert.equal(response.status,200,'fixed actual Account alarm fixture unavailable');return response.json();},
      probeClientCloseTrap:async()=>{const {NATIVE_SESSIONS}=await mf.getBindings('identity');const stub=NATIVE_SESSIONS.get(NATIVE_SESSIONS.idFromName('fixed-close-trap-probe'));const denied=await stub.fetch('https://native-session.internal/internal/paired-private',{method:'POST'});const count=await (await stub.fetch('https://native-session.internal/fixture/close-trap-count',{method:'POST'})).json();await stub.fetch('https://native-session.internal/fixture/close-trap-reset',{method:'POST'});const after=await (await stub.fetch('https://native-session.internal/fixture/close-trap-count',{method:'POST'})).json();return {rejected:denied.status,attempts:count.attempts,after_reset:after.attempts};},
      setNativeUnavailable:value=>{assert.equal(typeof value,'boolean');nativeUnavailable=value;},
      publicClientFetch:async(path,init)=>{assert.match(path,/^\/v1\/account-provisions\/[^/]+\/accounts\/[^/]+\/client\/(?:proof-challenges|client-enrollments\/[^/]+\/claim|test-sessions\/[^/]+\/(?:initialize|tools-list|heartbeat|finish|abort))$/);const incoming=new Headers(init.headers);for(const key of ['cookie','x-oasis7-csrf'])assert.equal(incoming.get(key),null,'public client never inherits manager credentials');return mf.dispatchFetch('https://localhost'+path,init);},
      publicClientHostFault:async(path,init)=>{assert.match(path,/^\/v1\/account-provisions\/[^/]+\/accounts\/[^/]+\/client\/(?:proof-challenges|client-enrollments\/[^/]+\/claim|test-sessions\/[^/]+\/(?:initialize|tools-list|heartbeat|finish|abort))$/);return mf.dispatchFetch('https://localhost/fixture/client-wrong-host'+path,init);},
      testClientAdmission:async(kind,body)=>{const paths={drain:'/internal/performance/client-source-drain',direct_admit:'/internal/performance/client-direct-admit'};assert.ok(Object.hasOwn(paths,kind),'closed client admission fixture port');const {ACCOUNTS}=await mf.getBindings('owner');return ACCOUNTS.get(ACCOUNTS.idFromName(original.entry.custody_account_id)).fetch('https://account.internal'+paths[kind],{method:'POST',headers:{'content-type':'application/json'},body:JSON.stringify(body)});},
      readAccountRestoreCallbacks:async()=>{const {ACCOUNTS}=await mf.getBindings('owner');const response=await ACCOUNTS.get(ACCOUNTS.idFromName(original.entry.custody_account_id)).fetch('https://account.internal/internal/performance/account-restore-observed',{method:'POST'});const raw=await response.text();assert.equal(response.status,200,'actual Account diagnostic callback port unavailable');return JSON.parse(raw);},
      attestTestSession:async material=>{identity=await mf.getWorker('identity');return identity.fetch('https://localhost/fixture/test-session-attestation',{method:'POST',headers:{'content-type':'application/json'},body:JSON.stringify(material)});},
      readPrivateNativeHistory:async call=>{identity=await mf.getWorker('identity');return identity.fetch('https://localhost/fixture/native-history',{method:'POST',headers:{'content-type':'application/json'},body:JSON.stringify(call)});},
      managementFetch:async(suffix,body)=>{identity=await mf.getWorker('identity');return identity.fetch(base+suffix,{method:'POST',headers,body:JSON.stringify(body)});},
      approveIntent:async request=>{const response=await fetch(native.data.address+'/_fixture/approve-intent',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(request)});const raw=await response.text();assert.equal(response.status,200,raw);const proof=JSON.parse(raw);assert.ok(proof.approval,'actual native fixture must issue UV evidence');return proof.approval;},planExpires:Number(draft.plan.expires_at),restart,suspend:async()=>{await mf?.dispose();mf=undefined;},
      reconcileBasis:async(setup_id,stage)=>{
        identity=await mf.getWorker('identity');
        const reply=await identity.fetch(base+'/owner-setup-reconciles/basis',{method:'POST',headers,body:JSON.stringify({api_version:1,setup_id,stage})});
        const text=await reply.text();assert.equal(reply.status,200,text);return JSON.parse(text);
      },
      approveReconcile:async plan=>{
        identity=await mf.getWorker('identity');
        const prepared=await identity.fetch(base+'/owner-setup-reconciles/prepare',{method:'POST',headers,body:JSON.stringify({api_version:1,approval_request_id:'owner-reconcile-approval-one',plan})});
        const text=await prepared.text();assert.equal(prepared.status,200,text);const request=JSON.parse(text);
        const proofReply=await fetch(native.data.address+'/_fixture/approve-intent',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(request)});
        const proofText=await proofReply.text();assert.equal(proofReply.status,200,proofText);const proof=JSON.parse(proofText);assert.ok(proof.approval);
        const reply=await identity.fetch(base+'/owner-setup-reconciles/approve',{method:'POST',headers,body:JSON.stringify({api_version:1,plan,input:{operation_id:plan.reconcile_id,request,approval:proof.approval}})});
        const raw=await reply.text();assert.equal(reply.status,200,raw);return JSON.parse(raw);
      },
      continueReconcile:async(reconcile_id,attempt_id)=>{
        identity=await mf.getWorker('identity');
        return identity.fetch(base+'/owner-setup-reconciles/continue',{method:'POST',headers,body:JSON.stringify({api_version:1,reconcile_id,attempt_id})});
      },
      setupBasis:async()=>{
        identity=await mf.getWorker('identity');
        const reply=await identity.fetch(base+'/owner-setups/basis',{method:'POST',headers,body:JSON.stringify({api_version:1})});
        const body=await reply.text();
        if(reply.status!==200) {
          const statusReply=await identity.fetch('https://localhost/v1/management/session-status',{headers});
          const status=statusReply.ok?await statusReply.json():{};
          const saved=persistedRuntime(persist,original.entry.custody_account_id);
          assert.equal(reply.status,200,JSON.stringify({body,manager:Object.fromEntries(['identity_namespace','identity_instance_epoch','security_version','policy_version'].map(key=>[key,status[key]])),stored:{stage:saved.provision_stage,epoch:saved.account_security_epoch,binding:saved.native_binding}}));
        }
        return JSON.parse(body);
      },
      approveSetup:async plan=>{
        identity=await mf.getWorker('identity');
        const prepared=await identity.fetch(base+'/owner-setups/prepare',{method:'POST',headers,body:JSON.stringify({api_version:1,approval_request_id:'owner-setup-approval-one',plan})});
        const raw=await prepared.text();assert.equal(prepared.status,200,raw);const request=JSON.parse(raw);
        const proofReply=await fetch(native.data.address+'/_fixture/approve-intent',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(request)});
        const proofRaw=await proofReply.text();assert.equal(proofReply.status,200,proofRaw);const proof=JSON.parse(proofRaw);assert.ok(proof.approval);
        const reply=await identity.fetch(base+'/owner-setups/approve',{method:'POST',headers,body:JSON.stringify({api_version:1,plan,input:{operation_id:plan.setup_id,request,approval:proof.approval}})});
        const body=await reply.text();assert.equal(reply.status,200,JSON.stringify({status:reply.status,body,requests:nativeRequests}));return JSON.parse(body);
      },
      continueSetup:async setup_id=>{
        identity=await mf.getWorker('identity');
        return identity.fetch(base+'/owner-setups/continue',{method:'POST',headers,body:JSON.stringify({api_version:1,custody_account_id:original.entry.custody_account_id,provision_id:original.entry.provision_id,setup_id})});
      },
      approveManaged:async(blocked,limit='64',label='one')=>{
        assert.match(label,/^[a-z0-9-]{1,32}$/);
        const saved=persistedRuntime(persist,original.entry.custody_account_id);
        assert.ok(saved.native_binding);
        const request={approval_request_id:'fresh-managed-approval-'+label,context:{...completed.approval.proof.context,security_version:'1'},
          intent:{kind:'managed_login',native_account_id:saved.native_binding.native_account_id,login_key_ref:saved.native_binding.login_key_ref,
            native_identity_namespace:saved.native_binding.identity_namespace,blocked_operation_ref:blocked,login_request_id:'fresh-managed-login-'+label,transport_limit:limit,deadline:String(Date.now()+60000)}};
        const proofReply=await fetch(native.data.address+'/_fixture/approve-intent',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(request)});
        assert.equal(proofReply.status,200);const proof=await proofReply.json();assert.ok(proof.approval);
        identity=await mf.getWorker('identity');
        const reply=await identity.fetch(base+'/managed-logins/approve',{method:'POST',headers,body:JSON.stringify({api_version:1,input:{operation_id:'fresh-managed-operation-'+label,request,approval:proof.approval}})});
        assert.equal(reply.status,200);return reply.json();
      },
      continueManaged:async operation=>{identity=await mf.getWorker('identity');const reply=await identity.fetch(base+'/managed-logins/continue',{method:'POST',headers,body:JSON.stringify({api_version:1,operation_id:operation})});assert.equal(reply.status,200);return reply.json();},
      drive:async operation=>{identity=await mf.getWorker('identity');const reply=await identity.fetch(base+'/continue',{method:'POST',headers,body:JSON.stringify({api_version:1,operation_id:operation})});const body=await reply.text();assert.equal(reply.status,200,JSON.stringify({body,requests:nativeRequests,world:worldResults}));return JSON.parse(body);},
      driveResponse:async operation=>{identity=await mf.getWorker('identity');return identity.fetch(base+'/continue',{method:'POST',headers,body:JSON.stringify({api_version:1,operation_id:operation})});},
      freezeFence:async()=>{const reply=await (await mf.getWorker('fence')).fetch('https://fixture.test/freeze');assert.equal(reply.status,200);},
      oracle:async(role,body)=>(await mf.getWorker('oracle-proxy')).fetch('https://fixture.test/'+role+'-permit',{method:'POST',body:JSON.stringify(body)}),
      defaultExecutor:async(kind,body)=>(await mf.getWorker('oracle-proxy')).fetch('https://fixture.test/executor-default-'+kind,{method:'POST',body:JSON.stringify(body)}),
      reconcileFault:async()=>(await (await mf.getWorker('owner')).fetch('https://fixture.test/_fixture/reconcile-audit-fault')).json(),
      participantFault:async()=>(await (await mf.getWorker('owner')).fetch('https://fixture.test/_fixture/participant-audit-fault')).json(),
      participantRoleProbes:async()=>(await mf.getWorker('owner')).fetch('https://fixture.test/_fixture/participant-role-probes'),
      nativeCalls:()=>nativeVerifications,nativeRequests:()=>({...nativeRequests}),worldResults:()=>[...worldResults],approvalFacts:()=>[...approvalFacts]});
  } finally {await mf?.dispose();await native?.close();await worldProcess?.close();rmSync(directory,{recursive:true,force:true});}
}

// Only non-secret facts leave the isolated SQLite files; no envelope/seed/session
// is printed. Corruption is injected only while the owning workerd is disposed.
export function persistedRuntime(persist, account, corruptIdentity=false) {
  return JSON.parse(execFileSync('python3',['-c',`
import hashlib,json,sqlite3,sys
from pathlib import Path
result={'slots':[],'dispatches':[],'inspections':0,'native_results':[],'native_reads':[],'native_binding':None,'cipher_records':0,'history_count':0,'status_recoveries':[],'participants':[],'setups':[]}
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
    result['participant_final_audits']=sum(1 for (payload,) in db.execute("SELECT payload FROM custody_audit WHERE record_id=?",("custody-provision-approval-v1",)) if "complete_participant" in payload)
    result['inspections']=len(state['origin_inspections'])
    result['dispatches']=[{'role':d['scope']['context']['identity']['role'],'reads':d['permit_reads'],'expires_at':d['expires_at'],'request':json.loads(d['request_bytes']),'has_result':d['result'] is not None} for d in state['role_key_dispatches'].values()]
    result['native_results']=[v['result']['kind'] for v in state.get('native_stages',{}).values() if v.get('result') is not None]
    result['native_reads']=[{'stage':v['request']['stage'],'reads':v['reads']} for v in state.get('native_stages',{}).values()]
    result['history_count']=len(state.get('native_history',{}))
    result['provision_stage']=state['provision']['stage']
    result['account_security_epoch']=state['provision']['security_epoch']
    ledger=state['provision']['transport'];result['transport_remaining']=str(int(ledger['limit'])-int(ledger['spent'])-sum(int(v) for v in ledger['reservations'].values()))
    result['participants']=[{'reads':v['oracle_reads'],'claims':sorted(v['claims']),'callbacks':sorted(v['callbacks']),'has_result':v['result'] is not None,'submit_digest':hashlib.sha256((v.get('submit_bytes') or '').encode()).hexdigest()} for v in state.get('participant_provisions',{}).values()]
    result['status_recoveries']=[{'reads':v['reads'],'has_result':v['result'] is not None,'nonce':v['nonce']} for v in state.get('session_status_recoveries',{}).values()]
    result['setup_reconciles']=[{'attempts':[{'reads':a['oracle_reads'],'has_fact':a['fact'] is not None,'has_result':a['result'] is not None,'lookup_claimed':a['lookup_claimed_at'] is not None,'published':a['published_at'] is not None,'admitted':a['admitted_at'] is not None,'post_claimed':a['completion_reserved_at'] is not None} for a in v['attempts'].values()]} for v in state.get('owner_setup_reconciles',{}).values()]
    result['setups']=[{'stages':[{'stage':stage,'reads':entry['oracle_reads'],'has_result':entry['result'] is not None,'result_provenance':entry.get('result_provenance'),'archived_sources':sorted(entry.get('canonical_history',{})),'has_original':entry['original'] is not None,'has_access':entry['access'] is not None,'has_world':entry['world'] is not None,'source_created_at':entry['source']['created_at'],'source_verified_at':entry['source'].get('verified_at'),'source_observed_at':((entry['source'].get('verification') or {}).get('verification') or {}).get('observed_at'),'source_expires_at':entry['source']['expires_at'],**{name:entry.get(name) for name in ('reserved_at','published_at','admission_callback_claimed_at','admitted_at','claimed_at','completed_at')}} for stage,entry in setup['stages'].items()]} for setup in state.get('owner_setups',{}).values()]
    for v in state.get('native_stages',{}).values():
     if (v.get('result') or {}).get('kind')=='binding_receipt':
      receipt=v['result']['result']['receipt'];result['native_binding']={k:receipt[k] for k in ('native_account_id','login_key_ref','identity_namespace','identity_instance_epoch')}
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
