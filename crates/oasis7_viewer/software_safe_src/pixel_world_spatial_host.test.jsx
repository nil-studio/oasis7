import { fireEvent, screen, waitFor } from '@solidjs/testing-library';
import { beforeAll, expect, it, vi } from 'vitest';
import { renderPixelWorldHost, runtimeMock, sampleSnapshot, useTestRustRenderState } from './pixel_world_host_test_support.jsx';

// Compile the shared host fixture before measuring interaction deadlines. Each case still resets its core modules.
beforeAll(async()=>{await import('./pixel_world_host.jsx');});

it('connects Rust camera bounds to authorized queries and opens a read-only facility inspector',async()=>{
 useTestRustRenderState();let request;
 const {core}=await renderPixelWorldHost(sampleSnapshot(),undefined,'en',{setupCore(core){
  core.state.viewerProtocol.capabilities=['world_view','world_view_pins_v1'];
  request=vi.spyOn(core,'requestWorldView').mockImplementation(async query=>({world_id:core.state.worldId || 'W',scope_id:'S',source_cut:'cut',request_generation:query.request_generation,
   entities:[{kind:'factory',entity_id:'F',label:'Furnace',position:{x_cm:'10',y_cm:'20',z_cm:'0'},render_record:{id:'F'},provenance:{geometry_source:'location_derived',freshness:'current',valid_at_source_cut:'cut'}}],relations:[],completeness:'complete'}));
 }});
 await waitFor(()=>expect(runtimeMock.mountCalls).toBe(1));
 const bounds={min_x_cm:'0',max_x_cm:'100',min_y_cm:'0',max_y_cm:'200'};
 runtimeMock.onEvent({type:'camera_state_changed',camera:{zoom:2,pan_x_px:0,pan_y_px:0,viewport_bounds:bounds}});
 await waitFor(()=>expect(request).toHaveBeenCalledWith(expect.objectContaining({bounds})));
 expect(request.mock.calls.filter(([query])=>query.bounds)).toHaveLength(1);
 const actor=core.state.auth.boundAgentId;
 fireEvent.click(screen.getByRole('button',{name:'Furnace'}));
 expect(screen.getByRole('region',{name:'Object inspection'})).toHaveTextContent('Read-only observation');
 await waitFor(()=>expect(document.querySelector('.pixel-world-exploration-disclosure').open).toBe(true));
 expect(document.querySelector('.pixel-world-exploration-disclosure > .pixel-world-exploration-panel .pixel-world-entity-inspection')).toBeInTheDocument();
 expect(core.state.auth.boundAgentId).toBe(actor);
 const disclosure=document.querySelector('.pixel-world-exploration-disclosure');disclosure.open=false;core.requestRender();
 await Promise.resolve();expect(disclosure.open).toBe(false);
});

it('inspects new authorized Agent and Location without rewriting the old snapshot, actor or receipt',async()=>{
 useTestRustRenderState();const derive=runtimeMock.deriveRenderState=vi.fn(runtimeMock.deriveRenderState);let originalSnapshot;
 const {core}=await renderPixelWorldHost(sampleSnapshot(),undefined,'en',{setupCore(core){
  core.state.viewerProtocol.capabilities=['world_view','world_view_pins_v1'];originalSnapshot=core.state.snapshot;
  core.applySelection({kind:'agent',id:'agent-0'});core.state.lastGameplayActionFeedback={kind:'gameplay_action',id:'original-operation',agentId:'agent-0',stage:'sent'};
  vi.spyOn(core,'requestWorldView').mockImplementation(async query=>({world_id:core.state.worldId || 'W',scope_id:'S',source_cut:'new-cut',request_generation:query.request_generation,
   entities:[{kind:'agent',entity_id:'new-agent',label:'New Agent'},{kind:'location',entity_id:'new-location',label:'New Location'}].map(entity=>({...entity,position:{x_cm:'10',y_cm:'20',z_cm:'0'},provenance:{geometry_source:'authoritative',freshness:'current',valid_at_source_cut:'new-cut'}})),relations:[],completeness:'complete'}));
 }});
 await waitFor(()=>expect(runtimeMock.mountCalls).toBe(1));core.requestRender();
 await waitFor(()=>expect(screen.getByRole('button',{name:'New Agent'})).toBeInTheDocument());
 const actor=core.state.auth.boundAgentId, receipt=core.state.lastGameplayActionFeedback;
 fireEvent.click(screen.getByRole('button',{name:'New Agent'}));
 expect(screen.getByRole('region',{name:'Object inspection'})).toHaveTextContent('New Agent');
 await waitFor(()=>expect(derive.mock.lastCall[0]).toMatchObject({selectedKind:'agent',selectedId:'new-agent'}));
 fireEvent.click(screen.getByRole('button',{name:'New Location'}));
 expect(screen.getByRole('region',{name:'Object inspection'})).toHaveTextContent('New Location');
 await waitFor(()=>expect(derive.mock.lastCall[0]).toMatchObject({selectedKind:'location',selectedId:'new-location'}));
 expect(core.state.selectedId).toBe('agent-0');expect(core.state.auth.boundAgentId).toBe(actor);expect(core.state.lastGameplayActionFeedback).toBe(receipt);
 expect(core.state.snapshot).toBe(originalSnapshot);expect(core.state.snapshot.model.agents['new-agent']).toBeUndefined();expect(core.state.snapshot.model.locations['new-location']).toBeUndefined();
});

it('removes a selected projected object on a delta while preserving actor and receipt identity',async()=>{
 useTestRustRenderState();let request,removed=false;
 const {core}=await renderPixelWorldHost(sampleSnapshot(),undefined,'en',{setupCore(core){
  core.state.viewerProtocol.capabilities=['world_view','world_view_pins_v1','world_view_delta_v1'];
  core.applySelection({kind:'agent',id:'agent-0'});core.state.lastGameplayActionFeedback={id:'original-operation',agentId:'agent-0',stage:'sent'};
  request=vi.spyOn(core,'requestWorldView').mockImplementation(async query=>{
   const common={world_id:core.state.worldId || 'W',scope_id:'S',reorg_epoch:'0',query_id:'Q',request_generation:query.request_generation,completeness:'complete',relations:[],aggregates:[],events:[]};
   if(query.resume_cursor){removed=true;return {...common,delivery:'delta',base_cut:'A',source_cut:'B',previous_sequence:'1',update_sequence:'2',observed_at_tick:'5',carry_forward:true,resume_cursor:'next',entities:[],removals:[{record_kind:'entity',kind:'factory',id:'factory-3',reason:'factory_recycled',world_deleted:true}]};}
   return {...common,delivery:'snapshot',source_cut:'A',update_sequence:'1',resume_cursor:'resume',entities:removed?[]:[{kind:'factory',entity_id:'factory-3',label:'Projected Factory',provenance:{freshness:'unknown',valid_at_source_cut:'A'}}]};
  });
 }});
 await waitFor(()=>expect(runtimeMock.mountCalls).toBe(1));core.requestRender();
 await waitFor(()=>expect(screen.getByRole('button',{name:'Projected Factory'})).toBeInTheDocument());
 const actor=core.state.auth.boundAgentId,receipt=core.state.lastGameplayActionFeedback;
 fireEvent.click(screen.getByRole('button',{name:'Projected Factory'}));core.requestRender();
 await waitFor(()=>expect(request.mock.calls.length).toBeGreaterThanOrEqual(2));
 core.state.snapshot={...core.state.snapshot};core.requestRender();
 await waitFor(()=>expect(screen.queryByRole('button',{name:'Projected Factory'})).toBeNull());
 expect(screen.queryByRole('region',{name:'Object inspection'})).toBeNull();
 core.requestRender();
 await waitFor(()=>{const deltaIndex=request.mock.calls.findIndex(([query])=>query.resume_cursor);expect(deltaIndex).toBeGreaterThanOrEqual(0);expect(request.mock.calls.slice(deltaIndex+1).some(([query])=>!query.resume_cursor)).toBe(true);});
 expect(screen.getByText(/1 objects confirmed removed from the world/)).toBeInTheDocument();
 expect(screen.queryByRole('button',{name:'Projected Factory'})).toBeNull();
 expect(core.state.selectedId).toBe('agent-0');expect(core.state.auth.boundAgentId).toBe(actor);expect(core.state.lastGameplayActionFeedback).toBe(receipt);
 fireEvent.click(screen.getByRole('button',{name:'Overview query results'}));
 expect(screen.queryByText(/1 objects confirmed removed from the world/)).toBeNull();
});
