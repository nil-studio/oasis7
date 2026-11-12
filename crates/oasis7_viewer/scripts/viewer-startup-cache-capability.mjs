// Three bounded cache capability cases. No Viewer budget, frame or authority claim.
import { createServer } from 'node:http';
import { createRequire } from 'node:module';
import { mkdirSync, writeFileSync, existsSync } from 'node:fs';
import { resolve, join } from 'node:path';
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { verifyStartupCacheCases } from './viewer-startup-cache-policy.mjs';
import { createStartupProfile, launchOwnedStartupContext, readOwnedStartupProfile } from './viewer-owned-startup-context.mjs';
const modules = process.env.OASIS7_PLAYWRIGHT_NODE_MODULES || '/Users/scc/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules';
const { chromium } = createRequire(join(modules, 'startup-capability.cjs'))('playwright');
const chrome = process.env.OASIS7_PLAYWRIGHT_CHROME_PATH || '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome';
if (!existsSync(chrome)) throw Error('declare an existing Chrome executable');
if (!process.env.VIEWER_STARTUP_OUTPUT) throw Error('declare new VIEWER_STARTUP_OUTPUT');
const out = resolve(process.env.VIEWER_STARTUP_OUTPUT); mkdirSync(out); // No evidence overwrite.
const profileA = createStartupProfile();
const profileB = createStartupProfile();
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
const scripts = Object.fromEntries(['viewer-startup-cache-capability.mjs', 'viewer-startup-cache-policy.mjs', 'viewer-owned-startup-context.mjs'].map(name => [name, createHash('sha256').update(readFileSync(new URL(name, import.meta.url))).digest('hex')]));
const result = { cases: [], scripts, source: execFileSync('git', ['rev-parse', 'HEAD'], { encoding: 'utf8' }).trim(), limitations: ['Not Viewer UI or first-frame timing.', 'No OS page-cache purge; no WASM compiled-code-cache claim.', 'Not authenticated player, A/B network or P6 acceptance.'], profiles: [profileA.path, profileB.path] };
result.hardStopMs = 120000;
const hardStop = setTimeout(() => { console.error('owned startup capability exceeded hard stop; cleanup unproven'); process.exit(1); }, result.hardStopMs);
let originalError, failed = false;
try {
  for (const [name, profile] of [['cold-first', profileA], ['warm-same-profile', profileA], ['cold-independent', profileB]]) {
    const context = await launchOwnedStartupContext(chromium, chrome, profile);
    const row = { name, profile: readOwnedStartupProfile(context), responses: [], networkRequests: 0, wasmCompiled: false, nativePid: null, processExited: false };
    let actionFailed = false;
    try {
      if (context.pages().length !== 1) throw Error('owned fresh profile did not contain exactly one page');
      const browserCdp = await bounded(context.browser().newBrowserCDPSession(), 'attach owned browser');
      const commandLine = await bounded(browserCdp.send('Browser.getBrowserCommandLine'), 'verify owned profile command line');
      if (commandLine.arguments.filter(x => x === '--user-data-dir=' + profile.path).length !== 1) throw Error('native browser profile ownership unproven');
      const processes = await bounded(browserCdp.send('SystemInfo.getProcessInfo'), 'read owned browser PID');
      const browsers = processes.processInfo.filter(p => p.type === 'browser');
      if (browsers.length !== 1 || !Number.isSafeInteger(browsers[0].id) || browsers[0].id <= 0) throw Error('native browser PID unproven');
      row.nativePid = browsers[0].id;
      const page = context.pages()[0]; const cdp = await bounded(context.newCDPSession(page), 'attach owned page');
      await bounded(cdp.send('Network.enable'), 'enable network observation');
      cdp.on('Network.responseReceived', e => { if (e.response.url === url + 'cache.wasm') row.responses.push({ status: e.response.status, fromDiskCache: !!e.response.fromDiskCache, fromServiceWorker: !!e.response.fromServiceWorker }); });
      await page.goto(url, { waitUntil: 'load', timeout: 10000 });
      const before = requests;
      row.wasmCompiled = await bounded(page.evaluate(async () => { const module = await WebAssembly.compileStreaming(fetch('/cache.wasm')); return module instanceof WebAssembly.Module; }), 'compile capability module');
      row.networkRequests = requests - before;
      row.browserVersion = context.browser()?.version() || await bounded(cdp.send('Browser.getVersion'), 'read version');
    } catch (error) { actionFailed = true; throw error; }
    finally {
      try {
        await bounded(context.close(), 'close owned browser');
        if (!row.nativePid) throw Error('owned native browser cleanup cannot be verified');
        const end = performance.now() + 2000;
        while (performance.now() < end) {
          try { process.kill(row.nativePid, 0); } catch (error) { if (error.code === 'ESRCH') { row.processExited = true; break; } throw error; }
          await new Promise(yes => setTimeout(yes, 50));
        }
        if (!row.processExited) throw Error('owned browser PID still exists after close; stop remaining cases');
      } catch (error) { row.cleanupError = String(error); if (!actionFailed) throw error; }
      result.cases.push(row);
    }
  }
  result.verdict = verifyStartupCacheCases(result.cases);
} catch (error) { failed = true; originalError = error; result.error = String(error); }
finally {
  try { await new Promise((yes, no) => server.close(error => error ? no(error) : yes())); }
  catch (error) { result.serverCleanupError = String(error); if (!failed) { failed = true; originalError = error; } }
  // Keep owned profiles as evidence; never clear another profile or an OS cache.
  try { writeFileSync(join(out, 'summary.json'), JSON.stringify(result, null, 2)); }
  catch (error) { if (!failed) { failed = true; originalError = error; } else console.error('evidence write failed:', error); }
  clearTimeout(hardStop);
}
if (failed) throw originalError;
console.log(JSON.stringify(result.verdict));
