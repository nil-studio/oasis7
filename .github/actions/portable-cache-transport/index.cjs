'use strict';
const fs = require('node:fs');
const path = require('node:path');
const {execFileSync} = require('node:child_process');
const CWD = '/home/runner/work/oasis7/oasis7';
const REPOSITORY = 'nil-studio/oasis7';
const WORKFLOW = '.github/workflows/rust-cache.yml';
const BUDGET = '20000000000';

function noSymlinks(value) {
  if (!path.isAbsolute(value) || path.normalize(value) !== value || /[\x00-\x1f\x7f]/.test(value)) throw Error('unsafe canonical path');
  for (let current = value;; current = path.dirname(current)) {
    try { if (fs.lstatSync(current).isSymbolicLink()) throw Error('canonical path contains symlink'); }
    catch (error) { if (error.code !== 'ENOENT') throw error; }
    if (current === path.dirname(current)) break;
  }
}
function environmentForInspector(environment) {
  return Object.fromEntries(['PATH', 'LANG', 'LC_ALL', 'RUST_TOOLCHAIN', 'RUNNER_OS', 'RUNNER_ARCH', 'PYTHONDONTWRITEBYTECODE']
    .filter(key => environment[key] !== undefined).map(key => [key, environment[key]]));
}
function validateTrust(environment, inputs, actualHead) {
  const job = inputs.contract === 'worker-build' && inputs.component === 'worker-build' ? 'worker-build-cache' :
    inputs.contract === 'browser-archive' && ['fonts-noto-cjk', 'fonts-freefont-ttf', 'chrome'].includes(inputs.component) ? 'browser-archive-cache' : undefined;
  if (!job || !['restore', 'save'].includes(inputs.operation) ||
      environment.GITHUB_ACTIONS !== 'true' || environment.GITHUB_REPOSITORY !== REPOSITORY ||
      environment.GITHUB_REF !== 'refs/heads/main' || !/^[a-f0-9]{40}$/.test(environment.GITHUB_SHA || '') ||
      actualHead !== environment.GITHUB_SHA || environment.GITHUB_WORKFLOW_SHA !== actualHead ||
      environment.GITHUB_WORKFLOW_REF !== `${REPOSITORY}/${WORKFLOW}@refs/heads/main` || environment.GITHUB_JOB !== job ||
      !['schedule', 'workflow_dispatch'].includes(environment.GITHUB_EVENT_NAME) ||
      !/^[1-9][0-9]*$/.test(environment.GITHUB_RUN_ID || '') ||
      environment.RUNNER_OS !== 'Linux' || environment.RUNNER_ARCH !== 'X64' ||
      !['True', 'true'].includes(environment.ACTIONS_CACHE_SERVICE_V2) || !['read', 'write'].includes(environment.ACTIONS_CACHE_MODE)) throw Error('portable transport requires the exact trusted main V2 tool job');
  if (inputs.operation === 'save' && environment.ACTIONS_CACHE_MODE !== 'write') throw Error('cache save requires write mode');
  if (environment.GITHUB_EVENT_NAME === 'workflow_dispatch') {
    const file = environment.GITHUB_EVENT_PATH;
    noSymlinks(file);
    const fd = fs.openSync(file, fs.constants.O_RDONLY | fs.constants.O_NOFOLLOW | fs.constants.O_NONBLOCK);
    try {
      const info = fs.fstatSync(fd);
      if (!info.isFile() || info.size > 1024 * 1024) throw Error('invalid bounded dispatch event');
      const bytes = Buffer.alloc(1024 * 1024 + 1);
      const count = fs.readSync(fd, bytes, 0, bytes.length, 0);
      if (count > 1024 * 1024 || JSON.parse(bytes.subarray(0, count)).inputs?.run_mode !== 'full') throw Error('manual transport requires run_mode=full');
    } finally { fs.closeSync(fd); }
  }
  if (inputs.operation === 'save') {
    if (!['true', 'false'].includes(inputs.admitted)) throw Error('explicit save admission required');
    if (inputs.admitted === 'true' && (environment.CI_CACHE_REPOSITORY_BUDGET_BYTES !== BUDGET ||
        !/^[1-9][0-9]{0,10}$/.test(inputs.uploadBound || '') || BigInt(inputs.uploadBound) > BigInt(BUDGET))) throw Error('bounded original save admission required');
  }
}
function readLayout(inputs, environment) {
  const output = execFileSync('/usr/bin/python3', [path.join(CWD, 'scripts/ci-portable-cache-archive.py'), '--supervise', '--contract', inputs.contract, '--component', inputs.component],
    {env: environmentForInspector(environment), cwd: CWD, timeout: 90000, maxBuffer: 16384, encoding: 'utf8'});
  return JSON.parse(output.split('\n').find(line => line.startsWith('{') && !line.includes('\"suite\"')));
}
async function transport(inputs, environment, sdk, inspect, layout) {
  if (inputs.key !== layout.key || layout.cwd !== CWD || layout.contract !== inputs.contract || layout.component !== inputs.component) throw Error('metadata/key contract mismatch');
  if (inputs.operation === 'save') {
    if (inputs.admitted !== 'true') return {saved: false, reason: 'original save admission denied'};
    // Exact original admission remains responsible for REST identity, byte
    // measurement, repository inventory and rotation headroom. This adapter
    // adds no alternate admission path or API token.
    const id = await sdk.saveCache([layout.root], layout.key, {archiveWorkingDirectory: CWD});
    return {saved: Number.isSafeInteger(id) && id >= 0};
  }
  let inspected = false;
  const matched = await sdk.restoreCache([layout.root], layout.key, [], {
    archiveWorkingDirectory: CWD,
    beforeExtract: async (archive, compression, matchedKey) => {
      if (matchedKey !== layout.key) throw Error('nonexact portable key rejected before extraction');
      await inspect(archive, compression);
      inspected = true;
    }
  });
  if (matched && !inspected) throw Error('restore bypassed archive validation');
  if (matched && matched !== layout.key) throw Error('unexpected restore result');
  return {'cache-hit': matched === layout.key, matchedKey: matched || ''};
}
async function main(core, sdk) {
  try {
    const environment = process.env; // Read only: never override runner GITHUB_*.
    const inputs = Object.fromEntries(['operation', 'contract', 'component', 'key', 'admitted', 'upload-bound'].map(name =>
      [name === 'upload-bound' ? 'uploadBound' : name, core.getInput(name)]));
    if (process.platform !== 'linux' || process.getuid() !== 1001 || process.getgid() !== 1001) throw Error('portable transport requires hosted Linux UID/GID1001');
    noSymlinks(CWD);
    noSymlinks(environment.GITHUB_WORKSPACE);
    const canonical = fs.statSync(CWD), workspace = fs.statSync(environment.GITHUB_WORKSPACE);
    if (canonical.dev !== workspace.dev || canonical.ino !== workspace.ino) throw Error('canonical checkout alias differs');
    const head = execFileSync('git', ['-c', `safe.directory=${CWD}`, '-C', CWD, 'rev-parse', 'HEAD'],
      {env: {PATH: environment.PATH}, timeout: 10000, maxBuffer: 1024, encoding: 'utf8'}).trim();
    validateTrust(environment, inputs, head);
    const actionRoot = path.join(CWD, '.github/actions/portable-cache-transport');
    noSymlinks(actionRoot);
    noSymlinks(__dirname);
    const originalAction = fs.statSync(actionRoot), mappedAction = fs.statSync(__dirname);
    if (originalAction.dev !== mappedAction.dev || originalAction.ino !== mappedAction.ino) throw Error('unexpected action source');
    const layout = readLayout(inputs, environment);
    const inspector = async (archive, compression) => execFileSync('/usr/bin/python3', [path.join(CWD, 'scripts/ci-portable-cache-archive.py'),
      '--supervise', '--contract', inputs.contract, '--component', inputs.component, '--archive', archive, '--compression', compression],
      {env: environmentForInspector(environment), cwd: CWD, timeout: 90000, maxBuffer: 16384});
    const result = await transport(inputs, environment, sdk, inspector, layout);
    for (const [name, value] of Object.entries(result)) core.setOutput(name, String(value));
  } catch (error) { core.setFailed(error.message); }
}
module.exports = {noSymlinks, environmentForInspector, validateTrust, transport, main};
