import { createServer } from 'node:http';
import { readFileSync, writeFileSync, mkdirSync } from 'node:fs';
import { resolve, extname, relative } from 'node:path';
import { spawn, execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { createOwnedSessionLifecycle } from './agent-browser-visual-runner-lifecycle.mjs';

// Real WASM/WebGL evidence. Fixture results do not attest account authorization.
const root = resolve(process.env.AGENT_VISUAL_ARTIFACT_ROOT || resolve(import.meta.dirname, '../.viewer-test-dist'));
const out = resolve(import.meta.dirname, '../../../output/playwright/pixel-world-agent', new Date().toISOString().replace(/[:.]/g, '-'));
mkdirSync(out, { recursive: true });
const scope=process.env.AGENT_VISUAL_SCOPE || 'all';
if(!['all','interaction','recovery'].includes(scope)) throw Error('unsupported AGENT_VISUAL_SCOPE');
const session = `pixel-agent-${process.pid}`;
const command = process.env.AGENT_BROWSER_BIN || 'agent-browser';
const lifecycle = createOwnedSessionLifecycle({ command, session, prefixArgs: [] });
const summary = { source: execFileSync('git',['rev-parse','HEAD'],{cwd:resolve(root,'../../..'),encoding:'utf8'}).trim(), runnerSource: execFileSync('git',['rev-parse','HEAD'],{cwd:resolve(import.meta.dirname,'../../..'),encoding:'utf8'}).trim(), assets: Object.fromEntries(['viewer.js','pixel-world-bridge/webgl2/pixel_world_bridge_bindgen_bg.wasm'].map(path=>[path,createHash('sha256').update(readFileSync(resolve(root,path))).digest('hex')])), profile: 'visual-test', scope, expectedPipeline: process.env.AGENT_PIXEL_PIPELINE || 'native', cases: [] };
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
const read = `(() => { const canvas=document.querySelector('#pixel-world-embedded-runtime-canvas'); const gl=canvas?.getContext('webgl2'); const debug=gl?.getExtension('WEBGL_debug_renderer_info'); return {ready:document.querySelector('.pixel-world-canvas')?.dataset.rendererReady==='true', dto:window.__OASIS7_PIXEL_WORLD_RENDER_DTO__?.(), dpr:devicePixelRatio, viewport:{width:innerWidth,height:innerHeight}, canvas:canvas?{width:canvas.width,height:canvas.height,cssWidth:canvas.getBoundingClientRect().width,cssHeight:canvas.getBoundingClientRect().height}:null, gpu:debug?gl.getParameter(debug.UNMASKED_RENDERER_WEBGL):null, targets:[...document.querySelectorAll('button[data-agent-id][data-renderer-target]')].map(b=>({id:b.dataset.agentId,selected:b.dataset.selected,label:b.getAttribute('aria-label')}))}; })()`;
async function state() { return JSON.parse(await evaluate(`JSON.stringify(${read})`)); }
async function waitFor(expression) {
  return evaluate(`(async()=>{const deadline=Date.now()+20000;while(Date.now()<deadline){if(${expression})return true;await new Promise(r=>setTimeout(r,50));}throw new Error('condition timed out: '+${JSON.stringify(expression)});})()`);
}
async function verifyAgentPixels(path) {
  const anchor=JSON.parse(await evaluate(`JSON.stringify((()=>{const r=document.querySelector('button[data-agent-id="agent-0"][data-renderer-target]').getBoundingClientRect();return {x:r.x+r.width/2,y:r.y+r.height/2,width:innerWidth,scroll:scrollY,canvasWidth:document.querySelector('#pixel-world-embedded-runtime-canvas').width,canvasHeight:document.querySelector('#pixel-world-embedded-runtime-canvas').height,cssWidth:document.querySelector('#pixel-world-embedded-runtime-canvas').getBoundingClientRect().width,cssHeight:document.querySelector('#pixel-world-embedded-runtime-canvas').getBoundingClientRect().height};})())`));
  const program=String.raw`import sys,json,math
from PIL import Image
shot=Image.open(sys.argv[1]).convert('RGB');sprite=Image.open(sys.argv[2]).convert('RGBA');a=json.loads(sys.argv[3]);scale=shot.width/a['width'];f=max(1,min(a['canvasWidth']//640,a['canvasHeight']//360)) if sys.argv[4]=='integer' else 1;ux=f*a['cssWidth']/a['canvasWidth'] if sys.argv[4]=='integer' else 1;uy=f*a['cssHeight']/a['canvasHeight'] if sys.argv[4]=='integer' else 1;matched=total=0
for y in range(16):
 for x in range(16):
  rgba=sprite.getpixel((x,y))
  if rgba[3]!=255:continue
  px=math.floor((a['x']+(-8+x+0.5)*ux)*scale);py=math.floor((a['y']+a['scroll']+(-15+y+0.5)*uy)*scale)
  total+=1
  if 0<=px<shot.width and 0<=py<shot.height and all(abs(shot.getpixel((px,py))[c]-rgba[c])<=3 for c in range(3)):matched+=1
print(json.dumps({'matched':matched,'total':total,'ratio':matched/max(total,1),'scale':scale}))`;
  let measured;
  for(let attempt=0;attempt<20;attempt++) {
    measured=JSON.parse(execFileSync('python3',['-c',program,path,resolve(import.meta.dirname,'../../../doc/world-simulator/viewer/assets/ui-ux/pixel-world/agent.png'),JSON.stringify(anchor),summary.expectedPipeline],{encoding:'utf8'}));
    if(measured.ratio>=0.9) return {...measured,attempt};
    await new Promise(done=>setTimeout(done,200));
    await run(['screenshot','--full',path]);
  }
  throw new Error(`shared Agent pixels absent or misaligned after draw: ${JSON.stringify(measured)}`);
}
const server = createServer((req, res) => {
  const pathname = new URL(req.url, 'http://localhost').pathname;
  const file = resolve(root, '.' + (pathname==='/'?'/viewer.html':pathname));
  if(relative(root,file).startsWith('..')) {res.writeHead(403);res.end();return;}
  try { const bytes=readFileSync(file);res.writeHead(200,{'Content-Type':({'.js':'text/javascript','.wasm':'application/wasm','.html':'text/html','.css':'text/css'})[extname(file)]||'application/octet-stream','Cache-Control':'no-store'});res.end(bytes); } catch {res.writeHead(404);res.end();}
});
try {
  await new Promise(done=>server.listen(0,'127.0.0.1',done)); lifecycle.prepare();
  const url=`http://127.0.0.1:${server.address().port}/viewer.html?test_api=1&connect=0&locale=zh&pixel_world_visual_fixture=selected_blocker`;
  for(const [name,width,height] of [['desktop',1440,1000],['narrow',390,844],['dpr2',1440,1000]]) {
    await run(['set','viewport',String(width),String(height),name==='dpr2'?'2':'1']); await run(['open',url]); await run(['set','viewport',String(width),String(height)]);
    await waitFor(`(${read}).ready`);
    await evaluate(`(()=>{const s=window.__OASIS7_PIXEL_WORLD_VISUAL_FIXTURES__.get('selected_blocker')();const auth=window.__AW_TEST__.getState();const a=s.model.agents['agent-0'];s.model.agents['agent-1']={...a,id:'agent-1',name:'独立观察目标一号长中文名称',pos:{...a.pos,x_cm:a.pos.x_cm+2000000}};for(const id of ['agent-0','agent-1']){s.model.agent_player_bindings[id]=auth.authPlayerId;s.model.agent_player_public_key_bindings[id]=auth.authPublicKey;}s.time=13;window.__OASIS7_AGENT_TEST_SNAPSHOT__=s;window.__AW_TEST__.injectSnapshot(s);return true;})()`);
    await waitFor(`(${read}).dto?.agents?.some(a=>a.id==='agent-1')`);
    const before=await state(); summary.active={name,width,height,before}; assert(before.viewport.width===width&&before.viewport.height===height,'actual browser viewport differs from requested case'); assert(before.gpu,'missing GPU identity');
    if(name==='dpr2') assert(before.dpr===2,'DPR2 emulation did not take effect');
    const initialPng=resolve(out,`${name}-initial.png`);
    await run(['screenshot','--full',initialPng]);
    summary.active.initialPixels=await verifyAgentPixels(initialPng);
    let recovered=null;
    if(scope!=='interaction') {
    await evaluate(`(()=>{const c=document.querySelector('#pixel-world-embedded-runtime-canvas');const gl=c.getContext('webgl2');window.__OASIS7_AGENT_LOSS__=gl.getExtension('WEBGL_lose_context');if(!window.__OASIS7_AGENT_LOSS__)throw Error('WEBGL_lose_context unavailable');window.__OASIS7_AGENT_LOSS__.loseContext();return true;})()`);
    await waitFor(`document.querySelector('.pixel-world-render-unavailable__retry')`);
    assert((await state()).targets.length===0,'stale Agent targets survived context loss');
    await evaluate(`window.__OASIS7_AGENT_LOSS__.restoreContext();document.querySelector('.pixel-world-render-unavailable__retry').focus({preventScroll:true});true`);
    await run(['press','Enter']); await waitFor(`(${read}).ready`);
    recovered=await state();
    assert(recovered.dto.agents.some(agent=>agent.id==='agent-1'),'authorized Agent missing after renderer recovery');
    summary.active.recovered=recovered;
    const recoveredPng=resolve(out,`${name}-recovered.png`);
    await run(['screenshot','--full',recoveredPng]);
    summary.active.recoveredPixels=await verifyAgentPixels(recoveredPng);
    }
    if(scope==='recovery') { summary.cases.push({name,width,height,before,recovered}); continue; }
    const target='button[data-agent-id="agent-0"][data-renderer-target]';
    // Native browser mouse and Enter dispatch go through the real rendered target.
    const other='button[data-agent-id="agent-1"][data-renderer-target]';
    await evaluate(`document.querySelector(${JSON.stringify(other)}).focus({preventScroll:true})`);
    await run(['press','Enter']);
    await waitFor(`document.querySelector(${JSON.stringify(target)})?.dataset.selected==='false'`);
    await evaluate(`(async()=>{let last='',stable=0;for(let n=0;n<40;n++){const r=document.querySelector(${JSON.stringify(target)}).getBoundingClientRect();const key=[r.x,r.y,r.width,r.height].join(',');stable=key===last?stable+1:0;last=key;if(stable>=4)return true;await new Promise(done=>setTimeout(done,50));}throw Error('Agent target camera did not settle');})()`);
    await run(['click',target]); await waitFor(`document.querySelector(${JSON.stringify(target)})?.dataset.selected==='true'`);
    await evaluate(`document.querySelector(${JSON.stringify(other)}).focus({preventScroll:true})`); await run(['press','Enter']);
    await waitFor(`document.querySelector(${JSON.stringify(target)})?.dataset.selected==='false'`);
    await evaluate(`document.querySelector(${JSON.stringify(target)}).focus({preventScroll:true})`); await run(['press','Enter']);
    await waitFor(`document.querySelector(${JSON.stringify(target)})?.getAttribute('aria-pressed')==='true'`);
    await run(['screenshot','--full',resolve(out,`${name}.png`)]);
    const after=await state();
    await evaluate(`(()=>{const s=JSON.parse(JSON.stringify(window.__OASIS7_AGENT_TEST_SNAPSHOT__));s.time=14;s.model.agents['agent-1'].pos={...s.model.agents['agent-0'].pos};window.__AW_TEST__.injectSnapshot(s);return true;})()`);
    await waitFor(`(${read}).dto?.world_tick===14`);
    await evaluate(`document.querySelector(${JSON.stringify(other)}).focus({preventScroll:true})`); await run(['press','Enter']);
    await waitFor(`document.querySelector(${JSON.stringify(other)})?.dataset.selected==='true'`);
    const dense=await state();
    await evaluate(`(()=>{const s=JSON.parse(JSON.stringify(window.__OASIS7_AGENT_TEST_SNAPSHOT__));s.time=15;delete s.model.agents['agent-1'].pos;delete s.model.agents['agent-1'].location_id;window.__AW_TEST__.injectSnapshot(s);return true;})()`);
    await waitFor(`(${read}).dto?.agents?.some(a=>a.id==='agent-1'&&a.position_source==='missing')`);
    const missing=await state();
    await evaluate(`(()=>{const s=JSON.parse(JSON.stringify(window.__OASIS7_AGENT_TEST_SNAPSHOT__));s.time=16;delete s.model.agents['agent-1'];window.__AW_TEST__.injectSnapshot(s);return true;})()`);
    await waitFor(`!document.querySelector(${JSON.stringify(other)})&& !(${read}).dto?.agents?.some(a=>a.id==='agent-1')`);
    const removed=await state();
    await evaluate(`window.__AW_TEST__.injectSnapshot(window.__OASIS7_AGENT_TEST_SNAPSHOT__);true`);
    await waitFor(`(${read}).ready&&(${read}).dto?.agents?.some(a=>a.id==='agent-1'&&a.pos)`);
    const restored=await state();
    await evaluate(`window.__AW_TEST__.logoutHostedPlayerSession()`);
    await waitFor(`!(${read}).ready&&(${read}).targets.length===0&&!((${read}).dto?.agents?.length)`);
    const loggedOut=await state();
    await run(['screenshot','--full',resolve(out,`${name}-logged-out.png`)]);
    summary.cases.push({name,width,height,before,after,dense,missing,removed,restored,recovered,loggedOut,mouse:true,keyboard:true});
  }
  summary.status=scope==='all'?'passed':'passed_scope_only';
} catch(error) { summary.status='failed';summary.error=String(error);try{summary.failureState=await state();await run(['screenshot','--full',resolve(out,'failure.png')]);}catch{}throw error; }
finally {
  try { writeFileSync(resolve(out,'summary.json'),JSON.stringify(summary,null,2)); }
  finally { lifecycle.close(); await new Promise(done=>server.close(done)); }
  console.log(out);
}
