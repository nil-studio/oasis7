/** Finite controlled WS/GPU query-overview cases; no signed authority, P5/P6 or long-window result.
 * QUERY_OVERVIEW_ARTIFACT_ROOT=<immutable canonical dist>
 * QUERY_OVERVIEW_SOURCE_HEAD=<built commit> QUERY_OVERVIEW_COMPOSED_MANIFEST=<manifest>
 * QUERY_OVERVIEW_CASE=complete|cancel|terminal node scripts/query-overview-visual.mjs
 * The manifest variable accepts canonical provenance; the summary preserves its actual kind.
 */
import { createServer } from 'node:http';
import { WebSocketServer } from 'ws';
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
assert(process.env.QUERY_OVERVIEW_ARTIFACT_ROOT && process.env.QUERY_OVERVIEW_SOURCE_HEAD && process.env.QUERY_OVERVIEW_COMPOSED_MANIFEST, 'declare immutable artifacts, built source and provenance');
const here = dirname(fileURLToPath(import.meta.url)), repo = resolve(here, '../../..');
const root = resolve(process.env.QUERY_OVERVIEW_ARTIFACT_ROOT), fixture = createScopePendingFixture();
const out = resolve(process.env.QUERY_OVERVIEW_OUTPUT || resolve(repo, 'output/scope-pending-5605', new Date().toISOString().replace(/[:.]/g, '-')));
mkdirSync(out, { recursive: true });
const sha = bytes => createHash('sha256').update(bytes).digest('hex');
const files = dir => readdirSync(dir, { withFileTypes: true }).flatMap(e => e.isDirectory() ? files(resolve(dir, e.name)) : [resolve(dir, e.name)]);
const manifest = () => Object.fromEntries(files(root).sort().map(p => [relative(root, p), { bytes: readFileSync(p).length, sha256: sha(readFileSync(p)) }]));
const provenance = JSON.parse(readFileSync(process.env.QUERY_OVERVIEW_COMPOSED_MANIFEST)), assets = manifest();
const source = execFileSync('git', ['rev-parse', process.env.QUERY_OVERVIEW_SOURCE_HEAD + '^{commit}'], { cwd: repo, encoding: 'utf8' }).trim();
assert(provenance.source === source && isDeepStrictEqual(provenance.files, assets), 'artifact provenance mismatch');
const viewportWidth=Number(process.env.QUERY_OVERVIEW_WIDTH||1440);assert([390,1440].includes(viewportWidth),'unsupported viewport');const qaCase = process.env.QUERY_OVERVIEW_CASE || 'complete'; assert(['complete','cancel','terminal'].includes(qaCase),'unknown case');
const summary = { source, runnerHead: execFileSync('git', ['rev-parse', 'HEAD'], { cwd: repo, encoding: 'utf8' }).trim(),
  scripts: Object.fromEntries(['query-overview-visual.mjs', 'scope-pending-fixture.mjs', 'viewer-owned-cdp-input.mjs', 'viewer-owned-cdp-session.mjs'].map(n => [n, sha(readFileSync(resolve(here, n)))])),
  assets, provenance, case: qaCase, viewportWidth,
  scope: 'Finite controlled observer/debug query-overview transport; not real grant/revocation, P5, density, P6 or long-window proof.', phases: {} };
const command = process.env.AGENT_BROWSER_BIN || 'agent-browser', session = 'query-overview-' + process.pid;
const lifecycle = createOwnedSessionLifecycle({ command, session }), exec = promisify(execFile);
const run = async args => { summary.lastCommand = args; try { return (await exec(command, ['--session', session, ...args], { timeout: 30000, maxBuffer: 8 * 1024 * 1024 })).stdout; } catch (error) { summary.failedCommand ||= { args, stdout: error.stdout, stderr: error.stderr }; throw error; } };
const evaluate = async expression => { const r = JSON.parse(await run(['--json', 'eval', expression])); assert(r.success, JSON.stringify(r)); return r.data.result; };
const browserState = `(()=>{const c=document.querySelector('#pixel-world-embedded-runtime-canvas'),s=window.__AW_TEST__.getState(),feedback=document.querySelector('[data-world-selection-status]');return{time:performance.now(),origin:performance.timeOrigin,core:s,metrics:window.__SCOPE_METRICS__?.(),overview:document.querySelector('[data-query-overview-status]')?.dataset.queryOverviewStatus||null,overviewText:document.querySelector('[data-query-overview-status]')?.textContent||null,overviewRect:document.querySelector('[data-query-overview-status]')?.getBoundingClientRect().toJSON()||null,feedback:feedback?{status:feedback.dataset.worldSelectionStatus,text:feedback.textContent,rect:feedback.getBoundingClientRect().toJSON()}:null,canvas:c?{rect:c.getBoundingClientRect().toJSON(),width:c.width,height:c.height}:null,targets:[...document.querySelectorAll('button[data-agent-id][data-renderer-target]')].map(b=>({id:b.dataset.agentId,selected:b.dataset.selected,rect:b.getBoundingClientRect().toJSON()})),browser:{userAgent:navigator.userAgent,platform:navigator.platform,dpr:devicePixelRatio}}})()`;
const state = async () => JSON.parse(await evaluate(`JSON.stringify(${browserState})`));
const wait = async condition => JSON.parse(await evaluate(`(async()=>{const end=Date.now()+12000;let s;while(Date.now()<end){s=${browserState};if(s.core.lastError?.startsWith('window.error:'))throw Error(s.core.lastError);if(${condition})return JSON.stringify(s);await new Promise(r=>setTimeout(r,25))}throw Error('scope condition not observed: '+JSON.stringify(s))})()`));
const guest = s => assert(s.core.authReady === false && s.core.authPlayerId === null && s.core.authPublicKey === null && s.core.selectedId === null && s.core.selectedKind === null, 'observer became a business identity/selection');
const observed = n => wait(`s.metrics?.status==='observed'&&s.metrics.display_profile==='world_view_v1'&&s.metrics.projected_marker_count===${n}&&s.metrics.agent_body_sprites===${n}&&s.metrics.in_view_agent_body_sprites===${n}&&s.targets.length===${n}`);
const wire = [], http = [], ids = new WeakMap(); let gate = null;
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
        const generation = message.query.request_generation.split(':')[0];
        if (!gate.generation && !message.query.cursor) gate.generation = generation;
        if (generation === gate.generation) { gate.pages.push({ request: structuredClone(message), prepared: structuredClone(response), connection, wall: Date.now() });
          if (gate.pages.length === 3) { gate.third={socket,request:message,response};gate.ready();return; }
        }
      }
      send(socket, response);
    } catch (error) { summary.protocolError = String(error); socket.close(1008, 'read-only fixture violation'); }
  });
});
const camera = row => row.core.pixelWorldCamera;
const sameCamera = (a,b) => isDeepStrictEqual(camera(a),camera(b));
const requests = () => wire.filter(r=>r.direction==='in'&&r.message.type==='request_world_view').length;
async function pan(dx) {
  const point=JSON.parse(await evaluate(`JSON.stringify((()=>{const c=document.querySelector('#pixel-world-embedded-runtime-canvas');for(let y=400;y<Math.min(innerHeight-50,900);y+=25)for(let x=40;x<innerWidth-60;x+=25)if(document.elementFromPoint(x,y)===c&&document.elementFromPoint(x+${dx},y)===c)return{x,y};throw Error('no canvas pan point')})())`));
  const before=await state();await run(['mouse','move',String(point.x),String(point.y)]);await run(['mouse','down']);await run(['mouse','move',String(point.x+dx),String(point.y)]);await run(['mouse','up']);
  const after=await wait(`s.core.pixelWorldCamera?.pan_x_px!==${camera(before).pan_x_px}`);assert(camera(after).zoom===camera(before).zoom,'pan changed zoom');return after;
}
try {
  await new Promise(done=>server.listen(0,'127.0.0.1',done));lifecycle.prepare();
  await run(['open','about:blank']);await run(['set','viewport',String(viewportWidth),'1000','1']);
  await run(['open',`http://scope-observer.localhost:${server.address().port}/viewer.html?test_api=1&connect=1&hosted_bootstrap=0&locale=en`]);
  await evaluate(`(async()=>{const m=await import('/pixel-world-bridge/pixel_world_bridge.js');window.__SCOPE_METRICS__=m.readPixelWorldDisplayMetrics;return true})()`);
  summary.phases.initial=await observed(3);guest(summary.phases.initial);
  summary.phases.manual=await pan(40);await run(['wait','350']);
  const disclosure='.pixel-world-exploration-disclosure';if(!await evaluate(`document.querySelector('${disclosure}').open`))await run(['click',disclosure+' > summary']);
  let ready;gate={pages:[],received:new Promise(r=>ready=r),ready};
  await run(['fill','.pixel-world-exploration form input','scope-']);await run(['press','Enter']);await within(gate.received,'held third page');
  summary.phases.partial=await observed(2);guest(summary.phases.partial);const count=requests();
  await run(['click','[data-query-overview]']);summary.phases.pending=await wait(`s.overview==='pending'`);guest(summary.phases.pending);
  assert(sameCamera(summary.phases.partial,summary.phases.pending),'partial overview fitted camera');assert(requests()===count,'overview issued a query');assert(summary.phases.pending.overviewText?.includes('incomplete'),'partial was presented as complete');
  assert(await evaluate(`document.querySelector('.pixel-world-exploration form input').value==='scope-'`),'overview cleared search');
  const statusRect=summary.phases.pending.overviewRect;assert(statusRect&&statusRect.top>=0&&statusRect.bottom<=1000&&statusRect.left>=0&&statusRect.right<=viewportWidth,'overview status outside viewport');await run(['screenshot','--full',resolve(out,'pending.png')]);
  const held=gate.third;gate=null;
  if(qaCase==='terminal') {const response=structuredClone(held.response);response.view.completeness='partial';response.view.budget_limited=true;send(held.socket,response);summary.phases.unavailable=await wait(`s.overview==='unavailable'`);assert(sameCamera(summary.phases.pending,summary.phases.unavailable),'terminal partial fitted');}
  else {
    if(qaCase==='cancel') {await run(['click',disclosure+' > summary']);summary.phases.cancelled=await pan(24);}
    send(held.socket,held.response);summary.phases.complete=await observed(3);
    if(qaCase==='complete') {summary.phases.applied=await wait(`s.overview===null&&(s.core.pixelWorldCamera?.zoom!==${camera(summary.phases.pending).zoom}||s.core.pixelWorldCamera?.pan_x_px!==${camera(summary.phases.pending).pan_x_px}||s.core.pixelWorldCamera?.pan_y_px!==${camera(summary.phases.pending).pan_y_px})`);await run(['wait','300']);summary.phases.stable=await state();assert(sameCamera(summary.phases.applied,summary.phases.stable),'complete fitted repeatedly');await run(['click',disclosure+' > summary']);summary.phases.manualAfterFit=await pan(24);await observed(3);await run(['wait','350']);summary.phases.backgroundComplete=await state();assert(sameCamera(summary.phases.manualAfterFit,summary.phases.backgroundComplete),'background full stole manual camera');}
    else {await run(['wait','300']);summary.phases.afterLate=await state();assert(sameCamera(summary.phases.cancelled,summary.phases.afterLate),'cancelled overview fitted late complete');}
  }
  const final=await state();guest(final);assert(!summary.protocolError&&isDeepStrictEqual(assets,manifest()),'protocol or immutable failure');summary.status='passed_controlled_query_overview_only';
} catch(error) {summary.status='failed';summary.error=String(error);summary.failure=await state().catch(()=>null);await run(['screenshot','--full',resolve(out,'failure.png')]).catch(()=>{});throw error;}
finally {lifecycle.close();for(const socket of wss.clients)socket.terminate();await new Promise(done=>wss.close(done));await new Promise(done=>server.close(done));summary.fixture=fixture.describe();summary.http=http;writeFileSync(resolve(out,'summary.json'),JSON.stringify(summary,null,2));console.log(out);}
