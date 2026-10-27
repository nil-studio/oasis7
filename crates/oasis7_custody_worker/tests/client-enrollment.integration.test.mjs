import assert from 'node:assert/strict';
import test from 'node:test';
import {createPrivateKey,createPublicKey,createHash,sign} from 'node:crypto';
import {mkdtempSync,rmSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {dirname,resolve} from 'node:path';
import {fileURLToPath} from 'node:url';
import {Miniflare,convertV4MiniflareOptions} from 'miniflare';
const root=resolve(dirname(fileURLToPath(import.meta.url)),'..');
const origin='https://custody.example';
const key=createPrivateKey({key:Buffer.concat([Buffer.from('302e020100300506032b657004220420','hex'),Buffer.alloc(32,8)]),format:'der',type:'pkcs8'});
const publicKey=createPublicKey(key).export({format:'der',type:'spki'}).subarray(-32).toString('hex');
const sha=body=>createHash('sha256').update(body).digest('base64');
function proofHeaders(view,body){
  const created=Math.floor(Date.now()/1000);
  const components=['@method','@target-uri','content-type','content-digest','oasis7-token-digest','oasis7-enrollment','oasis7-request-id'];
  const params=`(${components.map(x=>JSON.stringify(x)).join(' ')});created=${created};expires=${created+29};nonce="${view.server_nonce}";alg="ed25519";keyid="${view.key_id}";tag="oasis7-custody-enrollment-proof-v1"`;
  const headers={'content-type':'application/json','content-digest':`sha-256=:${sha(body)}:`,'oasis7-token-digest':`:${sha('')}:`,'oasis7-enrollment':view.enrollment_id,'oasis7-request-id':'prove-one'};
  const values={'@method':'POST','@target-uri':`${origin}/v1/client-enrollments/${view.enrollment_id}/prove`,...headers};
  const base=components.map(x=>`"${x}": ${values[x]}`).concat(`"@signature-params": ${params}`).join('\n');
  return {...headers,'signature-input':`oasis7=${params}`,signature:`oasis7=:${sign(null,Buffer.from(base),key).toString('base64')}:`};
}
function options(persist){return {...convertV4MiniflareOptions({name:'enrollment-test',modulesRoot:root,modules:[
  {type:'ESModule',path:resolve(root,'tests/fixtures/client_enrollment_runtime.mjs')},
  {type:'ESModule',path:resolve(root,'artifacts/identity/entry.mjs')},
  {type:'ESModule',path:resolve(root,'artifacts/identity/index.js')},
  {type:'CompiledWasm',path:resolve(root,'artifacts/identity/index_bg.wasm')}],compatibilityDate:'2026-10-25',cf:false,
  durableObjects:{CLIENT_ENROLLMENTS:{className:'ClientEnrollmentAuthority',useSQLite:true}},bindings:{CUSTODY_CLIENT_ENROLLMENT_ENABLED:'true',CUSTODY_PUBLIC_ORIGIN:origin,CUSTODY_ENROLLMENT_REALM:'fixture-enrollment',CUSTODY_HOST_DEPLOYMENTS:'[]'}}),resourcePersistencePath:persist,unsafeDevRegistryPath:resolve(persist,'registry')};}
test('actual Enrollment actor preserves directed proof across restart without granting host approval',{timeout:60000},async()=>{
  const persist=mkdtempSync(resolve(tmpdir(),'oasis7-enrollment-real-'));
  let mf=new Miniflare(options(persist));
  try{
    const target={custody_account_id:'account-one',provision_id:'provision-one',invite_id:'invite-one',invite_digest:'ab'.repeat(32)};
    const create=JSON.stringify({api_version:1,request_id:'create-one',target,descriptor:{api_version:1,environment:'test',transport_profile:'native-broker-custody-v1',tool_profile:'custody-native-v1',host_instance_id:'host-one',client_public_key:publicKey,build_digest:'bc'.repeat(32),skill_manifest_digest:'cd'.repeat(32),tool_manifest_digest:'de'.repeat(32),https_origin:'https://host.example',restricted_runtime_identity:'synthetic-not-isolation-acceptance',isolation_mechanism:'separate_os_principal',isolation_acceptance_refs:['synthetic-no-CT008-PASS']}});
    const post=body=>({method:'POST',headers:{'content-type':'application/json'},body});
    const response=await mf.dispatchFetch(origin+'/v1/client-enrollments',post(create));
    assert.equal(response.status,200,await response.clone().text());
    const view=await response.json();
    assert.deepEqual(await (await mf.dispatchFetch(origin+'/v1/client-enrollments',post(create))).json(),view);
    const body=JSON.stringify({api_version:1,enrollment_id:view.enrollment_id,request_id:'prove-one',target,client_public_key:publicKey,server_nonce:view.server_nonce,challenge_expires_at:view.challenge_expires_at,descriptor_digest:view.descriptor_digest});
    const url=origin+`/v1/client-enrollments/${view.enrollment_id}/prove`;
    const headers=proofHeaders(view,body);
    const invalid=await mf.dispatchFetch(url,{method:'POST',headers,body:body.replace('invite-one','invite-other')});
    assert.ok(invalid.status>=400,'modified directed body cannot be proved');
    const proved=await mf.dispatchFetch(url,{method:'POST',headers,body});
    assert.equal(proved.status,200,await proved.clone().text());
    assert.equal((await proved.json()).status,'proved');
    const read=()=>mf.dispatchFetch(origin+'/fixture/read/'+view.enrollment_id).then(r=>r.json());
    const original=await read();assert.equal(original.fact.registration.request.target.custody_account_id,'account-one');
    assert.equal(original.fact.registration.request.descriptor.isolation_acceptance_refs[0],'synthetic-no-CT008-PASS');
    await mf.dispose();mf=new Miniflare(options(persist));
    assert.deepEqual(await read(),original);
  }finally{await mf.dispose();rmSync(persist,{recursive:true,force:true});}
});
