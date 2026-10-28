import assert from 'node:assert/strict';
import test from 'node:test';
import {mkdtempSync,rmSync} from 'node:fs';
import {execFileSync} from 'node:child_process';
import {tmpdir} from 'node:os';
import {dirname,resolve} from 'node:path';
import {fileURLToPath} from 'node:url';
import {Miniflare,convertV4MiniflareOptions} from 'miniflare';
const root=resolve(dirname(fileURLToPath(import.meta.url)),'..');
const origin='https://custody.example';
function counters(persist){return JSON.parse(execFileSync('python3',['-c',`
import sqlite3,json,sys
from pathlib import Path
out={'realm':None,'providers':[]}
for path in Path(sys.argv[1]).rglob('*.sqlite'):
 try:
  db=sqlite3.connect(path.as_uri()+'?mode=ro',uri=True)
  for row in db.execute("SELECT payload FROM custody_records WHERE id='custody-enrollment-abuse-v1'"):
   item=json.loads(row[0]);safe={key:item[key] for key in ['window','creates','proves']}
   if item['scope']=='realm':out['realm']=safe
   elif item['scope']=='provider':out['providers'].append(safe)
 except sqlite3.OperationalError:pass
 finally:
  if 'db' in locals():db.close()
print(json.dumps(out,separators=(',',':')))
`,persist],{encoding:'utf8',stdio:['ignore','pipe','pipe']}));}
test('overlimit provider cannot consume the shared realm enrollment budget',{timeout:60000},async()=>{
  const persist=mkdtempSync(resolve(tmpdir(),'oasis7-client-abuse-real-'));
  const mf=new Miniflare({...convertV4MiniflareOptions({name:'abuse-test',modulesRoot:root,modules:[
    {type:'ESModule',path:resolve(root,'tests/fixtures/client_abuse_runtime.mjs')},
    {type:'ESModule',path:resolve(root,'artifacts/identity/entry.mjs')},
    {type:'ESModule',path:resolve(root,'artifacts/identity/index.js')},
    {type:'CompiledWasm',path:resolve(root,'artifacts/identity/index_bg.wasm')}],
    compatibilityDate:'2026-10-25',cf:false,
    durableObjects:{CLIENT_ENROLLMENTS:{className:'ClientEnrollmentAuthority',useSQLite:true}},
    bindings:{CUSTODY_CLIENT_ENROLLMENT_ENABLED:'true',CUSTODY_PUBLIC_ORIGIN:origin,CUSTODY_ENROLLMENT_REALM:'abuse-fixture-enrollment',CUSTODY_HOST_DEPLOYMENTS:'[]'}}),
    resourcePersistencePath:persist,unsafeDevRegistryPath:resolve(persist,'registry')});
  const body=JSON.stringify({api_version:1,request_id:'same-enrollment-original',target:{custody_account_id:'synthetic-no-grant',provision_id:'provision-one',invite_id:'invite-one',invite_digest:'ab'.repeat(32)},descriptor:{api_version:1,environment:'test',transport_profile:'native-broker-custody-v1',tool_profile:'custody-native-v1',host_instance_id:'synthetic-host',client_public_key:'3b6a27bcceb6a42d62a3a8d02a6f0d73653215771de243a63ac048a18b59da29',build_digest:'bc'.repeat(32),skill_manifest_digest:'cd'.repeat(32),tool_manifest_digest:'de'.repeat(32),https_origin:'https://host.example',restricted_runtime_identity:'synthetic-only',isolation_mechanism:'separate_os_principal',isolation_acceptance_refs:['synthetic-not-CT008']}});
  const send=peer=>mf.dispatchFetch(`${origin}/fixture/provider-${peer}/create`,{method:'POST',headers:{'content-type':'application/json'},body});
  try{
    const untilNextWindow=60000-Date.now()%60000;
    if(untilNextWindow<5000)await new Promise(resolve=>setTimeout(resolve,untilNextWindow+25));
    for(let i=0;i<8;i++)assert.equal((await send('a')).status,200,'the provider may use its eight real create attempts');
    const before=counters(persist);assert.equal(before.realm.creates,8);assert.deepEqual(before.providers.map(v=>v.creates),[8]);
    assert.equal(Math.floor(Date.now()/60000),before.realm.window,'all attempts must belong to the same real metadata window');
    assert.ok((await send('a')).status>=400,'the ninth provider attempt must be rejected');
    const rejected=counters(persist);
    assert.equal(rejected.realm.window,before.realm.window);assert.equal(rejected.realm.creates,before.realm.creates,'a provider rejection must not consume the shared realm allowance');
    assert.deepEqual(rejected.providers,before.providers);
    assert.equal((await send('b')).status,200,'an independent provider retains its own allowance');
    const other=counters(persist);assert.equal(other.realm.creates,9);assert.deepEqual(other.providers.map(v=>v.creates).sort((a,b)=>a-b),[1,8]);
  }finally{await mf.dispose();rmSync(persist,{recursive:true,force:true});}
});
