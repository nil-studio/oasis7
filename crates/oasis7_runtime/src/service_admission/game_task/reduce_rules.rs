//! Unique historical task reduction. The fixed Legacy and Current adapters
//! supply only accounting and original SDK operations; selection is elsewhere.
use super::*;
pub(crate) trait TaskReductionOpsV1 {
 type Error;
 fn is_metered(&self)->bool;
 fn work(&self,steps:u64,bytes:usize)->Result<(),Self::Error>;
 fn before_owned(&self,bytes:usize)->Result<(),Self::Error>;
 fn business(&self,note:&'static str)->Self::Error;
 fn copy(&self,text:&str)->Result<String,Self::Error>;
 fn digest<T:serde::Serialize>(&self,domain:&str,value:&T)->Result<String,Self::Error>;
 fn create_control(&self,request:&SubmitIntentRequest<WorldServicePayloadV1>)->Result<GameTaskControlV1,Self::Error>;
 fn executor_receipt(&self,request:&SubmitIntentRequest<WorldServicePayloadV1>)->Result<executor::ExecutorReceipt,Self::Error>;
 fn publish_receipt(&self,request:&SubmitIntentRequest<WorldServicePayloadV1>,height:u64)->Result<oasis7_client_api::legacy_task_results::PublishReceipt,Self::Error>;
 fn control_comparison(&self,control:&GameTaskControlV1)->Result<(),Self::Error>;
}
fn tree_work<V,O:TaskReductionOpsV1>(tree:&BTreeMap<String,V>,key:&str,ops:&O)->Result<(),O::Error>{if !ops.is_metered(){return Ok(())}for existing in tree.keys(){ops.work(1,existing.len()+key.len())?;}Ok(())}
fn insert_slots<K,V,O:TaskReductionOpsV1>(len:usize,ops:&O)->Result<(),O::Error>{if !ops.is_metered(){return Ok(())}let levels=(usize::BITS-len.leading_zeros())as usize+2;let n=std::mem::size_of::<(K,V)>().checked_mul(32).and_then(|n|n.checked_add(1024)).and_then(|n|n.checked_mul(levels)).unwrap_or(usize::MAX);ops.before_owned(n)}
fn publication_slot<V,O:TaskReductionOpsV1>(values:&Vec<V>,ops:&O)->Result<(),O::Error>{if values.len()==values.capacity(){let n=values.capacity().checked_mul(2).unwrap_or(usize::MAX).max(4).checked_mul(std::mem::size_of::<V>()).unwrap_or(usize::MAX);ops.before_owned(n)?;ops.work(1,values.len().saturating_mul(std::mem::size_of::<V>()))?;}Ok(())}
struct SortStopped;
fn sort_publications<V,O:TaskReductionOpsV1>(values:&mut[(String,u64,V)],ops:&O)->Result<(),O::Error>{
 ops.before_owned(values.len().saturating_mul(std::mem::size_of::<(String,u64,V)>()))?;
 let mut first=None;let outcome=std::panic::catch_unwind(std::panic::AssertUnwindSafe(||values.sort_by(|a,b|{if let Err(error)=ops.work(1,a.0.len()+b.0.len()){first=Some(error);std::panic::panic_any(SortStopped)}(&a.0,a.1).cmp(&(&b.0,b.1))})));
 if let Some(error)=first{return Err(error)}if let Err(panic)=outcome{std::panic::resume_unwind(panic)}Ok(())
}
pub(crate) fn reduce_with_work_v1<V: std::borrow::Borrow<CanonicalIntentResultV1>, O:TaskReductionOpsV1>(results:Vec<V>,ops:&O)->Result<BTreeMap<String,ReducedTask<V>>,O::Error>{
    ops.before_owned(std::mem::size_of::<BTreeMap<String,ReducedTask<V>>>()+std::mem::size_of::<BTreeMap<String,String>>()+std::mem::size_of::<Vec<(String,u64,V)>>())?;
    let mut tasks = BTreeMap::new();
    let mut scopes = BTreeMap::new();
    let mut publishes = Vec::new();
    for value in results {
        ops.work(1,0)?;
        let result=value.borrow();
        if result.rejected.is_some() {
            continue;
        }
        match &result.request.signed_payload {
            WorldServicePayloadV1::GameTaskCreate(_) => {
                let control = ops.create_control(&result.request)?;
                let scope = ops.digest(
                    "game-task-history-scope",
                    &(
                        &control.world_id,
                        &control.genesis_digest,
                        &control.agent_id,
                        &control.owner_public_key,
                        &control.owner_binding,
                        control.owner_generation,
                        &control.branch_id,
                        control.reorg_epoch,
                    ),
                )?;
                tree_work(&scopes,&scope,ops)?;
                insert_slots::<String,String,_>(scopes.len(),ops)?;
                if scopes.insert(scope, ops.copy(&control.task_id)?).is_some() {
                    return Err(ops.business("duplicate canonical task owner/history scope"));
                }

                tree_work(&tasks,&control.task_id,ops)?;
                insert_slots::<String,ReducedTask<V>,_>(tasks.len(),ops)?;
                if tasks
                    .insert(
                        ops.copy(&control.task_id)?,
                        ReducedTask {
                            control,
                            publish: None,
                            last_height: result.committed_height,
                        },
                    )
                    .is_some()
                {
                    return Err(ops.business("duplicate canonical task identity"));
                }
            }
            WorldServicePayloadV1::GameExecutorControl(signed) => { publication_slot(&publishes,ops)?;publishes.push((
                ops.copy(&signed.request.fence.task_id)?,
                signed.request.fence.task_revision.0,
                value,
            ));},
            WorldServicePayloadV1::GameGoalPublish(signed) => { publication_slot(&publishes,ops)?;publishes.push((
                ops.copy(&signed.request.task_id)?,
                signed.request.expected_task_revision.0,
                value,
            ));},
            _ => (),
        }
    }
    sort_publications(&mut publishes,ops)?;
    for (_, _, value) in publishes {
        ops.work(1,0)?;
        let result=value.borrow();
        if let WorldServicePayloadV1::GameExecutorControl(signed) = &result.request.signed_payload {
            tree_work(&tasks,&signed.request.fence.task_id,ops)?;
            let task = tasks
                .get_mut(&signed.request.fence.task_id)
                .ok_or_else(||ops.business("executor has no canonical task predecessor"))?;
            ops.control_comparison(&task.control)?;
            if oasis7_agent_api::game::GameTaskFenceV1::from_control(&task.control)
                != signed.request.fence
            {
                return Err(ops.business("executor predecessor or owner fence mismatch"));
            }
            if task.last_height > result.committed_height {
                return Err(ops.business("executor control heights regress"));
            }
            executor::apply_executor_receipt(
                &mut task.control,
                ops.executor_receipt(&result.request)?,
            );
            task.last_height = result.committed_height;
            continue;
        }
        let WorldServicePayloadV1::GameGoalPublish(signed) = &result.request.signed_payload else {
            unreachable!()
        };
        tree_work(&tasks,&signed.request.task_id,ops)?;
        let task = tasks
            .get_mut(&signed.request.task_id)
            .ok_or_else(||ops.business("goal publication has no canonical task predecessor"))?;
        ops.control_comparison(&task.control)?;
        if !matches_control(&signed.request, &task.control) {
            return Err(ops.business("goal publication predecessor or owner fence mismatch"));
        }
        let receipt = ops.publish_receipt(&result.request, result.committed_height)?;
        if task.last_height > result.committed_height {
            return Err(ops.business("goal publication heights regress"));
        }
        task.control.task_revision = receipt.task_revision;
        task.control.latest_goal_revision = receipt.goal_reference.goal_revision;
        task.control.active_goal_revision = receipt.goal_reference.goal_revision;
        task.control.active_goal_reference = Some(receipt.goal_reference);
        task.last_height = result.committed_height;
        task.publish = Some(value);
    }
    Ok(tasks)
}
pub(super) struct Legacy;
impl TaskReductionOpsV1 for Legacy{
 type Error=String;
 fn is_metered(&self)->bool{false}
 fn work(&self,_:u64,_:usize)->Result<(),String>{Ok(())}fn before_owned(&self,_:usize)->Result<(),String>{Ok(())}fn business(&self,note:&'static str)->String{note.into()}fn copy(&self,text:&str)->Result<String,String>{Ok(text.to_owned())}
 fn digest<T:serde::Serialize>(&self,domain:&str,value:&T)->Result<String,String>{request_digest(domain,value)}
 fn create_control(&self,request:&SubmitIntentRequest<WorldServicePayloadV1>)->Result<GameTaskControlV1,String>{create_control(request)}
 fn executor_receipt(&self,request:&SubmitIntentRequest<WorldServicePayloadV1>)->Result<executor::ExecutorReceipt,String>{executor::executor_receipt(request)}
 fn publish_receipt(&self,request:&SubmitIntentRequest<WorldServicePayloadV1>,height:u64)->Result<oasis7_client_api::legacy_task_results::PublishReceipt,String>{publish_receipt(request,height)}
 fn control_comparison(&self,_:&GameTaskControlV1)->Result<(),String>{Ok(())}
}
