import assert from 'node:assert/strict';
import test from 'node:test';
import {createHash, randomBytes, webcrypto} from 'node:crypto';
import {mkdtempSync, readFileSync, rmSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {dirname, resolve} from 'node:path';
import {fileURLToPath} from 'node:url';
import {Miniflare, convertV4MiniflareOptions} from 'miniflare';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const fixture = JSON.parse(readFileSync(resolve(root, 'artifacts/context-body-fixture.json'), 'utf8'));
assert.equal(fixture.label, 'synthetic-storage-scope-not-authority');
const entry = name => {
  const value = fixture.entries.find(item => item.name === name);
  assert.ok(value, `missing canonical structural scope ${name}`);
  assert.deepEqual(JSON.parse(value.scope_wire), value.scope);
  assert.deepEqual(JSON.parse(value.envelope_context_wire), value.envelope_context);
  return value;
};
const kek = Buffer.alloc(32, 0x6d); // Synthetic encryption input, never a custody grant.
const digest = bytes => createHash('sha256').update(bytes).digest('hex');
const recordDigest = value => digest(JSON.stringify(value));
const aad = (value, kind) => Buffer.from(value[`${kind}_aad_hex`], 'hex');
async function aes(raw, nonce, associated, input, encrypt) {
  const key = await webcrypto.subtle.importKey('raw', raw, 'AES-GCM', false, [encrypt ? 'encrypt' : 'decrypt']);
  const algorithm = {name: 'AES-GCM', iv: nonce, additionalData: associated, tagLength: 128};
  return Buffer.from(await webcrypto.subtle[encrypt ? 'encrypt' : 'decrypt'](algorithm, key, input));
}
async function encrypted(value, plaintext) {
  const dek = randomBytes(32), valueNonce = randomBytes(12), wrapNonce = randomBytes(12);
  assert.notDeepEqual(valueNonce, wrapNonce);
  const record = {
    scope: JSON.parse(value.scope_wire),
    envelope: {
      context: JSON.parse(value.envelope_context_wire),
      ciphertext: (await aes(dek, valueNonce, aad(value, 'value'), plaintext, true)).toString('hex'),
      value_nonce: valueNonce.toString('hex'),
      wrapped_dek: (await aes(kek, wrapNonce, aad(value, 'wrap'), dek, true)).toString('hex'),
      wrap_nonce: wrapNonce.toString('hex'),
      wrapping_key_version: '1',
    },
  };
  assert.ok(Buffer.byteLength(JSON.stringify(record)) <= 256 * 1024);
  return record;
}
async function decrypt(value, record) {
  const saved = record.envelope;
  const dek = await aes(kek, Buffer.from(saved.wrap_nonce, 'hex'), aad(value, 'wrap'), Buffer.from(saved.wrapped_dek, 'hex'), false);
  return aes(dek, Buffer.from(saved.value_nonce, 'hex'), aad(value, 'value'), Buffer.from(saved.ciphertext, 'hex'), false);
}
function options(persist, selected, bindings) {
  return {...convertV4MiniflareOptions({
    name: 'context-body-fixture', modulesRoot: root, compatibilityDate: '2026-10-25', cf: false,
    modules: [
      {type: 'ESModule', path: resolve(root, 'tests/fixtures/context_body_runtime.mjs')},
      {type: 'ESModule', path: resolve(root, 'artifacts/identity/entry.mjs')},
      {type: 'ESModule', path: resolve(root, 'artifacts/identity/index.js')},
      {type: 'CompiledWasm', path: resolve(root, 'artifacts/identity/index_bg.wasm')},
    ],
    durableObjects: {CONTEXT_BODIES: {className: 'FixtureContextBody', useSQLite: true}},
    // Neither a Native binding nor any KEK is configured in the opaque actor.
    bindings: {CUSTODY_CONTEXT_BODY_ENABLED: 'true',
      CUSTODY_ROLE_KEY_ENVIRONMENT: selected.scope.identity_scope.context.identity.environment, ...bindings},
  }), resourcePersistencePath: persist, unsafeDevRegistryPath: resolve(persist, 'registry')};
}
async function withStore(run, {name = 'prepared', bindings = {}} = {}) {
  const selected = entry(name), persist = mkdtempSync(resolve(tmpdir(), 'oasis7-context-body-'));
  let currentBindings = bindings;
  let mf = new Miniflare(options(persist, selected, currentBindings));
  const stub = async value => {
    const {CONTEXT_BODIES} = await mf.getBindings('context-body-fixture');
    return CONTEXT_BODIES.get(CONTEXT_BODIES.idFromName(value.actor_name));
  };
  const send = async (value, path, body) => (await stub(value)).fetch(`https://context-body.internal${path}`, {
    method: 'POST', headers: {'content-type': 'application/json'}, body: typeof body === 'string' ? body : JSON.stringify(body),
  });
  const inventory = async value => (await send(value, '/fixture/inventory', {})).json();
  const mutate = async (value, mode, record) => assert.equal((await send(value, '/fixture/mutate', {mode, record})).status, 204);
  const faults = async value => (await (await send(value, '/fixture/update-fault-count', {})).json()).reached;
  const restart = async next => {
    await mf.dispose(); currentBindings = {...currentBindings, ...next};
    mf = new Miniflare(options(persist, selected, currentBindings));
  };
  try { await run({selected, send, inventory, mutate, faults, restart, publicFetch: (...args) => mf.dispatchFetch(...args)}); }
  finally { await mf.dispose(); rmSync(persist, {recursive: true, force: true}); }
}
const consume = async response => { const status = response.status; await response.arrayBuffer(); return status; };
function metadataOnly(value) {
  if (!value || typeof value !== 'object') return;
  for (const [key, nested] of Object.entries(value)) {
    assert.ok(!['ciphertext', 'envelope', 'wrapped_dek', 'value_nonce', 'wrap_nonce', 'plaintext', 'body', 'kek', 'usable', 'verified'].includes(key));
    metadataOnly(nested);
  }
}

test('actual ContextBody opaque store bounds full 64KiB ciphertext and preserves the first envelope', {timeout: 60000}, async () => {
  await withStore(async ({selected, send, inventory}) => {
    const before = await inventory(selected);
    assert.equal((await (await send(selected, '/internal/metadata', selected.scope)).json()).stored, null);
    assert.equal(await (await send(selected, '/internal/read', selected.scope)).json(), null);
    assert.deepEqual(await inventory(selected), before, 'fresh reads must not initialize records/audit');
    const plaintext = Buffer.alloc(64 * 1024, 0x4d);
    const original = await encrypted(selected, plaintext);
    const first = await send(selected, '/internal/put', original);
    assert.equal(first.status, 200);
    const fact = await first.json(); metadataOnly(fact);
    assert.ok(Number(fact.stored.record_wire_bytes) <= 256 * 1024);
    const saved = await inventory(selected); assert.equal(saved.rows, 1); assert.equal(saved.audit, 1);
    const repeat = await send(selected, '/internal/put', JSON.stringify(original, null, 2));
    assert.equal(repeat.status, 200); assert.deepEqual(await repeat.json(), fact);
    assert.deepEqual(await inventory(selected), saved);
    const newlySealed = await encrypted(selected, plaintext);
    assert.equal(await consume(await send(selected, '/internal/put', newlySealed)), 409, 'same plaintext does not authorize replacing its envelope');
    assert.deepEqual(await inventory(selected), saved);
    const read = await send(selected, '/internal/read', selected.scope);
    assert.equal(read.status, 200); const record = await read.json(); assert.equal(recordDigest(record), recordDigest(original));
    assert.equal(digest(await decrypt(selected, record)), digest(plaintext));
  }, {name: 'max_prepared'});
});

test('actual ContextBody rejects physical aliases, wrong environment, AAD and unbounded input without changes', {timeout: 60000}, async () => {
  await withStore(async ({selected, send, inventory, publicFetch}) => {
    const before = await inventory(selected), original = await encrypted(selected, Buffer.from('synthetic opaque body'));
    const captured = entry('captured'), alternate = await encrypted(captured, Buffer.from('other slot'));
    assert.equal(await consume(await send(selected, '/internal/put', alternate)), 409, 'a valid alternate record must not initialize the wrong physical actor');
    const wrongAad = structuredClone(original); wrongAad.envelope.context.identity.key_id = 'a'.repeat(64);
    assert.equal(await consume(await send(selected, '/internal/put', wrongAad)), 409);
    assert.equal(await consume(await send(selected, '/internal/put', 'x'.repeat(256 * 1024 + 1))), 400);
    assert.equal(await consume(await send(selected, '/internal/put', {...original, verified: true})), 409);
    assert.equal(await consume(await publicFetch('https://custody.example/internal/put', {
      method: 'POST', headers: {'content-type': 'application/json'}, body: JSON.stringify(original),
    })), 404);
    assert.deepEqual(await inventory(selected), before);
  });
  for (const bindings of [{CUSTODY_CONTEXT_BODY_ENABLED: 'false'}, {CUSTODY_ROLE_KEY_ENVIRONMENT: 'wrong-environment'}]) {
    await withStore(async ({selected, send, inventory}) => {
      const before = await inventory(selected), candidate = await encrypted(selected, Buffer.from('synthetic'));
      assert.equal(await consume(await send(selected, '/internal/put', candidate)), 409);
      assert.deepEqual(await inventory(selected), before);
    }, {bindings});
  }
});

test('actual ContextBody SQLite UPDATE failure rolls back the empty marker and audit together', {timeout: 60000}, async () => {
  await withStore(async ({selected, send, inventory, mutate, faults}) => {
    await mutate(selected, 'fail_update'); const before = await inventory(selected);
    assert.equal(before.rows, 0); assert.equal(before.audit, 0);
    assert.equal(await faults(selected), 0);
    const candidate = await encrypted(selected, Buffer.from('synthetic body'));
    assert.equal(await consume(await send(selected, '/internal/put', candidate)), 409);
    assert.equal(await faults(selected), 1, 'the actual UPDATE must throw its SQLite CHECK error after audit insertion');
    assert.deepEqual(await inventory(selected), before, 'actual SQLite constraint failure must roll back all first-write changes');
  });
});

test('actual ContextBody refuses extra rows, unknown tables and noncanonical stored records', {timeout: 60000}, async () => {
  for (const mode of ['extra_row', 'unknown_table', 'noncanonical']) {
    await withStore(async ({selected, send, inventory, mutate}) => {
      const candidate = await encrypted(selected, Buffer.from('synthetic body'));
      assert.equal(await consume(await send(selected, '/internal/put', candidate)), 200);
      await mutate(selected, mode); const before = await inventory(selected);
      assert.equal(await consume(await send(selected, '/internal/read', selected.scope)), 409);
      assert.equal(await consume(await send(selected, '/internal/metadata', selected.scope)), 409);
      assert.equal(await consume(await send(selected, '/internal/put', candidate)), 409);
      assert.deepEqual(await inventory(selected), before);
    });
  }
});

test('actual ContextBody expired historical facts disclose neither ciphertext nor usable authority', {timeout: 60000}, async () => {
  await withStore(async ({selected, send, inventory, mutate}) => {
    const candidate = await encrypted(selected, Buffer.from('synthetic expired body'));
    assert.equal(await consume(await send(selected, '/internal/put', candidate)), 409);
    assert.equal(await consume(await send(selected, '/internal/read', selected.scope)), 409);
    // Explicit historical storage fixture, not a newly approved or SDK-valid context.
    await mutate(selected, 'historical_expired', JSON.stringify(candidate));
    const before = await inventory(selected), response = await send(selected, '/internal/metadata', selected.scope);
    assert.equal(response.status, 200); const fact = await response.json(); metadataOnly(fact); assert.ok(fact.stored);
    assert.equal(await consume(await send(selected, '/internal/read', selected.scope)), 409);
    assert.deepEqual(await inventory(selected), before);
  }, {name: 'expired_prepared'});
});

test('actual ContextBody preserves a committed envelope across a lost reply and restart', {timeout: 60000}, async () => {
  await withStore(async ({selected, send, inventory, restart}) => {
    const candidate = await encrypted(selected, Buffer.from('synthetic original body'));
    const lost = await send(selected, '/internal/put', candidate).then(async response => {
      await response.arrayBuffer(); return response.status === 500;
    }, () => true);
    assert.equal(lost, true, 'fixture drops only the actual successful store reply');
    await restart({FIXTURE_DROP_CONTEXT_REPLY: 'false'});
    const response = await send(selected, '/internal/read', selected.scope);
    assert.equal(response.status, 200); assert.equal(recordDigest(await response.json()), recordDigest(candidate));
    const saved = await inventory(selected); assert.equal(saved.rows, 1); assert.equal(saved.audit, 1);
    const fact = await (await send(selected, '/internal/metadata', selected.scope)).json(); metadataOnly(fact); assert.ok(fact.stored);
    assert.deepEqual(await inventory(selected), saved, 'unknown recovery reads do not write or re-seal');
  }, {bindings: {FIXTURE_DROP_CONTEXT_REPLY: 'true'}});
});
