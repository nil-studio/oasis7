import {createRoot} from 'solid-js';
import {it,expect,vi,afterEach} from 'vitest';
import {createPixelWorldSpatialController} from './pixel_world_spatial_controller.js';
import {createWorldViewClient} from './pixel_world_world_view.js';
let dispose;
afterEach(()=>{dispose?.();vi.useRealTimers();});
const page=q=>({world_id:'W',scope_id:'S',source_cut:'C',request_generation:q.request_generation,entities:[],relations:[],completeness:'complete'});
function fixture(){let release;const core={state:{worldId:'W',auth:{sessionEpoch:1},viewerProtocol:{capabilities:['world_view']}},buildGameplaySummary:()=>null,requestWorldView:vi.fn(q=>core.requestWorldView.mock.calls.length===1?new Promise(r=>release=()=>r(page(q))):Promise.resolve(page(q)))};let controller;createRoot(d=>{dispose=d;controller=createPixelWorldSpatialController({core,enabled:()=>true,getRenderState:()=>null,getLocale:()=> 'en'});});return{core,controller,release:()=>release()};}
it('keeps only the latest explicit intent behind a held read and settles replaced callers',async()=>{
 const f=fixture(),old=f.controller.query({search:'old'});await Promise.resolve();
 const a=f.controller.query({search:'A'}),b=f.controller.query({search:'B'});
 expect(await Promise.race([a,Promise.resolve('not settled')])).toBeNull();
 f.release();await Promise.all([old,b]);expect(f.core.requestWorldView.mock.calls.map(([q])=>q.search)).toEqual(['old','B']);
});
it.each(['scope','dispose','navigation','feed','return'])('cancels queued callers on %s and ignores old late pages',async kind=>{
 const f=fixture(),old=f.controller.query({search:'old'});const pending=f.controller.query({search:'new'});
 if(kind==='scope'){f.core.state.auth.sessionEpoch++;f.controller.resetScope();}
 if(kind==='dispose')f.controller.dispose();
 if(kind==='navigation')f.controller.navigate('stop');
 if(kind==='feed')f.controller.eventFeed({event_id:'missing'});
 if(kind==='return')await f.controller.returnTarget();
 expect(await pending).toBeNull();f.release();await old;await Promise.resolve();
 expect(f.core.requestWorldView).toHaveBeenCalledTimes(1);expect(f.controller.state().view).toBeNull();
});
it.each(['feed','return','navigation'])('invalidates an active explicit read before %s early return',async kind=>{
 const f=fixture(),read=f.controller.query({search:'old'});
 if(kind==='feed')f.controller.eventFeed({event_id:'missing'});else if(kind==='return')await f.controller.returnTarget();else f.controller.navigate('stop');
 expect(await read).toBeNull();f.release();await Promise.resolve();await Promise.resolve();expect(f.controller.state().view).toBeNull();expect(f.controller.state().loading).toBe(false);
});
it('settles a queued intent at its acceptance deadline without starting parallel transport',async()=>{
 vi.useFakeTimers();const f=fixture(),old=f.controller.query({search:'old'}),pending=f.controller.query({search:'new'});
 await vi.advanceTimersByTimeAsync(30000);expect(await pending).toBeNull();expect(f.controller.queryStatus()).toBe('unavailable');
 expect(f.controller.state().loading).toBe(false);expect(f.controller.readStatistics('C',false).loading).toBe(false);
 expect(f.core.requestWorldView).toHaveBeenCalledTimes(1);f.release();await old;
});
it('publishes canceled client reads as last-known without reviving their late page or resume baseline',async()=>{
 let release,calls=0;const client=createWorldViewClient({request:q=>++calls===1?Promise.resolve(page(q)):new Promise(r=>release=()=>r(page(q)))});
 client.reset('S','W');await client.query({});const prior=client.state().view,read=client.query({});client.cancel();
 expect(client.state()).toMatchObject({loading:false,stale:true,view:prior});expect(client.statistics('C',false).published.queryGenerationMatches).toBe(false);
 release();expect(await read).toBeNull();expect(client.state()).toMatchObject({loading:false,stale:true,view:prior});
});
it('does not replay an expired undispatched predicate through a later dirty refresh',async()=>{
 vi.useFakeTimers();const f=fixture(),old=f.controller.query({search:'old'}),pending=f.controller.query({search:'expired'});
 await vi.advanceTimersByTimeAsync(30000);expect(await pending).toBeNull();
 f.controller.refresh({time:2});f.release();await old;await vi.advanceTimersByTimeAsync(0);
 expect(f.core.requestWorldView.mock.calls.map(([q])=>q.search)).toEqual(['old','old']);
});
it('coalesces background refresh without invalidating the current explicit result',async()=>{
 const f=fixture(),read=f.controller.query({search:'mine'});f.controller.refresh({time:2});
 f.release();const result=await read;expect(result).not.toBeNull();expect(f.core.requestWorldView.mock.calls.every(([q])=>q.search==='mine')).toBe(true);
});
