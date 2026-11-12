import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import vm from 'node:vm';

const agent = readFileSync(new URL('./pixel-world-agent-visual-smoke.mjs', import.meta.url), 'utf8');
const cjk = readFileSync(new URL('./pixel-world-cjk-visual-smoke.mjs', import.meta.url), 'utf8');
const selector = id => `button[data-agent-id="${id}"][data-renderer-target]`;
const templates = source => [...source.matchAll(/`([^`]+)`/g)].map(match => match[1]);
const decode = template => vm.runInNewContext('`' + template + '`');
const plain = value => JSON.parse(JSON.stringify(value));

function helper(name) {
  const start = agent.indexOf(`async function ${name}(`);
  assert.ok(start >= 0);
  const end = agent.indexOf('\nasync function ', start + 1);
  assert.ok(end > start);
  return agent.slice(start, end);
}

test('keyboard helper focuses the other fixture target then sends real Enter before waiting', async () => {
  const calls = [];
  const context = vm.createContext({
    document: { querySelector: value => { assert.equal(value, selector('agent-1')); return { focus: options => calls.push(['focus', plain(options)]) }; } },
    run: async args => calls.push(['run', plain(args)]),
    waitFor: async expression => { calls.push(['wait', expression]); assert.equal(vm.runInNewContext(expression, { document: { querySelector: value => { assert.equal(value, selector('agent-1')); return { dataset: { selected: 'true' } }; } } }), true); },
  });
  context.evaluate = code => vm.runInContext(code, context);
  await vm.runInContext(helper('keyboardSelectOther') + '\nkeyboardSelectOther()', context);
  assert.deepEqual(calls.slice(0, 2), [['focus', { preventScroll: true }], ['run', ['press', 'Enter']]]);
  assert.equal(calls[2][0], 'wait');
});

for (const moving of [false, true]) test(`primary geometry polling ${moving ? 'times out after 40 samples' : 'requires four stable repeats'}`, async () => {
  let samples = 0;
  const delays = [];
  const context = vm.createContext({
    document: { querySelector: value => { assert.equal(value, selector('agent-0')); return { getBoundingClientRect: () => ({ x: moving ? ++samples : (samples++, 10), y: 20, width: 30, height: 40 }) }; } },
    setTimeout: (resolve, delay) => { delays.push(delay); resolve(); },
  });
  context.evaluate = code => vm.runInContext(code, context);
  const result = vm.runInContext(helper('settlePrimaryTarget') + '\nsettlePrimaryTarget()', context);
  if (moving) await assert.rejects(result, /Agent target camera did not settle/); else await result;
  assert.equal(samples, moving ? 40 : 5);
  assert.deepEqual(delays, Array(moving ? 40 : 4).fill(50));
});

test('both fixed anchor variants preserve center and CSS/backing coordinates', () => {
  const start = agent.indexOf('  const anchorPrograms = {');
  const end = agent.indexOf('\n  let anchor=', start);
  assert.ok(start >= 0 && end > start);
  const choose = targetId => vm.runInNewContext(agent.slice(start, end) + '\nanchorScript', { targetId });
  for (const [id, x] of [['agent-0', 11], ['agent-1', 31]]) {
    const queries = [];
    const canvas = { width: 2880, height: 2000, getBoundingClientRect: () => ({ width: 1440, height: 1000 }) };
    const code = choose(id);
    const result = JSON.parse(vm.runInNewContext(code, { innerWidth: 1440, scrollY: 7, document: { querySelector: value => { queries.push(value); if (value === selector(id)) return { getBoundingClientRect: () => ({ x, y: 21, width: 40, height: 60 }) }; assert.equal(value, '#pixel-world-embedded-runtime-canvas'); return canvas; } } }));
    assert.deepEqual(result, { x: x + 20, y: 51, width: 1440, scroll: 7, canvasWidth: 2880, canvasHeight: 2000, cssWidth: 1440, cssHeight: 1000 });
    assert.equal(queries[0], selector(id));
  }
  for (const id of ['__proto__', 'agent-2', `agent-0'];globalThis.injected=true;//`]) assert.throws(() => choose(id), /unknown Agent pixel target/);
});

for (const [name, source] of [['agent', agent], ['cjk', cjk]]) test(`${name} fixed focus and selection programs query exact targets`, () => {
  const fixed = templates(source).filter(text => !text.includes('${') && text.includes('document.querySelector(') && text.includes('button[data-agent-id=') && (text.startsWith('document.querySelector(')));
  assert.equal(fixed.length, name === 'agent' ? 12 : 3);
  const expectedIds = name === 'agent' ? ['agent-1', 'agent-1', 'agent-1', 'agent-0', 'agent-0', 'agent-1', 'agent-0', 'agent-0', 'agent-0', 'agent-0', 'agent-1', 'agent-1'] : ['agent-1', 'agent-0', 'agent-0'];
  for (const [index, template] of fixed.entries()) {
    const code = decode(template), queries = [], focus = [];
    const evaluate = selected => vm.runInNewContext(code, { document: { querySelector: value => { queries.push(value); return { dataset: { selected }, focus: options => focus.push(plain(options)), getAttribute: attribute => { assert.equal(attribute, 'aria-pressed'); return selected; } }; } } });
    const result = evaluate('true');
    assert.equal(queries[0], selector(expectedIds[index]));
    if (code.includes('.focus(')) assert.deepEqual(focus, [{ preventScroll: true }]);
    else { assert.equal(result, !code.includes("==='false'")); assert.equal(evaluate('false'), code.includes("==='false'")); }
  }
});

const labelStart = cjk.indexOf('const labelCases = Object.freeze({');
const captureStart = cjk.indexOf('async function captureLabel(');
const captureEnd = cjk.indexOf('\nconst glyphOracle', captureStart);
assert.ok(labelStart >= 0 && captureStart > labelStart && captureEnd > captureStart);
const cases = vm.runInNewContext(cjk.slice(labelStart, captureStart) + '\nlabelCases', { read: 'window.observation' });
const expectedLabels = { hanA: '中中中中AB123，。', hanB: '国国国国AB123，。', long: '独立观察目标一号长中文名称ABC123，。' };

for (const [key, text] of Object.entries(expectedLabels)) test(`CJK ${key} clones input and waits for its exact observed label`, async () => {
  const original = { time: 13, model: { agents: { 'agent-0': { name: 'Before', pos: { x: 2 } }, 'agent-1': { name: 'Other' } } } };
  const before = structuredClone(original), injected = [], calls = [];
  const context = vm.createContext({
    structuredClone,
    __CJK_INPUT__: original,
    __AW_TEST__: { injectSnapshot: snapshot => injected.push(snapshot) },
    observation: { dto: { agents: [{ id: 'agent-0', label: text }] } },
    out: '/artifacts', resolve: (...parts) => parts.join('/'),
    state: async () => ({ ready: true }),
    run: async args => calls.push(plain(args)),
  });
  context.window = context;
  context.evaluate = code => { calls.push('update'); return vm.runInContext(code, context); };
  context.waitFor = expression => { calls.push('observed'); assert.equal(vm.runInContext(expression, context), true); };
  context.selectedCase = cases[key];
  const result = await vm.runInContext(cjk.slice(captureStart, captureEnd) + `\ncaptureLabel('desktop', 'cold', '${key}', selectedCase)`, context);
  assert.deepEqual(original, before);
  assert.equal(injected.length, 1);
  assert.notEqual(injected[0], original);
  assert.notEqual(injected[0].model.agents['agent-0'].pos, original.model.agents['agent-0'].pos);
  assert.equal(injected[0], context.__CJK_INPUT__);
  const expected = structuredClone(before); expected.model.agents['agent-0'].name = text;
  assert.deepEqual(plain(injected[0]), expected);
  assert.deepEqual(plain(result), { text, shot: `/artifacts/desktop-cold-${key}.png`, state: { ready: true } });
  assert.deepEqual(calls, ['update', 'observed', ['wait', '1200'], ['screenshot', '--full', result.shot]]);
  context.observation.dto.agents[0].label = text + 'different';
  assert.equal(vm.runInContext(cases[key].observed, context), false);
  context.observation.dto.agents = [{ id: 'agent-1', label: text }];
  assert.equal(vm.runInContext(cases[key].observed, context), false);
});

test('CJK same-label negative control reuses the frozen hanA case and distinct filename', async () => {
  assert.ok(Object.isFrozen(cases));
  for (const value of Object.values(cases)) assert.ok(Object.isFrozen(value));
  const start = cjk.indexOf('async function glyphPair('), end = cjk.indexOf('\n  const diff =', start);
  assert.ok(start >= 0 && end > start);
  const captures = [];
  const context = vm.createContext({ labelCases: cases, waitForDrawable: async () => [], captureLabel: async (...args) => { captures.push(args); return {}; } });
  await vm.runInContext(cjk.slice(start, end) + '\n}\nglyphPair("desktop", "cold")', context);
  assert.deepEqual(captures.map(args => args[2]), ['hanA', 'hanA-repeat', 'hanB', 'long']);
  assert.equal(captures[0][3], cases.hanA);
  assert.equal(captures[1][3], captures[0][3]);
  assert.equal(captures[2][3], cases.hanB);
  assert.equal(captures[3][3], cases.long);
});
