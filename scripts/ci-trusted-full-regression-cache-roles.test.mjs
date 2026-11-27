import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import fs from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import {tar, utils} from '../vendor/actions-cache-explicit-context/index.mjs';
import {fullRegressionCacheLayout} from './ci-trusted-full-regression-cache-roles.mjs';

const a = 'a'.repeat(64);
const b = 'b'.repeat(64);
const c = 'c'.repeat(64);
const cases = [
  ['npm', '/home/runner/.npm', `node-cache-Linux-x64-npm-${a}`, {setupNodeLockDigest: a}],
  ['trunk', '/home/runner/work/_temp/ci-tools/trunk-x86_64-unknown-linux-gnu.tar.gz',
    `trunk-archive-Linux-X64-0.21.14-${b}`, {trunkVersion: '0.21.14', installerHashFiles: b}],
  ['wasm-bindgen', '/home/runner/.cache/oasis7/wasm-bindgen-cli/0.2.127',
    `wasm-bindgen-cli-v1-Linux-X64-1.96.0-0.2.127-${c}`,
    {rustToolchain: '1.96.0', bindgenLockVersion: '0.2.127', helpersHashFiles: c}],
  ['sentinel', '/home/runner/work/_temp/cache-permission-sentinel',
    'oasis7-cache-permission-sentinel-trusted-v2', {}],
];

test('finite role keys and paths preserve the original SDK version geometry', () => {
  for (const [role, raw, key, metadata] of cases) {
    const input = {...metadata, actualRawPath: raw, actualKey: key};
    const layout = fullRegressionCacheLayout(role, input);
    assert.equal(layout.archiveWorkingDirectory, '/home/runner/work/oasis7/oasis7');
    assert.deepEqual(layout.rawPaths, [raw]);
    assert.equal(layout.key, key);
    assert.equal(layout.restore, role !== 'sentinel');
    assert.equal(layout.version, createHash('sha256')
      .update([raw, 'zstd-without-long', '1.0'].join('|')).digest('hex'));
    for (const changed of [{actualRawPath: '/__w' + raw}, {actualKey: key + '-fallback'}]) {
      assert.throws(() => fullRegressionCacheLayout(role, {...input, ...changed}), /root or key/);
    }
  }
  assert.throws(() => fullRegressionCacheLayout('cargo-target', {}), /unsupported/);
  assert.throws(() => fullRegressionCacheLayout('npm', {setupNodeLockDigest: ''}), /lock digest/);
  assert.throws(() => fullRegressionCacheLayout('wasm-bindgen', {rustToolchain: 'nightly'}), /Rust toolchain/);
});

test('real SDK tar resolves each member from the legacy root with explicit cwd', async () => {
  const temporary = await fs.mkdtemp(path.join(os.tmpdir(), 'full-cache-role-'));
  const prior = process.env.GITHUB_WORKSPACE;
  try {
    for (const [role, raw, key, metadata] of cases) {
      const layout = fullRegressionCacheLayout(role, {...metadata, actualRawPath: raw, actualKey: key});
      const workspace = temporary + layout.archiveWorkingDirectory;
      const mapped = path.join(temporary, '__w/oasis7/oasis7');
      const root = temporary + raw;
      const payload = role === 'trunk' ? root : path.join(root, 'payload');
      await fs.mkdir(workspace, {recursive: true});
      await fs.mkdir(mapped, {recursive: true});
      await fs.mkdir(path.dirname(payload), {recursive: true});
      await fs.writeFile(payload, role);
      const archive = await fs.mkdtemp(path.join(temporary, 'archive-'));

      process.env.GITHUB_WORKSPACE = workspace;
      const hostMembers = await utils.resolvePaths([root]);
      process.env.GITHUB_WORKSPACE = mapped;
      const containerMembers = await utils.resolvePaths([root], workspace);
      assert.deepEqual(hostMembers, [layout.member]);
      assert.deepEqual(containerMembers, hostMembers);

      await tar.createTar(archive, containerMembers, layout.compression, workspace);
      await fs.rm(root, {recursive: true});
      process.env.GITHUB_WORKSPACE = workspace;
      await tar.extractTar(path.join(archive, utils.getCacheFileName(layout.compression)), layout.compression);
      assert.equal(await fs.readFile(payload, 'utf8'), role);
    }
  } finally {
    if (prior === undefined) delete process.env.GITHUB_WORKSPACE;
    else process.env.GITHUB_WORKSPACE = prior;
    await fs.rm(temporary, {recursive: true, force: true});
  }
});
