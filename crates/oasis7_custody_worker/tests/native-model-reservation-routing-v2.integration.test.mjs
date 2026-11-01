import assert from 'node:assert/strict';
import test from 'node:test';
import {mkdtempSync,rmSync,readFileSync} from 'node:fs';
import {execFileSync} from 'node:child_process';
import {tmpdir} from 'node:os';
import {dirname,resolve} from 'node:path';
import {fileURLToPath} from 'node:url';
import {Miniflare,convertV4MiniflareOptions,Response as MiniflareResponse} from 'miniflare';
const root=resolve(dirname(fileURLToPath(import.meta.url)),'..');
function guards(persist){return JSON.parse(execFileSync('python3',['-c',`
import sqlite3,json,sys
from pathlib import Path
out={'provider':[],'realm':[]}
for path in Path(sys.argv[1]).rglob('*.sqlite'):
 try:
  db=sqlite3.connect(path.as_uri()+'?mode=ro',uri=True)
  for row in db.execute("SELECT payload FROM custody_records WHERE id='custody-enrollment-abuse-v1'"):
   item=json.loads(row[0])
   out[item['scope']].append(item['client_requests'])
 except sqlite3.OperationalError:pass
 finally:
  if 'db' in locals():db.close()
print(json.dumps(out,separators=(',',':')))
`,persist],{encoding:'utf8',stdio:['ignore','pipe','pipe']}));}
test('actual N refusal traverses both guards and one Owner service/Account Actor pair',{timeout:60000},async()=>{
 const path=process.env.OASIS7_N_MODEL_ENTRY_FIXTURE_PATH;
 assert.ok(path,'first run the genuine Rust fixture exporter; no invented request DTO');
 const fixture=JSON.parse(readFileSync(path,'utf8'));
 assert.equal(fixture.fixture,'genuine-core-unissued-credential-routing-refusal-v2');
 assert.equal(fixture.credential_qualification,'never_issued_refusal_only');
 assert.equal(fixture.component_time,'13000');
 assert.ok(Date.now()>30000,'this intentionally expired component is never a live billing positive');
 assert.ok(readFileSync(resolve(root,'artifacts/owner/entry.mjs'),'utf8').includes('nativeModelReservationV2'),'rebuild actual Owner with new closed named method');
 const persist=mkdtempSync(resolve(tmpdir(),'oasis7-n-model-routing-v2-'));
 const common={modulesRoot:root,compatibilityDate:'2026-10-25',cf:false};
 const artifacts=role=>[{type:'ESModule',path:resolve(root,`artifacts/${role}/entry.mjs`)},{type:'ESModule',path:resolve(root,`artifacts/${role}/index.js`)},{type:'CompiledWasm',path:resolve(root,`artifacts/${role}/index_bg.wasm`)}];
 let external=0;
 const blocked=()=>{external++;return new MiniflareResponse('no external source or provider in this fixture',{status:503});};
 const mf=new Miniflare({...convertV4MiniflareOptions({workers:[{
  name:'identity',...common,modules:[{type:'ESModule',path:resolve(root,'tests/fixtures/native_model_identity_counter_v2.mjs')},...artifacts('identity')],
  bindings:{CUSTODY_TEST_SESSION_ENABLED:'true',CUSTODY_TEST_SESSION_V2_ENABLED:'true',CUSTODY_CLIENT_ENROLLMENT_ENABLED:'true',CUSTODY_PUBLIC_ORIGIN:fixture.public_origin,CUSTODY_ENROLLMENT_REALM:'n-model-routing-refusal',CUSTODY_HOST_DEPLOYMENTS:'[]',FIXTURE_N_MODEL_TARGET:fixture.http_request.target_uri},
  durableObjects:{CLIENT_ENROLLMENTS:{className:'ClientEnrollmentAuthority',useSQLite:true}},
  serviceBindings:{OWNER_NATIVE_PERMITS:{name:'owner',entrypoint:'OwnerNativePermits'}},outboundService:blocked,
 },{
  name:'owner',...common,modules:[{type:'ESModule',path:resolve(root,'tests/fixtures/native_model_owner_counter_v2.mjs')},...artifacts('owner')],
  bindings:{CUSTODY_TEST_SESSION_ENABLED:'true',CUSTODY_TEST_SESSION_V2_ENABLED:'true',CUSTODY_PUBLIC_ORIGIN:fixture.public_origin},
  durableObjects:{ACCOUNTS:{className:'AccountAuthority',useSQLite:true}},outboundService:blocked,
 }]}),resourcePersistencePath:persist,unsafeDevRegistryPath:resolve(persist,'registry')});
 try{
  // Fixture/diagnostic reads below are outside the measured single operation.
  const response=await mf.dispatchFetch(`${fixture.public_origin}/fixture/n-model-refusal`,{method:'POST',headers:Object.fromEntries(fixture.http_request.headers.map(h=>[h.name,h.value])),body:fixture.http_request.body});
  assert.equal(response.status,409,'unknown Account must never be initialized or admitted');
  await response.text();
  const first=await (await mf.dispatchFetch(`${fixture.public_origin}/fixture/n-model-counts`)).json();
  const owner=await mf.getWorker('owner');
  const service=await (await owner.fetch('https://fixture.invalid/fixture/n-model-service-counts')).json();
  const {ACCOUNTS}=await mf.getBindings('owner');
  const account=await (await ACCOUNTS.get(ACCOUNTS.idFromName(fixture.account_id)).fetch('https://account.internal/fixture/n-model-account-counts')).json();
  const abuse=guards(persist);
  assert.equal(first.publicIngress,1);assert.deepEqual(abuse.provider,[1]);assert.deepEqual(abuse.realm,[1]);
  assert.equal(service.serviceCalls,1);assert.equal(account.accountCalls,1);assert.equal(account.authorityRows,0);
  assert.equal(first.publicIngress+abuse.provider[0]+abuse.realm[0]+service.serviceCalls+account.accountCalls,5);
  assert.equal(external,0,'no Native/Current/World/provider callback is allowed on this refusal');
 }finally{await mf.dispose();rmSync(persist,{recursive:true,force:true});}
});
