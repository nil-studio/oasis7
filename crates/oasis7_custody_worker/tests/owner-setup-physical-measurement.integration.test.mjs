import assert from 'node:assert/strict';
import test from 'node:test';
import {withRoleRuntime,persistedRuntime} from './fixtures/role_runtime_native_support.mjs';
import {ownerSetupPhysicalTrace,configureOwnerSetupPhysicalTrace,physicalStartCounts} from './fixtures/owner_setup_physical_trace.mjs';
function plan(b) {
  return {api_version:1,setup_id:'physical-setup',provision_id:b.provision_id,participant_receipt_digest:b.participant_receipt_digest,
    world_identity:b.world_identity,owner_key_ref:b.owner_key.key_id,executor_key_ref:b.executor_key.key_id,
    bootstrap_config_version:b.bootstrap_config_version,goal_content:{goal_text:'Isolated physical cleanup feasibility measurement',soft_constraints:[]},
    create_operation_id:'physical-create',prepare_operation_id:'physical-prepare',prepare_nonce:'ac'.repeat(32),publish_operation_id:'physical-publish',
    delegation_operation_id:'physical-delegate',bind_operation_id:'physical-bind',executor_id:'physical-executor',executor_public_key:b.executor_key.public_key,
    delegation_generation:'1',delegation_nonce:'1',resource_refs:[],deadline:String(Date.now()+120000),request_limit:'6',transport_limit:'584',write_limit:'4'};
}
const count=(events,port)=>events.filter(v=>v.port===port&&v.phase==='start').length;
function measuredSubgraphs(all) {
  const events=all.filter(e=>e.stage==='delegation');
  const afterPrivate=events.findIndex(e=>e.port==='private.csrf.after'&&e.phase==='success');
  assert.ok(afterPrivate>=0,'actual second private read must complete');
  let secondFenceEnd=-1,returns=0;
  for(let i=afterPrivate+1;i<events.length;i++)if(events[i].port==='fence.service'&&events[i].phase==='success'&&++returns===2){secondFenceEnd=i;break;}
  assert.ok(secondFenceEnd>=0,'both paid second-observation Fence replies must complete');
  const prefix=events.slice(0,secondFenceEnd+1);
  assert.equal(prefix.filter(v=>v.phase==='start').length,28,'Source verify has one actual Native HTTP, no challenge');
  assert.deepEqual(physicalStartCounts(prefix),{'directory.actor':4,'fence.service':6,'fence.actor':6,'source.verify.service':1,'native.call':1,'identity.driver.service':1,'identity.manifest.actor':1,'owner.permit.service':2,'owner.permit.account':2,'private.cookie.before':1,'private.cookie.after':1,'private.csrf.before':1,'private.csrf.after':1});
  const start=events.findIndex(e=>e.port==='owner.factory.service'&&e.phase==='start');
  const end=events.findIndex((e,i)=>i>start&&e.port==='owner.factory.service'&&e.phase==='success');
  assert.ok(start>=0&&end>start,'the real factory must return after Vault publication');
  const signing=events.slice(start,end+1);
  assert.equal(signing.filter(v=>v.phase==='start').length,23);
  assert.deepEqual(physicalStartCounts(signing),{'owner.factory.service':1,'owner.permit.service':3,'owner.permit.account':3,'directory.actor':3,'fence.service':6,'fence.actor':6,'owner.vault.actor':1});
}
test('actual physical OwnerSetup graph counts its Source prefix and signing plus cold original recovery',{timeout:240000,skip:process.env.OASIS7_OWNER_SETUP_PHYSICAL_MEASUREMENT!=='true'},async()=>{
  const trace=ownerSetupPhysicalTrace();
  await withRoleRuntime('native-participant-owner-setup-real-fence-lost-delegation',async runtime=>{
    assert.equal((await runtime.drive('physical-participant')).stage,'participant_bound');
    const setup=plan(await runtime.setupBasis());await runtime.approveSetup(setup);
    const pending=await runtime.continueSetup(setup.setup_id);assert.equal(pending.status,200,await pending.clone().text());
    assert.equal((await pending.json()).stage,'ready_for_setup');
    assert.equal(runtime.lostSubmitCount(),1);assert.equal(runtime.lostLookupCount(),1);
    await runtime.suspend();
    measuredSubgraphs(trace.snapshot());
    const saved=persistedRuntime(runtime.persist,runtime.account).setups[0].stages.find(v=>v.stage==='delegation');
    assert.equal(saved.has_result,false);assert.equal(saved.has_original,true);
    await runtime.restart('native-participant-owner-setup-real-fence-lost-delegation');
    const b=await runtime.reconcileBasis(setup.setup_id,'delegation');
    const reconcile={api_version:1,reconcile_id:'physical-reconcile',setup_id:b.setup_id,provision_id:b.provision_id,stage:b.stage,
      setup_plan_digest:b.setup_plan_digest,original_digest:b.original_digest,correlation_digest:b.correlation_digest,world_identity:b.world_identity,
      owner_key_ref:b.owner_key_ref,executor_key_ref:b.executor_key_ref,original_deadline:b.original_deadline,deadline:b.original_deadline,attempt_limit:1,transport_limit:'112'};
    await runtime.approveReconcile(reconcile);
    const mark=trace.snapshot().length, nativeBefore=runtime.nativeRequests(),worldBefore=runtime.worldPortCounts(),controlsBefore=runtime.nativeControlCalls().length;
    const response=await runtime.continueReconcile(reconcile.reconcile_id,'physical-read');
    assert.equal(response.status,200,await response.clone().text());assert.deepEqual((await response.json()).completed_attempts,['physical-read']);
    const controlsAfter=runtime.nativeControlCalls().length;
    assert.deepEqual(runtime.nativeControlCalls().slice(controlsBefore),[{operation:'lookup',kind:'delegation',exactOriginal:true}]);
    const replay=await runtime.continueReconcile(reconcile.reconcile_id,'physical-read');assert.equal(replay.status,200,await replay.text());
    assert.equal(runtime.nativeControlCalls().length,controlsAfter);
    await runtime.suspend();
    const recovery=trace.snapshot().slice(mark).filter(v=>v.stage==='reconcile');
    const counts=physicalStartCounts(recovery);
    assert.ok(counts['owner.permit.service']>0,'recovery must use actual traced consumers');
    assert.equal(counts['owner.permit.service'],counts['owner.permit.account']);
    assert.equal(counts['fence.service'],counts['fence.actor']);
    assert.equal(counts['source.verify.service'],counts['native.call']);
    for(const p of ['private.cookie.before','private.cookie.after','private.csrf.before','private.csrf.after','identity.driver.service','identity.manifest.actor','owner.factory.service','owner.vault.actor'])assert.equal(counts[p],1,p);
    const nativeAfter=runtime.nativeRequests();
    const nativeDelta=Object.fromEntries(Object.keys(nativeAfter).map(p=>[p,nativeAfter[p]-(nativeBefore[p]??0)]).filter(([,n])=>n>0));
    const worldAfter=runtime.worldPortCounts();
    const worldDelta=Object.fromEntries(Object.keys(worldAfter).map(p=>[p,worldAfter[p]-(worldBefore[p]??0)]).filter(([,n])=>n>0));
    assert.equal(worldDelta['post:lookup'],1,'fresh recovery reaches the real World original lookup exactly once');
    assert.equal(worldDelta['post:submit']??0,0,'fresh recovery cannot submit another effect');
    assert.ok(Object.values(worldDelta).every(n=>Number.isSafeInteger(n)&&n>0));
    console.log(JSON.stringify({physical_recovery_worker_starts:counts,native_http_delta:nativeDelta,world_receiver_delta:worldDelta,native_world_receivers_complete:true,cleanup_dispatcher_qualified:false}));
  },{settleSubmits:true,physicalWorldPorts:true,configureOptions:raw=>configureOwnerSetupPhysicalTrace(raw,trace)});
});
