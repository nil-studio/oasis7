import {execFileSync} from 'node:child_process';
import {lstat, readFile} from 'node:fs/promises';
import {fileURLToPath} from 'node:url';
import {core, sdk} from '../../../vendor/actions-cache-explicit-context/index.mjs';
import {fullRegressionCacheLayout} from '../../../scripts/ci-trusted-full-regression-cache-roles.mjs';
import {
  restoreFullCache,
  saveFullCache,
  shouldSaveFullCache,
  verifiedNpmLockDigest,
  verifyFullCacheAuthority,
  verifyFullCacheGeometry,
} from '../../../scripts/ci-trusted-full-regression-cache-transport.mjs';

const workspace = '/home/runner/work/oasis7/oasis7';
const environment = process.env;
const inspectorEnvironment = Object.fromEntries(
  ['PATH', 'LANG', 'LC_ALL', 'HOME', 'TMPDIR'].filter(key => environment[key]).map(key => [key, environment[key]]),
);
const metadata = {
  actualKey: core.getInput('key'),
  actualRawPath: core.getInput('raw-path'),
  setupNodeLockDigest: core.getInput('lock-digest'),
  trunkVersion: core.getInput('trunk-version'),
  installerHashFiles: core.getInput('installer-digest'),
  rustToolchain: environment.RUST_TOOLCHAIN,
  bindgenLockVersion: core.getInput('bindgen-version'),
  helpersHashFiles: core.getInput('bindgen-helpers-digest'),
};

async function dispatchInputs() {
  if (environment.GITHUB_EVENT_NAME !== 'workflow_dispatch') return undefined;
  const entry = await lstat(environment.GITHUB_EVENT_PATH);
  if (!entry.isFile() || entry.size > 1024 * 1024) throw new Error('invalid workflow event file');
  return JSON.parse(await readFile(environment.GITHUB_EVENT_PATH, 'utf8')).inputs;
}

function helper(operation, role, extra, env, timeout) {
  const command = [`${workspace}/scripts/ci-trusted-full-regression-cache.py`, operation, role, ...extra];
  if (role === 'wasm-bindgen') command.push('--bindgen-version', metadata.bindgenLockVersion);
  return JSON.parse(execFileSync('/usr/bin/python3', command, {
    cwd:workspace, env, timeout, encoding:'utf8', maxBuffer:64 * 1024,
  }));
}

async function save(role, state) {
  if (!shouldSaveFullCache(role, state)) return false;
  const permitted = [
    'GITHUB_ACTIONS', 'GITHUB_REPOSITORY', 'GITHUB_WORKFLOW_SHA', 'GITHUB_WORKFLOW_REF',
    'GITHUB_REF', 'GITHUB_SHA', 'GITHUB_EVENT_NAME', 'GITHUB_RUN_ID', 'GITHUB_EVENT_PATH',
    'GITHUB_WORKSPACE', 'GITHUB_JOB', 'RUNNER_OS', 'RUNNER_ARCH',
    'CI_CACHE_REPOSITORY_BUDGET_BYTES',
  ];
  const admissionEnvironment = {...inspectorEnvironment, GH_TOKEN:environment.GH_TOKEN};
  for (const key of permitted) {
    if (environment[key] !== undefined) admissionEnvironment[key] = environment[key];
  }
  const report = helper('admit', role, ['--key', metadata.actualKey], admissionEnvironment, 130_000);
  const effective = report.effective_repository_budget_bytes;
  const admitted = report.save === true && Number.isSafeInteger(effective) &&
    effective > 0 && effective <= 20_000_000_000;
  return saveFullCache(sdk, role, metadata, state, {
    allowed: admitted,
    key: metadata.actualKey,
    repositoryBudgetBytes: Number(environment.CI_CACHE_REPOSITORY_BUDGET_BYTES),
    uploadBoundBytes: report.upload_bound_bytes,
  });
}

async function main() {
  const post = core.getState('trusted-full-cache-post') === 'true';
  const role = core.getInput('role');
  const operation = post ? 'save' : core.getInput('operation');
  const layout = fullRegressionCacheLayout(role, metadata);
  const actual = execFileSync('git', ['-c', `safe.directory=${workspace}`, '-C', workspace, 'rev-parse', 'HEAD'], {
    env:inspectorEnvironment, encoding:'utf8', timeout:10_000, maxBuffer:1024,
  }).trim();
  verifyFullCacheAuthority(environment, {
    head:actual, platform:process.platform, uid:process.getuid(), gid:process.getgid(),
  }, operation, await dispatchInputs());
  await verifyFullCacheGeometry(environment, layout);
  const expectedAction = await lstat(`${workspace}/.github/actions/full-regression-cache/run.mjs`);
  const runningAction = await lstat(fileURLToPath(import.meta.url));
  if (!expectedAction.isFile() || !runningAction.isFile() ||
      expectedAction.ino !== runningAction.ino || expectedAction.dev !== runningAction.dev) {
    throw new Error('cache action source differs from verified checkout');
  }
  if (role === 'npm') {
    // The verified workflow supplies the original upstream lock digest. Check
    // that its one fixed path is regular before accepting runner metadata.
    await verifiedNpmLockDigest(workspace, () => metadata.setupNodeLockDigest);
  }

  if (operation === 'restore') {
    const hit = await restoreFullCache(sdk, role, metadata, archive => {
      helper('inspect', role, ['--archive', archive], inspectorEnvironment, 70_000);
    });
    if (!hit && ['npm', 'trunk'].includes(role) && environment.ACTIONS_CACHE_MODE === 'write') {
      core.saveState('trusted-full-cache-post', 'true');
    }
    core.setOutput('cache-hit', String(hit));
  } else if (post || role === 'wasm-bindgen' || role === 'sentinel') {
    const cacheHit = post ? false : core.getInput('cache-hit') !== 'false';
    const saved = await save(role, {
      phase:post ? 'post' : 'before-full', success:true, cacheHit,
      prepareOutcome:core.getInput('prepare-outcome'),
    });
    core.setOutput('saved', String(saved));
  } else {
    throw new Error('npm and Trunk saves are permitted only in the original success post phase');
  }
}

// The runner invokes post for every action instance, including explicit save.
if (core.getState('trusted-full-cache-invoked') !== 'true' || core.getState('trusted-full-cache-post') === 'true') {
  core.saveState('trusted-full-cache-invoked', 'true');
  try {
    await main();
  } catch (error) {
    core.setFailed(error.message);
  }
}
