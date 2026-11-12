import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import vm from 'node:vm';

// Execute the programs the runner actually sends, rather than copied test programs.
const source = readFileSync(new URL('./world-view-pending-selection-probe.mjs', import.meta.url), 'utf8');
const programs = [...source.matchAll(/evaluate\(`([^`]+)`\)/g)].map(match => match[1]);
function program(marker) {
  const matches = programs.filter(text => text.includes(marker));
  assert.equal(matches.length, 1, `unique evaluated program: ${marker}`);
  assert.ok(!matches[0].includes('${'), 'fixed-target program must not interpolate data');
  return vm.runInNewContext('`' + matches[0] + '`');
}
const target = 'button[data-agent-id="qa-agent-0500"][data-renderer-target]';
const replacement = 'button[data-agent-id="qa-agent-0001"][data-renderer-target]';
const pending = '[data-world-selection-status=pending]';
function harness(nodes = new Map()) {
  let now = 0;
  const queries = [], listeners = new Map();
  const metrics = { status: 'observed', projected_marker_count: 1000, agent_body_sprites: 1000, in_view_agent_body_sprites: 1000 };
  const context = vm.createContext({
    Date: { now: () => now },
    setTimeout: resolve => { now += 50; resolve(); },
    document: {
      querySelector: selector => { queries.push(selector); return nodes.get(selector) ?? null; },
      addEventListener: (type, listener, capture) => { assert.equal(capture, true); listeners.set(type, listener); },
      removeEventListener: (type, listener, capture) => { assert.equal(capture, true); assert.equal(listeners.get(type), listener); listeners.delete(type); },
    },
    __QA_READ_METRICS__: () => metrics,
    __AW_TEST__: { getState: () => ({ authReady: false, selectedId: null }) },
  });
  context.window = context;
  return { context, nodes, queries, listeners, metrics, execute: marker => vm.runInContext(program(marker), context) };
}
const plain = value => JSON.parse(JSON.stringify(value));

test('initial observation requires the exact old target and full observed scene', async () => {
  const h = harness(new Map([[target, {}]]));
  const result = JSON.parse(await h.execute('initial full observed scene did not settle'));
  assert.equal(result.targetPresent, true);
  assert.deepEqual(result.core, { authReady: false, selectedId: null });
  assert.deepEqual(h.queries, [target]);
  h.nodes.delete(target);
  await assert.rejects(h.execute('initial full observed scene did not settle'), /initial full observed scene did not settle/);
  h.nodes.set(target, {}); h.metrics.in_view_agent_body_sprites = 999;
  await assert.rejects(h.execute('initial full observed scene did not settle'), /initial full observed scene did not settle/);
});

test('focus and native event recording preserve target identity, trust and disposal', () => {
  const focus = [];
  const h = harness(new Map([[target, { focus: options => focus.push(plain(options)), getBoundingClientRect: () => ({ x: 10, y: 20, width: 40, height: 60 }) }], ['.pixel-world-exploration [role=status]', { textContent: 'Partial results' }]]));
  assert.deepEqual(plain(h.execute('old presented0500 target missing before first partial')), { x: 30, y: 50 });
  assert.deepEqual(focus, [{ preventScroll: true }]);
  assert.deepEqual([...h.listeners.keys()], ['keydown', 'click', 'pointerdown']);
  const event = (type, key, trusted = true) => ({ type, key, isTrusted: trusted, timeStamp: 123, target: { closest: selector => { assert.equal(selector, '[data-agent-id]'); return { dataset: { agentId: 'qa-agent-0500' } }; } } });
  h.listeners.get('keydown')(event('keydown', 'Escape'));
  assert.equal(h.context.__QA_PENDING_NATIVE__.length, 0);
  for (const type of ['keydown', 'click', 'pointerdown']) h.listeners.get(type)(event(type, 'Enter', type !== 'pointerdown'));
  const recorded = plain(h.context.__QA_PENDING_NATIVE__);
  assert.deepEqual(recorded.map(e => [e.type, e.trusted, e.target, e.oldDomPresent]), [
    ['keydown', true, 'qa-agent-0500', true], ['click', true, 'qa-agent-0500', true], ['pointerdown', false, 'qa-agent-0500', true],
  ]);
  assert.ok(recorded.every(e => e.explorer === 'Partial results' && e.time === 123));
  h.nodes.delete(target); h.listeners.get('click')(event('click'));
  assert.equal(h.context.__QA_PENDING_NATIVE__.at(-1).oldDomPresent, false);
  h.context.__QA_PENDING_DISPOSE__(); assert.equal(h.listeners.size, 0);
  assert.throws(() => h.execute('old presented0500 target missing before first partial'), /old presented0500 target missing/);
});

test('feedback reports actual pending geometry and the old target selection', () => {
  const rect = { toJSON: () => ({ x: 1, y: 2, width: 30, height: 40 }) };
  const h = harness(new Map([[target, { dataset: { selected: 'false' } }], [pending, { textContent: 'Checking', getBoundingClientRect: () => rect }], ['#pixel-world-embedded-runtime-canvas', { getBoundingClientRect: () => rect }]]));
  assert.deepEqual(plain(h.execute('selected0500:')), { text: 'Checking', rect: rect.toJSON(), selected0500: 'false', canvas: rect.toJSON() });
  assert.ok(h.queries.includes(target));
});

for (const [marker, selector, wrong] of [
  ['same-cut later page did not confirm pending target', target, replacement],
  ['new explicit selection did not cancel pending', replacement, target],
]) test(marker, async () => {
  const h = harness(new Map([[selector, { dataset: { selected: 'true' } }]]));
  assert.equal(await h.execute(marker), true);
  assert.equal(h.queries[0], selector);
  h.nodes.set(pending, {});
  await assert.rejects(h.execute(marker), new RegExp(marker));
  h.nodes.delete(pending); h.nodes.delete(selector); h.nodes.set(wrong, { dataset: { selected: 'true' } });
  await assert.rejects(h.execute(marker), new RegExp(marker));
});
