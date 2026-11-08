import { appendFileSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { numericRetentionRecord, compareRetentionComponent } from './viewer-retention-components.mjs';
const assert = (value, reason) => { if (!value) throw Error(reason); };
// No trace, DTO snapshots, GC or reload. Only bounded numeric receipts are retained.
export async function measureWorldViewRetention({ evaluate, run, fixture, out, durationMs, overview,
  beginStreamingWire, readWireSummary, readMemory }) {
  assert(Number.isFinite(durationMs) && durationMs > 0 && durationMs <= 2400000, 'memory duration must be bounded to40min');
  assert(typeof readMemory === 'function' && typeof beginStreamingWire === 'function', 'declare shared read-only memory observer and streaming wire');
  const json = async code => { const value = await evaluate(code); return typeof value === 'string' ? JSON.parse(value) : value; };
  const read = () => json(`(()=>{const s=window.__AW_TEST__.getState(),m=window.__QA_READ_METRICS__(),r=window.__OASIS7_WORLD_VIEW_RETENTION__?.(${JSON.stringify(fixture.description().cut)});return {stats:r,metrics:{status:m.status,cut:m.source_cut,bodies:m.in_view_agent_body_sprites,labels:m.laid_out_in_view_labels},camera:s.pixelWorldCamera,identity:{ready:s.authReady,player:s.authPlayerId,key:s.authPublicKey,bound:s.authBoundAgentId,registration:s.authRegistrationStatus},target:document.querySelector('button[data-agent-id="qa-agent-0499"][data-renderer-target]')?.dataset.selected}})()`);
  const settle = async expected => {
    const end = Date.now()+12000; let row;
    do { row = await read(); if (row.stats && !row.stats.loading && !row.stats.stale && !row.stats.hasError && row.stats.published.sourceCutMatches===true && row.stats.published.completeness==='complete' && row.metrics.status==='observed' && row.metrics.cut===fixture.description().cut && row.stats.published.entities===expected) return row; await run(['wait','50']); } while(Date.now()<end);
    throw Error('retention phase did not settle: '+JSON.stringify({expected,metrics:row?.metrics,loading:row?.stats?.loading}));
  };
  const disclosure='.pixel-world-exploration-disclosure';
  const setOpen = async wanted => { const open=await json(`document.querySelector(${JSON.stringify(disclosure)})?.open`); assert(typeof open==='boolean','exploration disclosure missing'); if(open!==wanted)await run(['click',disclosure+' > summary']); };
  const nativeCount = type => json(`window.__RETENTION_NATIVE_COUNTS__?.[${JSON.stringify(type)}]`);
  const select = async (id='qa-agent-0499') => { const nativeBefore=await nativeCount('selection'); await run(['click','button[data-agent-id="'+id+'"][data-renderer-target]']); const end=Date.now()+5000; do {if(await json(`document.querySelector('button[data-agent-id="${id}"][data-renderer-target]')?.dataset.selected==='true'`)){assert(await nativeCount('selection')>nativeBefore,'selection lacked trusted click');return;}await run(['wait','25']);}while(Date.now()<end);throw Error('trusted selection not confirmed'); };
  const filter = async () => { await setOpen(true); await run(['fill','.pixel-world-exploration input','qa-agent-049']); await run(['click','.pixel-world-exploration button[type="submit"]']); await settle(10); await setOpen(false); await sample('filtered',10); };
  const pan = async dx => {
    const point=await json(`(()=>{const c=document.querySelector('#pixel-world-embedded-runtime-canvas'),r=c.getBoundingClientRect();for(const y of [2,8,16,r.height-16,r.height-8,r.height-2])for(const x of [2,8,16,r.width-16,r.width-8,r.width-2]){const px=Math.round(r.left+x),py=Math.round(r.top+y);if(px+${dx}>r.left&&px+${dx}<r.right&&document.elementFromPoint(px,py)===c&&document.elementFromPoint(px+${dx},py)===c)return{x:px,y:py}}throw Error('no native pan point')})()`);
    const before=(await read()).camera,nativeBefore=await nativeCount('pan');await run(['mouse','move',String(point.x),String(point.y)]);await run(['mouse','down']);await run(['mouse','move',String(point.x+dx),String(point.y)]);await run(['mouse','up']);
    const end=Date.now()+5000;do{const after=(await read()).camera;if(before.zoom===after.zoom&&(before.pan_x_px!==after.pan_x_px||before.pan_y_px!==after.pan_y_px)){assert(await nativeCount('pan')>nativeBefore,'pan lacked trusted pointer input');return;}await run(['wait','25']);}while(Date.now()<end);throw Error('native pan did not change camera');
  };
  const zoom = async () => {
    const before=(await read()).camera,nativeBefore=await nativeCount('zoom');
    const point=await json(`(()=>{const c=document.querySelector('#pixel-world-embedded-runtime-canvas'),r=c.getBoundingClientRect();for(const y of [2,8,16,r.height-16,r.height-8,r.height-2])for(const x of [2,8,16,r.width-16,r.width-8,r.width-2]){const px=Math.round(r.left+x),py=Math.round(r.top+y);if(document.elementFromPoint(px,py)===c)return{x:px,y:py}}throw Error('no native zoom point')})()`);
    await run(['mouse','move',String(point.x),String(point.y)]);await run(['mouse','wheel','-20']);
    const end=Date.now()+5000;do{if((await read()).camera.zoom!==before.zoom){assert(await nativeCount('zoom')>nativeBefore,'zoom lacked trusted wheel');return;}await run(['wait','25']);}while(Date.now()<end);throw Error('native wheel did not change zoom');
  };
  await beginStreamingWire();
  const started=Date.now(), first=[],last=[];let samples=0,cycles=0,homeCamera=null;
  const sample = async (phase, expected=1000) => {
    const row=await settle(expected), mem=await readMemory(), stats=row.stats;
    assert(row.identity.ready===false&&row.identity.player===null&&row.identity.key===null&&row.identity.bound===null&&row.identity.registration==='guest','observer acquired identity');
    assert(row.metrics.bodies===expected&&(expected!==1000||row.metrics.labels===200),'phase density changed');
    if(phase==='home'||phase==='warmup'){if(homeCamera===null)homeCamera=row.camera;else assert(JSON.stringify(homeCamera)===JSON.stringify(row.camera),'matched home camera changed');}
    const record=numericRetentionRecord({timeMs:Date.now()-started,phase,cameraZoom:row.camera.zoom,cameraPanX:row.camera.pan_x_px,cameraPanY:row.camera.pan_y_px,viewportWidth:row.camera.viewport_size_px.width,viewportHeight:row.camera.viewport_size_px.height,generation:stats.localGeneration,viewJsonBytes:stats.published.logicalJsonBytes,viewObjects:stats.published.uniqueObjects,viewArrays:stats.published.uniqueArrays,pagingJsonBytes:stats.paging.logicalJsonBytes,pagingObjects:stats.paging.uniqueObjects,pagingArrays:stats.paging.uniqueArrays,pagingActive:stats.paging.active,pagingEntities:stats.paging.maps[0],pagingRelations:stats.paging.maps[1],pagingAggregates:stats.paging.maps[2],pagingEvents:stats.paging.maps[3],pagingCursors:stats.paging.cursors,observationMs:stats.observationMs,jsUsedHeapBytes:mem.jsUsedHeapBytes??null,jsTotalHeapBytes:mem.jsTotalHeapBytes??null,domNodes:mem.domNodes??null,documents:mem.documents??null,listeners:mem.listeners??null,wasmLinearBytes:mem.wasmLinearBytes??null,rendererBytes:mem.rendererBytes??null,gpuBytes:mem.gpuBytes??null,ownedRssBytes:mem.ownedRssBytes??null});
    appendFileSync(resolve(out,'retention-memory.jsonl'),JSON.stringify(record)+'\n');
    if(phase==='home'){samples++;if(first.length<6)first.push(record);last.push(record);if(last.length>6)last.shift();}return record;
  };
  const cycle = async () => { await select('qa-agent-0498');await pan(6);await pan(-6);await zoom();await filter();await overview();await settle(1000);await select();cycles++; };
  const summary={durationMs,scope:'M3/16GiB component observation; not formalP6, physical GPU memory, P5 or8GiB A/B; client counts current generation only, old inflight closure residual unknown',missingComponents:['WASM linear / renderer / GPU / owned RSS remain unknown unless an active-instance capability is established'],observationPolicy:'natural GC only; no forced GC. JS heap includes product plus bounded observer allocation; not attribution to client cache. Upper median of first/last six matched home samples; short windows are method probes only.',status:'incomplete'};
  try {
    await evaluate(`(()=>{const counts=window.__RETENTION_NATIVE_COUNTS__={selection:0,pan:0,zoom:0};const handler=e=>{if(!e.isTrusted)return;if(e.type==='click'&&e.target.closest?.('button[data-agent-id][data-renderer-target]'))counts.selection++;if(e.target.id==='pixel-world-embedded-runtime-canvas'){if(e.type==='pointerdown')counts.pan++;if(e.type==='wheel')counts.zoom++;}};for(const t of ['click','pointerdown','wheel'])document.addEventListener(t,handler,true);window.__RETENTION_NATIVE_DISPOSE__=()=>{for(const t of ['click','pointerdown','wheel'])document.removeEventListener(t,handler,true);delete window.__RETENTION_NATIVE_COUNTS__;delete window.__RETENTION_NATIVE_DISPOSE__;};return true})()`);
    for(let n=0;n<3;n++){await cycle();await sample('warmup');}
    homeCamera=null;const windowStarted=Date.now();await sample('home');
    do{await cycle();await run(['wait','10000']);await sample('home');}while(Date.now()-windowStarted<durationMs);
    const median=(rows,key)=>{const values=rows.map(x=>x[key]).filter(Number.isFinite).sort((a,b)=>a-b);return values.length===rows.length?values[Math.floor(values.length/2)]:null;};
    const components={};for(const key of ['viewJsonBytes','viewObjects','jsUsedHeapBytes','jsTotalHeapBytes','wasmLinearBytes','rendererBytes','gpuBytes','ownedRssBytes'])components[key]=compareRetentionComponent(median(first,key),median(last,key));
    Object.assign(summary,{actualSpanMs:Date.now()-windowStarted,samples,cycles,initial:first,final:last,components,nativeCounts:await json('JSON.stringify(window.__RETENTION_NATIVE_COUNTS__)'),wire:await readWireSummary(),status:'observed_components_only'});
    return summary;
  } catch(error){summary.error=String(error);throw error;} finally { await evaluate('window.__RETENTION_NATIVE_DISPOSE__?.();true').catch(()=>{}); writeFileSync(resolve(out,'retention-memory-summary.json'),JSON.stringify(summary,null,2)); }
}
