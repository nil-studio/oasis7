import assert from 'node:assert/strict';
import { test } from 'node:test';
import { readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { unstable_dev } from 'wrangler';
import Ajv from 'ajv/dist/2020.js';
const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const contract = JSON.parse(readFileSync(resolve(root, 'contract/http.schema.json'), 'utf8'));
const ajv = new Ajv({ strict: true });
const metadata = ajv.compile(contract.metadata);
const error = ajv.compile(contract.error);
const roles = { identity: 'NativeSession', owner: 'AccountAuthority', executor: 'ExecutorKeys', 'recovery-fence': 'RecoveryFence' };

for (const [role, ownClass] of Object.entries(roles)) {
  test(role + ': real Wasm exports and HTTP contract stay isolated', { timeout: 60_000 }, async () => {
    const entry = resolve(root, 'artifacts', role, 'index.js');
    const source = readFileSync(entry, 'utf8');
    // worker-build bundles only the selected Rust DO class into this ES module.
    for (const name of Object.values(roles)) {
      if (name === ownClass) assert.match(source, new RegExp('\\b' + name + '\\b'));
      else assert.doesNotMatch(source, new RegExp('\\b' + name + '\\b'));
    }
    const server = await unstable_dev(entry, {
      config: resolve(root, 'deploy', role, 'wrangler.toml'),
      local: true,
      experimental: { disableExperimentalWarning: true },
    });
    try {
      const response = await server.fetch('/v1/metadata');
      assert.equal(response.status, 200);
      const body = await response.json();
      assert.ok(metadata(body), JSON.stringify(metadata.errors));
      assert.deepEqual(body, contract.fixtures.metadata.find(value => value.role === role));
      assert.equal(body.execution_enabled, false);
      for (const path of ['/v1/actions', '/v1/accounts', '/v1/test-sessions', '/v1/owner-intents', '/get_secret', '/v1/metadata?secret=anything']) {
        const rejected = await server.fetch(path, { method: 'POST', body: '{}' });
        assert.equal(rejected.status, 404);
        const failure = await rejected.json();
        assert.ok(error(failure), JSON.stringify(error.errors));
        assert.equal(failure.code, 'not_found');
      }
      const method = await server.fetch('/v1/metadata', { method: 'POST' });
      assert.equal(method.status, 405);
      assert.ok(error(await method.json()));
    } finally { await server.stop(); }
  });
}

test('Rust fixture schemas reject unknown data and missing fields', () => {
  for (const value of contract.fixtures.metadata) assert.ok(metadata(value));
  for (const value of contract.fixtures.error) assert.ok(error(value));
  assert.equal(metadata({ ...contract.fixtures.metadata[0], capability: 'secret' }), false);
  assert.equal(metadata({ ...contract.fixtures.metadata[0], role: 'admin' }), false);
  assert.equal(error({ code: 'not_found' }), false);
});
