const {spawn} = require('node:child_process');
const path = require('node:path');
const {main} = require('../../../scripts/storage-producer-diagnostics.cjs');
function command(args, options) {
  return new Promise(resolve => {
    const child = spawn('python3', args, {...options, stdio: 'inherit'});
    child.once('error', () => resolve(1));
    child.once('exit', code => resolve(Number.isInteger(code) ? code : 1));
  });
}
async function runBuild({root, diagnostics, env}, invoke = command) {
  const bundle = path.join(env.RUNNER_TEMP, 'component');
  const script = path.join(root, 'scripts/ci-storage-acceptance.py');
  const code = await invoke([script, 'build', '--component', 'storage-required', '--bundle', bundle,
    '--build-diagnostics', diagnostics], {cwd: root, env});
  if (code) return code;
  return invoke([script, 'pack', '--component', 'storage-required', '--bundle', bundle,
    '--output', path.join(env.RUNNER_TEMP, 'storage-required.tar.gz')], {cwd: root, env});
}
module.exports = {runBuild};
if (require.main === module) main({runBuild}).then(code => { process.exitCode = code; }).catch(() => {
  console.error('::error::Storage producer orchestration failed'); process.exitCode = 1;
});
