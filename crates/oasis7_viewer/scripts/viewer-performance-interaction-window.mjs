// Controlled read-only input window; no renderer DTO override or world command.
export const densityScript = `JSON.stringify((()=>{
 const started=performance.now(),dto=window.__OASIS7_PIXEL_WORLD_RENDER_DTO__?.();
 const fields=['agents','locations','module_visual_entities','micro_depot_facilities','industry_aggregates','world_events'];
 const counts=Object.fromEntries(fields.map(key=>[key,dto?.[key]?.length||0]));
 const canvas=document.querySelector('#pixel-world-embedded-runtime-canvas'),bounds=canvas.getBoundingClientRect();
 const targets=[...document.querySelectorAll('.pixel-world-renderer-target')];
 let inViewport=0;
 for(const target of targets){const r=target.getBoundingClientRect();if(r.width>0&&r.height>0&&r.right>Math.max(0,bounds.left)&&r.left<Math.min(innerWidth,bounds.right)&&r.bottom>Math.max(0,bounds.top)&&r.top<Math.min(innerHeight,bounds.bottom))inViewport++;}
 return {time:performance.now(),counts,projectedMarkers:Object.values(counts).reduce((a,b)=>a+b,0),nativeTargets:targets.length,inViewportTargets:inViewport,displayBudget:dto?.display_budget,canvas:{cssWidth:bounds.width,cssHeight:bounds.height,width:canvas.width,height:canvas.height},visibility:document.visibilityState,monitorMs:performance.now()-started};
})())`;

function installFeed() {
 const source=structuredClone(window.__VIEWER_PERF_SOURCE__);
 if(!source||!window.__AW_TEST__)throw Error('controlled source fixture unavailable');
 const initialX=source.model.agents['agent-0'].pos.x_cm,initialTime=source.time;
 const feed={updates:[],changes:[],revision:0,hz:0};let timer,burstTimer;
 const update=()=>{
  source.time=initialTime+(++feed.revision);
  source.model.agents['agent-0'].pos.x_cm=initialX+(feed.revision%2?1000:-1000);
  window.__AW_TEST__.injectSnapshot(source,{returnState:false});
  feed.updates.push({time:performance.now(),revision:feed.revision,hz:feed.hz});
 };
 const start=hz=>{clearInterval(timer);feed.hz=hz;feed.changes.push({time:performance.now(),hz});timer=setInterval(update,1000/hz);};
 feed.burst=()=>{clearTimeout(burstTimer);start(10);burstTimer=setTimeout(()=>start(2),5000);};
 feed.dispose=()=>{clearInterval(timer);clearTimeout(burstTimer);feed.hz=0;};
 start(2);window.__VIEWER_SNAPSHOT_FEED__=feed;
 return {type:'controlled full snapshot updates, not WorldFeed events',initialTime};
}
export const installSnapshotFeedScript = `(${installFeed.toString()})()`;

export async function nativeCameraGesture({evaluate,run,index}) {
 const point=await evaluate(`JSON.stringify((()=>{const c=document.querySelector('#pixel-world-embedded-runtime-canvas'),r=c.getBoundingClientRect();for(const [a,b] of Array.from({length:19*19},(_,i)=>[(i%19+1)/20,(Math.floor(i/19)+1)/20])){const x=Math.floor(r.x+r.width*a),y=Math.floor(r.y+r.height*b);if(document.elementFromPoint(x,y)===c&&document.elementFromPoint(x+12,y)===c)return{x,y};}return null;})())`);
 if(!point)throw Error('no unobscured native canvas point for camera input');
 const before=await evaluate('JSON.stringify(window.__AW_TEST__.getState().pixelWorldCamera)');
 await evaluate(`(()=>{window.__VIEWER_CAMERA_INPUT_DISPOSE__?.();window.__VIEWER_CAMERA_INPUT__=[];const dispose=[];const c=document.querySelector('#pixel-world-embedded-runtime-canvas');for(const type of ['pointerdown','pointermove','pointerup','wheel']){const handler=e=>{if(__VIEWER_CAMERA_INPUT__.length<64)__VIEWER_CAMERA_INPUT__.push({type:e.type,time:e.timeStamp,trusted:e.isTrusted,buttons:e.buttons,clientX:e.clientX,clientY:e.clientY,deltaY:e.deltaY});};c.addEventListener(type,handler,true);dispose.push(()=>c.removeEventListener(type,handler,true));}window.__VIEWER_CAMERA_INPUT_DISPOSE__=()=>dispose.forEach(fn=>fn());return true;})()`);
 await run(['mouse','move',String(point.x),String(point.y)]);
 const type=index%16===15?'zoom':'pan';
 if(type==='pan'){
  await run(['mouse','down']);await run(['mouse','move',String(point.x+12),String(point.y)]);await run(['mouse','up']);
 }else{
  await run(['mouse','wheel',index%32===31?'100':'-100']);
 }
 await evaluate('new Promise(resolve=>requestAnimationFrame(()=>resolve(true)))');
 const after=await evaluate('JSON.stringify({camera:window.__AW_TEST__.getState().pixelWorldCamera,input:window.__VIEWER_CAMERA_INPUT__})');
 await evaluate('window.__VIEWER_CAMERA_INPUT_DISPOSE__();true');
 if(!after.input.some(row=>row.trusted&&(row.type==='pointerdown'||row.type==='wheel')))throw Error('camera gesture did not deliver trusted input');
 return {type,point,before,after};
}
