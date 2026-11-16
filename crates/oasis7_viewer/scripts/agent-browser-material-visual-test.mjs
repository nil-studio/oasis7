import { createServer } from 'node:http';
import { readFileSync, writeFileSync, mkdirSync } from 'node:fs';
import { resolve, extname, relative, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawn, execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { materialRecoveryHook, runMaterialRecovery } from './pixel-world-material-recovery.mjs';
import { createOwnedSessionLifecycle } from './agent-browser-visual-runner-lifecycle.mjs';
import { factoryPhaseProgram, terrainPhaseProgram, factoryFocusProgram, factorySelectedExpression, runtimeBaselineProgram, runtimePhaseProgram, assertRuntimeInitialInput } from './material-visual-phase-programs.mjs';

// Controlled material fixture evidence; no live authorization/world outcome claim.
const scripts = dirname(fileURLToPath(import.meta.url));
const repo = resolve(scripts, '../../..');
const root = resolve(process.env.AGENT_VISUAL_ARTIFACT_ROOT || resolve(scripts, '../.viewer-test-dist'));
if (!process.env.AGENT_VISUAL_SOURCE_HEAD) throw Error('declare the immutable visual build AGENT_VISUAL_SOURCE_HEAD');
const git = args => execFileSync('git', args, { cwd: repo, encoding: 'utf8' }).trim();
const source = git(['rev-parse', process.env.AGENT_VISUAL_SOURCE_HEAD + '^{commit}']);
const out = resolve(repo, 'output/playwright/pixel-world-materials', new Date().toISOString().replace(/[:.]/g, '-'));
mkdirSync(out, { recursive: true });
const scope = process.env.MATERIAL_VISUAL_SCOPE || 'all';
if (!['all', 'factory', 'terrain', 'runtime', 'recovery'].includes(scope)) throw Error('unsupported MATERIAL_VISUAL_SCOPE');
const requestedCase = process.env.AGENT_VISUAL_CASE || 'all';
if (!['all', 'desktop', 'narrow', 'dpr2'].includes(requestedCase)) throw Error('unsupported AGENT_VISUAL_CASE');
const session = `pixel-material-${process.pid}`;
const command = process.env.AGENT_BROWSER_BIN || 'agent-browser';
const lifecycle = createOwnedSessionLifecycle({ command, session, prefixArgs: [] });
const hash = bytes => createHash('sha256').update(bytes).digest('hex');
const sprites = {};
for (const name of ['ore', 'smelter', 'depot']) {
  const bytes = execFileSync('git', ['show', `${source}:doc/world-simulator/viewer/assets/ui-ux/pixel-world/${name}.png`], { cwd: repo });
  writeFileSync(resolve(out, `${name}-source.png`), bytes);
  sprites[name] = hash(bytes);
}
const summary = { source, checkoutHead: git(['rev-parse', 'HEAD']), runnerScriptSha256: hash(readFileSync(new URL(import.meta.url))), phaseProgramSha256: hash(readFileSync(new URL('./material-visual-phase-programs.mjs', import.meta.url))), oracleSha256: hash(readFileSync(resolve(scripts, 'pixel-world-material-oracle.py'))), sprites, assets: Object.fromEntries(['viewer.js', 'pixel-world-bridge/webgl2/pixel_world_bridge_bindgen_bg.wasm'].map(path => [path, hash(readFileSync(resolve(root, path)))])), profile: 'visual-test', scope, requestedCase, fixture: 'material_shells', liveConnection: false, controlledOverrides: { factory: { y_cm: 3000000 }, terrain: { location: 'loc-0', pos: { x_cm: 3400000, y_cm: 3300000, z_cm: 0 }, modulesRemoved: true } }, limitations: ['受控输入不代签真实世界授权', '地形验证局部变化与恢复；原base组成色同时变化，独立ore纹理特征/受控启停尚未覆盖', '未覆盖 depot PNG oracle、全组合恢复、性能预算'], cases: [] };
const oracle = (kind, ...args) => JSON.parse(execFileSync('python3', [resolve(scripts, 'pixel-world-material-oracle.py'), kind, ...args], { encoding: 'utf8' }));
const assert = (ok, message) => { if (!ok) throw new Error(message); };
const run = args => new Promise((yes, no) => {
  const child = spawn(command, ['--session', session, ...args]);
  const timer=setTimeout(()=>{child.kill('SIGTERM');no(new Error('browser command timed out: '+args.join(' ')));},45000);
  let stdout = '', stderr = '';
  child.stdout.on('data', data => stdout += data); child.stderr.on('data', data => stderr += data);
  child.on('error', no); child.on('close', code => {clearTimeout(timer);code === 0 ? yes(stdout) : no(new Error(stderr + stdout));});
});
async function evaluate(code) {
  const result = JSON.parse(await run(['--json', 'eval', code]));
  assert(result.success, JSON.stringify(result));
  return result.data.result;
}
const read = `(() => { const canvas=document.querySelector('#pixel-world-embedded-runtime-canvas'); const gl=canvas?.getContext('webgl2'); const debug=gl?.getExtension('WEBGL_debug_renderer_info'); return {ready:document.querySelector('.pixel-world-canvas')?.dataset.rendererReady==='true', dto:window.__OASIS7_PIXEL_WORLD_RENDER_DTO__?.(), dpr:devicePixelRatio,browser:navigator.userAgent, viewport:{width:innerWidth,height:innerHeight}, canvas:canvas?{width:canvas.width,height:canvas.height,cssWidth:canvas.getBoundingClientRect().width,cssHeight:canvas.getBoundingClientRect().height,inlineWidth:canvas.style.width,inlineHeight:canvas.style.height,parentBorderLeft:getComputedStyle(canvas.parentElement).borderLeftWidth,parents:[canvas.parentElement,canvas.parentElement?.parentElement].filter(Boolean).map(e=>({classes:e.className,width:e.getBoundingClientRect().width,height:e.getBoundingClientRect().height}))}:null,documentLayout:{width:document.documentElement.clientWidth,height:document.documentElement.clientHeight}, gpu:debug?gl.getParameter(debug.UNMASKED_RENDERER_WEBGL):null, targets:[...document.querySelectorAll('button[data-agent-id][data-renderer-target]')].map(b=>({id:b.dataset.agentId,selected:b.dataset.selected,label:b.getAttribute('aria-label')}))}; })()`;
async function state() { return JSON.parse(await evaluate(`JSON.stringify(${read})`)); }
async function waitFor(expression) {
  return evaluate(`(async()=>{const deadline=Date.now()+20000;while(Date.now()<deadline){if(${expression})return true;await new Promise(r=>setTimeout(r,50));}throw new Error('condition timed out: '+${JSON.stringify(expression)});})()`);
}
let runtimeInput;
let recoveryPhase="known";
// Local QA page uses the existing public mount/update ABI; no production hook,
// action endpoint or authorization path is added. Input originates at the gated fixture.
const runtimeHtml = `<!doctype html><html><head><style>html,body{margin:0;width:100%;height:100%;overflow:hidden}canvas{width:100%;height:100%}</style></head><body><canvas id="pixel-world-embedded-runtime-canvas"></canvas><script type="module">import{createPixelWorldBridge}from'/pixel-world-bridge/webgl2/pixel_world_bridge.js';window.events=[];window.fatal=null;window.s=await(await fetch('/material-runtime.json')).json();${runtimeBaselineProgram}window.bridge=await createPixelWorldBridge({onEvent:e=>{events.push(e);if(e.type==='camera_state_changed')window.camera=e.camera},onFatal:e=>window.fatal=e});window.mountResult=bridge.mount(document.querySelector('canvas'),s);</script></body></html>`;
const server = createServer((req, res) => {
  const pathname = new URL(req.url, 'http://localhost').pathname;
  if(pathname === '/material-runtime.json'){res.writeHead(200,{'Content-Type':'application/json'});res.end(JSON.stringify(runtimeInput));return;}
  if(pathname === '/material-runtime.html'){res.writeHead(200,{'Content-Type':'text/html'});res.end(runtimeHtml);return;}
  const file = resolve(root, '.' + (pathname==='/'?'/viewer.html':pathname));
  if(relative(root,file).startsWith('..')) {res.writeHead(403);res.end();return;}
  try { let bytes=readFileSync(file);if(scope==='recovery'&&extname(file)==='.html')bytes=Buffer.from(bytes.toString().replace('</head>',`<script>${materialRecoveryHook(recoveryPhase)}</script></head>`));res.writeHead(200,{'Content-Type':({'.js':'text/javascript','.wasm':'application/wasm','.html':'text/html','.css':'text/css'})[extname(file)]||'application/octet-stream','Cache-Control':'no-store'});res.end(bytes); } catch {res.writeHead(404);res.end();}
});
try {
 await new Promise(done=>server.listen(0,'127.0.0.1',done));lifecycle.prepare();
 const url=`http://127.0.0.1:${server.address().port}/viewer.html?test_api=1&connect=0&locale=zh&pixel_world_visual_fixture=material_shells`;
 if(scope==='recovery') {
  summary.controlledOverrides={source:'pixel-world-material-recovery.mjs HTTP fixture wrapper',freshInputPerReload:true};
  summary.limitations=['受控fixture重新装载当前输入，不代签真实服务授权或原操作身份','实际context loss与显式Reload；本项无正式P6预算声明'];
  await runMaterialRecovery({run,evaluate,state,waitFor,assert,out,url,requestedCase,summary,setPhase:p=>recoveryPhase=p,oracle});
 }
 if (scope === 'all' || scope === 'factory') for(const [name,w,h,d] of [['desktop',1440,1000,1],['narrow',390,844,1],['dpr2',1440,1000,2]].filter(([name]) => requestedCase === 'all' || requestedCase === name)) {
  await run(['open',url]);await run(['set','viewport',String(w),String(h),String(d)]);
  await evaluate(`setTimeout(()=>location.assign(${JSON.stringify(url)}),50);true`);await run(['wait','500']);await waitFor(`(${read}).ready`);
  await evaluate(`window.__MATERIAL_SNAPSHOT__=window.__OASIS7_PIXEL_WORLD_VISUAL_FIXTURES__.get('material_shells')();true`);
  await evaluate(`(()=>{const s=window.__MATERIAL_SNAPSHOT__;const auth=window.__AW_TEST__.getState();s.model.agent_player_bindings['agent-0']=auth.authPlayerId;s.model.agent_player_public_key_bindings['agent-0']=auth.authPublicKey;s.model.module_visual_entities['material-smelter'].anchor.data.pos.y_cm=3000000;s.model.module_visual_entities['material-unknown'].anchor.data.pos.y_cm=3000000;s.time+=1;window.__AW_TEST__.injectSnapshot(s);return true;})()`);await run(['wait','1000']);
  const before=await state();assert(before.canvas.width===w*d&&before.canvas.cssWidth===w,'actual backing/CSS mismatch');
  await run(['wait','1500']);await run(['screenshot','--full',resolve(out,name+'-material-initial.png')]);
  assert(before.dto.module_visual_entities.find(e=>e.id==='material-smelter')?.kind==='factory','factory identity missing');
  const row={name,scope:'factory',before,inspection:await evaluate(`JSON.stringify({dto:window.__OASIS7_PIXEL_WORLD_RENDER_DTO__(),buttons:[...document.querySelectorAll('button[data-renderer-target]')].map(b=>({data:{...b.dataset},rect:b.getBoundingClientRect().toJSON()}))})`)};
  row.anchor=JSON.parse(await evaluate(`JSON.stringify((()=>{const r=document.querySelector('button[data-module-id="material-smelter"][data-renderer-target]').getBoundingClientRect();const c=document.querySelector('#pixel-world-embedded-runtime-canvas');const cr=c.getBoundingClientRect();return {x:r.x+r.width/2,y:r.y+r.height/2,width:innerWidth,scroll:scrollY,canvasWidth:c.width,canvasHeight:c.height,cssWidth:cr.width,cssHeight:cr.height};})())`));
  for(const phase of ['unknown','restored','deleted','recreated']) {
   await evaluate(factoryPhaseProgram(phase));
   await waitFor(`(${read}).dto?.module_visual_entities?.some(e=>e.id==='material-smelter')===${phase!=='deleted'}`);await run(['wait','700']);await run(['screenshot','--full',resolve(out,name+'-material-'+phase+'.png')]);row[phase]=await state();
   if(phase==='unknown')assert(row[phase].dto.module_visual_entities.find(e=>e.id==='material-smelter').factory_kind==='unsupported','unknown update stale');
  }
  await evaluate(factoryFocusProgram);await run(['press','Enter']);await waitFor(factorySelectedExpression);row.keyboardSelection=await state();await run(['screenshot','--full',resolve(out,name+'-material-keyboard.png')]);
  row.pngOracle=oracle('factory', out, name, JSON.stringify(row.anchor));
  assert(row.pngOracle.passed, 'factory PNG/lifecycle oracle failed: '+JSON.stringify(row.pngOracle));
  summary.cases.push(row);
 }
 if (scope === 'all' || scope === 'terrain') for(const [name,w,h,d] of [['desktop',1440,1000,1],['narrow',390,844,1],['dpr2',1440,1000,2]].filter(([name]) => requestedCase === 'all' || requestedCase === name)) {
  await run(['open',url]);await run(['set','viewport',String(w),String(h),String(d)]);await evaluate(`setTimeout(()=>location.assign(${JSON.stringify(url)}),50);true`);await run(['wait','500']);await waitFor(`(${read}).ready`);
  await evaluate(`(()=>{const s=window.__OASIS7_PIXEL_WORLD_VISUAL_FIXTURES__.get('material_shells')();const auth=window.__AW_TEST__.getState();s.model.agent_player_bindings['agent-0']=auth.authPlayerId;s.model.agent_player_public_key_bindings['agent-0']=auth.authPublicKey;s.model.locations['loc-0'].pos={x_cm:3400000,y_cm:3300000,z_cm:0};s.model.module_visual_entities={};s.time++;window.__TERRAIN_BASE__=structuredClone(s);window.__AW_TEST__.injectSnapshot(s);return true;})()`);await run(['wait','1500']);
  const row={name,scope:'terrain',initial:await state()};await run(['screenshot','--full',resolve(out,name+'-terrain-initial.png')]);
  for(const phase of ['unknown','restored','deleted','recreated']) {
   await evaluate(terrainPhaseProgram(phase));await run(['wait','900']);row[phase]=await state();await run(['screenshot','--full',resolve(out,name+'-terrain-'+phase+'.png')]);
  }
  assert(row.initial.dto.fragment_terrain.some(b=>b.dominant_compound!=='unknown'),'known terrain absent');
  assert(row.unknown.dto.fragment_terrain.length>0 && row.unknown.dto.fragment_terrain.every(b=>b.dominant_compound==='unknown'),'unknown terrain stale');
  assert(row.deleted.dto.fragment_terrain.length===0,'deleted fragment remains in DTO');
  assert(JSON.stringify(row.initial.dto.fragment_terrain)===JSON.stringify(row.restored.dto.fragment_terrain) && JSON.stringify(row.initial.dto.fragment_terrain)===JSON.stringify(row.recreated.dto.fragment_terrain),'terrain DTO restoration differs');
  row.visualDeltaOracle=oracle('terrain', out, name);
  assert(row.visualDeltaOracle.passed, 'terrain lifecycle oracle failed: '+JSON.stringify(row.visualDeltaOracle));
  summary.cases.push(row);
 }
 if (scope === 'all' || scope === 'runtime') for (const [name,w,h,d] of [['desktop',1440,1000,1],['narrow',390,844,1],['dpr2',1440,1000,2],['narrow-dpr2',390,844,2]].filter(([name])=>requestedCase==='all'||requestedCase===name)) {
  await run(['open',url]); await run(['set','viewport',String(w),String(h),String(d)]);
  await evaluate(`setTimeout(()=>location.assign(${JSON.stringify(url)}),50);true`); await run(['wait','500']); await waitFor(`(${read}).ready`);
  const projected = (await state()).dto;
  const pos={x_cm:5000000,y_cm:5000000,z_cm:0};
  for (const material of ['ore','depot']) {
   runtimeInput=structuredClone(projected);
   for (const k of ['agents','locations','fragment_terrain','micro_depot_facilities','module_visual_entities','links','social_links','visual_hotspots','world_events','recent_event_hotspots','industry_aggregates']) runtimeInput[k]=[];
   for (const k of ['selection','receipt_target','recommended_target','active_intent_target','blocker_highlight','goal_highlight']) runtimeInput[k]=null;
   runtimeInput.world_bounds={width_cm:10000000,depth_cm:10000000,height_cm:1000000};
   runtimeInput.exploration_mode=true; runtimeInput.navigation={request_id:'material-isolated',mode:'overview',target:null};
   if(material==='ore') runtimeInput.fragment_terrain=[{id:'runtime-ore',location_id:'runtime',pos,footprint_cm:2000000,dominant_compound:'silicate_matrix',color:[255,255,255],emphasis:1}];
   else runtimeInput.micro_depot_facilities=[{...projected.micro_depot_facilities[0],id:'runtime-depot',facility_id:'runtime-depot',location_id:'runtime',pos,service_radius_cm:0,status:'active',available_units_by_kind:{iron:100},throughput_limit_units_per_epoch:100,throughput_remaining_units:100}];
   const original=structuredClone(runtimeInput);
   const runtimeUrl=`http://127.0.0.1:${server.address().port}/material-runtime.html`;
   await evaluate(`setTimeout(()=>location.assign(${JSON.stringify(runtimeUrl)}),50);true`); await run(['wait','500']); await waitFor(`window.mountResult?.status==='ready' && window.camera && !window.fatal`); await run(['wait','800']);
   const row={name,scope:'runtime-'+material,input:original,phases:{}};
   for(const phase of ['initial','absent','restored','deleted','recreated']) {
    const phaseProgram=runtimePhaseProgram(material,phase);
    if(phaseProgram!==null) await evaluate(phaseProgram);
    await run(['wait','700']);
    row.phases[phase]=JSON.parse(await evaluate(`JSON.stringify({input:s,camera,fatal,canvas:{width:document.querySelector('canvas').width,height:document.querySelector('canvas').height,cssWidth:document.querySelector('canvas').getBoundingClientRect().width,cssHeight:document.querySelector('canvas').getBoundingClientRect().height},viewport:{width:innerWidth,height:innerHeight},dpr:devicePixelRatio})`));
    assert(!row.phases[phase].fatal,'isolated runtime fatal');
    if(phase==='initial') assertRuntimeInitialInput(row.phases.initial.input,original);
    await run(['screenshot','--full',resolve(out,`${name}-runtime-${material}-${phase}.png`)]);
   }
   const originalProof=structuredClone(row.phases.initial.input);
   const absentProof=structuredClone(row.phases.absent.input);
   if(material==='ore') {
    assert(absentProof.fragment_terrain[0].dominant_compound==='unknown','ore negative control not unknown');
    absentProof.fragment_terrain[0].dominant_compound=originalProof.fragment_terrain[0].dominant_compound;
   } else absentProof.micro_depot_facilities=structuredClone(originalProof.micro_depot_facilities);
   assert(JSON.stringify(absentProof)===JSON.stringify(originalProof),'isolated negative control changed unrelated DTO fields');
   const deletedProof=structuredClone(row.phases.deleted.input);
   deletedProof.fragment_terrain=structuredClone(originalProof.fragment_terrain);
   deletedProof.micro_depot_facilities=structuredClone(originalProof.micro_depot_facilities);
   assert(JSON.stringify(deletedProof)===JSON.stringify(originalProof),'isolated deletion changed unrelated DTO fields');
   row.inputDifferenceProof={negativeControlOnly:material==='ore'?'fragment_terrain[0].dominant_compound':'micro_depot_facilities',deletionOnly:material==='ore'?'fragment_terrain':'micro_depot_facilities'};
   const a=row.phases.initial; assert(a.canvas.width===w*d && a.canvas.height===h*d,'isolated runtime backing mismatch');
   writeFileSync(resolve(out,`${name}-runtime-${material}-input.json`),JSON.stringify(row,null,2));
   row.oracle=oracle('runtime-'+material,out,name,JSON.stringify(a));
   summary.cases.push(row); assert(row.oracle.passed,'isolated material oracle failed: '+JSON.stringify(row.oracle));
  }
 }
 if(summary.cases.some(row=>row.scope==='runtime-ore') && summary.cases.some(row=>row.scope==='runtime-depot')) summary.limitations=['局部public runtime ABI不代签真实世界授权或生产Explorer流程','生产fixture的terrain delta与独立同色源PNG oracle分开报告','全组合恢复与完整性能预算未覆盖'];
 summary.status='passed_scope_only';
} catch(error){summary.status='failed';summary.error=String(error);process.exitCode=1;}
finally { try { writeFileSync(resolve(out,'summary.json'),JSON.stringify(summary,null,2)); console.log(out); } finally { lifecycle.close(); server.close(); } }
