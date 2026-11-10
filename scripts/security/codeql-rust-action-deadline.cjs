'use strict';
// A synchronous preload: the outer runner Node never falls through into a second main.
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const child = require('node:child_process');
const MODE = 'candidate-workload-research';
const ACTION = '33d7b85da1a04171e211befe2c739303b9a4c066';
const MAIN_BLOB = '8b8462e5a06f394cd0504d57868488cebc7030c9';
const POST_BLOB = 'ea0e9ead88d0835b98aac612aa89b76fed74b7a5';

function main() {
  if (process.env.CAPACITY_MODE !== MODE) return;
  const entry = process.argv[1];
  const base = path.basename(entry || '');
  if (!entry || fs.realpathSync(entry) !== entry || !['analyze-entry.js', 'analyze-post-entry.js'].includes(base)
      || path.basename(path.dirname(entry)) !== 'lib'
      || path.basename(path.dirname(path.dirname(entry))) !== ACTION) {
    throw new Error('unexpected fixed CodeQL Action entry');
  }
  const bytes = fs.readFileSync(entry);
  const blob = crypto.createHash('sha1').update(`blob ${bytes.length}\0`).update(bytes).digest('hex');
  if (blob !== (base === 'analyze-entry.js' ? MAIN_BLOB : POST_BLOB)) throw new Error('fixed Action bootstrap identity differs');
  // The runner still registers and executes the unchanged official post hook, after the job steps.
  if (base === 'analyze-post-entry.js') return;
  if (process.env.OASIS7_CODEQL_ACTION_SUPERVISOR !== undefined) {
    if (String(process.ppid) !== process.env.OASIS7_CODEQL_ACTION_SUPERVISOR) throw new Error('supervisor parent differs');
    const out = path.join(process.env.RUNNER_TEMP, 'rust-capacity-evidence');
    const budgetFile = path.join(out, 'capacity-budget.json');
    if (fs.realpathSync(out) !== out || fs.realpathSync(budgetFile) !== budgetFile
        || fs.statSync(budgetFile).size > 256 * 1024) throw new Error('unsafe Action budget path');
    const raw = fs.readFileSync(budgetFile);
    const budget = JSON.parse(raw.toString());
    const digest = crypto.createHash('sha256').update(raw).digest('hex');
    const cutoff = Number(process.env.CAPACITY_ANALYSIS_DEADLINE);
    if (!Number.isFinite(cutoff) || cutoff !== budget.analysis_deadline || budget.mode !== MODE
        || digest !== process.env.OASIS7_CODEQL_ACTION_BUDGET_SHA256
        || Date.now() / 1000 >= cutoff - 3) throw new Error('original analysis deadline expired before actual Action main');
    const receipt = {pid: process.pid, supervisor_pid: process.ppid, action_sha: ACTION,
      source_root: process.cwd(), analysis_deadline: cutoff, budget_sha256: digest,
      started_at: Date.now() / 1000, node: process.execPath, node_version: process.version,
      exec_argv: process.execArgv, argv: process.argv.slice(1)};
    // No raw Action output or environment is copied; retain only bounded control metadata.
    let total = 0, count = 0;
    function inventory(directory) {
      for (const name of fs.readdirSync(directory)) {
        if (++count > 8192) throw new Error('official evidence inventory bound');
        const p = path.join(directory, name), stat = fs.lstatSync(p);
        if (stat.isSymbolicLink()) throw new Error('unsafe official evidence');
        if (stat.isDirectory()) inventory(p);
        else if (stat.isFile()) total += stat.size;
        else throw new Error('unsafe official evidence type');
      }
    }
    inventory(out);
    const data = JSON.stringify(receipt, null, 2) + '\n';
    if (Buffer.byteLength(data) > 32768 || total + Buffer.byteLength(data) > 255 * 1024 ** 2) throw new Error('official receipt capacity exhausted');
    fs.writeFileSync(path.join(out, 'action-deadline-start.json'), data, {flag: 'wx', mode: 0o600});
    delete process.env.NODE_OPTIONS;
    delete process.env.OASIS7_CODEQL_ACTION_SUPERVISOR;
    delete process.env.OASIS7_CODEQL_ACTION_BUDGET_SHA256;
    // Check again after bounded receipt work, immediately before the original main is admitted.
    if (Date.now() / 1000 >= cutoff - 3) throw new Error('original analysis deadline expired at Action main admission');
    return;
  }
  const invocation = JSON.stringify({node: process.execPath, exec_argv: process.execArgv,
    argv: process.argv.slice(1), cwd: process.cwd(), parent_pid: process.pid});
  const result = child.spawnSync('/usr/bin/python3', [path.join(__dirname, 'codeql_rust_action_deadline.py'), invocation],
    {cwd: process.cwd(), env: process.env, stdio: 'inherit'});
  if (result.error || result.signal || result.status === null) throw new Error('Action supervisor did not complete normally');
  process.exit(result.status);
}
try { main(); }
catch (error) {
  process.stderr.write('CodeQL absolute deadline admission failed: ' + String(error.message) + '\n');
  process.exit(1);
}
