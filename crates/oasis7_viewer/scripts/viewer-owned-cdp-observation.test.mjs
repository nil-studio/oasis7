import {test} from 'node:test';
import assert from 'node:assert/strict';
import {createOwnedCdpSession} from './viewer-owned-cdp-session.mjs';
import {readOwnedMemory} from './viewer-owned-cdp-memory.mjs';

test('memory components remain separate and missing accounting is unknown',async()=>{
 const calls=[];const value=await readOwnedMemory({page:async method=>{calls.push(method);return method==='Runtime.getHeapUsage'?{usedSize:12,totalSize:24}:{nodes:5,documents:1,jsEventListeners:3};}});
 assert.equal(value.jsUsedHeapBytes,12);assert.equal(value.domNodes,5);
 assert.equal(value.ownedRssBytes,null);assert.equal(value.wasmLinearBytes,null);assert.equal(value.gpuBytes,null);
 assert.deepEqual(calls,['Runtime.getHeapUsage','Memory.getDOMCounters']);
});
test('invalid or unavailable component does not become zero or hide other components',async()=>{
 const value=await readOwnedMemory({page:async method=>{if(method==='Runtime.getHeapUsage')throw Error('unsupported');return{nodes:-1,documents:1,jsEventListeners:1};}});
 assert.equal(value.jsUsedHeapBytes,null);assert.equal(value.domNodes,null);assert.match(value.capabilityReasons.heap,/unsupported/);assert.match(value.capabilityReasons.dom,/invalid/);
});
test('session restricts routing, browser-global observation, and closes pending work',async()=>{
 const original=globalThis.WebSocket, sent=[],pages=[{type:'page',url:'http://owned/',targetId:'a'}];let socket;
 globalThis.WebSocket=class extends EventTarget {
  constructor(){super();socket=this;queueMicrotask(()=>this.dispatchEvent(new Event('open')));}
  send(raw){const r=JSON.parse(raw);sent.push(r);const result=r.method==='Target.getTargets'?{targetInfos:pages}:r.method==='Target.attachToTarget'?{sessionId:'owned'}:{};queueMicrotask(()=>this.dispatchEvent(new MessageEvent('message',{data:JSON.stringify({id:r.id,result})})));}
  close(){this.closed=true;}
 };
 try {
  const s=await createOwnedCdpSession(async()=>JSON.stringify({data:{cdpUrl:'ws://127.0.0.1:9222/devtools/browser/owned'}}),async()=> 'http://owned/');
  await s.browser('Browser.getVersion');
  pages.push({type:'page',url:'http://other/',targetId:'b'});
  await assert.rejects(s.browser('Tracing.start'),/one owned page/);
  assert(!sent.some(row=>row.method==='Tracing.start'));
  assert.throws(()=>s.page('HeapProfiler.collectGarbage'),/outside/);
  await s.page('Memory.getDOMCounters');assert.equal(sent.at(-1).sessionId,'owned');
  let events=0;const remove=s.onEvent(()=>events++);socket.dispatchEvent(new MessageEvent('message',{data:JSON.stringify({method:'Tracing.bufferUsage',params:{percentFull:1}})}));assert.equal(events,1);remove();
  s.close();assert(socket.closed);await assert.rejects(s.page('Runtime.getHeapUsage'),/closed/);
 }finally{globalThis.WebSocket=original;}
});
