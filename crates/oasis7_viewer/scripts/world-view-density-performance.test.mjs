import { test } from 'node:test';
import assert from 'node:assert/strict';
import { summarizeDensityFeedback } from './world-view-density-performance.mjs';
import { collectNativeSelectionSamples } from './viewer-performance-native-input.mjs';
const display = { status: 'observed', profile: 'world_view_v1', cut: 'C', markers: 1000, bodies: 1000, inView: 1000, labels: 200 };
const sample = { index: 0, expectedId: 'A', startTime: 100, clickTime: 102, endTime: 180,
  durationMs: 80, isTrusted: true, clickTrusted: true, displayAtInput: display };
const trace = { supported: true, mutations: [{ target: 'A', time: 108, selected: true }],
  events: [{ type: 'click', target: 'A', startTime: 102, duration: 40, interactionId: 1 }] };
test('actual observed 1000/200 is independently required for a complete density result', () => {
  const r = summarizeDensityFeedback([sample], trace);
  assert.equal(r.counts.observedDense, 1); assert.equal(r.allInputsConfirmedDensity, true);
  assert.equal(r.allFeedback100ms.dom, 'pass'); assert.equal(r.confirmedDensityFeedback100ms.paint, 'pass');
});
test('pending metrics are unknown and retain feedback without borrowing last density', () => {
  const r = summarizeDensityFeedback([{ ...sample, displayAtInput: { status: 'awaiting_frame' } }], trace);
  assert.equal(r.all.samples, 1); assert.equal(r.allFeedback100ms.dom, 'pass');
  assert.equal(r.dense.samples, 0); assert.equal(r.counts.pendingOrUnknown, 1);
  assert.equal(r.allInputsConfirmedDensity, false); assert.equal(r.confirmedDensityFeedback100ms.dom, 'unknown');
});
test('a real partial 800 scene is not certified as 1000 because labels still reach 200', () => {
  const r = summarizeDensityFeedback([{ ...sample, displayAtInput: { ...display, markers: 800, bodies: 800, inView: 800 } }], trace);
  assert.equal(r.counts.observedBelowDensity, 1); assert.equal(r.dense.samples, 0);
  assert.equal(r.allInputsConfirmedDensity, false);
});
test('missing DOM feedback cannot pass a budget based on the observed subset', () => {
  const r = summarizeDensityFeedback([sample], { ...trace, mutations: [] });
  assert.equal(r.allFeedback100ms.dom, 'unknown'); assert.equal(r.allFeedback100ms.paint, 'unknown');
});
test('a 104ms estimate fails 100ms while its quantization ambiguity is retained', () => {
  const r = summarizeDensityFeedback([{ ...sample, clickTime: 100 }], {
    ...trace, events: [{ ...trace.events[0], startTime: 100, duration: 104 }] });
  assert.equal(r.allFeedback100ms.paint, 'fail');
  assert.deepEqual(r.allFeedback100ms.paintP95Interval, [100, 108]);
  assert.equal(r.allFeedback100ms.paintBound, 'rounding_straddles_100');
});
test('unknown selection observation mode rejects before any browser input', async () => {
  await assert.rejects(collectNativeSelectionSamples({ selectionSource: 'invented', evaluate() { throw Error('must not evaluate'); } }), /unknown selection/);
});
