import test from 'node:test';
import assert from 'node:assert/strict';
import { EventEmitter } from 'node:events';
import { rm } from 'node:fs/promises';
import { createStartupProfile, launchOwnedStartupContext, readOwnedStartupProfile } from './viewer-owned-startup-context.mjs';
test('only owned lease can launch; sandbox is explicit and warm reopens exact owned directory', async () => {
  const calls = [], chromium = { launchPersistentContext: async (...args) => { calls.push(args); return new EventEmitter(); } };
  await assert.rejects(launchOwnedStartupContext(chromium, '/chrome', { path: '/unowned' }));
  assert.throws(() => readOwnedStartupProfile({}));
  const lease = createStartupProfile();
  const a = await launchOwnedStartupContext(chromium, '/chrome', lease), b = await launchOwnedStartupContext(chromium, '/chrome', lease);
  assert.equal(readOwnedStartupProfile(a), readOwnedStartupProfile(b));
  assert.equal(calls.length, 2); assert.equal(calls[0][0], lease.path); assert.equal(calls[1][0], lease.path);
  assert.equal(calls[0][1].chromiumSandbox, true);
  assert.deepEqual(calls[0][1].args, ['--enable-automation']);
  a.emit('close'); assert.throws(() => readOwnedStartupProfile(a));
  assert.equal(readOwnedStartupProfile(b), lease.path);
});


test('a clean owned profile defaults to full Chromium without disabling the sandbox', async () => {
  const lease = createStartupProfile();
  try {
    const chromium = { launchPersistentContext: async (dir, options) => {
      assert.equal(dir, lease.path);
      assert.equal(options.channel, 'chromium');
      assert.equal('executablePath' in options, false);
      assert.equal(options.chromiumSandbox, true);
      return new EventEmitter();
    } };
    const context = await launchOwnedStartupContext(chromium, undefined, lease);
    assert.equal(readOwnedStartupProfile(context), lease.path);
  } finally {
    await rm(lease.path, { recursive: true, force: true });
  }
});
