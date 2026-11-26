'use strict';
// Build-time only. Never patch an already distributed bundle at action runtime.
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const {execFileSync} = require('node:child_process');
const root = __dirname;
const digest = data => crypto.createHash('sha256').update(data).digest('hex');
const patch = JSON.parse(fs.readFileSync(path.join(root, 'sdk-patch.json')));
const provenance = JSON.parse(fs.readFileSync(path.join(root, 'provenance.json')));
const lock = JSON.parse(fs.readFileSync(path.join(root, 'package-lock.json')));
const sdkRoot = path.join(root, 'node_modules/@actions/cache');
if (patch.sdk !== '6.1.0' || JSON.parse(fs.readFileSync(path.join(sdkRoot, 'package.json'))).version !== patch.sdk ||
    lock.packages['node_modules/@actions/cache'].integrity !== provenance.sdk.dist.integrity ||
    provenance.upstream_action.sha !== '55cc8345863c7cc4c66a329aec7e433d2d1c52a9') throw Error('unverified SDK provenance');
for (const [file, change] of Object.entries(patch.files)) {
  if (!/^lib\/(?:internal\/)?[A-Za-z]+(?:\.d)?\.js$/.test(file) && !/^lib\/(?:internal\/)?[A-Za-z]+\.d\.ts$/.test(file)) throw Error('unexpected patch path');
  const target = path.join(sdkRoot, file);
  if (JSON.stringify(provenance.patched_files[file]) !== JSON.stringify({before_sha256: change.before_sha256, after_sha256: change.after_sha256})) throw Error('provenance patch mismatch');
  let data = fs.readFileSync(target, 'utf8');
  if (digest(data) === change.after_sha256) continue;
  if (digest(data) !== change.before_sha256) throw Error(`SDK preimage mismatch: ${file}`);
  for (const replacement of change.replacements) {
    const count = data.split(replacement.before).length - 1;
    if (count !== (replacement.count || 1)) throw Error(`ambiguous SDK patch: ${file}`);
    data = data.split(replacement.before).join(replacement.after);
  }
  if (digest(data) !== change.after_sha256) throw Error(`SDK postimage mismatch: ${file}`);
  fs.writeFileSync(target, data);
}
if (process.argv.includes('--patch-only')) process.exit(0);
const destination = path.resolve(root, '../../../vendor/actions-cache-explicit-context');
fs.mkdirSync(destination, {recursive: true});
execFileSync(process.execPath, [path.join(root, 'node_modules/@vercel/ncc/dist/ncc/cli.js'), 'build', 'entry.mjs', '-o', destination, '--license', 'licenses.txt', '--minify'], {cwd: root, stdio: 'inherit'});
const inputs = ['entry.mjs', 'build.cjs', 'package.json', 'package-lock.json', 'provenance.json', 'sdk-patch.json'];
const outputs = ['index.mjs', 'licenses.txt'];
const hashes = (directory, names) => Object.fromEntries(names.map(name => [name, digest(fs.readFileSync(path.join(directory, name)))]));
fs.writeFileSync(path.join(root, 'build-receipt.json'), JSON.stringify({schema: 1, inputs: hashes(root, inputs), outputs: hashes(destination, outputs)}, null, 2) + '\n');
