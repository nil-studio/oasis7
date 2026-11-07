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
import { measureDensityPerformance } from './world-view-density-performance.mjs';
import { probePendingWorldSelection } from './world-view-pending-selection-probe.mjs';

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
const composedManifest = process.env.DENSITY_QA_COMPOSED_MANIFEST ? JSON.parse(readFileSync(process.env.DENSITY_QA_COMPOSED_MANIFEST)) : null;
if (composedManifest) assert(composedManifest.source === source && isDeepStrictEqual(composedManifest.files, assets), 'composed artifact provenance mismatch');
const summary = { source, runnerHead: execFileSync('git', ['rev-parse', 'HEAD'], { cwd: repo, encoding: 'utf8' }).trim(), assets, scripts: Object.fromEntries(['pixel-world-world-view-density-visual.mjs', 'world-view-density-fixture.mjs', 'world-view-label-oracle.py', 'world-view-density-performance.mjs', 'viewer-performance-native-input.mjs', 'viewer-performance-paint-trace.mjs', 'world-view-pending-selection-probe.mjs'].map(n => [n, sha(readFileSync(resolve(here, n)))])), scope, dpr, controlledTransport: true, realServiceAuthority: false, observerDebugProbe: { deploymentMode: 'trusted_local_only', hostname: 'density-qa.localhost', hostedGate: 'hosted_public_join requires real login; this probe does not validate the ordinary player entry' }, phases: {}, limitations: ['Read-only controlled WS peer; not signed WorldService, genuine revocation, grants or P5 operation identity.', 'Text2d counts/layout are not GPU fences; paired canvas pixels are checked separately.', 'Single visual probe, not P6 timing/A-B/cold/long-stability acceptance.', 'A legacy capability downgrade is expected to clear/lock without a new authorization proof; this does not claim legitimate legacy reauthorization.'] };
const command = process.env.AGENT_BROWSER_BIN || 'agent-browser', session = `world-view-density-${process.pid}`;
summary.artifactProvenance = composedManifest;
const lifecycle = createOwnedSessionLifecycle({ command, session });
const run = args => new Promise((yes, no) => {
  summary.lastBrowserCommand = args;
  const child = spawn(command, ['--session', session, ...args]); let stdout = '', stderr = '';
  const timer = setTimeout(() => { child.kill('SIGTERM'); no(Error('browser command timeout: ' + args[0])); }, 45000);
  child.stdout.on('data', x => stdout += x); child.stderr.on('data', x => stderr += x);
  child.on('error', no); child.on('close', code => { clearTimeout(timer); if (code === 0) yes(stdout); else { summary.failedBrowserCommand ||= { args, stderr, stdout, code }; no(Error(stderr + stdout)); } });
});
async function evaluate(code) { const r = JSON.parse(await run(['--json', 'eval', code])); assert(r.success, JSON.stringify(r)); return r.data.result; }
const browserState = `(()=>{const c=document.querySelector('#pixel-world-embedded-runtime-canvas'),r=c?.getBoundingClientRect(),gl=c?.getContext('webgl2'),debug=gl?.getExtension('WEBGL_debug_renderer_info');return {ready:document.querySelector('.pixel-world-canvas')?.dataset.rendererReady==='true',metrics:window.__QA_READ_METRICS__?.()||{status:'uninitialized'},core:window.__AW_TEST__?.getState(),browser:{userAgent:navigator.userAgent,platform:navigator.platform},viewport:{width:innerWidth,height:innerHeight},dpr:devicePixelRatio,scrollX,scrollY,origin:performance.timeOrigin,gpu:debug?gl.getParameter(debug.UNMASKED_RENDERER_WEBGL):null,canvas:c?{left:r.left,top:r.top,cssWidth:r.width,cssHeight:r.height,width:c.width,height:c.height}:null,targets:[...document.querySelectorAll('.pixel-world-renderer-target')].map(b=>{const r=b.getBoundingClientRect();return{id:b.dataset.agentId||b.dataset.locationId||b.dataset.moduleId,selected:b.dataset.selected,x:r.x+r.width/2,y:r.y+r.height/2,width:r.width,height:r.height,inView:r.right>0&&r.bottom>0&&r.left<innerWidth&&r.top<innerHeight}})}})()`;
const state = async () => JSON.parse(await evaluate(`JSON.stringify(${browserState})`));
const waitFor = expression => evaluate(`(async()=>{const end=Date.now()+30000;while(Date.now()<end){const error=window.__AW_TEST__?.getState()?.lastError;if(typeof error==='string'&&error.startsWith('window.error:'))throw Error(error);if(${expression})return true;await new Promise(r=>setTimeout(r,100))}throw Error('condition timed out: '+${JSON.stringify(expression)})})()`);
let pauseReplies = false, pauseAfterComplete = false, holdLastPage = true, corruptNextCursorPage = false;
let held = [], lastOldPage = null;
let pendingGate = null;
function armPendingGate(search, onThird) {
  assert(!pendingGate, 'pending gate already armed');
  let firstDone, thirdDone;
  const gate = { search, generation: null, pages: [], first: null, third: null,
    firstReceived: new Promise(resolve => firstDone = resolve), thirdReceived: new Promise(resolve => thirdDone = resolve),
    releaseFirst() { assert(gate.first, 'first page not held'); const x = gate.first; gate.first = null; send(x.socket, x.connection, x.response); },
    releaseThird() { assert(gate.third, 'third page not held'); const x = gate.third; gate.third = null; send(x.socket, x.connection, x.response); },
    releaseThirdFresh() { assert(gate.third, 'third page not held'); const x = gate.third; gate.third = null; const response = fixture.reply(x.request, x.connection); send(x.socket, x.connection, response); return response; },
    replayThirdOnCurrentSockets() { assert(gate.third, 'third page not held'); const x = gate.third; for (const socket of wss.clients) send(socket, connectionIds.get(socket), x.response, { kind: 'old_pending_response_on_current_socket', originalConnection: x.connection, originalRequestGeneration: x.request.query.request_generation }); },
    dispose() { pendingGate = null; }, firstDone, thirdDone, onThird };
  gate.firstDone = firstDone; gate.thirdDone = thirdDone; pendingGate = gate; return gate;
}
const wire = [], http = [];
const connectionIds = new WeakMap();
const latestQueryByConnection = new Map();
const audit = row => { wire.push(row); appendFileSync(resolve(out, 'wire.jsonl'), JSON.stringify(row) + '\n'); };
const send = (socket, connection, response, replay = false) => {
  if (socket.readyState !== 1) return;
  audit({ direction: 'out', connection, replay, bytes: Buffer.byteLength(JSON.stringify(response)), message: response }); socket.send(JSON.stringify(response));
  if (pauseAfterComplete && response.type === 'world_view' && response.view.completeness === 'complete'
    && ['baseline', 'unchanged'].includes(response.view.delivery)
    && response.view.request_generation?.split(':')[0] === latestQueryByConnection.get(connection)) { pauseAfterComplete = false; pauseReplies = true; }
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
      const configCount = (html.match(pattern) || []).length;
      assert(configCount <= 1, 'duplicate trusted runtime config');
      // Controlled observer/debug probe only, outside the ordinary player flow.
      // This owned localhost subdomain
      // is not the exact-loopback automatic local-player issuance lane.
      const config = { deploymentMode: 'trusted_local_only', viewerWsEndpoint: `ws://density-qa.localhost:${httpServer.address().port}/qa-world-ws` };
      const configScript = `<script id="oasis7-viewer-runtime-config" type="application/json">${JSON.stringify(config)}</script>`;
      bytes = Buffer.from(configCount ? html.replace(pattern, configScript) : html.replace('</head>', configScript + '</head>'));
    }
    res.writeHead(200, { 'Content-Type': ({ '.html': 'text/html', '.js': 'text/javascript', '.css': 'text/css; charset=utf-8', '.wasm': 'application/wasm' })[extname(p)] || 'application/octet-stream', 'Cache-Control': 'no-store' }); res.end(bytes);
  } catch (error) { if (pathname.endsWith('.html')) summary.entryError = String(error); res.writeHead(404); res.end(); }
});
const wss = new WebSocketServer({ server: httpServer, path: '/qa-world-ws', maxPayload: 512 * 1024 });
wss.on('connection', socket => {
  const connection = randomUUID();
  connectionIds.set(socket, connection);
  socket.on('message', bytes => {
    try {
      const message = JSON.parse(bytes.toString()); audit({ direction: 'in', connection, message });
      if (message.type === 'request_world_view') latestQueryByConnection.set(connection, message.query?.request_generation?.split(':')[0]);
      const response = fixture.reply(message, connection); if (!response) return;
      if (pendingGate && message.type === 'request_world_view' && message.query.search === pendingGate.search && response.view?.delivery === 'baseline') {
        const gate = pendingGate, generation = message.query.request_generation.split(':')[0];
        if (!gate.generation && !message.query.cursor) gate.generation = generation;
        if (generation === gate.generation) {
          const page = { socket, connection, response, request: structuredClone(message), receivedAt: Date.now() };
          gate.pages.push({ connection, request: page.request, response: structuredClone(response), receivedAt: page.receivedAt });
          if (gate.pages.length === 1) { gate.first = page; gate.firstDone(); return; }
          if (gate.pages.length === 3) { gate.third = page; Promise.resolve(gate.onThird(page)).then(gate.thirdDone, error => { gate.inputError = String(error); gate.thirdDone(); }); return; }
          send(socket, connection, response); return;
        }
      }
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
async function waitNegotiation(worldView) {
  await waitFor(`(${browserState}).core?.connectionStatus==='connected'&&(${browserState}).core?.viewerProtocol?.negotiated===true&&(${browserState}).core?.worldId===${JSON.stringify(fixture.description().world)}&&(${browserState}).core?.viewerProtocol?.capabilities?.includes('world_view')===${worldView}`);
}
async function attachMetrics() {
  await evaluate(`(async()=>{const m=await import('/pixel-world-bridge/pixel_world_bridge.js');if(typeof m.readPixelWorldDisplayMetrics!=='function')throw Error('candidate lacks active-instance readonly metrics');window.__QA_READ_METRICS__=m.readPixelWorldDisplayMetrics;return true})()`);
}
async function waitProjection(count, cut = fixture.description().cut) {
  await waitFor(`(${browserState}).ready&&(${browserState}).metrics.status==='observed'&&(${browserState}).metrics.display_profile==='world_view_v1'&&(${browserState}).metrics.source_cut===${JSON.stringify(cut)}&&(${browserState}).metrics.projected_marker_count===${count}&&(${browserState}).targets.length===${count}`);
}
async function recoveredScopeState(count, selectedId = null) {
  const expected = fixture.description();
  const row = JSON.parse(await evaluate(`(async()=>{const end=Date.now()+30000;let s;while(Date.now()<end){s=${browserState};const m=s.metrics;if(s.core?.lastError?.startsWith('window.error:'))throw Error(s.core.lastError);if(s.ready&&s.core?.worldId===${JSON.stringify(expected.world)}&&m.status==='observed'&&m.display_profile==='world_view_v1'&&m.source_cut===${JSON.stringify(expected.cut)}&&m.projected_marker_count===${count}&&m.agent_body_sprites===${count}&&m.in_view_agent_body_sprites===${count}&&s.targets.length===${count}&&(${selectedId === null ? 'true' : `s.targets.some(t=>t.id===${JSON.stringify(selectedId)}&&t.selected==='true')`}))return JSON.stringify(s);await new Promise(r=>setTimeout(r,50))}throw Error('recovered scope frame did not settle: '+JSON.stringify(s))})()`));
  assertGuest(row); return row;
}
async function overview() {
  const disclosure = '.pixel-world-exploration-disclosure';
  await evaluate(`(()=>{const d=document.querySelector(${JSON.stringify(disclosure)});if(!d)throw Error('negotiated exploration UI missing');return d.open})()`)
    .then(async open => { if (!open) await run(['click', disclosure + ' > summary']); });
  await evaluate(`(()=>{const b=[...document.querySelectorAll('.pixel-world-exploration button')].find(b=>b.textContent.trim()==='World overview');if(!b)throw Error('overview action missing');b.focus({preventScroll:true});return true})()`); await run(['press', 'Enter']);
  await run(['click', disclosure + ' > summary']);
}
async function zoomForLabels() {
  summary.zoomSteps ||= [];
  for (let n = 0; n < 12; n++) {
    const s = await state(); summary.zoomSteps.push({ camera: s.core?.pixelWorldCamera, canvas: s.canvas, markers: s.metrics.projected_marker_count });
    if (s.core?.pixelWorldCamera?.zoom >= 1.75) return;
    const point = await evaluate(`(()=>{const c=document.querySelector('#pixel-world-embedded-runtime-canvas'),r=c.getBoundingClientRect(),points=[];for(let y=1;y<20;y++)for(let x=1;x<20;x++){const px=r.left+r.width*x/20,py=r.top+r.height*y/20;if(document.elementFromPoint(px,py)===c)points.push({x:px,y:py,d:(x-10)**2+(y-10)**2})}points.sort((a,b)=>a.d-b.d);if(points.length)return points[0];throw Error('no unobscured canvas for native zoom')})()`);
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
async function centerWithNativePan() {
  const before = await state(), camera = before.core?.pixelWorldCamera;
  assert(camera && Number.isFinite(camera.pan_x_px) && Number.isFinite(camera.pan_y_px), 'native pan lacks actual camera');
  const dx = -camera.pan_x_px, dy = -camera.pan_y_px;
  if (Math.abs(dx) < .5 && Math.abs(dy) < .5) return;
  const point = await evaluate(`(()=>{const c=document.querySelector('#pixel-world-embedded-runtime-canvas'),r=c.getBoundingClientRect(),points=[];for(let y=1;y<20;y++)for(let x=1;x<20;x++){const px=r.left+r.width*x/20,py=r.top+r.height*y/20,ex=px+${dx},ey=py+${dy};if(ex>r.left&&ex<r.right&&ey>r.top&&ey<r.bottom&&document.elementFromPoint(px,py)===c)points.push({x:px,y:py,d:(x-10)**2+(y-10)**2})}points.sort((a,b)=>a.d-b.d);if(points.length)return points[0];throw Error('no actual canvas start for native centering pan')})()`);
  const input = { x: Math.round(point.x), y: Math.round(point.y), endX: Math.round(point.x + dx), endY: Math.round(point.y + dy) };
  assert(await evaluate(`(()=>{const c=document.querySelector('#pixel-world-embedded-runtime-canvas'),r=c.getBoundingClientRect();return document.elementFromPoint(${input.x},${input.y})===c&&${input.endX}>r.left&&${input.endX}<r.right&&${input.endY}>r.top&&${input.endY}<r.bottom})()`), 'integer native pan coordinates lost the actual canvas hit/bounds');
  summary.pendingNativePan = { point, dx, dy, input, actualDelta: { x: input.endX - input.x, y: input.endY - input.y }, before: camera };
  await run(['mouse', 'move', String(input.x), String(input.y)]); await run(['mouse', 'down']);
  await run(['mouse', 'move', String(input.endX), String(input.endY)]); await run(['mouse', 'up']);
  await run(['wait', '500']);
  const after = await state(); summary.nativeCenteringPans ||= []; summary.nativeCenteringPans.push({ point, dx, dy, before: camera, after: after.core?.pixelWorldCamera });
  assert(after.core?.pixelWorldCamera?.zoom === camera.zoom && Math.abs(after.core.pixelWorldCamera.pan_x_px) < 1 && Math.abs(after.core.pixelWorldCamera.pan_y_px) < 1, 'real canvas drag did not center the observed camera');
}
function actualGeometry(row) {
  const camera = row.core?.pixelWorldCamera;
  assert(camera && ['zoom', 'pan_x_px', 'pan_y_px'].every(key => Number.isFinite(camera[key])), 'actual camera observation missing');
  return { camera, viewport: row.viewport, canvas: row.canvas, dpr: row.dpr, scrollX: row.scrollX, scrollY: row.scrollY };
}
function assertGuest(row) {
  assert(row.core?.authReady === false && row.core?.authPlayerId === null && row.core?.authPublicKey === null
    && row.core?.authRegistrationStatus === 'guest' && row.core?.authBoundAgentId === null, 'controlled observer probe acquired a player identity');
}
async function freezeCompleteBoundary() {
  pauseAfterComplete = true;
  const deadline = Date.now() + 10000;
  while (!pauseReplies && Date.now() < deadline) await new Promise(done => setTimeout(done, 20));
  assert(pauseReplies && !pauseAfterComplete, 'no complete wire boundary reached for observation');
  await waitProjection(1000);
}
async function capture(name, labels = false) {
  await waitProjection(1000);
  if (labels) await waitFor(`(${browserState}).metrics.laid_out_in_view_labels===200&&(${browserState}).metrics.label_layouts?.length===200`);
  // Freeze future source replies only after a real complete response. Pausing
  // halfway through a new baseline would make the screenshot a partial page.
  try {
    await freezeCompleteBoundary();
    await run(['wait', '1200']); const before = await state();
    const shot = resolve(out, name + '.png'); await run(['screenshot', '--full', shot]); const after = await state();
    assertGuest(before); assertGuest(after);
    assert(before.metrics.status === 'observed' && after.metrics.status === 'observed', 'capture borrowed an unobserved frame');
    assert([before, after].every(row => row.metrics.agent_body_sprites === 1000 && row.metrics.in_view_agent_body_sprites === 1000), 'actual rendered/in-view Agent body count was not1000');
    assert(before.metrics.render_version === after.metrics.render_version && before.metrics.source_cut === after.metrics.source_cut, 'capture crossed render version/source cut');
    assert(isDeepStrictEqual(actualGeometry(before), actualGeometry(after)), 'capture crossed actual camera/viewport/CSS/backing/DPR/scroll');
    assert(before.targets.length === 1000 && before.targets.every(t => t.inView), 'not all1000 targets are in view');
    summary.phases[name] = before; return before;
  } finally { pauseAfterComplete = false; pauseReplies = false; release(); }
}
function sourceFor(capture) {
  const cut = capture.metrics.source_cut;
  const pages = wire.filter(row => row.direction === 'out' && !row.replay && row.message.type === 'world_view'
    && row.message.view.delivery === 'baseline' && row.message.view.source_cut === cut);
  const records = new Map(), groups = new Map(); let binding;
  for (const page of pages) {
    const view = page.message.view;
    const current = { world_id: view.world_id, scope_id: view.scope_id, world_bounds: view.world_bounds,
      source_binding: view.source_binding, reorg_epoch: view.reorg_epoch, source_frozen: view.source_frozen };
    if (binding) assert(isDeepStrictEqual(binding, current), 'source pages crossed authority/world/bounds');
    binding = current;
    const match = /^(\d+):\d+$/.exec(view.request_generation);
    assert(match, 'world-view wire correlation did not use the existing transport');
    const groupKey = page.connection + ':' + match[1];
    const group = groups.get(groupKey) || []; group.push(view); groups.set(groupKey, group);
    for (const entity of view.entities) {
      const key = `${entity.kind}:${entity.entity_id}`;
      if (records.has(key)) assert(isDeepStrictEqual(records.get(key), entity), 'same cut contains conflicting source records');
      records.set(key, entity);
    }
  }
  assert(records.size === 1000, 'actual wire did not disclose1000 source records at capture cut');
  const completeFivePages = [...groups.values()].some(group => group.length === 5 && group.every(view => view.entities.length === 200)
    && group.at(-1).completeness === 'complete' && !group.at(-1).next_cursor
    && new Set(group.flatMap(view => view.entities.map(entity => `${entity.kind}:${entity.entity_id}`))).size === 1000);
  assert(completeFivePages, 'no single actual wire query supplied five200-page complete1000 baseline');
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
  const url = `http://density-qa.localhost:${httpServer.address().port}/viewer.html?test_api=1&connect=1&hosted_bootstrap=0&locale=en`;
  await run(['open', 'about:blank']); await run(['set', 'viewport', '1440', '1000', String(dpr)]); await run(['open', url]); await attachMetrics();
  await waitProjection(800); summary.partial = await state(); assertGuest(summary.partial);
  assert(held.some(x => x.response.view?.entities?.length === 200), 'last200-page was not actually held');
  holdLastPage = false; release(); await waitProjection(1000); await overview(); await run(['wait', '500']); await waitProjection(1000); await zoomForLabels(); await centerWithNativePan(); await waitProjection(1000);
  const css = await evaluate(`(async()=>{const l=[...document.querySelectorAll('link[rel=stylesheet]')].find(l=>l.href.includes('viewer_terminal_shell.css')),r=l?await fetch(l.href):null,t=document.querySelector('.pixel-world-renderer-target:not([data-selected=true])'),s=t?getComputedStyle(t):null;return{status:r?.status,mime:r?.headers.get('content-type'),rules:l?.sheet?.cssRules.length,background:s?.backgroundColor,border:s?.borderTopColor,shadow:s?.boxShadow}})()`);
  assert(css.status === 200 && css.mime?.startsWith('text/css') && css.rules > 0 && css.background === 'rgba(0, 0, 0, 0)' && css.border === 'rgba(0, 0, 0, 0)' && css.shadow === 'none', 'canonical style prerequisite failed'); summary.css = css;
  const initial = await state();
  assert(initial.gpu && !/swiftshader|llvmpipe|softpipe|software/i.test(initial.gpu) && initial.dpr === dpr
    && initial.viewport.width === 1440 && initial.viewport.height === 1000
    && initial.canvas.cssWidth > 0 && initial.canvas.cssHeight > 0
    && initial.canvas.width === Math.floor(initial.canvas.cssWidth * dpr)
    && initial.canvas.height === Math.floor(initial.canvas.cssHeight * dpr), 'actual GPU/canvas CSS/backing/DPR mismatch');
  const labelCaptures = { positive: await capture('labels-positive', true) };
  fixture.setLabelStyle('empty'); await waitProjection(1000); labelCaptures.negative = await capture('labels-negative');
  fixture.setLabelStyle('normal'); await waitProjection(1000); labelCaptures.restored = await capture('labels-restored', true);
  writeFileSync(resolve(out, 'label-captures.json'), JSON.stringify(labelCaptures, null, 2));
  summary.labelOnlyInvariant = labelOnlyInvariant(labelCaptures);
  summary.labelOracle = JSON.parse(execFileSync('python3', [resolve(here, 'world-view-label-oracle.py'), out], { encoding: 'utf8' })); assert(summary.labelOracle.passed, '200 native label paired PNG oracle failed');
  if (process.env.DENSITY_QA_PENDING === '1') summary.pendingSelection = await probePendingWorldSelection({ run, evaluate, state, waitProjection, armPendingGate, freezeCompleteBoundary, unfreeze: () => { pauseReplies = false; pauseAfterComplete = false; release(); }, fixture, reconnect, waitNegotiation, out });
  if (process.env.DENSITY_QA_PERFORMANCE_MS) summary.performance = await measureDensityPerformance({ evaluate, run, waitProjection, fixture, wire, out, overview, durationMs: Number(process.env.DENSITY_QA_PERFORMANCE_MS) });
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
    const old = lastOldPage; fixture.newWorld(); await reconnect(); await waitNegotiation(true); await waitProjection(1000); summary.worldB = await state();
    assert(old?.response && old.connection, 'missing actual old-connection response for replay');
    try {
      await freezeCompleteBoundary(); const before = await state();
      for (const socket of wss.clients) send(socket, connectionIds.get(socket), old.response,
        { kind: 'old_response_on_current_socket', originalConnection: old.connection, originalRequestGeneration: old.response.view.request_generation });
      await run(['wait', '300']); const after = await state();
      assert(after.metrics.status === 'observed' && after.metrics.source_cut === fixture.description().cut
        && before.metrics.render_version === after.metrics.render_version, 'old response changed the observed new-world scene');
      summary.oldResponseReplay = { before, after, originalConnection: old.connection, originalRequestGeneration: old.response.view.request_generation };
    } finally { pauseAfterComplete = false; pauseReplies = false; release(); }
    const deniedWireStart = wire.length;
    fixture.revoke(); await reconnect(); await waitNegotiation(true);
    await waitFor(`document.querySelector('.pixel-world-exploration [role=status]')?.textContent.includes('Read unavailable')&&(${browserState}).targets.length===0&&!((${browserState}).metrics.projected_marker_count>0)`);
    const deniedReplies = wire.slice(deniedWireStart).filter(row => row.direction === 'out' && row.message.type === 'world_view' && row.message.view.unavailable_reasons?.includes('scope_unavailable'));
    assert(deniedReplies.length > 0 && deniedReplies.every(row => row.message.view.entities.length === 0), 'scope denial did not produce an actual empty unavailable response');
    summary.denied = { state: await state(), replies: deniedReplies, claim: 'controlled scope query refusal with protocol capability retained; not genuine service revocation' };
    fixture.restoreWorldView(); await reconnect(); await waitNegotiation(true); await waitProjection(1000); summary.restoredScope = await state();
    const recoveredTarget = await evaluate(`(()=>{const candidates=[...document.querySelectorAll('button[data-agent-id][data-renderer-target]')].map(b=>{const r=b.getBoundingClientRect(),x=r.x+r.width/2,y=r.y+r.height/2;return{b,id:b.dataset.agentId,x,y,visible:r.width>0&&r.height>0&&x>0&&x<innerWidth&&y>0&&y<innerHeight}}).filter(t=>t.visible&&t.b.dataset.selected!=='true'&&t.b.contains(document.elementFromPoint(t.x,t.y)));candidates.sort((a,b)=>(a.x-innerWidth/2)**2+(a.y-innerHeight/2)**2-((b.x-innerWidth/2)**2+(b.y-innerHeight/2)**2));if(!candidates.length)throw Error('no unobscured Agent after fresh scope recovery');const{id,x,y}=candidates[0];return{id,x,y}})()`);
    assert(/^qa-agent-\d{4}$/.test(recoveredTarget.id), 'recovered target was not an actual controlled Agent');
    const recoveredSelector = `button[data-agent-id=${JSON.stringify(recoveredTarget.id)}][data-renderer-target]`;
    await run(['click', recoveredSelector]);
    await waitFor(`document.querySelector(${JSON.stringify(recoveredSelector)})?.dataset.selected==='true'`);
    const selectedAfterRecovery = await recoveredScopeState(1000, recoveredTarget.id);
    assert(selectedAfterRecovery.core.selectedId === null && selectedAfterRecovery.core.selectedKind === null, 'recovered spatial inspection became a business selection');
    fixture.remove(recoveredTarget.id); const removedAfterRecovery = await recoveredScopeState(999);
    assert(!removedAfterRecovery.targets.some(target => target.id === recoveredTarget.id), 'recovered selected target survived source deletion');
    fixture.restore(); const recreatedAfterRecovery = await recoveredScopeState(1000);
    summary.recoveredScopeSelection = { target: recoveredTarget, selected: selectedAfterRecovery, removed: removedAfterRecovery, recreated: recreatedAfterRecovery };
    fixture.setLegacy(); await reconnect(); await waitNegotiation(false); await waitFor(`(${browserState}).targets.length===0&&!((${browserState}).metrics.projected_marker_count>0)`); summary.legacyDowngrade = { state: await state(), claim: 'cleared/locked; no legitimate legacy reauthorization proof supplied' };
    fixture.restoreWorldView(); await reconnect(); await waitNegotiation(true); await waitProjection(1000);
    const origin = await evaluate('performance.timeOrigin');
    await evaluate(`(()=>{const gl=document.querySelector('#pixel-world-embedded-runtime-canvas').getContext('webgl2'),loss=gl.getExtension('WEBGL_lose_context');if(!loss)throw Error('loss extension unavailable');loss.loseContext();return true})()`);
    await waitFor(`document.querySelector('.pixel-world-render-unavailable__retry')&&(${browserState}).targets.length===0`); assert(await evaluate('performance.timeOrigin') === origin, 'automatic page reload occurred');
    await evaluate(`document.querySelector('.pixel-world-render-unavailable__retry').focus({preventScroll:true});true`); await run(['press', 'Enter']);
    await waitFor(`performance.timeOrigin!==${origin}`); await attachMetrics(); await waitProjection(1000); await overview(); await zoomForLabels(); await centerWithNativePan(); summary.reloaded = await capture('reloaded', true);
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
