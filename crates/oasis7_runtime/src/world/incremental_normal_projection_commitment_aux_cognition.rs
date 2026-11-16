//! Fixed committed Some(Agent) Cognition observation. All inputs come from
//! the actual readonly column pipeline; the result is DTO DATA, not admission.
use super::*;
use super::super::{shared_json,raw_hash,state_columns::OriginalStateColumnsDataV1,original_rows::OriginalProjectionRowsDataV1};
use crate::world::incremental_normal_business_commitment::OriginalProjectionReadMetadataV1;
use crate::world::incremental_normal_process_source::PaidProjectionProcessFactsV1;
use oasis7_world_projection::{WorldServiceAgentContext,WorldServiceContinuationContext,WorldServiceProjection,current_commitment::{ProjectionColumnDataV1,ProjectionColumnRefV1,ProjectionColumnErrorV1}};
use oasis7_world_state::{AgentContinuation,SchedulerWakeV1,CognitionLeaseV1};
use std::{collections::BTreeMap,mem::size_of};
#[path="incremental_normal_projection_commitment_aux_cognition_values.rs"]mod values;
#[path="incremental_normal_projection_commitment_aux_cognition_provider.rs"]mod provider;
#[path="incremental_normal_projection_commitment_aux_cognition_scheduler.rs"]mod scheduler;
struct Fields{agent_context:Option<WorldServiceAgentContext>,cognition_leases:Vec<CognitionLeaseV1>,continuations:Vec<AgentContinuation>,scheduler_wakes:Vec<SchedulerWakeV1>,continuation_contexts:BTreeMap<String,WorldServiceContinuationContext>}
pub(in crate::world) struct CognitionProjectionFeesV1{_facts:CurrentProjectionFactsFeesV1,_economy:crate::world::cognition_economy::bounded_work::CurrentEconomyWorkFeesV1}
pub(in crate::world) struct PaidCurrentCognitionProjectionDataV1{fields:Fields,fees:CognitionProjectionFeesV1}
impl PaidCurrentCognitionProjectionDataV1{
 pub(in crate::world) fn apply_to_projection_v1(self,projection:&mut WorldServiceProjection<ProjectionColumnDataV1>)->CognitionProjectionFeesV1{let Self{fields,fees}=self;projection.agent_context=fields.agent_context;projection.cognition_leases=fields.cognition_leases;projection.continuations=fields.continuations;projection.scheduler_wakes=fields.scheduler_wakes;projection.continuation_contexts=fields.continuation_contexts;fees}
}
#[allow(clippy::too_many_arguments)]
pub(in crate::world) fn prepare_current_cognition_projection_data_v1<M:MerkleNodeReader>(state:&OriginalStateColumnsDataV1,electricity:&OriginalProjectionRowsDataV1,aux:&ProjectionColumnDataV1,metadata:&OriginalProjectionReadMetadataV1<'_>,process:&PaidProjectionProcessFactsV1,authorization:&crate::world::incremental_authorization_root_current::FrozenNormalAuthorizationProjectionV1,finality:&crate::world::incremental_normal_bootstrap_finality::CurrentBootstrapLiveFinalityV1,raw_state:&raw_hash::PaidOriginalStateHashDataV1,binding:Option<&ReservedValue<oasis7_agent_api::RuntimeBindingV1>>,agent:&str,tx:&ExecutionTransaction<'_,M>,common:&mut FormalBoundedContextV1<'_>,canonical:&mut oasis7_wasm_abi::CanonicalCborWorkV1,live:&mut dyn FnMut()->ExecutionFence)->Result<PaidCurrentCognitionProjectionDataV1,BodyMutationError>{
 raw_state.validate_context(common)?;
 let facts=CurrentProjectionReadFactsV1::for_source_data_v1(state,electricity,metadata,tx,common,live)?.with_process_facts_v1(process)?.with_authorization_facts_v1(authorization)?.with_finality_facts_v1(finality)?;
 facts.before_owned(size_of::<PaidCurrentCognitionProjectionDataV1>())?;
 let view=values::View::new(&facts,aux)?;
 // Exact original committed formatter order: optional provider context,
 // complete validated economy, raw continuations, scheduler, then contexts.
 let agent_context=provider::optional_context(&view,binding,agent,canonical)?;
 let (economy,economy_fees)=values::economy(&view,canonical)?;
 let mut cognition_leases=Vec::new();
 for lease in economy.leases.into_values(){facts.work(1,lease.agent_id.len()+agent.len())?;if lease.agent_id==agent{values::push(&facts,&mut cognition_leases,lease)?;}}
 let all_continuations=values::continuations(&view)?;
 let mut continuations=Vec::new();
 for continuation in &all_continuations{facts.work(1,continuation.agent_id.len()+agent.len())?;if continuation.agent_id==agent{view.before_clone(continuation)?;values::push(&facts,&mut continuations,continuation.clone())?;}}
 let scheduler=scheduler::current_in_flight(&view,all_continuations,canonical)?;
 let mut scheduler_wakes=Vec::new();
 for wake in scheduler{facts.work(1,wake.agent_id.len()+agent.len())?;if wake.agent_id==agent{values::push(&facts,&mut scheduler_wakes,wake)?;}}
 let mut continuation_contexts=BTreeMap::new();
 for continuation in &continuations{
  let Some(contexts)=view.optional(&["cognition","continuation_contexts"])? else{continue};
  let Some(context)=view.member_like_json_get(&contexts,&continuation.continuation_id)? else{continue};
  let context=match view.decode::<WorldServiceContinuationContext>(&context)?{Ok(v)=>v,Err(error)=>return Err(view.business(format_args!("{error}")))};
  let key=view.copy(&continuation.continuation_id)?;
  facts.work(1+11*(continuation_contexts.len()+1).ilog2()as u64,key.len())?;
  facts.before_owned(11*(size_of::<String>()+size_of::<WorldServiceContinuationContext>())+256)?;
  continuation_contexts.insert(key,context);
 }
 facts.check()?;drop(view);
 Ok(PaidCurrentCognitionProjectionDataV1{fields:Fields{agent_context,cognition_leases,continuations,scheduler_wakes,continuation_contexts},fees:CognitionProjectionFeesV1{_facts:facts.into_fees(),_economy:economy_fees}})
}

#[path="incremental_normal_projection_commitment_aux_cognition_native.rs"]
mod native;
pub(in crate::world) use native::{prepare_current_native_authorizations_data_v1,PaidCurrentNativeAuthorizationsDataV1};

#[path="incremental_normal_projection_commitment_aux_cognition_feedback.rs"]
pub(in crate::world) mod feedback;

pub(in crate::world) use native::{prepare_current_builtin_recipe_quote_data_v1,PaidCurrentBuiltinRecipeQuoteDataV1};

#[path = "incremental_normal_projection_commitment_status.rs"]
pub(super) mod status;
