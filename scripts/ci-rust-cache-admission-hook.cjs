// Trace refs: O7-REQ-d9292810-5229-417f-9fc8-262ee04759c7.
// The pinned provider measures its completed archive synchronously before either
// v1 reserveCache or v2 CreateCacheEntry. Windows uses an absolute tar.exe path,
// so a PATH-only tar adapter cannot enforce this boundary on all Release runners.
'use strict';
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const childProcess = require('node:child_process');
const {syncBuiltinESMExports} = require('node:module');

// Upgrade this revision and the reviewed bundle hashes together.
const ACTION_REVISION = 'd1527c8f67ef9af357290475d4af4b0fab8f12e0';
const PINNED_HASHES = Object.freeze({
  'restore.js': 'fe663d7d2fa188abdba01be739acffc3fde7f66dfd579f9917a9e3e78c1e4e81',
  'save.js': '7a067804c0f1f976cbb84d32cee9ceb5fffa6c5d4400c54ef378679f6fccff17',
  'cleanup-e3M0pm4f.js': 'f7172929896dd424f825d9fd196013a5f3ce1b98b9aa3f5bd08697b03f8a5f03',
  'cache-Dl81DHPs.js': '86ab541f44139ba8839857ff73407c447b2ab4da0e813865013c775fb8a3db44',
});
const ARCHIVES = new Set(['cache.tzst', 'cache.tgz']);

function fail(reason) {
  throw new Error('Rust dependency cache upload guard: ' + reason);
}

function inspectAction(entry, hashes) {
  if (typeof entry !== 'string' || !path.isAbsolute(entry) ||
      !['restore.js', 'save.js'].includes(path.basename(entry))) {
    fail('unknown action entry');
  }
  const directory = path.dirname(entry);
  for (const [filename, expected] of Object.entries(hashes)) {
    const file = path.join(directory, filename);
    const stat = fs.lstatSync(file);
    if (!stat.isFile() || stat.size <= 0 || stat.size > 8 * 1024 * 1024 ||
        crypto.createHash('sha256').update(fs.readFileSync(file)).digest('hex') !== expected) {
      fail('pinned action bundle changed');
    }
  }
  return path.basename(entry) === 'restore.js' ? 'restore' : 'save';
}

function fileIdentity(file) {
  const stat = fs.lstatSync(file, {bigint: true});
  if (!stat.isFile() || stat.nlink !== 1n || stat.size <= 0n) {
    fail('archive and manifest must be complete single-link regular files');
  }
  return ['dev', 'ino', 'size', 'mode', 'nlink', 'mtimeNs', 'ctimeNs']
    .map(name => String(stat[name])).join(':');
}

function samePath(first, second) {
  return process.platform === 'win32'
    ? first.toLowerCase() === second.toLowerCase() : first === second;
}

function archiveIdentity(file, environment) {
  const temporary = environment.RUNNER_TEMP;
  const workspace = environment.GITHUB_WORKSPACE;
  if (typeof temporary !== 'string' || !path.isAbsolute(temporary) ||
      typeof workspace !== 'string' || !path.isAbsolute(workspace) ||
      !path.isAbsolute(file) || !ARCHIVES.has(path.basename(file))) {
    fail('archive paths are outside the pinned provider contract');
  }
  const directory = path.dirname(file);
  if (!fs.lstatSync(directory).isDirectory() ||
      !samePath(path.dirname(fs.realpathSync(directory)), fs.realpathSync(temporary)) ||
      samePath(fs.realpathSync(directory), fs.realpathSync(workspace))) {
    fail('archive is outside its private runner temp directory');
  }
  const manifest = path.join(directory, 'manifest.txt');
  const identity = fileIdentity(file) + '/' + fileIdentity(manifest);
  if (fs.lstatSync(manifest).size > 1024 * 1024) {
    fail('archive manifest exceeds the pinned provider contract');
  }
  return identity;
}

function checkWithPython(file, environment) {
  const python = environment.OASIS7_DEPENDENCY_CACHE_PYTHON;
  if (typeof python !== 'string' || !path.isAbsolute(python)) {
    fail('absolute admission interpreter is missing');
  }
  const childEnvironment = {...environment};
  // Only the REST read token is needed by this child. Do not pass the cache
  // transport credentials or preload to the verifier and its local git read.
  for (const name of ['NODE_OPTIONS', 'ACTIONS_RUNTIME_TOKEN', 'ACTIONS_CACHE_URL', 'ACTIONS_RESULTS_URL']) {
    delete childEnvironment[name];
  }
  const result = childProcess.spawnSync(python,
    [path.join(__dirname, 'ci-rust-cache-admission.py'), 'check', '--archive', file],
    {env: childEnvironment, shell: false, encoding: 'utf8', timeout: 135000, maxBuffer: 1024 * 1024});
  let report;
  try {
    report = JSON.parse((result.stdout || '').split(/\r?\n/)[0]);
  } catch {
    fail('admission did not produce a valid report');
  }
  if (report.mode !== 'dependency-cache-upload-admission' ||
      !Number.isSafeInteger(report.archive_bytes) || report.archive_bytes <= 0) {
    fail('admission report has an invalid archive identity');
  }
  // The helper emits bounded diagnostics; stderr is deliberately not copied.
  process.stdout.write(result.stdout);
  if (result.error || result.status !== 0 || report.allowed !== true ||
      BigInt(report.archive_bytes) !== fs.lstatSync(file, {bigint: true}).size) {
    fail('completed archive was not admitted');
  }
}

function installGuard(options = {}) {
  // Explicit arguments permit isolated Node fixtures. The production preload
  // supplies no overrides and exposes no environment/CLI bypass.
  const environment = options.environment || process.env;
  const mode = inspectAction(options.entry || process.argv[1], options.hashes || PINNED_HASHES);
  if (environment['INPUT_CACHE-PROVIDER'] !== 'github') {
    fail('only the pinned GitHub cache provider is supported');
  }
  if (mode === 'restore') {
    return {mode, restore() {}};
  }
  const check = options.checkArchive || checkWithPython;
  const original = fs.statSync;
  const admitted = new Map();
  fs.statSync = function guardedStat(file, ...args) {
    if (typeof file !== 'string' || !ARCHIVES.has(path.basename(file))) {
      return original.call(fs, file, ...args);
    }
    const before = archiveIdentity(file, environment);
    const previous = admitted.get(file);
    if (previous !== undefined && previous !== before) {
      fail('archive changed after admission');
    }
    if (previous === undefined) {
      check(file, environment);
      if (before !== archiveIdentity(file, environment)) {
        fail('archive changed during admission');
      }
    }
    const result = original.call(fs, file, ...args);
    if (before !== archiveIdentity(file, environment)) {
      fail('archive changed while its upload size was measured');
    }
    admitted.set(file, before);
    return result;
  };
  // The upstream bundle imports fs through ESM namespace bindings.
  syncBuiltinESMExports();
  return {mode, restore() { fs.statSync = original; syncBuiltinESMExports(); }};
}

module.exports = {ACTION_REVISION, PINNED_HASHES, installGuard, inspectAction, archiveIdentity, checkWithPython};
