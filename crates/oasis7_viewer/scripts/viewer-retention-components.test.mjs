import { test } from 'node:test';
import assert from 'node:assert/strict';
import { compareRetentionComponent, numericRetentionRecord, summarizeRetentionWindow } from './viewer-retention-components.mjs';
test('unknown and zero baselines cannot masquerade as memory growth passes', () => {
 assert.equal(compareRetentionComponent(null, 100).status, 'unknown'); assert.equal(compareRetentionComponent(0, 100).status, 'unknown'); assert.equal(compareRetentionComponent(100, 111).status, 'over_budget'); assert.equal(compareRetentionComponent(100, 110).status, 'within_budget');
});
test('stream records reject DTO, identity and arbitrary strings', () => {
 for (const input of [{view:{entities:[]}},{scope:'secret'},{phase:'private-agent-name'},{jsUsedHeapBytes:NaN},{jsUsedHeapBytes:true},{pagingActive:1}]) assert.throws(()=>numericRetentionRecord(input));
 assert.deepEqual(numericRetentionRecord({timeMs:2,phase:'home',jsUsedHeapBytes:null,wasmLinearBytes:65536}),{timeMs:2,phase:'home',jsUsedHeapBytes:null,wasmLinearBytes:65536});
});

test('short or overlapping windows never certify budgets; known30min component violation remains separate',()=>{
 const first=Array.from({length:6},(_,i)=>({timeMs:i,jsUsedHeapBytes:100,jsTotalHeapBytes:100,viewJsonBytes:1,viewObjects:1}));
 const last=Array.from({length:6},(_,i)=>({timeMs:i+20,jsUsedHeapBytes:112,jsTotalHeapBytes:200,viewJsonBytes:100,viewObjects:100}));
 assert.equal(summarizeRetentionWindow(first,first,{actualSpanMs:1800000,samples:12}).eligible,false);
 const short=summarizeRetentionWindow(first,last,{actualSpanMs:14000,samples:12});assert.equal(short.components.jsUsedHeapBytes.status,'unknown');
 const full=summarizeRetentionWindow(first,last,{actualSpanMs:1800000,samples:12});assert.deepEqual(full.failedComponents,['jsUsedHeapBytes']);assert.equal(full.components.wasmLinearBytes.status,'unknown');assert(!('viewJsonBytes' in full.components));assert.equal(full.diagnosticTrends.viewJsonBytes.growth,99);
});
