import assert from 'node:assert/strict';
import { test } from 'node:test';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { Miniflare, convertV4MiniflareOptions } from 'miniflare';
const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
function options(fixture, className, persist, bindings = {}) {
  const modules = [{ type: 'ESModule', path: resolve(root, 'tests/fixtures', fixture) }];
  if (className === 'RecoveryFence') {
    modules.push({ type: 'ESModule', path: resolve(root, 'artifacts/recovery-fence/index.js') },
      { type: 'CompiledWasm', path: resolve(root, 'artifacts/recovery-fence/index_bg.wasm') });
  } else modules.push({ type: 'ESModule', path: resolve(root, 'platform/sql.mjs') });
  const converted = convertV4MiniflareOptions({ name: "custody-fixture", modules, modulesRoot: root,
    compatibilityDate: '2026-10-25', cf: false, durableObjects: { STORE: { className, useSQLite: true } },
    bindings });
  return { ...converted, resourcePersistencePath: persist, unsafeDevRegistryPath: resolve(persist, 'registry') };
}
async function get(mf, path) {
  const { STORE } = await mf.getBindings();
  return STORE.get(STORE.idFromName('same-original')).fetch('https://fixture.test' + path);
}

test('actual SQLite glue rolls back partial state and audit; TEXT preserves full-width integers', { timeout: 60000 }, async () => {
  const persist = mkdtempSync(resolve(tmpdir(), 'oasis7-custody-sql-'));
  let mf = new Miniflare(options('sql_harness.mjs', 'SqlHarness', persist));
  try {
    const initial = await (await get(mf, '/read')).json();
    assert.equal(JSON.parse(initial.payload).epoch, '9007199254740993');
    assert.equal(JSON.parse(initial.payload).label, '测试');
    assert.deepEqual(await (await get(mf, '/rollback')).json(), initial);
    const committed = await (await get(mf, '/advance')).json();
    assert.equal(JSON.parse(committed.payload).epoch, '18446744073709551615');
    await mf.dispose();
    mf = new Miniflare(options('sql_harness.mjs', 'SqlHarness', persist));
    assert.deepEqual(await (await get(mf, '/read')).json(), committed);
  } finally { await mf.dispose(); rmSync(persist, { recursive: true, force: true }); }
});

test('Rust RecoveryFence initializes independently frozen and keeps its epoch across namespace restart', { timeout: 60000 }, async () => {
  const persist = mkdtempSync(resolve(tmpdir(), 'oasis7-custody-fence-'));
  let mf = new Miniflare(options('fence_harness.mjs', 'RecoveryFence', persist,
    { CUSTODY_INITIAL_SERVICE_EPOCH: '18446744073709551615' }));
  try {
    let response = await get(mf, '/internal/recovery-fence');
    assert.equal(response.status, 200);
    assert.deepEqual(await response.json(), { epoch: '18446744073709551615', disabled: true });
    await mf.dispose();
    mf = new Miniflare(options('fence_harness.mjs', 'RecoveryFence', persist,
      { CUSTODY_INITIAL_SERVICE_EPOCH: '1' }));
    assert.deepEqual(await (await get(mf, '/internal/recovery-fence')).json(),
      { epoch: '18446744073709551615', disabled: true });
    const denied = await get(mf, '/internal/recovery-fence?enable=true');
    assert.equal(denied.status, 404);
  } finally { await mf.dispose(); rmSync(persist, { recursive: true, force: true }); }
});
