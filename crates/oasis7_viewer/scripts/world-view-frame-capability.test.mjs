import {test} from 'node:test';
import assert from 'node:assert/strict';
import {summarizeFrameEvents,probeWorldFrameCapabilities} from './world-view-frame-capability.mjs';
test('browser DrawFrame and screenshots cannot establish world presentation or a frame-budget pass',()=>{
 const r=summarizeFrameEvents([{name:'DrawFrame'},{name:'DrawFrame'},{name:'Screenshot'},{name:'Paint'}]);
 assert.equal(r.eventCounts.DrawFrame,2);assert.equal(r.screenshots,1);
 assert.equal(r.worldGpuCompletion,null);assert.equal(r.worldPresentedFrames,null);assert.equal(r.frameBudget,'unknown');
});
test('probe refuses an unbounded duration before connecting',async()=>{
 for(const durationMs of [0,999,10001,NaN])await assert.rejects(probeWorldFrameCapabilities({durationMs}),/1..10 seconds/);
});
