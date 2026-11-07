import { afterEach, describe, expect, it, vi } from 'vitest';
import { createRoot } from 'solid-js';
import { createPixelWorldSpatialController } from './pixel_world_spatial_controller.js';
const bounds = n => ({min_x_cm:String(n),max_x_cm:String(n+100),min_y_cm:'0',max_y_cm:'100'});
const page = (query, extra={}) => ({world_id:'W',scope_id:'S',source_cut:'cut',request_generation:query.request_generation,entities:[],relations:[],completeness:'complete',...extra});
let dispose;
afterEach(()=>{dispose?.();vi.useRealTimers();});
function setup(request) {
 const core={state:{worldId:'W',auth:{playerId:'A'},viewerProtocol:{capabilities:['world_view','world_view_pins_v1']},selectedKind:null,selectedId:null},requestWorldView:vi.fn(request|| (async q=>page(q))),applySelection:vi.fn(),buildGameplaySummary:()=>({})};
 let controller;createRoot(cleanup=>{dispose=cleanup;controller=createPixelWorldSpatialController({core,enabled:()=>true,getRenderState:()=>null,getLocale:()=> 'en',debounceMs:180});});
 return {core,controller};
}
describe('viewport query controller',()=>{
 it('keeps public reads available with no gameplay summary or business target',async()=>{
  const {core,controller}=setup();
  core.state.auth={};controller.resetScope();
  core.buildGameplaySummary=()=>null;
  expect(()=>controller.refresh({time:1,player_gameplay:null})).not.toThrow();
  await Promise.resolve();await Promise.resolve();
  expect(core.requestWorldView).toHaveBeenCalledOnce();
  expect(core.requestWorldView.mock.lastCall[0].pinned_entities).toEqual([]);
  await controller.returnTarget();
  expect(core.requestWorldView).toHaveBeenCalledOnce();
  expect(controller.navigation()).toBeNull();
  expect(controller.selection()).toBeNull();
  expect(core.applySelection).not.toHaveBeenCalled();
 });
 it('coalesces camera events using only Rust bounds and ignores apply echoes',async()=>{
  vi.useFakeTimers();const {core,controller}=setup();
  controller.cameraChanged({viewport_bounds:bounds(1)});controller.cameraChanged({viewport_bounds:bounds(2)});controller.cameraChanged({viewport_bounds:bounds(3)});
  expect(core.requestWorldView).not.toHaveBeenCalled();await vi.advanceTimersByTimeAsync(180);
  expect(core.requestWorldView).toHaveBeenCalledOnce();expect(core.requestWorldView.mock.calls[0][0].bounds).toEqual(bounds(3));
  controller.cameraChanged({viewport_bounds:bounds(3)});await vi.advanceTimersByTimeAsync(1000);
  expect(core.requestWorldView).toHaveBeenCalledOnce();expect(controller.navigation()).toBeNull();
 });
 it('invalidates an old result immediately when camera moves before the debounce',async()=>{
  vi.useFakeTimers();let finish;const {controller}=setup(q=>new Promise(resolve=>{finish=()=>resolve(page(q,{entities:[{kind:'agent',entity_id:'old'}]}));}));
  const old=controller.query({});controller.cameraChanged({viewport_bounds:bounds(30)});finish();await old;
  expect(controller.state().view).toBeNull();
 });
 it('cancels queued queries on scope change and disposal',async()=>{
  vi.useFakeTimers();const {core,controller}=setup();controller.cameraChanged({viewport_bounds:bounds(4)});
  core.state.auth.playerId='B';controller.resetScope();await vi.advanceTimersByTimeAsync(180);expect(core.requestWorldView).not.toHaveBeenCalled();
  controller.cameraChanged({viewport_bounds:bounds(5)});dispose();await vi.advanceTimersByTimeAsync(180);expect(core.requestWorldView).not.toHaveBeenCalled();
 });
});

it('pins the inspected facility, original target and explicit follow without changing actor control',async()=>{
 const {core,controller}=setup(async q=>page(q,{entities:[{kind:'factory',entity_id:'F',label:'Furnace'}]}));
 core.buildGameplaySummary=()=>({acceptedIntentTarget:'original'});core.state.selectedKind='agent';core.state.selectedId='old';
 await controller.query({});expect(controller.select({kind:'module_visual',id:'F'})).toEqual({kind:'factory',id:'F'});
 controller.follow({kind:'agent',id:'following'});await controller.query({bounds:bounds(40)});
 expect(core.requestWorldView.mock.lastCall[0].pinned_entities).toEqual([{kind:'factory',id:'F'},{kind:'agent',id:'original'},{kind:'agent',id:'following'}]);
 expect(core.applySelection).not.toHaveBeenCalled();expect(core.state.selectedId).toBe('old');
});
it('preserves viewport bounds through world refresh without issuing navigation',async()=>{
 vi.useFakeTimers();const {core,controller}=setup();controller.cameraChanged({viewport_bounds:bounds(10)});await vi.advanceTimersByTimeAsync(180);
 const snapshot={time:5};controller.refresh(snapshot);await Promise.resolve();controller.refresh(snapshot);
 expect(core.requestWorldView).toHaveBeenCalledTimes(2);expect(core.requestWorldView.mock.lastCall[0].bounds).toEqual(bounds(10));expect(controller.navigation()).toBeNull();
});
it('returns to the original target then reads its viewport without retaining a hidden search filter',async()=>{
 vi.useFakeTimers();const {core,controller}=setup(async q=>page(q,{entities:[{kind:'agent',entity_id:'main',position:{x_cm:'10',y_cm:'20',z_cm:'0'},provenance:{geometry_source:'authoritative',freshness:'current',valid_at_source_cut:'cut'}}]}));
 core.buildGameplaySummary=()=>({acceptedIntentTarget:'main'});await controller.returnTarget();expect(controller.navigation()).toMatchObject({mode:'locate',target:{kind:'agent',id:'main'}});
 controller.cameraChanged({viewport_bounds:bounds(6)});await vi.advanceTimersByTimeAsync(180);
 expect(core.requestWorldView.mock.lastCall[0].search).toBeUndefined();expect(core.requestWorldView.mock.lastCall[0].pinned_entities).toContainEqual({kind:'agent',id:'main'});
});
it('never borrows a stale snapshot selection for a new projected entity or a changed scope',async()=>{
 const {core,controller}=setup(async q=>page(q,{entities:[{kind:'agent',entity_id:'new'}]}));
 core.state.selectedKind='agent';core.state.selectedId='old';await controller.query({});
 expect(controller.select({kind:'agent',id:'new'})).toEqual({kind:'agent',id:'new'});expect(controller.selection()).toEqual({kind:'agent',id:'new'});expect(core.applySelection).not.toHaveBeenCalled();
 expect(controller.select({kind:'location',id:'not-authorized'})).toBeNull();
 core.state.auth.playerId='B';expect(controller.selection()).toBeNull();expect(controller.select({kind:'agent',id:'new'})).toBeNull();
});

it('keeps querying an older world_view server without sending the unnegotiated pins extension',async()=>{
 vi.useFakeTimers();const {core,controller}=setup();core.state.viewerProtocol.capabilities=['world_view'];
 core.state.selectedKind='agent';core.state.selectedId='visible';await controller.query({search:'ore'});
 expect(core.requestWorldView.mock.lastCall[0]).not.toHaveProperty('pinned_entities');
 controller.cameraChanged({viewport_bounds:bounds(8)});await vi.advanceTimersByTimeAsync(180);
 expect(core.requestWorldView.mock.lastCall[0]).toMatchObject({search:'ore',bounds:bounds(8)});
 expect(core.requestWorldView.mock.lastCall[0]).not.toHaveProperty('pinned_entities');expect(controller.state().error).toBeNull();
});

it('gates evolution fields and coalesces snapshot refreshes during a delta request',async()=>{
 const {core,controller}=setup();core.state.viewerProtocol.capabilities.push('world_view_delta_v1','world_view_aggregate_v1','world_view_event_anchor_v1');
 let finish;core.requestWorldView.mockImplementationOnce(q=>new Promise(resolve=>{finish=()=>resolve(page(q));}));
 const initial=controller.query({});controller.refresh({time:1});controller.refresh({time:2});controller.refresh({time:3});
 expect(core.requestWorldView).toHaveBeenCalledOnce();finish();await initial;await Promise.resolve();
 expect(core.requestWorldView).toHaveBeenCalledTimes(2);expect(core.requestWorldView.mock.lastCall[0]).toMatchObject({detail_level:'auto',include_events:true});
});
it('drills an authorized aggregate down without replacing the business actor',async()=>{
 const aggregate={aggregate_id:'site',member_count:'3',provenance:{geometry_source:'display_aggregation',valid_at_source_cut:'cut',freshness:'unknown'},drilldown:{bounds:bounds(4),detail_level:'entity'}};
 const {core,controller}=setup(async q=>page(q,{aggregates:[aggregate]}));core.state.viewerProtocol.capabilities.push('world_view_aggregate_v1');
 core.state.selectedKind='agent';core.state.selectedId='actor';await controller.query({});await controller.drilldown(aggregate);
 expect(controller.navigation()).toMatchObject({mode:'fit_bounds',bounds:bounds(4),target:null});expect(core.requestWorldView.mock.lastCall[0].detail_level).toBe('entity');expect(core.state.selectedId).toBe('actor');expect(core.applySelection).not.toHaveBeenCalled();
});

it('links only the exact authorized event identity in the current feed',async()=>{
 const event={event_id:'W:2:7',identity:{world_id:'W',reorg_epoch:'2',event_seq:'7'},feed_ref:{world_id:'W',reorg_epoch:'2',event_seq:'7'},logical_time:'3',source_class:'runtime_journal_payload',source_cut:'cut',freshness:'last_known'};
 const {core,controller}=setup(async q=>page(q,{reorg_epoch:'2',events:[event]}));await controller.query({});
 core.state.worldFeed={worldId:'W',reorgEpoch:'1',events:[{event_seq:'7'}]};expect(controller.eventFeed(event)).toBe(false);
 core.state.worldFeed.reorgEpoch='2';const exploration=document.createElement('details');exploration.className='pixel-world-exploration-disclosure';exploration.open=true;document.body.append(exploration);const details=document.createElement('details');const row=document.createElement('article');row.dataset.worldFeedEvent='7';details.append(row);document.body.append(details);
 expect(controller.eventFeed(event)).toBe(true);expect(details.open).toBe(true);expect(exploration.open).toBe(false);expect(document.activeElement).toBe(row);
 core.state.auth.playerId='other';expect(controller.eventFeed(event)).toBe(false);details.remove();exploration.remove();expect(core.applySelection).not.toHaveBeenCalled();
});

it('returns to the target after cancelling an outstanding viewport read',async()=>{
 let finish;const {core,controller}=setup();core.buildGameplaySummary=()=>({acceptedIntentTarget:'main'});
 core.requestWorldView.mockImplementationOnce(q=>new Promise(resolve=>{finish=()=>resolve(page(q,{entities:[]}));}));
 core.requestWorldView.mockImplementation(q=>Promise.resolve(page(q,{entities:[{kind:'agent',entity_id:'main',position:{x_cm:'1',y_cm:'2',z_cm:'0'},provenance:{geometry_source:'authoritative',freshness:'current',valid_at_source_cut:'cut'}}]})));
 const old=controller.query({bounds:bounds(1)});const returning=controller.returnTarget();finish();await old;await returning;
 expect(core.requestWorldView.mock.lastCall[0].search).toBe('main');expect(controller.navigation()).toMatchObject({mode:'locate',target:{kind:'agent',id:'main'}});expect(controller.state().notice).not.toBe('target_unavailable');
});

it('keeps a programmatic drill level until manual camera input restores automatic aggregation',async()=>{
 vi.useFakeTimers();const aggregate={aggregate_id:'site',member_count:'3',provenance:{geometry_source:'display_aggregation',valid_at_source_cut:'cut',freshness:'unknown'},drilldown:{bounds:bounds(4),detail_level:'entity'}};
 const {core,controller}=setup(async q=>page(q,{aggregates:[aggregate]}));core.state.viewerProtocol.capabilities.push('world_view_aggregate_v1');
 await controller.query({});await controller.drilldown(aggregate);
 controller.cameraChanged({viewport_bounds:bounds(4)});await vi.advanceTimersByTimeAsync(180);
 expect(core.requestWorldView.mock.lastCall[0].detail_level).toBe('entity');
 controller.pauseFollow();controller.cameraChanged({viewport_bounds:bounds(40)});await vi.advanceTimersByTimeAsync(180);
 expect(core.requestWorldView.mock.lastCall[0]).toMatchObject({detail_level:'auto',bounds:bounds(40)});
});
