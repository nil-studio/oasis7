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
function configureOptions(options){
  assert.equal(options.workers[0]?.name,'identity','public dispatchFetch must target the actual Identity ingress');
  let configuredRoles=0;
  for(const worker of options.workers){
    if(!['identity','owner'].includes(worker.name))continue;
    configuredRoles++;
    if(worker.name==='owner' && ownerDiagnosticActive && process.env.OASIS7_TEST_SESSION_OWNER_DIAGNOSTIC==='true') {
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
    worker.bindings={...worker.bindings,CUSTODY_TEST_SESSION_ENABLED:'true',CUSTODY_CLIENT_ENROLLMENT_ENABLED:'true',CUSTODY_PUBLIC_ORIGIN:origin,CUSTODY_ENROLLMENT_REALM:'session-fixture-enrollment',CUSTODY_HOST_DEPLOYMENTS:JSON.stringify([{environment:'test',host_instance_id:descriptor.host_instance_id,descriptor_digest:descriptorDigest,client_public_key:publicKey}])};
    worker.durableObjects={...worker.durableObjects,CLIENT_ENROLLMENTS:{className:'ClientEnrollmentAuthority',useSQLite:true,...(worker.name==='owner'?{scriptName:'identity'}:{})}};
    if(worker.name==='identity')worker.modules=[{type:'ESModule',path:resolve(root,'tests/fixtures/test_session_runtime.mjs')},...worker.modules.filter(m=>!m.path.endsWith('/native_revocation_scheduler.mjs'))];
    else worker.serviceBindings={...worker.serviceBindings,IDENTITY_TEST_SESSIONS:{name:'identity',entrypoint:'IdentityTestSessions'},TEST_SESSION_PRIVATE_HISTORY:{name:'identity',entrypoint:'TestSessionPrivateHistory'}};
    if(worker.name==='identity' && identityDiagnosticActive && process.env.OASIS7_TEST_SESSION_IDENTITY_DIAGNOSTIC==='true') {
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

test('real fresh MFA consumes control-only authority before isolated Host preflight activates three independent roots',{timeout:300000},async()=>{
  ownerDiagnosticActive=false;identityDiagnosticActive=false;
  await withRoleRuntime(mode,async runtime=>{
    assert.equal((await runtime.drive('session-participant')).stage,'participant_bound');
    const setup=setupPlan(await runtime.setupBasis());await runtime.approveSetup(setup);
    const setupProgress=await json(await runtime.continueSetup(setup.setup_id));
    if(setupProgress.stage!=='prepared') {
      await runtime.suspend();
      const saved=persistedRuntime(runtime.persist,runtime.account);
      const keys=['stage','reads','has_result','has_original','has_access','has_world','source_created_at','source_verified_at','source_observed_at','source_expires_at','reserved_at','published_at','admission_callback_claimed_at','admitted_at','claimed_at','completed_at'];
      const summary={stage:saved.provision_stage,setups:saved.setups.map(setup=>({stages:setup.stages.map(stage=>Object.fromEntries(keys.map(key=>[key,stage[key]])))}))};
      assert.fail(JSON.stringify({baseline_prepared:false,summary}));
    }
    if(process.env.OASIS7_TEST_SESSION_OWNER_DIAGNOSTIC==='true'||process.env.OASIS7_TEST_SESSION_IDENTITY_DIAGNOSTIC==='true') {
      // Preserve the actual production Account/SQLite; only target operations use diagnostic Wasm.
      ownerDiagnosticActive=process.env.OASIS7_TEST_SESSION_OWNER_DIAGNOSTIC==='true';
      identityDiagnosticActive=process.env.OASIS7_TEST_SESSION_IDENTITY_DIAGNOSTIC==='true';
      await runtime.restart(mode);
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
    const consumed=await json(await runtime.managementFetch('/test-sessions/approve',{api_version:1,input:{operation_id:basis.session_id,request,approval},plan}));
    assert.equal(consumed.stage,'consumed_pending_preflight');assert.deepEqual(consumed.purposes,[]);assert.equal(consumed.identity_control_http_remaining,'159');
    await runtime.suspend();const beforeState=stored(runtime,basis.session_id);assert.equal(beforeState.consumed,true);assert.deepEqual(beforeState.roots,[]);assert.equal(beforeState.approved,false);
    await runtime.restart(mode);
    const before=runtime.nativeRequests();
    const active=await json(await runtime.managementFetch('/test-sessions/continue',{api_version:1,session_id:basis.session_id}));
    assert.equal(active.stage,'active');assert.equal(active.write_closed,false);assert.deepEqual(active.pending_operations,[]);
    assert.deepEqual(active.purposes.map(p=>p.purpose).sort(),['execution','reconcile_read','termination_plan']);
    const roots=Object.fromEntries(active.purposes.map(p=>[p.purpose,p]));
    for(const [purpose,transport] of [['execution','400'],['reconcile_read','300'],['termination_plan','24']])assert.equal(roots[purpose].transport_remaining,transport);
    assert.equal(roots.execution.deadline,plan.write_deadline);assert.equal(roots.reconcile_read.deadline,plan.read_until);assert.equal(roots.termination_plan.deadline,plan.cleanup_until);
    const after=runtime.nativeRequests(),delta=path=>(after[path]??0)-(before[path]??0);
    assert.equal(delta('/participant/policy-original'),1);assert.equal(delta('/participant/authorize'),1);assert.equal(delta('/custody/managed-bindings/session-status'),2,'FreshAccess pair includes two actual Root status HTTP calls');assert.equal(delta('/custody/managed-bindings/session-lifetime'),1);assert.equal(delta('/participant/owner-view'),1);assert.equal(delta('/custody/approvals/verify'),1,'preflight has its own real source inspection');assert.equal(active.identity_control_http_remaining,'152');
    await runtime.suspend();const finalState=stored(runtime,basis.session_id);assert.deepEqual(finalState.roots,['execution','reconcile_read','termination_plan']);assert.equal(finalState.credentials,0);assert.equal(finalState.terminations,0);assert.equal(finalState.reads,5);assert.deepEqual(finalState.source_reads,[7]);assert.deepEqual(finalState.steps,['fresh_access_pair','lifetime','owner_read']);
    await runtime.restart(mode);const count=runtime.nativeRequests();const retry=await json(await runtime.managementFetch('/test-sessions/continue',{api_version:1,session_id:basis.session_id}));assert.equal(retry.stage,'active');assert.deepEqual(runtime.nativeRequests(),{...count,'/custody/management-session-status':(count['/custody/management-session-status']??0)+1},'history does not repeat source or preflight business HTTP');
  },{configureOptions});
});
