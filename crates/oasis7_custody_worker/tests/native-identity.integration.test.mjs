import assert from 'node:assert/strict';
import { test } from 'node:test';
import { spawn } from 'node:child_process';
import { once } from 'node:events';
import { existsSync, readFileSync, writeFileSync, mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { setTimeout as delay } from 'node:timers/promises';
import { Miniflare, convertV4MiniflareOptions, Response as MiniflareResponse } from 'miniflare';
const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');

async function authority(directory) {
  const binary = readFileSync(resolve(root, 'artifacts/native-test-binary.txt'), 'utf8').trim();
  const ready = resolve(directory, 'native-ready.json');
  const stop = resolve(directory, 'native-stop');
  const child = spawn(binary, ['serve_native_identity_http_fixture', '--ignored', '--nocapture'], {
    env: { ...Object.fromEntries(Object.entries(process.env).filter(([name]) => !name.startsWith('OASIS7_'))),
      OASIS7_CUSTODY_NATIVE_HTTP_READY: ready, OASIS7_CUSTODY_NATIVE_HTTP_STOP: stop },
    stdio: ['ignore', 'ignore', 'ignore'],
  });
  let exited = false;
  const exit = once(child, 'exit').then(([code]) => { exited = true; return code; });
  const close = async () => {
    writeFileSync(stop, 'stop');
    await Promise.race([exit, delay(5000, undefined, { ref: false })]);
    if (!exited) { child.kill('SIGTERM'); await exit; }
  };
  for (let attempt = 0; attempt < 300; attempt++) {
    if (exited) throw new Error('isolated native authority fixture exited before ready');
    if (existsSync(ready)) {
      try { return { data: JSON.parse(readFileSync(ready, 'utf8')), close }; }
      catch { /* Writer may still be completing its bounded local fixture file. */ }
    }
    await delay(20);
  }
  await close();
  throw new Error('isolated native authority fixture did not become ready');
}
function bindings(data) {
  return {
    CUSTODY_NATIVE_ORIGIN: 'https://localhost', CUSTODY_NATIVE_RP_ID: 'localhost',
    CUSTODY_NATIVE_NAMESPACE: data.namespace, CUSTODY_NATIVE_INSTANCE_EPOCH: data.identity_instance_epoch,
    CUSTODY_NATIVE_ENVIRONMENT: 'test', CUSTODY_NATIVE_AUDIENCE: data.audience,
    CUSTODY_NATIVE_APPROVAL_KEY_ID: data.key_id, CUSTODY_NATIVE_APPROVAL_PUBLIC_KEY: data.public_key,
    CUSTODY_NATIVE_ADAPTER_SECRET: data.adapter_secret,
    ...(data.revocation_poll_secret ? { CUSTODY_NATIVE_REVOCATION_POLL_SECRET: data.revocation_poll_secret, CUSTODY_NATIVE_REVOCATION_ACK_SECRET: data.revocation_ack_secret } : {}),
  };
}
const world = { world_id: 'world-one', genesis_digest: 'genesis-one', branch_id: 'branch-one', reorg_epoch: '0', service_public_key: 'ab'.repeat(32) };
const routes = { environment: 'test', identity_route: 'native-one', world_route: 'world-route-one', world_identity: world.world_id };
function ownerBindings(data) {
  return { CUSTODY_NATIVE_APPROVAL_KEY_ID: data.key_id, CUSTODY_NATIVE_APPROVAL_PUBLIC_KEY: data.public_key, CUSTODY_NATIVE_AUDIENCE: data.audience,
    CUSTODY_REVOCATION_OWNER_KEY_ID: 'owner-source-test', CUSTODY_REVOCATION_OWNER_SIGNING_KEY: Buffer.alloc(32, 92).toString('hex'), CUSTODY_NATIVE_NAMESPACE: data.namespace, CUSTODY_NATIVE_INSTANCE_EPOCH: data.identity_instance_epoch,
    CUSTODY_NATIVE_ENVIRONMENT: 'test', CUSTODY_POLICY_VERSION: '1',
    CUSTODY_BOOTSTRAP_MANAGERS: JSON.stringify([{ principal: { identity_namespace: data.namespace, native_account_id: 'a' }, environments: ['test'] }]),
    CUSTODY_PROVISION_ROUTES: JSON.stringify(routes), CUSTODY_PROVISION_WORLD_IDENTITY: JSON.stringify(world) };
}
function options(directory, data, bridge, overrides = {}) {
  return { ...convertV4MiniflareOptions({
    workers: [{ name: 'caller', modules: [{ type: 'ESModule', path: resolve(root, 'tests/fixtures/native_identity_caller.mjs') }],
      modulesRoot: root, compatibilityDate: '2026-10-25', cf: false,
      serviceBindings: { PUMP: { name: 'identity', entrypoint: 'ScheduledFixture' }, IDENTITY: { name: 'identity', entrypoint: 'IdentityNativeApproval' }, IDENTITY_DEFAULT: 'identity' },
    }, { name: 'identity', modules: [
      { type: 'ESModule', path: resolve(root, 'tests/fixtures/native_revocation_scheduler.mjs') },
      { type: 'ESModule', path: resolve(root, 'artifacts/identity/entry.mjs') },
      { type: 'ESModule', path: resolve(root, 'artifacts/identity/index.js') },
      { type: 'CompiledWasm', path: resolve(root, 'artifacts/identity/index_bg.wasm') },
    ], modulesRoot: root, compatibilityDate: '2026-10-25', cf: false,
    bindings: { ...bindings(data), ...overrides }, outboundService: bridge,
    serviceBindings: { OWNER_REVOCATIONS: { name: 'owner', entrypoint: 'OwnerRevocations' }, OWNER_MANAGEMENT: { name: 'owner', entrypoint: 'OwnerManagement' } },
    durableObjects: { SESSIONS: { className: 'NativeSession', useSQLite: true } },
    }, { name: 'owner', modules: [
      { type: 'ESModule', path: resolve(root, 'artifacts/owner/entry.mjs') },
      { type: 'ESModule', path: resolve(root, 'artifacts/owner/index.js') },
      { type: 'CompiledWasm', path: resolve(root, 'artifacts/owner/index_bg.wasm') },
    ], modulesRoot: root, compatibilityDate: '2026-10-25', cf: false,
      bindings: ownerBindings(data),
      durableObjects: { ACCOUNTS: { className: 'AccountAuthority', useSQLite: true }, MANAGER_ASSOCIATIONS: { className: 'ManagerAssociations', useSQLite: true } },
    }],
  }), resourcePersistencePath: directory, unsafeDevRegistryPath: resolve(directory, 'registry') };
}

// The bridge maps only the fixture's pinned HTTPS authority to loopback HTTP.
// It does not alter the production HTTPS-only configuration or emulate the native authority.
test('production Identity workerd consumes actual native HTTP/SQLite management and private verification', { timeout: 90000 }, async () => {
  const directory = mkdtempSync(resolve(tmpdir(), 'oasis7-custody-native-http-'));
  let native;
  let mf;
  try {
    native = await authority(directory);
    let fault;
    let nativeStatus;
    let foreignAttempts = 0;
    const calls = [];
    const bridge = async request => {
      nativeStatus = undefined;
      const url = new URL(request.url);
      if (url.origin !== 'https://localhost') foreignAttempts++;
      assert.equal(url.origin, 'https://localhost');
      assert.ok(['/api/public/hosted-account/v2/custody/management-session-status', '/api/public/hosted-account/v2/custody/approvals/verify'].includes(url.pathname));
      if (url.pathname.endsWith('/management-session-status')) assert.ok(request.headers.get('cookie') === native.data.cookie, 'only current SID cookie may leave Identity');
      calls.push({ path: url.pathname, method: request.method, cookie: request.headers.has('cookie'), origin: request.headers.has('origin') });
      if (fault === 'redirect') return new MiniflareResponse(null, { status: 302, headers: { Location: 'https://other.invalid/steal' } });
      if (fault === 'oversize') return new MiniflareResponse(' '.repeat(32769), { headers: { 'Content-Type': 'application/json' } });
      const body = request.method === 'GET' ? undefined : Buffer.from(await request.arrayBuffer());
      // Rebuild only native semantic headers. The workerd stream is now bytes;
      // Node owns its loopback Host, Content-Length and other transport metadata.
      const nativeHeaders = new Headers();
      for (const name of ['authorization', 'cookie', 'origin', 'sec-fetch-site', 'x-oasis7-csrf', 'content-type']) {
        const value = request.headers.get(name);
        if (typeof value === 'string') nativeHeaders.set(name, value);
      }
      const response = await fetch(native.data.address + url.pathname, { method: request.method,
        headers: nativeHeaders, body, redirect: 'manual' });
      const text = await response.text();
      nativeStatus = response.status;
      if (fault === 'unknown' || fault === 'identity' || fault === 'signature' || fault === 'correlation') {
        const value = JSON.parse(text);
        if (fault === 'unknown') value.unknown = true;
        if (fault === 'identity') value.identity_namespace = 'other';
        if (fault === 'signature') value.signature = 'A'.repeat(86);
        if (fault === 'correlation') value.verification.verify_nonce = 'wrong-owner-nonce';
        return MiniflareResponse.json(value, { status: response.status });
      }
      return new MiniflareResponse(text, { status: response.status, headers: { 'Content-Type': 'application/json' } });
    };
    mf = new Miniflare(options(resolve(directory, 'workerd'), native.data, bridge));
    const identity = await mf.getWorker('identity');
    const headers = { Cookie: 'unrelated=must-stay-local; ' + native.data.cookie };
    let status = await identity.fetch('https://localhost/v1/management/session-status', { headers });
    assert.equal(status.status, 200, 'management native status=' + nativeStatus);
    const original = await status.json();
    assert.equal(original.account_id, 'a');
    assert.equal(original.identity_namespace, native.data.namespace);
    assert.equal(original.identity_instance_epoch, native.data.identity_instance_epoch);
    assert.ok(!JSON.stringify(original).includes(native.data.cookie));
    for (const invalid of [
      { Cookie: native.data.cookie + '; ' + native.data.cookie },
      { Cookie: native.data.cookie, Origin: 'https://other.invalid' },
      { Cookie: native.data.cookie, Authorization: 'Bearer arbitrary' },
      { Cookie: native.data.cookie, 'Sec-Fetch-Site': 'cross-site' },
    ]) assert.notEqual((await identity.fetch('https://localhost/v1/management/session-status', { headers: invalid })).status, 200);
    assert.notEqual((await identity.fetch('https://other.invalid/v1/management/session-status', { headers })).status, 200);
    assert.notEqual((await identity.fetch('https://localhost/v1/management/session-status?account_id=other', { headers })).status, 200);
    for (const kind of ['redirect', 'oversize', 'unknown', 'identity']) {
      fault = kind;
      assert.notEqual((await identity.fetch('https://localhost/v1/management/session-status', { headers })).status, 200);
    }
    fault = undefined;
    assert.equal(foreignAttempts, 0, 'manual redirects must never attempt another origin');
    const before = calls.length;
    let verify = await mf.dispatchFetch('https://caller.test/', { method: 'POST', body: JSON.stringify(native.data.verify_request) });
    assert.equal(verify.status, 200, 'private verification native dispatches=' + (calls.length - before) + ', status=' + nativeStatus);
    const signed = await verify.json();
    const dispatchCount = calls.length;
    const defaultRpc = await mf.dispatchFetch('https://caller.test/default-verify', { method: 'POST', body: JSON.stringify(native.data.verify_request) });
    assert.equal(defaultRpc.status, 409);
    assert.equal((await defaultRpc.json()).missing_rpc, true, 'default binding must not expose generated Rust RPC');
    assert.equal(calls.length, dispatchCount);
    assert.equal(signed.verification.verify_nonce, native.data.verify_request.verify_nonce);
    assert.equal(signed.verification.origin_state, 'active');
    assert.equal(calls.length, before + 1);
    assert.deepEqual(calls.at(-1), { path: '/api/public/hosted-account/v2/custody/approvals/verify', method: 'POST', cookie: false, origin: false });
    for (const kind of ['redirect', 'oversize', 'unknown', 'signature', 'correlation']) {
      fault = kind;
      assert.equal((await mf.dispatchFetch('https://caller.test/', { method: 'POST', body: JSON.stringify(native.data.verify_request) })).status, 409);
    }
    fault = undefined;
    assert.equal(foreignAttempts, 0, 'private verify must never forward a redirect');
    const noDispatch = calls.length;
    const wrongAudience = { ...native.data.verify_request, expected_audience: 'other' };
    assert.equal((await mf.dispatchFetch('https://caller.test/', { method: 'POST', body: JSON.stringify(wrongAudience) })).status, 409);
    const wrongIdentity = structuredClone(native.data.verify_request);
    wrongIdentity.approval.proof.identity_namespace = 'other';
    assert.equal((await mf.dispatchFetch('https://caller.test/', { method: 'POST', body: JSON.stringify(wrongIdentity) })).status, 409);
    const extra = { ...native.data.verify_request, url: 'https://other.invalid/' };
    assert.equal((await mf.dispatchFetch('https://caller.test/', { method: 'POST', body: JSON.stringify(extra) })).status, 409);
    assert.equal((await mf.dispatchFetch('https://caller.test/', { method: 'POST', body: ' '.repeat(32769) })).status, 409);
    assert.equal(calls.length, noDispatch);
    // The native service-auth verify route remains unavailable through public fetch.
    assert.equal((await identity.fetch('https://localhost/v1/approvals/verify', { method: 'POST', body: '{}' })).status, 404);
    // Real Passkey cookie + original browser CSRF/Origin must authorize the private
    // Identity -> Owner -> per-account SQLite draft initialization, without world calls.
    const draft = { api_version: 1, client_request_id: 'native-authenticated-draft',
      plan: { ...routes, purpose: 'native auth integration only', expires_at: String(Date.now() + 120000), transport_limit: '3' } };
    const writeHeaders = { ...headers, Origin: 'https://localhost', 'Content-Type': 'application/json', 'X-Oasis7-CSRF': native.data.csrf };
    for (const invalid of [
      { ...writeHeaders, 'X-Oasis7-CSRF': '0'.repeat(64) },
      { ...writeHeaders, Origin: 'https://other.invalid' },
    ]) assert.notEqual((await identity.fetch('https://localhost/v1/account-provisions', { method: 'POST', headers: invalid, body: JSON.stringify(draft) })).status, 200);
    const initialized = await identity.fetch('https://localhost/v1/account-provisions', { method: 'POST', headers: writeHeaders, body: JSON.stringify(draft) });
    assert.equal(initialized.status, 200);
    const view = await initialized.json();
    assert.equal(view.phase, 'draft_initialized');
    assert.deepEqual(view.entry.manager, { identity_namespace: native.data.namespace, native_account_id: 'a' });
    assert.deepEqual(view.world_identity, world);
    const repeated = await identity.fetch('https://localhost/v1/account-provisions', { method: 'POST', headers: writeHeaders, body: JSON.stringify(draft) });
    assert.equal(repeated.status, 200);
    assert.deepEqual(await repeated.json(), view);
    const read = await identity.fetch('https://localhost/v1/account-provisions/' + view.entry.provision_id, { headers });
    assert.equal(read.status, 200);
    assert.deepEqual(await read.json(), view);
    const selectedManager = { ...draft, manager: { native_account_id: 'other' } };
    assert.equal((await identity.fetch('https://localhost/v1/account-provisions', { method: 'POST', headers: writeHeaders, body: JSON.stringify(selectedManager) })).status, 400);
    let configCase = 0;
    for (const invalid of [
      { CUSTODY_NATIVE_ORIGIN: 'http://localhost' },
      { CUSTODY_NATIVE_ORIGIN: 'https://localhost/untrusted-path' },
      { CUSTODY_NATIVE_RP_ID: 'other.invalid' },
      { CUSTODY_NATIVE_NAMESPACE: 'other' },
      { CUSTODY_NATIVE_INSTANCE_EPOCH: '99' },
      { CUSTODY_NATIVE_AUDIENCE: 'other' },
      { CUSTODY_NATIVE_APPROVAL_PUBLIC_KEY: '01' + '00'.repeat(31) },
    ]) {
      const noDispatch = calls.length;
      const invalidWorker = new Miniflare(options(resolve(directory, 'config-' + configCase++), native.data, bridge, invalid));
      try {
        assert.equal((await invalidWorker.dispatchFetch('https://caller.test/', { method: 'POST', body: JSON.stringify(native.data.verify_request) })).status, 409);
        assert.equal(calls.length, noDispatch, 'configuration/binding rejection must precede native dispatch');
      } finally { await invalidWorker.dispose(); }
    }
    await fetch(native.data.address + '/_fixture/native-key', { method: 'POST' });
    assert.notEqual((await identity.fetch('https://localhost/v1/management/session-status', { headers })).status, 200);
    await fetch(native.data.address + '/_fixture/reset', { method: 'POST' });
    assert.equal((await identity.fetch('https://localhost/v1/management/session-status', { headers })).status, 200);
    await fetch(native.data.address + '/_fixture/expire', { method: 'POST' });
    assert.notEqual((await identity.fetch('https://localhost/v1/management/session-status', { headers })).status, 200);
    const inspect = { ...native.data.verify_request, purpose: 'inspect_origin', verify_nonce: 'owner-origin-inspection' };
    const inspected = await mf.dispatchFetch('https://caller.test/', { method: 'POST', body: JSON.stringify(inspect) });
    assert.equal(inspected.status, 200);
    assert.notEqual((await inspected.json()).verification.origin_state, 'active');
    assert.equal((await mf.dispatchFetch('https://caller.test/', { method: 'POST', body: JSON.stringify(native.data.verify_request) })).status, 409);
  } finally {
    try { if (mf) await mf.dispose(); }
    finally {
      try { if (native) await native.close(); }
      finally { rmSync(directory, { recursive: true, force: true }); }
    }
  }
});

// Actual native transaction/outbox, scheduled Identity handler, Owner SQLite closure,
// and native acknowledgement. Only the HTTPS-to-loopback transport is a local bridge.
test('revoked native manager reaches Owner closure without its revoked browser session', { timeout: 90000 }, async () => {
  const directory = mkdtempSync(resolve(tmpdir(), 'oasis7-custody-native-revocation-'));
  let native; let mf;
  try {
    native = await authority(directory); const calls = [];
    const bridge = async request => {
      const url = new URL(request.url); assert.equal(url.origin, 'https://localhost');
      assert.ok(['management-session-status', 'revocations/poll', 'revocations/ack'].some(path => url.pathname === '/api/public/hosted-account/v2/custody/' + path));
      if (url.pathname.includes('/revocations/')) { assert.equal(request.headers.has('cookie'), false); assert.equal(request.headers.has('origin'), false); }
      calls.push(url.pathname);
      const headers = new Headers(); for (const name of ['authorization', 'cookie', 'origin', 'sec-fetch-site', 'x-oasis7-csrf', 'content-type']) {
        const value = request.headers.get(name); if (typeof value === 'string') headers.set(name, value);
      }
      const body = request.method === 'GET' ? undefined : Buffer.from(await request.arrayBuffer());
      const result = await fetch(native.data.address + url.pathname, { method: request.method, headers, body, redirect: 'manual' });
      return new MiniflareResponse(await result.text(), { status: result.status, headers: { 'Content-Type': 'application/json' } });
    };
    const create = () => new Miniflare(options(resolve(directory, 'workerd'), native.data, bridge));
    mf = create(); let identity = await mf.getWorker('identity');
    const headers = { Cookie: native.data.cookie, Origin: 'https://localhost', 'Content-Type': 'application/json', 'X-Oasis7-CSRF': native.data.csrf };
    const draft = { api_version: 1, client_request_id: 'revocation-original', plan: { ...routes, purpose: 'native revocation integration', expires_at: String(Date.now() + 120000), transport_limit: '3' } };
    const initialized = await identity.fetch('https://localhost/v1/account-provisions', { method: 'POST', headers, body: JSON.stringify(draft) });
    assert.equal(initialized.status, 200); const view = await initialized.json(); assert.equal(view.phase, 'draft_initialized');
    assert.equal((await fetch(native.data.address + '/_fixture/revoke-manager', { method: 'POST' })).status, 200);
    assert.notEqual((await identity.fetch('https://localhost/v1/management/session-status', { headers: { Cookie: native.data.cookie } })).status, 200);
    const poll = async () => {
      const result = await fetch(native.data.address + '/api/public/hosted-account/v2/custody/revocations/poll', { method: 'POST',
        headers: { Authorization: 'Bearer ' + native.data.revocation_poll_secret, 'Content-Type': 'application/json' }, body: JSON.stringify({ api_version: 1, limit: 8 }) });
      assert.equal(result.status, 200); return result.json();
    };
    const pending = await poll(); assert.ok(pending.events.length > 0);
    const outboxStatus = async () => (await fetch(native.data.address + '/_fixture/revocation-status')).json();
    assert.ok((await outboxStatus()).pending > 0);
    assert.equal((await mf.dispatchFetch('https://caller.test/pump', { method: 'POST' })).status, 204);
    assert.deepEqual((await poll()).events, []);
    const confirmed = await outboxStatus(); assert.equal(confirmed.pending, 0); assert.ok(confirmed.acknowledged > 0);
    assert.ok(calls.some(path => path.endsWith('/revocations/ack')));
    await mf.dispose(); mf = create(); identity = await mf.getWorker('identity');
    assert.equal((await mf.dispatchFetch('https://caller.test/pump', { method: 'POST' })).status, 204);
    assert.deepEqual((await poll()).events, []);
    const accountBindings = await mf.getBindings('owner');
    const account = accountBindings.ACCOUNTS.get(accountBindings.ACCOUNTS.idFromName(view.entry.custody_account_id));
    const late = await account.fetch('https://account.test/internal/draft', { method: 'POST', headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ entry: view.entry, plan: view.plan, world_identity: view.world_identity }) });
    assert.equal(late.status, 409, 'confirmed source closure must preserve Account freeze across restart');
  } finally {
    try { if (mf) await mf.dispose(); } finally { try { if (native) await native.close(); } finally { rmSync(directory, { recursive: true, force: true }); } }
  }
});
