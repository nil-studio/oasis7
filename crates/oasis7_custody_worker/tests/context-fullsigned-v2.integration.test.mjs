import assert from 'node:assert/strict';
import test from 'node:test';
import {isDeepStrictEqual} from 'node:util';
import {createHash} from 'node:crypto';
import {readFileSync,mkdtempSync,rmSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {dirname,resolve,join} from 'node:path';
import {fileURLToPath} from 'node:url';
import {Miniflare,convertV4MiniflareOptions} from 'miniflare';
const root=resolve(dirname(fileURLToPath(import.meta.url)),'..');
const fixture=JSON.parse(readFileSync(resolve(root,'artifacts/context-fullsigned-v2-fixture.json'),'utf8'));
assert.equal(fixture.label,'synthetic-historical-fullsigned-storage-not-authority');
const digest=value=>createHash('sha256').update(JSON.stringify(value)).digest('hex');
const consume=async response=>{const status=response.status;await response.arrayBuffer();return status;};
async function withFixture(run,extra={}) {
  const persist=mkdtempSync(join(tmpdir(),'oasis7-fullsigned-v2-'));
  const bindings={CUSTODY_CONTEXT_BODY_ENABLED:'true',CUSTODY_CONTEXT_BODY_KEK:'6d'.repeat(32),
    CUSTODY_CONTEXT_BODY_WRAPPING_KEY_VERSION:'1',SYNTHETIC_CONTEXT_WORLD_PIN:JSON.stringify(fixture.world),
    CUSTODY_ROLE_KEY_ENVIRONMENT:fixture.prepared.scope.identity_scope.context.identity.environment,...extra};
  const options=()=>({...convertV4MiniflareOptions({name:'fullsigned-fixture',modulesRoot:root,compatibilityDate:'2026-10-25',cf:false,
    modules:[{type:'ESModule',path:resolve(root,'tests/fixtures/context_fullsigned_v2_runtime.mjs')},
      {type:'ESModule',path:resolve(root,'artifacts/native-harness/entry.mjs')},
      {type:'ESModule',path:resolve(root,'artifacts/native-harness/index.js')},
      {type:'CompiledWasm',path:resolve(root,'artifacts/native-harness/index_bg.wasm')}],
    durableObjects:{HARNESS:{className:'ContextFullSignedV2Harness',useSQLite:true},
      CONTEXT_BODIES:{className:'FullSignedFixtureActor',useSQLite:true}},bindings}),
    resourcePersistencePath:persist,unsafeDevRegistryPath:resolve(persist,'registry')});
  let mf=new Miniflare(options());
  const harness=async input=>{const {HARNESS}=await mf.getBindings();return HARNESS.get(HARNESS.idFromName('synthetic-fixed')).fetch('https://fixture.invalid/fullsigned',
    {method:'POST',body:JSON.stringify(input),headers:{'content-type':'application/json'}});};
  const actor=async(value,path,body)=>{const {CONTEXT_BODIES}=await mf.getBindings();return CONTEXT_BODIES.get(CONTEXT_BODIES.idFromName(value.actor_name))
    .fetch(`https://context-body.internal${path}`,{method:'POST',headers:{'content-type':'application/json'},body:JSON.stringify(body)});};
  const restart=async(next={})=>{await mf.dispose();Object.assign(bindings,next);mf=new Miniflare(options());};
  try {await run({harness,actor,restart});} finally {await mf.dispose();rmSync(persist,{recursive:true,force:true});}
}
async function sealed(harness,input) {const response=await harness(input);assert.equal(response.status,200);return response.json();}
async function opened(harness,value) {const response=await harness({operation:'open',reference:value.reference,record:value.record});
  assert.equal(response.status,200);const fact=await response.json();
  assert.equal(fact.digest,value.reference.scope.body.canonical_sha256);
  assert.equal(fact.bytes,Number(value.reference.scope.body.canonical_bytes));}
test('actual Identity Rust WebCrypto seals both full signed SDK slots and actual Actor preserves ciphertext', {timeout:120000},async()=>{
  await withFixture(async({harness,actor})=>{
    const prepared=await sealed(harness,fixture.prepared);await opened(harness,prepared);
    const captured=await sealed(harness,{...fixture.captured,prior:prepared.reference});await opened(harness,captured);
    for (const value of [prepared,captured]) {
      assert.equal(await consume(await actor(value,'/internal/fullsigned-v2/put',value.record)),200);
      const read=await actor(value,'/internal/fullsigned-v2/read',value.reference.scope);assert.equal(read.status,200);
      assert.equal(isDeepStrictEqual(await read.json(),value.record),true);
      const metadata=await actor(value,'/internal/fullsigned-v2/metadata',value.reference.scope);assert.equal(metadata.status,200);
      const fact=await metadata.json();assert.equal(fact.stored.envelope_digest,value.reference.envelope_digest);
      assert.equal(Object.hasOwn(fact,'envelope'),false);
      assert.equal(await consume(await actor(value,'/internal/put',value.record)),409);
    }
    const bad=structuredClone(prepared);bad.record.envelope.ciphertext='00'+bad.record.envelope.ciphertext.slice(2);
    assert.equal(await consume(await harness({operation:'open',reference:bad.reference,record:bad.record})),409);
    assert.equal(await consume(await harness({...fixture.prepared,verified:true})),409);
    const newEnvelope=await sealed(harness,fixture.prepared);
    assert.equal(digest(newEnvelope.record)===digest(prepared.record),false);
    assert.equal(await consume(await actor(prepared,'/internal/fullsigned-v2/put',newEnvelope.record)),409);
  });
});
test('actual committed V2 Actor lost reply restarts and recovers only the immutable original', {timeout:120000},async()=>{
  await withFixture(async({harness,actor,restart})=>{
    const value=await sealed(harness,fixture.prepared);let lost=false;
    try {lost=await consume(await actor(value,'/internal/fullsigned-v2/put',value.record))===500;} catch {lost=true;}
    assert.equal(lost,true);assert.equal((await (await actor(value,'/fixture/puts',{})).json()).audit,1);
    await restart();
    const response=await actor(value,'/internal/fullsigned-v2/read',value.reference.scope);assert.equal(response.status,200);
    const record=await response.json();assert.equal(isDeepStrictEqual(record,value.record),true);
    await opened(harness,{...value,record});
    const stats=await (await actor(value,'/fixture/puts',{})).json();
    assert.equal(stats.puts,0,'recovery issued no PUT');assert.equal(stats.audit,1);
  },{SYNTHETIC_DROP_FULLSIGNED_REPLY:'true'});
});

test('V2 opaque metadata recovery needs no KEK, Native binding, private SID or usable context', {timeout:120000},async()=>{
  await withFixture(async({harness,actor,restart})=>{
    const value=await sealed(harness,fixture.prepared);
    assert.equal(await consume(await actor(value,'/internal/fullsigned-v2/put',value.record)),200);
    await restart({CUSTODY_CONTEXT_BODY_KEK:''});
    const response=await actor(value,'/internal/fullsigned-v2/metadata',value.reference.scope);
    assert.equal(response.status,200);const metadata=await response.json();
    assert.equal(metadata.stored.envelope_digest,value.reference.envelope_digest);
    for (const key of ['envelope','ciphertext','wrapped_dek','usable','verified','private_body']) assert.equal(Object.hasOwn(metadata,key),false);
    assert.equal(await consume(await harness({operation:'open',reference:value.reference,record:value.record})),409);
  });
});
