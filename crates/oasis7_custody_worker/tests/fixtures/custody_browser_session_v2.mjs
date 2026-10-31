// Local component fixture: real browser UV and Worker authority; no OS/liveplay claim.
import assert from 'node:assert/strict';
import {execFileSync} from 'node:child_process';
import {createHash,generateKeyPairSync,sign,randomBytes} from 'node:crypto';
const client=generateKeyPairSync('ed25519');
const publicKey=client.publicKey.export({format:'der',type:'spki'}).subarray(-32).toString('hex');
// Same deterministic public-metadata CBOR as existing enrollment tests. No Native/Owner key.
function head(m,v){let n=BigInt(v);if(n<24n)return Buffer.from([(m<<5)|Number(n)]);for(const [b,s,t] of [[256n,1,24],[65536n,2,25],[4294967296n,4,26],[18446744073709551616n,8,27]])if(n<b){const a=Buffer.alloc(1+s);a[0]=(m<<5)|t;for(let i=s;i>0;i--){a[i]=Number(n&255n);n>>=8n;}return a;}throw Error('public metadata integer bound');}
function cbor(v){if(v===null)return Buffer.from([246]);if(typeof v==='boolean')return Buffer.from([v?245:244]);if(typeof v==='number'){assert.ok(Number.isSafeInteger(v)&&v>=0);return head(0,v);}if(typeof v==='string'){const b=Buffer.from(v);return Buffer.concat([head(3,b.length),b]);}if(Array.isArray(v))return Buffer.concat([head(4,v.length),...v.map(cbor)]);const a=Object.entries(v).map(([k,x])=>[cbor(k),cbor(x)]).sort((a,b)=>a[0].length-b[0].length||Buffer.compare(a[0],b[0]));return Buffer.concat([head(5,a.length),...a.flat()]);}
const digest=(d,v)=>createHash('sha256').update(cbor([d,v])).digest('hex');
const descriptor={api_version:1,environment:'test',transport_profile:'native-broker-custody-v1',tool_profile:'custody-native-v1',host_instance_id:'local-browser-component',client_public_key:publicKey,build_digest:'b1'.repeat(32),skill_manifest_digest:'b2'.repeat(32),tool_manifest_digest:'b3'.repeat(32),https_origin:'https://host.example',restricted_runtime_identity:'fixture-not-isolation-qualified',isolation_mechanism:'separate_os_principal',isolation_acceptance_refs:['component-not-CT008']};
const descriptorDigest=digest('oasis7.custody.host-deployment-descriptor.v1',descriptor);
export function sessionV2BrowserOptions(options,root,origin){
  for(const w of options.workers){if(!['identity','owner'].includes(w.name))continue;
    w.bindings={...w.bindings,CUSTODY_OWNER_SETUP_V2_ENABLED:'true',CUSTODY_TEST_SESSION_ENABLED:'true',CUSTODY_TEST_SESSION_V2_ENABLED:'true',CUSTODY_CLIENT_ENROLLMENT_ENABLED:'true',CUSTODY_PUBLIC_ORIGIN:origin,CUSTODY_ENROLLMENT_REALM:'browser-v2-component',CUSTODY_HOST_DEPLOYMENTS:JSON.stringify([{environment:'test',host_instance_id:descriptor.host_instance_id,descriptor_digest:descriptorDigest,client_public_key:publicKey}])};
    w.durableObjects={...w.durableObjects,CLIENT_ENROLLMENTS:{className:'ClientEnrollmentAuthority',useSQLite:true,...(w.name==='owner'?{scriptName:'identity'}:{})}};
    if(w.name==='identity')w.serviceBindings={...w.serviceBindings,TEST_SESSION_PRIVATE_HISTORY:{name:'identity',entrypoint:'TestSessionPrivateHistory'},TEST_SESSION_NATIVE_HISTORY:{name:'identity',entrypoint:'NativeProvisionHistory'}};
    else w.serviceBindings={...w.serviceBindings,IDENTITY_TEST_SESSIONS:{name:'identity',entrypoint:'IdentityTestSessions'},TEST_SESSION_PRIVATE_HISTORY:{name:'identity',entrypoint:'TestSessionPrivateHistory'}};
  }
  return options;
}
function proofHeaders(origin,view,body){
  const created=Math.floor(Date.now()/1000),sha=v=>createHash('sha256').update(v).digest('base64');
  const names=['@method','@target-uri','content-type','content-digest','oasis7-token-digest','oasis7-enrollment','oasis7-request-id'];
  const params=`(${names.map(x=>JSON.stringify(x)).join(' ')});created=${created};expires=${created+29};nonce="${view.server_nonce}";alg="ed25519";keyid="${view.key_id}";tag="oasis7-custody-enrollment-proof-v1"`;
  const headers={'content-type':'application/json','content-digest':`sha-256=:${sha(body)}:`,'oasis7-token-digest':`:${sha('')}:`,'oasis7-enrollment':view.enrollment_id,'oasis7-request-id':'browser-v2-proof'};
  const values={'@method':'POST','@target-uri':`${origin}/v1/client-enrollments/${view.enrollment_id}/prove`,...headers};
  const base=names.map(n=>`"${n}": ${values[n]}`).concat(`"@signature-params": ${params}`).join('\n');
  return {...headers,'signature-input':`oasis7=${params}`,signature:`oasis7=:${sign(null,Buffer.from(base),client.privateKey).toString('base64')}:`};
}
function sqlFacts(persist,account,id){
  let raw;
  try { raw=execFileSync('rtk',['proxy','python3','-c',`
import json,sqlite3,sys
from pathlib import Path
found=[]
for path in Path(sys.argv[1]).rglob('*.sqlite'):
 try:
  db=sqlite3.connect(path.as_uri()+'?mode=ro',uri=True)
  try: row=db.execute("SELECT payload FROM custody_records WHERE id=?",('custody-provision-approval-v1',)).fetchone()
  finally: db.close()
  if not row: continue
  s=json.loads(row[0])['state']
  if s['draft']['entry']['custody_account_id']!=sys.argv[2]: continue
  v=s.get('test_sessions_v2',{}).get(sys.argv[3])
  if v is None: continue
  found.append({'consumed':v.get('consumed_at') is not None,'activated':v.get('approved_at') is not None,'roots':sorted(v.get('roots',{}).keys()),'claims':len(v.get('credential_claims',{})),'old_v1_sessions':len(s.get('test_sessions',{})),'execution_book_absent':v.get('execution_book') is None})
 except (sqlite3.Error,KeyError,TypeError,ValueError): pass
if len(found)!=1: raise SystemExit(2)
print(json.dumps(found[0]))
`,persist,account,id],{encoding:'utf8',maxBuffer:4096,stdio:['ignore','pipe','ignore']}); }
  catch {throw Error('safe V2 SQL fact unavailable');}
  return JSON.parse(raw);
}
export async function acceptSessionV2({page,identity,origin,sourceHeaders,parent,world,setStage,persist,onActivated,nativeCounts}){
  const prefix=`browser-v2-${randomBytes(8).toString('hex')}`;
  const base=`/v1/account-provisions/${encodeURIComponent(parent.entry.provision_id)}`;
  const post=async(path,body,headers=sourceHeaders)=>{
    const r=await identity.fetch(origin+path,{method:'POST',headers,body:typeof body==='string'?body:JSON.stringify(body)});
    assert.equal(r.status,200,'fixed V2 phase response must succeed');return r.json();
  };
  const parentView={custody_account_id:parent.entry.custody_account_id,provision_id:parent.entry.provision_id,environment:'test',world_id:world.world_id};
  setStage('v2-owner-metadata');const b=await post(base+'/owner-setups/basis',{api_version:1});
  const selection={kind:'move',position_cm:['11','22','33']};
  const plan={api_version:2,model_budget:null,setup_id:prefix,provision_id:b.provision_id,participant_receipt_digest:b.participant_receipt_digest,world_identity:b.world_identity,owner_key_ref:b.owner_key.key_id,executor_key_ref:b.executor_key.key_id,bootstrap_config_version:b.bootstrap_config_version,goal_content:{goal_text:'Public local fixture: exact Move business0',soft_constraints:[]},create_operation_id:prefix+'-create',prepare_operation_id:prefix+'-prepare',prepare_nonce:randomBytes(32).toString('hex'),publish_operation_id:prefix+'-publish',delegation_operation_id:prefix+'-delegation',bind_operation_id:prefix+'-bind',executor_id:prefix+'-executor',executor_public_key:b.executor_key.public_key,delegation_generation:'1',delegation_nonce:'1',selection,deadline:String(Date.now()+120000),request_limit:'6',transport_limit:'584',write_limit:'4'};
  setStage('v2-owner-mounted-prepare');await page.evaluate(({parent,request})=>window.surface.prepareCustodyV2(parent,request),{parent:parentView,request:{api_version:2,approval_request_id:prefix+'-owner-approval',plan}});
  await page.getByText('Public local fixture: exact Move business0',{exact:true}).waitFor();
  setStage('v2-owner-actual-uv');await page.getByRole('button',{name:'确认摘要并使用通行密钥批准',exact:true}).click();
  await page.getByText('已保留批准。当前阶段：v2_approval_completed。本页未继续执行。',{exact:true}).waitFor();
  setStage('v2-owner-four-writes');const prepared=await post(base+'/owner-setups/continue',{api_version:2,custody_account_id:parentView.custody_account_id,provision_id:parentView.provision_id,setup_id:prefix});
  assert.equal(prepared.stage,'prepared','actual four-write OwnerV2 must reach Prepared');assert.deepEqual(prepared.pending_operations,[]);assert.deepEqual(prepared.blocked_operations,[]);
  // Fresh page clears only the known-completed browser original; same Native UV session.
  await page.reload();await page.getByRole('heading',{name:'账号与安全',exact:true}).waitFor();
  setStage('v2-enrollment');const invite=await post(base+'/test-sessions/invite',{api_version:1,request_id:prefix+'-invite',transport_profile:'native-broker-custody-v1',tool_profile:'custody-native-v1'});
  const target={custody_account_id:parentView.custody_account_id,provision_id:parentView.provision_id,invite_id:invite.invite_id,invite_digest:digest('oasis7.custody.pairing-invite.v1',invite)};
  const view=await post('/v1/client-enrollments',{api_version:1,request_id:prefix+'-enroll',target,descriptor},{'content-type':'application/json'});assert.equal(view.descriptor_digest,descriptorDigest);
  const body=JSON.stringify({api_version:1,enrollment_id:view.enrollment_id,request_id:'browser-v2-proof',target,client_public_key:publicKey,server_nonce:view.server_nonce,challenge_expires_at:view.challenge_expires_at,descriptor_digest:view.descriptor_digest});
  assert.equal((await post(`/v1/client-enrollments/${view.enrollment_id}/prove`,body,proofHeaders(origin,view,body))).status,'proved');
  setStage('v2-session-metadata');const cleanup=Date.now()+90000;
  const basis=await post(base+'/test-sessions/basis',{api_version:2,request_id:prefix+'-session',enrollment_id:view.enrollment_id,setup_id:prefix,cleanup_until:String(cleanup)});
  const sessionPlan={api_version:2,model_budget:null,termination:basis.termination,request_id:basis.request_id,enrollment_id:view.enrollment_id,basis_digest:basis.basis_digest,owner_setup_plan_digest:basis.owner_setup_plan_digest,prepared_resource_basis_digest:null,selection,actions:[{tool:'move',position_cm:['11','22','33']}],allowed_resource_refs:[],write_deadline:String(cleanup-30000),read_until:String(cleanup+30000),cleanup_until:String(cleanup),request_limit:'100',transport_limit:'400',business_limit:'0',read_budget:{requests:'100',transport:'300',business:'0'},cleanup_budget:{requests:'2',transport:'24',business:'0'},control_budget:{requests:'80',transport_per_hop:'240',identity_transport:'160',transport_hops:1}};
  setStage('v2-session-mounted-prepare');await page.evaluate(({parent,request})=>window.surface.prepareCustodySessionV2(parent,request),{parent:parentView,request:{api_version:2,approval_request_id:prefix+'-session-approval',plan:sessionPlan,scope:basis.scope}});
  setStage('v2-session-full-confirmation');await page.getByRole('button',{name:'确认摘要并使用通行密钥批准',exact:true}).click();
  await page.getByRole('heading',{name:'确认完整 TestSession V2 范围与永久关闭',exact:true}).waitFor();
  await page.getByText('无模型推理权',{exact:true}).waitFor();
  await page.getByRole('button',{name:'已核对预算，继续通行密钥',exact:true}).click();
  await page.getByText('已保留批准。当前阶段：v2_approval_completed。本页未继续执行。',{exact:true}).waitFor();
  const consumed=sqlFacts(persist,parentView.custody_account_id,basis.scope.session_id);
  assert.equal(consumed.consumed,true);assert.equal(consumed.activated,false);assert.deepEqual(consumed.roots,[]);assert.equal(consumed.old_v1_sessions,0);
  setStage('v2-session-source-preflight');const active=await post(base+'/test-sessions/continue',{api_version:2,session_id:basis.scope.session_id});
  assert.equal(active.api_version,2);assert.match(active.activated_at,/^[1-9][0-9]*$/);assert.equal(active.preflight_pending,false);assert.equal(active.closing,false);
  const saved=sqlFacts(persist,parentView.custody_account_id,basis.scope.session_id);assert.equal(saved.activated,true);assert.deepEqual(saved.roots,['execution','reconcile_read','termination_plan']);assert.equal(saved.claims,0);assert.equal(saved.old_v1_sessions,0);assert.equal(saved.execution_book_absent,true);
  let claimFacts={};
  if(onActivated) claimFacts=await onActivated({
    cycle:credentialCycle({identity,origin,scope:basis.scope,enrollment:view,plan:sessionPlan,prefix,nativeCounts}),
    counters:()=>credentialCounters(persist,parentView.custody_account_id,basis.scope.session_id),setStage,
  });
  return {consume_zero_roots:true,final_sql_three_roots:true,owner_v2_actual_prepared:true,session_v2_actual_activation:true,actual_two_separate_uv:true,model_budget:'none-no-inference',world:'local-fixture-not-liveplay',os_isolation:'not_tested',cleanup:'not_tested',...claimFacts};
}

// Closed fixture signer: origin, account/provision/session/enrollment and key are
// all lexical from the genuine activation above. No raw signing/key API exists.
function credentialCycle({identity,origin,scope,enrollment,plan,prefix,nativeCounts}){
  const targetPrefix=`${origin}/v1/account-provisions/${encodeURIComponent(scope.client_pairing.target.provision_id)}/accounts/${encodeURIComponent(scope.client_pairing.target.custody_account_id)}/client/`;
  const total=()=>Object.values(nativeCounts).reduce((n,v)=>n+v,0);
  const signed=async(kind,body,nonce=null,nonceUntil=null)=>{
    assert.equal(body.api_version,1);assert.ok(['challenge','claim'].includes(kind));
    if(kind==='challenge') {assert.equal(body.session_id,scope.session_id);assert.equal(body.kind.kind,'credential_claim');}
    else {assert.equal(body.test_session_id,scope.session_id);assert.equal(body.enrollment_id,enrollment.enrollment_id);}
    const raw=JSON.stringify(body),target=targetPrefix+(kind==='challenge'?'proof-challenges':`client-enrollments/${encodeURIComponent(enrollment.enrollment_id)}/claim`);
    const deadline=kind==='challenge'&&body.purpose==='reconcile_read'||kind==='claim'&&body.purposes.every(p=>p==='read')?plan.read_until:plan.write_deadline;
    const until=nonceUntil===null?BigInt(deadline):BigInt(nonceUntil)<BigInt(deadline)?BigInt(nonceUntil):BigInt(deadline);
    const created=Math.floor(Date.now()/1000),expires=Math.min(created+29,Number(until/1000n));
    assert.ok(expires*1000>Date.now(),'original signed root/nonce must remain live');
    const sha=v=>createHash('sha256').update(v).digest('base64');
    const fields=['@method','@target-uri','content-type','content-digest','oasis7-token-digest','oasis7-test-session','oasis7-request-id'];
    const tag=kind==='challenge'?'oasis7-custody-challenge-v1':'oasis7-custody-enroll-v1';
    const params=`(${fields.map(n=>JSON.stringify(n)).join(' ')});created=${created};expires=${expires}${nonce===null?'':`;nonce="${nonce}"`};alg="ed25519";keyid="${enrollment.key_id}";tag="${tag}"`;
    const headers={'content-type':'application/json','content-digest':`sha-256=:${sha(raw)}:`,'oasis7-token-digest':`:${sha('')}:`,'oasis7-test-session':scope.session_id,'oasis7-request-id':body.request_id,Host:new URL(origin).host};
    const values={'@method':'POST','@target-uri':target,...headers};
    const message=fields.map(n=>`"${n}": ${values[n]}`).concat(`"@signature-params": ${params}`).join('\n');
    headers['signature-input']=`oasis7=${params}`;headers.signature=`oasis7=:${sign(null,Buffer.from(message),client.privateKey).toString('base64')}:`;
    return identity.fetch(target,{method:'POST',headers,body:raw});
  };
  const authorized=async(kind,body,nonce=null,until=null)=>{
    const before={...nativeCounts};const count=total();
    const response=await signed(kind,body,nonce,until);
    assert.equal(response.status,200,'actual V2 public credential HTTP must succeed');
    assert.equal((nativeCounts['/api/public/hosted-account/v2/custody/approvals/verify']??0)-(before['/api/public/hosted-account/v2/custody/approvals/verify']??0),1);
    assert.equal((nativeCounts['/api/public/hosted-account/v2/custody/managed-bindings/session-lifetime']??0)-(before['/api/public/hosted-account/v2/custody/managed-bindings/session-lifetime']??0),1);
    assert.equal(total()-count,2,'each live public V2 credential HTTP must perform only its actual Inspect+LF');
    return response;
  };
  return async({label,purposes,replace,expectedGeneration,discardReply})=>{
    assert.ok(/^[a-z-]{1,32}$/.test(label));assert.ok(purposes.length>0&&purposes.every(p=>['execute','read'].includes(p)));
    const request={api_version:1,enrollment_id:enrollment.enrollment_id,test_session_id:scope.session_id,request_id:`${prefix}-claim-${label}`,purposes,replace,expected_generation:expectedGeneration};
    const challengeRequest={api_version:1,session_id:scope.session_id,request_id:`${prefix}-challenge-${label}`,purpose:purposes[0]==='read'?'reconcile_read':'execution',kind:{kind:'credential_claim'},logical_request_id:request.request_id,operation_request_digest:digest('oasis7.custody.test-session-claim.v1',request)};
    const challenge=await(await authorized('challenge',challengeRequest)).json();
    assert.equal(typeof challenge.nonce,'string');assert.equal(challenge.nonce.length,43);
    const response=await authorized('claim',request,challenge.nonce,challenge.expires_at);
    const replay=async()=>{const before=total();const r=await signed('claim',request,challenge.nonce,challenge.expires_at);assert.equal(r.status,200);assert.equal(total(),before,'paid history must perform no native HTTP');return r.json();};
    if(discardReply){await response.body?.cancel();return {discarded_committed_reply:true,replay};}
    return {reply:await response.json(),replay};
  };
}
function credentialCounters(persist,account,id){
  let raw;
  try {raw=execFileSync('rtk',['proxy','python3','-c',`
import json,sqlite3,sys
from pathlib import Path
def remaining(b):return int(b['limit'])-int(b['spent'])-sum(int(v) for v in b['reservations'].values())
found=[]
for p in Path(sys.argv[1]).rglob('*.sqlite'):
 try:
  db=sqlite3.connect(p.as_uri()+'?mode=ro',uri=True)
  try: row=db.execute("SELECT payload FROM custody_records WHERE id='custody-provision-approval-v1'").fetchone()
  finally: db.close()
  if not row:continue
  s=json.loads(row[0])['state']
  if s['draft']['entry']['custody_account_id']!=sys.argv[2]:continue
  v=s.get('test_sessions_v2',{}).get(sys.argv[3])
  if v is None:continue
  roots={}
  for purpose,wrapped in v['roots'].items():
   r=wrapped['root'];roots[purpose]={'transport':remaining(r['budgets']['transport']),'business':remaining(r['budgets']['business']),'history':len(r['credential_history']),'generation':None if r['credential'] is None else r['credential']['generation']}
  found.append({'host':remaining(v['control']['transport']),'native':remaining(v['identity_transport']),'requests':remaining(v['control']['requests']),'sources':len(v['sources']),'claims':len(v['credential_claims']),'purposes':roots,'execution_book_absent':v['execution_book'] is None,'old_v1_sessions':len(s.get('test_sessions',{}))})
 except (sqlite3.Error,KeyError,TypeError,ValueError):pass
if len(found)!=1:raise SystemExit(2)
print(json.dumps(found[0]))
`,persist,account,id],{encoding:'utf8',maxBuffer:4096,stdio:['ignore','pipe','ignore']});}
  catch {throw Error('safe V2 credential SQL counters unavailable');}
  return JSON.parse(raw);
}
