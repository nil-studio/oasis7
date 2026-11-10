import { isDeepStrictEqual } from 'node:util';
import { resolve } from 'node:path';
import { writeFileSync } from 'node:fs';

// Executed only by the local QA HTTP server. Intercept the existing gated fixture
// factory before injectSnapshot consumes it; no release API or persisted DTO.
export function materialRecoveryHook(phase) {
 return `(()=>{
 const phase=${JSON.stringify(phase)};
 Object.defineProperty(window,'__OASIS7_PIXEL_WORLD_VISUAL_FIXTURES__',{configurable:true,set(map){
  const get=map.get.bind(map);map.get=function(name){const factory=get(name);if(name!=='material_shells')return factory;return ()=>{
   const s=factory();s.model.locations['loc-0'].pos={x_cm:3400000,y_cm:3750000,z_cm:0};
   for(const m of Object.values(s.model.module_visual_entities))m.anchor.data.pos.y_cm=3000000;
   const location=(id,x,y)=>({id,name:'·',pos:{x_cm:x,y_cm:y,z_cm:0},profile:{radius_cm:12000,radiation_emission_per_tick:0,material:'silicate'}});
   s.model.locations['loc-recovery-ore']=location('loc-recovery-ore',2400000,2750000);
   s.model.locations['loc-recovery-depot']=location('loc-recovery-depot',1600000,3100000);
   const depot=s.player_gameplay.micro_depot_facilities[0];depot.location_id='loc-recovery-depot';
   depot.available_units_by_kind={iron:100};depot.throughput_limit_units_per_epoch=100;depot.throughput_remaining_units=100;
   if(phase==='removed'){s.model.module_visual_entities={};delete s.model.locations['loc-0'].fragment_profile;s.player_gameplay.micro_depot_facilities=[];}
   window.__MATERIAL_RECOVERY_INPUT__={phase,snapshot:s};return s;
  };};Object.defineProperty(window,'__OASIS7_PIXEL_WORLD_VISUAL_FIXTURES__',{value:map,configurable:true,writable:true});
 }});
 const send=WebSocket.prototype.send;WebSocket.prototype.send=function(data){let type='unparsed';try{type=JSON.parse(data).type||'untyped';}catch{}const log=JSON.parse(sessionStorage.getItem('material-outbound')||'[]');log.push(type);sessionStorage.setItem('material-outbound',JSON.stringify(log.slice(-200)));return send.call(this,data);};
 const fetch=window.fetch;window.fetch=function(input,init){const log=JSON.parse(sessionStorage.getItem('material-http')||'[]');log.push({method:String(init?.method||input?.method||'GET').toUpperCase(),path:new URL(input?.url||input,location.href).pathname});sessionStorage.setItem('material-http',JSON.stringify(log.slice(-200)));return fetch.call(this,input,init);};
 })();`;
}

export async function runMaterialRecovery({run,evaluate,state,waitFor,assert,out,url,requestedCase,summary,setPhase,oracle}) {
 const capture = async (name,phase) => {
  await run(['wait','1200']);const s=await state();
  const metadata=JSON.parse(await evaluate(`JSON.stringify({identity:[__AW_TEST__.getState().authPlayerId,__AW_TEST__.getState().authPublicKey],origin:performance.timeOrigin,input:__MATERIAL_RECOVERY_INPUT__,anchors:Object.fromEntries(['module:material-smelter','location:loc-recovery-ore','location:loc-recovery-depot'].map(key=>{const [kind,id]=key.split(':');const b=document.querySelector('button[data-'+kind+'-id="'+id+'"][data-renderer-target]');const r=b?.getBoundingClientRect();return [key,r?{x:r.x+r.width/2,y:r.y+r.height/2}:null]})),outbound:JSON.parse(sessionStorage.getItem('material-outbound')||'[]'),http:JSON.parse(sessionStorage.getItem('material-http')||'[]')})`));
  await run(['screenshot','--full',resolve(out,`${name}-recovery-${phase}.png`)]);
  return {...s,...metadata};
 };
 for(const [name,w,h,d] of [['desktop',1440,1000,1],['narrow',390,844,1],['dpr2',1440,1000,2]].filter(([n])=>requestedCase==='all'||requestedCase===n)) {
  setPhase('known');await run(['open',url]);await run(['set','viewport',String(w),String(h),String(d)]);
  await evaluate(`setTimeout(()=>location.assign(${JSON.stringify(url)}),50);true`);await run(['wait','500']);await waitFor(`document.querySelector('.pixel-world-canvas')?.dataset.rendererReady==='true'`);
  const row={name,scope:'recovery',phases:{initial:await capture(name,'initial')}};summary.cases.push(row);
  const initial=row.phases.initial;
  assert(initial.identity.every(v=>typeof v==='string'&&v.length>0),'empty session identity');
  assert(initial.dto.receipt_target?.agent_id==='agent-0','missing explicit fixture receipt target');
  assert(initial.dto.fragment_terrain.length>0&&initial.dto.module_visual_entities.length>0&&initial.dto.micro_depot_facilities.length>0,'combined initial materials missing');
  for(const phase of ['restored','removed','recreated']) {
   const old=await evaluate('performance.timeOrigin');
   await evaluate(`sessionStorage.setItem('material-outbound','[]');sessionStorage.setItem('material-http','[]');(()=>{const gl=document.querySelector('#pixel-world-embedded-runtime-canvas').getContext('webgl2');const loss=gl.getExtension('WEBGL_lose_context');if(!loss)throw Error('loss extension missing');loss.loseContext();return true;})()`);
   await waitFor(`document.querySelector('.pixel-world-render-unavailable__retry')`);
   assert((await state()).targets.length===0,'stale input targets survived context loss');
   assert(await evaluate('performance.timeOrigin')===old,'automatic reload occurred');
   setPhase(phase==='removed'?'removed':'known');
   await evaluate(`document.querySelector('.pixel-world-render-unavailable__retry').focus({preventScroll:true});true`);await run(['press','Enter']);await run(['wait','500']);
   await waitFor(`performance.timeOrigin!==${old}&&document.querySelector('.pixel-world-canvas')?.dataset.rendererReady==='true'`);
   const current=row.phases[phase]=await capture(name,phase);
   assert(JSON.stringify(current.identity)===JSON.stringify(initial.identity),'fixture session identity changed');
   assert(JSON.stringify(current.dto.receipt_target)===JSON.stringify(initial.dto.receipt_target),'fixture receipt target changed');
   assert(!current.outbound.some(t=>['control','gameplay_action','prompt_control','agent_chat'].includes(t)),'Command sent during recovery');
   assert(!current.http.some(r=>['POST','PUT','PATCH','DELETE'].includes(r.method)),'mutating HTTP during recovery');
   assert(current.input.phase===(phase==='removed'?'removed':'known'),'HTTP reload did not consume current controlled phase');
   const normalized=structuredClone(current.input.snapshot);
   if(phase==='removed'){normalized.model.module_visual_entities=structuredClone(initial.input.snapshot.model.module_visual_entities);normalized.model.locations['loc-0'].fragment_profile=structuredClone(initial.input.snapshot.model.locations['loc-0'].fragment_profile);normalized.player_gameplay.micro_depot_facilities=structuredClone(initial.input.snapshot.player_gameplay.micro_depot_facilities);}
   assert(isDeepStrictEqual(normalized,initial.input.snapshot),'reload fixture changed unrelated source input');
   const count=current.dto.fragment_terrain.length+current.dto.module_visual_entities.length+current.dto.micro_depot_facilities.length;
   assert(phase==='removed'?count===0:count>0,'fresh controlled input lifecycle mismatch');
  }
  writeFileSync(resolve(out,`${name}-recovery-input.json`),JSON.stringify(row,null,2));
  row.oracle=oracle('recovery',out,name,JSON.stringify(row));assert(row.oracle.passed,'combined recovery PNG oracle failed: '+JSON.stringify(row.oracle));
 }
}
