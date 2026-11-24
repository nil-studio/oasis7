#!/usr/bin/env node
// Disposable loopback runtime + real HTTP/WebAuthn adapter contract evidence.
// Virtual authenticators and the debug minimal world do not certify gameplay.
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { mkdtempSync, writeFileSync, readFileSync, openSync, chmodSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { homedir } from 'node:os';
import { generateKeyPairSync, randomBytes, sign, createHash } from 'node:crypto';
import { spawn, execFileSync } from 'node:child_process';
import net from 'node:net';
const arg = (name, fallback) => {
  const i = process.argv.indexOf(name);
  return i < 0 ? fallback : process.argv[i + 1];
};
const runtimeBin = arg('--runtime-bin'),
  fixtureBin = arg('--fixture-bin');
const url = arg('--url', 'https://localhost:8449/');
assert.ok(runtimeBin && fixtureBin, '--runtime-bin and --fixture-bin required');
assert.equal(new URL(url).origin, 'https://localhost:8449');
const scratch = mkdtempSync(join(tmpdir(), 'oasis7-world-auth-smoke-'));
chmodSync(scratch, 0o700);
const keys = () => {
  const k = generateKeyPairSync('ed25519');
  return {
    ...k,
    public: k.publicKey.export({
      format: 'der',
      type: 'spki'
    }).subarray(-32).toString('hex'),
    seed: k.privateKey.export({
      format: 'der',
      type: 'pkcs8'
    }).subarray(-32).toString('hex')
  };
};
const issuer = keys(),
  sessionKey = randomBytes(32).toString('hex');
writeFileSync(join(scratch, 'test-only.env'), `OASIS7_HOSTED_AUTHORIZATION_ISSUER_PRIVATE_KEY=${issuer.seed}\nOASIS7_PASSKEY_FIXTURE_SESSION_KEY=${sessionKey}\n`, {
  mode: 0o600
});
const evidence = {
  kind: 'isolated_debug_runtime_auth_contract',
  gameplay: 'not_tested',
  real_device_support: 'not_tested',
  checks: {},
  binary_sha256: {
    runtime: createHash('sha256').update(readFileSync(runtimeBin)).digest('hex'),
    fixture: createHash('sha256').update(readFileSync(fixtureBin)).digest('hex')
  }
};
const children = [];
let browser;
const sleep = ms => new Promise(r => setTimeout(r, ms));
async function free(port) {
  await new Promise((resolve, reject) => {
    const s = net.createServer();
    s.once('error', reject);
    s.listen(port, '127.0.0.1', () => s.close(resolve));
  });
}
function launch(bin, args, env, name) {
  const fd = openSync(join(scratch, name + '.log'), 'w', 0o600);
  const p = spawn('rtk', ['proxy', bin, ...args], {
    env: {
      PATH: process.env.PATH,
      ...env
    },
    stdio: ['ignore', fd, fd],
    detached: true
  });
  children.push(p);
  return p;
}
function rpc(command) {
  return new Promise((resolve, reject) => {
    const s = net.connect(5023, '127.0.0.1');
    let buf = '',
      greeted = false;
    const timer = setTimeout(() => {
      s.destroy();
      reject(new Error('runtime RPC timeout'));
    }, 3000);
    const done = value => {
      clearTimeout(timer);
      s.destroy();
      resolve(value);
    };
    s.on('error', e => {
      clearTimeout(timer);
      reject(e);
    });
    s.on('connect', () => s.write(JSON.stringify({
      type: 'hello',
      client: 'isolated_auth_smoke',
      version: 2
    }) + '\n'));
    s.on('data', chunk => {
      buf += chunk;
      assert.ok(buf.length < 4_194_304);
      let n;
      while ((n = buf.indexOf('\n')) >= 0) {
        const v = JSON.parse(buf.slice(0, n));
        buf = buf.slice(n + 1);
        if (v.type === 'hello_ack' && !greeted) {
          assert.equal(v.world_id, 'live-runtime-minimal');
          greeted = true;
          s.write(JSON.stringify({
            type: 'authoritative_recovery',
            command
          }) + '\n');
        } else if (greeted && ['authoritative_recovery_ack', 'authoritative_recovery_error'].includes(v.type)) return done(v);
      }
    });
  });
}
async function waitRuntime() {
  for (let i = 0; i < 100; i++) {
    try {
      await rpc({
        mode: 'reconnect_sync',
        request: {
          player_id: 'smoke_readiness'
        }
      });
      return;
    } catch {}
    await sleep(100);
  }
  throw new Error('runtime did not start');
}
async function waitFixture() {
  for (let i = 0; i < 100; i++) {
    if (readFileSync(join(scratch, 'fixture.log'), 'utf8').includes('virtual_fixture_ready')) return;
    await sleep(100);
  }
  throw new Error('fixture did not start');
}
try {
  await free(5023);
  await free(4179);
  launch(runtimeBin, ['minimal', '--allow-debug-scenario', '--no-llm', '--no-auto-play', '--bind', '127.0.0.1:5023', '--no-web-bind', '--deployment-mode', 'hosted_public_join'], {
    OASIS7_HOSTED_IDENTITY_V2_ENABLED: '1',
    OASIS7_HOSTED_AUTHORIZATION_ISSUER_PUBLIC_KEY: issuer.public,
    OASIS7_HOSTED_IDENTITY_NAMESPACE: '15:virtual-fixture:test',
    OASIS7_HOSTED_IDENTITY_INSTANCE_EPOCH: '1',
    OASIS7_HOSTED_WORLD_ID: 'live-runtime-minimal',
    OASIS7_HOSTED_WORLD_IDENTITY_SCOPE: 'fixture-world',
    OASIS7_HOSTED_REGISTRATION_REPLAY_LEDGER_PATH: join(scratch, 'runtime-replay.json')
  }, 'runtime');
  await waitRuntime();
  const list = execFileSync('rtk', ['proxy', fixtureBin, '--list'], {
    encoding: 'utf8',
    env: {
      PATH: process.env.PATH
    }
  });
  const test = list.split('\n').find(x => x.includes('hosted_passkey_virtual_authenticator_fixture: test'))?.split(': test')[0];
  assert.ok(test, 'fixture test missing');
  launch(fixtureBin, ['--ignored', '--exact', test, '--nocapture'], {
    OASIS7_PASSKEY_FIXTURE_ROOT: scratch,
    OASIS7_PASSKEY_FIXTURE_SESSION_KEY: sessionKey,
    OASIS7_HOSTED_IDENTITY_V2_ENABLED: '1',
    OASIS7_HOSTED_AUTHORIZATION_ISSUER_PRIVATE_KEY: issuer.seed,
    OASIS7_HOSTED_WORLD_ID: 'live-runtime-minimal',
    OASIS7_HOSTED_WORLD_ADMISSION_MODE: 'open_preview'
  }, 'fixture');
  await waitFixture();
  console.log(JSON.stringify({
    state: 'fixture_ready',
    scratch,
    ports: {
      http: 4179,
      runtime: 5023,
      tls: 8449
    }
  }));
  const modules = process.env.OASIS7_PLAYWRIGHT_NODE_MODULES || join(homedir(), '.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules');
  const require = existsSync(join(modules, 'playwright')) ? createRequire(join(modules, 'smoke.cjs')) : createRequire(import.meta.url);
  browser = await require('playwright').chromium.launch({
    channel: 'chrome',
    headless: true
  });
  async function client(credential) {
    const context = await browser.newContext({
      ignoreHTTPSErrors: true
    });
    const page = await context.newPage();
    const cdp = await context.newCDPSession(page);
    await cdp.send('WebAuthn.enable');
    const {
      authenticatorId
    } = await cdp.send('WebAuthn.addVirtualAuthenticator', {
      options: {
        protocol: 'ctap2',
        transport: 'internal',
        hasResidentKey: true,
        hasUserVerification: true,
        isUserVerified: true,
        automaticPresenceSimulation: true
      }
    });
    if (credential) await cdp.send('WebAuthn.addCredential', {
      authenticatorId,
      credential
    });
    await page.goto(url);
    await page.evaluate(async () => {
      const m = await import('/viewer_passkey_account_module.js');
      const a = await import('/viewer_auth_crypto.js');
      window.client = m.createPasskeyAccountClient();
      window.authEnvelope = a.buildAuthEnvelope;
      window.result = null;
      const b = document.createElement('button');
      b.id = 'auth-smoke-action';
      b.textContent = 'test';
      b.onclick = async () => {
        try {
          window.result = {
            ok: true,
            value: await window.client[window.operation](window.payload)
          };
        } catch (e) {
          window.result = {
            ok: false,
            code: e.code,
            status: e.status,
            payload: e.payload
          };
        }
      };
      document.body.append(b);
    });
    return {
      context,
      page,
      cdp,
      authenticatorId
    };
  }
  async function action(c, operation, payload) {
    await c.page.evaluate(({
      operation,
      payload
    }) => {
      window.operation = operation;
      window.payload = payload;
      window.result = null;
    }, {
      operation,
      payload
    });
    await c.page.locator('#auth-smoke-action').click();
    await c.page.waitForFunction(() => window.result !== null);
    return c.page.evaluate(() => window.result);
  }
  async function okAction(c, op, p) {
    const r = await action(c, op, p);
    assert.equal(r.ok, true, `HTTP ${op}: ${r.code}/${r.status}`);
    return r.value;
  }
  let nonce = Date.now();
  async function register(c, g, k) {
    const n = ++nonce,
      payload = {
        operation: 'session_register',
        player_id: g.player_id,
        public_key: k.public,
        nonce: n,
        force_rebind: false
      };
    const bytes = await c.page.evaluate(p => Array.from(window.authEnvelope(p)), payload);
    const signature = 'awviewauth:v1:' + sign(null, Buffer.from(bytes), k.privateKey).toString('hex');
    const r = await rpc({
      mode: 'register_session',
      request: {
        player_id: g.player_id,
        public_key: k.public,
        registration_grant: g.registration_grant,
        force_rebind: false,
        auth: {
          scheme: 'ed25519',
          player_id: g.player_id,
          public_key: k.public,
          nonce: n,
          signature
        }
      }
    });
    assert.equal(r.type, 'authoritative_recovery_ack', `register: ${r.error?.message || JSON.stringify(r)}`);
    assert.equal(r.ack.status, 'session_registered');
    assert.equal(r.ack.session_epoch, g.world_session_epoch);
    return r.ack;
  }
  const a = await client();
  await okAction(a, 'capabilities');
  const account = await okAction(a, 'register');
  assert.equal(account.world.state, 'not_requested');
  evidence.checks.account_no_auto_join = 'passed';
  const k1 = keys(),
    attempt1 = randomBytes(32).toString('base64url');
  const joined = await okAction(a, 'joinWorld', {
    public_key: k1.public,
    join_attempt_id: attempt1,
    takeover: false,
    expected_epoch: 0
  });
  const g1 = joined.grant;
  assert.ok(!('release_token' in g1));
  await register(a, g1, k1);
  evidence.checks.real_register = 'passed';
  const binding = (g, k) => ({
    public_key: k.public,
    device_session_id: g.device_session_id,
    expected_epoch: g.world_session_epoch
  });
  const keyed = (g, k) => rpc({
    mode: 'reconnect_sync',
    request: {
      player_id: g.player_id,
      session_pubkey: k.public
    }
  });
  const bound = await okAction(a, 'refreshWorld', binding(g1, k1));
  assert.equal(bound.world.state, 'bound');
  await register(a, bound.grant, k1);
  for (let i = 0; i < 4; i++) {
    await sleep(10_000);
    const renewed = await okAction(a, 'refreshWorld', binding(g1, k1));
    await register(a, renewed.grant, k1);
  }
  assert.equal((await keyed(g1, k1)).type, 'authoritative_recovery_ack');
  evidence.checks.renewal_past_35_seconds = 'passed';
  console.log(JSON.stringify({
    state: 'renewal_past_35_seconds_passed'
  }));
  await sleep(31_500);
  assert.equal((await keyed(g1, k1)).type, 'authoritative_recovery_error');
  evidence.checks.no_perpetual_cached_auth = 'passed';
  await register(a, (await okAction(a, 'refreshWorld', binding(g1, k1))).grant, k1);
  assert.equal((await keyed(g1, k1)).type, 'authoritative_recovery_ack');
  evidence.checks.renew_after_expiry = 'passed';
  const creds = await a.cdp.send('WebAuthn.getCredentials', {
    authenticatorId: a.authenticatorId
  });
  const b = await client(creds.credentials[0]);
  await okAction(b, 'capabilities');
  const login = await okAction(b, 'login');
  assert.equal(login.account.account_id, account.account.account_id);
  assert.equal(login.world.state, 'not_requested');
  const k2 = keys(),
    attempt2 = randomBytes(32).toString('base64url');
  const conflict = await action(b, 'joinWorld', {
    public_key: k2.public,
    join_attempt_id: attempt2,
    takeover: false,
    expected_epoch: g1.world_session_epoch
  });
  assert.equal(conflict.ok, false);
  assert.equal(conflict.code, 'control_elsewhere');
  evidence.checks.second_session_no_implicit_takeover = 'passed';
  const taken = await okAction(b, 'joinWorld', {
    public_key: k2.public,
    join_attempt_id: attempt2,
    takeover: true,
    expected_epoch: g1.world_session_epoch
  });
  const g2 = taken.grant;
  assert.equal(g2.world_session_epoch, g1.world_session_epoch + 1);
  await register(b, g2, k2);
  assert.equal((await keyed(g1, k1)).type, 'authoritative_recovery_error');
  evidence.checks.explicit_takeover_fences_old_key = 'passed';
  const oldrelease = await action(a, 'releaseWorld', binding(g1, k1));
  assert.equal(oldrelease.ok, false);
  assert.equal((await keyed(g2, k2)).type, 'authoritative_recovery_ack');
  evidence.checks.old_release_preserves_new_control = 'passed';
  await okAction(b, 'refreshWorld', binding(g2, k2));
  await okAction(b, 'releaseWorld', binding(g2, k2));
  assert.equal((await keyed(g2, k2)).type, 'authoritative_recovery_error');
  const fence = await rpc({
    mode: 'reconnect_sync',
    request: {
      player_id: g2.player_id
    }
  });
  assert.equal(fence.ack.session_epoch, g2.world_session_epoch + 1);
  assert.ok(!fence.ack.session_pubkey);
  evidence.checks.current_release_revokes_runtime = 'passed';
  evidence.ok = true;
  writeFileSync(join(scratch, 'evidence.json'), JSON.stringify(evidence, null, 2), {
    mode: 0o600
  });
  console.log(JSON.stringify({
    ...evidence,
    scratch
  }));
  const hold = Number(arg('--hold-ms', '0'));
  assert.ok(Number.isFinite(hold) && hold >= 0 && hold <= 180000);
  if (hold) await sleep(hold);
} catch (e) {
  evidence.ok = false;
  evidence.failure = e.message;
  writeFileSync(join(scratch, 'evidence.json'), JSON.stringify(evidence, null, 2), {
    mode: 0o600
  });
  console.error(JSON.stringify({
    ok: false,
    scratch,
    error: e.message,
    checks: evidence.checks
  }));
  process.exitCode = 1;
} finally {
  try {
    if (browser) await Promise.race([browser.close(), sleep(3000).then(() => {
      throw new Error('browser_close_timeout');
    })]);
  } catch {
    process.exitCode = 1;
    console.error(JSON.stringify({ cleanup: 'browser_close_failed' }));
  } finally {
    try {
      writeFileSync(join(scratch, 'stop'), 'stop', { mode: 0o600 });
    } catch {
      process.exitCode = 1;
      console.error(JSON.stringify({ cleanup: 'fixture_stop_file_failed' }));
    }
    for (const p of children) {
      if (p.exitCode !== null || p.signalCode !== null) continue;
      try { process.kill(-p.pid, 'SIGTERM'); } catch {}
    }
    await sleep(500);
    for (const p of children) {
      if (p.exitCode !== null || p.signalCode !== null) continue;
      try { process.kill(-p.pid, 'SIGKILL'); } catch {}
    }
  }
}
