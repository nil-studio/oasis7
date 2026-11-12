import assert from 'node:assert/strict';
import { test } from 'node:test';
import vm from 'node:vm';
import { readFileSync } from 'node:fs';
import { factoryPhaseProgram, terrainPhaseProgram, factoryFocusProgram, factorySelectedExpression, runtimeBaselineProgram, runtimePhaseProgram, assertRuntimeInitialInput } from './material-visual-phase-programs.mjs';

const phases = ['unknown', 'restored', 'deleted', 'recreated'];
const factoryEntity = marker => ({
  id: 'material-smelter', factory_kind: 'factory.smelter.mk1', marker,
  anchor: { data: { pos: { x_cm: 7, y_cm: 9, z_cm: 11 } } },
});

test('factory phases preserve object identity, exact unrelated data and fresh recreation', () => {
  const originalEntity = factoryEntity('initial');
  const snapshot = { time: 40, model: { module_visual_entities: {
    'material-smelter': originalEntity, untouched: { state: 'retained' },
  } }, binding: { player: 'fixture-player' } };
  const expected = structuredClone(snapshot);
  const injected = [];
  let factoryCalls = 0;
  let recreatedEntity;
  const context = { window: {
    __MATERIAL_SNAPSHOT__: snapshot,
    __AW_TEST__: { injectSnapshot(value) { injected.push(value); } },
    __OASIS7_PIXEL_WORLD_VISUAL_FIXTURES__: { get(name) {
      assert.equal(name, 'material_shells');
      return () => {
        factoryCalls += 1;
        recreatedEntity = factoryEntity('fresh');
        return { model: { module_visual_entities: { 'material-smelter': recreatedEntity } } };
      };
    } },
  } };
  for (const [index, phase] of phases.entries()) {
    assert.equal(vm.runInNewContext(factoryPhaseProgram(phase), context), true);
    expected.time = 41 + index;
    if (phase === 'deleted') delete expected.model.module_visual_entities['material-smelter'];
    else if (phase === 'recreated') {
      expected.model.module_visual_entities['material-smelter'] = factoryEntity('fresh');
      expected.model.module_visual_entities['material-smelter'].anchor.data.pos.y_cm = 3000000;
    } else expected.model.module_visual_entities['material-smelter'].factory_kind = phase === 'unknown' ? 'unsupported' : 'factory.smelter.mk1';
    assert.deepEqual(snapshot, expected);
    assert.equal(injected.length, index + 1);
    assert.equal(injected[index], snapshot, 'factory phases must inject the same evolving snapshot');
    assert.equal(factoryCalls, phase === 'recreated' ? 1 : 0);
  }
  assert.notEqual(recreatedEntity, originalEntity);
  assert.equal(snapshot.model.module_visual_entities['material-smelter'], recreatedEntity);
  assert.equal(originalEntity.anchor.data.pos.y_cm, 9, 'recreation must not reuse the old entity');
});

test('terrain phases clone the base independently and preserve exact time offsets and fields', () => {
  const base = { time: 70, model: { locations: {
    'loc-0': { pos: { x_cm: 3, y_cm: 4 }, fragment_profile: { retained: true, blocks: { blocks: [
      { id: 'a', compounds: { ppm: { iron: 50 }, other: 1 } },
      { id: 'b', compounds: { ppm: { copper: 20 }, other: 2 } },
    ] } } },
    elsewhere: { name: 'retained' },
  } }, metadata: { id: 'unchanged' } };
  const original = structuredClone(base);
  const injected = [];
  const context = { structuredClone, window: {
    __TERRAIN_BASE__: base,
    __AW_TEST__: { injectSnapshot(value) { injected.push(value); } },
  } };
  for (const [index, phase] of phases.entries()) {
    assert.equal(vm.runInNewContext(terrainPhaseProgram(phase), context), true);
    const expected = structuredClone(original);
    expected.time = 71 + index;
    if (phase === 'unknown') expected.model.locations['loc-0'].fragment_profile.blocks.blocks.forEach(b => { b.compounds.ppm = {}; });
    if (phase === 'deleted') delete expected.model.locations['loc-0'].fragment_profile;
    assert.equal(injected.length, index + 1);
    assert.deepEqual(structuredClone(injected[index]), expected);
    assert.notEqual(injected[index], base);
    assert.deepEqual(base, original, 'no phase may mutate the baseline');
  }
  assert.equal(new Set(injected).size, 4, 'every phase must have an independent clone');
});

test('phase selectors reject unexpected and code-shaped values before producing a program', () => {
  for (const select of [factoryPhaseProgram, terrainPhaseProgram]) {
    for (const phase of ['', 'initial', '__proto__', 'constructor', 'unknown\');globalThis.compromised=true;//', null, undefined, 1, {}]) {
      assert.throws(() => select(phase), /unsupported material visual phase/);
    }
    for (const phase of phases) assert.equal(typeof select(phase), 'string');
  }
});

test('missing fixture data fails before injection instead of silently passing a phase', () => {
  let injections = 0;
  const snapshot = { time: 4, model: { module_visual_entities: {} } };
  const context = { structuredClone, window: {
    __MATERIAL_SNAPSHOT__: snapshot,
    __TERRAIN_BASE__: { time: 7, model: { locations: { 'loc-0': {} } } },
    __AW_TEST__: { injectSnapshot() { injections += 1; } },
  } };
  assert.throws(() => vm.runInNewContext(factoryPhaseProgram('unknown'), context), /factory_kind/);
  assert.throws(() => vm.runInNewContext(terrainPhaseProgram('unknown'), context), /blocks/);
  assert.equal(injections, 0);
  assert.equal(snapshot.time, 4);
});

test('static keyboard probes retain the exact selector, focus options and selected-string check', () => {
  let focusCalls = 0;
  let queries = 0;
  const button = { dataset: { selected: 'false' }, focus(options) {
    focusCalls += 1;
    assert.equal(options.preventScroll, true);
    assert.deepEqual(Object.keys(options), ['preventScroll']);
  } };
  const context = { document: { querySelector(selector) {
    queries += 1;
    assert.equal(selector, 'button[data-module-id="material-smelter"][data-renderer-target]');
    return button;
  } } };
  assert.equal(vm.runInNewContext(factoryFocusProgram, context), true);
  assert.equal(vm.runInNewContext(factorySelectedExpression, context), false);
  button.dataset.selected = 'true';
  assert.equal(vm.runInNewContext(factorySelectedExpression, context), true);
  button.dataset.selected = true;
  assert.equal(vm.runInNewContext(factorySelectedExpression, context), false);
  assert.equal(focusCalls, 1);
  assert.equal(queries, 4);
});

const runtimePhases = ['initial', 'absent', 'restored', 'deleted', 'recreated'];
const runtimeDto = material => ({
  world_tick: 47,
  time: 19,
  fragment_terrain: material === 'ore' ? [{ id: 'runtime-ore', dominant_compound: 'silicate_matrix', pos: { x_cm: 7 }, color: [255, 255, 255], emphasis: 1 }] : [],
  micro_depot_facilities: material === 'depot' ? [{ id: 'runtime-depot', available_units_by_kind: { iron: 100 }, pos: { x_cm: 7 } }] : [],
  navigation: { request_id: 'material-isolated', mode: 'overview', target: null },
  metadata: { unchanged: ['keep', { nested: true }] },
});
const runnerSource = readFileSync(new URL('./agent-browser-material-visual-test.mjs', import.meta.url), 'utf8');
const runtimeHtmlTemplate = runnerSource.match(/const runtimeHtml = (`[^`]+`);/)[1];
const runtimeHtml = vm.runInNewContext(runtimeHtmlTemplate, { runtimeBaselineProgram });
const runtimeScript = runtimeHtml.match(/<script type="module">([\s\S]*?)<\/script>/)[1]
  .replace("import{createPixelWorldBridge}from'/pixel-world-bridge/webgl2/pixel_world_bridge.js';", '');
async function mountRuntime(original, { mutateMount = () => {}, mutateUpdate = () => {} } = {}) {
  const mounted = [], updated = [];
  const canvas = {};
  const bridge = {
    mount(element, input) { assert.equal(element, canvas); mounted.push(input); mutateMount(input); return { status: 'ready' }; },
    update(input) { updated.push({ input, beforeMutation: structuredClone(input) }); mutateUpdate(input); },
  };
  const context = vm.createContext({
    structuredClone,
    fetch: async path => { assert.equal(path, '/material-runtime.json'); return { json: async () => JSON.parse(JSON.stringify(original)) }; },
    createPixelWorldBridge: async () => bridge,
    document: { querySelector: selector => { assert.equal(selector, 'canvas'); return canvas; } },
  });
  context.window = context;
  await vm.runInContext(`(async () => {${runtimeScript}})()`, context);
  return { context, mounted, updated };
}

for (const material of ['ore', 'depot']) test(`runtime ${material} preserves all five phase inputs, tick and update counts`, async () => {
  const original = runtimeDto(material);
  const { context, mounted, updated } = await mountRuntime(original);
  assert.equal(mounted.length, 1);
  const initial = context.s;
  for (const [index, phase] of runtimePhases.entries()) {
    const program = runtimePhaseProgram(material, phase);
    if (program !== null) assert.equal(vm.runInContext(program, context), true);
    const expected = structuredClone(original);
    if (phase === 'absent') {
      if (material === 'ore') expected.fragment_terrain[0].dominant_compound = 'unknown';
      else expected.micro_depot_facilities = [];
    }
    if (phase === 'deleted') { expected.fragment_terrain = []; expected.micro_depot_facilities = []; }
    assert.deepEqual(structuredClone(context.s), expected);
    assert.equal(updated.length, index);
    if (phase === 'initial') {
      assert.equal(program, null);
      assert.equal(context.s, initial);
      assert.equal(context.s, mounted[0]);
      assertRuntimeInitialInput(context.s, original);
    } else {
      assert.notEqual(context.s, initial);
      assert.equal(updated.at(-1).input, context.s);
    }
  }
  assert.equal(new Set(updated.map(row => row.input)).size, 4);
  assert.deepEqual(original, runtimeDto(material));
});

test('runtime private baseline survives mount, update and returned-clone mutations', async () => {
  const original = runtimeDto('ore');
  const { context, updated } = await mountRuntime(original, {
    mutateMount: input => { input.metadata.unchanged[1].nested = 'mount mutation'; input.world_tick = 99; },
    mutateUpdate: input => { input.metadata.unchanged[1].nested = 'update mutation'; input.world_tick = 100; },
  });
  assert.throws(() => assertRuntimeInitialInput(context.s, original), /initial input differs/);
  const copy = context.__MATERIAL_RUNTIME_INPUT__();
  copy.metadata.unchanged.length = 0; copy.fragment_terrain = [];
  for (const phase of ['restored', 'recreated', 'restored']) {
    vm.runInContext(runtimePhaseProgram('ore', phase), context);
    assert.deepEqual(updated.at(-1).beforeMutation, original);
    assert.notEqual(updated.at(-1).input, copy);
  }
  assert.equal(new Set(updated.map(row => row.input)).size, 3);
  assert.deepEqual(context.__MATERIAL_RUNTIME_INPUT__(), original);
});

test('runtime rejects unknown phase or material and fails without a baseline', () => {
  for (const material of ['', '__proto__', 'constructor', 'ore\');globalThis.compromised=true;//', null, undefined, 1, {}]) {
    for (const phase of runtimePhases) assert.throws(() => runtimePhaseProgram(material, phase), /unsupported material runtime fixture/);
  }
  for (const phase of ['', 'unknown', '__proto__', 'constructor', 'restored\');globalThis.compromised=true;//', null, undefined, 1, {}]) {
    for (const material of ['ore', 'depot']) assert.throws(() => runtimePhaseProgram(material, phase), /unsupported material visual phase/);
  }
  let updates = 0;
  const context = vm.createContext({ window: { bridge: { update() { updates++; } } } });
  assert.throws(() => vm.runInContext(runtimePhaseProgram('ore', 'restored'), context), /__MATERIAL_RUNTIME_INPUT__/);
  assert.equal(updates, 0);
});

test('initial guard compares the requested JSON representation and rejects stale HTTP data', async () => {
  const original = { ...runtimeDto('depot'), omitted: undefined, nonFinite: NaN };
  const { context } = await mountRuntime(original);
  assertRuntimeInitialInput(context.s, original);
  const stale = structuredClone(context.s); stale.world_tick--;
  assert.throws(() => assertRuntimeInitialInput(stale, original), /initial input differs/);
  const wrong = structuredClone(context.s); wrong.micro_depot_facilities[0].id = 'wrong-depot';
  assert.throws(() => assertRuntimeInitialInput(wrong, original), /initial input differs/);
  // Guard the actual runner placement: the first capture must be checked before
  // its screenshot or any later phase can hide a stale HTTP response.
  const capture = runnerSource.indexOf('row.phases[phase]=JSON.parse');
  const guard = runnerSource.indexOf("if(phase==='initial') assertRuntimeInitialInput(row.phases.initial.input,original);");
  const screenshot = runnerSource.indexOf("await run(['screenshot'", capture);
  assert.ok(capture >= 0 && guard > capture && screenshot > guard);
});

test('runtime JSON strings and own __proto__ remain data rather than browser source', async () => {
  const original = runtimeDto('ore');
  original.metadata.payload = `quotes ' " \\ newline\n</script><script>globalThis.compromised=true</script>\u2028\u2029`;
  Object.defineProperty(original.metadata, '__proto__', { value: { compromised: true }, enumerable: true, writable: true, configurable: true });
  const wire = JSON.parse(JSON.stringify(original));
  const { context, updated } = await mountRuntime(original);
  assertRuntimeInitialInput(context.s, original);
  for (const phase of runtimePhases) {
    const program = runtimePhaseProgram('ore', phase);
    if (program !== null) vm.runInContext(program, context);
    assert.deepEqual(context.s.metadata, wire.metadata);
    assert.equal(Object.hasOwn(context.s.metadata, '__proto__'), true);
    assert.equal(context.s.metadata.compromised, undefined);
    assert.equal(context.compromised, undefined);
  }
  assert.equal(updated.length, 4);
  assert.ok(!runtimeHtml.includes(original.metadata.payload));
});
