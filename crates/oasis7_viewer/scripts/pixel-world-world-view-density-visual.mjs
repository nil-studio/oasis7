/** Controlled WS projection, not signed WorldService/P5 or a performance gate.
 * Requires the pinned ws devDependency, agent-browser, Python/Pillow.
 * DENSITY_QA_ARTIFACT_ROOT=/absolute/immutable/dist DENSITY_QA_SOURCE_HEAD=<commit>
 * node scripts/pixel-world-world-view-density-visual.mjs
 * Optional DENSITY_QA_SCOPE=baseline|all (default all), DENSITY_QA_DPR=1|2.
 * No snapshot injection, capability override, account/grant or renderer mutation.
 */
import { createServer } from 'node:http';
import { WebSocketServer } from 'ws';
import { readFileSync, writeFileSync, appendFileSync, mkdirSync, readdirSync } from 'node:fs';
import { resolve, dirname, relative, extname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawn, execFileSync } from 'node:child_process';
import { createHash, randomUUID } from 'node:crypto';
import { isDeepStrictEqual } from 'node:util';
import { createOwnedSessionLifecycle } from './agent-browser-visual-runner-lifecycle.mjs';
import { createDensityFixture } from './world-view-density-fixture.mjs';

const here = dirname(fileURLToPath(import.meta.url)), repo = resolve(here, '../../..');
const assert = (value, message) => { if (!value) throw Error(message); };
assert(process.env.DENSITY_QA_ARTIFACT_ROOT && process.env.DENSITY_QA_SOURCE_HEAD, 'declare immutable artifacts and actual source');
const root = resolve(process.env.DENSITY_QA_ARTIFACT_ROOT);
const source = execFileSync('git', ['rev-parse', process.env.DENSITY_QA_SOURCE_HEAD + '^{commit}'], { cwd: repo, encoding: 'utf8' }).trim();
const scope = process.env.DENSITY_QA_SCOPE || 'all', dpr = Number(process.env.DENSITY_QA_DPR || 1);
assert(['baseline', 'all'].includes(scope) && [1, 2].includes(dpr), 'unsupported scope/DPR');
const out = resolve(process.env.DENSITY_QA_OUTPUT || resolve(repo, 'output/playwright/world-view-density', new Date().toISOString().replace(/[:.]/g, '-')));
mkdirSync(dirname(out), { recursive: true }); mkdirSync(out);
const sha = bytes => createHash('sha256').update(bytes).digest('hex');
const files = dir => readdirSync(dir, { withFileTypes: true }).flatMap(e => e.isDirectory() ? files(resolve(dir, e.name)) : [resolve(dir, e.name)]);
const manifest = () => Object.fromEntries(files(root).sort().map(p => [relative(root, p), { bytes: readFileSync(p).length, sha256: sha(readFileSync(p)) }]));
const assets = manifest(), fixture = createDensityFixture();
const summary = { source, runnerHead: execFileSync('git', ['rev-parse', 'HEAD'], { cwd: repo, encoding: 'utf8' }).trim(), assets, scripts: Object.fromEntries(['pixel-world-world-view-density-visual.mjs', 'world-view-density-fixture.mjs', 'world-view-label-oracle.py'].map(n => [n, sha(readFileSync(resolve(here, n)))])), scope, dpr, controlledTransport: true, realServiceAuthority: false, phases: {}, limitations: ['Read-only controlled WS peer; not signed WorldService, genuine revocation, grants or P5 operation identity.', 'Text2d counts/layout are not GPU fences; paired canvas pixels are checked separately.', 'Single visual probe, not P6 timing/A-B/cold/long-stability acceptance.', 'A legacy capability downgrade is expected to clear/lock without a new authorization proof; this does not claim legitimate legacy reauthorization.'] };
const command = process.env.AGENT_BROWSER_BIN || 'agent-browser', session = `world-view-density-${process.pid}`;
const lifecycle = createOwnedSessionLifecycle({ command, session });
const run = args => new Promise((yes, no) => {
  const child = spawn(command, ['--session', session, ...args]); let stdout = '', stderr = '';
  const timer = setTimeout(() => { child.kill('SIGTERM'); no(Error('browser command timeout: ' + args[0])); }, 45000);
  child.stdout.on('data', x => stdout += x); child.stderr.on('data', x => stderr += x);
  child.on('error', no); child.on('close', code => { clearTimeout(timer); code === 0 ? yes(stdout) : no(Error(stderr + stdout)); });
});
async function evaluate(code) { const r = JSON.parse(await run(['--json', 'eval', code])); assert(r.success, JSON.stringify(r)); return r.data.result; }
const browserState = `(()=>{const c=document.querySelector('#pixel-world-embedded-runtime-canvas'),r=c?.getBoundingClientRect(),gl=c?.getContext('webgl2'),debug=gl?.getExtension('WEBGL_debug_renderer_info');return {ready:document.querySelector('.pixel-world-canvas')?.dataset.rendererReady==='true',metrics:window.__QA_READ_METRICS__?.()||{status:'uninitialized'},core:window.__AW_TEST__?.getState(),browser:{userAgent:navigator.userAgent,platform:navigator.platform},viewport:{width:innerWidth,height:innerHeight},dpr:devicePixelRatio,scrollX,scrollY,origin:performance.timeOrigin,gpu:debug?gl.getParameter(debug.UNMASKED_RENDERER_WEBGL):null,canvas:c?{left:r.left,top:r.top,cssWidth:r.width,cssHeight:r.height,width:c.width,height:c.height}:null,targets:[...document.querySelectorAll('.pixel-world-renderer-target')].map(b=>{const r=b.getBoundingClientRect();return{id:b.dataset.agentId||b.dataset.locationId||b.dataset.moduleId,selected:b.dataset.selected,x:r.x+r.width/2,y:r.y+r.height/2,width:r.width,height:r.height,inView:r.right>0&&r.bottom>0&&r.left<innerWidth&&r.top<innerHeight}})}})()`;
const state = async () => JSON.parse(await evaluate(`JSON.stringify(${browserState})`));
const waitFor = expression => evaluate(`(async()=>{const end=Date.now()+30000;while(Date.now()<end){if(${expression})return true;await new Promise(r=>setTimeout(r,100))}throw Error('condition timed out: '+${JSON.stringify(expression)})})()`);
let pauseReplies = false, holdLastPage = true, corruptNextCursorPage = false;
let held = [], lastOldPage = null;
const wire = [], http = [];
const connectionIds = new WeakMap();
const audit = row => { wire.push(row); appendFileSync(resolve(out, 'wire.jsonl'), JSON.stringify(row) + '\n'); };
const send = (socket, connection, response, replay = false) => {
  if (socket.readyState !== 1) return;
  audit({ direction: 'out', connection, replay, bytes: Buffer.byteLength(JSON.stringify(response)), message: response }); socket.send(JSON.stringify(response));
};
const httpServer = createServer((req, res) => {
  const pathname = new URL(req.url, 'http://localhost').pathname;
  http.push({ method: req.method, path: pathname });
  if (req.method !== 'GET') { res.writeHead(405); res.end(); return; }
  const p = resolve(root, '.' + (pathname === '/' ? '/viewer.html' : pathname));
  if (relative(root, p).startsWith('..')) { res.writeHead(403); res.end(); return; }
  try {
    let bytes = readFileSync(p);
    if (extname(p) === '.html') {
      const html = bytes.toString(), pattern = /<script\b[^>]*id=["']oasis7-viewer-runtime-config["'][^>]*>[\s\S]*?<\/script>/gi;
      assert((html.match(pattern) || []).length === 1, 'expected exactly one trusted runtime config');
      const config = { deploymentMode: 'hosted_public_join', viewerWsEndpoint: `ws://127.0.0.1:${httpServer.address().port}/qa-world-ws` };
      bytes = Buffer.from(html.replace(pattern, `<script id="oasis7-viewer-runtime-config" type="application/json">${JSON.stringify(config)}</script>`));
    }
    res.writeHead(200, { 'Content-Type': ({ '.html': 'text/html', '.js': 'text/javascript', '.css': 'text/css; charset=utf-8', '.wasm': 'application/wasm' })[extname(p)] || 'application/octet-stream', 'Cache-Control': 'no-store' }); res.end(bytes);
  } catch { res.writeHead(404); res.end(); }
});
const wss = new WebSocketServer({ server: httpServer, path: '/qa-world-ws', maxPayload: 512 * 1024 });
wss.on('connection', socket => {
  const connection = randomUUID();
  connectionIds.set(socket, connection);
  socket.on('message', bytes => {
    try {
      const message = JSON.parse(bytes.toString()); audit({ direction: 'in', connection, message });
      const response = fixture.reply(message, connection); if (!response) return;
      if (message.type === 'request_world_view' && message.query?.cursor && corruptNextCursorPage) {
        response.view.source_cut += '-mixed-negative'; corruptNextCursorPage = false;
        send(socket, connection, response); pauseReplies = true; return;
      }
      const isLast = response.type === 'world_view' && response.view.delivery === 'baseline' && response.view.entities?.length && !response.view.next_cursor;
      if (isLast) lastOldPage = { response: structuredClone(response), connection };
      if (pauseReplies || (holdLastPage && isLast)) { held.push({ socket, connection, response }); return; }
      send(socket, connection, response);
    } catch (error) { summary.protocolError = String(error); socket.close(1008, 'read-only QA contract violation'); }
  });
});
const release = () => { const queue = held; held = []; for (const x of queue) send(x.socket, x.connection, x.response); };
async function reconnect() { held = []; for (const socket of wss.clients) socket.close(1012, 'controlled source transition'); await run(['wait', '1400']); }
async function attachMetrics() {
  await evaluate(`(async()=>{const m=await import('/pixel-world-bridge/pixel_world_bridge.js');if(typeof m.readPixelWorldDisplayMetrics!=='function')throw Error('candidate lacks active-instance readonly metrics');window.__QA_READ_METRICS__=m.readPixelWorldDisplayMetrics;return true})()`);
}
async function waitProjection(count, cut = fixture.description().cut) {
  await waitFor(`(${browserState}).ready&&(${browserState}).metrics.status==='observed'&&(${browserState}).metrics.display_profile==='world_view_v1'&&(${browserState}).metrics.source_cut===${JSON.stringify(cut)}&&(${browserState}).metrics.projected_marker_count===${count}&&(${browserState}).targets.length===${count}`);
}
async function overview() {
  const disclosure = '.pixel-world-exploration-disclosure';
  await evaluate(`(()=>{const d=document.querySelector(${JSON.stringify(disclosure)});if(!d)throw Error('negotiated exploration UI missing');return d.open})()`)
    .then(async open => { if (!open) await run(['click', disclosure + ' > summary']); });
  await evaluate(`(()=>{const b=[...document.querySelectorAll('.pixel-world-exploration button')].find(b=>b.textContent.trim()==='World overview');if(!b)throw Error('overview action missing');b.focus({preventScroll:true});return true})()`); await run(['press', 'Enter']);
  await run(['click', disclosure + ' > summary']);
}
async function zoomForLabels() {
  for (let n = 0; n < 12; n++) {
    const s = await state(); if (s.core?.pixelWorldCamera?.zoom >= 1.75) return;
    const point = await evaluate(`(()=>{const c=document.querySelector('#pixel-world-embedded-runtime-canvas'),r=c.getBoundingClientRect();for(let y=1;y<20;y++)for(let x=1;x<20;x++){const px=r.left+r.width*x/20,py=r.top+r.height*y/20;if(document.elementFromPoint(px,py)===c)return{x:px,y:py}}throw Error('no unobscured canvas for native zoom')})()`);
    await run(['mouse', 'move', String(point.x), String(point.y)]); await run(['mouse', 'wheel', '-100']); await run(['wait', '200']);
  }
  throw Error('native zoom did not reach label producer threshold');
}
async function searchEmpty() {
  const disclosure = '.pixel-world-exploration-disclosure';
  const open = await evaluate(`document.querySelector(${JSON.stringify(disclosure)})?.open`);
  if (!open) await run(['click', disclosure + ' > summary']);
  await run(['fill', '.pixel-world-exploration form input', '']);
  await evaluate(`document.querySelector('.pixel-world-exploration form input').focus({preventScroll:true});true`); await run(['press', 'Enter']);
}
function actualGeometry(row) {
  const camera = row.core?.pixelWorldCamera;
  assert(camera && ['zoom', 'pan_x_px', 'pan_y_px'].every(key => Number.isFinite(camera[key])), 'actual camera observation missing');
  return { camera, viewport: row.viewport, canvas: row.canvas, dpr: row.dpr, scrollX: row.scrollX, scrollY: row.scrollY };
}
async function capture(name, labels = false) {
  await waitProjection(1000);
  if (labels) await waitFor(`(${browserState}).metrics.laid_out_in_view_labels===200&&(${browserState}).metrics.label_layouts?.length===200`);
  pauseReplies = true;
  try {
    await run(['wait', '1200']); const before = await state();
    const shot = resolve(out, name + '.png'); await run(['screenshot', '--full', shot]); const after = await state();
    assert(before.metrics.status === 'observed' && after.metrics.status === 'observed', 'capture borrowed an unobserved frame');
    assert(before.metrics.render_version === after.metrics.render_version && before.metrics.source_cut === after.metrics.source_cut, 'capture crossed render version/source cut');
    assert(isDeepStrictEqual(actualGeometry(before), actualGeometry(after)), 'capture crossed actual camera/viewport/CSS/backing/DPR/scroll');
    assert(before.targets.length === 1000 && before.targets.every(t => t.inView), 'not all1000 targets are in view');
    summary.phases[name] = before; return before;
  } finally { pauseReplies = false; release(); }
}
function sourceFor(capture) {
  const cut = capture.metrics.source_cut;
  const pages = wire.filter(row => row.direction === 'out' && !row.replay && row.message.type === 'world_view'
    && row.message.view.delivery === 'baseline' && row.message.view.source_cut === cut);
  const records = new Map(); let binding;
  for (const page of pages) {
    const view = page.message.view;
    const current = { world_id: view.world_id, scope_id: view.scope_id, world_bounds: view.world_bounds,
      source_binding: view.source_binding, reorg_epoch: view.reorg_epoch, source_frozen: view.source_frozen };
    if (binding) assert(isDeepStrictEqual(binding, current), 'source pages crossed authority/world/bounds');
    binding = current;
    for (const entity of view.entities) {
      const key = `${entity.kind}:${entity.entity_id}`;
      if (records.has(key)) assert(isDeepStrictEqual(records.get(key), entity), 'same cut contains conflicting source records');
      records.set(key, entity);
    }
  }
  assert(records.size === 1000, 'actual wire did not disclose1000 source records at capture cut');
  return { cut, binding, records: [...records.values()].sort((a, b) => `${a.kind}:${a.entity_id}`.localeCompare(`${b.kind}:${b.entity_id}`)),
    pages: pages.map(row => ({ connection: row.connection, request_generation: row.message.view.request_generation, bytes: row.bytes, entities: row.message.view.entities.length })) };
}
function labelOnlyInvariant(captures) {
  const normalized = (source, stripLabels = true) => ({ binding: source.binding, records: source.records.map(record => {
    const value = structuredClone(record);
    if (stripLabels) { delete value.label; delete value.render_record.name; }
    delete value.provenance.valid_at_source_cut; delete value.provenance.observed_at_tick;
    return value;
  }) });
  const inputs = Object.fromEntries(Object.entries(captures).map(([phase, capture]) => [phase, sourceFor(capture)]));
  assert(inputs.negative.records.every(row => row.label === '\u200b' && row.render_record.name === '\u200b'), 'negative source did not contain only zero-width label names');
  for (const phase of ['negative', 'restored']) {
    assert(isDeepStrictEqual(actualGeometry(captures.positive), actualGeometry(captures[phase])), 'label comparison changed actual camera/viewport/CSS/backing/DPR/scroll');
    assert(isDeepStrictEqual(normalized(inputs.positive), normalized(inputs[phase])), 'label source comparison changed non-label source facts');
  }
  assert(isDeepStrictEqual(normalized(inputs.positive, false), normalized(inputs.restored, false)), 'restored source did not restore original names/labels');
  writeFileSync(resolve(out, 'phase-source-inputs.json'), JSON.stringify(inputs, null, 2));
  return { geometryIdentical: true, sourceIdentityGeometryResourcesIdentical: true,
    allowedDifferences: ['entity.label', 'entity.render_record.name', 'source_cut', 'provenance.valid_at_source_cut', 'provenance.observed_at_tick'], entityCounts: Object.fromEntries(Object.entries(inputs).map(([phase, input]) => [phase, input.records.length])) };
}
try {
  await new Promise(done => httpServer.listen(0, '127.0.0.1', done)); lifecycle.prepare();
  const url = `http://127.0.0.1:${httpServer.address().port}/viewer.html?test_api=1&connect=1&hosted_bootstrap=0&locale=en`;
  await run(['open', 'about:blank']); await run(['set', 'viewport', '1440', '1000', String(dpr)]); await run(['open', url]); await attachMetrics();
  await waitProjection(800); summary.partial = await state();
  assert(held.some(x => x.response.view?.entities?.length === 200), 'last200-page was not actually held');
  holdLastPage = false; release(); await overview(); await waitProjection(1000); await zoomForLabels(); await waitProjection(1000);
  const css = await evaluate(`(async()=>{const l=[...document.querySelectorAll('link[rel=stylesheet]')].find(l=>l.href.includes('viewer_terminal_shell.css')),r=l?await fetch(l.href):null,t=document.querySelector('.pixel-world-renderer-target:not([data-selected=true])'),s=t?getComputedStyle(t):null;return{status:r?.status,mime:r?.headers.get('content-type'),rules:l?.sheet?.cssRules.length,background:s?.backgroundColor,border:s?.borderTopColor,shadow:s?.boxShadow}})()`);
  assert(css.status === 200 && css.mime?.startsWith('text/css') && css.rules > 0 && css.background === 'rgba(0, 0, 0, 0)' && css.border === 'rgba(0, 0, 0, 0)' && css.shadow === 'none', 'canonical style prerequisite failed'); summary.css = css;
  const initial = await state(); assert(initial.gpu && initial.dpr === dpr && initial.canvas.width === 1440 * dpr && initial.canvas.height === 1000 * dpr, 'actual GPU/CSS/backing mismatch');
  const labelCaptures = { positive: await capture('labels-positive', true) };
  fixture.setLabelStyle('empty'); await waitProjection(1000); labelCaptures.negative = await capture('labels-negative');
  fixture.setLabelStyle('normal'); await waitProjection(1000); labelCaptures.restored = await capture('labels-restored', true);
  writeFileSync(resolve(out, 'label-captures.json'), JSON.stringify(labelCaptures, null, 2));
  summary.labelOnlyInvariant = labelOnlyInvariant(labelCaptures);
  summary.labelOracle = JSON.parse(execFileSync('python3', [resolve(here, 'world-view-label-oracle.py'), out], { encoding: 'utf8' })); assert(summary.labelOracle.passed, '200 native label paired PNG oracle failed');
  if (scope === 'all') {
    const selector = 'button[data-agent-id="qa-agent-0000"][data-renderer-target]';
    await run(['click', selector]); await waitFor(`document.querySelector(${JSON.stringify(selector)})?.dataset.selected==='true'`);
    fixture.remove('qa-agent-0000'); await waitProjection(999); assert(!(await state()).targets.some(t => t.id === 'qa-agent-0000'), 'deleted projection target retained'); summary.removal = await state();
    fixture.restore(); await waitProjection(1000); summary.recreated = await state();
    corruptNextCursorPage = true; await searchEmpty();
    await waitFor(`document.querySelector('.pixel-world-exploration [role=status]')?.textContent.includes('Read unavailable')`);
    await waitFor(`(${browserState}).metrics.status==='observed'&&(${browserState}).metrics.projected_marker_count===200`);
    summary.mixedCutRejected = await state();
    pauseReplies = false; release(); await overview(); await waitProjection(1000);
    const old = lastOldPage; fixture.newWorld(); await reconnect(); await waitProjection(1000); summary.worldB = await state();
    assert(old?.response && old.connection, 'missing actual old-connection response for replay');
    for (const socket of wss.clients) send(socket, connectionIds.get(socket), old.response,
      { kind: 'old_response_on_current_socket', originalConnection: old.connection, originalRequestGeneration: old.response.view.request_generation });
    await run(['wait', '300']); assert((await state()).metrics.source_cut === fixture.description().cut, 'old response crossed new world source fence');
    fixture.revoke(); await reconnect(); await waitFor(`(${browserState}).targets.length===0&&!((${browserState}).metrics.projected_marker_count>0)`); summary.denied = await state();
    fixture.restoreWorldView(); await reconnect(); await waitProjection(1000); summary.restoredScope = await state();
    fixture.setLegacy(); await reconnect(); await waitFor(`(${browserState}).targets.length===0&&!((${browserState}).metrics.projected_marker_count>0)`); summary.legacyDowngrade = { state: await state(), claim: 'cleared/locked; no legitimate legacy reauthorization proof supplied' };
    fixture.restoreWorldView(); await reconnect(); await waitProjection(1000);
    const origin = await evaluate('performance.timeOrigin');
    await evaluate(`(()=>{const gl=document.querySelector('#pixel-world-embedded-runtime-canvas').getContext('webgl2'),loss=gl.getExtension('WEBGL_lose_context');if(!loss)throw Error('loss extension unavailable');loss.loseContext();return true})()`);
    await waitFor(`document.querySelector('.pixel-world-render-unavailable__retry')&&(${browserState}).targets.length===0`); assert(await evaluate('performance.timeOrigin') === origin, 'automatic page reload occurred');
    await evaluate(`document.querySelector('.pixel-world-render-unavailable__retry').focus({preventScroll:true});true`); await run(['press', 'Enter']);
    await waitFor(`performance.timeOrigin!==${origin}`); await attachMetrics(); await waitProjection(1000); await overview(); await zoomForLabels(); summary.reloaded = await capture('reloaded', true);
  }
  assert(!summary.protocolError, summary.protocolError);
  assert(!http.some(r => ['POST', 'PUT', 'PATCH', 'DELETE'].includes(r.method)), 'unexpected mutating HTTP');
  assert(wire.filter(x => x.direction === 'out' && x.message.type === 'snapshot').every(x => Object.keys(x.message.snapshot.model.agents).length === 0), 'density was injected via snapshot');
  assert(JSON.stringify(manifest()) === JSON.stringify(assets), 'immutable artifacts changed'); summary.status = 'passed_scope_only';
} catch (error) {
  summary.status = 'failed'; summary.error = String(error);
  try { summary.failureState = await state(); await run(['screenshot', '--full', resolve(out, 'failure.png')]); } catch {}
  throw error;
} finally {
  try { summary.fixture = fixture.description(); summary.http = http; writeFileSync(resolve(out, 'summary.json'), JSON.stringify(summary, null, 2)); }
  finally { lifecycle.close(); for (const socket of wss.clients) socket.terminate(); await new Promise(done => wss.close(done)); await new Promise(done => httpServer.close(done)); }
  console.log(out);
}
