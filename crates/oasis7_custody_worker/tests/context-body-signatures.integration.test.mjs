import assert from 'node:assert/strict';
import test from 'node:test';
import {readFileSync} from 'node:fs';
import {dirname, resolve} from 'node:path';
import {fileURLToPath} from 'node:url';
import {Miniflare, convertV4MiniflareOptions} from 'miniflare';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const fixture = JSON.parse(readFileSync(resolve(root, 'artifacts/context-body-native-fixture.json'), 'utf8'));
assert.equal(fixture.label, 'synthetic-native-core-webcrypto-replay-not-live-authority');
assert.equal(fixture.clock_mode, 'fixed-component-replay-only');
assert.equal(fixture.scope_write_deadlines_are_historical, true);
assert.equal(fixture.captured_prepared_envelope_is_structural_not_aead_proof, true);

test('actual Core manifests replay strong Native artifacts through Rust WebCrypto and the final factory', {timeout: 120000}, async () => {
  const mf = new Miniflare(convertV4MiniflareOptions({
    name: 'context-signature-fixture', modulesRoot: root, compatibilityDate: '2026-10-25', cf: false,
    modules: [
      {type: 'ESModule', path: resolve(root, 'artifacts/native-harness/entry.mjs')},
      {type: 'ESModule', path: resolve(root, 'artifacts/native-harness/index.js')},
      {type: 'CompiledWasm', path: resolve(root, 'artifacts/native-harness/index_bg.wasm')},
    ],
    durableObjects: {FIXTURE: {className: 'ContextBodyChecksHarness', useSQLite: true}},
  }));
  try {
    const {FIXTURE} = await mf.getBindings();
    const actor = FIXTURE.get(FIXTURE.idFromName('fixed-offline-native-replay'));
    const invoke = value => actor.fetch('https://fixture.invalid/replay', {
      method: 'POST', headers: {'content-type': 'application/json'}, body: JSON.stringify(value),
    });
    const counts = [];
    assert.deepEqual(fixture.cases.map(value => value.name), ['prepared', 'captured']);
    for (const value of fixture.cases) {
      const before = JSON.stringify(value);
      const response = await invoke(value);
      assert.equal(response.status, 200, `${value.name} must pass actual SDK/Core and WebCrypto`);
      const output = await response.json();
      assert.equal(output.clock_mode, 'fixed-component-replay-only');
      assert.ok(output.manifest_jobs >= 3);
      assert.ok(output.canonical_bytes > 0 && output.canonical_bytes <= 64 * 1024);
      assert.match(output.body_digest, /^[0-9a-f]{64}$/);
      counts.push(output.manifest_jobs);
      assert.equal(JSON.stringify(value), before);
      const badSignature = structuredClone(value);
      badSignature.response.signature_hex = '00'.repeat(64);
      const rejected = await invoke(badSignature);
      assert.equal(rejected.status, 409);
      await rejected.arrayBuffer();
      const unknown = await invoke({...value, verified: true});
      assert.equal(unknown.status, 409);
      await unknown.arrayBuffer();
    }
    assert.equal(counts[1] - counts[0], 4, 'Captured adds Get, Prefix, committed Prefix outer, and Captured outer jobs');
  } finally { await mf.dispose(); }
});
