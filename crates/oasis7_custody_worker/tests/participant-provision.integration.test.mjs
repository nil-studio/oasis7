import assert from 'node:assert/strict';
import test from 'node:test';
import {withRoleRuntime,persistedRuntime} from './fixtures/role_runtime_native_support.mjs';
test('actual native and canonical World first binding consumes only its approved 104 remaining credits', {timeout:180000},async()=>{
  await withRoleRuntime('native-participant',async runtime=>{
    const progress=await runtime.drive('participant-one');
    assert.equal(progress.stage,'participant_bound',JSON.stringify({progress,requests:runtime.nativeRequests(),world:runtime.worldResults()}));
    assert.equal(progress.transport_remaining,'0');
    assert.deepEqual(progress.pending_operations,[]);
    const businessCounts=()=>Object.fromEntries(Object.entries(runtime.nativeRequests()).filter(([path])=>!['/custody/management-session-status','/custody/approvals/verify'].includes(path)));
    const before=businessCounts();
    assert.equal(before['/participant/policy-original'],1);
    assert.equal(before['/participant/authorize'],1);
    assert.equal(before['/participant/submit'],1);
    assert.equal(before['/participant/owner-view'],1);
    assert.ok((before['/participant/lookup']??0)<=1);
    await runtime.restart('native-participant');
    assert.equal((await runtime.drive('participant-another-management-id')).stage,'participant_bound');
    assert.deepEqual(businessCounts(),before,'continuation cannot create another participant send');
    await runtime.suspend();
    const saved=persistedRuntime(runtime.persist,runtime.account);
    assert.equal(saved.participants.length,1);
    assert.equal(saved.participants[0].reads,8);
    assert.equal(saved.participants[0].has_result,true);
    assert.equal(saved.slots.length,3);
  });
});

test('a lost submit response uses only the registered lookup and never sends another registration',{timeout:180000},async()=>{
  await withRoleRuntime('native-participant-lost-submit',async runtime=>{
    const progress=await runtime.drive('participant-loss-one');
    assert.equal(progress.stage,'participant_bound',JSON.stringify({progress,requests:runtime.nativeRequests()}));
    assert.equal(runtime.nativeRequests()['/participant/submit'],1);
    assert.equal(runtime.nativeRequests()['/participant/lookup'],1);
    await runtime.restart('native-participant');
    assert.equal((await runtime.drive('participant-loss-another')).stage,'participant_bound');
    assert.equal(runtime.nativeRequests()['/participant/submit'],1);
    assert.equal(runtime.nativeRequests()['/participant/lookup'],1);
  });
});

for(const fault of ['tamper-policy','tamper-final','real-fence-freeze','session-revoke','wrapper-version','wrapper-ok','wrapper-unknown','wrapper-upstream']) {
  test(`actual participant ${fault} cannot settle and restart cannot resend`,{timeout:180000},async()=>{
    await withRoleRuntime('native-participant-'+fault,async runtime=>{
      const progress=await runtime.drive('participant-negative-one');
      assert.equal(progress.stage,'native_session_ready');
      assert.equal(progress.transport_remaining,'0');
      assert.equal(progress.pending_operations.length,1);
      const submits=runtime.nativeRequests()['/participant/submit']??0;
      if(fault==='session-revoke') assert.equal(runtime.nativeRequests()['/participant/authorize']??0,0);
      assert.equal(submits,fault==='tamper-final'||fault.startsWith('wrapper-')?1:0);
      await runtime.restart('native-participant');
      const reply=await runtime.driveResponse('participant-negative-another-id');
      if(reply.ok) assert.equal((await reply.json()).stage,'native_session_ready');
      assert.equal(runtime.nativeRequests()['/participant/submit']??0,submits);
      await runtime.suspend();
      const saved=persistedRuntime(runtime.persist,runtime.account);
      assert.equal(saved.participants.length,1);
      assert.equal(saved.participants[0].has_result,false);
    });
  });
}

test('103 remaining credits cannot silently purchase a 104-credit participant attempt',{timeout:180000},async()=>{
  await withRoleRuntime('native-participant-low-budget',async runtime=>{
    const reply=await runtime.driveResponse('participant-low-budget');
    assert.equal(reply.status,503);
    assert.equal((await reply.json()).code,'not_ready');
    assert.equal(runtime.nativeRequests()['/participant/policy-original']??0,0);
    assert.equal(runtime.nativeRequests()['/participant/submit']??0,0);
    await runtime.suspend();
    const saved=persistedRuntime(runtime.persist,runtime.account);
    assert.equal(saved.participants.length,0);
    assert.equal(saved.provision_stage,'native_session_ready');
    assert.equal(saved.transport_remaining,'103');
  });
});

test('final participant publication and audit roll back together after actual World commit',{timeout:180000},async()=>{
  await withRoleRuntime('native-participant-audit-rollback',async runtime=>{
    const progress=await runtime.drive('participant-rollback');
    assert.equal(progress.stage,'native_session_ready');
    assert.equal(progress.transport_remaining,'0');
    assert.deepEqual(await runtime.participantFault(),{faults:1,attempted_bound:true});
    const before=runtime.nativeRequests();
    assert.equal(before['/participant/submit'],1);
    assert.equal(before['/participant/owner-view'],1);
    await runtime.restart('native-participant');
    const reply=await runtime.driveResponse('participant-rollback-restart');
    if(reply.ok) assert.equal((await reply.json()).stage,'native_session_ready');
    assert.equal(runtime.nativeRequests()['/participant/submit'],1);
    await runtime.suspend();
    const saved=persistedRuntime(runtime.persist,runtime.account);
    assert.equal(saved.provision_stage,'native_session_ready');
    assert.equal(saved.participants[0].has_result,false);
    assert.equal(saved.participants[0].reads,8);
    assert.equal(saved.participant_final_audits,0);
  });
});
