import assert from 'node:assert/strict';
import test from 'node:test';
import {withRoleRuntime, persistedRuntime} from './fixtures/role_runtime_native_support.mjs';

test('actual manager UV, Account, three vaults and native SQLite establish one managed session without manager credentials', {timeout:120000}, async () => {
  await withRoleRuntime('native-chain', async runtime => {
    const progress=await runtime.drive('native-chain-one');
    assert.equal(progress.stage,'native_session_ready');
    const first=runtime.nativeRequests();
    for(const route of ['/machine-registration/complete','/custody/managed-bindings/complete','/native-key/complete']) {
      assert.equal(first[route],1,`exactly one original ${route}`);
    }
    await runtime.restart('native-chain');
    const retry=await runtime.drive('native-chain-one');
    assert.equal(retry.stage,'native_session_ready');
    const after=runtime.nativeRequests();
    for(const route of ['/machine-registration/complete','/custody/managed-bindings/complete','/native-key/complete']) {
      assert.equal(after[route],first[route],`restart never replays ${route}`);
    }
    await runtime.suspend();
    const saved=persistedRuntime(runtime.persist,runtime.account);
    assert.deepEqual(saved.native_results.sort(),['binding_receipt','registration_challenge','session_status']);
    assert.equal(new Set(saved.slots.map(v=>v.public_key)).size,3);
    assert.ok(saved.cipher_records>=6,'private originals and SID/CSRF remain encrypted');
  });
});
