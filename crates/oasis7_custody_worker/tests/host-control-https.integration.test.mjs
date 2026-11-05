// Explicit full integration requires actual Linux CLI/image/Skill and local trusted TLS CA.
import assert from 'node:assert/strict';import test from 'node:test';
import {createHash} from 'node:crypto';import {mkdtempSync,rmSync} from 'node:fs';
import {tmpdir} from 'node:os';import {join,dirname,resolve} from 'node:path';import {fileURLToPath} from 'node:url';
import {withRoleRuntime} from './fixtures/role_runtime_native_support.mjs';
import {hostControlDriver} from './fixtures/host_control_driver.mjs';
import {hostControlHttps} from './fixtures/host_control_https.mjs';
const root=resolve(dirname(fileURLToPath(import.meta.url)),'..');
const mode='native-participant-owner-setup-real-fence-test-session';
const json=async response=>{assert.equal(response.status,200);return response.json();};
function head(major,value){let n=BigInt(value);if(n<24n)return Buffer.from([(major<<5)|Number(n)]);for(const [bound,size,tag] of [[256n,1,24],[65536n,2,25],[4294967296n,4,26],[18446744073709551616n,8,27]])if(n<bound){const b=Buffer.alloc(1+size);b[0]=(major<<5)|tag;for(let i=size;i>0;i--){b[i]=Number(n&255n);n>>=8n;}return b;}assert.fail('fixture integer bound');}
function cbor(value){if(value===null)return Buffer.from([246]);if(typeof value==='boolean')return Buffer.from([value?245:244]);if(typeof value==='number'){assert.ok(Number.isSafeInteger(value)&&value>=0);return head(0,value);}if(typeof value==='string'){const b=Buffer.from(value);return Buffer.concat([head(3,b.length),b]);}if(Array.isArray(value))return Buffer.concat([head(4,value.length),...value.map(cbor)]);const pairs=Object.entries(value).map(([k,v])=>[cbor(k),cbor(v)]).sort((a,b)=>a[0].length-b[0].length||Buffer.compare(a[0],b[0]));return Buffer.concat([head(5,pairs.length),...pairs.flat()]);}
const digest=(domain,value)=>createHash('sha256').update(cbor([domain,value])).digest('hex');
function setupPlan(b){return {api_version:1,setup_id:'session-setup',provision_id:b.provision_id,participant_receipt_digest:b.participant_receipt_digest,world_identity:b.world_identity,owner_key_ref:b.owner_key.key_id,executor_key_ref:b.executor_key.key_id,bootstrap_config_version:b.bootstrap_config_version,goal_content:{goal_text:'Private isolated TestSession prerequisite',soft_constraints:['Use only approved task']},create_operation_id:'session-create',prepare_operation_id:'session-prepare',prepare_nonce:'a7'.repeat(32),publish_operation_id:'session-publish',delegation_operation_id:'session-delegate',bind_operation_id:'session-bind',executor_id:'session-executor',executor_public_key:b.executor_key.public_key,delegation_generation:'1',delegation_nonce:'1',resource_refs:[],deadline:String(Date.now()+120000),request_limit:'6',transport_limit:'584',write_limit:'4'};}
test('actual V1 Host CLI crosses trusted TLS and isolated H/M Control with cold lost response recovery',{timeout:600000,skip:process.env.OASIS7_HOST_CONTROL_INTEGRATION!=='true'},async()=>{
 const directory=mkdtempSync(join(tmpdir(),'oasis7-host-control-'));let driver=null;const tls=await hostControlHttps(directory);let descriptor=null;
 function configureOptions(options){for(const w of options.workers){if(!['owner','identity'].includes(w.name))continue;
  w.bindings={...w.bindings,CUSTODY_TEST_SESSION_ENABLED:'true',CUSTODY_CLIENT_ENROLLMENT_ENABLED:'true',CUSTODY_PUBLIC_ORIGIN:'https://localhost',CUSTODY_ENROLLMENT_REALM:'host-control-integration',CUSTODY_HOST_DEPLOYMENTS:JSON.stringify(descriptor?[{environment:descriptor.environment,host_instance_id:descriptor.host_instance_id,descriptor_digest:digest('oasis7.custody.host-deployment-descriptor.v1',descriptor),client_public_key:descriptor.client_public_key}]:[])};
  w.durableObjects={...w.durableObjects,CLIENT_ENROLLMENTS:{className:'ClientEnrollmentAuthority',useSQLite:true,...(w.name==='owner'?{scriptName:'identity'}:{})}};
  if(w.name==='identity'){w.serviceBindings={...w.serviceBindings,TEST_SESSION_PRIVATE_HISTORY:{name:'identity',entrypoint:'TestSessionPrivateHistory'},TEST_SESSION_NATIVE_HISTORY:{name:'identity',entrypoint:'NativeProvisionHistory'}};w.modules=[{type:'ESModule',path:resolve(root,'tests/fixtures/test_session_runtime.mjs')},...w.modules.filter(m=>!m.path.endsWith('/native_revocation_scheduler.mjs'))];}
  else w.serviceBindings={...w.serviceBindings,IDENTITY_TEST_SESSIONS:{name:'identity',entrypoint:'IdentityTestSessions'},TEST_SESSION_PRIVATE_HISTORY:{name:'identity',entrypoint:'TestSessionPrivateHistory'}};
 }}
 try{driver=hostControlDriver({directory,fixtureSource:resolve(root,'tests/fixtures'),port:tls.port});
 await withRoleRuntime(mode,async runtime=>{tls.bind(runtime);
  assert.equal((await runtime.drive('host-control-participant')).stage,'participant_bound');
  const setup=setupPlan(await runtime.setupBasis());await runtime.approveSetup(setup);assert.equal((await json(await runtime.continueSetup(setup.setup_id))).stage,'prepared');
  const invite=await json(await runtime.managementFetch('/test-sessions/invite',{api_version:1,request_id:'host-control-invite',transport_profile:'native-broker-custody-v1',tool_profile:'custody-native-v1'}));
  const target={custody_account_id:runtime.account,provision_id:runtime.provisionId,invite_id:invite.invite_id,invite_digest:digest('oasis7.custody.pairing-invite.v1',invite)};
  const initialized=await driver.rpc('initialize',{request_id:'host-control-create',identity:{api_version:1,enabled:true,service_origin:'https://localhost',skill_manifest_path:'/replaced-by-supervisor',descriptor:{api_version:1,environment:'test',transport_profile:'native-broker-custody-v1',tool_profile:'custody-native-v1',host_instance_id:'actual-linux-control-host',client_public_key:'',build_digest:'',skill_manifest_digest:'',tool_manifest_digest:'',https_origin:'https://host.example',restricted_runtime_identity:'deterministic-control-component-not-gameplay',isolation_mechanism:'separate_os_principal',isolation_acceptance_refs:['component-only-not-CT008']},target,isolation:{host_uid:21001,model_uid:21002,native:{kind:'remote',https_origin:'https://localhost'}}}});
  descriptor=initialized.descriptor;assert.ok(descriptor);await runtime.restart(mode);
  const enrolled=await driver.rpc('enroll',{});const cleanup=Date.now()+90000;
  const {basis,basis_digest}=await json(await runtime.managementFetch('/test-sessions/basis',{api_version:1,request_id:'host-control-plan',enrollment_id:enrolled.enrollment_id,cleanup_until:String(cleanup)}));
  const plan={api_version:1,request_id:basis.request_id,enrollment_id:enrolled.enrollment_id,basis_digest,actions:[{tool:'move',position_cm:['11','22','33']}],allowed_resource_refs:basis.allowed_resource_refs,write_deadline:String(cleanup-30000),read_until:String(cleanup+30000),cleanup_until:String(cleanup),request_limit:'100',transport_limit:'400',business_limit:'0',read_budget:{requests:'100',transport:'300',business:'0'},cleanup_budget:{requests:'2',transport:'24',business:'0'},control_budget:{requests:'80',transport_per_hop:'240',identity_transport:'160',transport_hops:1}};
  const request=await json(await runtime.managementFetch('/test-sessions/prepare',{api_version:1,approval_request_id:'host-control-approval',plan}));const approval=await runtime.approveIntent(request);
  await json(await runtime.managementFetch('/test-sessions/approve',{api_version:1,input:{operation_id:basis.session_id,request,approval},plan}));
  assert.equal((await json(await runtime.managementFetch('/test-sessions/continue',{api_version:1,session_id:basis.session_id}))).stage,'active');
  const resource_id='host-control-resource';await driver.rpc('bind',{session_id:basis.session_id,resource_id,pairing:basis.client_pairing,plan});
  await driver.rpc('claim',{api_version:1,enrollment_id:enrolled.enrollment_id,test_session_id:basis.session_id,request_id:'host-control-claim',purposes:['execute','read'],replace:false,expected_generation:null});
  const os=await driver.rpc('serve',{});assert.equal(os.ready,true);assert.equal(os.private_denied_m,true);assert.equal(os.private_denied_n_witness,true);
  const control=(method,id)=>driver.rpc('control',{api_version:1,resource_id,request_id:id,method});
  assert.equal((await control('initialize','host-control-init')).status,'accepted');
  tls.dropHeartbeat('host-control-heartbeat');const unknown=await control('heartbeat','host-control-heartbeat');assert.ok(['unknown','unavailable'].includes(unknown.status));
  await driver.rpc('restart',{});assert.equal((await driver.rpc('serve',{})).ready,true);
  assert.ok(['accepted','historical'].includes((await control('heartbeat','host-control-heartbeat')).status));
  for(const [method,id]of [['tools_list','host-control-tools'],['request_finish','host-control-finish'],['request_abort','host-control-abort']])assert.ok(['accepted','historical'].includes((await control(method,id)).status));
  const heartbeats=tls.facts().filter(v=>v.path.endsWith('/heartbeat'));assert.equal(heartbeats.length,2);assert.ok(heartbeats.every(v=>v.status===200));assert.ok(heartbeats[0].body.equals(heartbeats[1].body),'same retained original body after actual dropped response and Host restart');
  console.log(JSON.stringify({host_control_evidence:{actual_https:true,actual_cli:true,distinct_h_m_uid:true,cold_unknown_original_restored:true,world_fixture:true,world_closed:false,real_agent_gameplay:false}}));
 },{configureOptions});
 }finally{if(driver)await driver.close();await tls.close();rmSync(directory,{recursive:true,force:true});}
});
