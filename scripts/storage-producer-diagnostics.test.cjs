const {test} = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs/promises');
const os = require('node:os');
const path = require('node:path');
const {execFileSync} = require('node:child_process');
const {buildEnvironment, snapshot, uploadSnapshot, downloadUploader, main, execute, MAX_BYTES} = require('./storage-producer-diagnostics.cjs');
const identity = {GITHUB_SHA: 'a'.repeat(40), GITHUB_RUN_ID: '1', GITHUB_RUN_ATTEMPT: '2', GITHUB_REF: 'refs/heads/main', GITHUB_EVENT_NAME: 'workflow_dispatch', SOURCE_SHA: 'a'.repeat(40), GITHUB_WORKFLOW_SHA: 'a'.repeat(40), GITHUB_REPOSITORY: 'nil-studio/oasis7'};
async function temporary(fn) {
  const dir = await fs.mkdtemp(path.join(os.tmpdir(), 'storage-diag-test-'));
  try { await fn(dir); } finally { await fs.rm(dir, {recursive: true, force: true}); }
}
test('build retains profile but receives no action upload or GitHub credentials', () => {
  const env = buildEnvironment({CARGO_PROFILE_TEST_OPT_LEVEL: '2', ACTIONS_RUNTIME_TOKEN: 'secret',
    ACTIONS_RESULTS_URL: 'private', GITHUB_TOKEN: 'secret', GH_TOKEN: 'secret'});
  assert.deepEqual(env, {CARGO_PROFILE_TEST_OPT_LEVEL: '2'});
});
test('snapshot is explicit non-acceptance identity and bounded process details', () => temporary(async dir => {
  await fs.mkdir(path.join(dir, 'formal-required'));
  await fs.writeFile(path.join(dir, 'formal-required/resources.json'), JSON.stringify({process_tree: {
    unlisted_secret: 'secret', latest: {processes: Array.from({length: 100}, (_, pid) => ({pid, unlisted_secret: 'secret'}))}}}));
  const data = JSON.parse(await snapshot(dir, {...identity, ACTIONS_RUNTIME_TOKEN: 'secret'}, 3));
  assert.equal(data.diagnostic_only, true); assert.equal(data.sequence, 3);
  assert.ok(data.orchestrator.rss_bytes > 0);
  assert.match(data.orchestrator.scope, /Excludes Cargo and upload child/);
  assert.equal(data.resources.process_tree.latest.processes.length, 64);
  assert.equal(data.resources.process_tree.latest.omitted_processes, 36);
  assert.equal(data.build, null); assert.ok(!JSON.stringify(data).includes('secret'));
}));
test('snapshot rejects symlink and oversized source without reading contents', () => temporary(async dir => {
  await fs.mkdir(path.join(dir, 'formal-required'));
  await fs.writeFile(path.join(dir, 'outside'), 'private');
  await fs.symlink(path.join(dir, 'outside'), path.join(dir, 'formal-required/build.json'));
  await fs.writeFile(path.join(dir, 'formal-required/resources.json'), 'x'.repeat(MAX_BYTES + 1));
  const data = JSON.parse(await snapshot(dir, identity, 1));
  assert.equal(data.build, null); assert.equal(data.resources, null);
}));
test('official output acknowledgement supports multiline core output protocol', () => temporary(async dir => {
  const file = path.join(dir, 'snapshot.json'); await fs.writeFile(file, '{}');
  const result = await uploadSnapshot('official.cjs', file, 'unique', {ACTIONS_RUNTIME_TOKEN: 'secret', NODE_OPTIONS: '--require=existing-cache-admission.cjs'},
    async (args, options, timeout) => {
      assert.equal(timeout, 30000); assert.equal(options.env.INPUT_OVERWRITE, 'false');
      assert.equal(options.env.ACTIONS_RUNTIME_TOKEN, 'secret');
      assert.equal(options.env.NODE_OPTIONS, '--require=existing-cache-admission.cjs');
      await fs.writeFile(options.env.GITHUB_OUTPUT, 'artifact-id<<marker\n123\nmarker\nartifact-digest<<digest\n' + 'b'.repeat(64) + '\ndigest\n');
      return 0;
    });
  assert.equal(result.artifact_id, '123'); assert.equal(result.artifact_digest, 'b'.repeat(64));
}));
test('upload failure and missing official acknowledgement remain failures', () => temporary(async dir => {
  const file = path.join(dir, 'snapshot.json'); await fs.writeFile(file, '{}');
  await assert.rejects(uploadSnapshot('official', file, 'one', {}, async () => 1));
  await assert.rejects(uploadSnapshot('official', file, 'two', {}, async () => 0));
}));
test('download rejects modified official bytes', () => temporary(async dir => {
  await assert.rejects(downloadUploader(path.join(dir, 'uploader'), async () => ({ok: true,
    headers: new Map(), body: [Buffer.from('modified')]})), /integrity/);
}));
test('missing uploader cannot turn a failed build green or retry it', () => temporary(async dir => {
  let builds = 0;
  execFileSync('git', ['init', '-q', dir]);
  execFileSync('git', ['-C', dir, '-c', 'user.name=Fixture', '-c', 'user.email=fixture@example.invalid', 'commit', '-q', '--allow-empty', '-m', 'fixture']);
  const sha = execFileSync('git', ['-C', dir, 'rev-parse', 'HEAD'], {encoding: 'utf8'}).trim();
  const code = await main({env: {...identity, GITHUB_SHA: sha, SOURCE_SHA: sha, GITHUB_WORKFLOW_SHA: sha, RUNNER_TEMP: dir, GITHUB_WORKSPACE: dir,
    STORAGE_ARTIFACT_CHECKOUT: await fs.realpath(dir),
    ACTIONS_RUNTIME_TOKEN: 'secret'}, fetchImpl: async () => { throw Error('offline'); },
    runBuild: async options => { builds++; assert.equal(options.env.ACTIONS_RUNTIME_TOKEN, undefined); return 17; }});
  assert.equal(code, 17); assert.equal(builds, 1);
}));

test('original build failure stops pack, successful build returns exact pack failure', async () => {
  const {runBuild} = require('../.github/actions/storage-producer-diagnostics/index.cjs');
  for (const [buildCode, packCode, expectedCalls, expected] of [[17, 0, 1, 17], [0, 23, 2, 23], [0, 0, 2, 0]]) {
    const calls = [];
    const result = await runBuild({root: '/source', diagnostics: '/observation', env: {RUNNER_TEMP: '/temporary'}},
      async args => { calls.push(args); return args[1] === 'build' ? buildCode : packCode; });
    assert.equal(result, expected); assert.equal(calls.length, expectedCalls);
    assert.deepEqual(calls[0], ['/source/scripts/ci-storage-acceptance.py', 'build', '--component', 'storage-required',
      '--bundle', '/temporary/component', '--build-diagnostics', '/observation']);
  }
});

test('candidate source cannot enter live action even with supplied runtime credentials', () => temporary(async dir => {
  let builds = 0, downloads = 0;
  await assert.rejects(main({env: {...identity, GITHUB_REF: 'refs/heads/candidate',
    GITHUB_WORKSPACE: dir, RUNNER_TEMP: dir, ACTIONS_RUNTIME_TOKEN: 'secret'},
    runBuild: async () => { builds++; }, fetchImpl: async () => { downloads++; }}), /trusted main/);
  assert.equal(builds, 0); assert.equal(downloads, 0);
}));

test('upload subprocess deadline terminates only its own child', async () => {
  const started = Date.now();
  const code = await execute(['-e', 'setInterval(() => {}, 1000)'],
    {stdio: ['ignore', 'ignore', 'ignore']}, 50);
  assert.equal(code, 1); assert.ok(Date.now() - started < 2000);
});

test('checkout path and actual Git source are checked before uploader or Cargo', () => temporary(async dir => {
  let downloads = 0, builds = 0;
  const options = {env: {...identity, RUNNER_TEMP: dir, GITHUB_WORKSPACE: dir, STORAGE_ARTIFACT_CHECKOUT: '/wrong'},
    runBuild: async () => { builds++; }, fetchImpl: async () => { downloads++; }};
  await assert.rejects(main(options), /checkout path/);
  execFileSync('git', ['init', '-q', dir]);
  execFileSync('git', ['-C', dir, '-c', 'user.name=Fixture', '-c', 'user.email=fixture@example.invalid', 'commit', '-q', '--allow-empty', '-m', 'fixture']);
  options.env.STORAGE_ARTIFACT_CHECKOUT = await fs.realpath(dir);
  await assert.rejects(main(options), /checkout source/);
  assert.equal(builds, 0); assert.equal(downloads, 0);
}));
test('FIFO diagnostic report is rejected before a blocking read', {skip: process.platform === 'win32'}, () => temporary(async dir => {
  await fs.mkdir(path.join(dir, 'formal-required'));
  execFileSync('mkfifo', [path.join(dir, 'formal-required/resources.json')]);
  const started = Date.now();
  const result = JSON.parse(await snapshot(dir, identity, 1));
  assert.equal(result.resources, null); assert.ok(Date.now() - started < 1000);
}));

test('fake-build transport receipts cannot be mistaken for normal producer observations', () => temporary(async dir => {
  const probe = JSON.parse(await snapshot(dir, {...identity, GITHUB_JOB: 'diagnostics-probe'}, 1));
  const normal = JSON.parse(await snapshot(dir, {...identity, GITHUB_JOB: 'build-components'}, 1));
  assert.equal(probe.diagnostic_only, true); assert.equal(probe.probe_only, true);
  assert.equal(normal.diagnostic_only, true); assert.equal(normal.probe_only, false);
}));

test('fork main and a different workflow source cannot obtain live diagnostics', () => temporary(async dir => {
  for (const change of [{GITHUB_REPOSITORY: 'fork/oasis7'}, {GITHUB_WORKFLOW_SHA: 'b'.repeat(40)}]) {
    let calls = 0;
    await assert.rejects(main({env: {...identity, GITHUB_WORKSPACE: dir, RUNNER_TEMP: dir, ...change},
      runBuild: async () => { calls++; }, fetchImpl: async () => { calls++; }}), /trusted main/);
    assert.equal(calls, 0);
  }
}));
