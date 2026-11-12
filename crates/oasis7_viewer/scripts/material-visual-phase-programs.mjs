// Complete, static programs for the gated material QA fixture. Phase names
// select an operation in Node; they are never interpolated into browser code.
const factoryPrograms = Object.freeze({
  unknown: `(() => {
    const s = window.__MATERIAL_SNAPSHOT__;
    s.model.module_visual_entities['material-smelter'].factory_kind = 'unsupported';
    s.time++;
    window.__AW_TEST__.injectSnapshot(s);
    return true;
  })()`,
  restored: `(() => {
    const s = window.__MATERIAL_SNAPSHOT__;
    s.model.module_visual_entities['material-smelter'].factory_kind = 'factory.smelter.mk1';
    s.time++;
    window.__AW_TEST__.injectSnapshot(s);
    return true;
  })()`,
  deleted: `(() => {
    const s = window.__MATERIAL_SNAPSHOT__;
    delete s.model.module_visual_entities['material-smelter'];
    s.time++;
    window.__AW_TEST__.injectSnapshot(s);
    return true;
  })()`,
  recreated: `(() => {
    const s = window.__MATERIAL_SNAPSHOT__;
    s.model.module_visual_entities['material-smelter'] = window.__OASIS7_PIXEL_WORLD_VISUAL_FIXTURES__.get('material_shells')().model.module_visual_entities['material-smelter'];
    s.model.module_visual_entities['material-smelter'].anchor.data.pos.y_cm = 3000000;
    s.time++;
    window.__AW_TEST__.injectSnapshot(s);
    return true;
  })()`,
});

const terrainPrograms = Object.freeze({
  unknown: `(() => {
    const s = structuredClone(window.__TERRAIN_BASE__);
    s.model.locations['loc-0'].fragment_profile.blocks.blocks.forEach(b => b.compounds.ppm = {});
    s.time += 1;
    window.__AW_TEST__.injectSnapshot(s);
    return true;
  })()`,
  restored: `(() => {
    const s = structuredClone(window.__TERRAIN_BASE__);
    s.time += 2;
    window.__AW_TEST__.injectSnapshot(s);
    return true;
  })()`,
  deleted: `(() => {
    const s = structuredClone(window.__TERRAIN_BASE__);
    delete s.model.locations['loc-0'].fragment_profile;
    s.time += 3;
    window.__AW_TEST__.injectSnapshot(s);
    return true;
  })()`,
  recreated: `(() => {
    const s = structuredClone(window.__TERRAIN_BASE__);
    s.time += 4;
    window.__AW_TEST__.injectSnapshot(s);
    return true;
  })()`,
});

function selectProgram(programs, phase) {
  if (typeof phase !== 'string' || !Object.hasOwn(programs, phase)) {
    throw new Error('unsupported material visual phase');
  }
  return programs[phase];
}

export const factoryPhaseProgram = phase => selectProgram(factoryPrograms, phase);
export const terrainPhaseProgram = phase => selectProgram(terrainPrograms, phase);

export const factoryFocusProgram = `document.querySelector('button[data-module-id="material-smelter"][data-renderer-target]').focus({preventScroll:true});true`;
export const factorySelectedExpression = `document.querySelector('button[data-module-id="material-smelter"][data-renderer-target]').dataset.selected==='true'`;
