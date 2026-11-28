import assert from 'node:assert/strict';
import {execFileSync} from 'node:child_process';
import fs from 'node:fs';
import {core, sdk} from '../../../vendor/actions-cache-explicit-context/index.mjs';
import {fullRegressionCacheLayout} from '../../../scripts/ci-trusted-full-regression-cache-roles.mjs';
import {
  restoreFullCache, verifyFullCacheAuthority, verifyFullCacheGeometry,
} from '../../../scripts/ci-trusted-full-regression-cache-transport.mjs';

const environment = process.env;
const workspace = '/home/runner/work/oasis7/oasis7';
const actualSource = execFileSync('git', ['-c', `safe.directory=${workspace}`, '-C', workspace, 'rev-parse', 'HEAD'],
  {encoding:'utf8',timeout:10000,maxBuffer:1024}).trim();
assert.equal(actualSource,environment.PROBE_SOURCE);
assert.equal(environment.ACTIONS_CACHE_MODE, 'read');
assert.throws(() => verifyFullCacheAuthority(environment, {
  head:actualSource,platform:process.platform,uid:process.getuid(),gid:process.getgid(),
}, 'restore'), /exact trusted Linux main job/);

const npmDigest = core.getInput('npm-digest');
const trunkDigest = core.getInput('trunk-digest');
const canonicalCases = [
  ['npm', {
    actualRawPath:'/home/runner/.npm',
    actualKey:'node-cache-Linux-x64-npm-acd55e82e0cce419528277c42cc2770a73b0de23412aa645c8d7f11b63a11612',
    setupNodeLockDigest:npmDigest,
  },'04d00cc68e25ee34fd8b04095d0a46a28244d98af53e6925e47ed311d487b1d6'],
  ['trunk', {
    actualRawPath:'/home/runner/work/_temp/ci-tools/trunk-x86_64-unknown-linux-gnu.tar.gz',
    actualKey:'trunk-archive-Linux-X64-0.21.14-3ab6647b8afaff6415c91dac7bd89cce4d2214ad21d095f4d9484e57544dd233',
    trunkVersion:'0.21.14', installerHashFiles:trunkDigest,
  },'e0efc04ef87192cfccfa4a2769ba83d2298c280c86817c2034049f142b313649'],
];
const childEnvironment = Object.fromEntries(
  ['PATH','LANG','LC_ALL','HOME','TMPDIR'].filter(key => environment[key]).map(key => [key,environment[key]]),
);
for (const [role,metadata,version] of canonicalCases) {
  const layout = fullRegressionCacheLayout(role, metadata);
  assert.equal(layout.version,version);
  await verifyFullCacheGeometry(environment, layout);
  let validated;
  const hit = await restoreFullCache(sdk,role,metadata,archive => {
    validated = JSON.parse(execFileSync('/usr/bin/python3', [
      `${workspace}/scripts/ci-trusted-full-regression-cache.py`,'inspect',role,'--archive',archive,
    ], {cwd:workspace,env:childEnvironment,timeout:70000,encoding:'utf8',maxBuffer:16384}));
  });
  if (hit) {
    assert.ok(validated.files>0);
    if (role==='trunk') assert.ok(fs.statSync(layout.rawPaths[0]).isFile());
    else assert.ok(fs.readdirSync(layout.rawPaths[0]).length>0);
  }
  core.setOutput(`${role}-hit`,String(hit));
  console.log(JSON.stringify({role, key:layout.key, version, hit, verified:validated, production_authority:false, probe_only:true}));
}
