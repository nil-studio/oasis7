/**
 * Independent native CJK GPU probe; requires agent-browser and Python/Pillow.
 * AGENT_CJK_ARTIFACT_ROOT=/absolute/immutable/dist AGENT_CJK_SOURCE_HEAD=<commit>
 *   node scripts/pixel-world-cjk-visual-smoke.mjs
 * Optional AGENT_CJK_CASE=desktop|narrow|dpr2 (default all),
 * AGENT_CJK_MODE=cold-observation (one fresh desktop navigation, no P95 claim),
 * AGENT_CJK_OUTPUT=/absolute/new-directory, AGENT_BROWSER_BIN=agent-browser.
 * The gated connect=0 fixture never proves live authority or receipt identity.
 */
import { createServer } from 'node:http';
import { readFileSync, writeFileSync, mkdirSync, readdirSync } from 'node:fs';
import { dirname, resolve, relative, extname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawn, execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { createOwnedSessionLifecycle } from './agent-browser-visual-runner-lifecycle.mjs';

const repo = resolve(dirname(fileURLToPath(import.meta.url)), '../../..');
const assert = (ok, why) => { if (!ok) throw Error(why); };
assert(process.env.AGENT_CJK_ARTIFACT_ROOT, 'declare an immutable AGENT_CJK_ARTIFACT_ROOT');
assert(process.env.AGENT_CJK_SOURCE_HEAD, 'declare the actual build AGENT_CJK_SOURCE_HEAD');
const root = resolve(process.env.AGENT_CJK_ARTIFACT_ROOT);
const source = execFileSync('git', ['rev-parse', `${process.env.AGENT_CJK_SOURCE_HEAD}^{commit}`], { cwd: repo, encoding: 'utf8' }).trim();
const mode = process.env.AGENT_CJK_MODE || 'visual';
const requestedCase = process.env.AGENT_CJK_CASE || 'all';
assert(['visual', 'cold-observation'].includes(mode), 'invalid AGENT_CJK_MODE');
assert(['all', 'desktop', 'narrow', 'dpr2'].includes(requestedCase), 'invalid AGENT_CJK_CASE');
const out = resolve(process.env.AGENT_CJK_OUTPUT || resolve(repo, 'output/playwright/pixel-world-cjk', new Date().toISOString().replace(/[:.]/g, '-')));
mkdirSync(dirname(out), { recursive: true });
mkdirSync(out); // Never overwrite an earlier verification receipt.
const sha = bytes => createHash('sha256').update(bytes).digest('hex');
const files = dir => readdirSync(dir, { withFileTypes: true }).flatMap(e => e.isDirectory() ? files(resolve(dir, e.name)) : [resolve(dir, e.name)]);
const assetHashes = () => Object.fromEntries(files(root).sort().map(p => [relative(root, p), sha(readFileSync(p))]));
const assets = assetHashes();
const summary = { source, runnerHead: execFileSync('git', ['rev-parse', 'HEAD'], { cwd: repo, encoding: 'utf8' }).trim(), runnerSha256: sha(readFileSync(fileURLToPath(import.meta.url))), assets, mode, requestedCase, fixture: 'selected_blocker', liveConnection: false, cases: [], limitations: ['Controlled visual input; no real account, server revocation or operation/receipt identity proof.', 'Glyph difference checks the sampled native label band, not universal Unicode coverage.', 'Startup observations include actual loopback transfer and polling; not formal cold/P95/P6 acceptance. Browser disk/WASM caches and OS page cache are not independently inspected or purged.'] };
const command = process.env.AGENT_BROWSER_BIN || 'agent-browser';
const session = `pixel-cjk-${process.pid}`;
const lifecycle = createOwnedSessionLifecycle({ command, session });
const run = args => new Promise((yes, no) => {
  const child = spawn(command, ['--session', session, ...args]);
  let stdout = '', stderr = '';
  const timer = setTimeout(() => { child.kill('SIGTERM'); no(Error(`browser timeout: ${args[0]}`)); }, 45000);
  child.stdout.on('data', x => stdout += x); child.stderr.on('data', x => stderr += x);
  child.on('error', no); child.on('close', code => { clearTimeout(timer); code === 0 ? yes(stdout) : no(Error(stderr + stdout)); });
});
async function evaluate(code) {
  const r = JSON.parse(await run(['--json', 'eval', code]));
  assert(r.success, JSON.stringify(r)); return r.data.result;
}
const read = `(()=>{const c=document.querySelector('#pixel-world-embedded-runtime-canvas'),r=c?.getBoundingClientRect(),gl=c?.getContext('webgl2'),debug=gl?.getExtension('WEBGL_debug_renderer_info');return {ready:document.querySelector('.pixel-world-canvas')?.dataset.rendererReady==='true',dto:window.__OASIS7_PIXEL_WORLD_RENDER_DTO__?.(),dpr:devicePixelRatio,viewport:{width:innerWidth,height:innerHeight},gpu:debug?gl.getParameter(debug.UNMASKED_RENDERER_WEBGL):null,canvas:c?{width:c.width,height:c.height,cssWidth:r.width,cssHeight:r.height}:null,targets:[...document.querySelectorAll('button[data-agent-id][data-renderer-target]')].map(b=>{const r=b.getBoundingClientRect();return {id:b.dataset.agentId,selected:b.dataset.selected,x:r.x+r.width/2,y:r.y+r.height/2}})}})()`;
const state = async () => JSON.parse(await evaluate(`JSON.stringify(${read})`));
const waitFor = expression => evaluate(`(async()=>{const deadline=Date.now()+20000;while(Date.now()<deadline){if(${expression})return true;await new Promise(r=>setTimeout(r,50))}throw Error('condition timed out: '+${JSON.stringify(expression)})})()`);
const trackingHook = `<script>(()=>{const send=WebSocket.prototype.send;WebSocket.prototype.send=function(data){let type='unparsed';try{type=JSON.parse(data).type||'untyped'}catch{}const log=JSON.parse(sessionStorage.getItem('cjk-outbound')||'[]');log.push(type);sessionStorage.setItem('cjk-outbound',JSON.stringify(log.slice(-200)));return send.call(this,data)};const fetch=window.fetch;window.fetch=function(input,init){const log=JSON.parse(sessionStorage.getItem('cjk-http')||'[]');log.push({method:String(init?.method||input?.method||'GET').toUpperCase(),path:new URL(input?.url||input,location.href).pathname});sessionStorage.setItem('cjk-http',JSON.stringify(log.slice(-200)));return fetch.call(this,input,init)}})();</script>`;
const server = createServer((req, res) => {
  const pathname = new URL(req.url, 'http://localhost').pathname;
  const p = resolve(root, '.' + (pathname === '/' ? '/viewer.html' : pathname));
  if (relative(root, p).startsWith('..')) { res.writeHead(403); res.end(); return; }
  try {
    let bytes = readFileSync(p);
    if (extname(p) === '.html') bytes = Buffer.from(bytes.toString().replace('</head>', trackingHook + '</head>'));
    res.writeHead(200, { 'Content-Type': ({ '.html': 'text/html', '.js': 'text/javascript', '.wasm': 'application/wasm', '.css': 'text/css' })[extname(p)] || 'application/octet-stream', 'Cache-Control': 'no-store' }); res.end(bytes);
  } catch { res.writeHead(404); res.end(); }
});
async function installFixture() {
  await evaluate(`(()=>{const s=__OASIS7_PIXEL_WORLD_VISUAL_FIXTURES__.get('selected_blocker')(),auth=__AW_TEST__.getState(),a=s.model.agents['agent-0'];s.model.agents['agent-1']={...a,id:'agent-1',name:'Independent observer',pos:{...a.pos,x_cm:a.pos.x_cm+2000000}};for(const id of ['agent-0','agent-1']){s.model.agent_player_bindings[id]=auth.authPlayerId;s.model.agent_player_public_key_bindings[id]=auth.authPublicKey}s.time=13;window.__CJK_INPUT__=s;__AW_TEST__.injectSnapshot(s);return true})()`);
  await waitFor(`(${read}).dto?.agents?.some(a=>a.id==='agent-1')`);
}
async function captureLabel(name, phase, key, text) {
  await evaluate(`(()=>{const s=structuredClone(__CJK_INPUT__);s.model.agents['agent-0'].name=${JSON.stringify(text)};window.__CJK_INPUT__=s;__AW_TEST__.injectSnapshot(s);return true})()`);
  await waitFor(`(${read}).dto?.agents?.find(a=>a.id==='agent-0')?.label===${JSON.stringify(text)}`);
  // Allow the production low-power renderer and Text2d layout to consume the
  // snapshot before sampling; DTO publication alone is not a GPU draw fence.
  await run(['wait', '1200']);
  const shot = resolve(out, `${name}-${phase}-${key}.png`);
  await run(['screenshot', '--full', shot]);
  return { text, shot, state: await state() };
}
const glyphOracle = String.raw`import sys,json,math
from PIL import Image,ImageChops
a=Image.open(sys.argv[1]).convert('RGB');b=Image.open(sys.argv[2]).convert('RGB');s=json.loads(sys.argv[3]);t=next(v for v in s['targets'] if v['id']=='agent-0');c=s['canvas'];step=max(1,min(c['width']//640,c['height']//360));uy=step*c['cssHeight']/c['height'];scale=a.width/s['viewport']['width'];r=[t['x']-120,t['y']-15*uy-24,t['x']+120,t['y']-15*uy];roi=tuple(round(v*scale) for v in r);d=ImageChops.difference(a.crop(roi),b.crop(roi));print(json.dumps({'roiPhysical':roi,'changedPixels':sum(p!=(0,0,0) for p in d.getdata())}))`;
async function waitForDrawable(name, phase) {
  // rendererReady means the bridge initialized; a recreated WebGL context may
  // still have no submitted frame. Wait on actual shared atlas pixels before
  // comparing text, retaining all readiness samples without retrying input.
  const drawable = [];
  for (let attempt = 0; attempt < 30; attempt++) {
    const shot = resolve(out, `${name}-${phase}-draw-ready.png`);
    await run(['screenshot', '--full', shot]);
    const measured = JSON.parse(execFileSync('python3', ['-c', atlasOracle, shot, resolve(repo, 'doc/world-simulator/viewer/assets/ui-ux/pixel-world/agent.png'), JSON.stringify(await state())], { encoding: 'utf8' }));
    drawable.push(measured);
    if (measured.ratio >= .9) break;
    assert(attempt < 29, 'initialized bridge never produced actual atlas pixels');
    await run(['wait', '100']);
  }
  return drawable;
}
async function glyphPair(name, phase) {
  const drawable = await waitForDrawable(name, phase);
  const a = await captureLabel(name, phase, 'hanA', '中中中中AB123，。');
  const same = await captureLabel(name, phase, 'hanA-repeat', '中中中中AB123，。');
  const b = await captureLabel(name, phase, 'hanB', '国国国国AB123，。');
  const long = await captureLabel(name, phase, 'long', '独立观察目标一号长中文名称ABC123，。');
  const diff = x => JSON.parse(execFileSync('python3', ['-c', glyphOracle, a.shot, x.shot, JSON.stringify(a.state)], { encoding: 'utf8' }));
  const negative = diff(same), positive = diff(b);
  assert(negative.changedPixels === 0, 'same-label native ROI changed; glyph oracle is not isolated');
  assert(positive.changedPixels > 0, 'different Han labels produced identical native pixels (possible tofu)');
  return { drawable, a, same, b, long, negative, positive };
}
const atlasOracle = String.raw`import sys,json,math
from PIL import Image
im=Image.open(sys.argv[1]).convert('RGB');src=Image.open(sys.argv[2]).convert('RGBA');s=json.loads(sys.argv[3]);t=next(v for v in s['targets'] if v['id']=='agent-0');c=s['canvas'];step=max(1,min(c['width']//640,c['height']//360));ux=step*c['cssWidth']/c['width'];uy=step*c['cssHeight']/c['height'];scale=im.width/s['viewport']['width'];good=total=0
for y in range(16):
 for x in range(16):
  p=src.getpixel((x,y))
  if p[3]!=255:continue
  total+=1;px=math.floor((t['x']+(-8+x+.5)*ux)*scale);py=math.floor((t['y']+(-15+y+.5)*uy)*scale);good+=all(abs(im.getpixel((px,py))[k]-p[k])<=3 for k in range(3))
print(json.dumps({'matched':good,'total':total,'ratio':good/total}))`;
try {
  await new Promise(done => server.listen(0, '127.0.0.1', done)); lifecycle.prepare();
  const url = `http://127.0.0.1:${server.address().port}/viewer.html?test_api=1&connect=0&locale=zh&pixel_world_visual_fixture=selected_blocker`;
  const profiles = mode === 'cold-observation' ? [['desktop', 1440, 1000, 1]] : [['desktop', 1440, 1000, 1], ['narrow', 390, 844, 1], ['dpr2', 1440, 1000, 2]].filter(([n]) => requestedCase === 'all' || requestedCase === n);
  for (const [name, width, height, dpr] of profiles) {
    await run(['open', 'about:blank']); await run(['set', 'viewport', String(width), String(height), String(dpr)]);
    const started = performance.now(); await run(['open', url]); await waitFor(`(${read}).ready`);
    const row = { name, startup: { wallObservedRendererReadyMs: performance.now() - started, navigation: await evaluate(`({observedRendererReadyMs:performance.now(),resources:performance.getEntriesByType('resource').filter(e=>e.name.includes('.wasm')).map(e=>({path:new URL(e.name).pathname,duration:e.duration,transferSize:e.transferSize,encodedBodySize:e.encodedBodySize,decodedBodySize:e.decodedBodySize}))})`) }, before: await state() }; summary.cases.push(row);
    assert(row.before.gpu, 'missing actual GPU identity');
    assert(row.before.dpr === dpr && row.before.canvas.width === width * dpr && row.before.canvas.height === height * dpr, 'requested CSS/DPR framebuffer was not realized');
    if (mode === 'cold-observation') {
      row.drawable = await waitForDrawable(name, 'cold');
      row.startup.navigation.observedAtlasVisibleMs = await evaluate('performance.now()');
      row.startup.wallObservedAtlasVisibleMs = performance.now() - started;
      continue;
    }
    await installFixture(); row.initialGlyphs = await glyphPair(name, 'initial');
    const before = await evaluate(`({identity:[__AW_TEST__.getState().authPlayerId,__AW_TEST__.getState().authPublicKey],origin:performance.timeOrigin,receipt:__OASIS7_PIXEL_WORLD_RENDER_DTO__().receipt_target})`);
    assert(before.identity.every(x => typeof x === 'string' && x.length > 0), 'fixture identity empty');
    assert(before.receipt?.agent_id === 'agent-0', 'fixture receipt target not explicitly Agent A');
    await evaluate(`sessionStorage.setItem('cjk-outbound','[]');sessionStorage.setItem('cjk-http','[]');(()=>{const gl=document.querySelector('#pixel-world-embedded-runtime-canvas').getContext('webgl2'),loss=gl.getExtension('WEBGL_lose_context');if(!loss)throw Error('loss extension unavailable');loss.loseContext();return true})()`);
    await waitFor(`document.querySelector('.pixel-world-render-unavailable__retry')`);
    assert((await state()).targets.length === 0, 'stale targets survived actual context loss');
    assert(await evaluate('performance.timeOrigin') === before.origin, 'context loss automatically reloaded');
    await evaluate(`document.querySelector('.pixel-world-render-unavailable__retry').focus({preventScroll:true});true`); await run(['press', 'Enter']);
    await waitFor(`performance.timeOrigin!==${before.origin}&&(${read}).ready`);
    row.recovery = await evaluate(`({identity:[__AW_TEST__.getState().authPlayerId,__AW_TEST__.getState().authPublicKey],origin:performance.timeOrigin,receipt:__OASIS7_PIXEL_WORLD_RENDER_DTO__().receipt_target,outbound:JSON.parse(sessionStorage.getItem('cjk-outbound')||'[]'),http:JSON.parse(sessionStorage.getItem('cjk-http')||'[]')})`);
    assert(JSON.stringify(row.recovery.identity) === JSON.stringify(before.identity), 'fixture identity changed');
    assert(JSON.stringify(row.recovery.receipt) === JSON.stringify(before.receipt), 'fixture receipt target projection changed');
    assert(!row.recovery.outbound.some(t => ['control', 'gameplay_action', 'prompt_control', 'agent_chat'].includes(t)), 'reload sent a Command');
    assert(!row.recovery.http.some(r => ['POST', 'PUT', 'PATCH', 'DELETE'].includes(r.method)), 'reload sent a mutating HTTP request');
    await installFixture(); row.reloadedGlyphs = await glyphPair(name, 'reloaded');
    const target = id => `button[data-agent-id="${id}"][data-renderer-target]`;
    await run(['click', target('agent-1')]); await waitFor(`document.querySelector(${JSON.stringify(target('agent-1'))})?.dataset.selected==='true'`);
    await evaluate(`document.querySelector(${JSON.stringify(target('agent-0'))}).focus({preventScroll:true});true`); await run(['press', 'Enter']);
    await waitFor(`document.querySelector(${JSON.stringify(target('agent-0'))})?.dataset.selected==='true'`);
    await run(['wait', '300']); await evaluate(`document.querySelector('#pixel-world-embedded-runtime-canvas').focus({preventScroll:true});true`);
    const shot = resolve(out, `${name}-atlas.png`); await run(['screenshot', '--full', shot]);
    row.atlas = JSON.parse(execFileSync('python3', ['-c', atlasOracle, shot, resolve(repo, 'doc/world-simulator/viewer/assets/ui-ux/pixel-world/agent.png'), JSON.stringify(await state())], { encoding: 'utf8' }));
    assert(row.atlas.ratio >= .9, 'shared Agent atlas regression'); row.mouseSelection = true; row.keyboardSelection = true;
  }
  assert(JSON.stringify(assetHashes()) === JSON.stringify(assets), 'artifact tree changed during probe');
  summary.status = mode === 'cold-observation' ? 'observed_only' : 'passed_scope_only';
} catch (error) {
  summary.status = 'failed'; summary.error = String(error);
  try { summary.failureState = await state(); await run(['screenshot', '--full', resolve(out, 'failure.png')]); } catch {}
  throw error;
} finally {
  try { writeFileSync(resolve(out, 'summary.json'), JSON.stringify(summary, null, 2)); }
  finally { lifecycle.close(); await new Promise(done => server.close(done)); }
  console.log(out);
}
