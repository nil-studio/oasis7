import test from 'node:test';
import assert from 'node:assert/strict';
import { copyFileSync, mkdtempSync, mkdirSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';
import { browserExecutablePath, fullBrowserLaunchOptions, loadPlaywright } from './playwright-module-loader.mjs';

const tmp = mkdtempSync(join(tmpdir(), 'oasis7-playwright-loader-'));
const previous = process.env.OASIS7_PLAYWRIGHT_NODE_MODULES;
try {
  const caller = pathToFileURL(join(tmp, 'caller', 'sample.mjs')).href;
  const localPackage = join(tmp, 'node_modules', 'playwright');
  const overrideRoot = join(tmp, 'override');
  const overridePackage = join(overrideRoot, 'node_modules', 'playwright');
  mkdirSync(localPackage, { recursive: true });
  mkdirSync(overridePackage, { recursive: true });
  writeFileSync(join(localPackage, 'index.js'), "module.exports = { source: 'local' };\n");
  writeFileSync(join(overridePackage, 'index.js'), "module.exports = { source: 'override' };\n");

  await test('finds Playwright near the invoking script without a platform runtime', async () => {
    // Isolate the fallback check from any real repository-pinned installation.
    const copied = join(tmp, 'fallback-loader.mjs');
    copyFileSync(new URL('./playwright-module-loader.mjs', import.meta.url), copied);
    const { loadPlaywright: fromFallback } = await import(pathToFileURL(copied));
    assert.equal(fromFallback(caller, {}).source, 'local');
  });

  await test('explicit node_modules directory wins over the local module', () => {
    process.env.OASIS7_PLAYWRIGHT_NODE_MODULES = join(overrideRoot, 'node_modules');
    assert.equal(loadPlaywright(caller).source, 'override');
  });

  await test('invalid explicit override fails clearly rather than falling back', () => {
    process.env.OASIS7_PLAYWRIGHT_NODE_MODULES = join(tmp, 'absent');
    assert.throws(() => loadPlaywright(caller), /OASIS7_PLAYWRIGHT_NODE_MODULES/);
  });

  await test('missing transitive dependency retains its original error', () => {
    const broken = join(tmp, 'broken');
    mkdirSync(join(broken, 'playwright'), { recursive: true });
    writeFileSync(join(broken, 'playwright', 'index.js'), "require('oasis7-intentionally-missing-transitive');\n");
    process.env.OASIS7_PLAYWRIGHT_NODE_MODULES = broken;
    assert.throws(() => loadPlaywright(caller), /oasis7-intentionally-missing-transitive/);
  });

  await test('CI package export wins over caller-local but follows an explicit directory', () => {
    assert.equal(loadPlaywright(caller, { OASIS7_PLAYWRIGHT_MODULE: overridePackage }).source, 'override');
    assert.equal(loadPlaywright(caller, {
      OASIS7_PLAYWRIGHT_NODE_MODULES: join(tmp, 'node_modules'),
      OASIS7_PLAYWRIGHT_MODULE: overridePackage,
    }).source, 'local');
    assert.throws(
      () => loadPlaywright(caller, { OASIS7_PLAYWRIGHT_MODULE: join(tmp, 'absent') }),
      /OASIS7_PLAYWRIGHT_MODULE/,
    );
  });

  await test('resolves a pinned sibling installation without modifying the real repository', async () => {
    const scripts = join(tmp, 'repo', 'scripts');
    const pinned = join(scripts, 'ci-browser-tools', 'node_modules', 'playwright');
    mkdirSync(pinned, { recursive: true });
    writeFileSync(join(pinned, 'index.js'), "module.exports = { source: 'pinned' };\n");
    const copied = join(scripts, 'playwright-module-loader.mjs');
    copyFileSync(new URL('./playwright-module-loader.mjs', import.meta.url), copied);
    const { loadPlaywright: fromPinned } = await import(pathToFileURL(copied));
    assert.equal(fromPinned(caller, {}).source, 'pinned');
    assert.equal(fromPinned(caller, { OASIS7_PLAYWRIGHT_MODULE: overridePackage }).source, 'override');
  });

  await test('full-browser probes use managed Chromium unless given an executable', () => {
    assert.deepEqual(fullBrowserLaunchOptions(browserExecutablePath('')), { channel: 'chromium' });
    assert.equal(browserExecutablePath(process.execPath), process.execPath);
    assert.deepEqual(fullBrowserLaunchOptions(browserExecutablePath(process.execPath)), { executablePath: process.execPath });
    assert.throws(() => browserExecutablePath(tmp), /executable file/);
  });
} finally {
  if (previous === undefined) delete process.env.OASIS7_PLAYWRIGHT_NODE_MODULES;
  else process.env.OASIS7_PLAYWRIGHT_NODE_MODULES = previous;
  rmSync(tmp, { recursive: true, force: true });
}
