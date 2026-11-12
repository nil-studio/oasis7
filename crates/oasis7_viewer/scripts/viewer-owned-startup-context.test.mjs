import test from 'node:test';
import assert from 'node:assert/strict';
import { createStartupProfile, launchOwnedStartupContext, readOwnedStartupProfile } from './viewer-owned-startup-context.mjs';
test('only owned lease can launch; sandbox is explicit and warm reopens exact owned directory', async () => {
  const calls = [], chromium = { launchPersistentContext: async (...args) => { calls.push(args); return {}; } };
  await assert.rejects(launchOwnedStartupContext(chromium, '/chrome', { path: '/unowned' }));
  assert.throws(() => readOwnedStartupProfile({}));
  const lease = createStartupProfile();
  const a = await launchOwnedStartupContext(chromium, '/chrome', lease), b = await launchOwnedStartupContext(chromium, '/chrome', lease);
  assert.equal(readOwnedStartupProfile(a), readOwnedStartupProfile(b));
  assert.equal(calls.length, 2); assert.equal(calls[0][0], lease.path); assert.equal(calls[1][0], lease.path);
  assert.equal(calls[0][1].chromiumSandbox, true);
});
