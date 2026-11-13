import assert from 'node:assert/strict';
import {test} from 'node:test';
import {readFileSync,mkdtempSync,rmSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {resolve,dirname} from 'node:path';
import {fileURLToPath} from 'node:url';
import {Miniflare,convertV4MiniflareOptions} from 'miniflare';
const root=resolve(dirname(fileURLToPath(import.meta.url)),'..');
const token='71'.repeat(32), other='72'.repeat(32), canary='PRIVATE_ACCOUNT_CREDENTIAL_MUST_NOT_BE_RETURNED';
const now=()=>Math.floor(Date.now()/1000);
function policy(){return {version:1,generation:1,service_id:'standalone-test',origin:'https://worker.example.test',expires_at_unix:now()+1800,max_calls:40,
 credentials:[{caller_id:'caller',token_secret:'ACCOUNT_SERVICE_TOKEN',expires_at_unix:now()+1700,revoked:false},{caller_id:'other',token_secret:'ACCOUNT_SERVICE_OTHER',expires_at_unix:now()+1700,revoked:false}],
 accounts:{A:{alias:'Alice',account_id:'id-A',provider:'generic',environment:'test',credential_secret:'ACCOUNT_SERVICE_A',credential_version:1,disabled:false},B:{alias:'Bob',account_id:'id-B',provider:'generic',environment:'test',credential_secret:'ACCOUNT_SERVICE_B',credential_version:1,disabled:false}},
 grants:[{grant_ref:'grant-A',caller_id:'caller',account_ref:'A',authorization_version:1,expires_at_unix:now()+1600,enabled:true},{grant_ref:'grant-B',caller_id:'caller',account_ref:'B',authorization_version:1,expires_at_unix:now()+1600,enabled:true},{grant_ref:'other-B',caller_id:'other',account_ref:'B',authorization_version:1,expires_at_unix:now()+1600,enabled:true}]};}
test('standalone account service has no game bindings or outbound network and preserves durable authority',{timeout:120000},async()=>{
 const persist=mkdtempSync(resolve(tmpdir(),'oasis7-account-service-'));let outbound=0;let p=policy();
 const make=()=>new Miniflare({...convertV4MiniflareOptions({name:'account-service',modulesRoot:root,compatibilityDate:'2026-10-25',cf:false,
  modules:[{type:'ESModule',path:resolve(root,'build/entry.mjs')},{type:'ESModule',path:resolve(root,'build/index.js')},{type:'CompiledWasm',path:resolve(root,'build/index_bg.wasm')}],
  durableObjects:{DEV_TEST_ACCOUNTS:{className:'DevTestAccounts',useSQLite:true}},
  bindings:{ACCOUNT_SERVICE_POLICY:JSON.stringify(p),ACCOUNT_SERVICE_TOKEN:token,ACCOUNT_SERVICE_OTHER:other,ACCOUNT_SERVICE_A:canary,ACCOUNT_SERVICE_B:canary,ACCOUNT_SERVICE_B_V2:canary+'V2'},
  outboundService:()=>{outbound++;throw Error('standalone service must not call a backend');}}),resourcePersistencePath:persist,unsafeDevRegistryPath:resolve(persist,'registry')});
 let mf=make();const replace=async()=>{await mf.dispose();mf=make();};
 const invoke=(method,params,auth=token)=>mf.dispatchFetch('https://worker.example.test/accounts/mcp',{method:'POST',headers:{'Content-Type':'application/json',Authorization:'Bearer '+auth},body:JSON.stringify({jsonrpc:'2.0',id:1,method,...(params?{params}:{})})});
 const call=async(args,auth=token)=>{const r=await invoke('tools/call',{name:'account_service_call',arguments:args},auth);assert.equal(r.status,200);const text=await r.text();assert.ok(!text.includes(canary));assert.ok(!text.includes('ACCOUNT_SERVICE_A'));return JSON.parse(text).result.structuredContent;};
 try{
  assert.equal((await invoke('ping')).status,200,'new independent ingress must exist');
  assert.equal((await invoke('ping',undefined,'bad')).status,401);
  const tools=await (await invoke('tools/list')).json();assert.equal(tools.result.tools[0].name,'account_service_call');assert.deepEqual(tools.result.tools[0].inputSchema.properties.method.enum,['accounts','open','describe','close']);
  const invalid=await (await invoke('tools/list',{invalid:true})).json();assert.ok(invalid.error);assert.equal(invalid.result,undefined);
  const directory=await call({method:'accounts'});assert.equal(directory.status,'ok');assert.equal(directory.value.result.accounts.length,2);assert.equal(directory.value.usage.used,1);
  const a=await call({method:'open',grant_ref:'grant-A',expires_at_unix:now()+900});const b=await call({method:'open',grant_ref:'grant-B',expires_at_unix:now()+900});const ar=a.value.result.session_ref,br=b.value.result.session_ref;
  for(const [ref,account] of [[ar,'A'],[br,'B'],[ar,'A']]){const v=await call({method:'describe',session_ref:ref});assert.equal(v.value.result.account_ref,account);}
  assert.equal((await call({method:'describe',session_ref:ar},other)).code,'session_unavailable');
  assert.equal((await call({method:'open',grant_ref:'grant-A',expires_at_unix:now()+100},other)).code,'grant_unavailable');
  await replace();const resumed=await call({method:'describe',session_ref:br});assert.equal(resumed.value.result.account_ref,'B');assert.equal(resumed.value.usage.used,7);
  p.generation++;p.accounts.B.alias='Renamed';p.accounts.C={...p.accounts.A,alias:'Third',account_id:'id-C'};p.grants.push({...p.grants[0],grant_ref:'grant-C',account_ref:'C'});await replace();
  const updated=await call({method:'describe',session_ref:br});assert.equal(updated.value.result.alias,'Renamed');assert.equal(updated.value.usage.used,8);
  assert.equal((await call({method:'accounts'})).value.result.accounts.length,3);
  p.generation++;p.accounts.B.credential_secret='ACCOUNT_SERVICE_B_V2';p.accounts.B.credential_version=2;for(const g of p.grants)if(g.account_ref==='B')g.authorization_version++;await replace();
  assert.equal((await call({method:'describe',session_ref:br})).code,'session_scope_denied');
  const b2=await call({method:'open',grant_ref:'grant-B',expires_at_unix:now()+800});const b2r=b2.value.result.session_ref;assert.equal((await call({method:'describe',session_ref:b2r})).value.result.credential_version,2);
  p.generation++;p.accounts.A.disabled=true;p.grants[0].authorization_version++;await replace();assert.equal((await call({method:'open',grant_ref:'grant-A',expires_at_unix:now()+100})).code,'grant_unavailable');
  p.generation++;p.credentials[1].revoked=true;p.grants.find(g=>g.grant_ref==='other-B').authorization_version++;await replace();assert.equal((await invoke('ping',undefined,other)).status,401);
  const before=await call({method:'describe',session_ref:b2r});p.generation++;p.max_calls=before.value.usage.used+1;await replace();
  const last=await call({method:'describe',session_ref:b2r});assert.equal(last.value.usage.used,p.max_calls);assert.equal((await call({method:'accounts'})).code,'call_limit_exhausted');
  await replace();assert.equal((await call({method:'accounts'})).code,'call_limit_exhausted');assert.equal((await call({method:'close',session_ref:b2r})).value.result.closed,true);assert.equal((await call({method:'describe',session_ref:b2r})).code,'session_unavailable');
  p.generation--;await replace();assert.equal((await invoke('ping')).status,503,'policy rollback cannot reload old rights or budget');
  p.generation++;const previous=structuredClone(p);p.generation++;
  for(const credential of p.credentials)credential.revoked=true;
  for(const grant of p.grants)grant.authorization_version++;
  await replace();assert.equal((await invoke('ping')).status,401);
  p=previous;await replace();assert.equal((await invoke('ping')).status,503,'revoking all callers must persist the trusted generation before authentication');
  assert.equal(outbound,0);
 }finally{await mf.dispose();rmSync(persist,{recursive:true,force:true});}
});
