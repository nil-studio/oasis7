import { test } from 'node:test';
import assert from 'node:assert/strict';
import { compareRetentionComponent, numericRetentionRecord, summarizeRetentionWindow, classifyRetentionPhase, describeHomeCameraMismatch } from './viewer-retention-components.mjs';
test('unknown and zero baselines cannot masquerade as memory growth passes', () => {
 assert.equal(compareRetentionComponent(null, 100).status, 'unknown'); assert.equal(compareRetentionComponent(0, 100).status, 'unknown'); assert.equal(compareRetentionComponent(100, 111).status, 'over_budget'); assert.equal(compareRetentionComponent(100, 110).status, 'within_budget');
});
test('stream records reject DTO, identity and arbitrary strings', () => {
 for (const input of [{view:{entities:[]}},{scope:'secret'},{phase:'private-agent-name'},{jsUsedHeapBytes:NaN},{jsUsedHeapBytes:true},{pagingActive:1}]) assert.throws(()=>numericRetentionRecord(input));
 assert.deepEqual(numericRetentionRecord({timeMs:2,phase:'home',jsUsedHeapBytes:null,wasmLinearBytes:65536}),{timeMs:2,phase:'home',jsUsedHeapBytes:null,wasmLinearBytes:65536});
});

test('short or overlapping windows never certify budgets; known30min component violation remains separate',()=>{
 const first=Array.from({length:6},(_,i)=>({timeMs:i,memoryPhaseMatched:true,jsUsedHeapBytes:100,jsTotalHeapBytes:100,viewJsonBytes:1,viewObjects:1}));
 const last=Array.from({length:6},(_,i)=>({timeMs:i+20,memoryPhaseMatched:true,jsUsedHeapBytes:112,jsTotalHeapBytes:200,viewJsonBytes:100,viewObjects:100}));
 assert.equal(summarizeRetentionWindow(first,first,{actualSpanMs:1800000,samples:12}).eligible,false);
 const short=summarizeRetentionWindow(first,last,{actualSpanMs:14000,samples:12});assert.equal(short.components.jsUsedHeapBytes.status,'unknown');
 const full=summarizeRetentionWindow(first,last,{actualSpanMs:1800000,samples:12});assert.deepEqual(full.failedComponents,['jsUsedHeapBytes']);assert.equal(full.components.wasmLinearBytes.status,'unknown');assert(!('viewJsonBytes' in full.components));assert.equal(full.diagnosticTrends.viewJsonBytes.growth,99);first[0].memoryPhaseMatched=false;assert.equal(summarizeRetentionWindow(first,last,{actualSpanMs:1800000,samples:12}).components.jsUsedHeapBytes.status,'unknown');
});

test('normal query crossing stays unknown while scope, identity or physical phase loss stops',()=>{
 const before={identityValid:true,clientSourceMatches:true,renderCutMatches:true,instance:1,scope:1,generation:2,origin:10,camera:{zoom:2},stale:false,error:false,loading:false,currentQuery:true,observed:true,complete:true,entities:1000,bodies:1000,labels:200};
 assert.equal(classifyRetentionPhase(before,{...before}),'matched');assert.equal(classifyRetentionPhase(before,{...before,generation:3,loading:true,complete:false,entities:400,bodies:400}),'poll_crossed');assert.equal(classifyRetentionPhase(before,{...before,generation:3,complete:true,entities:999}),'precondition_lost');assert.equal(classifyRetentionPhase(before,{...before,generation:3,observed:false,renderCutMatches:null}),'poll_crossed');assert.equal(classifyRetentionPhase(before,{...before,generation:3,observed:true,renderCutMatches:false}),'precondition_lost');
 for(const change of [{scope:2},{instance:2},{origin:11},{identityValid:false},{bodies:800},{camera:{zoom:3}}])assert.equal(classifyRetentionPhase(before,{...before,...change}),'precondition_lost');
});

test('failed home phase retains both exact cameras without rounding or relaxing the contract',()=>{
 const expected={zoom:1.8461538461538465,pan_y_px:0},actual={zoom:1.8461538461538463,pan_y_px:99.6923076923077};
 assert.deepEqual(describeHomeCameraMismatch(expected,actual,'home'),{phase:'home',expectedCamera:expected,actualCamera:actual});assert.equal(describeHomeCameraMismatch(expected,{...expected},'home'),null);
});

import {validateMountedMemoryLease} from './viewer-retention-components.mjs';
test('WASM observations cannot silently cross mount leases between samples',()=>{const first=validateMountedMemoryLease(null,1);assert.equal(validateMountedMemoryLease(first,1),1);assert.throws(()=>validateMountedMemoryLease(first,2),/between observations/);assert.throws(()=>validateMountedMemoryLease(null,null),/missing/);});
