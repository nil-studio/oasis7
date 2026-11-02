// Actual Rust receiver/Wasm + actual Miniflare SQLite, fixture Account key and
// complete Core Read-purpose signature. Not a paid Account/CF/Read300 proof.
import test from 'node:test';
import assert from 'node:assert/strict';
import {createPrivateKey, createPublicKey, createHash, sign} from 'node:crypto';
import {mkdtemp, rm} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {resolve, dirname} from 'node:path';
import {fileURLToPath} from 'node:url';
import {Miniflare, convertV4MiniflareOptions} from 'miniflare';
import {fixtureRecoveryPolicy, encodeFixtureRecoveryCbor as cbor} from './fixtures/fence_activation_signer.mjs';
const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const key = createPrivateKey({key: Buffer.concat([Buffer.from('302e020100300506032b657004220420', 'hex'), Buffer.alloc(32, 113)]), format: 'der', type: 'pkcs8'});
const issuer = {namespace: 'account-fixture', service_id: 'account-service', environment: 'test', key_id: 'read-rf-fixture', issuer_epoch: '1', config_generation: '1', native_audience_origin: 'https://native.fixture.invalid'};
const pin = {...issuer, public_key: createPublicKey(key).export({type: 'spki', format: 'der'}).subarray(-32).toString('hex'), not_before: '1', not_after: String(Date.now()+3600000), allowed_purpose: 'read_fence_read_v3'};
const verifier = {pin, floor: {issuer_epoch: '1', config_generation: '1'}};
const query = {api_version: 3, expected_service_epoch: '1'};
function original(actual) {
  const now = Date.now();
  const binding = {api_version: 3, ingress_attempt_id: 'fixture-read-ingress', original_digest: '11'.repeat(32), source_nonce: '22'.repeat(32), root_read_deadline: String(now+60000), source_until: String(now+4000), request_until: String(now+4000), issuer,
    scope: {provision_id: 'fixture-provision', operation_id: 'fixture-identity-operation', context: {schema_version: 1, identity: {environment: 'test', account_id: 'fixture-account', role: 'identity', key_id: 'fixture-identity-role', key_version: '1'}, algorithm: 'aes_256_gcm'}, algorithm: 'ed25519'},
    session_id: 'fixture-session', approved_input_digest: '33'.repeat(32), read_graph_digest: '44'.repeat(32), source_operation_id: 'fixture-read-source', reservation_ordinal: '10', dispatcher_claim_ordinal: '11', phase: 'claim_source', dispatcher_attempt_id: 'fixture-dispatch-attempt', fence_attempt_id: 'fixture-fence-attempt', dispatcher_object_id: '55'.repeat(32), fence_object_id: actual, expected_service_epoch: '1', business_body_digest: createHash('sha256').update(JSON.stringify(query)).digest('hex'), issued_at: String(now-2), expires_at: String(now+4000)};
  const claim = {binding, dispatch_digest: '66'.repeat(32), claim_ordinal: '12', claimed_at: String(now-1)};
  return {query, authorization: {claim, signature_hex: sign(null, cbor(['oasis7.custody.read-fence-read.v3', claim]), key).toString('hex')}};
}
test('actual noncanonical FENCES object rejects a complete strong Read proof before any admission SQL', {timeout:120000}, async () => {
  const persist = await mkdtemp(resolve(tmpdir(), 'oasis7-read-fence-canonical-'));
  const options = () => ({...convertV4MiniflareOptions({name: 'read-fence-canonical', modulesRoot: root, compatibilityDate: '2026-10-25', cf: false, modules: [
    {type: 'ESModule', path: resolve(root, 'tests/fixtures/read_fence_canonical_receiver_actor_v3.mjs')},
    {type: 'ESModule', path: resolve(root, 'artifacts/recovery-fence/entry.mjs')},
    {type: 'ESModule', path: resolve(root, 'artifacts/recovery-fence/index.js')},
    {type: 'CompiledWasm', path: resolve(root, 'artifacts/recovery-fence/index_bg.wasm')},
  ], durableObjects: {FENCES: {className: 'RecoveryFence', useSQLite: true}}, bindings: {CUSTODY_INITIAL_SERVICE_EPOCH: '1', CUSTODY_RECOVERY_CONTROL_ENABLED: 'true', CUSTODY_RECOVERY_AUTHORITY_POLICY: JSON.stringify(fixtureRecoveryPolicy), CUSTODY_READ_FENCE_ADMISSION_ENABLED: 'true', CUSTODY_READ_FENCE_ADMISSION_CAPACITY: '2', CUSTODY_READ_FENCE_ADMISSION_VERIFIER_HISTORY: JSON.stringify([verifier]), CUSTODY_READ_FENCE_ADMISSION_ACTIVE_VERIFIER: JSON.stringify(verifier)}}), resourcePersistencePath: persist, unsafeDevRegistryPath: resolve(persist, 'registry')});
  let mf = new Miniflare(options());
  const call = (path, name, body) => mf.dispatchFetch(`https://component.invalid/${path}?name=${name}`, {method: 'POST', ...(body ? {headers: {'content-type': 'application/json'}, body: JSON.stringify(body)} : {})});
  try {
    const canonical = 'service-fence-v1', other = 'fixture-noncanonical-fence-v3';
    for (const name of [canonical, other]) {
      const legacy = await call('legacy', name); assert.equal(legacy.status, 200);
      assert.deepEqual(await legacy.json(), {epoch: '1', disabled: true});
    }
    const canonicalId = (await (await call('identity', canonical)).json()).object_id;
    const otherId = (await (await call('identity', other)).json()).object_id;
    assert.notEqual(canonicalId, otherId);
    // Positive precondition: actual Rust accepted the complete signature and
    // committed a paid once, then denied disabled Current. Not a setup RED.
    assert.equal((await call('paid', canonical, original(canonicalId))).status, 409);
    const positive = await (await call('observe', canonical)).json();
    assert.ok(positive.paid_sql > 0, 'actual canonical strong proof must reach admission SQL');
    assert.equal(positive.once, 1, 'canonical signature is genuinely accepted before disabled-FENCE denial');
    assert.equal((await call('legacy', canonical)).status, 200, 'ordinary legacy reader remains usable with strong archived birth');
    const before = await (await call('observe', other)).json(); assert.equal(before.once, 0);
    assert.equal((await call('paid', other, original(otherId))).status, 409);
    const after = await (await call('observe', other)).json();
    assert.equal(after.paid_sql - before.paid_sql, 0, 'noncanonical object must reject before the first admission SQL');
    assert.equal(after.once, 0, 'receiver must not create paid inventory on the wrong fixed object');
    assert.equal((await call('legacy', other)).status, 200, 'ordinary legacy behavior is preserved');
    await mf.dispose(); mf = new Miniflare(options());
    for (const name of [canonical, other]) assert.equal((await call('legacy', name)).status, 200, 'cold ordinary reader keeps existing semantics');
  } finally {await mf.dispose(); await rm(persist, {recursive: true, force: true});}
});
