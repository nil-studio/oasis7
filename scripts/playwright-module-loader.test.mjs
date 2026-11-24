import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, mkdirSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';
import { loadPlaywright } from './playwright-module-loader.mjs';

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

  await test('finds Playwright near the invoking script without a platform runtime', () => {
    delete process.env.OASIS7_PLAYWRIGHT_NODE_MODULES;
    assert.equal(loadPlaywright(caller).source, 'local');
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
} finally {
  if (previous === undefined) delete process.env.OASIS7_PLAYWRIGHT_NODE_MODULES;
  else process.env.OASIS7_PLAYWRIGHT_NODE_MODULES = previous;
  rmSync(tmp, { recursive: true, force: true });
}
