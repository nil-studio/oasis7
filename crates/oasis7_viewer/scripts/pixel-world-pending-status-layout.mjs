/** Actual shell layout with DOM-only status copies; no WS/pending logic proof.
 * PENDING_LAYOUT_ARTIFACT_ROOT=<immutable visual dist>
 * PENDING_LAYOUT_SOURCE_HEAD=<actual source> node scripts/pixel-world-pending-status-layout.mjs
 * Requires agent-browser and Python/Pillow. Does not run a density/performance gate.
 */
import { createServer } from 'node:http';
import { readFileSync, writeFileSync, mkdirSync, readdirSync } from 'node:fs';
import { resolve, dirname, extname, relative } from 'node:path';
import { fileURLToPath } from 'node:url';
import { execFile, execFileSync } from 'node:child_process';
import { promisify, isDeepStrictEqual } from 'node:util';
import { createHash } from 'node:crypto';
import { createOwnedSessionLifecycle } from './agent-browser-visual-runner-lifecycle.mjs';

const assert = (value, text) => { if (!value) throw Error(text); };
assert(process.env.PENDING_LAYOUT_ARTIFACT_ROOT && process.env.PENDING_LAYOUT_SOURCE_HEAD, 'declare immutable artifacts and actual source');
const here = dirname(fileURLToPath(import.meta.url)), repo = resolve(here, '../../..');
const root = resolve(process.env.PENDING_LAYOUT_ARTIFACT_ROOT);
const out = resolve(process.env.PENDING_LAYOUT_OUTPUT || resolve(repo, 'output/pending-layout-5606', new Date().toISOString().replace(/[:.]/g, '-')));
mkdirSync(out, { recursive: true });
const sha = bytes => createHash('sha256').update(bytes).digest('hex');
const files = dir => readdirSync(dir, { withFileTypes: true }).flatMap(e => e.isDirectory() ? files(resolve(dir, e.name)) : [resolve(dir, e.name)]);
const manifest = () => Object.fromEntries(files(root).sort().map(p => [relative(root, p), { bytes: readFileSync(p).length, sha256: sha(readFileSync(p)) }]));
const assets = manifest(), http = [];
const summary = { source: execFileSync('git', ['rev-parse', process.env.PENDING_LAYOUT_SOURCE_HEAD + '^{commit}'], { cwd: repo, encoding: 'utf8' }).trim(),
  runnerHead: execFileSync('git', ['rev-parse', 'HEAD'], { cwd: repo, encoding: 'utf8' }).trim(), runnerSha256: sha(readFileSync(fileURLToPath(import.meta.url))), assets,
  scope: 'Actual ready empty-world shell; DOM-only Selected A and status copies, including feed open/closed. No WS, pending logic, player authorization, density or performance proof.', rows: [] };
const server = createServer((req, res) => {
  try {
    const pathname = new URL(req.url, 'http://unused').pathname;
    http.push({ method: req.method, pathname });
    const p = resolve(root, '.' + (pathname === '/' ? '/viewer.html' : pathname));
    assert(p.startsWith(root + '/'), 'path escaped artifact root');
    let bytes = readFileSync(p);
    if (pathname.endsWith('.html')) {
      const config = `<script id="oasis7-viewer-runtime-config" type="application/json">${JSON.stringify({ deploymentMode: 'trusted_local_only', viewerWsEndpoint: `ws://pending-layout.localhost:${server.address().port}/unused` })}</script>`;
      let html = bytes.toString(), count = [...html.matchAll(/<script\b[^>]*id=["']oasis7-viewer-runtime-config["'][^>]*>[\s\S]*?<\/script>/g)].length;
      assert(count <= 1, 'ambiguous trusted runtime configuration');
      html = count ? html.replace(/<script\b[^>]*id=["']oasis7-viewer-runtime-config["'][^>]*>[\s\S]*?<\/script>/, config) : html.replace('</head>', config + '</head>');
      bytes = Buffer.from(html);
    }
    res.writeHead(200, { 'Content-Type': ({ '.html': 'text/html; charset=utf-8', '.js': 'text/javascript', '.css': 'text/css', '.wasm': 'application/wasm' })[extname(p)] || 'application/octet-stream', 'Cache-Control': 'no-store' }); res.end(bytes);
  } catch (error) { summary.serverError = String(error); res.writeHead(404); res.end(); }
});
let upgrades = 0;
server.on('upgrade', (_req, socket) => { upgrades++; socket.destroy(); });
const command = process.env.AGENT_BROWSER_BIN || 'agent-browser', session = 'pending-layout-' + process.pid;
const lifecycle = createOwnedSessionLifecycle({ command, session });
const exec = promisify(execFile);
const run = async args => (await exec(command, ['--session', session, ...args], { timeout: 30000, maxBuffer: 8 * 1024 * 1024 })).stdout;
const evaluate = async code => { const r = JSON.parse(await run(['--json', 'eval', code])); assert(r.success, JSON.stringify(r)); return r.data.result; };
const canvasState = `(()=>{const c=document.querySelector('#pixel-world-embedded-runtime-canvas');return{rect:c.getBoundingClientRect().toJSON(),width:c.width,height:c.height}})()`;
const pixelOracle = String.raw`import json,sys,math
from PIL import Image
r=json.loads(sys.argv[3]);a=Image.open(sys.argv[1]).convert('RGB');b=Image.open(sys.argv[2]).convert('RGB')
assert a.size==b.size
box=(math.floor(r['left']),math.floor(r['top']),math.ceil(r['right']),math.ceil(r['bottom']))
assert 0<=box[0]<box[2]<=a.width and 0<=box[1]<box[3]<=a.height
changed=sum(max(abs(x[k]-y[k]) for k in range(3))>12 for x,y in zip(a.crop(box).getdata(),b.crop(box).getdata()))
print(json.dumps({'box':box,'changedGlyphPixels':changed,'passed':changed>0}))`;
try {
  await new Promise(done => server.listen(0, '127.0.0.1', done)); lifecycle.prepare(); await run(['open', 'about:blank']);
  for (const [width, height] of [[1440, 1000], [390, 844]]) {
    await run(['set', 'viewport', String(width), String(height), '1']);
    await run(['open', `http://pending-layout.localhost:${server.address().port}/viewer.html?test_api=1&connect=0&hosted_bootstrap=0&locale=en`]);
    await evaluate(`(async()=>{const end=Date.now()+15000;while(Date.now()<end){if(document.querySelector('.pixel-world-canvas')?.dataset.rendererReady==='true')return true;await new Promise(r=>setTimeout(r,100))}throw Error('empty-world shell renderer not ready')})()`);
    for (const feedOpen of [false, true]) for (const status of ['pending', 'unavailable']) {
      const name = `${width}-${feedOpen ? 'feedopen' : 'feedclosed'}-${status}`;
      await evaluate(`(()=>{const c=document.querySelector('.pixel-world-canvas'),feed=document.querySelector('[data-viewer-overlay=feed]');if(feed instanceof HTMLDetailsElement)feed.open=${feedOpen};let chip=c.querySelector('.pixel-world-canvas__selection');if(!chip){chip=document.createElement('div');chip.className='pixel-world-canvas__selection';c.append(chip)}chip.innerHTML='<span>Selected: Agent A</span>';return true})()`);
      const before = await evaluate(canvasState); await run(['screenshot', '--full', resolve(out, name + '-no-status.png')]);
      const row = await evaluate(`(()=>{const chip=document.querySelector('.pixel-world-canvas__selection'),s=document.createElement('div');s.className='pixel-world-selection-status';s.setAttribute('role','status');s.setAttribute('aria-live','polite');s.dataset.worldSelectionStatus=${JSON.stringify(status)};s.textContent=${JSON.stringify(status === 'pending' ? 'Checking the selected object…' : 'This object could not be confirmed. Select again.')};chip.append(s);const range=document.createRange();range.selectNodeContents(s);const r=range.getBoundingClientRect(),rect=s.getBoundingClientRect();window.__LAYOUT_INPUT__=[];document.addEventListener('pointerdown',e=>__LAYOUT_INPUT__.push({trusted:e.isTrusted,target:e.target.tagName,id:e.target.id}),{once:true,capture:true});return{viewport:{width:innerWidth,height:innerHeight},dpr:devicePixelRatio,status:${JSON.stringify(status)},feedOpen:${feedOpen},textRect:r.toJSON(),rect:rect.toJSON(),chip:chip.getBoundingClientRect().toJSON(),pointer:[getComputedStyle(s).pointerEvents,getComputedStyle(chip).pointerEvents],text:chip.textContent,overlays:[...document.querySelectorAll('[data-viewer-overlay]')].map(e=>({kind:e.dataset.viewerOverlay,rect:e.getBoundingClientRect().toJSON(),z:getComputedStyle(e).zIndex})),auth:window.__AW_TEST__.getState().authReady}})()`);
      assert(row.dpr === 1 && row.textRect.top >= 0 && row.textRect.bottom <= height && row.textRect.left >= 0 && row.textRect.right <= width, 'text range outside viewport');
      assert(row.pointer.every(value => value === 'none') && row.auth === false && row.text.startsWith('Selected: Agent A'), 'status changed identity or intercepted input');
      const positive = resolve(out, name + '-positive.png'), negative = resolve(out, name + '-transparent-text.png');
      await run(['screenshot', '--full', positive]);
      await evaluate(`document.querySelector('.pixel-world-selection-status').style.color='transparent';true`);
      await run(['screenshot', '--full', negative]);
      await evaluate(`document.querySelector('.pixel-world-selection-status').style.removeProperty('color');true`);
      row.glyphOracle = JSON.parse(execFileSync('python3', ['-c', pixelOracle, positive, negative, JSON.stringify(row.textRect)], { encoding: 'utf8' }));
      assert(row.glyphOracle.passed, 'status glyphs were occluded or clipped');
      await run(['mouse', 'move', String(Math.round(row.rect.x + row.rect.width / 2)), String(Math.round(row.rect.y + row.rect.height / 2))]);
      await run(['mouse', 'down']); await run(['mouse', 'up']);
      row.input = JSON.parse(await evaluate('JSON.stringify(window.__LAYOUT_INPUT__)')); row.before = before; row.after = await evaluate(canvasState);
      assert(row.input.length === 1 && row.input[0].trusted && row.input[0].target === 'CANVAS' && row.input[0].id === 'pixel-world-embedded-runtime-canvas', 'status did not pass native input to actual canvas');
      assert(isDeepStrictEqual(before, row.after), 'status changed canvas CSS/backing layout'); summary.rows.push(row);
    }
  }
  assert(upgrades === 0 && !http.some(row => !['GET', 'HEAD'].includes(row.method)), 'layout probe performed non-read transport');
  assert(isDeepStrictEqual(assets, manifest()), 'immutable artifacts changed'); summary.status = 'passed_layout_scope_only';
} catch (error) { summary.status = 'failed'; summary.error = String(error); throw error; }
finally { lifecycle.close(); await new Promise(done => server.close(done)); summary.http = http; summary.wsUpgrades = upgrades; writeFileSync(resolve(out, 'summary.json'), JSON.stringify(summary, null, 2)); console.log(out); }
