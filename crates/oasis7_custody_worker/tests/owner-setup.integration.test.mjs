import assert from 'node:assert/strict';
import test from 'node:test';
import {withRoleRuntime,persistedRuntime} from './fixtures/role_runtime_native_support.mjs';
import {readSavedAccountSnapshot,verifySnapshotReuse} from './fixtures/account_restore_bench.mjs';

function setupPlan(basis) {
  return {api_version:1,setup_id:'owner-setup-one',provision_id:basis.provision_id,
      participant_receipt_digest:basis.participant_receipt_digest,world_identity:basis.world_identity,
      owner_key_ref:basis.owner_key.key_id,executor_key_ref:basis.executor_key.key_id,
      bootstrap_config_version:basis.bootstrap_config_version,
      goal_content:{goal_text:'Isolated setup fixture private objective',soft_constraints:['Remain within this approved task']},
      create_operation_id:'owner-setup-create',prepare_operation_id:'owner-setup-prepare',prepare_nonce:'ac'.repeat(32),
      publish_operation_id:'owner-setup-publish',delegation_operation_id:'owner-setup-delegate',bind_operation_id:'owner-setup-bind',
      executor_id:'owner-setup-executor',executor_public_key:basis.executor_key.public_key,
      delegation_generation:'1',delegation_nonce:'1',resource_refs:[],deadline:String(Date.now()+120000),
      request_limit:'6',transport_limit:'584',write_limit:'4'};
}

test('separate real UV owner setup creates canonical task, private goal, delegation and binding once', {timeout:240000},async()=>{
  await withRoleRuntime('native-participant-owner-setup-real-fence',async runtime=>{
    assert.equal((await runtime.drive('owner-setup-participant')).stage,'participant_bound');
    const basis=await runtime.setupBasis();
    assert.equal(basis.security_epoch,'1');
    const plan=setupPlan(basis);
    const approved=await runtime.approveSetup(plan);
    assert.equal(approved.stage,'participant_bound','UV alone never grants ReadyForSetup');
    const reply=await runtime.continueSetup(plan.setup_id);
    const body=await reply.text();
    assert.equal(reply.status,200,JSON.stringify({status:reply.status,body,requests:runtime.nativeRequests()}));
    const progress=JSON.parse(body);
    if(progress.stage!=='prepared') {
      await runtime.suspend();
      assert.fail(JSON.stringify({progress,setup:persistedRuntime(runtime.persist,runtime.account).setups,requests:runtime.nativeRequests(),world:runtime.worldResults()}));
    }
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
    await verifySnapshotReuse(readSavedAccountSnapshot(runtime.persist,runtime.account));
    assert.equal(saved.setups.length,1);
    assert.equal(saved.setups[0].stages.length,6);
    assert.ok(saved.setups[0].stages.every(stage=>stage.reads===5 && stage.has_result));
  });
});

for(const revoke of [false,true]) {
  test(`a setup stage after the original UV window ${revoke?'rejects actual manager revocation':'uses a new signed origin inspection'}`,{timeout:240000},async()=>{
    const mode='native-participant-owner-setup-real-fence-gap'+(revoke?'-revoke':'');
    await withRoleRuntime(mode,async runtime=>{
      assert.equal((await runtime.drive('owner-setup-gap-participant')).stage,'participant_bound');
      const plan=setupPlan(await runtime.setupBasis());
      await runtime.approveSetup(plan);
      const reply=await runtime.continueSetup(plan.setup_id);
      const body=await reply.text();
      if(!revoke) {
        assert.equal(reply.status,200,JSON.stringify({body,requests:runtime.nativeRequests()}));
        assert.equal(JSON.parse(body).stage,'prepared',JSON.stringify({body,requests:runtime.nativeRequests(),facts:runtime.approvalFacts()}));
        assert.ok(runtime.approvalFacts().some(fact=>fact.approval_request_id==='owner-setup-approval-one' &&
          fact.purpose==='inspect_origin' && fact.proof_state==='expired' && fact.origin_state==='active' &&
          BigInt(fact.observed_at)>=BigInt(fact.proof_expires_at)), 'a real expired proof and fresh origin observation must be present');
      } else {
        if(reply.ok) assert.notEqual(JSON.parse(body).stage,'prepared');
        else {assert.equal(reply.status,503,body);assert.equal(JSON.parse(body).code,'not_ready');}
        assert.equal(runtime.nativeRequests()['/participant/authorize'],2,'revocation stops before the next stage access or effect');
        assert.ok(runtime.approvalFacts().some(fact=>fact.approval_request_id==='owner-setup-approval-one' &&
          fact.purpose==='inspect_origin' && fact.origin_state!=='active'), 'actual native origin revocation must be observed');
        await runtime.suspend();
        const saved=persistedRuntime(runtime.persist,runtime.account);
        assert.equal(saved.provision_stage,'ready_for_setup');
        assert.equal(saved.setups[0].stages.filter(stage=>stage.has_result).length,1);
      }
    });
  });
}

for(const recoveryStage of ['create_task','bind','final_observation','rollback']) {
const rollback=recoveryStage==='rollback';
const targetStage=rollback?'create_task':recoveryStage;
test(rollback?'reconcile final SQL failure rolls back both old and new results and audit':`fresh paid reconciliation restores ${targetStage} without resending its write`,{timeout:240000},async()=>{
  await withRoleRuntime('native-participant-owner-setup-real-fence-reconcile'+(rollback?'-rollback':targetStage==='bind'?'-bind':targetStage==='final_observation'?'-final':''),async runtime=>{
    assert.equal((await runtime.drive('owner-setup-reconcile-participant')).stage,'participant_bound');
    const plan=setupPlan(await runtime.setupBasis());
    await runtime.approveSetup(plan);
    const stopped=await runtime.continueSetup(plan.setup_id);
    assert.equal(stopped.status,200,await stopped.clone().text());
    assert.equal((await stopped.json()).stage,'ready_for_setup');
    const basis=await runtime.reconcileBasis(plan.setup_id,targetStage);
    const reconcile={api_version:1,reconcile_id:'owner-reconcile-one',setup_id:basis.setup_id,
      provision_id:basis.provision_id,stage:basis.stage,setup_plan_digest:basis.setup_plan_digest,
      original_digest:basis.original_digest,correlation_digest:basis.correlation_digest,
      world_identity:basis.world_identity,owner_key_ref:basis.owner_key_ref,executor_key_ref:basis.executor_key_ref,
      original_deadline:basis.original_deadline,deadline:basis.original_deadline,attempt_limit:1,transport_limit:'112'};
    await runtime.approveReconcile(reconcile);
    const before=runtime.nativeRequests()['/entry-owner/control'];
    const recovered=await runtime.continueReconcile(reconcile.reconcile_id,'read-one');
    const text=await recovered.text();assert.equal(recovered.status,200,text);
    const progress=JSON.parse(text);
    if(rollback) {
      assert.deepEqual(progress.completed_attempts,[]);
      assert.deepEqual(await runtime.reconcileFault(),{faults:1,attempted_result:true});
      await runtime.suspend();
      const saved=persistedRuntime(runtime.persist,runtime.account);
      const created=saved.setups[0].stages.find(stage=>stage.stage===targetStage);
      assert.equal(created.has_result,false);
      assert.equal(created.result_provenance,null);
      assert.deepEqual(saved.setup_reconciles.map(v=>({attempts:v.attempts.map(({reads,has_fact,has_result,lookup_claimed})=>({reads,has_fact,has_result,lookup_claimed}))})),[{attempts:[{reads:5,has_fact:true,has_result:false,lookup_claimed:true}]}]);
      return;
    }
    if(progress.completed_attempts.length===0) {
      await runtime.suspend();
      const saved=persistedRuntime(runtime.persist,runtime.account);
      assert.fail(JSON.stringify({progress,reconciles:saved.setup_reconciles,setup:saved.setups,requests:runtime.nativeRequests(),world:runtime.worldResults().filter(v=>v.path==='/entry-owner/control').map(v=>({path:v.path,http:v.http,outcome:v.outcome,error:v.error}))}));
    }
    assert.deepEqual(progress.completed_attempts,['read-one'],text);
    assert.equal(progress.transport_remaining,'0');
    assert.equal(runtime.nativeRequests()['/entry-owner/control'],before+1,'recovery sends only the registered original lookup');
    const replay=await runtime.continueReconcile(reconcile.reconcile_id,'read-one');
    assert.equal(replay.status,200,await replay.text());
    assert.equal(runtime.nativeRequests()['/entry-owner/control'],before+1,'exact attempt retry performs no World I/O');
    const continued=await runtime.continueSetup(plan.setup_id);
    const body=await continued.text();assert.equal(continued.status,200,body);
    assert.equal(JSON.parse(body).stage,'prepared',body);
    await runtime.suspend();
    const saved=persistedRuntime(runtime.persist,runtime.account);
    assert.deepEqual(saved.setup_reconciles.map(v=>({attempts:v.attempts.map(({reads,has_fact,has_result,lookup_claimed})=>({reads,has_fact,has_result,lookup_claimed}))})),[{attempts:[{reads:5,has_fact:targetStage!=='final_observation',has_result:true,lookup_claimed:targetStage!=='final_observation'}]}]);
    const created=saved.setups[0].stages.find(stage=>stage.stage===targetStage);
    assert.deepEqual(created.result_provenance,{kind:'reconcile',reconcile_id:'owner-reconcile-one',attempt_id:'read-one'});
    assert.deepEqual(created.archived_sources,targetStage==='final_observation'?[]:['submit'],'recovery never overwrites the original stage history');
  });
});

}
