//! Fresh current task authority for an immutable historical business lookup.
use super::*;
pub(super) use oasis7_client_api::world_service::entry_executor::EXECUTOR_LOOKUP_PATH;
use oasis7_client_api::world_service::entry_executor::EntryExecutorLookupRequestV1;
use oasis7_client_api::world_service_wire::{WorldServicePayloadV1 as NativePayload,SchedulerOperationV1 as NativeScheduler};
use oasis7_agent_api::game::GameTaskFenceV1;

pub(super) fn lookup(service:&Service<'_>,request:EntryExecutorLookupRequestV1)->Result<Value,String> {
    let access=&request.access;let fence=&access.fence;
    fence.validate()?;
    access.read.request.validate().map_err(|e|e.to_string())?;
    verify_read_request(oasis7_client_api::world_service::VIEW_JSON_PATH,&access.read)?;
    if access.read.request.world!=service.identity || access.read.request.scope_id!=format!("agent:{}",fence.agent_id)
        || access.read.request.fixed_commit.is_some() || access.read.request.formal_anchor.is_some() || access.read.request.observation_nonce.is_none()
        || fence.world_id!=service.identity.world_id || fence.genesis_digest!=service.identity.genesis_digest
        || access.read.subject_public_key==fence.owner_public_key {return Err("executor lookup current access mismatch".into());}
    let pinned=service.pin(None)?;
    if let Some(minimum)=&access.read.request.min_commit && !pinned.commit.satisfies_minimum(minimum).map_err(|e|e.to_string())? {return Err("executor lookup read not caught up".into());}
    let control=game_task::current_control(&pinned.world,&fence.agent_id)?.ok_or("executor lookup task missing")?;
    if GameTaskFenceV1::from_control(&control)!=*fence
        || control.executor_binding.as_ref().is_none_or(|b|b.executor_public_key!=access.read.subject_public_key) {
        return Err("executor lookup current fence mismatch".into());
    }
    game_task::validate_task_reader(&pinned.world,fence,&access.read.subject_public_key)?;
    let (actor,historical)=match &request.lookup.original {
        NativePayload::Cognition(s)=>(&s.subject_public_key,s.request.task_fence.as_ref().ok_or("lookup cognition needs task fence")?),
        NativePayload::Scheduler(s)=>{
            let context=match &s.request.operation {NativeScheduler::TaskProviderPrefix{context}|NativeScheduler::TaskProviderFailure{context,..}=>context,_=>return Err("lookup scheduler needs task context".into())};
            (&s.subject_public_key,&context.fence)
        },
        _=>return Err("executor lookup operation not allowed".into()),
    };
    if actor!=&access.read.subject_public_key || !same_resource(historical,fence) {return Err("executor lookup historical resource mismatch".into());}
    request.lookup.request.validate().map_err(|e|e.to_string())?;
    let correlation=oasis7_client_api::world_service_correlation::derive_correlation(service.identity.clone(),&request.lookup.original)?;
    if correlation.key!=request.lookup.request.key {return Err("executor lookup original key mismatch".into());}
    let outcome=operations::outcome(service,&pinned,&correlation)?;
    service.signed(LOOKUP_PATH,&request.lookup,IntentResponse{formal_profile:None,
        execution_evidence_scope:service.guarded.map(|_|oasis7::world_service::verified_view::CONTROLLED_PREREQUISITE_READ_SCOPE.into()),
        contract_version:WORLD_SERVICE_CONTRACT_VERSION,correlation,outcome})
}
fn same_resource(a:&GameTaskFenceV1,b:&GameTaskFenceV1)->bool {
    a.world_id==b.world_id && a.genesis_digest==b.genesis_digest && a.agent_id==b.agent_id && a.task_id==b.task_id
        && a.owner_binding==b.owner_binding && a.owner_generation==b.owner_generation && a.owner_public_key==b.owner_public_key
}
