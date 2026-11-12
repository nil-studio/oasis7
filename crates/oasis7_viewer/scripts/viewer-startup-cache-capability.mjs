// Three bounded cache capability cases. No Viewer budget, frame or authority claim.
import { createServer } from 'node:http';
import { createRequire } from 'node:module';
import { mkdtempSync, mkdirSync, writeFileSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve, join } from 'node:path';
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { verifyStartupCacheCases } from './viewer-startup-cache-policy.mjs';
const modules = process.env.OASIS7_PLAYWRIGHT_NODE_MODULES || '/Users/scc/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules';
const { chromium } = createRequire(join(modules, 'startup-capability.cjs'))('playwright');
const chrome = process.env.OASIS7_PLAYWRIGHT_CHROME_PATH || '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome';
if (!existsSync(chrome)) throw Error('declare an existing Chrome executable');
if (!process.env.VIEWER_STARTUP_OUTPUT) throw Error('declare new VIEWER_STARTUP_OUTPUT');
const out = resolve(process.env.VIEWER_STARTUP_OUTPUT); mkdirSync(out); // No evidence overwrite.
const profileA = mkdtempSync(join(tmpdir(), 'oasis7-startup-A-'));
const profileB = mkdtempSync(join(tmpdir(), 'oasis7-startup-B-'));
// Empty valid module isolates browser HTTP/cache mechanics, not product compilation cost.
const wasm = Buffer.from([0,97,115,109,1,0,0,0]);
let requests = 0;
const server = createServer((req, res) => {
  if (req.url === '/cache.wasm') { requests++; res.writeHead(200, { 'Content-Type': 'application/wasm', 'Cache-Control': 'public,max-age=31536000,immutable' }); res.end(wasm); }
  else if (req.url === '/') { res.writeHead(200, { 'Content-Type': 'text/html', 'Cache-Control': 'no-store' }); res.end('<!doctype html><title>Owned startup cache capability</title>'); }
  else { res.writeHead(404); res.end(); }
});
await new Promise((yes, no) => { server.once('error', no); server.listen(0, '127.0.0.1', yes); });
const url = `http://127.0.0.1:${server.address().port}/`;
const bounded = async (operation, label) => {
  let timer;
  try { return await Promise.race([operation, new Promise((_, no) => { timer = setTimeout(() => no(Error(label + ' exceeded 10s')), 10000); })]); }
  finally { clearTimeout(timer); }
};
const scripts = Object.fromEntries(['viewer-startup-cache-capability.mjs', 'viewer-startup-cache-policy.mjs'].map(name => [name, createHash('sha256').update(readFileSync(new URL(name, import.meta.url))).digest('hex')]));
const result = { cases: [], scripts, source: execFileSync('git', ['rev-parse', 'HEAD'], { encoding: 'utf8' }).trim(), limitations: ['Not Viewer UI or first-frame timing.', 'No OS page-cache purge; no WASM compiled-code-cache claim.', 'Not authenticated player, A/B network or P6 acceptance.'], profiles: [profileA, profileB] };
let originalError;
try {
  for (const [name, profile] of [['cold-first', profileA], ['warm-same-profile', profileA], ['cold-independent', profileB]]) {
    const context = await chromium.launchPersistentContext(profile, { executablePath: chrome, headless: true, viewport: { width: 1440, height: 1000 }, timeout: 15000 });
    const row = { name, profile, responses: [], networkRequests: 0, wasmCompiled: false };
    let actionError;
    try {
      if (context.pages().length !== 1) throw Error('owned fresh profile did not contain exactly one page');
      const page = context.pages()[0]; const cdp = await bounded(context.newCDPSession(page), 'attach owned page');
      await bounded(cdp.send('Network.enable'), 'enable network observation');
      cdp.on('Network.responseReceived', e => { if (e.response.url === url + 'cache.wasm') row.responses.push({ status: e.response.status, fromDiskCache: !!e.response.fromDiskCache, fromServiceWorker: !!e.response.fromServiceWorker }); });
      await page.goto(url, { waitUntil: 'load', timeout: 10000 });
      const before = requests;
      row.wasmCompiled = await bounded(page.evaluate(async () => { const module = await WebAssembly.compileStreaming(fetch('/cache.wasm')); return module instanceof WebAssembly.Module; }), 'compile capability module');
      row.networkRequests = requests - before;
      row.browserVersion = context.browser()?.version() || await bounded(cdp.send('Browser.getVersion'), 'read version');
    } catch (error) { actionError = error; throw error; }
    finally { try { await context.close(); } catch (error) { row.cleanupError = String(error); if (!actionError) throw error; } result.cases.push(row); }
  }
  result.verdict = verifyStartupCacheCases(result.cases);
} catch (error) { originalError = error; result.error = String(error); }
finally {
  await new Promise(yes => server.close(yes));
  // Keep owned profiles as evidence; never clear another profile or an OS cache.
  try { writeFileSync(join(out, 'summary.json'), JSON.stringify(result, null, 2)); }
  catch (error) { if (!originalError) originalError = error; else console.error('evidence write failed:', error); }
}
if (originalError) throw originalError;
console.log(JSON.stringify(result.verdict));
