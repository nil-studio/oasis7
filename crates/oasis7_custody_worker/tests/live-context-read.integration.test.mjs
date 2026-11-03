// Valid Native SID/CSRF -> real World current executor/fence/Get read protocol.
// World Native authority and policy are preconfigured, strongly signed isolated fixtures;
// this does not prove Signup, Native MFA/managed binding issuance, or custody Source admission.
import test from 'node:test';
import assert from 'node:assert/strict';
import {isDeepStrictEqual} from 'node:util';
import {mkdtempSync,rmSync,readFileSync,existsSync,writeFileSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {resolve} from 'node:path';
import {setTimeout as delay} from 'node:timers/promises';
import {worldAuthority} from './fixtures/world_runtime_support.mjs';
import {nativeLiveContextAuthority} from './fixtures/role_runtime_native_support.mjs';
async function readReady(path) {
  for(let n=0;n<1000;n++) {
    if(existsSync(path)) {try{return JSON.parse(readFileSync(path,'utf8'));}catch{/* bounded atomic readiness */}}
    await delay(10);
  }
  throw new Error('fixed live context fixture readiness timeout');
}
test('real Native live context read binds World nonce and observes a genuine newer commit',{timeout:180000},async()=>{
  const directory=mkdtempSync(resolve(tmpdir(),'oasis7-live-context-bridge-'));
  let world,native;
  try {
    world=await worldAuthority(directory,{liveContext:true});
    native=await nativeLiveContextAuthority(directory,world.data);
    let nativeCalls=0;
    const call=async request=>{
      nativeCalls++;
      const response=await fetch(native.data.address+'/api/public/hosted-account/v2/entry-executor/control',{
        method:'POST',headers:{'Content-Type':'application/json',Origin:'https://localhost',Cookie:native.data.cookie,'X-Oasis7-CSRF':native.data.csrf,'Sec-Fetch-Site':'same-origin'},body:JSON.stringify(request)});
      assert.equal(response.status,200,'real Native handler must successfully forward both World reads');
      // Native fixture runs SDK verify_live_context_reply plus wrong-nonce rejection before returning this unmodified response.
      const reply=await response.json();
      assert.equal(reply.ok===true,true,'real Native response must report success');
      return reply.world_response;
    };
    const firstRequest=world.data.request;
    const first=await call(firstRequest);
    writeFileSync(resolve(directory,'advance'),'advance-one-real-world-commit');
    const secondRequest=await readReady(resolve(directory,'next.json'));
    assert.equal(isDeepStrictEqual(secondRequest.operation.request.original,firstRequest.operation.request.original),true,'original signed Get must remain unchanged');
    assert.equal(secondRequest.operation.request.observation_nonce!==firstRequest.operation.request.observation_nonce,true,'fresh observations must use distinct nonces');
    const second=await call(secondRequest);
    assert.equal(second.request_digest!==first.request_digest,true,'different observation requests must have distinct bound digests');
    assert.ok(BigInt(second.payload.current_public_context.read_commit.position)>BigInt(first.payload.current_public_context.read_commit.position),'World must fresh-load after real commit advancement');
    assert.equal(isDeepStrictEqual(second.payload.prepared.identity,first.payload.prepared.identity),true,'original context identity must remain unchanged');
    writeFileSync(resolve(directory,'stats-request'),'observe-fixed-ports');
    const counts=await readReady(resolve(directory,'stats.json'));
    assert.equal(nativeCalls,2);assert.equal(counts.view,2);assert.equal(counts.live_read,2);
    // Six actual HTTP ports for two calls; startup is separate and no Exec400 claim is made.
  } finally {
    await native?.close();await world?.close();rmSync(directory,{recursive:true,force:true});
  }
});
