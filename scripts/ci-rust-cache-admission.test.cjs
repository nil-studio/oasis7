#!/usr/bin/env node
'use strict';
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const crypto = require('node:crypto');
const childProcess = require('node:child_process');
const vm = require('node:vm');
const {test} = require('node:test');
const guard = require('./ci-rust-cache-admission-hook.cjs');

function fixture(t) {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'dependency-admission-'));
  t.after(() => fs.rmSync(root, {recursive: true, force: true}));
  const workspace = path.join(root, 'workspace');
  const directory = path.join(root, 'private cache');
  const action = path.join(root, 'action');
  for (const value of [workspace, directory, action]) fs.mkdirSync(value);
  const archive = path.join(directory, 'cache.tzst');
  fs.writeFileSync(archive, Buffer.alloc(100));
  fs.writeFileSync(path.join(directory, 'manifest.txt'), 'target\n');
  const hashes = {};
  for (const filename of Object.keys(guard.PINNED_HASHES)) {
    const data = 'isolated action fixture: ' + filename;
    fs.writeFileSync(path.join(action, filename), data);
    hashes[filename] = crypto.createHash('sha256').update(data).digest('hex');
  }
  const environment = {RUNNER_TEMP: root, GITHUB_WORKSPACE: workspace, 'INPUT_CACHE-PROVIDER': 'github'};
  function install(checkArchive = () => {}, changes = {}) {
    const installed = guard.installGuard({entry: path.join(action, 'save.js'), hashes,
      environment, checkArchive, ...changes});
    t.after(() => installed.restore());
    return installed;
  }
  return {root, workspace, directory, action, archive, hashes, environment, install};
}

test('only the fixed entry, provider and complete bundle chain are accepted', t => {
  const f = fixture(t);
  assert.equal(guard.inspectAction(path.join(f.action, 'restore.js'), f.hashes), 'restore');
  for (const entry of [undefined, 'save.js', path.join(f.action, 'post.js')]) {
    assert.throws(() => f.install(undefined, {entry}), /unknown action entry/);
  }
  assert.throws(() => f.install(undefined, {environment: {...f.environment, 'INPUT_CACHE-PROVIDER': 'other'}}), /provider/);
  fs.appendFileSync(path.join(f.action, 'cache-Dl81DHPs.js'), 'changed');
  assert.throws(() => f.install(), /bundle changed/);
});

test('a recognized restore never admits or blocks downloaded archives', t => {
  const f = fixture(t);
  const original = fs.statSync;
  f.install(() => { throw new Error('restore must not authorize uploads'); }, {entry: path.join(f.action, 'restore.js')});
  assert.equal(fs.statSync, original);
  assert.equal(fs.statSync(f.archive).size, 100);
});

test('save admits final bytes once and synchronizes the upstream ESM fs binding', async t => {
  const f = fixture(t);
  const esmFs = await import('node:fs');
  let checks = 0;
  f.install(file => { assert.equal(file, f.archive); checks += 1; });
  assert.equal(esmFs.statSync(f.archive).size, 100);
  assert.equal(fs.statSync(f.archive, {bigint: true}).size, 100n);
  fs.statSync(path.join(f.directory, 'manifest.txt'));
  assert.equal(checks, 1, 'later transport stat calls must not repeat REST admission after reserve');
  fs.appendFileSync(f.archive, 'changed');
  assert.throws(() => fs.statSync(f.archive), /changed after admission/);
  assert.equal(checks, 1);
});

test('denial and mutation during admission stop every archive measurement', t => {
  const f = fixture(t);
  let checks = 0;
  const denied = f.install(() => { checks += 1; throw new Error('capacity gap'); });
  assert.throws(() => fs.statSync(f.archive), /capacity gap/);
  assert.throws(() => fs.statSync(f.archive), /capacity gap/);
  assert.equal(checks, 2, 'a rejection must not be memoized as allowed');
  denied.restore();
  f.install(() => fs.appendFileSync(f.archive, 'changed'));
  assert.throws(() => fs.statSync(f.archive), /changed during admission/);
});

test('archive paths, manifest, symlinks and multiple links fail closed', t => {
  const f = fixture(t);
  f.install(() => { throw new Error('invalid archives must not reach admission'); });
  const outside = path.join(f.workspace, 'cache.tzst');
  fs.writeFileSync(outside, 'payload');
  fs.writeFileSync(path.join(f.workspace, 'manifest.txt'), 'target\n');
  assert.throws(() => fs.statSync(outside), /private runner temp/);
  assert.throws(() => fs.statSync('cache.tzst'), /archive paths/);
  fs.linkSync(f.archive, path.join(f.directory, 'second-link'));
  assert.throws(() => fs.statSync(f.archive), /single-link/);
  fs.unlinkSync(path.join(f.directory, 'second-link'));
  fs.unlinkSync(f.archive);
  fs.symlinkSync(path.join(f.directory, 'manifest.txt'), f.archive);
  assert.throws(() => fs.statSync(f.archive), /single-link/);
  fs.unlinkSync(f.archive);
  fs.writeFileSync(f.archive, 'payload');
  fs.unlinkSync(path.join(f.directory, 'manifest.txt'));
  assert.throws(() => fs.statSync(f.archive), /ENOENT/);
});

test('the Python verifier receives argv paths without a shell or cache transport credentials', t => {
  const f = fixture(t);
  const originalSpawn = childProcess.spawnSync;
  const originalWrite = process.stdout.write;
  t.after(() => { childProcess.spawnSync = originalSpawn; process.stdout.write = originalWrite; });
  process.stdout.write = () => true;
  const python = path.join(f.root, 'Python with spaces', 'python');
  const environment = {...f.environment, OASIS7_DEPENDENCY_CACHE_PYTHON: python,
    GH_TOKEN: 'read-only-rest-token', NODE_OPTIONS: '--require guard.cjs',
    ACTIONS_RUNTIME_TOKEN: 'transport-token', ACTIONS_CACHE_URL: 'cache-url', ACTIONS_RESULTS_URL: 'results-url'};
  const report = {mode: 'dependency-cache-upload-admission', allowed: true, archive_bytes: 100};
  let reply = {status: 0, stdout: JSON.stringify(report) + '\n'};
  childProcess.spawnSync = (command, args, options) => {
    assert.equal(command, python);
    assert.deepEqual(args, [path.join(__dirname, 'ci-rust-cache-admission.py'), 'check', '--archive', f.archive]);
    assert.equal(options.shell, false);
    assert.equal(options.env.GH_TOKEN, 'read-only-rest-token');
    for (const name of ['NODE_OPTIONS', 'ACTIONS_RUNTIME_TOKEN', 'ACTIONS_CACHE_URL', 'ACTIONS_RESULTS_URL']) {
      assert.equal(options.env[name], undefined);
    }
    return reply;
  };
  guard.checkWithPython(f.archive, environment);
  for (const invalid of [
    {status: 78, stdout: JSON.stringify({...report, allowed: false})},
    {status: 0, stdout: JSON.stringify({...report, archive_bytes: 99})},
    {status: 0, stdout: JSON.stringify({...report, allowed: false})},
    {status: 0, stdout: 'not a JSON report'},
    {status: null, error: new Error('timeout'), stdout: JSON.stringify(report)},
  ]) {
    reply = invalid;
    assert.throws(() => guard.checkWithPython(f.archive, environment), /Rust dependency cache upload guard/);
  }
});

test('the real preload cannot silently become disabled on an unknown entry', () => {
  const result = childProcess.spawnSync(process.execPath, ['-e', 'console.log("UNGUARDED-MAIN")'], {
    env: {...process.env, NODE_OPTIONS: '--require "' + path.join(__dirname, 'ci-rust-cache-admission.cjs') + '"',
      'INPUT_CACHE-PROVIDER': 'github'}, encoding: 'utf8', shell: false,
  });
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /unknown action entry/);
  assert.doesNotMatch(result.stdout, /UNGUARDED-MAIN/);
});

// An optional local source argument exercises the actual fixed upstream save
// implementations, without vendoring 1.8 MB or making normal CI need a network.
// Fetch cache-Dl81DHPs.js from ACTION_REVISION, then pass its path to this test.
// The fixed digest is verified before any upstream code is executed.
const providerSource = process.argv[2];
for (const version of ['V1', 'V2']) {
  test('fixed upstream ' + version + ' does not reserve or upload a rejected archive',
    {skip: !providerSource}, async t => {
      const source = fs.readFileSync(providerSource, 'utf8');
      assert.equal(crypto.createHash('sha256').update(source).digest('hex'), guard.PINNED_HASHES['cache-Dl81DHPs.js']);
      function extract(name) {
        const start = source.indexOf('function ' + name + '(');
        assert.ok(start >= 0);
        return source.slice(start, source.indexOf('\n}', start) + 2);
      }
      for (const allow of [false, true]) {
        const f = fixture(t);
        const calls = {admit: 0, reserve: 0, upload: 0, finalize: 0};
        const installed = f.install(() => { calls.admit += 1; if (!allow) throw new Error('capacity gap'); });
        const quiet = () => {};
        class ValidationError extends Error {}
        class ReserveCacheError extends Error {}
        class FinalizeCacheError extends Error {}
        class HttpClientError extends Error {}
        class CacheWriteDeniedError extends Error {}
        const context = vm.createContext({fs, path, ValidationError, ReserveCacheError,
          FinalizeCacheError, HttpClientError, CacheWriteDeniedError,
          CACHE_WRITE_DENIED_PREFIX: 'cache write denied:',
          debug: quiet, info: quiet, error: quiet, warning: quiet,
          getCompressionMethod: async () => 'zstd', resolvePaths: async paths => paths,
          createTempDirectory: async () => f.directory, getCacheFileName: () => 'cache.tzst',
          createTar: async () => {}, isDebug: () => false, isGhes: () => false,
          getCacheVersion: () => 'fixture-version', unlinkFile: async file => fs.unlinkSync(file),
          reserveCache: async () => { calls.reserve += 1; return {result: {cacheId: 7}}; },
          internalCacheTwirpClient: () => ({
            CreateCacheEntry: async () => { calls.reserve += 1; return {ok: true, signedUploadUrl: 'fixture'}; },
            FinalizeCacheEntryUpload: async () => { calls.finalize += 1; return {ok: true, entryId: '7'}; },
          }),
          saveCache$1: async (_id, file) => { fs.statSync(file); calls.upload += 1; },
          __awaiter: (receiver, args, _promise, generator) => {
            const iterator = generator.apply(receiver, args);
            function step(method, value) {
              let next;
              try { next = iterator[method](value); } catch (error) { return Promise.reject(error); }
              return next.done ? Promise.resolve(next.value)
                : Promise.resolve(next.value).then(value => step('next', value), error => step('throw', error));
            }
            return step('next');
          },
        });
        vm.runInContext(extract('getArchiveFileSizeInBytes') + '\n' + extract('saveCache' + version), context);
        const result = await context['saveCache' + version](['target'], 'fixture-key', {});
        assert.equal(result, allow ? 7 : -1);
        assert.deepEqual(calls, {admit: 1, reserve: Number(allow), upload: Number(allow),
          finalize: Number(allow && version === 'V2')});
        assert.equal(fs.existsSync(f.archive), false, 'the provider must still clean up after denial');
        installed.restore();
      }
    });
}
