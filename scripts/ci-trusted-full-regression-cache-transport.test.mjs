import assert from 'node:assert/strict';
import test from 'node:test';
import {
  restoreFullCache,
  saveFullCache,
  verifyFullCacheAuthority,
} from './ci-trusted-full-regression-cache-transport.mjs';

const sha = 'a'.repeat(40);
const digest = 'b'.repeat(64);
const trunk = {
  actualRawPath: '/home/runner/work/_temp/ci-tools/trunk-x86_64-unknown-linux-gnu.tar.gz',
  actualKey: `trunk-archive-Linux-X64-0.21.14-${digest}`,
  trunkVersion: '0.21.14',
  installerHashFiles: digest,
};
const cwd = '/home/runner/work/oasis7/oasis7';

test('only the original trusted main full source is authorized', () => {
  const env = {
    GITHUB_ACTIONS: 'true',
    GITHUB_REPOSITORY: 'nil-studio/oasis7',
    GITHUB_REF: 'refs/heads/main',
    GITHUB_WORKFLOW_REF: 'nil-studio/oasis7/.github/workflows/rust.yml@refs/heads/main',
    GITHUB_JOB: 'full-regression',
    GITHUB_SHA: sha,
    GITHUB_WORKFLOW_SHA: sha,
    GITHUB_RUN_ID: '1234',
    GITHUB_RUN_ATTEMPT: '1',
    GITHUB_EVENT_NAME: 'schedule',
    RUNNER_OS: 'Linux',
    RUNNER_ARCH: 'X64',
    ACTIONS_CACHE_SERVICE_V2: 'True',
    ACTIONS_CACHE_MODE: 'write',
  };
  const identity = {head: sha, platform: 'linux', uid: 1001, gid: 1001};
  verifyFullCacheAuthority(env, identity, 'save');
  for (const [key, value] of [
    ['GITHUB_REF', 'refs/pull/1/merge'],
    ['GITHUB_REPOSITORY', 'fork/oasis7'],
    ['GITHUB_WORKFLOW_REF', 'nil-studio/oasis7/.github/workflows/rust.yml@refs/heads/fake'],
    ['GITHUB_JOB', 'candidate-full-regression'],
    ['GITHUB_WORKFLOW_SHA', digest.slice(0, 40)],
    ['RUNNER_ARCH', 'ARM64'],
    ['ACTIONS_CACHE_SERVICE_V2', 'false'],
    ['ACTIONS_CACHE_MODE', 'read'],
  ]) {
    assert.throws(() => verifyFullCacheAuthority({...env, [key]:value}, identity, 'save'));
  }
  assert.throws(() => verifyFullCacheAuthority(env, {...identity, uid:0}, 'save'));
  const dispatch = {...env, GITHUB_EVENT_NAME:'workflow_dispatch'};
  verifyFullCacheAuthority(dispatch, identity, 'save', {run_mode:'full', source_ref:''});
  assert.throws(() => verifyFullCacheAuthority(dispatch, identity, 'save', {run_mode:'full', source_ref:sha}));
});

test('SDK restores only the exact validated role archive before reporting hit', async () => {
  const inspect = async (archive, layout) => {
    assert.equal(archive, '/archive');
    assert.equal(layout.member, '../../_temp/ci-tools/trunk-x86_64-unknown-linux-gnu.tar.gz');
  };
  const restoreCache = async (paths, key, aliases, options) => {
    assert.deepEqual(paths, [trunk.actualRawPath]);
    assert.deepEqual(aliases, []);
    assert.equal(options.archiveWorkingDirectory, cwd);
    await options.beforeExtract('/archive','zstd-without-long',key);
    return key;
  };
  assert.equal(await restoreFullCache({restoreCache}, 'trunk', trunk, inspect), true);
  await assert.rejects(restoreFullCache({restoreCache}, 'trunk', trunk, async () => {throw Error('archive rejected');}), /archive rejected/);
  await assert.rejects(restoreFullCache({restoreCache: async () => trunk.actualKey}, 'trunk', trunk, inspect), /unverified/);
  await assert.rejects(restoreFullCache({
    restoreCache: async (_paths, key, _aliases, options) => options.beforeExtract('/archive','zstd-without-long',key+'bad'),
  }, 'trunk', trunk, inspect), /unexpected key/);
  assert.equal(await restoreFullCache({restoreCache: async () => undefined}, 'trunk', trunk, inspect), false);
});

test('save requires the original step phase, miss, success and measured repository admission', async () => {
  let saves = 0;
  const saveCache = async (paths,key,options) => {
    assert.deepEqual(paths, [trunk.actualRawPath]);
    assert.equal(key,trunk.actualKey);
    assert.equal(options.archiveWorkingDirectory,cwd);
    saves += 1;
    return 22;
  };
  const state = {phase:'post', success:true, cacheHit:false};
  const admission = {allowed:true, key:trunk.actualKey, repositoryBudgetBytes:20_000_000_000, uploadBoundBytes:8_000_000};
  assert.equal(await saveFullCache({saveCache}, 'trunk', trunk, state, admission), true);
  for (const change of [{phase:'before-full'}, {cacheHit:true}, {success:false}]) {
    assert.equal(await saveFullCache({saveCache}, 'trunk', trunk, {...state,...change}, admission), false);
  }
  for (const change of [{allowed:false}, {key:'wrong'}, {uploadBoundBytes:0}, {repositoryBudgetBytes:1}]) {
    assert.equal(await saveFullCache({saveCache}, 'trunk', trunk, state, {...admission,...change}), false);
  }
  assert.equal(saves,1);
});

test('bindgen and sentinel keep their original in-job save gates', async () => {
  const bindgen = {
    actualRawPath:'/home/runner/.cache/oasis7/wasm-bindgen-cli/0.2.127',
    actualKey:`wasm-bindgen-cli-v1-Linux-X64-1.96.0-0.2.127-${digest}`,
    rustToolchain:'1.96.0', bindgenLockVersion:'0.2.127', helpersHashFiles:digest,
  };
  const sentinel = {
    actualRawPath:'/home/runner/work/_temp/cache-permission-sentinel',
    actualKey:'oasis7-cache-permission-sentinel-trusted-v2',
  };
  const saves = [];
  const sdk = {saveCache: async (paths,key) => { saves.push({paths,key}); return 5; }};
  const state = {phase:'before-full', success:true, cacheHit:false, prepareOutcome:'success'};
  const admission = metadata => ({
    allowed:true, key:metadata.actualKey, repositoryBudgetBytes:20_000_000_000, uploadBoundBytes:1,
  });
  for (const change of [{phase:'post'}, {cacheHit:true}, {prepareOutcome:'failure'}]) {
    assert.equal(await saveFullCache(sdk, 'wasm-bindgen', bindgen, {...state,...change}, admission(bindgen)), false);
  }
  assert.equal(await saveFullCache(sdk, 'wasm-bindgen', bindgen, state, admission(bindgen)), true);
  assert.equal(await saveFullCache(sdk, 'sentinel', sentinel, {phase:'before-full',success:true}, admission(sentinel)), true);
  await assert.rejects(restoreFullCache({}, 'sentinel', sentinel), /write-only/);
  assert.deepEqual(saves, [bindgen,sentinel].map(m => ({paths:[m.actualRawPath],key:m.actualKey})));
});
