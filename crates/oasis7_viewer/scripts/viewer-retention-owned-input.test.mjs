import test from 'node:test';
import assert from 'node:assert/strict';
import {createStartupProfile,launchOwnedStartupContext} from './viewer-owned-startup-context.mjs';
import {createRetentionOwnedInput,activateOwnedRetentionOverview} from './viewer-retention-owned-input.mjs';
import vm from 'node:vm';
test('closed exploration is opened before one native overview input, then closed again',async()=>{
 const d={open:false},button={disabled:false,getClientRects:()=>[{}],focus(){document.activeElement=this;}},document={activeElement:null,querySelector:s=>s==='[data-query-overview]'?button:d};const calls=[];
 await activateOwnedRetentionOverview({evaluate:async code=>vm.runInNewContext(code,{document}),run:async args=>{calls.push(args);d.open=!d.open;},enter:async()=>{assert.equal(d.open,true);assert.equal(document.activeElement,button);calls.push(['native','Enter']);}});
 assert.deepEqual(calls.map(x=>x[0]),['click','native','click']);assert.equal(d.open,false);
});
test('unavailable overview is refused before sending native input, never retried',async()=>{
 let enters=0;const document={querySelector:s=>s==='[data-query-overview]'?{disabled:true,getClientRects:()=>[{}]}:{open:true}};
 await assert.rejects(()=>activateOwnedRetentionOverview({evaluate:async code=>vm.runInNewContext(code,{document}),run:async()=>{throw Error('unexpected click');},enter:async()=>enters++}),/unavailable/);assert.equal(enters,0);
});
test('owned adapter sends one native Enter pair without native platform codes or another launch',async()=>{
 const sent=[];let launches=0,detaches=0,pages;
 const page={url:()=> 'owned',evaluate:async code=>code,mouse:{move:async(...p)=>sent.push(p)}};
 pages=[page];const context={pages:()=>pages,once(){},newCDPSession:async()=>({send:async(method,p)=>sent.push({method,p}),detach:async()=>detaches++})};
 await launchOwnedStartupContext({launchPersistentContext:async()=>{launches++;return context;}},'unused',createStartupProfile());
 const input=createRetentionOwnedInput(context,'owned'),native=await input.createNativeInput();await native.enter();await native.close();
 assert.equal(launches,1);assert.equal(sent.length,2);assert.ok(sent.every(r=>r.method==='Input.dispatchKeyEvent'&&!('nativeVirtualKeyCode' in r.p)));assert.equal(detaches,1);
 pages=[{...page}];await assert.rejects(()=>input.run(['mouse','move','1','2']),/page changed/);
});
test('unbranded context and unsupported operation cannot manufacture ownership or input',async()=>{
 assert.throws(()=>createRetentionOwnedInput({pages:()=>[]},'owned'),/factory/);
 const page={url:()=> 'owned'},context={pages:()=>[page],once(){}};
 await launchOwnedStartupContext({launchPersistentContext:async()=>context},'unused',createStartupProfile());
 const input=createRetentionOwnedInput(context,'owned');await assert.rejects(()=>input.run(['force-click','button']),/allowlist/);
 await assert.rejects(()=>input.run(['wait','45001']),/allowlist/);
});
