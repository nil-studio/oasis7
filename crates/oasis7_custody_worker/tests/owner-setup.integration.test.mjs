import assert from 'node:assert/strict';
import test from 'node:test';
import {withRoleRuntime,persistedRuntime} from './fixtures/role_runtime_native_support.mjs';

test('separate real UV owner setup creates canonical task, private goal, delegation and binding once', {timeout:240000},async()=>{
  await withRoleRuntime('native-participant-owner-setup-real-fence',async runtime=>{
    assert.equal((await runtime.drive('owner-setup-participant')).stage,'participant_bound');
    const basis=await runtime.setupBasis();
    assert.equal(basis.security_epoch,'1');
    const plan={api_version:1,setup_id:'owner-setup-one',provision_id:basis.provision_id,
      participant_receipt_digest:basis.participant_receipt_digest,world_identity:basis.world_identity,
      owner_key_ref:basis.owner_key.key_id,executor_key_ref:basis.executor_key.key_id,
      bootstrap_config_version:basis.bootstrap_config_version,
      goal_content:{goal_text:'Isolated setup fixture private objective',soft_constraints:['Remain within this approved task']},
      create_operation_id:'owner-setup-create',prepare_operation_id:'owner-setup-prepare',prepare_nonce:'ac'.repeat(32),
      publish_operation_id:'owner-setup-publish',delegation_operation_id:'owner-setup-delegate',bind_operation_id:'owner-setup-bind',
      executor_id:'owner-setup-executor',executor_public_key:basis.executor_key.public_key,
      delegation_generation:'1',delegation_nonce:'1',resource_refs:[],deadline:String(Date.now()+120000),
      request_limit:'6',transport_limit:'584',write_limit:'4'};
    const approved=await runtime.approveSetup(plan);
    assert.equal(approved.stage,'participant_bound','UV alone never grants ReadyForSetup');
    const reply=await runtime.continueSetup(plan.setup_id);
    const body=await reply.text();
    assert.equal(reply.status,200,JSON.stringify({status:reply.status,body,requests:runtime.nativeRequests()}));
    const progress=JSON.parse(body);
    assert.equal(progress.stage,'prepared',JSON.stringify({progress,requests:runtime.nativeRequests(),world:runtime.worldResults()}));
    assert.deepEqual(progress.pending_operations,[]);
    assert.deepEqual(progress.blocked_operations,[]);
    assert.equal(progress.transport_remaining,'0');
    const business=()=>Object.fromEntries(Object.entries(runtime.nativeRequests()).filter(([path])=>!['/custody/management-session-status','/custody/approvals/verify'].includes(path)));
    const before=business();
    assert.equal(before['/participant/authorize'],7,'each setup stage renews access with the original owner registration');
    assert.equal(before['/participant/policy-original'],7);
    assert.equal(before['/participant/submit'],1,'setup never re-registers the participant');
    await runtime.restart('native-participant-owner-setup-real-fence');
    const retry=await runtime.continueSetup(plan.setup_id);
    assert.equal(retry.status,200);
    assert.equal((await retry.json()).stage,'prepared');
    assert.deepEqual(business(),before,'terminal history never resends setup effects or renews admission');
    await runtime.suspend();
    const saved=persistedRuntime(runtime.persist,runtime.account);
    assert.equal(saved.setups.length,1);
    assert.equal(saved.setups[0].stages.length,6);
    assert.ok(saved.setups[0].stages.every(stage=>stage.reads===5 && stage.has_result));
  });
});
