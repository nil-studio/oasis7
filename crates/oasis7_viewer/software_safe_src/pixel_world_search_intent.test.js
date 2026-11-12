import {createRoot} from 'solid-js';
import {it,expect,vi,afterEach} from 'vitest';
import {createPixelWorldSpatialController} from './pixel_world_spatial_controller.js';
let dispose;
afterEach(()=>dispose?.());
const page=q=>({world_id:'W',scope_id:'S',source_cut:'C',request_generation:q.request_generation,entities:[],relations:[],completeness:'complete'});
function fixture(){let release;const core={state:{worldId:'W',auth:{sessionEpoch:1},viewerProtocol:{capabilities:['world_view']}},buildGameplaySummary:()=>null,requestWorldView:vi.fn(q=>core.requestWorldView.mock.calls.length===1?new Promise(r=>release=()=>r(page(q))):Promise.resolve(page(q)))};let controller;createRoot(d=>{dispose=d;controller=createPixelWorldSpatialController({core,enabled:()=>true,getRenderState:()=>null,getLocale:()=> 'en'});});return{core,controller,release:()=>release()};}
it('keeps only the latest explicit intent behind a held read and settles replaced callers',async()=>{
 const f=fixture(),old=f.controller.query({search:'old'});await Promise.resolve();
 const a=f.controller.query({search:'A'}),b=f.controller.query({search:'B'});
 expect(await Promise.race([a,Promise.resolve('not settled')])).toBeNull();
 f.release();await Promise.all([old,b]);expect(f.core.requestWorldView.mock.calls.map(([q])=>q.search)).toEqual(['old','B']);
});
