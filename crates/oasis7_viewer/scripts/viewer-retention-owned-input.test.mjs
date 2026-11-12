import test from 'node:test';
import assert from 'node:assert/strict';
import {createStartupProfile,launchOwnedStartupContext} from './viewer-owned-startup-context.mjs';
import {createRetentionOwnedInput} from './viewer-retention-owned-input.mjs';
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
