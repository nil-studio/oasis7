import test from 'node:test';
import assert from 'node:assert/strict';
import vm from 'node:vm';
import { pendingStatusLayoutCases } from './pending-status-layout-programs.mjs';

const plain = value => JSON.parse(JSON.stringify(value));
const expectedCases = [
  { feedOpen: false, status: 'pending', text: 'Checking the selected object…' },
  { feedOpen: false, status: 'unavailable', text: 'This object could not be confirmed. Select again.' },
  { feedOpen: true, status: 'pending', text: 'Checking the selected object…' },
  { feedOpen: true, status: 'unavailable', text: 'This object could not be confirmed. Select again.' },
];
function harness({ authReady = false, missingChip = false, pointerEvents = 'none' } = {}) {
  const trace = [], listeners = [];
  const textRect = { x: 11, y: 12, left: 11, top: 12, right: 101, bottom: 32, width: 90, height: 20 };
  const statusRect = { x: 10, y: 10, width: 100, height: 24 };
  const chipRect = { x: 8, y: 8, width: 120, height: 40 };
  const overlayRect = { x: 0, y: 80, width: 300, height: 100 };
  const measured = value => ({ toJSON: () => ({ ...value }) });
  let element, selected;
  const chip = {
    children: [],
    append: child => { trace.push('append'); chip.children.push(child); },
    getBoundingClientRect: () => { trace.push('chipRect'); return measured(chipRect); },
    get textContent() { return 'Selected: Agent A' + chip.children.map(child => child.textContent).join(''); },
  };
  const overlay = { dataset: { viewerOverlay: 'feed' }, getBoundingClientRect: () => { trace.push('overlayRect'); return measured(overlayRect); } };
  const context = vm.createContext({
    innerWidth: 390, innerHeight: 844, devicePixelRatio: 2,
    document: {
      querySelector: selector => { assert.equal(selector, '.pixel-world-canvas__selection'); trace.push('queryChip'); return missingChip ? null : chip; },
      createElement: tag => {
        assert.equal(tag, 'div'); trace.push('createStatus');
        element = {
          dataset: {}, attributes: {},
          setAttribute: (key, value) => { trace.push('attribute:' + key); element.attributes[key] = value; },
          getBoundingClientRect: () => { trace.push('statusRect'); return measured(statusRect); },
        };
        return element;
      },
      createRange: () => {
        trace.push('createRange');
        return {
          selectNodeContents: node => { selected = node; trace.push('selectContents'); },
          getBoundingClientRect: () => { assert.equal(selected, element); trace.push('textRect'); return measured(textRect); },
        };
      },
      addEventListener: (type, listener, options) => { trace.push('listener'); listeners.push({ type, listener, options: plain(options) }); },
      querySelectorAll: selector => { assert.equal(selector, '[data-viewer-overlay]'); trace.push('queryOverlays'); return [overlay]; },
    },
    getComputedStyle: node => {
      trace.push(node === element ? 'statusStyle' : node === chip ? 'chipStyle' : 'overlayStyle');
      return node === overlay ? { zIndex: '7' } : { pointerEvents };
    },
    __AW_TEST__: { getState: () => { trace.push('authState'); return { authReady }; } },
  });
  context.window = context;
  return { context, trace, listeners, chip, get element() { return element; }, textRect, statusRect, chipRect, overlayRect, execute: program => vm.runInContext(program, context) };
}

test('the immutable case table preserves both nested-loop order and literal metadata', () => {
  assert.deepEqual(pendingStatusLayoutCases.map(({ feedOpen, status }) => ({ feedOpen, status })), expectedCases.map(({ feedOpen, status }) => ({ feedOpen, status })));
  assert.ok(Object.isFrozen(pendingStatusLayoutCases));
  for (const entry of pendingStatusLayoutCases) {
    assert.ok(Object.isFrozen(entry));
    assert.equal(typeof entry.program, 'string');
    assert.throws(() => { entry.status = '\"; throw Error(\"injected\")'; }, TypeError);
  }
  assert.throws(() => pendingStatusLayoutCases.push({}), TypeError);
});

for (const [index, expected] of expectedCases.entries()) {
  test(`${expected.status}, feed ${expected.feedOpen}: DOM copy, measurement and event contract`, () => {
    const h = harness();
    const row = plain(h.execute(pendingStatusLayoutCases[index].program));
    assert.equal(h.element.className, 'pixel-world-selection-status');
    assert.deepEqual(h.element.attributes, { role: 'status', 'aria-live': 'polite' });
    assert.deepEqual(h.element.dataset, { worldSelectionStatus: expected.status });
    assert.equal(h.element.textContent, expected.text);
    assert.equal(h.chip.children.length, 1);
    assert.equal(h.chip.children[0], h.element);
    assert.deepEqual(row, {
      viewport: { width: 390, height: 844 }, dpr: 2, status: expected.status, feedOpen: expected.feedOpen,
      textRect: h.textRect, rect: h.statusRect, chip: h.chipRect,
      pointer: ['none', 'none'], text: 'Selected: Agent A' + expected.text,
      overlays: [{ kind: 'feed', rect: h.overlayRect, z: '7' }], auth: false,
    });
    assert.deepEqual(h.trace, ['queryChip', 'createStatus', 'attribute:role', 'attribute:aria-live', 'append', 'createRange', 'selectContents', 'textRect', 'statusRect', 'listener', 'chipRect', 'statusStyle', 'chipStyle', 'queryOverlays', 'overlayRect', 'overlayStyle', 'authState']);
    assert.equal(h.listeners.length, 1);
    const { type, listener, options } = h.listeners[0];
    assert.equal(type, 'pointerdown');
    assert.deepEqual(options, { once: true, capture: true });
    assert.deepEqual(plain(h.context.__LAYOUT_INPUT__), []);
    listener({ isTrusted: false, target: { tagName: 'DIV', id: 'unexpected' } });
    assert.deepEqual(plain(h.context.__LAYOUT_INPUT__), [{ trusted: false, target: 'DIV', id: 'unexpected' }]);
  });
}

test('actual auth and pointer styles remain observable instead of being forced to passing values', () => {
  const h = harness({ authReady: true, pointerEvents: 'auto' });
  const row = plain(h.execute(pendingStatusLayoutCases[0].program));
  assert.equal(row.auth, true);
  assert.deepEqual(row.pointer, ['auto', 'auto']);
});

test('missing selection chip still fails before measurement or listener registration', () => {
  const h = harness({ missingChip: true });
  assert.throws(() => h.execute(pendingStatusLayoutCases[0].program), /append/);
  assert.equal(h.listeners.length, 0);
  assert.equal(h.trace.includes('createRange'), false);
});
