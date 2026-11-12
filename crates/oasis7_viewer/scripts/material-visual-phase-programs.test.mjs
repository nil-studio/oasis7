import assert from 'node:assert/strict';
import { test } from 'node:test';
import vm from 'node:vm';
import { factoryPhaseProgram, terrainPhaseProgram, factoryFocusProgram, factorySelectedExpression } from './material-visual-phase-programs.mjs';

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
