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

async function provisionInput(mf, name, path = 'prepare') {
  const result = await ok(mf, name, '/provision/' + path);
  result.input.approval.signature = await sign(result.canonical_bytes);
  return result.input;
}
test('role-local encrypted key slot commits once, survives restart and authenticates the original material', { timeout: 90000 }, async () => fixture(async (current, restart) => {
  let mf = current(); const name = 'role-slot-original';
  const results = await Promise.all(Array.from({length: 3}, () => ok(mf, name, '/role-key/ensure')));
  assert.deepEqual(results[0], results[1]); assert.deepEqual(results[0], results[2]);
  assert.deepEqual(Object.keys(results[0]).sort(), ['key_id', 'key_version', 'public_key']);
  assert.match(results[0].public_key, /^[0-9a-f]{64}$/);
  const raw = await ok(mf, name, '/role-key/raw');
  const saved = JSON.parse(raw[0].payload);
  assert.equal(saved.audit_sequence, '1');
  assert.equal(saved.slot.envelope.ciphertext.length, 96, '32-byte seed plus GCM tag only');
  assert.equal(saved.slot.envelope.context.identity.role, 'owner');
  assert.deepEqual(await ok(mf, name, '/role-key/counts'), {slots: 1, audits: 1});
  const audit = await ok(mf, name, '/role-key/audit');
  assert.doesNotMatch(JSON.stringify(audit), /ciphertext|wrapped_dek|seed|signature|kek/i);
  const proof = await ok(mf, name, '/role-key/verify-material');
  assert.equal(proof.public_key, results[0].public_key);
  const publicKey = await webcrypto.subtle.importKey('raw', Buffer.from(proof.public_key, 'hex'), 'Ed25519', false, ['verify']);
  assert.equal(await webcrypto.subtle.verify('Ed25519', publicKey, Buffer.from(proof.signature, 'hex'), Uint8Array.from(proof.message)), true);
  mf = await restart();
  assert.deepEqual(await ok(mf, name, '/role-key/ensure'), results[0]);
  assert.deepEqual(await ok(mf, name, '/role-key/raw'), raw);
  assert.deepEqual(await ok(mf, name, '/role-key/audit'), audit);
  assert.equal((await call(mf, name, '/role-key/ensure-scope')).status, 409);
  assert.equal((await call(mf, name, '/role-key/ensure-role')).status, 409);
  assert.equal((await call(mf, name, '/role-key/ensure-wrong-kek')).status, 409);
  assert.deepEqual(await ok(mf, name, '/role-key/raw'), raw, 'failure cannot replace the original');
}));
test('role slot audit failure rolls back candidate; final authority changes cannot publish a key', { timeout: 90000 }, async () => fixture(async current => {
  const mf = current();
  for (const suffix of ['expired', 'frozen', 'cancelled', 'revision', 'unapproved', 'clock']) {
    const name = 'role-final-' + suffix;
    assert.equal((await call(mf, name, '/role-key/ensure-' + suffix)).status, 409);
    assert.deepEqual(await ok(mf, name, '/role-key/counts'), {slots: 0, audits: 0});
  }
  const name = 'role-audit-rollback';
  await ok(mf, name, '/role-key/fault');
  assert.equal((await call(mf, name, '/role-key/ensure')).status, 409);
  assert.deepEqual(await ok(mf, name, '/role-key/counts'), {slots: 0, audits: 0});
  await ok(mf, name, '/role-key/clear-fault');
  await ok(mf, name, '/role-key/ensure');
  const original = await ok(mf, name, '/role-key/raw');
  for (const suffix of ['frozen', 'cancelled', 'revision', 'expired']) {
    assert.equal((await call(mf, name, '/role-key/ensure-' + suffix)).status, 409);
    assert.deepEqual(await ok(mf, name, '/role-key/raw'), original);
  }
}));
test('damaged role slot ciphertext, public key or schema fails closed without replacement', { timeout: 90000 }, async () => fixture(async (current, restart) => {
  let mf = current();
  for (const field of ['public', 'ciphertext', 'schema']) {
    const name = 'role-corrupt-' + field;
    await ok(mf, name, '/role-key/ensure');
    await ok(mf, name, '/role-key/corrupt-' + field);
    const corrupt = await ok(mf, name, '/role-key/raw');
    mf = await restart();
    assert.equal((await call(mf, name, '/role-key/ensure')).status, 409);
    assert.deepEqual(await ok(mf, name, '/role-key/raw'), corrupt);
    assert.deepEqual(await ok(mf, name, '/role-key/counts'), {slots: 1, audits: 1});
  }
}));
async function provisionVerification(mf, name) {
  const result = await ok(mf, name, '/provision/verification');
  return { verification: result.verification, signature: await sign(result.canonical_bytes) };
}
test('provision proof consumption, approved plan and three fixed tasks commit once in actual Account SQLite', { timeout: 90000 }, async () => fixture(async (current, restart) => {
  let mf = current(); const name = 'provision-atomic';
  const input = await provisionInput(mf, name);
  const [pending, concurrent] = await Promise.all([ok(mf, name, '/provision/begin', input), ok(mf, name, '/provision/begin', input)]);
  assert.deepEqual(concurrent, pending, 'concurrent same-body begin keeps one stored nonce');
  assert.deepEqual(await ok(mf, name, '/provision/begin', input), pending);
  const signed = await provisionVerification(mf, name);
  const results = await Promise.all([ok(mf, name, '/provision/commit', signed), ok(mf, name, '/provision/commit', signed)]);
  assert.deepEqual(results[0], results[1]);
  const saved = await ok(mf, name, '/provision/snapshot');
  assert.equal(saved.audit_sequence, '2');
  assert.equal(saved.state.provision.stage, 'approved');
  assert.equal(saved.state.accepted_operation_id, 'operation-one');
  assert.deepEqual(Object.keys(saved.state.role_key_tasks).sort(), ['executor', 'identity', 'owner']);
  assert.ok(Object.values(saved.state.role_key_tasks).every(task => task.state === 'pending' && task.key_version === '1'));
  const audit = await ok(mf, name, '/provision/audit');
  assert.equal(audit.length, 3, 'one draft, one pending and one approved audit');
  assert.doesNotMatch(JSON.stringify(audit), /signature|verify_nonce|native_session_ref/);
  mf = await restart();
  assert.deepEqual(await ok(mf, name, '/provision/snapshot'), saved);
  assert.deepEqual(await ok(mf, name, '/provision/commit', signed), results[0]);
  assert.deepEqual(await ok(mf, name, '/provision/audit'), audit);
  const cancelled = await ok(mf, name, '/provision/cancel');
  assert.ok(Object.values(cancelled.state.role_key_tasks).every(task => task.state === 'cancelled'));
  await ok(mf, name, '/provision/commit', signed);
  assert.deepEqual(await ok(mf, name, '/provision/snapshot'), cancelled, 'historical retry cannot restore cancelled work');
}));
test('real provision SQLite audit failure leaves proof pending, plan draft and outbox empty', { timeout: 90000 }, async () => fixture(async current => {
  const mf = current(); const name = 'provision-rollback'; const input = await provisionInput(mf, name);
  await ok(mf, name, '/provision/begin', input);
  const signed = await provisionVerification(mf, name);
  const before = await ok(mf, name, '/provision/snapshot');
  const audit = await ok(mf, name, '/provision/audit');
  await ok(mf, name, '/provision/fault');
  assert.equal((await call(mf, name, '/provision/commit', signed)).status, 409);
  assert.deepEqual(await ok(mf, name, '/provision/snapshot'), before);
  assert.deepEqual(await ok(mf, name, '/provision/audit'), audit);
  assert.equal(before.state.provision.stage, 'draft');
  assert.deepEqual(before.state.role_key_tasks, {});
  await ok(mf, name, '/provision/clear-fault');
  await ok(mf, name, '/provision/commit', signed);
  assert.equal((await ok(mf, name, '/provision/snapshot')).state.provision.stage, 'approved');
}));
test('Account commits three exact role results atomically, rejects replacement and retains history after cancellation', { timeout: 90000 }, async () => fixture(async (current, restart) => {
  let mf = current(); const name = 'provision-three-role-results';
  const input = await provisionInput(mf,name); await ok(mf,name,'/provision/begin',input);
  await ok(mf,name,'/provision/commit',await provisionVerification(mf,name));
  const before = await ok(mf,name,'/provision/snapshot');
  await ok(mf,name,'/provision/role-fault');
  const identity = {role:'identity',public_key:'12'.repeat(32)};
  assert.equal((await call(mf,name,'/provision/role-result',identity)).status,409);
  assert.deepEqual(await ok(mf,name,'/provision/snapshot'),before,'role receipt and stage must roll back with audit');
  await ok(mf,name,'/provision/clear-fault');
  const first = await ok(mf,name,'/provision/role-result',identity);
  assert.deepEqual(await ok(mf,name,'/provision/role-result',identity),first);
  assert.equal((await call(mf,name,'/provision/role-result',{...identity,public_key:'13'.repeat(32)})).status,409);
  assert.equal((await call(mf,name,'/provision/role-result',{role:'owner',public_key:identity.public_key})).status,409,'distinct roles cannot reuse a public key');
  await ok(mf,name,'/provision/role-result',{role:'owner',public_key:'14'.repeat(32)});
  assert.equal((await ok(mf,name,'/provision/snapshot')).state.provision.stage,'approved');
  await ok(mf,name,'/provision/role-result',{role:'executor',public_key:'15'.repeat(32)});
  const ready = await ok(mf,name,'/provision/snapshot'); assert.equal(ready.state.provision.stage,'role_keys_ready');
  assert.equal(ready.audit_sequence,'5');
  mf = await restart(); assert.deepEqual(await ok(mf,name,'/provision/snapshot'),ready);
  const cancelled = await ok(mf,name,'/provision/cancel');
  assert.deepEqual(await ok(mf,name,'/provision/role-result',identity),first,'same historical receipt does not gain new authority');
  assert.deepEqual(await ok(mf,name,'/provision/snapshot'),cancelled);
}));
test('provision admission binds signed purpose and live policy; real freeze blocks final consumption', { timeout: 90000 }, async () => fixture(async current => {
  const mf = current();
  assert.equal((await call(mf, 'missing', '/provision/missing-draft')).status, 409);
  const wrongPurpose = await provisionInput(mf, 'purpose', 'prepare-purpose');
  assert.equal((await call(mf, 'purpose', '/provision/begin', wrongPurpose)).status, 409,
    'a valid signature on another purpose cannot approve this draft');
  const name = 'provision-freeze'; const input = await provisionInput(mf, name);
  const empty = await ok(mf, name, '/provision/snapshot');
  for (const path of ['begin-expired', 'begin-frozen', 'begin-policy', 'begin-allowlist', 'begin-clock']) {
    assert.equal((await call(mf, name, '/provision/' + path, input)).status, 409);
    assert.deepEqual(await ok(mf, name, '/provision/snapshot'), empty);
  }
  await ok(mf, name, '/provision/begin', input);
  const signed = await provisionVerification(mf, name);
  const pending = await ok(mf, name, '/provision/snapshot');
  for (const path of ['commit-expired', 'commit-frozen', 'commit-policy', 'commit-allowlist', 'commit-clock']) {
    assert.equal((await call(mf, name, '/provision/' + path, signed)).status, 409);
    assert.deepEqual(await ok(mf, name, '/provision/snapshot'), pending);
  }
  const freeze = await ok(mf, name, '/provision/freeze');
  assert.notEqual(freeze.status, 200, 'new inventory is still unknown to the production closer');
  assert.equal((await call(mf, name, '/provision/commit', signed)).status, 409);
  assert.deepEqual(await ok(mf, name, '/provision/snapshot'), pending);
}));

test('actual Rust WebCrypto envelope authenticates identity and both layers; rewrap preserves protocol material', { timeout: 90000 }, async () => fixture(async current => {
  const mf = current(); const name = 'envelope';
  const identity = { environment: 'staging', account_id: 'synthetic-account', role: 'executor',
    key_id: 'synthetic-protocol-key', key_version: '18446744073709551615' };
  const plaintext = [...Buffer.from('公開 fixture / private key bytes \u0000', 'utf8')];
  const invoke = (path, body) => ok(mf, name, '/envelope/' + path, { identity, ...body });
  const first = (await invoke('seal', { plaintext })).envelope;
  const second = (await invoke('seal', { plaintext })).envelope;
  assert.notEqual(first.ciphertext, second.ciphertext);
  assert.notEqual(first.value_nonce, first.wrap_nonce);
  assert.notEqual(first.value_nonce, second.value_nonce);
  assert.deepEqual((await invoke('open', { envelope: first })).plaintext, plaintext);
  assert.doesNotMatch(JSON.stringify(first), /公開 fixture|private key bytes/);
  const reject = async (envelope, changedIdentity = identity, key = 1) => {
    assert.equal((await call(mf, name, '/envelope/open', { identity: changedIdentity, envelope, key })).status, 409);
  };
  await reject(first, identity, 2);
  for (const [field, value] of [['environment', 'other'], ['account_id', 'other'], ['role', 'owner'],
    ['key_id', 'other'], ['key_version', '1']]) {
    // Supplying a different expected identity must fail even with an untouched envelope.
    await reject(first, { ...identity, [field]: value });
    const changed = structuredClone(first); changed.context.identity[field] = value;
    await reject(changed, changed.context.identity);
  }
  for (const field of ['ciphertext', 'wrapped_dek', 'value_nonce', 'wrap_nonce']) {
    const changed = structuredClone(first);
    changed[field] = (changed[field][0] === '0' ? '1' : '0') + changed[field].slice(1);
    await reject(changed);
  }
  for (const [field, value] of [['schema_version', 2], ['algorithm', 'unknown']]) {
    await reject({ ...first, context: { ...first.context, [field]: value } });
  }
  await reject({ ...first, wrapping_key_version: '2' });
  await reject({ ...first, ciphertext: 'A' + first.ciphertext.slice(1) });
  await reject({ ...first, wrap_nonce: first.value_nonce });
  const updated = (await invoke('rewrap', { envelope: first })).envelope;
  assert.equal(updated.ciphertext, first.ciphertext);
  assert.equal(updated.value_nonce, first.value_nonce);
  assert.deepEqual(updated.context, first.context);
  assert.equal(updated.wrapping_key_version, '2');
  assert.notEqual(updated.wrapped_dek, first.wrapped_dek);
  assert.deepEqual((await invoke('open', { envelope: updated, key: 2 })).plaintext, plaintext);
  await reject(updated);
  assert.equal((await call(mf, name, '/envelope/rewrap', { identity, envelope: updated, key: 2 })).status, 409,
    'same wrapping version cannot silently replace a KEK slot');
}));

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
