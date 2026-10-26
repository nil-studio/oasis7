import assert from 'node:assert/strict';
import { test } from 'node:test';
import { webcrypto } from 'node:crypto';
import { mkdtempSync, rmSync, readFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { Miniflare, convertV4MiniflareOptions } from 'miniflare';
const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
// Public interoperability vector, never an operator seed or credential.
const seed = Buffer.alloc(32, 41);
const key = await webcrypto.subtle.importKey('pkcs8', Buffer.concat([
  Buffer.from('302e020100300506032b657004220420', 'hex'), seed,
]), 'Ed25519', false, ['sign']);
async function sign(bytes) {
  return Buffer.from(await webcrypto.subtle.sign('Ed25519', key, Uint8Array.from(bytes))).toString('base64url');
}
function options(persist) {
  const modules = [
    { type: 'ESModule', path: resolve(root, 'tests/fixtures/approval_harness.mjs') },
    { type: 'ESModule', path: resolve(root, 'artifacts/approval-harness/index.js') },
    { type: 'CompiledWasm', path: resolve(root, 'artifacts/approval-harness/index_bg.wasm') },
  ];
  return { ...convertV4MiniflareOptions({ name: 'custody-approval-fixture', modules, modulesRoot: root,
    compatibilityDate: '2026-10-25', cf: false,
    durableObjects: { STORE: { className: 'TestApprovalAuthority', useSQLite: true } },
  }), resourcePersistencePath: persist, unsafeDevRegistryPath: resolve(persist, 'registry') };
}
async function call(mf, name, path, body) {
  const { STORE } = await mf.getBindings();
  return STORE.get(STORE.idFromName(name)).fetch('https://fixture.test' + path,
    { method: 'POST', ...(body === undefined ? {} : { body: JSON.stringify(body) }) });
}
async function ok(mf, name, path, body) {
  const response = await call(mf, name, path, body);
  assert.equal(response.status, 200, path + ': ' + (response.status === 200 ? '' : await response.text()));
  return response.json();
}
async function prepare(mf, name) {
  const prepared = await ok(mf, name, '/prepare');
  prepared.input.approval.signature = await sign(prepared.canonical_bytes);
  return prepared.input;
}
async function verification(mf, name) {
  const prepared = await ok(mf, name, '/verification');
  return { verification: prepared.verification, signature: await sign(prepared.canonical_bytes) };
}
async function fixture(run) {
  const persist = mkdtempSync(resolve(tmpdir(), 'oasis7-custody-approval-'));
  let mf = new Miniflare(options(persist));
  const restart = async () => { await mf.dispose(); mf = new Miniflare(options(persist)); return mf; };
  try { await run(() => mf, restart); }
  finally { await mf.dispose(); rmSync(persist, { recursive: true, force: true }); }
}

test('Owner actual Wasm SQLite keeps original nonce, one-time consumption and full-width values after restart', { timeout: 90000 }, async () => fixture(async (current, restart) => {
  let mf = current();
  const input = await prepare(mf, 'original');
  const begun = await ok(mf, 'original', '/begin', input);
  assert.match(begun.verify_nonce, /^[0-9a-f]{64}$/);
  assert.deepEqual(await ok(mf, 'original', '/begin', input), begun);
  const snapshot = await ok(mf, 'original', '/snapshot');
  assert.equal(snapshot.context.source.origin_credential_version, '9007199254740993');
  assert.equal(snapshot.context.source.passkey_credential_version, '18446744073709551615');
  assert.equal(snapshot.audit_sequence, '1');
  mf = await restart();
  assert.deepEqual(await ok(mf, 'original', '/snapshot'), snapshot);
  const signed = await verification(mf, 'original');
  assert.equal(signed.verification.verify_nonce, begun.verify_nonce);
  const consumed = await ok(mf, 'original', '/commit', signed);
  assert.match(consumed.consumed_at, /^[0-9]+$/);
  assert.deepEqual(consumed.verification, signed);
  assert.deepEqual(await ok(mf, 'original', '/commit', signed), consumed);
  const audit = await ok(mf, 'original', '/audit');
  assert.equal(audit.length, 2);
  assert.deepEqual(audit.map(row => row.sequence), ['1', '2']);
  assert.doesNotMatch(JSON.stringify(audit), /signature|manager_session_ref|verify_nonce/);
  mf = await restart();
  assert.deepEqual(await ok(mf, 'original', '/commit', signed), consumed);
  const other = structuredClone(input); other.operation_id = 'operation-two';
  assert.equal((await call(mf, 'original', '/begin', other)).status, 409);
  const conflict = structuredClone(input); conflict.request.approval_request_id = 'changed';
  assert.equal((await call(mf, 'original', '/begin', conflict)).status, 409);
  assert.equal((await ok(mf, 'original', '/audit')).length, 2);
}));

test('same-original concurrent begin/commit serializes one nonce and exactly two audit events', { timeout: 90000 }, async () => fixture(async current => {
  const mf = current(); const name = 'concurrent'; const input = await prepare(mf, name);
  const begins = await Promise.all([ok(mf, name, '/begin', input), ok(mf, name, '/begin', input)]);
  assert.deepEqual(begins[0], begins[1]);
  assert.equal((await ok(mf, name, '/audit')).length, 1);
  const signed = await verification(mf, name);
  const commits = await Promise.all([ok(mf, name, '/commit', signed), ok(mf, name, '/commit', signed)]);
  assert.deepEqual(commits[0], commits[1]);
  assert.equal((await ok(mf, name, '/audit')).length, 2);
}));

test('signature rejection, final-context expiry/freeze and audit/state rollback leave no admission changes', { timeout: 90000 }, async () => fixture(async current => {
  const mf = current(); const name = 'rollback'; const input = await prepare(mf, name);
  const empty = await ok(mf, name, '/snapshot');
  const forged = structuredClone(input); forged.approval.signature = 'A'.repeat(86);
  assert.equal((await call(mf, name, '/begin', forged)).status, 409);
  for (const path of ['/begin-expired', '/begin-frozen', '/begin-pin', '/begin-clock']) {
    assert.equal((await call(mf, name, path, input)).status, 409);
    assert.deepEqual(await ok(mf, name, '/snapshot'), empty);
  }
  assert.deepEqual(await ok(mf, name, '/audit'), []);
  // No successful transition has created the audit table.
  await ok(mf, name, '/fault');
  assert.equal((await call(mf, name, '/begin', input)).status, 409);
  assert.deepEqual(await ok(mf, name, '/snapshot'), empty);
  assert.deepEqual(await ok(mf, name, '/audit'), []);
  await ok(mf, name, '/clear-fault');
  await ok(mf, name, '/begin', input);
  assert.equal((await ok(mf, name, '/audit')).length, 1);
  const pending = await ok(mf, name, '/snapshot');
  const signed = await verification(mf, name);
  for (const path of ['/commit-expired', '/commit-frozen', '/commit-pin', '/commit-clock']) {
    assert.equal((await call(mf, name, path, signed)).status, 409);
    assert.deepEqual(await ok(mf, name, '/snapshot'), pending);
  }
  const tampered = structuredClone(signed); tampered.verification.verify_nonce = 'wrong';
  assert.equal((await call(mf, name, '/commit', tampered)).status, 409);
  await ok(mf, name, '/fault');
  assert.equal((await call(mf, name, '/commit', signed)).status, 409);
  assert.deepEqual(await ok(mf, name, '/snapshot'), pending);
  assert.equal((await ok(mf, name, '/audit')).length, 1);
  await ok(mf, name, '/clear-fault');
  await ok(mf, name, '/commit', signed);
  assert.equal((await ok(mf, name, '/audit')).length, 2);
}));

test('real strict Ed25519 plus WebCrypto reject tampering, weak points and noncanonical wire encodings', { timeout: 90000 }, async () => fixture(async current => {
  const mf = current(); const name = 'crypto';
  const rustFixture = JSON.parse(readFileSync(resolve(root, 'artifacts/approval-fixture.json'), 'utf8'));
  const rustChecks = rustFixture.sample_signature_checks.checks;
  assert.equal(rustChecks.length, 2);
  for (const check of rustChecks) {
    assert.equal((await ok(mf, name, '/crypto', {
      key: check.public_key_hex, signature: check.signature_base64url, bytes: check.canonical_bytes,
    })).valid, true, 'Rust signature must pass strict Rust and Worker WebCrypto');
  }
  assert.equal((await ok(mf, name, '/crypto', {
    key: rustChecks[0].public_key_hex, signature: rustChecks[0].signature_base64url,
    bytes: rustChecks[1].canonical_bytes,
  })).valid, false, 'proof and verification signing domains must remain distinct');
  const prepared = await ok(mf, name, '/prepare');
  const bytes = prepared.canonical_bytes;
  const signature = await sign(bytes);
  // Pin comes from Rust's fixture authority; no signing-image reconstruction in JS.
  const snapshot = await ok(mf, name, '/snapshot');
  const pin = snapshot.context.native_key_pins['native-one'];
  const valid = { key: pin, bytes, signature };
  assert.equal((await ok(mf, name, '/crypto', valid)).valid, true);
  const changed = [...bytes]; changed[0] ^= 1;
  const alphabet = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_';
  const noncanonical = signature.slice(0, -1) + alphabet[alphabet.indexOf(signature.at(-1)) + 1];
  const weak = '01' + '00'.repeat(31);
  const weakSignature = Buffer.concat([Buffer.from(weak, 'hex'), Buffer.alloc(32)]).toString('base64url');
  for (const vector of [
    { ...valid, bytes: changed }, { ...valid, key: '00'.repeat(32) },
    { ...valid, key: weak, signature: weakSignature }, { ...valid, signature: signature + '=' }, { ...valid, signature: noncanonical },
    { ...valid, key: pin.toUpperCase() }, { ...valid, key: '02'.repeat(32) },
  ]) assert.equal((await ok(mf, name, '/crypto', vector)).valid, false);
}));
