// Developer-only fixed synthetic Native/World fixture. No production account input.
import {withRoleRuntime} from '../tests/fixtures/role_runtime_native_support.mjs';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {dirname,resolve} from 'node:path';
import {fileURLToPath} from 'node:url';
import {readSavedAccountSnapshot,benchmarkAccountRestore} from '../tests/fixtures/account_restore_bench.mjs';
const root=resolve(dirname(fileURLToPath(import.meta.url)),'..');
function configureOptions(options){
  const owner=options.workers.find(worker=>worker.name==='owner');
  assert.ok(owner,'actual Owner worker required');
  const productionRoot=resolve(root,'artifacts/owner')+'/';
  const diagnosticRoot=resolve(root,'artifacts/approval-harness')+'/';
  let wrappers=0;
  owner.modules=owner.modules.map(module=>{
    if(module.path.startsWith(productionRoot))return {...module,path:diagnosticRoot+module.path.slice(productionRoot.length)};
    if(module.path===resolve(root,'tests/fixtures/native_owner_result_loss.mjs')){
      const contents=readFileSync(module.path,'utf8'),marker='../../artifacts/owner/entry.mjs';
      assert.equal(contents.split(marker).length-1,2);
      wrappers++;return {...module,contents:contents.replaceAll(marker,'../../artifacts/approval-harness/entry.mjs')};
    }
    return module;
  });
  assert.equal(wrappers,1);return options;
}
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
  const continuation_status=reply.status;
  let progress=null;
  if(reply.status===200)progress=await reply.json();
  else await reply.text(); // Never echo response input or error details; partial persisted state is still measured.
  const actual_callback_metrics=await runtime.readAccountRestoreCallbacks();
  assert.equal(actual_callback_metrics.actor_identity_available,true,'actual Account identity must be available for attribution');
  assert.ok(actual_callback_metrics.observed_callback_count>0,'actual Account callback evidence is required, not isolated callback zero');
  await runtime.suspend();
  const snapshot=readSavedAccountSnapshot(runtime.persist,runtime.account);
  const metrics=await benchmarkAccountRestore(snapshot);
  console.log(JSON.stringify({continuation_status,observed_stage:progress?.stage??null,pending_count:progress?.pending_operations?.length??null,actual_callback_metrics,...metrics}));
},{configureOptions});
