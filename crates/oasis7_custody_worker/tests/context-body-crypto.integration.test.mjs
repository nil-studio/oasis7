import assert from 'node:assert/strict';
import test from 'node:test';
import {createHash, randomBytes, webcrypto} from 'node:crypto';
import {readFileSync} from 'node:fs';
import {dirname, resolve} from 'node:path';
import {fileURLToPath} from 'node:url';
import {Miniflare, convertV4MiniflareOptions} from 'miniflare';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const fixture = JSON.parse(readFileSync(resolve(root, 'artifacts/context-body-fixture.json'), 'utf8'));
assert.equal(fixture.label, 'synthetic-storage-scope-not-authority');
const selected = name => fixture.entries.find(value => value.name === name);
const key = Buffer.alloc(32, 0x6d);
const digest = bytes => createHash('sha256').update(bytes).digest('hex');
async function aes(raw, iv, aad, bytes) {
  const imported = await webcrypto.subtle.importKey('raw', raw, 'AES-GCM', false, ['encrypt']);
  return Buffer.from(await webcrypto.subtle.encrypt({name: 'AES-GCM', iv, additionalData: aad, tagLength: 128}, imported, bytes));
}
async function sealed(value, plaintext) {
  const dek = randomBytes(32), nonce = randomBytes(12), wrap = randomBytes(12);
  assert.notDeepEqual(nonce, wrap);
  return {scope: JSON.parse(value.scope_wire), envelope: {
    context: JSON.parse(value.envelope_context_wire),
    ciphertext: (await aes(dek, nonce, Buffer.from(value.value_aad_hex, 'hex'), plaintext)).toString('hex'),
    value_nonce: nonce.toString('hex'),
    wrapped_dek: (await aes(key, wrap, Buffer.from(value.wrap_aad_hex, 'hex'), dek)).toString('hex'),
    wrap_nonce: wrap.toString('hex'), wrapping_key_version: '1',
  }};
}
async function opened(value, record, overrides = {}, extra = {}) {
  const mf = new Miniflare(convertV4MiniflareOptions({
    name: 'context-crypto-fixture', modulesRoot: root, compatibilityDate: '2026-10-25', cf: false,
    modules: [
      {type: 'ESModule', path: resolve(root, 'artifacts/native-harness/entry.mjs')},
      {type: 'ESModule', path: resolve(root, 'artifacts/native-harness/index.js')},
      {type: 'CompiledWasm', path: resolve(root, 'artifacts/native-harness/index_bg.wasm')},
    ],
    durableObjects: {FIXTURE: {className: 'ContextBodyCryptoHarness', useSQLite: true}},
    bindings: {
      CUSTODY_CONTEXT_BODY_ENABLED: 'true', CUSTODY_CONTEXT_BODY_KEK: key.toString('hex'),
      CUSTODY_CONTEXT_BODY_WRAPPING_KEY_VERSION: '1',
      CUSTODY_ROLE_KEY_ENVIRONMENT: value.scope.identity_scope.context.identity.environment,
      ...overrides,
    },
  }));
  try {
    const {FIXTURE} = await mf.getBindings();
    const response = await FIXTURE.get(FIXTURE.idFromName('fixed-synthetic-crypto')).fetch('https://fixture.invalid/open', {
      method: 'POST', headers: {'content-type': 'application/json'}, body: JSON.stringify({scope: value.scope, record, ...extra}),
    });
    return {status: response.status, value: response.status === 200 ? await response.json() : (await response.arrayBuffer(), null)};
  } finally { await mf.dispose(); }
}
test('actual Rust ContextBody crypto opens complete synthetic 64KiB with independent configured KEK', {timeout: 60000}, async () => {
  const value = selected('max_prepared'), plaintext = Buffer.alloc(64 * 1024, 0x4d);
  const record = await sealed(value, plaintext);
  assert.deepEqual(await opened(value, record), {status: 200, value: {bytes: plaintext.length, digest: digest(plaintext)}});
});
test('actual Rust ContextBody crypto rejects wrong keys, scope, version, expired body and disabled configuration', {timeout: 120000}, async () => {
  const value = selected('prepared'), record = await sealed(value, Buffer.from('synthetic full artifact bytes'));
  for (const overrides of [
    {CUSTODY_CONTEXT_BODY_KEK: '', CUSTODY_ROLE_KEY_KEK: key.toString('hex'), CUSTODY_NATIVE_PRIVATE_KEK: key.toString('hex')},
    {CUSTODY_CONTEXT_BODY_KEK: '7a'.repeat(32)},
    {CUSTODY_CONTEXT_BODY_WRAPPING_KEY_VERSION: '2'}, {CUSTODY_CONTEXT_BODY_WRAPPING_KEY_VERSION: '0'},
    {CUSTODY_ROLE_KEY_ENVIRONMENT: 'wrong'}, {CUSTODY_CONTEXT_BODY_ENABLED: 'false'},
  ]) assert.equal((await opened(value, record, overrides)).status, 409);
  const corrupted = structuredClone(record);
  corrupted.envelope.ciphertext = (corrupted.envelope.ciphertext.startsWith('00') ? '01' : '00') + corrupted.envelope.ciphertext.slice(2);
  assert.equal((await opened(value, corrupted)).status, 409);
  assert.equal((await opened(value, record, {}, {verified: true})).status, 409);
  assert.equal((await opened(selected('captured'), record)).status, 409);
  const expired = selected('expired_prepared');
  assert.equal((await opened(expired, await sealed(expired, Buffer.from('historical bytes')))).status, 409);
});
