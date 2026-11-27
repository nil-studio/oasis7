import path from 'node:path';
import {utils} from '../vendor/actions-cache-explicit-context/index.mjs';

const workspace = '/home/runner/work/oasis7/oasis7';
const temp = '/home/runner/work/_temp';
const home = '/home/runner';
const digest = /^[0-9a-f]{64}$/;
const version = /^(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)$/;
const compression = 'zstd-without-long';

function required(pattern, value, description) {
  if (typeof value !== 'string' || !pattern.test(value)) {
    throw new Error(`invalid ${description}`);
  }
  return value;
}

export function fullRegressionCacheLayout(role, metadata) {
  let rawPath, key;
  switch (role) {
    case 'npm': {
      const lockDigest = required(digest, metadata.setupNodeLockDigest, 'setup-node lock digest');
      rawPath = `${home}/.npm`;
      key = `node-cache-Linux-x64-npm-${lockDigest}`;
      break;
    }
    case 'trunk': {
      const trunk = required(version, metadata.trunkVersion, 'Trunk version');
      const installer = required(digest, metadata.installerHashFiles, 'Trunk installer hashFiles digest');
      rawPath = `${temp}/ci-tools/trunk-x86_64-unknown-linux-gnu.tar.gz`;
      key = `trunk-archive-Linux-X64-${trunk}-${installer}`;
      break;
    }
    case 'wasm-bindgen': {
      const rust = required(version, metadata.rustToolchain, 'Rust toolchain');
      const bindgen = required(version, metadata.bindgenLockVersion, 'wasm-bindgen lock version');
      const helpers = required(digest, metadata.helpersHashFiles, 'wasm-bindgen helper hashFiles digest');
      rawPath = `${home}/.cache/oasis7/wasm-bindgen-cli/${bindgen}`;
      key = `wasm-bindgen-cli-v1-Linux-X64-${rust}-${bindgen}-${helpers}`;
      break;
    }
    case 'sentinel':
      rawPath = `${temp}/cache-permission-sentinel`;
      key = 'oasis7-cache-permission-sentinel-trusted-v2';
      break;
    default:
      throw new Error('unsupported trusted full-regression cache role');
  }
  if (metadata.actualRawPath !== rawPath || metadata.actualKey !== key) {
    throw new Error('original cache root or key does not match the trusted role');
  }
  return {
    role,
    archiveWorkingDirectory: workspace,
    rawPaths: [rawPath],
    member: path.posix.relative(workspace, rawPath),
    key,
    compression,
    version: utils.getCacheVersion([rawPath], compression, false),
    restore: role !== 'sentinel',
  };
}
