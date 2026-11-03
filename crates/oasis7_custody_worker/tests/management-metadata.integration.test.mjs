import assert from 'node:assert/strict';
import test from 'node:test';
import {createPrivateKey,createPublicKey} from 'node:crypto';
import {mkdtempSync,rmSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {dirname,resolve} from 'node:path';
import {fileURLToPath} from 'node:url';
import {Miniflare,convertV4MiniflareOptions} from 'miniflare';
import {setTimeout as delay} from 'node:timers/promises';
const root=resolve(dirname(fileURLToPath(import.meta.url)),'..');
const origin='https://custody.example';
const privateKey=createPrivateKey({key:Buffer.concat([Buffer.from('302e020100300506032b657004220420','hex'),Buffer.alloc(32,91)]),format:'der',type:'pkcs8'});
const publicKey=createPublicKey(privateKey).export({format:'der',type:'spki'}).subarray(-32).toString('hex');
const world={world_id:'world-one',genesis_digest:'genesis-one',branch_id:'branch-one',reorg_epoch:'0',service_public_key:publicKey};
const routes={environment:'test',identity_route:'native-one',world_route:'world-route-one',world_identity:world.world_id};
function session(){const now=Date.now();return {api_version:1,identity_namespace:'identity:test',identity_instance_epoch:'3',account_id:'manager-one',session_ref:'component-session',credential_ref:'component-passkey',credential_version:'1',security_version:'1',policy_version:'1',server_time:String(now),expires_at:String(now+600000),idle_expires_at:String(now+300000)};}
const metadata={CUSTODY_MANAGEMENT_METADATA_ENABLED:'true',CUSTODY_MANAGEMENT_METADATA_REALM:'metadata-test',CUSTODY_PUBLIC_ORIGIN:origin,CUSTODY_NATIVE_ORIGIN:origin};
const modules=(role,fixture)=>[{type:'ESModule',path:resolve(root,`tests/fixtures/${fixture}`)},
  {type:'ESModule',path:resolve(root,`artifacts/${role}/entry.mjs`)},{type:'ESModule',path:resolve(root,`artifacts/${role}/index.js`)},{type:'CompiledWasm',path:resolve(root,`artifacts/${role}/index_bg.wasm`)}];
function options(persist,bridge,enabled=true){return {...convertV4MiniflareOptions({workers:[
  {name:'metadata-identity',routes:[origin+'/*'],modules:modules('identity','management_metadata_runtime.mjs'),modulesRoot:root,compatibilityDate:'2026-10-25',cf:false,
    durableObjects:{CLIENT_ENROLLMENTS:{className:'ClientEnrollmentAuthority',useSQLite:true}},serviceBindings:{OWNER_MANAGEMENT:{name:'metadata-owner',entrypoint:'OwnerManagement'}},outboundService:bridge,
    bindings:{...metadata,...(!enabled?{CUSTODY_MANAGEMENT_METADATA_ENABLED:'false'}:{}),CUSTODY_NATIVE_RP_ID:'custody.example',CUSTODY_NATIVE_NAMESPACE:'identity:test',CUSTODY_NATIVE_INSTANCE_EPOCH:'3',CUSTODY_NATIVE_ENVIRONMENT:'test',CUSTODY_NATIVE_AUDIENCE:'owner-test',CUSTODY_NATIVE_APPROVAL_KEY_ID:'native-test',CUSTODY_NATIVE_APPROVAL_PUBLIC_KEY:publicKey,CUSTODY_NATIVE_ADAPTER_SECRET:'de'.repeat(32)}},
  {name:'metadata-owner',modules:modules('owner','management_metadata_owner.mjs'),modulesRoot:root,compatibilityDate:'2026-10-25',cf:false,
    durableObjects:{ACCOUNTS:{className:'AccountAuthority',useSQLite:true},MANAGER_ASSOCIATIONS:{className:'ManagerAssociations',useSQLite:true}},serviceBindings:{SELF_MANAGEMENT:{name:'metadata-owner',entrypoint:'OwnerManagement'}},
    bindings:{...metadata,CUSTODY_NATIVE_NAMESPACE:'identity:test',CUSTODY_NATIVE_INSTANCE_EPOCH:'3',CUSTODY_NATIVE_ENVIRONMENT:'test',CUSTODY_POLICY_VERSION:'1',CUSTODY_BOOTSTRAP_MANAGERS:JSON.stringify([{principal:{identity_namespace:'identity:test',native_account_id:'manager-one'},environments:['test']}]),CUSTODY_PROVISION_ROUTES:JSON.stringify(routes),CUSTODY_PROVISION_WORLD_IDENTITY:JSON.stringify(world)}}
]}),resourcePersistencePath:persist,unsafeDevRegistryPath:resolve(persist,'registry')};}
test('metadata actual service graph is five calls and provider exhaustion stops before Native and Owner',{timeout:90000},async()=>{
  const persist=mkdtempSync(resolve(tmpdir(),'oasis7-management-metadata-'));
  let nativeCalls=0;
  const bridge=async request=>{
    nativeCalls++;
    assert.equal(request.url,origin+'/api/public/hosted-account/v2/custody/management-session-status');
    assert.equal(request.method,'POST');assert.equal(await request.text(),'{}');
    assert.equal(request.headers.get('Cookie'),'__Host-oasis7_sid='+'a'.repeat(43));
    return Response.json(session()); // Synthetic adapter authority; not real-browser authentication evidence.
  };
  const mf=new Miniflare(options(persist,bridge));
  try{
    await mf.ready;
    const owner=await mf.getWorker('metadata-owner'),identity=await mf.getWorker('metadata-identity');
    const reserved=await owner.fetch(origin+'/fixture/reserve',{method:'POST',body:JSON.stringify({manager_session:session(),request:{api_version:1,client_request_id:'metadata-parent',plan:{...routes,purpose:'metadata component',expires_at:String(Date.now()+120000),transport_limit:'3'}}})});
    assert.equal(reserved.status,200,await reserved.clone().text());
    await owner.fetch(origin+'/fixture/reset');await identity.fetch(origin+'/fixture/reset');
    const send=()=>identity.fetch(origin+'/v1/management/parents/query',{method:'POST',headers:{'Content-Type':'application/json',Origin:origin,'Sec-Fetch-Site':'same-origin',Cookie:'__Host-oasis7_sid='+'a'.repeat(43),'X-Oasis7-CSRF':'bc'.repeat(32)},body:JSON.stringify({api_version:1,limit:16,after_custody_account_id:null,directory_revision:null})});
    if(Date.now()%60000>10000) await delay(60000-Date.now()%60000+20);
    const response=await send();assert.equal(response.status,200,await response.clone().text());
    assert.equal((await response.json()).items.length,1);
    assert.deepEqual(await (await identity.fetch(origin+'/fixture/counts')).json(),{abuse:2});
    assert.deepEqual(await (await owner.fetch(origin+'/fixture/counts')).json(),{rpc:1,directory:1});assert.equal(nativeCalls,1);
    // All actual HTTP retries cost metadata funds, including manager-limit denial.
    for(let i=1;i<32;i++) await send();
    const before={native:nativeCalls,owner:await (await owner.fetch(origin+'/fixture/counts')).json()};
    const denied=await send();assert.ok(denied.status>=400);
    assert.equal(nativeCalls,before.native);
    assert.deepEqual(await (await owner.fetch(origin+'/fixture/counts')).json(),before.owner);
    assert.deepEqual(await (await identity.fetch(origin+'/fixture/counts')).json(),{abuse:65});
  }finally{await mf.dispose();rmSync(persist,{recursive:true,force:true});}
});
