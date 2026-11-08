/** Bounded 3-entity observer QA; not 1000/200, signed authority, auth-epoch or P6.
 * SCOPE_PENDING_ARTIFACT_ROOT=<immutable dist> SCOPE_PENDING_SOURCE_HEAD=<built source>
 * SCOPE_PENDING_COMPOSED_MANIFEST=<manifest> node scripts/scope-pending-visual.mjs
 * Optional SCOPE_PENDING_RESET_CASE=1 tests connection-generation reset separately.
 * One native input attempt; a missed presented-target window exits unmeasured.
 */
import { createServer } from 'node:http';
import { WebSocketServer, WebSocket } from 'ws';
import { readFileSync, writeFileSync, appendFileSync, mkdirSync, readdirSync } from 'node:fs';
import { resolve, dirname, extname, relative, sep } from 'node:path';
import { fileURLToPath } from 'node:url';
import { execFile, execFileSync } from 'node:child_process';
import { promisify, isDeepStrictEqual } from 'node:util';
import { createHash, randomUUID } from 'node:crypto';
import { createOwnedSessionLifecycle } from './agent-browser-visual-runner-lifecycle.mjs';
import { createScopePendingFixture } from './scope-pending-fixture.mjs';

const assert = (value, message) => { if (!value) throw Error(message); };
const within = async (promise, name) => { let timer; try { return await Promise.race([promise, new Promise((_, reject) => timer = setTimeout(() => reject(Error(name + ' timed out')), 10000))]); } finally { clearTimeout(timer); } };
assert(process.env.SCOPE_PENDING_ARTIFACT_ROOT && process.env.SCOPE_PENDING_SOURCE_HEAD && process.env.SCOPE_PENDING_COMPOSED_MANIFEST, 'declare immutable artifacts, built source and provenance');
const here = dirname(fileURLToPath(import.meta.url)), repo = resolve(here, '../../..');
const root = resolve(process.env.SCOPE_PENDING_ARTIFACT_ROOT), fixture = createScopePendingFixture();
const out = resolve(process.env.SCOPE_PENDING_OUTPUT || resolve(repo, 'output/scope-pending-5605', new Date().toISOString().replace(/[:.]/g, '-')));
mkdirSync(out, { recursive: true });
const sha = bytes => createHash('sha256').update(bytes).digest('hex');
const files = dir => readdirSync(dir, { withFileTypes: true }).flatMap(e => e.isDirectory() ? files(resolve(dir, e.name)) : [resolve(dir, e.name)]);
const manifest = () => Object.fromEntries(files(root).sort().map(p => [relative(root, p), { bytes: readFileSync(p).length, sha256: sha(readFileSync(p)) }]));
const provenance = JSON.parse(readFileSync(process.env.SCOPE_PENDING_COMPOSED_MANIFEST)), assets = manifest();
const source = execFileSync('git', ['rev-parse', process.env.SCOPE_PENDING_SOURCE_HEAD + '^{commit}'], { cwd: repo, encoding: 'utf8' }).trim();
assert(provenance.source === source && isDeepStrictEqual(provenance.files, assets), 'artifact provenance mismatch');
const resetCase = process.env.SCOPE_PENDING_RESET_CASE === '1';
const summary = { source, runnerHead: execFileSync('git', ['rev-parse', 'HEAD'], { cwd: repo, encoding: 'utf8' }).trim(),
  scripts: Object.fromEntries(['scope-pending-visual.mjs', 'scope-pending-fixture.mjs'].map(n => [n, sha(readFileSync(resolve(here, n)))])),
  assets, provenance, case: resetCase ? 'connection_scope_reset' : 'same_request_scope_unavailable',
  scope: 'Finite controlled observer/debug transport, one input attempt, no real grant/revocation/auth-epoch, world_deleted, density or performance proof.', phases: {} };
const command = process.env.AGENT_BROWSER_BIN || 'agent-browser', session = 'scope-pending-' + process.pid;
const lifecycle = createOwnedSessionLifecycle({ command, session }), exec = promisify(execFile);
const run = async args => { summary.lastCommand = args; try { return (await exec(command, ['--session', session, ...args], { timeout: 30000, maxBuffer: 8 * 1024 * 1024 })).stdout; } catch (error) { summary.failedCommand ||= { args, stdout: error.stdout, stderr: error.stderr }; throw error; } };
const evaluate = async expression => { const r = JSON.parse(await run(['--json', 'eval', expression])); assert(r.success, JSON.stringify(r)); return r.data.result; };
const browserState = `(()=>{const c=document.querySelector('#pixel-world-embedded-runtime-canvas'),s=window.__AW_TEST__.getState(),feedback=document.querySelector('[data-world-selection-status]');return{time:performance.now(),origin:performance.timeOrigin,core:s,metrics:window.__SCOPE_METRICS__?.(),feedback:feedback?{status:feedback.dataset.worldSelectionStatus,text:feedback.textContent,rect:feedback.getBoundingClientRect().toJSON()}:null,canvas:c?{rect:c.getBoundingClientRect().toJSON(),width:c.width,height:c.height}:null,targets:[...document.querySelectorAll('button[data-agent-id][data-renderer-target]')].map(b=>({id:b.dataset.agentId,selected:b.dataset.selected,rect:b.getBoundingClientRect().toJSON()})),browser:{userAgent:navigator.userAgent,platform:navigator.platform,dpr:devicePixelRatio}}})()`;
const state = async () => JSON.parse(await evaluate(`JSON.stringify(${browserState})`));
const wait = async condition => JSON.parse(await evaluate(`(async()=>{const end=Date.now()+12000;let s;while(Date.now()<end){s=${browserState};if(s.core.lastError?.startsWith('window.error:'))throw Error(s.core.lastError);if(${condition})return JSON.stringify(s);await new Promise(r=>setTimeout(r,25))}throw Error('scope condition not observed: '+JSON.stringify(s))})()`));
const guest = s => assert(s.core.authReady === false && s.core.authPlayerId === null && s.core.authPublicKey === null && s.core.selectedId === null && s.core.selectedKind === null, 'observer became a business identity/selection');
const observed = n => wait(`s.metrics?.status==='observed'&&s.metrics.display_profile==='world_view_v1'&&s.metrics.projected_marker_count===${n}&&s.metrics.agent_body_sprites===${n}&&s.metrics.in_view_agent_body_sprites===${n}&&s.targets.length===${n}`);
const wire = [], http = [], ids = new WeakMap(); let gate = null, cdp = null;
const audit = row => { wire.push(row); appendFileSync(resolve(out, 'wire.jsonl'), JSON.stringify(row) + '\n'); };
const send = (socket, response, replay = false) => { assert(socket.readyState === 1, 'response socket is not open'); audit({ direction: 'out', connection: ids.get(socket), wall: Date.now(), replay, message: response }); socket.send(JSON.stringify(response)); };
const server = createServer((req, res) => {
  try { const pathname = new URL(req.url, 'http://unused').pathname; http.push({ method: req.method, pathname });
    const p = resolve(root, '.' + (pathname === '/' ? '/viewer.html' : pathname)); assert(p.startsWith(root + sep), 'path escaped artifacts'); let bytes = readFileSync(p);
    if (pathname.endsWith('.html')) { const config = `<script id="oasis7-viewer-runtime-config" type="application/json">${JSON.stringify({ deploymentMode: 'trusted_local_only', viewerWsEndpoint: `ws://scope-observer.localhost:${server.address().port}/scope-ws` })}</script>`;
      let html = bytes.toString(); const pattern = /<script\b[^>]*id=["']oasis7-viewer-runtime-config["'][^>]*>[\s\S]*?<\/script>/g;
      const count = [...html.matchAll(pattern)].length; assert(count <= 1, 'ambiguous runtime config'); html = count ? html.replace(pattern, config) : html.replace('</head>', config + '</head>'); bytes = Buffer.from(html); }
    res.writeHead(200, { 'Content-Type': ({ '.html': 'text/html; charset=utf-8', '.js': 'text/javascript', '.css': 'text/css', '.wasm': 'application/wasm' })[extname(p)] || 'application/octet-stream', 'Cache-Control': 'no-store' }); res.end(bytes);
  } catch (error) { summary.httpError = String(error); res.writeHead(404); res.end(); }
});
const wss = new WebSocketServer({ server, path: '/scope-ws', maxPayload: 128 * 1024 });
wss.on('connection', socket => {
  ids.set(socket, randomUUID()); socket.on('message', bytes => {
    try { const message = JSON.parse(bytes.toString()), connection = ids.get(socket); audit({ direction: 'in', connection, wall: Date.now(), message });
      const response = fixture.reply(message, connection); if (!response) return;
      if (gate && message.type === 'request_world_view' && message.query.search === 'scope-' && response.view.delivery === 'baseline') {
        const generation = message.query.request_generation.split(':')[0]; if (!gate.generation && !message.query.cursor) gate.generation = generation;
        if (generation === gate.generation) { gate.pages.push({ request: structuredClone(message), prepared: structuredClone(response), connection, wall: Date.now() });
          if (gate.pages.length === 1) { gate.first = { socket, response }; gate.firstReady(); return; }
          if (gate.pages.length === 3) { gate.third = { socket, request: message, response }; summary.thirdRequestAt = Date.now();
            cdp.enter().then(gate.thirdReady, error => { gate.error = String(error); gate.thirdReady(); }); return; }
        }
      }
      send(socket, response);
    } catch (error) { summary.protocolError = String(error); socket.close(1008, 'read-only fixture violation'); }
  });
});
async function connectCdp() {
  const info = JSON.parse(await run(['--json', 'get', 'cdp-url'])), url = info.data?.cdpUrl; assert(url?.startsWith('ws://127.0.0.1:'), 'CDP is not owned loopback');
  const ws = new WebSocket(url, { handshakeTimeout: 10000 }), pending = new Map(); let sequence = 0;
  try { await within(new Promise((yes, no) => { ws.once('open', yes); ws.once('error', no); }), 'CDP open'); } catch (error) { ws.terminate(); throw error; }
  ws.on('message', bytes => { const r = JSON.parse(bytes); const p = pending.get(r.id); if (p) { clearTimeout(p.timer); pending.delete(r.id); r.error ? p.no(Error(JSON.stringify(r.error))) : p.yes(r.result); } });
  const call = (method, params = {}, sessionId) => new Promise((yes, no) => { const id = ++sequence, timer = setTimeout(() => { pending.delete(id); no(Error(method + ' timed out')); }, 10000); pending.set(id, { yes, no, timer }); ws.send(JSON.stringify({ id, method, params, ...(sessionId ? { sessionId } : {}) })); });
  const href = await evaluate('location.href'), targets = (await call('Target.getTargets')).targetInfos.filter(t => t.type === 'page' && t.url === href); assert(targets.length === 1, 'ambiguous owned Viewer page');
  const { sessionId } = await call('Target.attachToTarget', { targetId: targets[0].targetId, flatten: true });
  return { async frame() { return (await call('Runtime.evaluate', { expression: 'new Promise(resolve=>requestAnimationFrame(frame=>resolve({frame,now:performance.now(),wall:Date.now()})))', awaitPromise: true, returnByValue: true }, sessionId)).result.value; },
    async enter() { summary.cdpInputAt = Date.now(); const key = { key: 'Enter', code: 'Enter', windowsVirtualKeyCode: 13, nativeVirtualKeyCode: 13 }; await call('Input.dispatchKeyEvent', { type: 'keyDown', text: '\r', unmodifiedText: '\r', ...key }, sessionId); await call('Input.dispatchKeyEvent', { type: 'keyUp', ...key }, sessionId); }, close() { ws.close(); } };
}
async function reconnect() { for (const socket of wss.clients) socket.close(1012, 'controlled connection-scope transition'); await run(['wait', '1400']); }
try {
  await new Promise(done => server.listen(0, '127.0.0.1', done)); lifecycle.prepare();
  await run(['open', 'about:blank']); await run(['set', 'viewport', '1440', '1000', '1']);
  await run(['open', `http://scope-observer.localhost:${server.address().port}/viewer.html?test_api=1&connect=1&hosted_bootstrap=0&locale=en`]);
  await evaluate(`(async()=>{const m=await import('/pixel-world-bridge/pixel_world_bridge.js');window.__SCOPE_METRICS__=m.readPixelWorldDisplayMetrics;return true})()`);
  await observed(3); await run(['click', 'button[data-agent-id="scope-A"][data-renderer-target]']);
  summary.phases.initial = await wait(`s.metrics?.status==='observed'&&s.targets.length===3&&s.targets.some(t=>t.id==='scope-A'&&t.selected==='true')`); guest(summary.phases.initial);
  const initialB = summary.phases.initial.targets.find(t => t.id === 'scope-B')?.rect;
  assert(summary.phases.initial.metrics.in_view_agent_body_sprites === 3 && initialB?.left >= 0 && initialB.right <= 1440 && initialB.top >= 0 && initialB.bottom <= 1000, 'initial B was not actually in view');
  assert(!summary.phases.initial.core.viewerProtocol.capabilities.includes('world_view_delta_v1'), 'background delta poll was enabled');
  cdp = await connectCdp(); let firstReady, thirdReady;
  gate = { pages: [], firstReceived: new Promise(r => firstReady = r), thirdReceived: new Promise(r => thirdReady = r), firstReady, thirdReady };
  const disclosure = '.pixel-world-exploration-disclosure'; if (!await evaluate(`document.querySelector('${disclosure}').open`)) await run(['click', disclosure + ' > summary']);
  await run(['fill', '.pixel-world-exploration form input', 'scope-']); await evaluate(`document.querySelector('.pixel-world-exploration form input').focus({preventScroll:true});true`); await run(['press', 'Enter']);
  await within(gate.firstReceived, 'first page request');
  await run(['click', disclosure + ' > summary']); await observed(3);
  await evaluate(`(()=>{const b=document.querySelector('button[data-agent-id=scope-B][data-renderer-target]');if(!b)throw Error('presented B missing');b.focus({preventScroll:true});window.__SCOPE_INPUT__=[];for(const type of ['keydown','click'])document.addEventListener(type,e=>{if(type==='keydown'&&e.key!=='Enter')return;if(__SCOPE_INPUT__.length<32)__SCOPE_INPUT__.push({type,trusted:e.isTrusted,target:e.target.closest?.('[data-agent-id]')?.dataset.agentId||null,time:e.timeStamp,wall:Date.now(),oldB:!!document.querySelector('button[data-agent-id=scope-B][data-renderer-target]'),metrics:window.__SCOPE_METRICS__()})},true);return true})()`);
  summary.releaseFrame = await cdp.frame(); send(gate.first.socket, gate.first.response);
  await within(gate.thirdReceived, 'third page and native input'); assert(!gate.error, gate.error);
  summary.gatedPages = gate.pages; summary.input = JSON.parse(await evaluate('JSON.stringify(window.__SCOPE_INPUT__)'));
  assert(gate.pages.length === 3 && gate.pages.slice(0, 2).every(p => p.prepared.view.entities.length === 1 && p.prepared.view.entities[0].entity_id !== 'scope-B'), 'B was already disclosed in first two pages');
  assert(summary.input.some(e => e.type === 'keydown' && e.trusted && e.target === 'scope-B' && e.oldB)
    && summary.input.some(e => e.type === 'click' && e.trusted && e.target === 'scope-B' && e.oldB), 'unmeasured_native_window_missed');
  summary.phases.pending = await wait(`s.feedback?.status==='pending'&&!s.targets.some(t=>t.id==='scope-B'&&t.selected==='true')`); guest(summary.phases.pending);
  const r = summary.phases.pending.feedback.rect; assert(r.top >= 0 && r.bottom <= 1000, 'real pending feedback outside viewport');
  await run(['screenshot', '--full', resolve(out, 'pending.png')]);
  const held = gate.third, prepared = held.response; fixture.denyReads();
  if (!resetCase) {
    const refused = fixture.reply(held.request, ids.get(held.socket)); assert(refused.view.request_generation === held.request.query.request_generation, 'refusal correlation changed');
    summary.refusal = refused; send(held.socket, refused); gate = null;
    summary.phases.unavailable = await wait(`s.feedback?.status==='unavailable'&&!s.targets.some(t=>t.id==='scope-B'&&t.selected==='true')`); guest(summary.phases.unavailable);
    await run(['screenshot', '--full', resolve(out, 'unavailable.png')]); send(held.socket, prepared, { kind: 'settled_request_late_page', originalConnection: ids.get(held.socket), originalGeneration: held.request.query.request_generation });
    await run(['wait', '300']); summary.phases.afterLate = await state();
    assert(summary.phases.afterLate.feedback?.status !== 'pending' && !summary.phases.afterLate.targets.some(t=>t.id==='scope-B'&&t.selected==='true'), 'settled old page revived B');
  }
  gate = null; await reconnect(); summary.phases.deniedConnection = await wait(`s.core.connectionStatus==='connected'&&s.targets.length===0&&!s.feedback?.status`); guest(summary.phases.deniedConnection);
  if (resetCase) { for (const socket of wss.clients) send(socket, prepared, { kind: 'old_connection_pending_page', originalConnection: ids.get(held.socket), originalGeneration: held.request.query.request_generation }); await run(['wait', '300']); assert((await state()).targets.length === 0, 'old connection page revived denied scene'); }
  fixture.allowReads(); await reconnect(); summary.phases.restored = await observed(3); guest(summary.phases.restored);
  assert(!summary.phases.restored.targets.some(t=>t.id==='scope-B'&&t.selected==='true'), 'cancelled intent survived fresh connection');
  assert(!summary.protocolError && http.every(r => ['GET', 'HEAD'].includes(r.method)) && isDeepStrictEqual(assets, manifest()), 'read-only or immutable contract failed');
  summary.status = 'passed_scope_only';
} catch (error) { summary.status = String(error).includes('unmeasured_native_window_missed') ? 'unmeasured_native_window_missed' : 'failed'; summary.error = String(error); summary.failure = await state().catch(() => null); summary.native = await evaluate('JSON.stringify(window.__SCOPE_INPUT__||[])').then(JSON.parse).catch(() => null); await run(['screenshot', '--full', resolve(out, 'failure.png')]).catch(() => {}); throw error; }
finally { cdp?.close(); lifecycle.close(); for (const socket of wss.clients) socket.terminate(); await new Promise(done => wss.close(done)); await new Promise(done => server.close(done)); summary.fixture = fixture.describe(); summary.http = http; writeFileSync(resolve(out, 'summary.json'), JSON.stringify(summary, null, 2)); console.log(out); }
