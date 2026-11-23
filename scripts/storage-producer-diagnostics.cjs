// Bounded observations only; acceptance artifacts retain their existing authority.
const fs = require('node:fs/promises');
const {constants} = require('node:fs');
const path = require('node:path');
const {createHash} = require('node:crypto');
const {spawn, execFileSync} = require('node:child_process');
const UPLOAD_SHA = 'cf430e030ddbb5b0abf93d22962f4752f3646cd9';
const UPLOAD_DIGEST = 'c0e58b240360573e6d6c82ea1bb22deff4f85883e00211a52bd27822c3a22617';
const MAX_BYTES = 256 * 1024, MAX_SNAPSHOTS = 31, INTERVAL_MS = 300000;

function buildEnvironment(env) {
  return Object.fromEntries(Object.entries(env).filter(([key]) =>
    !['ACTIONS_RUNTIME_TOKEN', 'ACTIONS_RESULTS_URL', 'ACTIONS_CACHE_URL', 'GITHUB_TOKEN', 'GH_TOKEN'].includes(key)));
}
async function readJson(file) {
  const handle = await fs.open(file, constants.O_RDONLY | constants.O_NOFOLLOW | constants.O_NONBLOCK);
  try {
    const stat = await handle.stat();
    if (!stat.isFile() || stat.size > MAX_BYTES) throw Error('invalid diagnostic file');
    const bytes = Buffer.alloc(MAX_BYTES + 1);
    let size = 0;
    while (size <= MAX_BYTES) {
      const {bytesRead} = await handle.read(bytes, size, bytes.length - size, size);
      if (!bytesRead) break;
      size += bytesRead;
    }
    if (size > MAX_BYTES) throw Error('diagnostic file too large');
    return JSON.parse(bytes.subarray(0, size).toString('utf8'));
  } finally { await handle.close(); }
}
const number = value => Number.isSafeInteger(value) && value >= 0 ? value : null;
const decimal = value => typeof value === 'number' && Number.isFinite(value) && value >= 0 ? value : null;
const boolean = value => typeof value === 'boolean' ? value : null;
const text = (value, limit = 64) => typeof value === 'string' && value.length <= limit ? value : null;
const fields = (value, names, convert = number) => Object.fromEntries(names.map(name => [name, convert(value?.[name])]));
const bounds = value => fields(value, ['start', 'end', 'min', 'max']);
function projectBuild(value) {
  if (!value || typeof value !== 'object') return null;
  const unit = value.latest_completed_unit;
  return {variant: value.variant === 'formal-required' ? value.variant : null,
    status: ['running', 'success', 'failure'].includes(value.status) ? value.status : null,
    started_at: text(value.started_at), observed_at: text(value.observed_at),
    elapsed_seconds: decimal(value.elapsed_seconds), cargo_exit_code: number(value.cargo_exit_code),
    completed_units: number(value.completed_units), latest_completed_unit: unit ? {
      observed_at: text(unit.observed_at), target_name: text(unit.target_name, 256),
      target_kind: Array.isArray(unit.target_kind) ? unit.target_kind.slice(0, 8).map(kind => text(kind)) : null,
      fresh: boolean(unit.fresh)} : null};
}
function projectResources(value) {
  if (!value || typeof value !== 'object') return null;
  const tree = value.process_tree, latest = tree?.latest;
  const processes = Array.isArray(latest?.processes) ? latest.processes : [];
  const eventNames = ['low', 'high', 'max', 'oom', 'oom_kill', 'oom_group_kill'];
  return {started_at: text(value.started_at), observed_at: text(value.ended_at),
    elapsed_seconds: decimal(value.elapsed_seconds), sample_count: number(value.sample_count),
    complete: boolean(value.complete), disk: fields(value.disk, ['total_bytes', 'free_bytes']),
    host: Object.fromEntries(['mem_total_bytes', 'mem_available_bytes', 'swap_free_bytes'].map(name => [name, bounds(value.host?.[name])])),
    cgroup: {memory_current_bytes: bounds(value.cgroup?.sampled?.memory_current_bytes),
      memory_max_bytes: fields(value.cgroup?.memory_max_bytes, ['start', 'end'], item => item === 'max' ? item : number(item)),
      memory_peak_since_creation_bytes: fields(value.cgroup?.memory_peak_since_creation_bytes, ['start', 'end']),
      events: Object.fromEntries(['start', 'end', 'delta'].map(name => [name, fields(value.cgroup?.events?.[name], eventNames)]))},
    swap_pages: Object.fromEntries(['start', 'end', 'delta'].map(name => [name, fields(value.swap_pages?.[name], ['pswpin', 'pswpout'])])),
    process_tree: {...fields(tree, ['root_pid', 'root_start_ticks', 'sample_count', 'sampled_peak_rss_bytes']),
      available: boolean(tree?.available), target_exited: boolean(tree?.target_exited),
      latest: latest ? {observed_at: text(latest.observed_at), cpu_seconds: decimal(latest.cpu_seconds),
        rss_bytes: number(latest.rss_bytes), omitted_processes: Math.max(0, processes.length - 64),
        processes: processes.slice(0, 64).map(process => ({...fields(process, ['pid', 'ppid', 'start_ticks', 'rss_bytes']),
          comm: text(process.comm, 64), state: text(process.state, 1), cpu_seconds: decimal(process.cpu_seconds)}))} : null}};
}
async function snapshot(directory, env, sequence) {
  const receipt = {schema: 1, diagnostic_only: true, probe_only: env.GITHUB_JOB === 'diagnostics-probe', source_sha: env.GITHUB_SHA,
    run_id: env.GITHUB_RUN_ID, attempt: env.GITHUB_RUN_ATTEMPT,
    component: 'storage-required', image: env.STORAGE_BUILD_IMAGE || null, sequence, observed_at: new Date().toISOString(),
    orchestrator: {rss_bytes: process.memoryUsage().rss,
      cpu_user_microseconds: process.resourceUsage().userCPUTime,
      cpu_system_microseconds: process.resourceUsage().systemCPUTime,
      scope: 'Current Node action process; CPU is lifetime cumulative, RSS is sampled. Excludes Cargo and upload child.'}};
  for (const [key, name] of [['build', 'build.json'], ['resources', 'resources.json']]) {
    try { receipt[key] = (key === 'build' ? projectBuild : projectResources)(await readJson(path.join(directory, 'formal-required', name))); }
    catch { receipt[key] = null; }
  }
  const json = JSON.stringify(receipt) + '\n';
  if (Buffer.byteLength(json) > MAX_BYTES) throw Error('diagnostic snapshot too large');
  return json;
}
module.exports = {buildEnvironment, readJson, snapshot, MAX_BYTES, MAX_SNAPSHOTS, INTERVAL_MS,
  UPLOAD_SHA, UPLOAD_DIGEST};

function execute(args, options, timeoutMs) {
  return new Promise(resolve => {
    const child = spawn(process.execPath, args, options);
    let timer, settled = false;
    const finish = code => {
      if (settled) return;
      settled = true;
      if (timer) clearTimeout(timer);
      resolve(Number.isInteger(code) ? code : 1);
    };
    child.once('error', () => finish(1));
    child.once('exit', finish);
    if (timeoutMs) timer = setTimeout(() => { child.kill('SIGKILL'); }, timeoutMs);
  });
}
async function downloadUploader(destination, fetchImpl = fetch) {
  const response = await fetchImpl(`https://raw.githubusercontent.com/actions/upload-artifact/${UPLOAD_SHA}/dist/upload/index.js`,
    {signal: AbortSignal.timeout(30000)});
  if (!response.ok || Number(response.headers.get('content-length')) > 5000000) throw Error('upload implementation download failed');
  const chunks = [];
  let size = 0;
  for await (const chunk of response.body) {
    size += chunk.length;
    if (size > 5000000) throw Error('upload implementation too large');
    chunks.push(Buffer.from(chunk));
  }
  const bytes = Buffer.concat(chunks, size);
  if (createHash('sha256').update(bytes).digest('hex') !== UPLOAD_DIGEST)
    throw Error('upload implementation integrity mismatch');
  await fs.writeFile(destination, bytes, {mode: 0o600, flag: 'wx'});
}
async function uploadSnapshot(uploader, file, name, env, executeImpl = execute) {
  const output = file + '.outputs';
  await fs.writeFile(output, '', {mode: 0o600});
  const uploadEnv = {...env, GITHUB_OUTPUT: output,
    INPUT_NAME: name, INPUT_PATH: file, 'INPUT_IF-NO-FILES-FOUND': 'error',
    'INPUT_RETENTION-DAYS': '2', 'INPUT_COMPRESSION-LEVEL': '1', INPUT_OVERWRITE: 'false',
    'INPUT_INCLUDE-HIDDEN-FILES': 'false', INPUT_ARCHIVE: 'true'};
  const code = await executeImpl([uploader], {env: uploadEnv, stdio: ['ignore', 'ignore', 'ignore']}, 30000);
  if (code !== 0) throw Error('diagnostic upload unsuccessful');
  const text = await fs.readFile(output, 'utf8');
  function readOutput(name) {
    const lines = text.split('\n');
    for (let i = 0; i < lines.length; i++) {
      if (lines[i].startsWith(name + '=')) return lines[i].slice(name.length + 1);
      if (lines[i].startsWith(name + '<<') && lines[i + 2] === lines[i].slice(name.length + 2)) return lines[i + 1];
    }
  }
  const id = readOutput('artifact-id');
  const digest = readOutput('artifact-digest');
  if (!/^\d+$/.test(id || '') || !/^[a-f0-9]{64}$/.test(digest || '')) throw Error('missing official upload acknowledgement');
  return {artifact_id: id, artifact_digest: digest};
}
async function main({env = process.env, runBuild, fetchImpl = fetch, executeImpl = execute} = {}) {
  const root = env.GITHUB_WORKSPACE;
  if (env.GITHUB_REPOSITORY !== 'nil-studio/oasis7' || env.GITHUB_REF !== 'refs/heads/main' ||
      env.GITHUB_EVENT_NAME !== 'workflow_dispatch' || env.SOURCE_SHA !== env.GITHUB_SHA || env.GITHUB_WORKFLOW_SHA !== env.GITHUB_SHA)
    throw Error('live diagnostics require trusted main dispatch source');
  if (!root || !env.RUNNER_TEMP || !/^[a-f0-9]{40}$/.test(env.GITHUB_SHA || '') ||
      !/^\d+$/.test(env.GITHUB_RUN_ID || '') || !/^[1-9]\d*$/.test(env.GITHUB_RUN_ATTEMPT || ''))
    throw Error('invalid producer identity');
  const actualRoot = await fs.realpath(root);
  if (actualRoot !== env.STORAGE_ARTIFACT_CHECKOUT) throw Error('producer checkout path mismatch');
  const actualHead = execFileSync('git', ['rev-parse', 'HEAD'], {
    cwd: actualRoot, env: buildEnvironment(env), timeout: 5000, encoding: 'utf8', stdio: ['ignore', 'pipe', 'ignore']}).trim();
  if (actualHead !== env.SOURCE_SHA) throw Error('producer checkout source mismatch');
  const directory = await fs.mkdtemp(path.join(env.RUNNER_TEMP, 'storage-required-diagnostics-'));
  const diagnostics = path.join(directory, 'observation');
  const uploader = path.join(directory, 'upload-artifact.mjs');
  let available = false, sequence = 0, busy = false;
  try { await downloadUploader(uploader, fetchImpl); available = true; }
  catch { console.log('::warning::Storage diagnostic uploader unavailable'); }
  async function publish() {
    if (!available || busy || sequence >= MAX_SNAPSHOTS) return;
    busy = true;
    const index = ++sequence;
    try {
      const name = `storage-required-diagnostic-${env.GITHUB_SHA}-${env.GITHUB_RUN_ID}-${env.GITHUB_RUN_ATTEMPT}-${index}`;
      const file = path.join(directory, name + '.json');
      await fs.writeFile(file, await snapshot(diagnostics, env, index), {mode: 0o600});
      const acknowledgement = await uploadSnapshot(uploader, file, name, env, executeImpl);
      console.log('storage_diagnostic_saved=' + JSON.stringify({sequence: index, ...acknowledgement}));
    } catch { console.log(`::warning::Storage diagnostic snapshot ${index} was not acknowledged`); }
    finally { busy = false; }
  }
  let inflight = publish();
  const timer = setInterval(() => { if (!busy) inflight = publish(); }, INTERVAL_MS);
  let code;
  try {
    code = await runBuild({root, diagnostics, env: buildEnvironment(env)});
  } finally { clearInterval(timer); }
  const pendingAtExit = busy;
  await inflight;
  if (!pendingAtExit) await publish();
  return code;
}
module.exports = {...module.exports, execute, downloadUploader, uploadSnapshot, main};
