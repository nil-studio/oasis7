import { createRoot } from 'solid-js';
import { afterEach, expect, it, vi } from 'vitest';
import { createPixelWorldSpatialController } from './pixel_world_spatial_controller.js';
let dispose;
afterEach(() => {dispose?.();vi.useRealTimers();});
const entity = id => ({kind:'agent',entity_id:id});
const page = (q, ids, extra={}) => ({world_id:'W',scope_id:'S',source_cut:'C',request_generation:q.request_generation,
  entities:ids.map(entity),relations:[],completeness:'complete',...extra});
async function fixture(presentedIds = ['A','C','B']) {
  let release;
  const core={state:{worldId:'W',auth:{sessionEpoch:1},selectedKind:null,selectedId:null,viewerProtocol:{capabilities:['world_view']}},
    applySelection:vi.fn(),buildGameplaySummary:()=>null,requestWorldView:vi.fn(async q=>page(q,['A','C','B']))};
  let controller;createRoot(cleanup=>{dispose=cleanup;controller=createPixelWorldSpatialController({core,enabled:()=>true,getRenderState:()=>null,getLocale:()=> 'en'});});
  await controller.query({});controller.select({kind:'agent',id:'A'});
  // The Host still presents B from the preceding completed update.
  controller.present(controller.state().view,{display_source_cut:'C',agents:presentedIds.map(id=>({id}))});
  core.requestWorldView.mockImplementation(q=>q.cursor==='third'?new Promise((resolve,reject)=>{release=(ids,extra={})=>ids instanceof Error ? reject(ids) : resolve(page(q,ids,extra));}):Promise.resolve(page(q,q.cursor?['C']:['A'],{next_cursor:q.cursor?'third':'second'})));
  const read=controller.query({});await vi.waitFor(()=>expect(release).toBeTypeOf('function'));
  return {core,controller,read,release};
}
it('consumes a presented selection once only after its held third page proves it',async()=>{
  const {core,controller,read,release}=await fixture();
  expect(controller.state().view.completeness).toBe('partial');
  expect(controller.state().view.entities.map(e=>e.entity_id)).toEqual(['A','C']);
  controller.selectPresented({kind:'agent',id:'B'});
  expect(controller.selection()).toEqual({kind:'agent',id:'A'});
  release(['B']);await read;
  expect(controller.selection()).toEqual({kind:'agent',id:'B'});
  expect(core.applySelection).not.toHaveBeenCalled();
});

it('shows pending immediately and rejects unknown or unpresented targets without a read',async()=>{
  const {core,controller,read,release}=await fixture();
  const calls=core.requestWorldView.mock.calls.length;
  controller.selectPresented({kind:'agent',id:'unknown'});
  expect(controller.selectionStatus()).toBeNull();
  controller.selectPresented({kind:'agent',id:'B'});
  expect(controller.selectionStatus()).toBe('pending');
  expect(core.requestWorldView).toHaveBeenCalledTimes(calls);
  release(['B']);await read;
  expect(controller.selectionStatus()).toBeNull();
});
it('finishes unavailable when complete still omits the target',async()=>{
  const {controller,read,release}=await fixture();
  controller.selectPresented({kind:'agent',id:'B'});release([]);await read;
  expect(controller.selectionStatus()).toBe('unavailable');
  expect(controller.selection()).toEqual({kind:'agent',id:'A'});
});
for(const action of ['new selection','navigation','scope','business actor','dispose']) it(`never consumes an old intent after ${action}`,async()=>{
  const {core,controller,read,release}=await fixture();
  controller.selectPresented({kind:'agent',id:'B'});
  if(action==='new selection')controller.select({kind:'agent',id:'C'});
  if(action==='navigation')controller.overview();
  if(action==='scope'){core.state.auth.sessionEpoch=2;controller.resetScope();}
  if(action==='business actor'){core.state.selectedKind='agent';core.state.selectedId='different';}
  if(action==='dispose')controller.dispose();
  release(['B']);await read;
  expect(controller.selection()?.id).not.toBe('B');
  expect(controller.selectionStatus()).not.toBe('pending');
  expect(core.applySelection).not.toHaveBeenCalled();
});
it('times out without reading or borrowing snapshot records',async()=>{
  const {core,controller,read,release}=await fixture();
  core.state.snapshot={model:{agents:{B:{id:'B'}}}};
  vi.useFakeTimers();controller.selectPresented({kind:'agent',id:'B'});
  await vi.advanceTimersByTimeAsync(30000);
  expect(controller.selectionStatus()).toBe('unavailable');
  release(['B']);await read;vi.useRealTimers();
  expect(controller.selection()?.id).not.toBe('B');
  expect(core.applySelection).not.toHaveBeenCalled();
});
it('never accepts a prior scope presentation or mismatched source cut',async()=>{
  const {controller,read,release}=await fixture();
  const view=controller.state().view;
  controller.present({...view,source_cut:'old'}, {display_source_cut:'old',agents:[{id:'B'}]});
  controller.selectPresented({kind:'agent',id:'B'});expect(controller.selectionStatus()).toBeNull();
  controller.present(view,{display_source_cut:'C',agents:[{id:'B'}]},'other-scope');
  controller.selectPresented({kind:'agent',id:'B'});expect(controller.selectionStatus()).toBeNull();
  release(['B']);await read;
  expect(controller.selection()?.id).not.toBe('B');
});

it('ends unavailable on request error without consuming an old presentation',async()=>{
  const {controller,read,release}=await fixture();controller.selectPresented({kind:'agent',id:'B'});
  release(new Error('read denied'));await read;
  expect(controller.selectionStatus()).toBe('unavailable');
  expect(controller.selection()?.id).not.toBe('B');
});
for(const extra of [{source_cut:'new-cut'},{scope_id:'new-audience'},{reorg_epoch:'2'},{request_generation:'late'}]) it(`rejects held page binding mismatch ${JSON.stringify(extra)}`,async()=>{
  const {controller,read,release}=await fixture();controller.selectPresented({kind:'agent',id:'B'});
  release(['B'],extra);await read;
  expect(controller.selectionStatus()).toBe('unavailable');
  expect(controller.selection()?.id).not.toBe('B');
});
it('a late snapshot containing the same ID cannot turn deferred inspection into actor selection',async()=>{
  const {core,controller,read,release}=await fixture();controller.selectPresented({kind:'agent',id:'B'});
  core.state.snapshot={model:{agents:{B:{id:'B'}}}};
  release(['B']);await read;
  expect(controller.selection()).toEqual({kind:'agent',id:'B'});
  expect(core.applySelection).not.toHaveBeenCalled();
});
it('clearing the presented scene immediately discards pending inspection',async()=>{
  const {controller,read,release}=await fixture();controller.selectPresented({kind:'agent',id:'B'});
  controller.present(null,null);release(['B']);await read;
  expect(controller.selectionStatus()).toBeNull();
  expect(controller.selection()?.id).not.toBe('B');
});

it('does not queue an authorized but unpresented reference or request pins',async()=>{
  const {controller,core,read,release}=await fixture(['A','C']);
  const calls=core.requestWorldView.mock.calls.length;
  controller.selectPresented({kind:'agent',id:'B'});
  expect(controller.selectionStatus()).toBeNull();
  release(['B']);await read;
  expect(controller.selection()?.id).not.toBe('B');
  expect(core.requestWorldView).toHaveBeenCalledTimes(calls);
  expect(core.requestWorldView.mock.lastCall[0]).not.toHaveProperty('pinned_entities');
});

it('rejects an old permission-generation presentation even with the same cut',async()=>{
  const {controller,read,release}=await fixture();
  const view=controller.state().view;
  controller.present({...view,source_binding:{version:{commit:{binding:{permission_generation:'old'}}}}},
    {display_source_cut:'C',agents:[{id:'B'}]});
  controller.selectPresented({kind:'agent',id:'B'});
  expect(controller.selectionStatus()).toBeNull();
  release(['B']);await read;
  expect(controller.selection()?.id).not.toBe('B');
});

it('does not use the legacy snapshot fallback while worldView is awaiting a new scope',async()=>{
  const {core,controller,read,release}=await fixture();
  controller.resetScope();core.state.snapshot={model:{agents:{B:{id:'B'}}}};
  controller.selectPresented({kind:'agent',id:'B'});
  expect(controller.selectionStatus()).toBe('unavailable');
  expect(core.applySelection).not.toHaveBeenCalled();
  release(['B']);await read;
  expect(controller.selection()?.id).not.toBe('B');
});

it('an explicit return-to-target action cancels inspection even when no business target exists',async()=>{
  const {controller,read,release}=await fixture();controller.selectPresented({kind:'agent',id:'B'});
  await controller.returnTarget();
  release(['B']);await read;
  expect(controller.selection()?.id).not.toBe('B');
  expect(controller.selectionStatus()).not.toBe('pending');
});

it('an unavailable Feed navigation cancels pending inspection before its early return',async()=>{
  const {controller,read,release}=await fixture();controller.selectPresented({kind:'agent',id:'B'});
  expect(controller.eventFeed({event_id:'unavailable'})).toBe(false);
  release(['B']);await read;
  expect(controller.selection()?.id).not.toBe('B');
  expect(controller.selectionStatus()).not.toBe('pending');
});

it('binds pending and selected to one immutable local intent without authority facts',async()=>{
 const {core,controller,read,release}=await fixture();controller.selectPresented({kind:'agent',id:'B'});
 const first=controller.selectionFeedback();
 expect(first).toEqual({intent_generation:expect.any(Number),ref:{kind:'agent',id:'B'},world_id:'W',scope_generation:1,phase:'pending'});
 expect(Object.isFrozen(first)).toBe(true);expect(Object.isFrozen(first.ref)).toBe(true);
 controller.selectPresented({kind:'agent',id:'B'});const second=controller.selectionFeedback();
 expect(second.intent_generation).toBeGreaterThan(first.intent_generation);release(['B']);await read;
 expect(controller.selectionFeedback()).toEqual({...second,phase:'selected'});expect(core.applySelection).not.toHaveBeenCalled();
});
it('clears feedback on navigation and disposal; unavailable retains only the same intent',async()=>{
 const {controller,read,release}=await fixture();controller.selectPresented({kind:'agent',id:'B'});
 const pending=controller.selectionFeedback();release([]);await read;
 expect(controller.selectionFeedback()).toEqual({...pending,phase:'unavailable'});
 controller.overview();expect(controller.selectionFeedback()).toBeNull();controller.dispose();expect(controller.selectionFeedback()).toBeNull();
});

for(const action of ['scope','business identity','new selection','dispose']) it(`does not retain an older feedback intent after ${action}`,async()=>{
 const {core,controller,read,release}=await fixture();controller.selectPresented({kind:'agent',id:'B'});
 const previous=controller.selectionFeedback();
 if(action==='scope'){core.state.auth.sessionEpoch=2;controller.resetScope();}
 if(action==='business identity'){core.state.selectedKind='agent';core.state.selectedId='different';controller.refresh({});}
 if(action==='new selection')controller.select({kind:'agent',id:'C'});
 if(action==='dispose')controller.dispose();
 release(['B']);await read;
 const next=controller.selectionFeedback();
 expect(next?.phase).not.toBe('pending');
 if(next?.phase==='selected'){expect(next.ref.id).toBe('C');expect(next.intent_generation).toBeGreaterThan(previous.intent_generation);}
 else if(next)expect(next).toEqual({...previous,phase:'unavailable'});
});
it('uses a new local scope sequence after reset without exposing the scope binding',async()=>{
 const {core,controller,read,release}=await fixture();controller.selectPresented({kind:'agent',id:'B'});
 const before=controller.selectionFeedback();release(['B']);await read;
 core.state.auth.sessionEpoch=2;controller.resetScope();expect(controller.selectionFeedback()).toBeNull();
 core.requestWorldView.mockImplementation(async q=>page(q,['A']));await controller.query({});controller.select({kind:'agent',id:'A'});
 const next=controller.selectionFeedback();expect(next.scope_generation).toBeGreaterThan(before.scope_generation);
 expect(next.intent_generation).toBeGreaterThan(before.intent_generation);
 expect(JSON.stringify(next)).not.toMatch(/authority|sessionEpoch|source_cut|request_generation/);
});

it('keeps a genuine legacy selection correlation until the business identity changes',()=>{
 const core={state:{worldId:'W',auth:{},selectedKind:null,selectedId:null},requestWorldView:vi.fn(),buildGameplaySummary:()=>null,
 applySelection:vi.fn(ref=>{core.state.selectedKind=ref.kind;core.state.selectedId=ref.id;return ref;})};
 let controller;createRoot(cleanup=>{dispose=cleanup;controller=createPixelWorldSpatialController({core,enabled:()=>false,getRenderState:()=>null,getLocale:()=> 'en'});});
 controller.selectPresented({kind:'agent',id:'legacy'});const feedback=controller.selectionFeedback();
 expect(feedback).toMatchObject({ref:{kind:'agent',id:'legacy'},phase:'selected'});
 controller.refresh({});expect(controller.selectionFeedback()).toBe(feedback);
 core.state.selectedId='other';controller.refresh({});expect(controller.selectionFeedback()).toBeNull();
 expect(core.requestWorldView).not.toHaveBeenCalled();
});
