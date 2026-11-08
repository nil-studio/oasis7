import { test } from 'node:test';
import assert from 'node:assert/strict';
import { compareRetentionComponent, numericRetentionRecord } from './viewer-retention-components.mjs';
test('unknown and zero baselines cannot masquerade as memory growth passes', () => {
 assert.equal(compareRetentionComponent(null, 100).status, 'unknown'); assert.equal(compareRetentionComponent(0, 100).status, 'unknown'); assert.equal(compareRetentionComponent(100, 111).status, 'over_budget'); assert.equal(compareRetentionComponent(100, 110).status, 'within_budget');
});
test('stream records reject DTO, identity and arbitrary strings', () => {
 for (const input of [{view:{entities:[]}},{scope:'secret'},{phase:'private-agent-name'},{jsUsedHeapBytes:NaN},{jsUsedHeapBytes:true},{pagingActive:1}]) assert.throws(()=>numericRetentionRecord(input));
 assert.deepEqual(numericRetentionRecord({timeMs:2,phase:'home',jsUsedHeapBytes:null,wasmLinearBytes:65536}),{timeMs:2,phase:'home',jsUsedHeapBytes:null,wasmLinearBytes:65536});
});
