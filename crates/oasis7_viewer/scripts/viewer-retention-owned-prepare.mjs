import {performance} from 'node:perf_hooks';
import {readOwnedViewerScene,assertViewerScene} from './viewer-product-startup-endpoints.mjs';

// Fixture preparation only: partial pagination can publish a viewport before
// the initial unbounded query finishes. Do not wait for density before exposing
// the actual full fixture region through one ordinary canvas gesture.
export async function prepareOwnedRetentionCamera({page,expected,input,report,read=()=>readOwnedViewerScene(page,expected.cut),now=()=>performance.now()}) {
 const end=now()+12000;let before;
 do{before=await read();if(now()>=end)throw Error('owned retention preparation deadline elapsed');
  const s=before.stats;
  if(before.ready&&before.identityValid&&before.world===expected.world&&s?.scopeBound&&!s.loading&&!s.stale&&!s.hasError&&s.published.sourceCutMatches&&s.published.queryGenerationMatches&&s.published.completeness==='complete'&&before.camera?.zoom>=1.75)break;
  await input.run(['wait','50']);
 }while(now()<end);
 if(!before?.camera||now()>=end)throw Error('owned retention preparation lacks current complete query');
 report.before=before;
 const x=-before.camera.pan_x_px,y=-before.camera.pan_y_px;
 if(!Number.isFinite(x)||!Number.isFinite(y))throw Error('owned retention preparation camera unavailable');
 if(Math.abs(x)>=.5||Math.abs(y)>=.5){
  const point=await input.evaluate(`(()=>{const c=document.querySelector('#pixel-world-embedded-runtime-canvas'),r=c.getBoundingClientRect();for(let row=1;row<20;row++)for(let col=1;col<20;col++){const x=Math.round(r.left+r.width*col/20),y=Math.round(r.top+r.height*row/20),endX=Math.round(x+${x}),endY=Math.round(y+${y});if(document.elementFromPoint(x,y)===c&&endX>r.left&&endX<r.right&&endY>r.top&&endY<r.bottom)return{x,y,endX,endY}}throw Error('no actual canvas start/bounds for fixture preparation')})()`);
  report.input={...point,idealDelta:{x,y}};
  await input.evaluate(`(()=>{const a=window.__RETENTION_PREP_NATIVE__=[];const handler=e=>{if(e.target.id==='pixel-world-embedded-runtime-canvas')a.push({type:e.type,trusted:e.isTrusted,x:e.clientX,y:e.clientY,time:e.timeStamp})};for(const t of ['pointerdown','pointerup'])document.addEventListener(t,handler,true);window.__RETENTION_PREP_CLEANUP__=()=>{for(const t of ['pointerdown','pointerup'])document.removeEventListener(t,handler,true);delete window.__RETENTION_PREP_NATIVE__;delete window.__RETENTION_PREP_CLEANUP__};return true})()`);
  await input.run(['mouse','move',String(point.x),String(point.y)]);await input.run(['mouse','down']);await input.run(['mouse','move',String(point.endX),String(point.endY)]);await input.run(['mouse','up']);
  report.native=await input.evaluate(`(()=>{const a=window.__RETENTION_PREP_NATIVE__;window.__RETENTION_PREP_CLEANUP__();return a})()`);
  if(!report.native.some(e=>e.type==='pointerdown'&&e.trusted)||!report.native.some(e=>e.type==='pointerup'&&e.trusted))throw Error('fixture preparation lacked real canvas gesture');
 }
 let after;
 do{after=await read();report.last=after;if(now()>=end)throw Error('owned retention preparation deadline elapsed');
  if(after.origin!==before.origin||after.stats?.instanceSequence!==before.stats.instanceSequence||after.stats?.scopeSequence!==before.stats.scopeSequence||after.identityValid!==true)throw Error('owned retention preparation identity changed');
  try{assertViewerScene(after,{...expected,requireLabels:true});}catch{await input.run(['wait','50']);continue;}
  if(after.camera.zoom!==before.camera.zoom||report.input&&after.camera.pan_x_px===before.camera.pan_x_px&&after.camera.pan_y_px===before.camera.pan_y_px)throw Error('fixture preparation gesture did not change only pan');
  report.after=after;return after;
 }while(now()<end);
 throw Error('native fixture preparation did not produce actual1000/200');
}
