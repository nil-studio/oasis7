import {lstat} from 'node:fs/promises';
import path from 'node:path';
import {fullRegressionCacheLayout} from './ci-trusted-full-regression-cache-roles.mjs';

const repository = 'nil-studio/oasis7';
const mainRef = 'refs/heads/main';
const commit = /^[0-9a-f]{40}$/;
const positiveId = /^[1-9][0-9]*$/;
const budget = 20_000_000_000;

const npmLock = 'crates/oasis7_viewer/package-lock.json';

async function physicalPath(value, allowMissingLeaf = false) {
  if (typeof value !== 'string' || !path.isAbsolute(value) ||
      path.normalize(value) !== value || /[\u0000-\u001f\u007f]/.test(value)) {
    throw new Error('invalid cache filesystem path');
  }
  for (let directory = path.dirname(value); ; directory = path.dirname(directory)) {
    if (!(await lstat(directory)).isDirectory()) {
      throw new Error('cache path ancestor is not a physical directory');
    }
    if (directory === path.dirname(directory)) break;
  }
  try {
    const info = await lstat(value);
    if (info.isSymbolicLink()) throw new Error('cache path uses a symlink');
    return info;
  } catch (error) {
    if (allowMissingLeaf && error.code === 'ENOENT') return null;
    throw error;
  }
}

async function sameDirectory(canonical, mapped) {
  const [first, second] = await Promise.all([physicalPath(canonical), physicalPath(mapped)]);
  return first.isDirectory() && second.isDirectory() &&
    first.dev === second.dev && first.ino === second.ino;
}

export async function verifyFullCacheGeometry(environment, layout) {
  if (!(await sameDirectory(layout.archiveWorkingDirectory, environment.GITHUB_WORKSPACE)) ||
      !(await sameDirectory('/home/runner/work/_temp', environment.RUNNER_TEMP))) {
    throw new Error('cache workspace or temporary directory alias differs');
  }
  const root = await physicalPath(layout.rawPaths[0], true);
  if (root && (root.uid !== 1001 || root.gid !== 1001 ||
      !(layout.role === 'trunk' ? root.isFile() : root.isDirectory()))) {
    throw new Error('cache root has an incompatible type or owner');
  }
}

export async function verifiedNpmLockDigest(workspace, hashFiles) {
  if (!(await physicalPath(path.join(workspace, npmLock))).isFile()) {
    throw new Error('setup-node lock is not a regular file');
  }
  const digest = await hashFiles(path.join(workspace, npmLock), workspace);
  if (!/^[a-f0-9]{64}$/.test(digest || '')) {
    throw new Error('upstream setup-node lock digest is unavailable');
  }
  return digest;
}

export function verifyFullCacheAuthority(environment, identity, operation, dispatchInputs) {
  const sha = environment.GITHUB_SHA || '';
  if (!['restore', 'save'].includes(operation) ||
      environment.GITHUB_ACTIONS !== 'true' ||
      environment.GITHUB_REPOSITORY !== repository ||
      environment.GITHUB_REF !== mainRef ||
      environment.GITHUB_WORKFLOW_REF !== `${repository}/.github/workflows/rust.yml@${mainRef}` ||
      environment.GITHUB_JOB !== 'full-regression' ||
      !commit.test(sha) || sha !== environment.GITHUB_WORKFLOW_SHA || sha !== identity.head ||
      !positiveId.test(environment.GITHUB_RUN_ID || '') ||
      !positiveId.test(environment.GITHUB_RUN_ATTEMPT || '') ||
      environment.RUNNER_OS !== 'Linux' || environment.RUNNER_ARCH !== 'X64' ||
      identity.platform !== 'linux' || identity.uid !== 1001 || identity.gid !== 1001 ||
      !['true', 'True'].includes(environment.ACTIONS_CACHE_SERVICE_V2) ||
      !['read', 'write'].includes(environment.ACTIONS_CACHE_MODE) ||
      (operation === 'save' && environment.ACTIONS_CACHE_MODE !== 'write')) {
    throw new Error('full cache transport requires the exact trusted Linux main job');
  }
  if (environment.GITHUB_EVENT_NAME !== 'schedule' &&
      !(environment.GITHUB_EVENT_NAME === 'workflow_dispatch' &&
        dispatchInputs?.run_mode === 'full' && [undefined, ''].includes(dispatchInputs.source_ref))) {
    throw new Error('full cache transport requires the unpinned main full run');
  }
}

export async function restoreFullCache(sdk, role, metadata, inspect) {
  const layout = fullRegressionCacheLayout(role, metadata);
  if (!layout.restore) throw new Error('the trusted cache sentinel is write-only');
  let validated = false;
  const match = await sdk.restoreCache(layout.rawPaths, layout.key, [], {
    archiveWorkingDirectory: layout.archiveWorkingDirectory,
    beforeExtract: async (archive, compression, key) => {
      if (key !== layout.key || compression !== layout.compression || validated) {
        throw new Error('unexpected key, compression or duplicate archive inspection');
      }
      await inspect(archive, layout);
      validated = true;
    },
  });
  if (match !== undefined && (match !== layout.key || !validated)) {
    throw new Error('unexpected or unverified restored cache');
  }
  return match === layout.key;
}

export async function saveFullCache(sdk, role, metadata, state, admission) {
  const layout = fullRegressionCacheLayout(role, metadata);
  const phase = ['npm', 'trunk'].includes(role) ? 'post' : 'before-full';
  if (state.phase !== phase || state.success !== true ||
      (role !== 'sentinel' && state.cacheHit !== false) ||
      (role === 'wasm-bindgen' && state.prepareOutcome !== 'success')) {
    return false;
  }
  if (admission.allowed !== true || admission.key !== layout.key ||
      admission.repositoryBudgetBytes !== budget ||
      !Number.isSafeInteger(admission.uploadBoundBytes) || admission.uploadBoundBytes <= 0 ||
      admission.uploadBoundBytes > budget) {
    return false;
  }
  const id = await sdk.saveCache(layout.rawPaths, layout.key, {
    archiveWorkingDirectory: layout.archiveWorkingDirectory,
  });
  return Number.isSafeInteger(id) && id >= 0;
}
