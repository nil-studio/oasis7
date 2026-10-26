import assert from 'node:assert/strict';
import { test } from 'node:test';
import { readFileSync, mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { Miniflare, convertV4MiniflareOptions } from 'miniflare';
import Ajv from 'ajv/dist/2020.js';
const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const contract = JSON.parse(readFileSync(resolve(root, 'contract/http.schema.json'), 'utf8'));
const ajv = new Ajv({ strict: true });
const metadata = ajv.compile(contract.metadata);
const error = ajv.compile(contract.error);
const roles = { identity: 'NativeSession', owner: 'AccountAuthority', executor: 'ExecutorKeys', 'recovery-fence': 'RecoveryFence' };

for (const [role, ownClass] of Object.entries(roles)) {
  test(role + ': real Wasm exports and HTTP contract stay isolated', { timeout: 60_000 }, async () => {
    const index = resolve(root, 'artifacts', role, 'index.js');
    const entry = ['identity', 'owner'].includes(role) ? resolve(root, 'artifacts', role, 'entry.mjs') : index;
    const source = readFileSync(index, 'utf8');
    assert.doesNotMatch(source, /\bTestApprovalAuthority\b/);
    // worker-build bundles only the selected Rust DO class into this ES module.
    for (const name of Object.values(roles)) {
      if (name === ownClass) assert.match(source, new RegExp('\\b' + name + '\\b'));
      else assert.doesNotMatch(source, new RegExp('\\b' + name + '\\b'));
    }
    // Execute the saved artifact directly: deployment configs have build hooks,
    // which must never run concurrently with the component tests' Wasm artifact.
    // verify.sh separately validates each real deployment config by dry-run.
    const persist = mkdtempSync(resolve(tmpdir(), 'oasis7-custody-role-'));
    const server = new Miniflare({ ...convertV4MiniflareOptions({
      name: 'custody-production-role-' + role,
      modules: [{ type: 'ESModule', path: entry },
        ...(entry !== index ? [{ type: 'ESModule', path: index }] : []),
        { type: 'CompiledWasm', path: resolve(root, 'artifacts', role, 'index_bg.wasm') }],
      modulesRoot: root, compatibilityDate: '2026-10-25', cf: false,
      durableObjects: { ROLE: { className: ownClass, useSQLite: true },
        ...(role === 'owner' ? { DIRECTORY: { className: 'ManagerAssociations', useSQLite: true } } : {}) },
    }), resourcePersistencePath: persist, unsafeDevRegistryPath: resolve(persist, 'registry') });
    try {
      const response = await server.dispatchFetch('https://fixture.test/v1/metadata');
      assert.equal(response.status, 200);
      const body = await response.json();
      assert.ok(metadata(body), JSON.stringify(metadata.errors));
      assert.deepEqual(body, contract.fixtures.metadata.find(value => value.role === role));
      assert.equal(body.execution_enabled, false);
      for (const path of ['/v1/actions', '/v1/accounts', '/v1/test-sessions', '/v1/owner-intents', '/get_secret', '/v1/metadata?secret=anything']) {
        const rejected = await server.dispatchFetch('https://fixture.test' + path, { method: 'POST', body: '{}' });
        assert.equal(rejected.status, 404);
        const failure = await rejected.json();
        assert.ok(error(failure), JSON.stringify(error.errors));
        assert.equal(failure.code, 'not_found');
      }
      const method = await server.dispatchFetch('https://fixture.test/v1/metadata', { method: 'POST' });
      assert.equal(method.status, 405);
      assert.ok(error(await method.json()));
    } finally { await server.dispose(); rmSync(persist, { recursive: true, force: true }); }
  });
}

test('Rust fixture schemas reject unknown data and missing fields', () => {
  for (const value of contract.fixtures.metadata) assert.ok(metadata(value));
  for (const value of contract.fixtures.error) assert.ok(error(value));
  assert.equal(metadata({ ...contract.fixtures.metadata[0], capability: 'secret' }), false);
  assert.equal(metadata({ ...contract.fixtures.metadata[0], role: 'admin' }), false);
  assert.equal(error({ code: 'not_found' }), false);
});
