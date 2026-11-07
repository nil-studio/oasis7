import { createRoot } from 'solid-js';
import { afterEach, expect, it, vi } from 'vitest';
import { createPixelWorldSpatialController } from './pixel_world_spatial_controller.js';

// Isolate delivery of a validated delta removal while a baseline page is held.
// Page/cut validation itself is exercised by pending_selection.test's real client.
const delivery = vi.hoisted(()=>({publish:null}));
vi.mock('./pixel_world_world_view.js',async importOriginal=>({
  ...await importOriginal(),
  createWorldViewClient:({onChange})=>{
    delivery.publish=onChange;
    return {reset(){onChange({view:null,loading:false});},clearChanges(){},invalidate(){}};
  },
}));
let dispose;
afterEach(()=>dispose?.());
for(const beforeClick of [true,false]) it(`rejects explicit deletion ${beforeClick?'before':'after'} pending admission`,()=>{
  const core={state:{worldId:'W',auth:{sessionEpoch:1},viewerProtocol:{capabilities:['world_view']}},buildGameplaySummary:()=>null};
  let controller;createRoot(cleanup=>{dispose=cleanup;controller=createPixelWorldSpatialController({core,enabled:()=>true,getLocale:()=> 'en',getRenderState:()=>null});});
  const full={world_id:'W',scope_id:'S',source_cut:'C',request_generation:'1',completeness:'complete',entities:[{kind:'factory',entity_id:'F'}]};
  delivery.publish({view:full,loading:false,changes:[]});
  controller.present(full,{display_source_cut:'C',module_visual_entities:[{id:'F'}]});
  const partial={...full,request_generation:'2',completeness:'partial',entities:[]};
  const deletion={record_kind:'entity',kind:'factory',id:'F',world_deleted:true,reason:'factory_recycled'};
  delivery.publish({view:partial,loading:true,changes:beforeClick?[deletion]:[]});
  controller.selectPresented({kind:'module_visual',id:'F'});
  if(!beforeClick){expect(controller.selectionStatus()).toBe('pending');delivery.publish({view:partial,loading:true,changes:[deletion]});}
  expect(controller.selectionStatus()).not.toBe('pending');
  delivery.publish({view:{...partial,entities:full.entities},loading:true,changes:[]});
  expect(controller.selection()).toBeNull();
});
