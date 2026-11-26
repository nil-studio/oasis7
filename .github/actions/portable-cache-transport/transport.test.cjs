'use strict';
const {test} = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const os = require('node:os');
const {pathToFileURL} = require('node:url');
const {execFileSync} = require('node:child_process');
const {validateTrust, transport, environmentForInspector} = require('./index.cjs');
const sha = 'a'.repeat(40);
const inputs = {operation: 'restore', contract: 'worker-build', component: 'worker-build', key: 'fixed'};
const environment = {GITHUB_ACTIONS: 'true', GITHUB_REPOSITORY: 'nil-studio/oasis7', GITHUB_REF: 'refs/heads/main',
  GITHUB_SHA: sha, GITHUB_WORKFLOW_SHA: sha, GITHUB_WORKFLOW_REF: 'nil-studio/oasis7/.github/workflows/rust-cache.yml@refs/heads/main',
  GITHUB_JOB: 'worker-build-cache', GITHUB_EVENT_NAME: 'schedule', GITHUB_RUN_ID: '123', RUNNER_OS: 'Linux', RUNNER_ARCH: 'X64',
  ACTIONS_CACHE_SERVICE_V2: 'True', ACTIONS_CACHE_MODE: 'write', CI_CACHE_REPOSITORY_BUDGET_BYTES: '20000000000'};
const layout = {contract: 'worker-build', component: 'worker-build', cwd: '/home/runner/work/oasis7/oasis7', root: '/home/runner/.cache/oasis7/worker-build/0.7.5', key: 'fixed'};

test('trusted identity, mode, role and source failures reject before any SDK call', () => {
  validateTrust(environment, inputs, sha);
  for (const delta of [{GITHUB_REPOSITORY:'other/repo'}, {GITHUB_REF:'refs/pull/1/merge'}, {GITHUB_WORKFLOW_SHA:'b'.repeat(40)},
    {GITHUB_JOB:'other'}, {GITHUB_EVENT_NAME:'pull_request'}, {GITHUB_RUN_ID:'0'}, {RUNNER_ARCH:'ARM64'},
    {ACTIONS_CACHE_SERVICE_V2:''}, {ACTIONS_CACHE_MODE:'none'}, {ACTIONS_CACHE_MODE:'write-only'}]) {
    assert.throws(() => validateTrust({...environment, ...delta}, inputs, sha));
  }
  assert.throws(() => validateTrust(environment, inputs, 'b'.repeat(40)));
  assert.throws(() => validateTrust(environment, {...inputs, component:'../worker-build'}, sha));
  assert.throws(() => validateTrust({...environment, ACTIONS_CACHE_MODE:'read'}, {...inputs, operation:'save'}, sha));
});
test('original admission and fixed repository quota remain required for writes', async () => {
  const save = {...inputs, operation:'save', admitted:'true', uploadBound:'100'};
  validateTrust(environment, save, sha);
  for (const delta of [{admitted:''}, {uploadBound:'20000000001'}, {uploadBound:'0'}, {uploadBound:'1e3'}]) assert.throws(() => validateTrust(environment, {...save,...delta}, sha));
  assert.throws(() => validateTrust({...environment, CI_CACHE_REPOSITORY_BUDGET_BYTES:'30000000000'}, save, sha));
  let calls=0;
  const sdk = {saveCache:async () => {calls++; return -1;}};
  assert.deepEqual(await transport({...save, admitted:'false'}, environment, sdk, null, layout), {saved:false, reason:'original save admission denied'});
  assert.equal(calls,0);
  assert.deepEqual(await transport(save, environment, sdk, null, layout), {saved:false});
  assert.equal(calls,1);
});
test('matched prefix and malformed metadata reject; inspection precedes extraction', async () => {
  let inspected=0;
  const sdk = {restoreCache:async (_paths,key,_fallback,options) => {await options.beforeExtract('/archive','zstd-without-long',key+'extra'); throw Error('must not extract');}};
  await assert.rejects(transport(inputs,environment,sdk,()=>{inspected++;},layout), /nonexact/);
  assert.equal(inspected,0);
  await assert.rejects(transport({...inputs,key:'wrong'},environment,sdk,null,layout), /metadata/);
  const goodSdk = {restoreCache:async (_paths,key,_fallback,options) => {await options.beforeExtract('/archive','zstd-without-long',key);return key;}};
  assert.equal((await transport(inputs,environment,goodSdk,()=>{inspected++;},layout))['cache-hit'],true);
  assert.equal(inspected,1);
});
test('inspection child receives no Actions/GH/INPUT credentials', () => {
  const clean = environmentForInspector({...environment, PATH:'/bin', GH_TOKEN:'secret', INPUT_KEY:'secret', ACTIONS_RUNTIME_TOKEN:'secret', NODE_OPTIONS:'bad'});
  assert.deepEqual(Object.keys(clean).sort(), ['PATH','RUNNER_ARCH','RUNNER_OS']);
});

test('manual dispatch full mode and bounded regular event only', () => {
  const temporary=fs.mkdtempSync(path.join(os.tmpdir(),'portable-event-'));
  const event=path.join(temporary,'event.json');
  const manual={...environment,GITHUB_EVENT_NAME:'workflow_dispatch',GITHUB_EVENT_PATH:event};
  try {
    fs.writeFileSync(event,JSON.stringify({inputs:{run_mode:'full'}}));
    validateTrust(manual,inputs,sha);
    for(const value of [{inputs:{run_mode:'partial'}},{inputs:{}},{}]) {
      fs.writeFileSync(event,JSON.stringify(value));assert.throws(()=>validateTrust(manual,inputs,sha));
    }
    fs.writeFileSync(event,'x'.repeat(1024*1024+1));assert.throws(()=>validateTrust(manual,inputs,sha));
    fs.unlinkSync(event);fs.symlinkSync('/missing',event);assert.throws(()=>validateTrust(manual,inputs,sha));
  } finally {fs.rmSync(temporary,{recursive:true,force:true});}
});
test('inspector rejection propagates before restore and save uses exact path/cwd', async () => {
  let extracted=false;
  const restore={restoreCache:async(_paths,key,fallback,options)=>{
    assert.deepEqual(fallback,[]);assert.equal(options.archiveWorkingDirectory,layout.cwd);
    await options.beforeExtract('/archive','zstd-without-long',key);extracted=true;return key;
  }};
  await assert.rejects(transport(inputs,environment,restore,()=>{throw Error('unsafe archive');},layout),/unsafe archive/);
  assert.equal(extracted,false);
  const save={saveCache:async(paths,key,options)=>{
    assert.deepEqual(paths,[layout.root]);assert.equal(key,layout.key);assert.equal(options.archiveWorkingDirectory,layout.cwd);return 123;
  }};
  assert.deepEqual(await transport({...inputs,operation:'save',admitted:'true'},environment,save,null,layout),{saved:true});
});

test('a reported hit without beforeExtract cannot be accepted', async () => {
  await assert.rejects(transport(inputs,environment,{restoreCache:async()=>layout.key},()=>{},layout),/bypassed/);
  assert.deepEqual(await transport(inputs,environment,{restoreCache:async()=>undefined},()=>{},layout),{'cache-hit':false,matchedKey:''});
});
