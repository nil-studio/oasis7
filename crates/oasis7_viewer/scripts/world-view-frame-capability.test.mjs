import {test} from 'node:test';
import assert from 'node:assert/strict';
import {summarizeFrameEvents,probeWorldFrameCapabilities,withObservationCleanup} from './world-view-frame-capability.mjs';
test('browser DrawFrame and screenshots cannot establish world presentation or a frame-budget pass',()=>{
 const r=summarizeFrameEvents([{name:'DrawFrame'},{name:'DrawFrame'},{name:'Screenshot'},{name:'Paint'}]);
 assert.equal(r.eventCounts.DrawFrame,2);assert.equal(r.screenshots,1);
 assert.equal(r.worldGpuCompletion,null);assert.equal(r.worldPresentedFrames,null);assert.equal(r.frameBudget,'unknown');
});
test('probe refuses an unbounded duration before connecting',async()=>{
 for(const durationMs of [0,999,10001,NaN])await assert.rejects(probeWorldFrameCapabilities({durationMs}),/1..10 seconds/);
});

test('cleanup failure does not mask the original observation error',async()=>{
 const original=Error('action failed'),report={};
 await assert.rejects(withObservationCleanup(async()=>{throw original},async()=>{throw Error('stop failed')},report),e=>e===original);
 assert.deepEqual(report.cleanupErrors,['Error: stop failed']);
 await assert.rejects(withObservationCleanup(async()=>true,async()=>{throw Error('stop failed')},{}),/stop failed/);
});

test('outer report-write failure preserves body error and exposes the failed cleanup',async()=>{
 const original=Error('capture failed'),report={};
 await assert.rejects(withObservationCleanup(async()=>{throw original},async()=>{throw Error('ENOSPC report write')},report),e=>e===original&&e.observationCleanupErrors[0]==='Error: ENOSPC report write');
});

test('all thrown values remain failures, including falsy JavaScript rejection values',async()=>{
 for(const value of [null,undefined,0,'',false]) {
  let rejected=false;try{await withObservationCleanup(async()=>{throw value},async()=>{},{});}catch(actual){rejected=true;assert.equal(actual,value);}
  assert(rejected);
 }
});

test('ordered cleanup always reaches report writing after close failure and keeps falsy body error',async()=>{
 const calls=[],report={};let rejected=false;
 try { await withObservationCleanup(async()=>{throw 0},[()=>{calls.push('close');throw Error('close failed')},()=>calls.push('dispose'),()=>{calls.push('write');throw Error('write failed')}],report); }
 catch(actual){rejected=true;assert.equal(actual,0);}
 assert(rejected);assert.deepEqual(calls,['close','dispose','write']);assert.deepEqual(report.cleanupErrors,['Error: close failed','Error: write failed']);
});
test('without body failure the first cleanup error wins and remaining cleanup still runs',async()=>{
 const first=Error('first'),calls=[],report={};
 await assert.rejects(withObservationCleanup(async()=>true,[()=>{throw first},()=>calls.push('written')],report),e=>e===first);
 assert.deepEqual(calls,['written']);
});
