// Developer-only fixed synthetic Native/World fixture. No production account input.
import {withRoleRuntime} from '../tests/fixtures/role_runtime_native_support.mjs';
import {readSavedAccountSnapshot,benchmarkAccountRestore} from '../tests/fixtures/account_restore_bench.mjs';
await withRoleRuntime('native-participant-owner-setup-real-fence',async runtime=>{
  const participant=await runtime.drive('restore-measurement-participant');
  if(participant.stage!=='participant_bound') throw new Error('measurement fixture participant unavailable');
  const basis=await runtime.setupBasis();
  const plan={api_version:1,setup_id:'restore-measurement-setup',provision_id:basis.provision_id,
    participant_receipt_digest:basis.participant_receipt_digest,world_identity:basis.world_identity,
    owner_key_ref:basis.owner_key.key_id,executor_key_ref:basis.executor_key.key_id,
    bootstrap_config_version:basis.bootstrap_config_version,
    goal_content:{goal_text:'Synthetic restore measurement objective',soft_constraints:[]},
    create_operation_id:'restore-create',prepare_operation_id:'restore-prepare',prepare_nonce:'ac'.repeat(32),
    publish_operation_id:'restore-publish',delegation_operation_id:'restore-delegate',bind_operation_id:'restore-bind',
    executor_id:'restore-executor',executor_public_key:basis.executor_key.public_key,
    delegation_generation:'1',delegation_nonce:'1',resource_refs:[],deadline:String(Date.now()+120000),
    request_limit:'6',transport_limit:'584',write_limit:'4'};
  await runtime.approveSetup(plan);
  const reply=await runtime.continueSetup(plan.setup_id);
  if(reply.status!==200) throw new Error('measurement fixture continuation unavailable');
  const progress=await reply.json();
  await runtime.suspend();
  const snapshot=readSavedAccountSnapshot(runtime.persist,runtime.account);
  const metrics=await benchmarkAccountRestore(snapshot);
  console.log(JSON.stringify({observed_stage:progress.stage,pending_operations:progress.pending_operations,...metrics}));
});
