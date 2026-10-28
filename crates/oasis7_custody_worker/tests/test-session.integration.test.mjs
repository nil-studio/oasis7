import assert from 'node:assert/strict';
import test from 'node:test';
import {createHash,createPrivateKey,createPublicKey,sign} from 'node:crypto';
import {execFileSync} from 'node:child_process';
import {readFileSync} from 'node:fs';
import {dirname,resolve} from 'node:path';
import {fileURLToPath} from 'node:url';
import {withRoleRuntime,persistedRuntime} from './fixtures/role_runtime_native_support.mjs';
const root=resolve(dirname(fileURLToPath(import.meta.url)),'..');
const mode='native-participant-owner-setup-real-fence-test-session';
const origin='https://localhost';
const setupDiagnostic=process.env.OASIS7_OWNER_SETUP_DIAGNOSTIC==='true';
const key=createPrivateKey({key:Buffer.concat([Buffer.from('302e020100300506032b657004220420','hex'),Buffer.alloc(32,18)]),type:'pkcs8',format:'der'});
const publicKey=createPublicKey(key).export({format:'der',type:'spki'}).subarray(-32).toString('hex');
// Independent deterministic CBOR for public metadata; the real Core rejects any
// mismatch. This creates no authority and uses no native/Owner signing seed.
function head(major,value){let n=BigInt(value);if(n<24n)return Buffer.from([(major<<5)|Number(n)]);for(const [bound,size,tag] of [[256n,1,24],[65536n,2,25],[4294967296n,4,26],[18446744073709551616n,8,27]])if(n<bound){const b=Buffer.alloc(1+size);b[0]=(major<<5)|tag;for(let i=size;i>0;i--){b[i]=Number(n&255n);n>>=8n;}return b;}assert.fail('fixture integer bound');}
function cbor(value){if(value===null)return Buffer.from([246]);if(typeof value==='boolean')return Buffer.from([value?245:244]);if(typeof value==='number'){assert.ok(Number.isSafeInteger(value)&&value>=0);return head(0,value);}if(typeof value==='string'){const b=Buffer.from(value);return Buffer.concat([head(3,b.length),b]);}if(Array.isArray(value))return Buffer.concat([head(4,value.length),...value.map(cbor)]);const pairs=Object.entries(value).map(([k,v])=>[cbor(k),cbor(v)]).sort((a,b)=>a[0].length-b[0].length||Buffer.compare(a[0],b[0]));return Buffer.concat([head(5,pairs.length),...pairs.flat()]);}
const digest=(domain,value)=>createHash('sha256').update(cbor([domain,value])).digest('hex');
const descriptor={api_version:1,environment:'test',transport_profile:'native-broker-custody-v1',tool_profile:'custody-native-v1',host_instance_id:'isolated-synthetic-host',client_public_key:publicKey,build_digest:'b1'.repeat(32),skill_manifest_digest:'b2'.repeat(32),tool_manifest_digest:'b3'.repeat(32),https_origin:'https://host.example',restricted_runtime_identity:'synthetic-host-not-isolation-acceptance',isolation_mechanism:'separate_os_principal',isolation_acceptance_refs:['synthetic-fixture-not-CT008']};
const descriptorDigest=digest('oasis7.custody.host-deployment-descriptor.v1',descriptor);
let ownerDiagnosticActive=false;
let identityDiagnosticActive=false;
let identitySecretsDisabled=false;
let identityCloseReadTrap=false;
let accountAlarmFixtureActive=false;
let accountAlarmFault=null;
let activationReadDelay=false;
let paidDirectoryBarrier=null;
function configureOptions(options){
  assert.equal(options.workers[0]?.name,'identity','public dispatchFetch must target the actual Identity ingress');
  let configuredRoles=0;
  for(const worker of options.workers){
    if(!['identity','owner'].includes(worker.name))continue;
    configuredRoles++;
    if(worker.name==='owner' && ownerDiagnosticActive && (process.env.OASIS7_TEST_SESSION_OWNER_DIAGNOSTIC==='true'||setupDiagnostic)) {
      const ownerRoot=resolve(root,'artifacts/owner')+'/';
      const diagnosticRoot=resolve(root,'artifacts/approval-harness')+'/';
      let wrappers=0;
      worker.modules=worker.modules.map(module=>{
        if(module.path.startsWith(ownerRoot))return {...module,path:diagnosticRoot+module.path.slice(ownerRoot.length)};
        if(module.path===resolve(root,'tests/fixtures/native_owner_result_loss.mjs')) {
          const contents=readFileSync(module.path,'utf8');
          const marker='../../artifacts/owner/entry.mjs';
          assert.equal(contents.split(marker).length-1,2,'only two explicit Owner wrapper imports are redirected');
          wrappers++;
          return {...module,contents:contents.replaceAll(marker,'../../artifacts/approval-harness/entry.mjs')};
        }
        return module;
      });
      assert.equal(wrappers,1,'diagnostic fixture must select the actual Owner wrapper');
    }
    if(paidDirectoryBarrier&&worker.name==='owner'){
      worker.bindings={...worker.bindings,FIXTURE_PAID_DIRECTORY_MODE:paidDirectoryBarrier};
      worker.serviceBindings={...worker.serviceBindings,FIXTURE_FENCE_CONTROL:{name:'fence',entrypoint:'RecoveryFenceControl'}};
      worker.modules=[{type:'ESModule',path:resolve(root,'tests/fixtures/paid_directory_fence_barrier.mjs')},...worker.modules];
    }
    if(activationReadDelay&&worker.name==='owner'){
      worker.bindings={...worker.bindings,FIXTURE_ACTIVATION_READ_DELAY:'true',FIXTURE_ACTIVATION_READ_MODE:typeof activationReadDelay==='string'?activationReadDelay:'together'};
      worker.modules=[{type:'ESModule',path:resolve(root,'tests/fixtures/activation_read_delay_runtime.mjs')},...worker.modules];
    }
    if(accountAlarmFault&&worker.name==='owner'){
      worker.bindings={...worker.bindings,FIXTURE_TEST_SESSION_ALARM_FAULT:accountAlarmFault};
      worker.modules=[{type:'ESModule',path:resolve(root,'tests/fixtures/test_session_alarm_fault_runtime.mjs')},...worker.modules];
    }
    if(accountAlarmFixtureActive&&worker.name==='owner'){
      worker.bindings={...worker.bindings,FIXTURE_TEST_SESSION_ALARM:'true'};
      worker.modules=[{type:'ESModule',path:resolve(root,'tests/fixtures/test_session_alarm_runtime.mjs')},...worker.modules];
    }
    if(identityCloseReadTrap&&worker.name==='owner')delete worker.bindings.CUSTODY_ROLE_KEY_KEK;
    worker.bindings={...worker.bindings,CUSTODY_TEST_SESSION_ENABLED:'true',CUSTODY_CLIENT_ENROLLMENT_ENABLED:'true',CUSTODY_PUBLIC_ORIGIN:origin,CUSTODY_ENROLLMENT_REALM:'session-fixture-enrollment',CUSTODY_HOST_DEPLOYMENTS:JSON.stringify([{environment:'test',host_instance_id:descriptor.host_instance_id,descriptor_digest:descriptorDigest,client_public_key:publicKey}])};
    worker.durableObjects={...worker.durableObjects,CLIENT_ENROLLMENTS:{className:'ClientEnrollmentAuthority',useSQLite:true,...(worker.name==='owner'?{scriptName:'identity'}:{})}};
    if(worker.name==='identity') {
      worker.serviceBindings={...worker.serviceBindings,TEST_SESSION_PRIVATE_HISTORY:{name:'identity',entrypoint:'TestSessionPrivateHistory'},TEST_SESSION_NATIVE_HISTORY:{name:'identity',entrypoint:'NativeProvisionHistory'}};
      if(identitySecretsDisabled){delete worker.bindings.CUSTODY_NATIVE_PRIVATE_KEK;delete worker.bindings.CUSTODY_ROLE_KEY_KEK;}
      if(identityCloseReadTrap)worker.durableObjects.NATIVE_SESSIONS={className:'ClientCloseReadTrap',useSQLite:true};
    }
    if(worker.name==='identity')worker.modules=[{type:'ESModule',path:resolve(root,'tests/fixtures/test_session_runtime.mjs')},...worker.modules.filter(m=>!m.path.endsWith('/native_revocation_scheduler.mjs'))];
    else worker.serviceBindings={...worker.serviceBindings,IDENTITY_TEST_SESSIONS:{name:'identity',entrypoint:'IdentityTestSessions'},TEST_SESSION_PRIVATE_HISTORY:{name:'identity',entrypoint:'TestSessionPrivateHistory'}};
    if(worker.name==='identity' && identityDiagnosticActive && (process.env.OASIS7_TEST_SESSION_IDENTITY_DIAGNOSTIC==='true'||setupDiagnostic)) {
      const productionRoot=resolve(root,'artifacts/identity')+'/';
      const diagnosticRoot=resolve(root,'artifacts/native-harness')+'/';
      let wrappers=0;
      worker.modules=worker.modules.map(module=>{
        if(module.path.startsWith(productionRoot))return {...module,path:diagnosticRoot+module.path.slice(productionRoot.length)};
        if(module.path===resolve(root,'tests/fixtures/test_session_runtime.mjs')) {
          const contents=readFileSync(module.path,'utf8'),marker='../../artifacts/identity/entry.mjs';
          assert.equal(contents.split(marker).length-1,2,'only fixed Identity wrapper imports are redirected');
          wrappers++;return {...module,contents:contents.replaceAll(marker,'../../artifacts/native-harness/entry.mjs')};
        }
        return module;
      });
      assert.equal(wrappers,1,'diagnostic Identity must select the actual provider wrapper');
    }
  }
  assert.equal(configuredRoles,2,'test session V4 hook must configure actual Identity and Owner');
  return options;
}
function setupPlan(b){return {api_version:1,setup_id:'session-setup',provision_id:b.provision_id,participant_receipt_digest:b.participant_receipt_digest,world_identity:b.world_identity,owner_key_ref:b.owner_key.key_id,executor_key_ref:b.executor_key.key_id,bootstrap_config_version:b.bootstrap_config_version,goal_content:{goal_text:'Private isolated TestSession prerequisite',soft_constraints:['Use only approved task']},create_operation_id:'session-create',prepare_operation_id:'session-prepare',prepare_nonce:'a7'.repeat(32),publish_operation_id:'session-publish',delegation_operation_id:'session-delegate',bind_operation_id:'session-bind',executor_id:'session-executor',executor_public_key:b.executor_key.public_key,delegation_generation:'1',delegation_nonce:'1',resource_refs:[],deadline:String(Date.now()+120000),request_limit:'6',transport_limit:'584',write_limit:'4'};}
function proofHeaders(view,body){const created=Math.floor(Date.now()/1000),sha=v=>createHash('sha256').update(v).digest('base64');const components=['@method','@target-uri','content-type','content-digest','oasis7-token-digest','oasis7-enrollment','oasis7-request-id'];const params=`(${components.map(x=>JSON.stringify(x)).join(' ')});created=${created};expires=${created+29};nonce="${view.server_nonce}";alg="ed25519";keyid="${view.key_id}";tag="oasis7-custody-enrollment-proof-v1"`;const headers={'content-type':'application/json','content-digest':`sha-256=:${sha(body)}:`,'oasis7-token-digest':`:${sha('')}:`,'oasis7-enrollment':view.enrollment_id,'oasis7-request-id':'session-proof'};const values={'@method':'POST','@target-uri':`${origin}/v1/client-enrollments/${view.enrollment_id}/prove`,...headers};const base=components.map(x=>`"${x}": ${values[x]}`).concat(`"@signature-params": ${params}`).join('\n');return {...headers,'signature-input':`oasis7=${params}`,signature:`oasis7=:${sign(null,Buffer.from(base),key).toString('base64')}:`};}
async function json(response){const body=await response.text();assert.equal(response.status,200,body);return JSON.parse(body);}
// Invoke only after suspend. Print only counters and immutable structural facts.
function stored(runtime,id){return JSON.parse(execFileSync('python3',['-c',`
import json,sqlite3,sys
from pathlib import Path
result=None
for p in Path(sys.argv[1]).rglob('*.sqlite'):
 try:
  db=sqlite3.connect(p.as_uri()+'?mode=ro',uri=True)
  row=db.execute("SELECT payload FROM custody_records WHERE id='custody-provision-approval-v1'").fetchone()
  if not row: continue
  state=json.loads(row[0])['state']
  if state['draft']['entry']['custody_account_id']!=sys.argv[2]: continue
  s=state['test_sessions'][sys.argv[3]]
  result={'roots':sorted(s['roots']),'consumed':s['consumed_at'] is not None,'approved':s['approved_at'] is not None,'reads':s['preflight_oracle_reads'],'steps':sorted(s['preflight_steps']),'source_reads':sorted(s['source_oracle_reads'].values()),'credentials':sum(v['credential'] is not None for v in s['roots'].values()),'terminations':len(s['termination_slots'])}
 except sqlite3.OperationalError: continue
 finally:
  if 'db' in locals():db.close()
assert result is not None
print(json.dumps(result,separators=(',',':')))
`,runtime.persist,runtime.account,id],{encoding:'utf8'}));}

// Failure-only diagnostics: fixed stage names, booleans, counts and timestamps.
// This never returns a source original, signature, nonce, public key or payload hash.
function continueFailureState(runtime,id){
  try{return JSON.parse(execFileSync('python3',['-c',`
import json,sqlite3,sys
from pathlib import Path
out={'available':False}
def stamp(v):
 if isinstance(v,str) and v.isascii() and v.isdigit() and len(v)<=20 and int(v)<=18446744073709551615:return v
 if type(v) is int and 0<=v<=18446744073709551615:return str(v)
 return None
def obj(v):return v if isinstance(v,dict) else {}
for p in Path(sys.argv[1]).rglob('*.sqlite'):
 try:
  db=sqlite3.connect(p.as_uri()+'?mode=ro',uri=True)
  row=db.execute("SELECT payload FROM custody_records WHERE id='custody-provision-approval-v1'").fetchone()
  if not row:continue
  state=json.loads(row[0])['state']
  if state['draft']['entry']['custody_account_id']!=sys.argv[2]:continue
  session=state.get('test_sessions',{}).get(sys.argv[3])
  if not isinstance(session,dict):out={'available':True,'session_found':False};continue
  steps=obj(session.get('preflight_steps'))
  sources=obj(session.get('sources'))
  source_rows=[]
  for source in list(sources.values())[:8]:
   source=obj(source);verification=obj(obj(source.get('verification')).get('verification'))
   source_rows.append({'claimed':source.get('claimed_at') is not None,'verified':source.get('verification') is not None,'created_at':stamp(source.get('created_at')),'expires_at':stamp(source.get('expires_at')),'claimed_at':stamp(source.get('claimed_at')),'verified_at':stamp(source.get('verified_at')),'native_observed_at':stamp(verification.get('observed_at')),'native_source_session_expires_at':stamp(verification.get('source_session_expires_at')),'native_source_idle_expires_at':stamp(verification.get('source_session_idle_expires_at'))})
  closing=session.get('closing');reason=closing[0] if isinstance(closing,list) and len(closing)==2 else None
  reasons={'finished','aborted','deadline','lease_expired','budget_exhausted','revoked'}
  stage=obj(state.get('provision')).get('stage')
  stages={'draft','approved','role_keys_ready','native_registered','native_session_ready','participant_bound','ready_for_setup','prepared','frozen','cancelled'}
  lifetime=obj(obj(session.get('preflight_lifetime')).get('lifetime'));status=obj(lifetime.get('status'))
  out={'available':True,'session_found':True,'provision_stage':stage if stage in stages else 'unknown','closing_reason':reason if reason in reasons else None,'consumed':session.get('consumed_at') is not None,'approved':session.get('approved_at') is not None,'preflight_claimed':session.get('preflight_claimed_at') is not None,'consumed_at':stamp(session.get('consumed_at')),'preflight_claimed_at':stamp(session.get('preflight_claimed_at')),'oracle_reads':session.get('preflight_oracle_reads') if type(session.get('preflight_oracle_reads')) is int else None,'steps':{name:stamp(steps[name]) for name in ['fresh_access_pair','lifetime','owner_read'] if name in steps},'has_access':session.get('preflight_access') is not None,'has_published_read':session.get('preflight_read') is not None,'has_lifetime':session.get('preflight_lifetime') is not None,'lifetime_observed_at':stamp(status.get('observed_at')),'lifetime_status_expires_at':stamp(status.get('expires_at')),'lifetime_session_expires_at':stamp(lifetime.get('session_expires_at')),'sources_count':len(sources),'source_read_counts':[v for v in obj(session.get('source_oracle_reads')).values() if type(v) is int],'sources':source_rows}
 except (sqlite3.OperationalError,ValueError,KeyError,TypeError):continue
 finally:
  if 'db' in locals():db.close()
print(json.dumps(out,separators=(',',':')))
`,runtime.persist,runtime.account,id],{encoding:'utf8',stdio:['ignore','pipe','pipe']}));}
  catch{return {available:false,diagnostic_unavailable:true};}
}
// Failure-only readonly baseline evidence; no originals, identities or digests leave SQLite.
function baselineFailureState(runtime,observedAt){
  try{return JSON.parse(execFileSync('python3',['-c',`
import json,sqlite3,sys
from pathlib import Path
now=int(sys.argv[3]);out={'available':False,'observed_at':str(now)}
def obj(v):return v if isinstance(v,dict) else {}
def stamp(v):
 if isinstance(v,str) and v.isascii() and v.isdigit() and len(v)<=20 and int(v)<=18446744073709551615:return v
 if type(v) is int and 0<=v<=18446744073709551615:return str(v)
 return None
def source(v):
 v=obj(v);native=obj(obj(v.get('verification')).get('verification'));at=stamp(native.get('observed_at'))
 return {'created_at':stamp(v.get('created_at')),'verified_at':stamp(v.get('verified_at')),'claimed_at':stamp(v.get('claimed_at')),'expires_at':stamp(v.get('expires_at')),'native_observed_at':at,'source_age_ms':str(now-int(at)) if at is not None and now>=int(at) else None,'has_verification':v.get('verification') is not None}
for p in Path(sys.argv[1]).rglob('*.sqlite'):
 db=sqlite3.connect(p.as_uri()+'?mode=ro',uri=True)
 try:
  row=db.execute("SELECT payload FROM custody_records WHERE id='custody-provision-approval-v1'").fetchone()
 except sqlite3.OperationalError:
  db.close();continue
 db.close()
 if not row:continue
 state=json.loads(row[0])['state']
 if state['draft']['entry']['custody_account_id']!=sys.argv[2]:continue
 out['available']=True;out['participants']=[];out['setups']=[]
 for v in obj(state.get('participant_provisions')).values():
  v=obj(v);claims=obj(v.get('claims'));callbacks=obj(v.get('callbacks'))
  out['participants'].append({'source':source(v.get('source_inspection')),'workflow_claimed_at':stamp(v.get('workflow_claimed_at')),'oracle_reads':v.get('oracle_reads') if type(v.get('oracle_reads')) is int else None,'claims':{k:stamp(claims[k]) for k in ['policy','sign_bundle','authorize','submit','owner_view'] if k in claims},'callbacks':{k:stamp(callbacks[k]) for k in ['policy','admission','final'] if k in callbacks},'facts':{k:v.get(k) is not None for k in ['policy','bundle','admission','submit','lookup','owner_view','result']},'completed_at':stamp(v.get('completed_at'))})
 for setup in obj(state.get('owner_setups')).values():
  stages=[]
  for name,e in obj(obj(setup).get('stages')).items():
   if name not in ['create_task','prepare_goal','publish_goal','delegation','bind','final_observation']:continue
   e=obj(e);history=obj(e.get('canonical_history'))
   stages.append({'stage':name,'source':source(e.get('source')),'oracle_reads':e.get('oracle_reads') if type(e.get('oracle_reads')) is int else None,'times':{k:stamp(e.get(k)) for k in ['published_at','admission_callback_claimed_at','admitted_at','claimed_at','callback_claimed_at','completed_at','last_observed_at','lookup_reserved_at','lookup_claimed_at']},'has_result':e.get('result') is not None,'has_last_observation':e.get('last_observation') is not None,'canonical_history_count':len(history),'archived_sources':[k for k in ['submit','lookup'] if k in history],'archived_outcomes':{k:(obj(obj(obj(history[k]).get('response')).get('payload')).get('outcome') or {}).get('status') if (obj(obj(obj(history[k]).get('response')).get('payload')).get('outcome') or {}).get('status') in ['received','pending','committed','rejected','expired','unknown','history_unavailable'] else 'unknown_shape' for k in ['submit','lookup'] if k in history}})
  out['setups'].append({'stages':stages})
print(json.dumps(out,separators=(',',':')))
`,runtime.persist,runtime.account,String(observedAt)],{encoding:'utf8',stdio:['ignore','pipe','pipe']}));}
  catch{return {available:false,diagnostic_unavailable:true};}
}
async function withActiveSession(run,{lostInitial=false,approvalFault=null,preflightCase=null}={}){
  const selectedMode=lostInitial?mode+'-lost-initial':mode;
  ownerDiagnosticActive=setupDiagnostic;identityDiagnosticActive=setupDiagnostic;identitySecretsDisabled=false;identityCloseReadTrap=false;accountAlarmFixtureActive=false;accountAlarmFault=null;activationReadDelay=false;paidDirectoryBarrier=null;
  await withRoleRuntime(selectedMode,async runtime=>{
    let blocked=null;
    const initial=await runtime.drive('session-participant');
    if(lostInitial){
      assert.equal(initial.stage,'native_registered');
      assert.equal(initial.pending_operations.length,1);
      blocked=initial.pending_operations[0];
      await runtime.approveManaged(blocked);
      await runtime.continueManaged('fresh-managed-operation-one');
      assert.equal((await runtime.drive('session-participant-recovered')).stage,'participant_bound');
    }else {
      if(initial.stage!=='participant_bound'){
        const observedAt=Date.now(),native_requests=runtime.nativeRequests();await runtime.suspend();
        console.error(JSON.stringify({baseline_participant_failure:{native_requests,state:baselineFailureState(runtime,observedAt)}}));
      }
      assert.equal(initial.stage,'participant_bound');
    }
    const setup=setupPlan(await runtime.setupBasis());await runtime.approveSetup(setup);
    const setupProgress=await json(await runtime.continueSetup(setup.setup_id));
    if(setupProgress.stage!=='prepared') {
      const observedAt=Date.now(),native_requests=runtime.nativeRequests();await runtime.suspend();
      console.error(JSON.stringify({baseline_setup_failure:{native_requests,state:baselineFailureState(runtime,observedAt)}}));
      const saved=persistedRuntime(runtime.persist,runtime.account);
      const keys=['stage','reads','has_result','has_original','has_access','has_world','source_created_at','source_verified_at','source_observed_at','source_expires_at','reserved_at','published_at','admission_callback_claimed_at','admitted_at','claimed_at','completed_at'];
      const summary={stage:saved.provision_stage,setups:saved.setups.map(setup=>({stages:setup.stages.map(stage=>Object.fromEntries(keys.map(key=>[key,stage[key]])))}))};
      assert.fail(JSON.stringify({baseline_prepared:false,summary}));
    }
    if(process.env.OASIS7_TEST_SESSION_OWNER_DIAGNOSTIC==='true'||process.env.OASIS7_TEST_SESSION_IDENTITY_DIAGNOSTIC==='true') {
      // Preserve the actual production Account/SQLite; only target operations use diagnostic Wasm.
      ownerDiagnosticActive=process.env.OASIS7_TEST_SESSION_OWNER_DIAGNOSTIC==='true';
      identityDiagnosticActive=process.env.OASIS7_TEST_SESSION_IDENTITY_DIAGNOSTIC==='true';
      await runtime.restart(selectedMode);
    }
    const invite=await json(await runtime.managementFetch('/test-sessions/invite',{api_version:1,request_id:'session-invite',transport_profile:'native-broker-custody-v1',tool_profile:'custody-native-v1'}));
    const target={custody_account_id:runtime.account,provision_id:runtime.provisionId,invite_id:invite.invite_id,invite_digest:digest('oasis7.custody.pairing-invite.v1',invite)};
    const createBody=JSON.stringify({api_version:1,request_id:'session-create-enrollment',target,descriptor});
    const view=await json(await runtime.publicEnrollmentFetch('/v1/client-enrollments',{method:'POST',headers:{'content-type':'application/json'},body:createBody}));assert.equal(view.descriptor_digest,descriptorDigest);
    const proofBody=JSON.stringify({api_version:1,enrollment_id:view.enrollment_id,request_id:'session-proof',target,client_public_key:publicKey,server_nonce:view.server_nonce,challenge_expires_at:view.challenge_expires_at,descriptor_digest:view.descriptor_digest});
    assert.equal((await json(await runtime.publicEnrollmentFetch(`/v1/client-enrollments/${view.enrollment_id}/prove`,{method:'POST',headers:proofHeaders(view,proofBody),body:proofBody}))).status,'proved');
    const cleanup=Date.now()+60000;
    const basis=await json(await runtime.managementFetch('/test-sessions/basis',{api_version:1,request_id:'session-plan',enrollment_id:view.enrollment_id,cleanup_until:String(cleanup)}));
    const plan={api_version:1,request_id:basis.request_id,enrollment_id:view.enrollment_id,basis_digest:digest('oasis7.custody.test-session-basis.v1',basis),actions:[{tool:'move',position_cm:['11','22','33']}],allowed_resource_refs:basis.allowed_resource_refs,write_deadline:String(cleanup-20000),read_until:String(cleanup+20000),cleanup_until:String(cleanup),request_limit:'100',transport_limit:'400',business_limit:'0',read_budget:{requests:'100',transport:'300',business:'0'},cleanup_budget:{requests:'2',transport:'24',business:'0'},control_budget:{requests:'80',transport_per_hop:'240',identity_transport:'160',transport_hops:1}};
    const request=await json(await runtime.managementFetch('/test-sessions/prepare',{api_version:1,approval_request_id:'session-approval',plan}));
    const approval=await runtime.approveIntent(request);
    if(approvalFault){await approvalFault(runtime,{basis,plan,request,approval},selectedMode);return;}
    const consumed=await json(await runtime.managementFetch('/test-sessions/approve',{api_version:1,input:{operation_id:basis.session_id,request,approval},plan}));
    assert.equal(consumed.stage,'consumed_pending_preflight');assert.deepEqual(consumed.purposes,[]);assert.equal(consumed.identity_control_http_remaining,'159');
    await runtime.suspend();const beforeState=stored(runtime,basis.session_id);assert.equal(beforeState.consumed,true);assert.deepEqual(beforeState.roots,[]);assert.equal(beforeState.approved,false);
    if(preflightCase){await preflightCase(runtime,basis,selectedMode);return;}
    await runtime.restart(selectedMode);
    const before=runtime.nativeRequests();
    const activeResponse=await runtime.managementFetch('/test-sessions/continue',{api_version:1,session_id:basis.session_id});
    if(activeResponse.status!==200){
      const status=activeResponse.status;
      try{await activeResponse.arrayBuffer();}catch{} // Discard without printing body or errors.
      const failedNative=runtime.nativeRequests();
      const paths=['/custody/management-session-status','/custody/approvals/verify','/participant/policy-original','/participant/authorize','/custody/managed-bindings/session-status','/custody/managed-bindings/session-lifetime','/participant/owner-view'];
      const native_request_deltas=Object.fromEntries(paths.map(path=>[path,(failedNative[path]??0)-(before[path]??0)]));
      const diagnostic_started_at=Date.now();
      await runtime.suspend();
      console.error(JSON.stringify({test_session_continue_failure:{status,diagnostic_started_at,...continueFailureState(runtime,basis.session_id),native_request_deltas}}));
      assert.equal(status,200,'actual TestSession continue must succeed; fixed safe stage diagnostics are above');
    }
    const active=await json(activeResponse);
    assert.equal(active.stage,'active');assert.equal(active.write_closed,false);assert.deepEqual(active.pending_operations,[]);
    assert.deepEqual(active.purposes.map(p=>p.purpose).sort(),['execution','reconcile_read','termination_plan']);
    const roots=Object.fromEntries(active.purposes.map(p=>[p.purpose,p]));
    for(const [purpose,transport] of [['execution','400'],['reconcile_read','300'],['termination_plan','24']])assert.equal(roots[purpose].transport_remaining,transport);
    assert.equal(roots.execution.deadline,plan.write_deadline);assert.equal(roots.reconcile_read.deadline,plan.read_until);assert.equal(roots.termination_plan.deadline,plan.cleanup_until);
    const after=runtime.nativeRequests(),delta=path=>(after[path]??0)-(before[path]??0);
    assert.equal(delta('/participant/policy-original'),1);assert.equal(delta('/participant/authorize'),1);assert.equal(delta('/custody/managed-bindings/session-status'),2,'FreshAccess pair includes two actual Root status HTTP calls');assert.equal(delta('/custody/managed-bindings/session-lifetime'),1);assert.equal(delta('/participant/owner-view'),1);assert.equal(delta('/custody/approvals/verify'),1,'preflight has its own real source inspection');assert.equal(active.identity_control_http_remaining,'152');
    await runtime.suspend();const finalState=stored(runtime,basis.session_id);assert.deepEqual(finalState.roots,['execution','reconcile_read','termination_plan']);assert.equal(finalState.credentials,0);assert.equal(finalState.terminations,0);assert.equal(finalState.reads,5);assert.deepEqual(finalState.source_reads,[7]);assert.deepEqual(finalState.steps,['fresh_access_pair','lifetime','owner_read']);
    await runtime.restart(selectedMode);const count=runtime.nativeRequests();const retry=await json(await runtime.managementFetch('/test-sessions/continue',{api_version:1,session_id:basis.session_id}));assert.equal(retry.stage,'active');assert.deepEqual(runtime.nativeRequests(),{...count,'/custody/management-session-status':(count['/custody/management-session-status']??0)+1},'history does not repeat source or preflight business HTTP');
    await run(runtime,basis,selectedMode,blocked);
  },{configureOptions});
}

// Private signed facts and manifest originals flow through Node pipes/memory only.
// Errors deliberately discard child output; assertions never dump these objects.
function privateMetadata(runtime,operation,sessionId='',payload=null){
  const script=`
import hashlib,json,sqlite3,sys
from pathlib import Path
operation=sys.argv[3]
private=json.load(sys.stdin) if operation=='manifest_write' else None
found=[];records=[];audits=0
for p in Path(sys.argv[1]).rglob('*.sqlite'):
 db=None
 try:
  db=sqlite3.connect(p.as_uri()+('?mode=rw' if operation=='manifest_write' else '?mode=ro'),uri=True)
  if operation=='stamp':
   rows=db.execute('SELECT id,payload FROM custody_records ORDER BY id').fetchall()
   for record,payload in rows:records.append((str(p.relative_to(Path(sys.argv[1]))),record,payload))
   try:audits+=db.execute('SELECT count(*) FROM custody_audit').fetchone()[0]
   except sqlite3.OperationalError:pass
   continue
  key='custody-provision-approval-v1' if operation=='session' else 'custody-native-manifest-v1'
  row=db.execute('SELECT payload FROM custody_records WHERE id=?',(key,)).fetchone()
  if not row:continue
  value=json.loads(row[0])
  if operation=='session':
   state=value['state']
   if state['draft']['entry']['custody_account_id']!=sys.argv[2]:continue
   session=state['test_sessions'][sys.argv[4]]
   ref=session['session']
   if ref['phase']=='managed_login':
    call={'kind':'managed_login','custody_account_id':sys.argv[2],'provision_id':state['draft']['entry']['provision_id'],'operation_id':ref['source_permit_id']}
   else:
    original=state['native_stages'][ref['source_permit_id']]['request']
    call={'kind':'provision','request':original}
   found.append({'request':{'session':ref,'lifetime':session['preflight_lifetime']},'history_call':call})
  elif value['scope']['role_scope']['context']['identity']['account_id']==sys.argv[2]:
   if operation=='manifest_read':found.append({'raw':row[0],'generation':value['private_session_generation']})
   elif operation=='manifest_write':
    db.execute('UPDATE custody_records SET payload=? WHERE id=?',(private['raw'],key));db.commit();found.append({'changed':1})
 except sqlite3.OperationalError:continue
 finally:
  if db is not None:db.close()
if operation=='stamp':
 assert records
 digest=hashlib.sha256()
 for value in sorted(records):digest.update(json.dumps(value,separators=(',',':')).encode())
 result={'record_count':len(records),'audit_count':audits,'payload_digest':digest.hexdigest()}
else:
 assert len(found)==1
 result=found[0]
sys.stdout.write(json.dumps(result,separators=(',',':')))
`;
  try{return JSON.parse(execFileSync('python3',['-c',script,runtime.persist,runtime.account,operation,sessionId],{encoding:'utf8',input:operation==='manifest_write'?JSON.stringify(payload):undefined,stdio:['pipe','pipe','ignore'],maxBuffer:2*1024*1024}));}
  catch{throw new Error('private fixture metadata operation failed');}
}
async function assertGetterNoEffects(runtime,request,{accepted=true}={}){
  const before=privateMetadata(runtime,'stamp'),native=runtime.nativeRequests();
  const reply=await runtime.attestTestSession(request);
  if(accepted){
    assert.equal(reply.status,200,'fixed private metadata attestation should succeed');
    const result=await reply.json();
    assert.equal(result.session_ref,request.session.session_ref);
    assert.equal(result.private_sid_generation,request.session.private_sid_generation);
    assert.match(result.lifetime_digest,/^[0-9a-f]{64}$/);
  }else{await reply.text();assert.equal(reply.status,409,'invalid private metadata selector must be rejected');}
  assert.deepEqual(privateMetadata(runtime,'stamp'),before,'getter must not mutate custody records or audits');
  assert.deepEqual(runtime.nativeRequests(),native,'getter must not issue native HTTP');
}

test('real fresh MFA activates three roots and the private lifetime getter is readonly without KEKs',{timeout:300000},async()=>{
  await withActiveSession(async(runtime,basis,selectedMode)=>{
    await runtime.suspend();
    const material=privateMetadata(runtime,'session',basis.session_id);
    const manifest=privateMetadata(runtime,'manifest_read');assert.equal(manifest.generation,'1');
    await runtime.restart(selectedMode);
    await assertGetterNoEffects(runtime,material.request);
    for(const mutate of [
      value=>{value.session.scope.identity_namespace='wrong-namespace';},
      value=>{value.session.session_ref='wrong-session-ref';},
      value=>{value.session.source_permit_id='wrong-permit';},
      value=>{value.lifetime.lifetime.status.receipt.native_account_id='wrong-receipt-account';},
    ]){
      const invalid=structuredClone(material.request);mutate(invalid);
      await assertGetterNoEffects(runtime,invalid,{accepted:false});
    }
    // Root signatures remain historical facts. Reading one past its live status
    // window does not renew it or create business authority.
    const wait=Math.max(0,Number(material.request.lifetime.lifetime.status.expires_at)-Date.now()+10);
    if(wait>0)await new Promise(resolve=>setTimeout(resolve,wait));
    assert.ok(Date.now()>=Number(material.request.lifetime.lifetime.status.expires_at));
    await assertGetterNoEffects(runtime,material.request);
    identitySecretsDisabled=true;
    await runtime.restart(selectedMode);
    await assertGetterNoEffects(runtime,material.request);
    identitySecretsDisabled=false;
    await runtime.suspend();
    const corrupt=JSON.parse(manifest.raw);corrupt.schema_version=0;
    assert.equal(privateMetadata(runtime,'manifest_write','',{raw:JSON.stringify(corrupt)}).changed,1);
    await runtime.restart(selectedMode);
    await assertGetterNoEffects(runtime,material.request,{accepted:false});
    await runtime.suspend();
    assert.equal(privateMetadata(runtime,'manifest_write','',{raw:manifest.raw}).changed,1);
    await runtime.restart(selectedMode);
    await assertGetterNoEffects(runtime,material.request);
  });
});

test('a real second fresh ManagedLogin raises the private manifest floor without erasing gen1 history',{timeout:300000},async()=>{
  await withActiveSession(async(runtime,basis,selectedMode,blocked)=>{
    assert.ok(blocked,'the actual lost initial Cookie must remain an explicit-login-required original');
    await runtime.suspend();const material=privateMetadata(runtime,'session',basis.session_id);
    assert.equal(privateMetadata(runtime,'manifest_read').generation,'1');
    await runtime.restart(selectedMode);
    // Establish that this exact signed historical Lifetime is accepted at gen1
    // after its live observation window; expiry cannot explain the later denial.
    const wait=Math.max(0,Number(material.request.lifetime.lifetime.status.expires_at)-Date.now()+10);
    if(wait>0)await new Promise(resolve=>setTimeout(resolve,wait));
    await assertGetterNoEffects(runtime,material.request);
    await runtime.approveManaged(blocked,'64','two');
    await runtime.continueManaged('fresh-managed-operation-two');
    await runtime.suspend();assert.equal(privateMetadata(runtime,'manifest_read').generation,'2');
    await runtime.restart(selectedMode);
    const before=privateMetadata(runtime,'stamp'),native=runtime.nativeRequests();
    const history=await runtime.readPrivateNativeHistory(material.history_call);
    assert.equal(history.status,200,'the old completed history must remain readable in a valid gen2 manifest');
    const fact=await history.json();assert.equal(fact.session_attestation.private_sid_generation,'1');
    assert.deepEqual(privateMetadata(runtime,'stamp'),before);
    assert.deepEqual(runtime.nativeRequests(),native);
    await assertGetterNoEffects(runtime,material.request,{accepted:false});
  },{lostInitial:true});
});

// Literal RFC 9421 proof for the actual paired client key. No Root/Owner key is
// involved; all authority and paid ingress matching is verified by the real Store.
function clientClaimChallenge(basis,runtime){
  const requestId='source-exhausted-claim-challenge';
  const body=JSON.stringify({api_version:1,session_id:basis.session_id,request_id:requestId,purpose:'execution',kind:{kind:'credential_claim'},logical_request_id:'source-exhausted-claim',operation_request_digest:'61'.repeat(32)});
  const target=`${origin}/v1/account-provisions/${runtime.provisionId}/accounts/${runtime.account}/client/proof-challenges`;
  const now=Date.now(),created=Math.floor(now/1000),sha=value=>createHash('sha256').update(value).digest('base64');
  // The original fixture Plan fixes write_deadline=cleanup_until-20000.
  // Respect that same signed root bound rather than minting a 29s proof past it.
  const cleanup=BigInt(basis.termination.cleanup_until),writeDeadline=cleanup-20000n;
  const rootFloor=Number(writeDeadline/1000n);
  assert.ok(Number.isSafeInteger(rootFloor)&&rootFloor>created,'original root retains a positive whole-second proof window');
  const expires=Math.min(created+29,rootFloor);
  const positiveMargin=expires*1000-now>2000;
  console.log(JSON.stringify({client_proof_bounds:{unclamped_exceeds_root:BigInt(created+29)*1000n>writeDeadline,clamped_within_root:BigInt(expires)*1000n<=writeDeadline,positive_margin:positiveMargin}}));
  assert.ok(positiveMargin,'original root must retain at least 2s for real proof verification; do not renew it');
  const fields=['@method','@target-uri','content-type','content-digest','oasis7-token-digest','oasis7-test-session','oasis7-request-id'];
  const params=`(${fields.map(name=>JSON.stringify(name)).join(' ')});created=${created};expires=${expires};alg="ed25519";keyid="${basis.client_pairing.client_key_id}";tag="oasis7-custody-challenge-v1"`;
  const headers={'content-type':'application/json','content-digest':`sha-256=:${sha(body)}:`,'oasis7-token-digest':`:${sha('')}:`,'oasis7-test-session':basis.session_id,'oasis7-request-id':requestId};
  const values={'@method':'POST','@target-uri':target,...headers};
  const base=fields.map(name=>`"${name}": ${values[name]}`).concat(`"@signature-params": ${params}`).join('\n');
  headers['signature-input']=`oasis7=${params}`;headers.signature=`oasis7=:${sign(null,Buffer.from(base),key).toString('base64')}:`;
  return {ingress_attempt_id:'source-exhausted-actual-ingress',http:{selector:{api_version:1,custody_account_id:runtime.account,provision_id:runtime.provisionId,session_id:basis.session_id},original:{session_id:basis.session_id,method:'POST',target_uri:target,headers:Object.entries(headers).map(([name,value])=>({name,value})),body,nonce_hint:null}}};
}
function clientAdmissionCounters(runtime,id){
  let scan;
  try{scan=JSON.parse(execFileSync('python3',['-c',`
import json,sqlite3,sys
from pathlib import Path
out=None
counts={'files':0,'opened':0,'table':0,'row':0,'account':0,'session':0,'decode_errors':0,'sqlite_codes':{}}
for p in Path(sys.argv[1]).rglob('*.sqlite'):
 counts['files']+=1;db=None
 try:
  db=sqlite3.connect(p.as_uri()+'?mode=ro',uri=True);counts['opened']+=1
  if db.execute("SELECT 1 FROM sqlite_master WHERE type='table' AND name='custody_records'").fetchone():counts['table']+=1
  row=db.execute("SELECT payload FROM custody_records WHERE id='custody-provision-approval-v1'").fetchone()
  if not row:continue
  counts['row']+=1
  state=json.loads(row[0])['state']
  if state['draft']['entry']['custody_account_id']!=sys.argv[2]:continue
  counts['account']+=1
  session=state['test_sessions'].get(sys.argv[3])
  if session is None:continue
  counts['session']+=1
  out={'host_admissions':len(session['client_http_admissions']),'sources':len(session['sources'])}
 except sqlite3.Error as error:
  code=getattr(error,'sqlite_errorcode',None)
  label=str(code & 255) if type(code) is int and (code & 255) in (1,5,6,8,11,14,26) else 'other'
  counts['sqlite_codes'][label]=counts['sqlite_codes'].get(label,0)+1
 except (ValueError,KeyError,TypeError,UnicodeDecodeError):counts['decode_errors']+=1
 finally:
  if db is not None:db.close()
print(json.dumps({'counters':out,'scan':counts},separators=(',',':')))
`,runtime.persist,runtime.account,id],{encoding:'utf8',stdio:['ignore','pipe','pipe']}));}
  catch{assert.fail('fixed readonly client admission counter scan failed');}
  if(scan.counters===null)console.error(JSON.stringify({client_admission_sql_scan_failure:scan.scan}));
  assert.notEqual(scan.counters,null,'actual Account session counters must remain present after suspend');
  return scan.counters;
}
test('direct authenticated client admission keeps Host charge when native source budget is exhausted',{timeout:300000},async()=>{
  await withActiveSession(async(runtime,basis,selectedMode)=>{
    const previous=process.env.OASIS7_TEST_SESSION_OWNER_DIAGNOSTIC;
    try {
      process.env.OASIS7_TEST_SESSION_OWNER_DIAGNOSTIC='true';ownerDiagnosticActive=true;
      await runtime.restart(selectedMode);
      const valid=clientClaimChallenge(basis,runtime);valid.ingress_attempt_id='source-budget-valid-baseline';
      const accepted=await runtime.testClientAdmission('direct_admit',valid);
      assert.equal(accepted.status,200,'the same actual client proof profile must pass both crypto verifiers and SQL before exhaustion');
      const drained=await runtime.testClientAdmission('drain',{session_id:basis.session_id});
      assert.equal(drained.status,200,'bounded real Core source reservations must exhaust only the Native budget');
      const summary=await drained.json();assert.equal(summary.native_remaining,0);assert.ok(summary.reserved_native>0&&summary.reserved_native<=160);
      await runtime.suspend();const before=clientAdmissionCounters(runtime,basis.session_id);
      await runtime.restart(selectedMode);
      const nativeBefore=runtime.nativeRequests();
      const admission=clientClaimChallenge(basis,runtime);
      const rejected=await runtime.testClientAdmission('direct_admit',admission);
      assert.ok(rejected.status>=400,'source reservation must fail closed');
      const afterFirst=await (await runtime.testClientAdmission('drain',{session_id:basis.session_id})).json();
      assert.equal(afterFirst.host_remaining,summary.host_remaining-1,'the first authenticated rejected source attempt stays paid');
      const signedExpiry=Number(/;expires=([0-9]+);/.exec(admission.http.original.headers.find(h=>h.name==='signature-input').value)[1])*1000;
      assert.ok(signedExpiry-Date.now()>1000,'the exact signed proof must still be valid before callback replay');
      const callbackRetry=await runtime.testClientAdmission('direct_admit',admission);
      assert.ok(callbackRetry.status>=400,'same paid callback cannot create a source after budget exhaustion');
      const afterCallback=await (await runtime.testClientAdmission('drain',{session_id:basis.session_id})).json();
      assert.equal(afterCallback.host_remaining,afterFirst.host_remaining,'same ingress and exact proof are not charged twice');
      assert.ok(signedExpiry-Date.now()>1000,'the unchanged signed proof must remain valid for a new actual ingress');
      const nextIngress=structuredClone(admission);nextIngress.ingress_attempt_id='source-exhausted-next-actual-ingress';
      const nextRejected=await runtime.testClientAdmission('direct_admit',nextIngress);
      assert.ok(nextRejected.status>=400,'a distinct ingress also cannot create a source after exhaustion');
      const afterNext=await (await runtime.testClientAdmission('drain',{session_id:basis.session_id})).json();
      assert.equal(afterNext.host_remaining,afterCallback.host_remaining-1,'a distinct trusted ingress pays once for the exact same signed proof');
      // No public-driver revalidation runs here: this is the real local Store
      // method, actual Rust+WebCrypto signature path and final SQL transaction.
      assert.deepEqual(runtime.nativeRequests(),nativeBefore);
      await runtime.suspend();const after=clientAdmissionCounters(runtime,basis.session_id);
      assert.equal(after.sources,before.sources,'failed source reservation creates no source');
      assert.equal(after.host_admissions,before.host_admissions+2,'both distinct authenticated ingress attempts remain paid and the same-ingress callback is idempotent');
    } finally {
      ownerDiagnosticActive=false;
      if(previous===undefined)delete process.env.OASIS7_TEST_SESSION_OWNER_DIAGNOSTIC;else process.env.OASIS7_TEST_SESSION_OWNER_DIAGNOSTIC=previous;
    }
  });
});

function publicClientProof(basis,runtime,{tail,body,requestId,tag,token='',nonce=null,nonceUntil=null,purpose='execution'}){
  const path=`/v1/account-provisions/${runtime.provisionId}/accounts/${runtime.account}/client/${tail}`;
  const target=origin+path,raw=JSON.stringify(body),created=Math.floor(Date.now()/1000);
  const rootUntil=BigInt(basis.termination.cleanup_until)+(purpose==='reconcile_read'?20000n:-20000n);
  const bound=nonceUntil===null?rootUntil:BigInt(nonceUntil)<rootUntil?BigInt(nonceUntil):rootUntil;
  const expires=Math.min(created+29,Number(bound/1000n));
  assert.ok(expires*1000-Date.now()>1000,'the original signed root/nonce must retain a valid proof window');
  const sha=value=>createHash('sha256').update(value).digest('base64');
  const fields=['@method','@target-uri','content-type','content-digest','oasis7-token-digest','oasis7-test-session','oasis7-request-id'];
  const params=`(${fields.map(name=>JSON.stringify(name)).join(' ')});created=${created};expires=${expires}${nonce===null?'':`;nonce="${nonce}"`};alg="ed25519";keyid="${basis.client_pairing.client_key_id}";tag="${tag}"`;
  const headers={'content-type':'application/json','content-digest':`sha-256=:${sha(raw)}:`,'oasis7-token-digest':`:${sha(token)}:`,'oasis7-test-session':basis.session_id,'oasis7-request-id':requestId};
  const values={'@method':'POST','@target-uri':target,...headers};
  const base=fields.map(name=>`"${name}": ${values[name]}`).concat(`"@signature-params": ${params}`).join('\n');
  headers['signature-input']=`oasis7=${params}`;headers.signature=`oasis7=:${sign(null,Buffer.from(base),key).toString('base64')}:`;
  if(token)headers['oasis7-capability']=token;
  return {path,init:{method:'POST',headers,body:raw},expires};
}
function publicClientCounters(runtime,id){
  return JSON.parse(execFileSync('python3',['-c',`
import json,sqlite3,sys
from pathlib import Path
out=None
def remaining(v):return int(v['limit'])-int(v['spent'])-sum(map(int,v['reservations'].values()))
for p in Path(sys.argv[1]).rglob('*.sqlite'):
 try:
  db=sqlite3.connect(p.as_uri()+'?mode=ro',uri=True)
  row=db.execute("SELECT payload FROM custody_records WHERE id='custody-provision-approval-v1'").fetchone()
  if not row:continue
  state=json.loads(row[0])['state']
  if state['draft']['entry']['custody_account_id']!=sys.argv[2]:continue
  s=state['test_sessions'][sys.argv[3]]
  out={'requests':remaining(s['control']['requests']),'host':remaining(s['control']['transport']),'native':remaining(s['identity_transport']),'sources':len(s['sources']),'roots':{p:remaining(r['budgets']['transport']) for p,r in s['roots'].items()}}
 except sqlite3.OperationalError:continue
 finally:
  if 'db' in locals():db.close()
assert out is not None
print(json.dumps(out,separators=(',',':')))
`,runtime.persist,runtime.account,id],{encoding:'utf8',stdio:['ignore','pipe','pipe']}));
}
async function clientJson(response){assert.equal(response.status,200,'actual public client lane must succeed');return response.json();}
test('public scoped client claims credentials and heartbeats without borrowing purpose budgets',{timeout:300000},async()=>{
  await withActiveSession(async(runtime,basis)=>{
    const initial=publicClientCounters(runtime,basis.session_id);
    const challengePath='proof-challenges';
    const claim={api_version:1,enrollment_id:basis.client_pairing.enrollment_id,test_session_id:basis.session_id,request_id:'public-claim',purposes:['execute','read'],replace:false,expected_generation:null};
    const claimChallenge={api_version:1,session_id:basis.session_id,request_id:'public-claim-challenge',purpose:'execution',kind:{kind:'credential_claim'},logical_request_id:claim.request_id,operation_request_digest:digest('oasis7.custody.test-session-claim.v1',claim)};
    async function authorized(proof){
      const before=runtime.nativeRequests();
      const reply=await clientJson(await runtime.publicClientFetch(proof.path,proof.init));
      const after=runtime.nativeRequests();
      const delta=path=>(after[path]??0)-(before[path]??0);
      assert.equal(delta('/custody/approvals/verify'),1,'each authorized public HTTP has one actual source inspection');
      assert.equal(delta('/custody/managed-bindings/session-lifetime'),1,'each authorized public HTTP has one actual Root lifetime');
      const total=Object.values(after).reduce((a,b)=>a+b,0)-Object.values(before).reduce((a,b)=>a+b,0);
      assert.equal(total,2,'public HTTP must not create hidden extra native calls');
      return reply;
    }
    const challenge=await authorized(publicClientProof(basis,runtime,{tail:challengePath,body:claimChallenge,requestId:claimChallenge.request_id,tag:'oasis7-custody-challenge-v1'}));
    assert.equal(typeof challenge.nonce,'string');
    const claimProof=publicClientProof(basis,runtime,{tail:`client-enrollments/${claim.enrollment_id}/claim`,body:claim,requestId:claim.request_id,tag:'oasis7-custody-enroll-v1',nonce:challenge.nonce,nonceUntil:challenge.expires_at});
    const claimed=await authorized(claimProof);
    assert.equal(claimed.status,'claimed');assert.equal(claimed.credentials.length,2);
    const credentials=new Map();
    for(const item of claimed.credentials){assert.equal(typeof item.token,'string');assert.equal(item.token.length,43);const decoded=Buffer.from(item.token,'base64url');assert.equal(decoded.length,32);assert.equal(decoded.toString('base64url')===item.token,true,'credential token must use canonical base64url');assert.equal(item.generation,'1');credentials.set(item.purpose,item.token);}
    assert.ok(credentials.has('execution')&&credentials.has('reconcile_read'));
    assert.ok(claimProof.expires*1000-Date.now()>1000,'exact consumed-nonce proof must still be fresh for historical retry');
    const historical=await authorized(claimProof);
    assert.equal(historical.status,'historical');assert.equal(historical.credentials.length,2);
    for(const item of historical.credentials){assert.deepEqual(Object.keys(item).sort(),['generation','purpose']);assert.equal(item.generation,'1');}
    const heartbeat={api_version:1,session_id:basis.session_id,request_id:'public-heartbeat',kind:'heartbeat'};
    const heartbeatChallenge={api_version:1,session_id:basis.session_id,request_id:'public-heartbeat-challenge',purpose:'execution',kind:{kind:'control',control:'heartbeat'},logical_request_id:heartbeat.request_id,operation_request_digest:digest('oasis7.custody.test-session-control-request.v1',heartbeat)};
    const heartbeatNonce=await authorized(publicClientProof(basis,runtime,{tail:challengePath,body:heartbeatChallenge,requestId:heartbeatChallenge.request_id,tag:'oasis7-custody-challenge-v1',token:credentials.get('execution')}));
    const heartbeatProof=publicClientProof(basis,runtime,{tail:`test-sessions/${basis.session_id}/heartbeat`,body:heartbeat,requestId:heartbeat.request_id,tag:'oasis7-custody-client-v1',token:credentials.get('execution'),nonce:heartbeatNonce.nonce,nonceUntil:heartbeatNonce.expires_at});
    const ack=await authorized(heartbeatProof);
    assert.equal(ack.kind,'heartbeat');assert.ok(BigInt(ack.lease_until)<=BigInt(basis.termination.cleanup_until)-20000n);
    assert.ok(heartbeatProof.expires*1000-Date.now()>1000,'the exact heartbeat retry must still be live');
    const repeated=await authorized(heartbeatProof);
    assert.equal(repeated.acknowledged_at,ack.acknowledged_at);assert.equal(repeated.lease_until,ack.lease_until,'same logical ack must never extend the lease');
    const paid=publicClientCounters(runtime,basis.session_id);
    assert.equal(paid.host,initial.host-6);assert.equal(paid.native,initial.native-12);assert.equal(paid.requests,initial.requests-2,'challenge and operation share one logical request');assert.deepEqual(paid.roots,initial.roots,'control traffic must not borrow Execute/Read/Termination transport');
    const negativeBody={...heartbeatChallenge,request_id:'wrong-token-challenge',logical_request_id:'wrong-token-logical'};
    const wrongPurpose=publicClientProof(basis,runtime,{tail:challengePath,body:negativeBody,requestId:negativeBody.request_id,tag:'oasis7-custody-challenge-v1',token:credentials.get('reconcile_read')});
    const wrongBody=publicClientProof(basis,runtime,{tail:challengePath,body:{...negativeBody,request_id:'wrong-body-challenge'},requestId:'wrong-body-challenge',tag:'oasis7-custody-challenge-v1',token:credentials.get('execution')});
    wrongBody.init.body=JSON.stringify({...negativeBody,request_id:'mutated-after-signature'});
    const wrongHost=publicClientProof(basis,runtime,{tail:challengePath,body:{...negativeBody,request_id:'wrong-host-challenge'},requestId:'wrong-host-challenge',tag:'oasis7-custody-challenge-v1',token:credentials.get('execution')});wrongHost.init.headers.Host='wrong.example';
    for(const [name,proof] of [['wrong-purpose-token',wrongPurpose],['body-after-signature',wrongBody],['literal-host-fault',wrongHost]]){
      const before=runtime.nativeRequests(),counter=publicClientCounters(runtime,basis.session_id);
      const rejected=await (name==='literal-host-fault'?runtime.publicClientHostFault(proof.path,proof.init):runtime.publicClientFetch(proof.path,proof.init));
      assert.ok(rejected.status>=400,`${name}: invalid public proof must be rejected`);
      assert.deepEqual(runtime.nativeRequests(),before);assert.deepEqual(publicClientCounters(runtime,basis.session_id),counter,'invalid proof never charges a session or creates a source');
    }
  });
});

async function claimCloseCredentials(runtime,basis,label){
  const body={api_version:1,enrollment_id:basis.client_pairing.enrollment_id,test_session_id:basis.session_id,request_id:`${label}-claim`,purposes:['execute','read'],replace:false,expected_generation:null};
  const challenge={api_version:1,session_id:basis.session_id,request_id:`${label}-claim-challenge`,purpose:'execution',kind:{kind:'credential_claim'},logical_request_id:body.request_id,operation_request_digest:digest('oasis7.custody.test-session-claim.v1',body)};
  const nonce=await liveClientCall(runtime,publicClientProof(basis,runtime,{tail:'proof-challenges',body:challenge,requestId:challenge.request_id,tag:'oasis7-custody-challenge-v1'}));
  const claimed=await liveClientCall(runtime,publicClientProof(basis,runtime,{tail:`client-enrollments/${body.enrollment_id}/claim`,body,requestId:body.request_id,tag:'oasis7-custody-enroll-v1',nonce:nonce.nonce,nonceUntil:nonce.expires_at}));
  assert.equal(claimed.status,'claimed');
  const execute=claimed.credentials.find(v=>v.purpose==='execution');assert.equal(typeof execute?.token,'string');assert.equal(execute.token.length,43);
  return execute.token;
}
async function liveClientCall(runtime,proof){
  const native=runtime.nativeRequests();
  const reply=await clientJson(await runtime.publicClientFetch(proof.path,proof.init));
  const after=runtime.nativeRequests();
  assert.equal((after['/custody/approvals/verify']??0)-(native['/custody/approvals/verify']??0),1);
  assert.equal((after['/custody/managed-bindings/session-lifetime']??0)-(native['/custody/managed-bindings/session-lifetime']??0),1);
  assert.equal(Object.values(after).reduce((a,b)=>a+b,0)-Object.values(native).reduce((a,b)=>a+b,0),2,'each live client HTTP uses only its own source/lifetime native calls');
  return reply;
}
async function clientControlChallenge(runtime,basis,token,kind,label){
  const body={api_version:1,session_id:basis.session_id,request_id:`${label}-${kind}`,kind};
  const challenge={api_version:1,session_id:basis.session_id,request_id:`${label}-${kind}-challenge`,purpose:'execution',kind:{kind:'control',control:kind},logical_request_id:body.request_id,operation_request_digest:digest('oasis7.custody.test-session-control-request.v1',body)};
  const nonce=await liveClientCall(runtime,publicClientProof(basis,runtime,{tail:'proof-challenges',body:challenge,requestId:challenge.request_id,tag:'oasis7-custody-challenge-v1',token}));
  return publicClientProof(basis,runtime,{tail:`test-sessions/${basis.session_id}/${kind==='tools_list'?'tools-list':kind}`,body,requestId:body.request_id,tag:'oasis7-custody-client-v1',token,nonce:nonce.nonce,nonceUntil:nonce.expires_at});
}
function closeSessionState(runtime,id){
  return JSON.parse(execFileSync('python3',['-c',`
import json,sqlite3,sys
from pathlib import Path
out=None;reads=0
for p in Path(sys.argv[1]).rglob('*.sqlite'):
 try:
  db=sqlite3.connect(p.as_uri()+'?mode=ro',uri=True)
  if db.execute("SELECT 1 FROM sqlite_master WHERE type='table' AND name='fixture_close_reads'").fetchone():reads+=db.execute('SELECT COUNT(*) FROM fixture_close_reads').fetchone()[0]
  row=db.execute("SELECT payload FROM custody_records WHERE id='custody-provision-approval-v1'").fetchone()
  if not row:continue
  state=json.loads(row[0])['state']
  if state['draft']['entry']['custody_account_id']!=sys.argv[2]:continue
  s=state['test_sessions'][sys.argv[3]]
  out={'closing_reason':s['closing'][0] if s['closing'] else None,'closing_at':s['closing'][1] if s['closing'] else None,'termination_originals':len(s['termination_slots']),'termination_results':sum(v['result'] is not None for v in s['termination_slots'].values()),'lease_until':s['lease_until']}
 except sqlite3.OperationalError:continue
 finally:
  if 'db' in locals():db.close()
assert out is not None
out['private_read_attempts']=reads
print(json.dumps(out,separators=(',',':')))
`,runtime.persist,runtime.account,id],{encoding:'utf8',stdio:['ignore','pipe','pipe']}));
}
for(const kind of ['finish','abort'])test(`public ${kind} tightens without Native or private SID access after its live challenge`,{timeout:300000},async()=>{
  await withActiveSession(async(runtime,basis,selectedMode)=>{
    const initial=publicClientCounters(runtime,basis.session_id);
    const token=await claimCloseCredentials(runtime,basis,kind);
    if(kind==='finish')for(const control of ['initialize','tools_list']){
      const proof=await clientControlChallenge(runtime,basis,token,control,kind);
      const ack=await liveClientCall(runtime,proof);assert.equal(ack.kind,control);
    }
    const closeProof=await clientControlChallenge(runtime,basis,token,kind,kind);
    const before=publicClientCounters(runtime,basis.session_id),native=runtime.nativeRequests();
    const liveHttp=kind==='finish'?7:3,logical=kind==='finish'?4:2;
    assert.equal(before.host,initial.host-liveHttp);assert.equal(before.native,initial.native-liveHttp*2);assert.equal(before.requests,initial.requests-logical);assert.equal(before.sources,initial.sources+liveHttp);assert.deepEqual(before.roots,initial.roots);
    identitySecretsDisabled=true;identityCloseReadTrap=true;
    runtime.setNativeUnavailable(true);
    try {
      await runtime.restart(selectedMode);
      assert.deepEqual(await runtime.probeClientCloseTrap(),{rejected:409,attempts:1,after_reset:0},'the actual rebound namespace trap must reject reads and reset only its own counter');
      assert.ok(closeProof.expires*1000-Date.now()>1000,'closing proof must remain under its original nonce/root deadline');
      const ack=await clientJson(await runtime.publicClientFetch(closeProof.path,closeProof.init));
      assert.equal(ack.kind,kind);
      assert.deepEqual(runtime.nativeRequests(),native,'the close call cannot contact an unavailable Native provider');
      const after=publicClientCounters(runtime,basis.session_id);
      assert.equal(after.host,before.host-1);assert.equal(after.native,before.native);assert.equal(after.requests,before.requests,'the already reserved close logical request is not charged twice');assert.deepEqual(after.roots,initial.roots,'closing cannot borrow business/read/cleanup transports');
      await runtime.suspend();const state=closeSessionState(runtime,basis.session_id);
      assert.equal(state.private_read_attempts,0,'close cannot read the deny-all SID actor namespace');
      assert.equal(state.closing_reason,kind==='finish'?'finished':'aborted');assert.equal(state.termination_originals,0);assert.equal(state.termination_results,0,'closing acknowledgement is not World Stop/Revoke completion');
      await runtime.restart(selectedMode);
      assert.ok(closeProof.expires*1000-Date.now()>1000,'same ack retry remains bounded by its original proof');
      const repeated=await clientJson(await runtime.publicClientFetch(closeProof.path,closeProof.init));
      assert.equal(repeated.acknowledged_at,ack.acknowledged_at);assert.equal(repeated.lease_until,ack.lease_until);
      assert.deepEqual(runtime.nativeRequests(),native);
      const paidRetry=publicClientCounters(runtime,basis.session_id);assert.equal(paidRetry.host,after.host-1);assert.equal(paidRetry.native,before.native);assert.equal(paidRetry.requests,before.requests);assert.deepEqual(paidRetry.roots,initial.roots);
      await runtime.suspend();const same=closeSessionState(runtime,basis.session_id);
      assert.equal(same.private_read_attempts,0);assert.equal(same.closing_at,state.closing_at);assert.equal(same.termination_results,0);
    } finally {identitySecretsDisabled=false;identityCloseReadTrap=false;runtime.setNativeUnavailable(false);}
  });
});


test('real Account alarm re-arms an early wake and expires across restart without cleanup dispatch',{timeout:300000},async()=>{
  await withActiveSession(async(runtime,basis,selectedMode)=>{
    accountAlarmFixtureActive=true;
    try {
      await runtime.restart(selectedMode);
      const native=runtime.nativeRequests();
      const initial=await runtime.accountAlarmFixture('observe',basis.session_id);
      assert.equal(initial.events,0);assert.equal(initial.closing_reason,null);
      const expiryAt=Math.min(Number(initial.execution_deadline),Number(initial.lease_until));
      assert.ok(initial.scheduled_at!==null&&initial.scheduled_at<=expiryAt,'an earlier conservative prearm may be retained, never a later expiry timer');
      assert.ok(expiryAt-Date.now()>2000,'an early wake must precede the original actual deadline');
      await runtime.accountAlarmFixture('early',basis.session_id);
      let early;
      for(let n=0;n<30;n++){
        early=await runtime.accountAlarmFixture('observe',basis.session_id);
        if(early.events>=1&&early.scheduled_at===expiryAt)break;
        await new Promise(resolve=>setTimeout(resolve,100));
      }
      assert.ok(early.events>=1,'actual workerd alarm handler must have executed');
      assert.equal(early.scheduled_at,expiryAt);assert.equal(early.lease_until,initial.lease_until);
      assert.equal(early.closing_reason,null);assert.equal(early.audits,initial.audits,'early wake writes no expiry audit');assert.deepEqual(early.budgets,initial.budgets);
      await runtime.restart(selectedMode);
      const restored=await runtime.accountAlarmFixture('observe',basis.session_id);
      assert.equal(restored.events,0,'a new actual DO instance has a fresh diagnostic counter');assert.equal(restored.scheduled_at,expiryAt,'the real storage alarm survives restart');assert.equal(restored.lease_until,initial.lease_until);
      const wait=Math.max(0,expiryAt-Date.now())+50;
      await new Promise(resolve=>setTimeout(resolve,wait));
      let expired;
      for(let n=0;n<30;n++){
        expired=await runtime.accountAlarmFixture('observe',basis.session_id);
        if(expired.closing_reason!==null&&expired.scheduled_at===null)break;
        await new Promise(resolve=>setTimeout(resolve,100));
      }
      assert.ok(expired.events>=1,'deadline expiry is driven by the persisted alarm, not observer fetch repair');
      assert.equal(expired.closing_reason,'deadline');assert.equal(expired.scheduled_at,null,'Closing session does not rearm its old deadlines');assert.equal(expired.lease_until,initial.lease_until);
      assert.equal(expired.audits,initial.audits+1);assert.equal(expired.termination_originals,0);assert.equal(expired.termination_results,0,'expiry is not a Closed/Stop/Revoke receipt');assert.deepEqual(expired.budgets,initial.budgets);assert.deepEqual(runtime.nativeRequests(),native);
      await runtime.accountAlarmFixture('early',basis.session_id);
      let duplicate;
      for(let n=0;n<30;n++){
        duplicate=await runtime.accountAlarmFixture('observe',basis.session_id);
        if(duplicate.events>expired.events&&duplicate.scheduled_at===null)break;
        await new Promise(resolve=>setTimeout(resolve,100));
      }
      assert.ok(duplicate.events>expired.events);assert.equal(duplicate.audits,expired.audits,'duplicate real alarm adds no audit');assert.equal(duplicate.closing_at,expired.closing_at);assert.equal(duplicate.scheduled_at,null);assert.deepEqual(duplicate.budgets,initial.budgets);assert.deepEqual(runtime.nativeRequests(),native);
    }finally{accountAlarmFixtureActive=false;}
  });
});


for(const fault of ['set-once','sql-once'])test(`actual ${fault} alarm fault prevents first TestSession begin and does not consume approval`,{timeout:300000},async()=>{
  await withActiveSession(async()=>assert.fail('a rejected first begin must not activate a session'),{approvalFault:async(runtime,candidate,selectedMode)=>{
    accountAlarmFault=fault;
    try{
      await runtime.restart(selectedMode);
      const before=await runtime.accountAlarmFixture('observe',candidate.basis.session_id);
      assert.equal(before.sessions,0);assert.equal(before.roots,0);assert.equal(before.scheduled_at,null);
      const native=runtime.nativeRequests();
      const rejected=await runtime.managementFetch('/test-sessions/approve',{api_version:1,input:{operation_id:candidate.basis.session_id,request:candidate.request,approval:candidate.approval},plan:candidate.plan});
      assert.ok(rejected.status>=400,'a real scheduling or final SQL fault must reject first begin');
      const after=await runtime.accountAlarmFixture('observe',candidate.basis.session_id);
      assert.equal(after.sessions,0);assert.equal(after.roots,0);assert.equal(after.consumed,0);assert.equal(after.audits,before.audits);assert.equal(after.begin_audits,before.begin_audits);assert.equal(after.record_digest,before.record_digest,'actual final SQL and audit roll back together');
      assert.equal((runtime.nativeRequests()['/custody/approvals/verify']??0)-(native['/custody/approvals/verify']??0),0,'no pending begin means no Native approval consumption');
      if(fault==='set-once'){
        assert.equal(after.set_faults,1);assert.equal(after.sql_faults,0);assert.equal(after.scheduled_at,null,'a failed initial scheduling attempt cannot pretend an alarm exists');
      }else{
        assert.equal(after.sql_faults,1);assert.equal(after.final_update_seen,true,'the fault occurred after the real final record update');assert.equal(after.set_faults,0);assert.ok(after.scheduled_at!==null,'successful prearm survives the rolled-back SQL');assert.ok(after.scheduled_at<=Math.min(Number(candidate.approval.proof.expires_at),Number(candidate.plan.write_deadline)));
      }
    }finally{accountAlarmFault=null;}
  }});
});

test('unknown real Account inventory keeps its earlier alarm and strict public rejection',{timeout:300000},async()=>{
  await withActiveSession(async(runtime,basis,selectedMode)=>{
    accountAlarmFault='unknown-inventory';
    try{
      await runtime.restart(selectedMode);
      const before=await runtime.accountAlarmFixture('observe',basis.session_id);
      assert.ok(before.scheduled_at!==null);
      const native=runtime.nativeRequests(),counters=publicClientCounters(runtime,basis.session_id);
      await runtime.accountAlarmFixture('unknown',basis.session_id);
      // This is a valid paired-key proof. Both maintenance and its actual business
      // route see the real unexpected SQLite table; neither may treat it as empty.
      const original=clientClaimChallenge(basis,runtime);
      const response=await runtime.publicClientFetch(new URL(original.http.original.target_uri).pathname,{method:'POST',headers:Object.fromEntries(original.http.original.headers.map(h=>[h.name,h.value])),body:original.http.original.body});
      assert.equal(response.status,409,'unknown inventory must remain a strict pending denial');
      const after=await runtime.accountAlarmFixture('observe',basis.session_id);
      assert.equal(after.record_digest,before.record_digest);assert.equal(after.audits,before.audits);assert.equal(after.roots,before.roots);
      assert.ok(after.scheduled_at!==null,'maintenance failure must not delete the pending timer');assert.equal(after.scheduled_at,before.scheduled_at,'fixed retry must preserve an already earlier real timer');
      assert.deepEqual(runtime.nativeRequests(),native);assert.deepEqual(publicClientCounters(runtime,basis.session_id),counters,'unknown inventory cannot spend session budgets or dispatch');
    }finally{accountAlarmFault=null;}
  });
});


test('actual activation independent readonly inputs complete together under the original freshness window',{timeout:300000},async()=>{
  await withActiveSession(async()=>assert.fail('the controlled preflight callback owns this case'),{preflightCase:async(runtime,basis,selectedMode)=>{
    activationReadDelay=true;
    try{
      await runtime.restart(selectedMode);
      const before=runtime.nativeRequests();
      const response=await runtime.managementFetch('/test-sessions/continue',{api_version:1,session_id:basis.session_id});
      const status=response.status;
      const reads=await runtime.activationReadDelayStatus();
      const after=runtime.nativeRequests();
      const paths={'/custody/management-session-status':1,'/custody/approvals/verify':1,'/participant/policy-original':1,'/participant/authorize':1,'/custody/managed-bindings/session-status':2,'/custody/managed-bindings/session-lifetime':1,'/participant/owner-view':1};
      if(status!==200){
        await response.arrayBuffer();await runtime.suspend();
        const native_request_deltas=Object.fromEntries(Object.keys(paths).map(path=>[path,(after[path]??0)-(before[path]??0)]));
        console.error(JSON.stringify({activation_read_delay_failure:{status,...reads,native_request_deltas,state:continueFailureState(runtime,basis.session_id)}}));
      }
      assert.equal(status,200,'activation must succeed with both actual independent readonly inputs inside its unchanged freshness window');
      for(const [path,count] of Object.entries(paths))assert.equal((after[path]??0)-(before[path]??0),count,'the controlled delay must preserve actual native hop counts');
      assert.equal(reads.history_reads,1);assert.equal(reads.enrollment_reads,1,'successful activation must invoke both actual fixed bindings exactly once');
      assert.equal(reads.released,'independent_reads_started');assert.ok(reads.elapsed_ms<5000,'the batching delay is released only when both independent requests start');
      const result=await response.json();assert.equal(result.stage,'active');assert.equal(result.purposes.length,3);assert.equal(result.identity_control_http_remaining,'152');
      const roots=Object.fromEntries(result.purposes.map(p=>[p.purpose,p.transport_remaining]));assert.deepEqual(roots,{execution:'400',reconcile_read:'300',termination_plan:'24'});
    }finally{activationReadDelay=false;}
  }});
});

for(const fault of ['reject_history','over_window'])test(`actual concurrent activation rejects ${fault} without granting roots`,{timeout:300000},async()=>{
  await withActiveSession(async()=>assert.fail('the controlled preflight callback owns this case'),{preflightCase:async(runtime,basis,selectedMode)=>{
    activationReadDelay=fault;
    try{
      await runtime.restart(selectedMode);
      const before=runtime.nativeRequests();
      const response=await runtime.managementFetch('/test-sessions/continue',{api_version:1,session_id:basis.session_id});
      const status=response.status;await response.arrayBuffer();
      const reads=await runtime.activationReadDelayStatus();
      const after=runtime.nativeRequests();
      assert.equal(reads.history_reads,1);assert.equal(reads.enrollment_reads,1,'both concurrent fixed readonly inputs must actually start');
      if(fault==='reject_history'){
        assert.equal(reads.history_completed,1,'the fault never replaces the actual private history read');
        assert.equal(reads.history_rejected,true);assert.equal(reads.released,'independent_reads_started');assert.ok(reads.elapsed_ms<5000);
      }else{
        assert.equal(reads.history_rejected,false);assert.equal(reads.released,'bounded_timeout');assert.ok(reads.elapsed_ms>=5000);
      }
      assert.equal(status,503,'failed or stale readonly input cannot authorize activation');
      const paths={'/custody/management-session-status':1,'/custody/approvals/verify':1,'/participant/policy-original':1,'/participant/authorize':1,'/custody/managed-bindings/session-status':2,'/custody/managed-bindings/session-lifetime':1,'/participant/owner-view':1};
      for(const [path,count] of Object.entries(paths))assert.equal((after[path]??0)-(before[path]??0),count,'concurrent rejection cannot add native refreshes or retry business');
      await runtime.suspend();const finalState=stored(runtime,basis.session_id);
      assert.equal(finalState.consumed,true);assert.equal(finalState.approved,false);assert.deepEqual(finalState.roots,[]);assert.equal(finalState.credentials,0);assert.equal(finalState.terminations,0);assert.equal(finalState.reads,5);assert.deepEqual(finalState.source_reads,[7]);
    }finally{activationReadDelay=false;}
  }});
});

for(const barrierMode of ['together','over_window','reject_policy'])test(`actual fresh-access Root statuses overlap ${barrierMode} without reordering policy and authorize`,{timeout:300000},async()=>{
  await withActiveSession(async()=>assert.fail('the controlled preflight callback owns this case'),{preflightCase:async(runtime,basis,selectedMode)=>{
    await runtime.restart(selectedMode);await runtime.armAccessStatusBarrier(basis.session_id,barrierMode);
    const before=runtime.nativeRequests(),startedAt=Date.now();
    const response=await runtime.managementFetch('/test-sessions/continue',{api_version:1,session_id:basis.session_id});
    const status=response.status;const facts=runtime.accessStatusBarrierFacts();const after=runtime.nativeRequests();
    if(status!==200){
      await response.arrayBuffer();const observedAt=Date.now();await runtime.suspend();
      console.error(JSON.stringify({access_status_barrier:{status,...facts,native_status_delta:(after['/custody/managed-bindings/session-status']??0)-(before['/custody/managed-bindings/session-status']??0),state:continueFailureState(runtime,basis.session_id)}}));
      assert.ok(Number.isSafeInteger(observedAt));
    }
    if(barrierMode==='together'){
      assert.equal(status,200,'overlapped Root statuses must leave the unchanged policy-before-authorize path fresh');
      assert.equal(facts.policy_status,1);assert.equal(facts.authorize_status,1);assert.equal(facts.released,'independent_statuses_started');assert.ok(facts.elapsed_ms<5000);assert.equal(facts.policy_completed,1);assert.equal(facts.authorize_after_policy,true);
      const progress=await response.json();assert.equal(progress.stage,'active');assert.equal(progress.purposes.length,3);
    }else{
      assert.equal(facts.policy_status,1);assert.equal(facts.authorize_status,1);assert.equal(status,503);
      if(barrierMode==='reject_policy'){
        assert.equal(facts.released,'independent_statuses_started');assert.equal(facts.policy_http_status,200,'the injected failure requires an actually successful native policy response');assert.equal(facts.policy_completed,1);assert.equal(facts.policy_rejected,true);assert.equal(facts.authorize_after_policy,false);
        assert.equal((after['/participant/authorize']??0)-(before['/participant/authorize']??0),0,'a rejected actual policy response must prevent authorize mutation');
      }else{
        assert.ok(Date.now()-startedAt>=5000,'the actual readonly transport remains bounded by its unchanged five-second limit');
        // The real native transport may abort at5000 before the5100 fixture timer.
        assert.ok(facts.released===null||facts.released==='bounded_timeout');
        if(facts.released!==null)assert.ok(facts.elapsed_ms>=5000);
      }
      const finalState=stored(runtime,basis.session_id);assert.equal(finalState.approved,false);assert.deepEqual(finalState.roots,[]);
    }
    assert.equal((after['/custody/managed-bindings/session-status']??0)-(before['/custody/managed-bindings/session-status']??0),2,'the exact two prepaid Root status calls cannot refresh or retry');
  }});
});

for(const fault of ['together','reject_gate','freeze_fence','over_window'])test(`actual paid factory Directory and Fence overlap ${fault} after SQL ticket3`,{timeout:300000},async()=>{
  await withActiveSession(async()=>assert.fail('controlled preflight owns this case'),{preflightCase:async(runtime,basis,selectedMode)=>{
    paidDirectoryBarrier=fault;
    try{
      await runtime.restart(selectedMode);const before=runtime.nativeRequests();
      const response=await runtime.managementFetch('/test-sessions/continue',{api_version:1,session_id:basis.session_id});const status=response.status;
      const facts=await runtime.paidDirectoryFenceStatus();const after=runtime.nativeRequests();
      if(status!==200){await response.arrayBuffer();const observedAt=Date.now();await runtime.suspend();console.error(JSON.stringify({paid_directory_fence_failure:{status,facts,diagnostic_started_at:observedAt,state:continueFailureState(runtime,basis.session_id)}}));}
      if(fault==='together'){
        assert.equal(status,200,'fixed paid Directory/Fence reads must complete within unchanged freshness');
        const progress=await response.json();assert.equal(progress.stage,'active');assert.equal(progress.purposes.length,3);
      }else{
        assert.equal(status,503,'failed/frozen/stale independent readonly input must prevent grant');
        if(fault==='reject_gate'){assert.equal(facts.gate_status,200);assert.equal(facts.gate_faulted,true);}
        if(fault==='freeze_fence')assert.equal(facts.fence_disabled,true);
        if(fault==='over_window'){assert.equal(facts.released,'bounded_timeout');assert.ok(facts.elapsed_ms>=5000);}
        const saved=stored(runtime,basis.session_id);assert.equal(saved.approved,false);assert.deepEqual(saved.roots,[]);
        assert.equal((after['/custody/managed-bindings/session-lifetime']??0)-(before['/custody/managed-bindings/session-lifetime']??0),0);
        assert.equal((after['/participant/owner-view']??0)-(before['/participant/owner-view']??0),0);
      }
      assert.equal(facts.gate_reads,1);assert.equal(facts.fence_reads,1);assert.equal(facts.paid_confirmations,2);assert.equal(facts.oracle_ticket,3);assert.equal(facts.source_ticket,3);
      if(fault!=='over_window'){assert.equal(facts.released,'independent_reads_started');assert.ok(facts.elapsed_ms<5000);}
      assert.equal((after['/custody/managed-bindings/session-status']??0)-(before['/custody/managed-bindings/session-status']??0),2);
      assert.equal((after['/participant/policy-original']??0)-(before['/participant/policy-original']??0),1);
      assert.equal((after['/participant/authorize']??0)-(before['/participant/authorize']??0),1);
    }finally{paidDirectoryBarrier=null;}
  }});
});
