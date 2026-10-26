import assert from 'node:assert/strict';
import { test } from 'node:test';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { Miniflare, convertV4MiniflareOptions } from 'miniflare';
const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
function options(persist) {
  return { ...convertV4MiniflareOptions({ name: 'native-manifest-fixture', modulesRoot: root,
    compatibilityDate: '2026-10-25', cf: false,
    modules: [ { type: 'ESModule', path: resolve(root, 'tests/fixtures/native_runtime_harness.mjs') },
      { type: 'ESModule', path: resolve(root, 'artifacts/native-harness/index.js') },
      { type: 'CompiledWasm', path: resolve(root, 'artifacts/native-harness/index_bg.wasm') } ],
    bindings: { CUSTODY_NATIVE_PROVISION_ENABLED: 'true' },
    durableObjects: { STORE: { className: 'NativeSession', useSQLite: true } },
  }), resourcePersistencePath: persist, unsafeDevRegistryPath: resolve(persist, 'registry') };
}
async function invoke(mf, name, action) {
  const { STORE } = await mf.getBindings();
  return STORE.get(STORE.idFromName(name)).fetch('https://identity-native.internal/internal/harness', {
    method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(action),
  });
}
async function withFixture(run) {
  const persist = mkdtempSync(resolve(tmpdir(), 'oasis7-native-manifest-'));
  let mf = new Miniflare(options(persist));
  const restart = async () => { await mf.dispose(); mf = new Miniflare(options(persist)); return mf; };
  try { await run(() => mf, restart); } finally { await mf.dispose(); rmSync(persist, { recursive: true, force: true }); }
}
test('actual SQLite retains exact public registration fact across pre-Account-commit restart', { timeout: 60000 }, async () => withFixture(async (get, restart) => {
  const original = await (await invoke(get(), 'completed-original', 'complete-registration-before-account-submit')).json();
  assert.equal(original.fixture_only, true);
  await restart();
  const recovered = await (await invoke(get(), 'completed-original', 'recover-completed-registration')).json();
  assert.equal(recovered.phase, 'complete');
  assert.deepEqual(recovered.reconcile, original.original, 'reconcile must submit original signed bytes, including timestamp and nonce');
  assert.equal(recovered.private_sid_generation, null, 'registration is never session attestation');
}));
test('actual SQLite records B2 recovery as the exact known B1 receipt without rewriting binding identity', { timeout: 60000 }, async () => withFixture(async get => {
  const response = await invoke(get(), 'b2-known-b1', 'b2-recovers-known-b1');
  assert.equal(response.status, 200);
  const value = await response.json();
  assert.equal(value.current_binding_operation, 'fixture-B2');
  assert.equal(value.recovered.result.receipt.binding_operation_id, 'fixture-B1');
  assert.deepEqual(value.recovered, value.original);
}));
test('concurrent actual SQLite dispatch claims grant exactly one initial-login send; restart never grants replay', { timeout: 60000 }, async () => withFixture(async (get, restart) => {
  assert.equal((await invoke(get(), 'single-dispatch', 'prepare-initial-login')).status, 200);
  const claims = await Promise.all([invoke(get(), 'single-dispatch', 'dispatch-initial-login'), invoke(get(), 'single-dispatch', 'dispatch-initial-login')]);
  assert.deepEqual(claims.map(r => r.status).sort(), [200, 409], 'second durable dispatch must fail instead of treating Dispatch->Dispatch as an idempotent grant');
  await restart();
  const lost = await (await invoke(get(), 'single-dispatch', 'recover-initial-login-without-sid')).json();
  assert.equal(lost.phase, 'explicit_login_required');
  assert.equal(lost.retry_rejected, true);
  assert.equal((await invoke(get(), 'single-dispatch', 'dispatch-initial-login')).status, 409);
}));
