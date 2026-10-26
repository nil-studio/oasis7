import assert from 'node:assert/strict';
import test from 'node:test';
import {setTimeout as delay} from 'node:timers/promises';
import {withRoleRuntime, persistedRuntime} from './fixtures/role_runtime_native_support.mjs';
test('actual independently activated Fence admits the full native chain without giving Owner its control capability',{timeout:120000},async()=>{
  await withRoleRuntime('native-real-fence',async runtime=>{
    assert.equal((await runtime.drive('native-real-fence-one')).stage,'native_session_ready');
    assert.equal(runtime.nativeRequests()['/native-key/complete'],1);
    await runtime.restart('native-real-fence');
    assert.equal((await runtime.drive('native-real-fence-one')).stage,'native_session_ready');
    assert.equal(runtime.nativeRequests()['/native-key/complete'],1);
  });
});

test('persisted Identity status survives Account submission loss without another login', {timeout:120000}, async () => {
  await withRoleRuntime('native-lost-account-result', async runtime => {
    const pending=await runtime.drive('native-account-result-one');
    assert.equal(pending.stage,'native_registered');
    assert.equal(runtime.nativeRequests()['/native-key/complete'],1);
    await delay(5200); // The archived Root observation cannot itself authorize ready.
    await runtime.restart('native-chain');
    const recovered=await runtime.drive('native-account-result-one');
    assert.equal(recovered.stage,'native_session_ready',JSON.stringify({recovered,nativeRequests:runtime.nativeRequests()}));
    assert.equal(runtime.nativeRequests()['/native-key/complete'],1,'status recovery never repeats native login complete');
    assert.equal(runtime.nativeRequests()['/custody/managed-bindings/session-status'],2,'recovery obtains a fresh Root status');
    await runtime.suspend();
    const saved=persistedRuntime(runtime.persist,runtime.account);
    assert.equal(saved.history_count,1);
    assert.equal(saved.status_recoveries.length,1);
    assert.equal(saved.status_recoveries[0].has_result,true);
    assert.equal(saved.status_recoveries[0].reads,3,'actual recovery uses three of eight reserved reads');
  });
});

test('a still-fresh archived result uses original admission without purchasing another status grant', {timeout:120000}, async () => {
  await withRoleRuntime('native-lost-account-result', async runtime => {
    assert.equal((await runtime.drive('native-fresh-result-one')).stage,'native_registered');
    await runtime.restart('native-chain');
    assert.equal((await runtime.drive('native-fresh-result-one')).stage,'native_session_ready');
    assert.equal(runtime.nativeRequests()['/native-key/complete'],1);
    assert.equal(runtime.nativeRequests()['/custody/managed-bindings/session-status'],1);
    await runtime.suspend();
    const saved=persistedRuntime(runtime.persist,runtime.account);
    assert.equal(saved.history_count,1);
    assert.equal(saved.status_recoveries.length,0);
  });
});

for (const limit of ['64','128']) {
  test(`expired ManagedLogin result recovery spends only its signed ${limit} budget`, {timeout:120000}, async () => {
    await withRoleRuntime('native-lost-login', async runtime => {
      const blocked=await runtime.drive('native-managed-budget-one');
      assert.equal(blocked.stage,'native_registered');
      const approved=await runtime.approveManaged(blocked.pending_operations[0],limit);
      await runtime.restart('native-lost-account-result');
      assert.equal((await runtime.continueManaged(approved.operation_id)).stage,'approved');
      assert.equal(runtime.nativeRequests()['/native-key/complete'],2);
      await delay(5200);
      await runtime.restart('native-chain');
      const after=await runtime.continueManaged(approved.operation_id);
      assert.equal(after.stage,limit==='128'?'native_session_ready':'approved');
      assert.equal(runtime.nativeRequests()['/native-key/complete'],2,'neither budget allows another login send');
      await runtime.suspend();
      const saved=persistedRuntime(runtime.persist,runtime.account);
      assert.equal(saved.history_count,1);
      assert.equal(saved.status_recoveries.length,limit==='128'?1:0,'cannot borrow the original Provision balance');
      if(limit==='128') {
        assert.equal(saved.status_recoveries[0].has_result,true);
        assert.equal(saved.status_recoveries[0].reads,3);
      }
    });
  });
}

test('lost missing-SID submission is recovered only as explicit login required', {timeout:120000}, async () => {
  await withRoleRuntime('native-lost-missing-result', async runtime => {
    const pending=await runtime.drive('native-missing-result-one');
    assert.equal(pending.stage,'native_registered');
    await delay(5200);
    await runtime.restart('native-chain');
    const tightened=await runtime.drive('native-missing-result-one');
    assert.equal(tightened.stage,'native_registered');
    assert.equal(runtime.nativeRequests()['/native-key/complete'],1);
    assert.equal(runtime.nativeRequests()['/custody/managed-bindings/session-status']??0,0);
    await runtime.suspend();
    const saved=persistedRuntime(runtime.persist,runtime.account);
    assert.ok(saved.native_results.includes('explicit_login_required'));
    assert.equal(saved.history_count,0,'missing SID is never a signed Root history fact');
    assert.equal(saved.status_recoveries.length,0,'tightening never purchases a status grant');
  });
});

test('an independently frozen Fence blocks live recovery of a retained Identity result', {timeout:120000}, async () => {
  await withRoleRuntime('native-lost-account-result', async runtime => {
    assert.equal((await runtime.drive('native-fenced-result-one')).stage,'native_registered');
    await delay(5200);
    await runtime.restart('native-chain');
    await runtime.freezeFence();
    const reply=await runtime.driveResponse('native-fenced-result-one');
    if(reply.ok) assert.notEqual((await reply.json()).stage,'native_session_ready');
    assert.equal(runtime.nativeRequests()['/native-key/complete'],1);
    assert.equal(runtime.nativeRequests()['/custody/managed-bindings/session-status'],1,'frozen recovery never calls native status');
    await runtime.suspend();
    const saved=persistedRuntime(runtime.persist,runtime.account);
    assert.ok(saved.status_recoveries.every(v=>!v.has_result));
    assert.ok(!saved.native_results.includes('session_status'));
  });
});

test('actual manager UV, Account, three vaults and native SQLite establish one managed session without manager credentials', {timeout:120000}, async () => {
  await withRoleRuntime('native-chain', async runtime => {
    const progress=await runtime.drive('native-chain-one');
    assert.equal(progress.stage,'native_session_ready',JSON.stringify({progress,nativeRequests:runtime.nativeRequests()}));
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
    assert.equal(saved.native_reads.find(v=>v.stage==='login')?.reads,8,'cold login spends exactly the eight real Account oracle credits');
    assert.ok(saved.native_reads.every(v=>v.reads<=8),'no phase silently enlarges the oracle budget');
    assert.ok(saved.cipher_records>=6,'private originals and SID/CSRF remain encrypted');
  });
});

test('a fresh B2 attempt recovers the known committed B1 receipt after loss and expiry without another native binding', {timeout:120000}, async () => {
  await withRoleRuntime('native-lost-binding', async runtime => {
    const pending=await runtime.drive('native-binding-one');
    assert.equal(pending.stage,'role_keys_ready');
    assert.ok(pending.pending_operations.length>0);
    assert.equal(runtime.nativeRequests()['/custody/managed-bindings/complete'],1);
    // The original Owner statement is no longer fresh. Its immutable receipt is
    // evidence only; a new paid attempt must use the current source and Fence.
    await delay(5200);
    await runtime.restart('native-chain');
    const recovered=await runtime.drive('native-binding-two');
    assert.equal(recovered.stage,'native_session_ready',JSON.stringify({recovered,nativeRequests:runtime.nativeRequests()}));
    assert.equal(runtime.nativeRequests()['/custody/managed-bindings/complete'],1);
    assert.equal(runtime.nativeRequests()['/native-key/complete'],1);
  });
});

test('lost original Cookie never replays login and a fresh managed approval works after the old plan expires', {timeout:120000}, async () => {
  await withRoleRuntime('native-lost-login', async runtime => {
    const blocked=await runtime.drive('native-cookie-lost-one');
    assert.equal(blocked.stage,'native_registered');
    assert.ok(blocked.pending_operations.length>0);
    assert.equal(runtime.nativeRequests()['/native-key/complete'],1);
    const retry=await runtime.drive('native-cookie-lost-one');
    assert.equal(retry.stage,'native_registered');
    assert.equal(runtime.nativeRequests()['/native-key/complete'],1,'same old continuation cannot reclaim Cookie');
    await delay(Math.max(0,runtime.planExpires-Date.now()+100));
    const approved=await runtime.approveManaged(blocked.pending_operations[0]);
    assert.equal(approved.stage,'approved');
    const ready=await runtime.continueManaged(approved.operation_id);
    assert.equal(ready.stage,'native_session_ready',JSON.stringify({ready,nativeRequests:runtime.nativeRequests()}));
    assert.equal(runtime.nativeRequests()['/native-key/complete'],2,'only the separately approved new logical login sends');
  });
});
