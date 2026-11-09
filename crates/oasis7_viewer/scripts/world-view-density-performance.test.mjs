import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { measureDensityPerformance, persistDensityPerformanceEvidence, summarizeDensityFeedback } from './world-view-density-performance.mjs';
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

test('overflow preserves the original input error, raw trace and monitor before scoring fails', () => {
  const out = mkdtempSync(join(tmpdir(), 'density-evidence-'));
  try {
    const evidence = { error: 'Error: armed native input timed out' };
    const monitor = { overflow: false, raf: [], density: [{ status: 'awaiting_frame' }] };
    const overflow = { ...trace, overflow: true };
    persistDensityPerformanceEvidence({ out, evidence, samples: [sample], monitor, trace: overflow });
    const window = JSON.parse(readFileSync(join(out, 'performance-window.json')));
    assert.equal(window.error, evidence.error);
    assert.match(window.scoringError, /overflow/);
    assert.deepEqual(JSON.parse(readFileSync(join(out, 'performance-monitor.json'))), monitor);
    assert.deepEqual(JSON.parse(readFileSync(join(out, 'performance-input-paint-trace.raw.json'))), overflow);
    assert.equal(window.feedback, undefined);
  } finally { rmSync(out, { recursive: true }); }
});
test('missing trace remains an explicit scoring failure while available monitor survives', () => {
  const out = mkdtempSync(join(tmpdir(), 'density-evidence-'));
  try {
    const evidence = { collectionErrors: { trace: 'page closed' } }, monitor = { overflow: false, raf: [], density: [] };
    persistDensityPerformanceEvidence({ out, evidence, samples: [], monitor, trace: null });
    const window = JSON.parse(readFileSync(join(out, 'performance-window.json')));
    assert.match(window.scoringError, /collection incomplete/);
    assert.deepEqual(window.collectionErrors, evidence.collectionErrors);
    assert.deepEqual(JSON.parse(readFileSync(join(out, 'performance-monitor.json'))), monitor);
  } finally { rmSync(out, { recursive: true }); }
});

test('measurement rethrows the original collection error even when final trace overflows', async () => {
  const out = mkdtempSync(join(tmpdir(), 'density-measurement-'));
  const primary = Error('controlled pre-native armed timeout');
  const monitor = { overflow: false, raf: [], density: [{ status: 'awaiting_frame' }] };
  const evaluate = async code => {
    if (code.includes('__DENSITY_MONITOR__.dispose()')) return JSON.stringify(monitor);
    if (code === 'JSON.stringify(window.__VIEWER_INPUT_TRACE__)') return JSON.stringify({ ...trace, overflow: true });
    if (code.startsWith('JSON.stringify((()=>{')) throw primary;
    if (code.includes('dense frame did not settle')) return JSON.stringify({ visibility: 'visible', targets: 1000,
      metrics: { status: 'observed', display_profile: 'world_view_v1', projected_marker_count: 1000,
        agent_body_sprites: 1000, in_view_agent_body_sprites: 1000, laid_out_in_view_labels: 200 },
      authReady: false, playerId: null, publicKey: null, boundAgent: null, registration: 'guest' });
    return true;
  };
  try {
    await assert.rejects(measureDensityPerformance({ evaluate, run: async () => {}, fixture: { description: () => ({ cut: 'C' }) }, wire: [], out, durationMs: 1 }), error => error === primary);
    const window = JSON.parse(readFileSync(join(out, 'performance-window.json')));
    assert.equal(window.error, String(primary)); assert.match(window.scoringError, /overflow/);
    assert.deepEqual(JSON.parse(readFileSync(join(out, 'performance-monitor.json'))), monitor);
  } finally { rmSync(out, { recursive: true }); }
});

test('an armed input miss is retained with focus and attachment evidence without a retry', async () => {
  const primary = Error('controlled native input timeout'), commands = [];
  const diagnostic = { input: { status: 'armed', expectedId: 'A' }, active: { tag: 'BODY', id: null }, expectedTargetAttached: false, transport: 'cli' };
  await assert.rejects(collectNativeSelectionSamples({ count: 1,
    run: async args => commands.push(args),
    evaluate: async code => {
      if (code.includes('return candidates[0]')) return { id: 'A' };
      if (code.includes('const deadline=performance.now()+6000')) throw primary;
      if (code.includes('expectedTargetAttached')) return diagnostic;
      return true;
    },
  }), error => error === primary && error.nativeInputDiagnostic === diagnostic);
  assert.deepEqual(commands, [['press', 'Enter']]);
});
