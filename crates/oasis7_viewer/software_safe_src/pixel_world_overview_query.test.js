import {createRoot} from 'solid-js';
import {afterEach,expect,it,vi} from 'vitest';
import {createPixelWorldSpatialController} from './pixel_world_spatial_controller.js';
let dispose;
afterEach(()=>{dispose?.();vi.useRealTimers();});
const entity=id=>({kind:'agent',entity_id:id,position:{x_cm:100,y_cm:100,z_cm:0},
  provenance:{geometry_source:'authoritative',freshness:'current',valid_at_source_cut:'C'}});
const page=(q,ids=['A'],extra={})=>({world_id:'W',scope_id:'S',source_cut:'C',request_generation:q.request_generation,
  entities:ids.map(entity),relations:[],completeness:'complete',...extra});
function fixture(request=async q=>page(q),enabled=true) {
  const core={state:{worldId:'W',auth:{sessionEpoch:1},selectedKind:null,selectedId:null,
    viewerProtocol:{capabilities:['world_view']}},requestWorldView:vi.fn(request),applySelection:vi.fn(),buildGameplaySummary:()=>null};
  let controller;createRoot(cleanup=>{dispose=cleanup;controller=createPixelWorldSpatialController({core,enabled:()=>enabled,getRenderState:()=>null,getLocale:()=> 'en',debounceMs:1});});
  return {core,controller};
}
const applied=controller=>controller.navigationResult({request_id:controller.navigation().request_id,status:'applied',render_version:3});

it('overviews a complete current query without clearing search or making another query',async()=>{
  const {core,controller}=fixture();await controller.query({search:'mine'});const calls=core.requestWorldView.mock.calls.length;
  controller.overview();const nav=controller.navigation();
  expect(nav).toMatchObject({mode:'overview',overview_query:{version:1,binding:expect.any(Array)}});
  expect(controller.overviewStatus()).toBe('applying');expect(core.requestWorldView).toHaveBeenCalledTimes(calls);
  controller.navigationResult({request_id:'other',status:'applied',render_version:2});expect(controller.overviewStatus()).toBe('applying');
  applied(controller);expect(controller.overviewStatus()).toBeNull();
  expect(core.applySelection).not.toHaveBeenCalled();
});
it('waits for the same partial query and ignores automatic viewport echoes as user commands',async()=>{
  let release;const {core,controller}=fixture(q=>q.cursor?new Promise(r=>release=()=>r(page(q,['B']))):Promise.resolve(page(q,['A'],{next_cursor:'next'})));
  const read=controller.query({search:'mine'});await vi.waitFor(()=>expect(release).toBeTypeOf('function'));
  controller.overview();expect(controller.navigation().mode).toBe('stop');expect(controller.overviewStatus()).toBe('pending');
  const calls=core.requestWorldView.mock.calls.length;
  controller.cameraChanged({viewport_bounds:{min_x_cm:'0',max_x_cm:'1000',min_y_cm:'0',max_y_cm:'1000'}});
  expect(core.requestWorldView).toHaveBeenCalledTimes(calls);expect(controller.overviewStatus()).toBe('pending');
  release();await read;expect(controller.navigation().mode).toBe('overview');const id=controller.navigation().request_id;
  applied(controller);expect(controller.overviewStatus()).toBeNull();
  core.requestWorldView.mockImplementation(async q=>page(q));
  await controller.query({search:'elsewhere'});expect(controller.navigation().request_id).not.toBe(id);
});
it('does not mistake the retained previous complete view for a new in-flight query',async()=>{
  const {core,controller}=fixture();await controller.query({search:'old'});let release;
  core.requestWorldView.mockImplementation(q=>new Promise(r=>release=()=>r(page(q,['B']))));
  const read=controller.query({search:'new'});controller.overview();expect(controller.navigation().mode).toBe('stop');
  release();await read;expect(controller.navigation().overview_query.binding[2]).toBe(controller.state().view.request_generation);
  expect(controller.navigation().mode).toBe('overview');
});
for(const variant of ['terminal partial','frozen','unknown']) it(`makes ${variant} unavailable without a new read`,async()=>{
  const {core,controller}=fixture(async q=>page(q,['A'],variant==='terminal partial'?{completeness:'partial'}:variant==='frozen'?{source_frozen:true}:variant==='unknown'?{completeness:'unknown',unavailable_reasons:['scope_unavailable']}:{entities:[{kind:'agent',entity_id:'A'}]}));
  await controller.query({});const calls=core.requestWorldView.mock.calls.length;controller.overview();
  expect(controller.overviewStatus()).toBe('unavailable');expect(controller.navigation().mode).not.toBe('overview');
  expect(core.requestWorldView).toHaveBeenCalledTimes(calls);
});
for(const action of ['manual camera','new selection','new navigation','scope','dispose']) it(`never applies a late complete result after ${action}`,async()=>{
  let release;const {core,controller}=fixture(q=>q.cursor?new Promise(r=>release=()=>r(page(q,['B']))):Promise.resolve(page(q,['A'],{next_cursor:'next'})));
  const read=controller.query({});await vi.waitFor(()=>expect(release).toBeTypeOf('function'));controller.overview();
  if(action==='manual camera')controller.pauseFollow();
  if(action==='new selection')controller.select({kind:'agent',id:'A'});
  if(action==='new navigation')controller.follow({kind:'agent',id:'A'});
  if(action==='scope'){core.state.auth.sessionEpoch=2;controller.resetScope();}
  if(action==='dispose')controller.dispose();
  release();await read;expect(controller.navigation()?.mode).not.toBe('overview');expect(controller.overviewStatus()).not.toBe('pending');
});
it('keeps legacy explicit navigation independent of new completeness metadata',()=>{
  const {controller}=fixture(undefined,false);controller.overview();
  expect(controller.navigation()).toMatchObject({mode:'overview'});expect(controller.navigation().overview_query).toBeUndefined();
});
it('defers already scheduled refreshes only until actual navigation consumption',async()=>{
  const {core,controller}=fixture();await controller.query({});controller.overview();const calls=core.requestWorldView.mock.calls.length;
  controller.refresh({time:2});expect(core.requestWorldView).toHaveBeenCalledTimes(calls);
  applied(controller);await vi.waitFor(()=>expect(core.requestWorldView).toHaveBeenCalledTimes(calls+1));
  expect(controller.overviewStatus()).toBeNull();
});

it('lets Rust judge the complete projected geometry and surfaces no-position refusal',async()=>{
 const {controller}=fixture(async q=>page(q,[],{aggregates:[{aggregate_id:'group'}]}));
 await controller.query({});controller.overview();expect(controller.overviewStatus()).toBe('applying');
 controller.navigationResult({request_id:controller.navigation().request_id,status:'unavailable',render_version:3});
 expect(controller.overviewStatus()).toBe('unavailable');
});
