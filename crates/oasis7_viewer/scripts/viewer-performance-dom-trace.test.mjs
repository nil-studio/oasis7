import { test } from 'node:test';
import assert from 'node:assert/strict';
import { JSDOM } from 'jsdom';
import { installPaintTraceScript, matchInputPaintTrace } from './viewer-performance-paint-trace.mjs';

const plain = value => JSON.parse(JSON.stringify(value));
const flush = () => new Promise(resolve => setImmediate(resolve));
async function fixture(action) {
  const dom = new JSDOM('<body><button data-agent-id="A" data-renderer-target data-selected="false">A</button></body>', { runScripts: 'outside-only' });
  dom.window.PerformanceObserver = class {
    static supportedEntryTypes = [];
    observe() {}
    disconnect() {}
  };
  dom.window.eval(installPaintTraceScript);
  try { await action(dom.window, dom.window.__VIEWER_INPUT_TRACE__); }
  finally { dom.window.__VIEWER_INPUT_TRACE__.dispose(); dom.window.close(); }
}
test('connected attribute changes and removal both update observed selection truth', async () => {
  await fixture(async (window, trace) => {
    const button = window.document.querySelector('button');
    button.dataset.selected = 'true'; await flush();
    button.remove(); await flush();
    assert.deepEqual(plain(trace.mutations.map(row => [row.target, row.selected, row.present])), [['A', true, true], ['A', false, false]]);
  });
});
test('a newly inserted selected node is observed without attributing its detached construction', async () => {
  await fixture(async (window, trace) => {
    window.document.querySelector('button').remove();
    const next = window.document.createElement('button');
    next.dataset.agentId = 'A'; next.dataset.rendererTarget = ''; next.dataset.selected = 'true';
    assert.equal(trace.mutations.length, 0);
    window.document.body.append(next); await flush();
    assert.equal(trace.mutations.length, 1);
    assert.equal(trace.mutations[0].selected, true);
    assert.equal(trace.mutations[0].present, true);
    assert(trace.mutations[0].causes.includes('insert'));
    // More DOM evidence does not manufacture an exposed EventTiming identity.
    const result = matchInputPaintTrace([{ index: 0, expectedId: 'A', startTime: 0, clickTime: 0,
      endTime: window.performance.now() + 1, durationMs: 20, isTrusted: true, clickTrusted: true }], trace);
    assert.notEqual(result.rows[0].domAcceptedMs, null);
    assert.equal(result.rows[0].nextPaintEstimateMs, null);
  });
});
test('changes on a detached old node cannot revive its observed selection', async () => {
  await fixture(async (window, trace) => {
    const button = window.document.querySelector('button');
    button.remove(); button.dataset.selected = 'true'; await flush();
    assert(!trace.mutations.some(row => row.selected));
  });
});
test('coalesced true then false does not invent a visible true state', async () => {
  await fixture(async (window, trace) => {
    const button = window.document.querySelector('button');
    button.dataset.selected = 'true'; button.dataset.selected = 'false'; await flush();
    assert(!trace.mutations.some(row => row.selected));
    assert.equal(trace.domObserver.records, 2);
  });
});
test('identity replacement clears the old identity before observing the new one', async () => {
  await fixture(async (window, trace) => {
    const button = window.document.querySelector('button');
    button.dataset.selected = 'true'; await flush();
    button.dataset.agentId = 'B'; await flush();
    assert.deepEqual(plain(trace.mutations.map(row => [row.target, row.selected])), [['A', true], ['A', false], ['B', true]]);
  });
});
test('replacing a selected node in one batch preserves final DOM truth', async () => {
  await fixture(async (window, trace) => {
    const button = window.document.querySelector('button');
    button.dataset.selected = 'true'; await flush();
    const replacement = button.cloneNode(true);
    button.replaceWith(replacement); await flush();
    assert.deepEqual(plain(trace.mutations.map(row => row.selected)), [true]);
    replacement.remove(); await flush();
    assert.deepEqual(plain(trace.mutations.map(row => row.selected)), [true, false]);
    assert.equal(trace.domObserver.maxConnectedTargets, 1);
  });
});
test('removed target identities do not accumulate in the live node index', async () => {
  await fixture(async (window, trace) => {
    window.document.querySelector('button').remove(); await flush();
    for (let cycle = 0; cycle < 5; cycle++) {
      const group = window.document.createElement('div');
      group.innerHTML = [0, 1, 2].map(n => `<button data-agent-id="${cycle}-${n}" data-renderer-target data-selected="false"></button>`).join('');
      window.document.body.append(group); await flush();
      group.remove(); await flush();
    }
    assert.equal(trace.domObserver.maxConnectedTargets, 3);
    assert.equal(trace.domObserver.maxIndexedTargets, 3);
    assert.equal(trace.overflow, false);
  });
});
test('the live target index limit remains fail closed before scoring', async () => {
  await fixture(async (window, trace) => {
    const group = window.document.createElement('div');
    group.innerHTML = '<button data-agent-id="extra" data-renderer-target></button>'.repeat(20000);
    window.document.body.append(group); await flush();
    assert.equal(trace.domObserver.maxIndexedTargets, 20000);
    assert.equal(trace.overflow, true);
    assert.throws(() => matchInputPaintTrace([], trace), /overflow/);
  });
});
