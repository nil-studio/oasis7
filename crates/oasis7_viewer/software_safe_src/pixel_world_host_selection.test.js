import { createRoot, createSignal } from 'solid-js';
import { expect, it } from 'vitest';
import { createPixelWorldHostSelection } from './pixel_world_host_selection.js';
import { createPixelWorldSpatialController } from './pixel_world_spatial_controller.js';

it('restores reactive selection after scope rejection without relying on another core revision', async () => {
  let dispose, spatial, selected, changeRevision, revision;
  let entities = [{kind:'agent',entity_id:'A'}, {kind:'agent',entity_id:'B'}];
  const core = { state: {worldId:'W',auth:{sessionEpoch:1},selectedKind:null,selectedId:null,
    viewerProtocol:{capabilities:['world_view']}}, buildGameplaySummary:()=>null,
    requestWorldView:async q=>({world_id:'W',scope_id:'S',source_cut:'C',request_generation:q.request_generation,
      completeness:'complete',entities,relations:[]}) };
  createRoot(cleanup => {
    dispose = cleanup;
    [revision, changeRevision] = createSignal(0);
    spatial = createPixelWorldSpatialController({core,enabled:()=>true,getRenderState:()=>null,getLocale:()=> 'en'});
    selected = createPixelWorldHostSelection(revision, spatial);
  });
  try {
    await spatial.query({}); spatial.select({kind:'agent',id:'A'});
    expect(selected()).toEqual({kind:'agent',id:'A'});
    core.state.auth.sessionEpoch = 2; changeRevision(1);
    expect(selected()).toBeNull();
    spatial.resetScope(); await spatial.query({});
    spatial.select({kind:'agent',id:'B'});
    expect(revision()).toBe(1);
    expect(selected()).toEqual({kind:'agent',id:'B'});
    entities = []; await spatial.query({});
    expect(selected()).toBeNull();
  } finally { dispose(); }
});
