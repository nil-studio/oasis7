import assert from 'node:assert/strict';
import { test } from 'node:test';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { Miniflare, convertV4MiniflareOptions } from 'miniflare';
import { setTimeout as delay } from 'node:timers/promises';
const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const namespace = 'identity:test';
const manager = { identity_namespace: namespace, native_account_id: 'manager-one' };
const world = { world_id: 'world-one', genesis_digest: 'genesis-one', branch_id: 'branch-one', reorg_epoch: '0', service_public_key: 'ab'.repeat(32) };
const routes = { environment: 'test', identity_route: 'native-one', world_route: 'world-route-one', world_identity: world.world_id };
function session(account = manager.native_account_id) {
  const now = Date.now();
  return { api_version: 1, identity_namespace: namespace, identity_instance_epoch: '3', account_id: account,
    session_ref: 'synthetic-component-session', credential_ref: 'synthetic-component-passkey', credential_version: '9007199254740993',
    security_version: '18446744073709551614', policy_version: '4', server_time: String(now),
    expires_at: String(now + 600000), idle_expires_at: String(now + 300000) };
}
function request(id) { return { api_version: 1, client_request_id: id, plan: { ...routes, purpose: 'bounded component fixture', expires_at: String(Date.now() + 120000), transport_limit: '3' } }; }
function options(persist) {
  return { ...convertV4MiniflareOptions({ workers: [{ name: 'custody-owner-fixture', modules: [
    { type: 'ESModule', path: resolve(root, 'tests/fixtures/owner_directory_caller.mjs') },
    { type: 'ESModule', path: resolve(root, 'artifacts/owner/entry.mjs') },
    { type: 'ESModule', path: resolve(root, 'artifacts/owner/index.js') },
    { type: 'CompiledWasm', path: resolve(root, 'artifacts/owner/index_bg.wasm') },
  ], modulesRoot: root, compatibilityDate: '2026-10-25', cf: false,
    serviceBindings: { MANAGEMENT: { name: 'custody-owner-fixture', entrypoint: 'OwnerManagement' }, DEFAULT_OWNER: { name: 'custody-owner-deployed' } },
    durableObjects: { ACCOUNTS: { className: 'AccountHarness', useSQLite: true }, MANAGER_ASSOCIATIONS: { className: 'ManagerAssociations', useSQLite: true } },
    bindings: { CUSTODY_NATIVE_NAMESPACE: namespace, CUSTODY_NATIVE_INSTANCE_EPOCH: '3', CUSTODY_NATIVE_ENVIRONMENT: 'test',
      CUSTODY_BOOTSTRAP_MANAGERS: JSON.stringify([{ principal: manager, environments: ['test'] }]),
      CUSTODY_PROVISION_ROUTES: JSON.stringify(routes), CUSTODY_PROVISION_WORLD_IDENTITY: JSON.stringify(world), CUSTODY_POLICY_VERSION: '1' },
  }, { name: 'custody-owner-deployed', modules: [
    { type: 'ESModule', path: resolve(root, 'artifacts/owner/entry.mjs') },
    { type: 'ESModule', path: resolve(root, 'artifacts/owner/index.js') },
    { type: 'CompiledWasm', path: resolve(root, 'artifacts/owner/index_bg.wasm') },
  ], modulesRoot: root, compatibilityDate: '2026-10-25', cf: false,
    durableObjects: { ACCOUNTS: { className: 'AccountAuthority', useSQLite: true }, MANAGER_ASSOCIATIONS: { className: 'ManagerAssociations', useSQLite: true } },
  }] }), resourcePersistencePath: persist, unsafeDevRegistryPath: resolve(persist, 'registry') };
}
async function call(mf, path, body) {
  if (body.manager_session) body = { ...body, manager_session: { ...body.manager_session, server_time: String(Date.now()) } };
  return mf.dispatchFetch('https://component.test' + path, { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify(body) }); }
async function ok(response) { assert.equal(response.status, 200, response.status === 200 ? '' : await response.text()); return response.json(); }
async function account(mf, name, path, body) {
  const { ACCOUNTS } = await mf.getBindings();
  return ACCOUNTS.get(ACCOUNTS.idFromName(name)).fetch('https://account.test' + path, {
    method: 'POST', headers: { 'content-type': 'application/json' }, ...(body === undefined ? {} : { body: JSON.stringify(body) }),
  });
}
async function inspect(mf, name) { return ok(await account(mf, name, '/fixture/inspect')); }
async function fixture(run) {
  const persist = mkdtempSync(resolve(tmpdir(), 'oasis7-custody-owner-directory-'));
  let mf = new Miniflare(options(persist));
  const restart = async () => { await mf.dispose(); mf = new Miniflare(options(persist)); return mf; };
  try { await run(() => mf, restart); } finally { await mf.dispose(); rmSync(persist, { recursive: true, force: true }); }
}
async function draft(mf, id) {
  const view = await ok(await call(mf, '/reserve', { manager_session: session(), request: request(id) }));
  return { entry: view.entry, plan: view.plan, world_identity: view.world_identity };
}
function target(value) { return { custody_account_id: value.entry.custody_account_id, manager, environment: 'test', revocation_digest: 'cd'.repeat(32), minimum_security_epoch: '9007199254740993' }; }

test('actual AccountAuthority persists exact draft, empty closure and tombstone across restart', { timeout: 90000 }, async () => fixture(async (current, restart) => {
  let mf = current(); const value = await draft(mf, 'direct-original'); const name = 'direct-original';
  const original = await ok(await account(mf, name, '/internal/draft', value));
  assert.equal(original.phase, 'draft_initialized');
  assert.deepEqual(original.plan, value.plan); assert.deepEqual(original.world_identity, world);
  assert.deepEqual(await ok(await account(mf, name, '/internal/draft', value)), original);
  const before = await inspect(mf, name); assert.equal(before.audit.length, 1);
  const changed = structuredClone(value); changed.world_identity.genesis_digest = 'changed-genesis';
  assert.equal((await account(mf, name, '/internal/draft', changed)).status, 409);
  assert.deepEqual(await inspect(mf, name), before);
  const frozen = await ok(await account(mf, name, '/internal/freeze', target(value)));
  assert.equal(frozen.frozen_security_epoch, '9007199254740993'); assert.deepEqual(frozen.claimed_operation_ids, []);
  const after = await inspect(mf, name); assert.equal(after.audit.length, 2);
  assert.deepEqual(await ok(await account(mf, name, '/internal/freeze', target(value))), frozen);
  assert.deepEqual(await inspect(mf, name), after);
  mf = await restart();
  assert.deepEqual(await ok(await account(mf, name, '/internal/freeze', target(value))), frozen);
  assert.equal((await account(mf, name, '/internal/draft', value)).status, 409);
  assert.deepEqual(await inspect(mf, name), after);
  const early = 'freeze-before-init';
  await ok(await account(mf, early, '/internal/freeze', target(value)));
  assert.equal((await account(mf, early, '/internal/draft', value)).status, 409);
}));

test('unknown records, future schema and triggers freeze persistently without false empty receipts', { timeout: 90000 }, async () => fixture(async current => {
  const mf = current(); const value = await draft(mf, 'unknown-source');
  for (const fault of ['unknown-row', 'unknown-table', 'unknown-trigger', 'future-draft']) {
    const name = 'fault-' + fault;
    await ok(await account(mf, name, '/internal/draft', value));
    assert.equal((await account(mf, name, '/fixture/' + fault)).status, 204);
    const before = await inspect(mf, name);
    assert.equal((await account(mf, name, '/internal/freeze', target(value))).status, 503);
    const after = await inspect(mf, name);
    assert.ok(after.records.some(row => row.id === 'custody-account-freeze-v1'));
    for (const row of before.records) assert.deepEqual(after.records.find(after => after.id === row.id), row);
    assert.equal((await account(mf, name, '/internal/draft', value)).status, 409);
    assert.equal((await account(mf, name, '/internal/freeze', target(value))).status, 503);
    assert.deepEqual(await inspect(mf, name), after);
  }
}));

test('SQLite failure rolls back freeze row and audit together; explicit retry commits once', { timeout: 90000 }, async () => fixture(async current => {
  const mf = current(); const value = await draft(mf, 'rollback-source'); const name = 'rollback';
  await ok(await account(mf, name, '/internal/draft', value));
  assert.equal((await account(mf, name, '/fixture/rollback-trigger')).status, 204);
  const before = await inspect(mf, name);
  assert.equal((await account(mf, name, '/internal/freeze', target(value))).status, 503);
  assert.deepEqual(await inspect(mf, name), before);
  assert.equal((await account(mf, name, '/fixture/drop-rollback-trigger')).status, 204);
  await ok(await account(mf, name, '/internal/freeze', target(value)));
  assert.equal((await inspect(mf, name)).audit.length, 2);
}));

test('ManagerAssociations preserves stable bodies, prevents IDOR and closes complete fanout', { timeout: 90000 }, async () => fixture(async (current, restart) => {
  let mf = current(); const originalRequest = request('stable');
  const raw = { manager_session: session(), request: originalRequest };
  const original = await ok(await call(mf, '/reserve', raw));
  assert.deepEqual(await ok(await call(mf, '/reserve', raw)), original);
  const changed = structuredClone(raw); changed.request.plan.purpose = 'changed';
  assert.equal((await call(mf, '/reserve', changed)).status, 409);
  assert.deepEqual(await ok(await call(mf, '/read', { manager_session: session(), provision_id: original.entry.provision_id })), original);
  const foreign = await call(mf, '/read', { manager_session: session('other-manager'), provision_id: original.entry.provision_id });
  assert.ok([403, 404, 409].includes(foreign.status));
  for (let n = 0; n < 9; n++) await draft(mf, 'fanout-' + n);
  const freeze = { manager_session: session(), request: { api_version: 1, operation_id: 'freeze-manager-one' } };
  const first = await ok(await call(mf, '/freeze-manager', freeze));
  assert.equal(first.status, 'pending'); assert.equal(first.associated_accounts, '10'); assert.ok(first.pending_accounts.length > 0);
  mf = await restart();
  const closed = await ok(await call(mf, '/freeze-manager', freeze));
  assert.equal(closed.status, 'closed'); assert.deepEqual(closed.pending_accounts, []); assert.deepEqual(closed.pending_operations, []);
  assert.deepEqual(await ok(await call(mf, '/freeze-manager', freeze)), closed);
  assert.equal((await call(mf, '/reserve', { manager_session: session(), request: request('late') })).status, 409);
}));

test('directory closure wins while an actual AccountAuthority initialization reply is delayed', { timeout: 90000 }, async () => fixture(async current => {
  const mf = current(); const raw = { manager_session: session(), request: request('race-original') };
  const original = await ok(await call(mf, '/reserve', raw));
  const name = original.entry.custody_account_id;
  assert.equal((await account(mf, name, '/fixture/pause-next-draft')).status, 204);
  const late = call(mf, '/reserve', raw);
  let paused = false;
  for (let attempt = 0; attempt < 200; attempt++) {
    if ((await ok(await account(mf, name, '/fixture/draft-paused'))).paused) { paused = true; break; }
    await delay(5);
  }
  assert.ok(paused, 'test gate must delay the real AccountAuthority request');
  const closed = await ok(await call(mf, '/freeze-manager', { manager_session: session(), request: { api_version: 1, operation_id: 'race-freeze' } }));
  assert.equal(closed.status, 'closed');
  assert.equal((await account(mf, name, '/fixture/resume-draft')).status, 204);
  const lateResponse = await late;
  if (lateResponse.status === 200) assert.equal((await lateResponse.json()).phase, 'frozen');
  else assert.equal(lateResponse.status, 409);
  const view = await ok(await call(mf, '/read', { manager_session: session(), provision_id: original.entry.provision_id }));
  assert.equal(view.phase, 'frozen');
}));

test('directory durably inventories a failed first initialization before freezing its Account DO', { timeout: 90000 }, async () => fixture(async (current, restart) => {
  let mf = current();
  const pending = await ok(await call(mf, '/reserve', { manager_session: session(), request: request('fixture-pending-first') }));
  assert.equal(pending.phase, 'draft_pending');
  const name = pending.entry.custody_account_id;
  assert.deepEqual((await inspect(mf, name)).records, []);
  mf = await restart();
  assert.equal((await ok(await call(mf, '/read', { manager_session: session(), provision_id: pending.entry.provision_id }))).phase, 'draft_pending');
  const closed = await ok(await call(mf, '/freeze-manager', { manager_session: session(), request: { api_version: 1, operation_id: 'freeze-pending' } }));
  assert.equal(closed.associated_accounts, '1'); assert.equal(closed.status, 'closed');
  assert.equal((await account(mf, name, '/fixture/allow-pending-draft')).status, 204);
  const value = { entry: pending.entry, plan: pending.plan, world_identity: pending.world_identity };
  assert.equal((await account(mf, name, '/internal/draft', value)).status, 409);
  assert.ok((await inspect(mf, name)).records.some(row => row.id === 'custody-account-freeze-v1'));
}));

test('manager closure remains pending across restart when an Account DO cannot prove its authorization inventory', { timeout: 90000 }, async () => fixture(async (current, restart) => {
  let mf = current(); const value = await draft(mf, 'unavailable-inventory'); const name = value.entry.custody_account_id;
  assert.equal((await account(mf, name, '/fixture/unknown-row')).status, 204);
  const freeze = { manager_session: session(), request: { api_version: 1, operation_id: 'freeze-unknown-inventory' } };
  const first = await ok(await call(mf, '/freeze-manager', freeze));
  assert.equal(first.status, 'pending'); assert.deepEqual(first.pending_accounts, [name]);
  mf = await restart();
  assert.deepEqual(await ok(await call(mf, '/freeze-manager', freeze)), first);
  const stored = await inspect(mf, name);
  assert.ok(stored.records.some(row => row.id === 'custody-account-freeze-v1'));
  assert.ok(stored.records.some(row => row.id === 'owner-approval-admission-v1'));
}));

test('expired same-original POST returns historical draft without fresh initialization or audit', { timeout: 90000 }, async () => fixture(async current => {
  const mf = current(); await draft(mf, 'expiry-warmup');
  const body = { manager_session: session(), request: request('expired-original') };
  body.request.plan.expires_at = String(Date.now() + 1000);
  const original = await ok(await call(mf, '/reserve', body));
  const before = await inspect(mf, original.entry.custody_account_id);
  await delay(1100);
  assert.deepEqual(await ok(await call(mf, '/reserve', body)), original);
  assert.deepEqual(await inspect(mf, original.entry.custody_account_id), before);
}));

test('deployed Owner default entrypoint cannot call generated Rust management RPC', { timeout: 90000 }, async () => fixture(async current => {
  const mf = current(); const body = { manager_session: session(), request: request('default-denied') };
  assert.equal((await call(mf, '/default-rpc', body)).status, 409);
  assert.equal((await ok(await call(mf, '/reserve', body))).phase, 'draft_initialized');
}));


test('persistent freeze cursor reaches healthy targets behind eight unconfirmable accounts', { timeout: 90000 }, async () => fixture(async (current, restart) => {
  let mf = current(); const values = [];
  for (let i = 0; i < 9; i++) values.push(await draft(mf, 'starvation-' + i));
  values.sort((a, b) => a.entry.custody_account_id < b.entry.custody_account_id ? -1 : 1);
  for (const value of values.slice(0, 8)) await account(mf, value.entry.custody_account_id, '/fixture/unknown-row');
  const healthy = values[8].entry.custody_account_id;
  const freeze = { manager_session: session(), request: { api_version: 1, operation_id: 'starvation-freeze' } };
  const first = await ok(await call(mf, '/freeze-manager', freeze));
  assert.equal(first.status, 'pending'); assert.equal(first.pending_accounts.length, 9);
  mf = await restart();
  const second = await ok(await call(mf, '/freeze-manager', freeze));
  assert.equal(second.status, 'pending'); assert.equal(second.pending_accounts.length, 8);
  assert.ok(!second.pending_accounts.includes(healthy), 'later healthy account must freeze while earlier unknown inventory stays pending');
  const saved = await inspect(mf, healthy);
  assert.ok(saved.records.some(row => row.id.includes('freeze')), 'healthy Account DO must persist its own freeze tombstone');
}));
