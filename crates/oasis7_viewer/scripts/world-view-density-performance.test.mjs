import { test } from 'node:test';
import assert from 'node:assert/strict';
import vm from 'node:vm';
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

for (const selectionSource of ['core', 'renderer']) test(`${selectionSource} diagnostic observes DOM data without interpreting it as code`, async () => {
  const expectedId = `quote'"\\\n</script><script>globalThis.compromised=true</script>\u2028\u2029`;
  for (const attached of [false, true]) for (const active of [false, true]) {
    const primary = Error('stop before input'), commands = [];
    const input = { status: 'armed', expectedId };
    const context = vm.createContext({
      document: {
        activeElement: active ? { tagName: 'BUTTON', dataset: { agentId: expectedId } } : null,
        querySelectorAll: selector => {
          assert.equal(selector, 'button[data-agent-id][data-renderer-target]');
          return [{ dataset: { agentId: 'other' } }, ...(attached ? [{ dataset: { agentId: expectedId } }] : [])];
        },
      },
      window: { __VIEWER_PERF_INPUT__: input },
    });
    let evaluations = 0;
    await assert.rejects(collectNativeSelectionSamples({ selectionSource, count: 1,
      run: async args => commands.push(args),
      evaluate: async code => {
        evaluations++;
        if (evaluations === 1) throw primary;
        return vm.runInContext(code, context);
      },
    }), error => error === primary);
    assert.equal(evaluations, 2);
    assert.deepEqual(commands, []);
    assert.deepEqual(primary.nativeInputDiagnostic, {
      input, active: active ? { tag: 'BUTTON', id: expectedId } : { id: null },
      expectedTargetAttached: attached, transport: selectionSource === 'renderer' ? 'owned_cdp' : 'cli',
    });
    assert.equal(context.compromised, undefined);
  }
});

test('diagnostic failure and malformed JSON preserve the original error object', async () => {
  for (const failure of ['evaluate', 'json']) {
    const primary = Error('original collection failure'); let evaluations = 0;
    await assert.rejects(collectNativeSelectionSamples({ count: 1, evaluate: async () => {
      if (++evaluations === 1) throw primary;
      if (failure === 'evaluate') throw Error('diagnostic unavailable');
      return '{malformed';
    } }), error => error === primary);
    assert.equal(evaluations, 2);
    assert.match(primary.nativeInputDiagnosticError, failure === 'evaluate' ? /diagnostic unavailable/ : /SyntaxError/);
  }
});

test('primitive collection failures are rethrown without attempting a diagnostic', async () => {
  for (const primary of [null, undefined, false, 0, 'failure']) {
    let evaluations = 0;
    try {
      await collectNativeSelectionSamples({ count: 1, evaluate: async () => { evaluations++; throw primary; } });
      assert.fail('collection failure must be rethrown');
    } catch (error) { assert.equal(error, primary); }
    assert.equal(evaluations, 1);
  }
});

const densityRunner = readFileSync(new URL('./pixel-world-world-view-density-visual.mjs', import.meta.url), 'utf8');
function densityFunction(name) {
  const start = densityRunner.indexOf(`async function ${name}(`);
  const end = densityRunner.indexOf('\nasync function ', start + 1);
  assert.ok(start >= 0 && end > start);
  return densityRunner.slice(start, end);
}
for (const name of ['overview', 'searchEmpty']) test(`${name} retains disclosure conditions and native command ordering`, async () => {
  for (const open of [false, true, null]) {
    const commands = [], focuses = [], queries = [];
    const context = vm.createContext({
      document: { querySelector: selector => {
        queries.push(selector);
        if (selector === '.pixel-world-exploration-disclosure') return open === null ? null : { open };
        assert.equal(selector, name === 'overview' ? '[data-query-overview]' : '.pixel-world-exploration form input');
        return { focus: options => { focuses.push(JSON.parse(JSON.stringify(options))); commands.push(['focus', selector]); } };
      } },
      run: async args => commands.push(JSON.parse(JSON.stringify(args))),
    });
    context.evaluate = async code => vm.runInContext(code, context);
    const result = vm.runInContext(densityFunction(name) + `\n${name}()`, context);
    if (name === 'overview' && open === null) {
      await assert.rejects(result, /negotiated exploration UI missing/);
      assert.deepEqual(commands, []);
    } else {
      await result;
      const expected = open ? [] : [['click', '.pixel-world-exploration-disclosure > summary']];
      if (name === 'searchEmpty') expected.push(['fill', '.pixel-world-exploration form input', '']);
      expected.push(['focus', name === 'overview' ? '[data-query-overview]' : '.pixel-world-exploration form input'], ['press', 'Enter']);
      if (name === 'overview') expected.push(['click', '.pixel-world-exploration-disclosure > summary']);
      assert.deepEqual(commands, expected);
      assert.deepEqual(focuses, [{ preventScroll: true }]);
    }
    assert.equal(queries[0], '.pixel-world-exploration-disclosure');
  }
});
