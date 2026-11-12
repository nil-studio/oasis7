/** Finite controlled WS/GPU explicit search cases; no signed authority, P5/P6 or long-window result.
 * SEARCH_RACE_ARTIFACT_ROOT=<immutable canonical dist>
 * SEARCH_RACE_SOURCE_HEAD=<built commit> SEARCH_RACE_COMPOSED_MANIFEST=<manifest>
 * SEARCH_RACE_CLEAR=1 or SEARCH_RACE_SCOPE=1 selects clear or query-error; default is search.
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
assert(process.env.SEARCH_RACE_ARTIFACT_ROOT && process.env.SEARCH_RACE_SOURCE_HEAD && process.env.SEARCH_RACE_COMPOSED_MANIFEST, 'declare immutable artifacts, built source and provenance');
const here = dirname(fileURLToPath(import.meta.url)), repo = resolve(here, '../../..');
const root = resolve(process.env.SEARCH_RACE_ARTIFACT_ROOT), fixture = createScopePendingFixture();
const out = resolve(process.env.SEARCH_RACE_OUTPUT || resolve(repo, 'output/scope-pending-5605', new Date().toISOString().replace(/[:.]/g, '-')));
mkdirSync(out, { recursive: true });
const sha = bytes => createHash('sha256').update(bytes).digest('hex');
const files = dir => readdirSync(dir, { withFileTypes: true }).flatMap(e => e.isDirectory() ? files(resolve(dir, e.name)) : [resolve(dir, e.name)]);
const manifest = () => Object.fromEntries(files(root).sort().map(p => [relative(root, p), { bytes: readFileSync(p).length, sha256: sha(readFileSync(p)) }]));
const provenance = JSON.parse(readFileSync(process.env.SEARCH_RACE_COMPOSED_MANIFEST)), assets = manifest();
const declaredFiles = Array.isArray(provenance.files) ? Object.fromEntries(provenance.files.map(f => [f.path, {bytes:f.size,sha256:f.sha256}])) : provenance.files;
assert(!Array.isArray(provenance.files) || Object.keys(declaredFiles).length===provenance.files.length, 'duplicate manifest paths');
const source = execFileSync('git', ['rev-parse', process.env.SEARCH_RACE_SOURCE_HEAD + '^{commit}'], { cwd: repo, encoding: 'utf8' }).trim();
assert(provenance.source === source && isDeepStrictEqual(declaredFiles, assets), 'artifact provenance mismatch');
const viewportWidth=Number(process.env.SEARCH_RACE_WIDTH||1440);assert([390,1440].includes(viewportWidth),'unsupported viewport');const qaCase=process.env.SEARCH_RACE_SCOPE==='1'?'query-error':process.env.SEARCH_RACE_CLEAR==='1'?'clear':'search';
const summary = { source, runnerHead: execFileSync('git', ['rev-parse', 'HEAD'], { cwd: repo, encoding: 'utf8' }).trim(),
  scripts: Object.fromEntries(['search-race-visual.mjs', 'scope-pending-fixture.mjs', 'agent-browser-visual-runner-lifecycle.mjs', 'viewer-owned-cdp-input.mjs', 'viewer-owned-cdp-session.mjs'].map(n => [n, sha(readFileSync(resolve(here, n)))])),
  assets, provenance, case: qaCase, viewportWidth,
  scope: 'Finite controlled observer/debug explicit-search transport; not real grant/revocation, P5, density, P6 or long-window proof.', phases: {} };
const command = process.env.AGENT_BROWSER_BIN || 'agent-browser', session = 'search-race-' + process.pid;
const lifecycle = createOwnedSessionLifecycle({ command, session }), exec = promisify(execFile);
const run = async args => { summary.lastCommand = args; try { return (await exec(command, ['--session', session, ...args], { timeout: 30000, maxBuffer: 8 * 1024 * 1024 })).stdout; } catch (error) { summary.failedCommand ||= { args, stdout: error.stdout, stderr: error.stderr }; throw error; } };
const evaluate = async expression => { const r = JSON.parse(await run(['--json', 'eval', expression])); assert(r.success, JSON.stringify(r)); return r.data.result; };
const browserState = `(()=>{const c=document.querySelector('#pixel-world-embedded-runtime-canvas'),s=window.__AW_TEST__.getState(),feedback=document.querySelector('[data-world-selection-status]');return{readText:document.querySelector('.pixel-world-exploration')?.textContent||'',time:performance.now(),origin:performance.timeOrigin,core:s,metrics:window.__SCOPE_METRICS__?.(),overview:document.querySelector('[data-query-overview-status]')?.dataset.queryOverviewStatus||null,overviewText:document.querySelector('[data-query-overview-status]')?.textContent||null,overviewRect:document.querySelector('[data-query-overview-status]')?.getBoundingClientRect().toJSON()||null,feedback:feedback?{status:feedback.dataset.worldSelectionStatus,text:feedback.textContent,rect:feedback.getBoundingClientRect().toJSON()}:null,canvas:c?{rect:c.getBoundingClientRect().toJSON(),width:c.width,height:c.height}:null,targets:[...document.querySelectorAll('button[data-agent-id][data-renderer-target]')].map(b=>({id:b.dataset.agentId,selected:b.dataset.selected,disabled:b.disabled,tabIndex:b.tabIndex,display:getComputedStyle(b).display,visibility:getComputedStyle(b).visibility,rect:b.getBoundingClientRect().toJSON()})),browser:{userAgent:navigator.userAgent,platform:navigator.platform,dpr:devicePixelRatio}}})()`;
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
      if(gate && message.type==='request_world_view'){gate.held={socket,response,message};gate.ready();return;}
      send(socket, response);
    } catch (error) { summary.protocolError = String(error); socket.close(1008, 'read-only fixture violation'); }
  });
});
const camera = row=>row.core.pixelWorldCamera;const clearCase=process.env.SEARCH_RACE_CLEAR==='1',scopeCase=process.env.SEARCH_RACE_SCOPE==='1';
try {
 await new Promise(done=>server.listen(0,'127.0.0.1',done));lifecycle.prepare();
 await run(['open','about:blank']);await run(['set','viewport',String(viewportWidth),'1000','1']);await run(['open',`http://scope-observer.localhost:${server.address().port}/viewer.html?test_api=1&connect=1&hosted_bootstrap=0&locale=en`]);
 await evaluate(`(async()=>{const m=await import('/pixel-world-bridge/pixel_world_bridge.js');window.__SCOPE_METRICS__=m.readPixelWorldDisplayMetrics;return true})()`);summary.phases.initial=await observed(3);guest(summary.phases.initial);
 const disclosure='.pixel-world-exploration-disclosure';await run(['click',disclosure+' > summary']);if(clearCase){await run(['fill','.pixel-world-exploration input','scope-B']);await run(['click','.pixel-world-exploration button[type=submit]']);await observed(1);}await run(['fill','.pixel-world-exploration input',clearCase?'':'scope-B']);
 summary.prepared=JSON.parse(await evaluate(`JSON.stringify((()=>{const b=document.querySelector('.pixel-world-exploration button[type=submit]');b.focus();window.__SEARCH_EVENTS__=[];for(const type of ['keydown','click','submit'])document.addEventListener(type,e=>{if(type==='submit'){}if(__SEARCH_EVENTS__.length<32)__SEARCH_EVENTS__.push({type,trusted:e.isTrusted,target:e.target.tagName,focused:document.activeElement===b,disabled:b.disabled,time:e.timeStamp})},true);return{focused:document.activeElement===b,disabled:b.disabled}})())`));
 let ready;gate={received:new Promise(r=>ready=r),ready};for(const socket of wss.clients){const snapshot=fixture.snapshot();snapshot.time=2;send(socket,{type:'snapshot',snapshot});}
 await within(gate.received,'background snapshot read');summary.beforeInput=await state();summary.disabledBeforeInput=await evaluate(`document.querySelector('.pixel-world-exploration button[type=submit]').disabled`);
 const {createOwnedCdpInput}=await import('./viewer-owned-cdp-input.mjs');const input=await createOwnedCdpInput(run,evaluate);try{await input.enter();}finally{input.close();}
 summary.input=JSON.parse(await evaluate('JSON.stringify(__SEARCH_EVENTS__)'));summary.afterInput=await state();await run(['screenshot','--full',resolve(out,'after-input.png')]);
 const wireStart=wire.length,held=gate.held;summary.explicitQueryBoundary={wireStart,connection:ids.get(held.socket),heldRequest:held.message.query.request_generation};gate=null;if(scopeCase)fixture.denyReads();send(held.socket,held.response);summary.phases.result=scopeCase?await wait(`s.metrics?.status==='observed'&&s.metrics.in_view_agent_body_sprites===0&&s.targets.every(t=>t.rect.width===0&&t.rect.height===0&&(t.display==='none'||t.visibility==='hidden'||t.disabled))&&/scope refresh failed|Read unavailable/.test(s.readText)`):await observed(clearCase?3:1);guest(summary.phases.result);if(scopeCase){const r=summary.phases.initial.targets.find(t=>t.id==='scope-B').rect,x=Math.round(r.x+r.width/2),y=Math.round(r.y+r.height/2);assert(await evaluate(`document.elementFromPoint(${x},${y})?.id==='pixel-world-embedded-runtime-canvas'`),'queryerror hit test is covered by UI');await run(['mouse','move',String(x),String(y)]);await run(['mouse','down']);await run(['mouse','up']);summary.phases.afterCanvasHit=await state();guest(summary.phases.afterCanvasHit);assert(!summary.phases.afterCanvasHit.targets.some(t=>t.selected==='true'),'queryerror retained hidden target selected');}const expectedSearch=clearCase?'':'scope-B';assert(wire.slice(wireStart).some(r=>r.direction==='in'&&r.connection===ids.get(held.socket)&&r.message.type==='request_world_view'&&r.message.query.search===expectedSearch&&String(r.message.query.request_generation).split(':')[0]!==String(held.message.query.request_generation).split(':')[0]),'new explicit query never reached the same connection after submit/held boundary');assert(summary.input.filter(r=>r.type==='submit'&&r.trusted).length===1,'explicit trusted submit was not unique');send(held.socket,held.response,true);await run(['wait','250']);summary.phases.afterLate=scopeCase?await wait(`s.metrics?.status==='observed'&&s.metrics.in_view_agent_body_sprites===0&&s.targets.every(t=>t.rect.width===0&&t.rect.height===0&&(t.display==='none'||t.visibility==='hidden'||t.disabled))&&/scope refresh failed|Read unavailable/.test(s.readText)`):await observed(clearCase?3:1);guest(summary.phases.afterLate);if(scopeCase)assert(!summary.phases.afterLate.targets.some(t=>t.selected==='true'),'late hidden target selected');if(!scopeCase){const wanted=clearCase?summary.phases.initial.targets.map(t=>t.id).sort():['scope-B'];for(const row of [summary.phases.result,summary.phases.afterLate])assert(isDeepStrictEqual(row.targets.map(t=>t.id).sort(),wanted),'explicit search result identity changed');}summary.status='passed_explicit_search_only';
} catch(error){summary.status='failed';summary.error=String(error);throw error;}
finally{lifecycle.close();for(const socket of wss.clients)socket.terminate();await new Promise(done=>wss.close(done));await new Promise(done=>server.close(done));summary.fixture=fixture.describe();summary.http=http;writeFileSync(resolve(out,'summary.json'),JSON.stringify(summary,null,2));console.log(out);}
