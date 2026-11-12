// Read only the existing active module. These are observations, not GPU fences.
import {enterKeyEvents} from './viewer-owned-cdp-input.mjs';
import {readOwnedStartupProfile} from './viewer-owned-startup-context.mjs';
import {execFile} from 'node:child_process';
import {promisify,isDeepStrictEqual} from 'node:util';
import {existsSync} from 'node:fs';
import {join} from 'node:path';
import {fileURLToPath} from 'node:url';
const execute=promisify(execFile);
// Predeclare the final edge-row subject, reducing crowd overlap without hiding
// any other entity, changing labels, or choosing a subject after seeing pixels.
export const STARTUP_AGENT_ID='qa-agent-0999';
export async function readOwnedViewerScene(page,cut) {
  return page.evaluate(async({cut,id})=>{
    const ready=document.querySelector('.pixel-world-canvas')?.dataset.rendererReady==='true';
    if(!ready)return {ready:false,now:performance.now(),origin:performance.timeOrigin,visibility:document.visibilityState};
    const module=await import('/pixel-world-bridge/pixel_world_bridge.js');
    const metrics=module.readPixelWorldDisplayMetrics(),state=window.__AW_TEST__?.getState(),c=document.querySelector('#pixel-world-embedded-runtime-canvas'),r=c?.getBoundingClientRect();
    const target=document.querySelector(`button[data-agent-id="${id}"][data-renderer-target]`),t=target?.getBoundingClientRect();
    return {now:performance.now(),origin:performance.timeOrigin,visibility:document.visibilityState,ready,world:state?.worldId,
      identityValid:state?.authReady===false&&state?.authPlayerId===null&&state?.authPublicKey===null&&state?.authBoundAgentId===null&&state?.authRegistrationStatus==='guest',
      camera:state?.pixelWorldCamera,metrics,stats:window.__OASIS7_WORLD_VIEW_RETENTION__?.(cut,false)??null,
      viewport:{width:innerWidth,height:innerHeight},dpr:devicePixelRatio,
      canvas:c?{left:r.left,top:r.top,width:c.width,height:c.height,cssWidth:r.width,cssHeight:r.height}:null,
      target:t?{id,selected:target.dataset.selected,x:t.x+t.width/2,y:t.y+t.height/2,width:t.width,height:t.height}:null};
  },{cut,id:STARTUP_AGENT_ID});
}
export function assertViewerScene(row,{world,cut,requireLabels=false,requireGpu=false}) {
  if(!row.ready||row.visibility!=='visible'||row.world!==world||row.metrics.status!=='observed'||row.metrics.source_cut!==cut)throw Error('active Viewer frame/scope not ready');
  if(row.identityValid!==true)throw Error('controlled observer acquired identity');
  if(row.metrics.in_view_agent_body_sprites!==1000||row.metrics.agent_body_sprites!==1000)throw Error('actual1000 body prerequisite missing');
  if(!row.stats?.scopeBound||row.stats.published.sourceCutMatches!==true||row.stats.published.queryGenerationMatches!==true||row.stats.published.completeness!=='complete'||row.stats.published.entities!==1000||row.stats.loading)throw Error('current complete query/source prerequisite missing');
  if(requireLabels&&row.metrics.laid_out_in_view_labels!==200)throw Error('actual200 label prerequisite missing');
  if(!row.target||row.target.width<=0||row.target.height<=0||!row.canvas||row.dpr!==1)throw Error('visible target/framebuffer prerequisite missing');
  if(requireGpu&&(!row.gpu||/swiftshader|llvmpipe|softpipe|software/i.test(row.gpu)))throw Error('native GPU prerequisite missing');
}
// Call only after independent atlas pixels proved the real WebGL scene exists;
// getContext before then could itself acquire the context during initialization.
export async function readOwnedViewerGpuAfterAtlas(page) {
  return page.evaluate(()=>{const gl=document.querySelector('#pixel-world-embedded-runtime-canvas').getContext('webgl2'),debug=gl?.getExtension('WEBGL_debug_renderer_info');return debug?gl.getParameter(debug.UNMASKED_RENDERER_WEBGL):null;});
}
export function viewerAtlasStamp(row) {
  return {origin:row.origin,world:row.world,cut:row.metrics.source_cut,renderVersion:row.metrics.render_version,camera:row.camera,canvas:row.canvas,target:row.target,scope:row.stats.scopeSequence,instance:row.stats.instanceSequence,generation:row.stats.localGeneration};
}
export async function captureOwnedViewerAtlas({page,expected,out,atlasPath,prefix='atlas',deadline=performance.now()+30000}) {
  const attempts=[];
  for(let index=0;index<30&&performance.now()<deadline;index++){
    const before=await waitOwnedViewerScene(page,expected,{deadline});
    const path=join(out,`${prefix}-${String(index).padStart(3,'0')}.png`);
    if(existsSync(path))throw Error('atlas evidence path already exists');
    await page.screenshot({path,animations:'allow',timeout:10000});
    const after=await readOwnedViewerScene(page,expected.cut),row={path,before,after,bracketMatched:false,oracle:null};attempts.push(row);
    try{assertViewerScene(after,expected);row.bracketMatched=isDeepStrictEqual(viewerAtlasStamp(before),viewerAtlasStamp(after));}catch{}
    if(!row.bracketMatched)row.reason='producer/page/scope/query/geometry changed across image; unknown';
    else{
      const result=await execute('python3',[fileURLToPath(new URL('./viewer-startup-atlas-oracle.py',import.meta.url)),path,atlasPath,JSON.stringify(before)],{timeout:10000,maxBuffer:1024*1024});
      row.oracle=JSON.parse(result.stdout);
      if(row.oracle.ratio>=.9)return {attempts,matched:row,observedPageUpperMs:after.now,semantics:'atlas_image_observed_upper_not_first_physical_or_passive_present'};
    }
    await new Promise(yes=>setTimeout(yes,100));
  }
  const error=Error('bounded atlas visibility observation did not obtain matched pixels');error.atlasAttempts=attempts;throw error;
}
export async function waitOwnedViewerScene(page,expected,{requireLabels=false,deadline=performance.now()+30000}={}) {
  let last;
  while(performance.now()<deadline){
    last=await readOwnedViewerScene(page,expected.cut);
    if(performance.now()>=deadline)break;
    try{assertViewerScene(last,{...expected,requireLabels});return last;}catch{}
    await new Promise(yes=>setTimeout(yes,50));
  }
  throw Error('owned Viewer scene deadline expired: '+JSON.stringify(last));
}
export async function nativeSelectOwnedViewer(context,page,expected) {
  readOwnedStartupProfile(context);
  if(context.pages().length!==1||context.pages()[0]!==page)throw Error('native selection page ownership invalid');
  const before=await waitOwnedViewerScene(page,expected);
  if(before.target.selected==='true')throw Error('startup target was already selected before input');
  await page.evaluate(id=>{
    const b=document.querySelector(`button[data-agent-id="${id}"][data-renderer-target]`);b.focus({preventScroll:true});if(document.activeElement!==b)throw Error('native startup target focus failed');
    window.__STARTUP_NATIVE_EVIDENCE__=[];
    for(const type of ['keydown','click'])document.addEventListener(type,e=>{if(e.target===b)window.__STARTUP_NATIVE_EVIDENCE__.push({type,trusted:e.isTrusted,id:b.dataset.agentId,timeStamp:e.timeStamp,now:performance.now(),origin:performance.timeOrigin});},{capture:true,once:true});
  },STARTUP_AGENT_ID);
  const cdp=await context.newCDPSession(page);
  try{for(const params of enterKeyEvents())await cdp.send('Input.dispatchKeyEvent',params);}finally{await cdp.detach();}
  await page.waitForFunction(id=>document.querySelector(`button[data-agent-id="${id}"][data-renderer-target]`)?.dataset.selected==='true',STARTUP_AGENT_ID,{timeout:10000});
  const after=await waitOwnedViewerScene(page,expected),events=await page.evaluate(()=>window.__STARTUP_NATIVE_EVIDENCE__);
  if(after.origin!==before.origin||after.target.selected!=='true'||events.length!==2||events.some(e=>!e.trusted||e.id!==STARTUP_AGENT_ID||e.origin!==before.origin||!(e.timeStamp>0&&e.timeStamp<=e.now&&e.now-e.timeStamp<1000)))throw Error('native startup input/selection/clock pairing failed');
  return {before,after,events,semantics:'trusted_native_Enter_to_selected_DOM_not_GPU_selection_fence'};
}
export async function prepareOwnedViewerDensity(context,page,expected) {
  await waitOwnedViewerScene(page,expected);
  const disclosure=page.locator('.pixel-world-exploration-disclosure');
  await disclosure.locator('summary').click();
  const overview=page.locator('[data-query-overview]');await overview.focus();
  const cdp=await context.newCDPSession(page);
  try{for(const params of enterKeyEvents())await cdp.send('Input.dispatchKeyEvent',params);}finally{await cdp.detach();}
  await disclosure.locator('summary').click();
  const end=performance.now()+20000;
  while(performance.now()<end){
    const row=await waitOwnedViewerScene(page,expected,{deadline:end});
    if(row.metrics.laid_out_in_view_labels===200)return row;
    const point=await page.evaluate(()=>{const c=document.querySelector('#pixel-world-embedded-runtime-canvas'),r=c.getBoundingClientRect();for(let y=10;y<19;y++)for(let x=10;x<19;x++){const px=r.left+r.width*x/20,py=r.top+r.height*y/20;if(document.elementFromPoint(px,py)===c)return{x:px,y:py};}throw Error('no actual canvas wheel target');});
    await page.mouse.move(point.x,point.y);await page.mouse.wheel(0,-100);await new Promise(yes=>setTimeout(yes,200));
  }
  throw Error('normal overview/native wheel did not reach200 label layout');
}
