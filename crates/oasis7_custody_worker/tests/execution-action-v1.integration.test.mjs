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
let setupPaidResultBarrier=false;
let setupPaidResultStage='bind';
let toolNonceFixtureActive=false;
let toolNonceFaultMode=null;
const toolNonceSignalNames=["sql_fee_confirmed","sql_update_fault","reply_lost","actions_context"];
let toolNonceSignals=Object.fromEntries(toolNonceSignalNames.map(name=>[name,0]));
const toolNoncePortNames=["public","provider","realm","owner","account"];
let toolNoncePorts=Object.fromEntries(toolNoncePortNames.map(name=>[name,0]));
function configureOptions(options){
  assert.equal(options.workers[0]?.name,'identity','public dispatchFetch must target the actual Identity ingress');
  if(toolNonceFixtureActive)options.handleStructuredLogs=record=>{
    const name=toolNoncePortNames.find(name=>record.message.trim()===`fixture.tool_nonce.${name}`);
    if(name){toolNoncePorts[name]++;return;}
    const signal=toolNonceSignalNames.find(name=>record.message.trim()===`fixture.tool_nonce.${name}`);
    if(signal){toolNonceSignals[signal]++;return;}
    if(record.level==='error'||record.level==='warn')console.error(record.message);
    else console.log(record.message);
  };
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
    if(setupPaidResultBarrier&&worker.name==='owner'){
      worker.bindings={...worker.bindings,FIXTURE_SETUP_PAID_RESULT_BARRIER:'true',FIXTURE_SETUP_PAID_RESULT_STAGE:setupPaidResultStage,FIXTURE_SETUP_PAID_RESULT_MODE:typeof setupPaidResultBarrier==='string'?setupPaidResultBarrier:'together'};
      worker.serviceBindings={...worker.serviceBindings,FIXTURE_FENCE_CONTROL:{name:'fence',entrypoint:'RecoveryFenceControl'}};
      worker.modules=[{type:'ESModule',path:resolve(root,'tests/fixtures/setup_paid_result_barrier.mjs')},...worker.modules];
    }
    if(paidDirectoryBarrier&&worker.name==='owner'){
      const paidMode=typeof paidDirectoryBarrier==='string'?paidDirectoryBarrier:paidDirectoryBarrier.mode;
      const paidTicket=typeof paidDirectoryBarrier==='string'?3:paidDirectoryBarrier.ticket;
      assert.ok([3,4,5].includes(paidTicket));
      worker.bindings={...worker.bindings,FIXTURE_PAID_DIRECTORY_MODE:paidMode,FIXTURE_PAID_DIRECTORY_TICKET:String(paidTicket)};
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
  if(toolNonceFixtureActive)for(const worker of options.workers){
    if(worker.name==='identity')worker.modules=[{type:'ESModule',path:resolve(root,'tests/fixtures/tool_nonce_identity_runtime.mjs')},...worker.modules];
    if(worker.name==='owner'){worker.bindings={...worker.bindings,...(toolNonceFaultMode?{FIXTURE_TOOL_NONCE_FAULT:toolNonceFaultMode}:{})};worker.modules=[{type:'ESModule',path:resolve(root,'tests/fixtures/tool_nonce_owner_runtime.mjs')},...worker.modules];}
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
async function withActiveSession(run,{lostInitial=false,approvalFault=null,preflightCase=null,toolNonceFixture=false,wrongBasisDigest=false}={}){
  const previousFixtureState={ownerDiagnosticActive,identityDiagnosticActive,identitySecretsDisabled,identityCloseReadTrap,accountAlarmFixtureActive,accountAlarmFault,activationReadDelay,paidDirectoryBarrier,toolNonceFixtureActive,toolNonceFaultMode,toolNonceSignals,toolNoncePorts};
  const selectedMode=lostInitial?mode+'-lost-initial':mode;
  toolNonceFixtureActive=toolNonceFixture;
  toolNonceFaultMode=null;
  toolNonceSignals=Object.fromEntries(toolNonceSignalNames.map(name=>[name,0]));
  toolNoncePorts=Object.fromEntries(toolNoncePortNames.map(name=>[name,0]));
  ownerDiagnosticActive=setupDiagnostic;identityDiagnosticActive=setupDiagnostic;identitySecretsDisabled=false;identityCloseReadTrap=false;accountAlarmFixtureActive=false;accountAlarmFault=null;activationReadDelay=false;paidDirectoryBarrier=null;
  try {
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
    const basisResponse=await json(await runtime.managementFetch('/test-sessions/basis',{api_version:1,request_id:'session-plan',enrollment_id:view.enrollment_id,cleanup_until:String(cleanup)}));
    const {basis,basis_digest}=basisResponse;
    const plan={api_version:1,request_id:basis.request_id,enrollment_id:view.enrollment_id,basis_digest,actions:[{tool:'move',position_cm:['11','22','33']}],allowed_resource_refs:basis.allowed_resource_refs,write_deadline:String(cleanup-20000),read_until:String(cleanup+20000),cleanup_until:String(cleanup),request_limit:'100',transport_limit:'400',business_limit:'0',read_budget:{requests:'100',transport:'300',business:'0'},cleanup_budget:{requests:'2',transport:'24',business:'0'},control_budget:{requests:'80',transport_per_hop:'240',identity_transport:'160',transport_hops:1}};
    if(wrongBasisDigest){
      const changedDigest=(basis_digest[0]==='0'?'1':'0')+basis_digest.slice(1);
      const rejected=await runtime.managementFetch('/test-sessions/prepare',{api_version:1,approval_request_id:'wrong-basis-approval',plan:{...plan,basis_digest:changedDigest}});
      assert.equal(rejected.status,503,'a changed Rust basis commitment must be rejected before fresh MFA');
      await rejected.arrayBuffer();
    }
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
    if(process.env.OASIS7_TEST_SESSION_OWNER_DIAGNOSTIC==='true') {
      const metrics=await runtime.readAccountRestoreCallbacks();
      console.error(JSON.stringify({restore_profile:{attribution_complete:metrics.attribution_complete,observed_callback_count:metrics.observed_callback_count,samples:metrics.recent_callbacks.map(sample=>({event:sample.event,bytes:sample.bytes,decode_count:sample.decode_count,event_clock_ms:sample.event_clock_ms,strict_counts:Object.fromEntries(Object.entries(sample.strict_signature_metrics?.summary??{}).filter(([,value])=>typeof value==='number'))}))}}));
    }

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
  } finally {
    ({ownerDiagnosticActive,identityDiagnosticActive,identitySecretsDisabled,identityCloseReadTrap,accountAlarmFixtureActive,accountAlarmFault,activationReadDelay,paidDirectoryBarrier,toolNonceFixtureActive,toolNonceFaultMode,toolNonceSignals,toolNoncePorts}=previousFixtureState);
  }
}

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
async function clientJson(response){assert.equal(response.status,200,'actual public client lane must succeed');return response.json();}
async function claimCloseCredentials(runtime,basis,label,returnAll=false){
  const body={api_version:1,enrollment_id:basis.client_pairing.enrollment_id,test_session_id:basis.session_id,request_id:`${label}-claim`,purposes:['execute','read'],replace:false,expected_generation:null};
  const challenge={api_version:1,session_id:basis.session_id,request_id:`${label}-claim-challenge`,purpose:'execution',kind:{kind:'credential_claim'},logical_request_id:body.request_id,operation_request_digest:digest('oasis7.custody.test-session-claim.v1',body)};
  const nonce=await liveClientCall(runtime,publicClientProof(basis,runtime,{tail:'proof-challenges',body:challenge,requestId:challenge.request_id,tag:'oasis7-custody-challenge-v1'}));
  const claimed=await liveClientCall(runtime,publicClientProof(basis,runtime,{tail:`client-enrollments/${body.enrollment_id}/claim`,body,requestId:body.request_id,tag:'oasis7-custody-enroll-v1',nonce:nonce.nonce,nonceUntil:nonce.expires_at}));
  assert.equal(claimed.status,'claimed');
  const execute=claimed.credentials.find(v=>v.purpose==='execution');assert.equal(typeof execute?.token,'string');assert.equal(execute.token.length,43);
  if(returnAll){
    const tokens=new Map(claimed.credentials.map(item=>[item.purpose,item.token]));
    assert.equal(tokens.size,2);assert.equal(typeof tokens.get('reconcile_read'),'string');assert.equal(tokens.get('reconcile_read').length,43);
    return tokens;
  }
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

// Target regression: real approval/activation/PoP Control succeeds first.
// The separately closed business challenge issuer is the first missing stage;
// a ToolMetadata nonce is deliberately never used for business admission. No Book/state seed, Native signer or fake Current.
test('approved V1 Move reaches real public Actions and an authoritative World result', {timeout:300000}, async()=>{
  await withActiveSession(async(runtime,basis)=>{
    const token=await claimCloseCredentials(runtime,basis,'v1-action');
    const initialize={api_version:1,session_id:basis.session_id,request_id:'v1-action-auth-control',kind:'initialize'};
    const initializeChallenge={api_version:1,session_id:basis.session_id,request_id:'v1-action-auth-challenge',purpose:'execution',kind:{kind:'control',control:'initialize'},logical_request_id:initialize.request_id,operation_request_digest:digest('oasis7.custody.test-session-control-request.v1',initialize)};
    const initChallengeProof=publicClientProof(basis,runtime,{tail:'proof-challenges',body:initializeChallenge,requestId:initializeChallenge.request_id,tag:'oasis7-custody-challenge-v1',token});
    const initNonce=await liveClientCall(runtime,initChallengeProof);
    const initProof=publicClientProof(basis,runtime,{tail:`test-sessions/${basis.session_id}/initialize`,body:initialize,requestId:initialize.request_id,tag:'oasis7-custody-client-v1',token,nonce:initNonce.nonce,nonceUntil:initNonce.expires_at});
    const initialized=await liveClientCall(runtime,initProof);assert.equal(initialized.kind,'initialize');
    const action={api_version:1,session_id:basis.session_id,request_id:'v1-action-move',action:{kind:'execute',request:{tool:'move',position_cm:['11','22','33']}}};
    const challenge={api_version:1,session_id:basis.session_id,request_id:'v1-action-nonce',purpose:'execution',kind:{kind:'tool'},logical_request_id:action.request_id,operation_request_digest:digest('oasis7.custody.test-session-tool-request.v1',action)};
    const nonceProof=publicClientProof(basis,runtime,{tail:`test-sessions/${basis.session_id}/action-challenges`,body:challenge,requestId:challenge.request_id,tag:'oasis7-custody-challenge-v1',token});
    const nonceResponse=await runtime.publicClientFetch(nonceProof.path,nonceProof.init);
    assert.equal(nonceResponse.status,200,'a separately paid business challenge issuer is required after real Control authentication; metadata nonce is not business authority');
    const nonce=await nonceResponse.json();
    assert.equal(nonce.purpose,'execution');assert.equal(nonce.kind?.kind,'tool','only a purpose-funded Tool challenge may authorize this action');
    assert.equal(typeof nonce.nonce,'string');assert.equal(nonce.nonce.length,43);
    const proof=publicClientProof(basis,runtime,{tail:'actions',body:action,requestId:action.request_id,tag:'oasis7-custody-client-v1',token,nonce:nonce.nonce,nonceUntil:nonce.expires_at});
    const before=runtime.worldResults().length;
    const response=await runtime.publicClientFetch(proof.path,proof.init);
    assert.equal(response.status,200,'the exact approved Move must cross the real Actions dispatcher after valid PoP and paid nonce');
    await response.arrayBuffer();
    assert.ok(runtime.worldResults().length>before,'HTTP acceptance alone is insufficient: actual World result is required');
  },{toolNonceFixture:true});
});
