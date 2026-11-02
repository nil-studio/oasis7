// Transport component only: fake private binding, no Account payment/Current proof.
import test from 'node:test';
import assert from 'node:assert/strict';
import {fetchTermOwnerSlotQualification as once} from '../platform/term_owner_slot_qualification_v2.mjs';
const name='ab'.repeat(32);
function binding(fetch){let calls=0;const env={ROLE_KEYS:{idFromName(value){assert.equal(value,name);return value;},get(value){assert.equal(value,name);return{fetch(request){calls++;return fetch(request);}}}}};return{env,count:()=>calls};}
test('Term slot fixed transport sends once with exact authority and bounded JSON',async()=>{
  let captured;
  const b=binding(async request=>{captured=request;assert.equal(request.url,'https://role-key.internal/internal/role-key/term-owner-slot-qualification-v2');assert.equal(request.method,'POST');assert.equal(request.redirect,'manual');assert.equal(await request.text(),'{}');return Response.json({component:true});});
  assert.equal(await once(b.env,name,'{}',Date.now()+1000),'{"component":true}');
  assert.equal(b.count(),1);assert.equal(captured.signal.aborted,true);
});
test('Term slot fixed transport deadline cancels a stalled body without retry', {timeout:5000},async()=>{
  let cancelled=false;let signal;
  const b=binding(async request=>{signal=request.signal;return new Response(new ReadableStream({cancel(){cancelled=true;}}),{headers:{'Content-Type':'application/json'}});});
  await assert.rejects(once(b.env,name,'{}',Date.now()+80));
  assert.equal(b.count(),1);assert.equal(signal.aborted,true);assert.equal(cancelled,true);
});
test('Term slot fixed transport rejects unknown, wrong MIME and oversized bodies',async()=>{
  for(const response of [()=>Promise.reject(new Error('unknown')),()=>new Response('{}',{headers:{'Content-Type':'application/jsonp'}}),()=>new Response('x'.repeat(32769),{headers:{'Content-Type':'application/json'}})]){
    const b=binding(response);await assert.rejects(once(b.env,name,'{}',Date.now()+1000));assert.equal(b.count(),1);
  }
  const b=binding(()=>Response.json({}));await assert.rejects(once(b.env,name,'{}',Date.now()-1));assert.equal(b.count(),0);
});
