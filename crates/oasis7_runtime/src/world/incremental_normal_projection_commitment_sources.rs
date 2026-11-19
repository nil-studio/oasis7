//! Fixed actual source capture. Each owner retains its real typed read cut;
//! data folding happens after the single Common IO child has closed.
use super::*;
use crate::world::incremental_module_invocation::*;
use oasis7_formal_protocol::{incremental_execution::ExecutionFence,incremental_material::OriginalObjectStreamReader,incremental_state::{MerkleNodeReader,StatePageWriter},incremental_value::{ValueNodeReader,ValueNodeWriter}};
use oasis7_world_state::incremental_field_read::WorldFieldReadContextV1;
use oasis7_world_state::{incremental_world_rule_projection_delta as native,incremental_owner_closure as primitive,incremental_inventory_projection_delta as inventory,incremental_industry_projection_delta as industry,incremental_agent_instance_projection_delta as agents,incremental_module_state_projection_delta as module_state};
pub(super) struct ActualNormalStateSourcesV1{
 pub(super) marketplace:oasis7_world_state::incremental_owner_closure::CurrentMarketplaceProjectionDeltaV1,
 pub(super) reward:oasis7_world_state::incremental_reward_budget_records::CurrentRewardProjectionDeltaV1,
 pub(super) diplomacy:oasis7_world_state::incremental_diplomacy::CurrentDiplomacyProjectionDeltaV1,
 pub(super) governance:oasis7_world_state::incremental_governance_records::CurrentGovernanceProjectionDeltaV1,
 pub(super) controls:oasis7_world_state::incremental_world_collection_controls::CurrentCollectionControlsProjectionDeltaV1,
 pub(super) main_policy:oasis7_world_state::incremental_main_token_policy_records::CurrentMainTokenPolicyProjectionDeltaV1,
 pub(super) materials:oasis7_world_state::incremental_material_records::CurrentMaterialProjectionDeltaV1,
 pub(super) fixed:oasis7_world_state::incremental_fixed_economy_records::CurrentFixedEconomyProjectionDeltaV1,
 pub(super) native:native::CurrentNativeProjectionDeltaV1,
 pub(super) primitive:primitive::CurrentPrimitiveProjectionDeltaV1,
 pub(super) inventory:inventory::CurrentInventoryProjectionDeltaV1,
 pub(super) industry:industry::CurrentIndustryProjectionDeltaV1,
 pub(super) agent_authority:oasis7_world_state::incremental_agent_authority_records::CurrentAgentAuthorityProjectionDeltaV1,
 pub(super) factory_authority:oasis7_world_state::incremental_factory_authority_records::CurrentFactoryAuthorityProjectionDeltaV1,
 pub(super) agents:agents::CurrentAgentInstanceProjectionDeltaV1,
 pub(super) module_states:module_state::CurrentModuleStateProjectionDeltaV1,
 _fee:MemoryReservation,
}
fn overlay(error:oasis7_world_state::incremental_field_overlay::WorldFieldOverlayErrorV1)->BodyMutationError{match error{oasis7_world_state::incremental_field_overlay::WorldFieldOverlayErrorV1::Execution(error)=>BodyMutationError::Execution(error),oasis7_world_state::incremental_field_overlay::WorldFieldOverlayErrorV1::Value(error)=>BodyMutationError::Value(error),other=>paid(other)}}
impl<M,R,E,C,W,F,O> RootModuleExecutionContextV1<'_, '_, '_, '_,M,R,E,C,W,F,O>
where M:MerkleNodeReader,R:ValueNodeReader,E:FnMut()->Option<ValueTreeError>,C:FnMut()->bool,W:ValueNodeReader+ValueNodeWriter+StatePageWriter,F:FnMut(&mut dyn crate::world::incremental_body_event::AgentEventOriginalV1)->Result<(),BodyMutationError>,O:OriginalObjectStreamReader{
 pub(super) fn capture_normal_projection_state_sources_v1<P:MerkleNodeReader,V:ValueNodeReader,PE:FnMut()->Option<ValueTreeError>,PC:FnMut()->bool>(
 &mut self,parent:&crate::world::incremental_normal_sources::BoundNormalTickParentV1<'_>,read:&mut WorldFieldReadContextV1<'_,P,V,PE,PC>,work:&mut NormalProjectionCurrentWorkV1<'_>,common:&mut FormalBoundedContextV1<'_>,live:&mut dyn FnMut()->ExecutionFence,
 )->Result<ActualNormalStateSourcesV1,RootModuleExecutionErrorV1<O::Error>>{
 parent.validate_context(common).map_err(paid)?;let refs=parent.refs();let current=refs.storage.current();let mut original_error=None;
 let result=work.with_source_io_v1(self.ctx,read,common,|ctx,read,work|{
 super::coverage::validate_actual_state_write_coverage_v1(refs.roots,ctx.transaction,work.pending_entries,work.processing,&mut ctx.cancelled)?;
 let guard=ctx.transaction.guard();let native=native::read_current_native_projection_delta_v1(current.native().native(),refs.roots,read.state,ctx,work.scan,work.typed,work.pending_entries,work.processing,&guard,&live()).map_err(overlay)?;
 let guard=ctx.transaction.guard();let primitive=primitive::read_current_primitive_projection_delta_v1(current.native().primitive(),refs.roots,ctx,work.primitives,work.pending_entries,work.processing,&guard,&live()).map_err(overlay)?;
 let guard=ctx.transaction.guard();let inventory=inventory::read_current_inventory_projection_delta_v1(refs.roots,current.native().inventory(),ctx,work.inventory,work.processing,work.pending_entries,&guard,&live()).map_err(overlay)?;
 let guard=ctx.transaction.guard();let industry=industry::read_current_industry_projection_delta_v1(refs.roots,current.native().factory(),current.native().industry(),current.native().logistics(),ctx,work.factory,work.industry,work.logistics,work.processing,work.pending_entries,&guard,&live()).map_err(overlay)?;
 let guard=ctx.transaction.guard();let max_text=ctx.record_bytes;let marketplace=oasis7_world_state::incremental_owner_closure::read_current_marketplace_projection_delta_v1(current.native().market(),refs.roots,read.state,ctx,work.scan,&mut work.typed.json.cells_remaining,work.pending_entries,work.processing,max_text,&guard,&live())?;
 let guard=ctx.transaction.guard();let max_text=ctx.record_bytes;let reward=oasis7_world_state::incremental_reward_budget_records::read_current_reward_projection_delta_v1(current.native().reward(),refs.roots,read.state,ctx,work.scan,&mut work.typed.json.cells_remaining,work.pending_entries,work.processing,max_text,&guard,&live())?;
 let guard=ctx.transaction.guard();let max_text=ctx.record_bytes;let diplomacy=oasis7_world_state::incremental_diplomacy::read_current_diplomacy_projection_delta_v1(current.native().diplomacy(),refs.roots,read.state,ctx,work.scan,&mut work.typed.json.cells_remaining,work.pending_entries,work.processing,max_text,&guard,&live())?;
 let guard=ctx.transaction.guard();let max_text=ctx.record_bytes;let governance=oasis7_world_state::incremental_governance_records::read_current_governance_projection_delta_v1(current.native().governance(),refs.roots,read.state,ctx,work.scan,&mut work.typed.json.cells_remaining,work.pending_entries,work.processing,max_text,&guard,&live())?;
 let guard=ctx.transaction.guard();let controls=oasis7_world_state::incremental_world_collection_controls::read_current_collection_controls_projection_delta_v1(current.native().controls(),refs.roots,read.state,ctx,work.controls,work.pending_entries,work.processing,&guard,&live())?;
 let guard=ctx.transaction.guard();let max_text=ctx.record_bytes;let main_policy=oasis7_world_state::incremental_main_token_policy_records::read_current_main_token_policy_projection_delta_v1(current.native().main_policy(),refs.roots,read.state,ctx,work.scan,&mut work.typed.json.cells_remaining,work.pending_entries,work.processing,max_text,&guard,&live())?;
 let guard=ctx.transaction.guard();let max_text=ctx.record_bytes;let materials=oasis7_world_state::incremental_material_records::read_current_material_projection_delta_v1(current.native().material(),refs.roots,read.state,ctx,work.scan,&mut work.typed.json.cells_remaining,work.pending_entries,work.processing,max_text,&guard,&live())?;
 let guard=ctx.transaction.guard();let fixed=oasis7_world_state::incremental_fixed_economy_records::read_current_fixed_economy_projection_delta_v1(current.native().fixed(),refs.roots,read.state,ctx,work.scan,&mut work.typed.json.cells_remaining,work.pending_entries,work.processing,&guard,&live())?;
 let guard=ctx.transaction.guard();let max_text=ctx.record_bytes;let factory_authority=oasis7_world_state::incremental_factory_authority_records::read_current_factory_authority_projection_delta_v1(current.native().factory_authority(),refs.roots,read.state,ctx,work.scan,&mut work.typed.json.cells_remaining,work.pending_entries,work.processing,max_text,&guard,&live())?;
 let guard=ctx.transaction.guard();let max_text=ctx.record_bytes;let agent_authority=oasis7_world_state::incremental_agent_authority_records::read_current_agent_authority_projection_delta_v1(current.native().agent_authority(),refs.roots,read.state,ctx,work.scan,&mut work.typed.json.cells_remaining,work.pending_entries,work.processing,max_text,&guard,&live())?;
 let guard=ctx.transaction.guard();let agents=match agents::read_current_agent_instance_projection_delta_v1(current.raw().agents(),current.raw().instances(),refs.roots,ctx,work.pending_entries,work.processing,&mut*self.originals,work.originals,work.events,&guard,&live()){
 Ok(value)=>value,Err(agents::AgentInstanceProjectionErrorV1::Value(error))=>return Err(BodyMutationError::Value(error)),Err(agents::AgentInstanceProjectionErrorV1::Overlay(error))=>return Err(overlay(error)),Err(agents::AgentInstanceProjectionErrorV1::Body(error))=>return Err(error),Err(agents::AgentInstanceProjectionErrorV1::Original(error))=>{original_error=Some(error);return Err(invalid("projection actual Original source transport"))}};
 let guard=ctx.transaction.guard();let module_states=module_state::read_current_module_state_projection_delta_v1(current.raw().module_states(),ctx,work.module_states,work.pending_entries,work.processing,&guard,&live())?;
 Ok((marketplace,reward,diplomacy,governance,controls,main_policy,materials,fixed,native,primitive,inventory,industry,factory_authority,agent_authority,agents,module_states))
 });
 if let Some(error)=original_error{return Err(RootModuleExecutionErrorV1::ArtifactOriginalStorage(error))}
 let(marketplace,reward,diplomacy,governance,controls,main_policy,materials,fixed,native,primitive,inventory,industry,factory_authority,agent_authority,agents,module_states)=result?;let n=std::mem::size_of::<ActualNormalStateSourcesV1>();common.charge(1,n as u64).map_err(paid)?;let fee=common.memory().reserve(n,MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;
 Ok(ActualNormalStateSourcesV1{marketplace,reward,diplomacy,governance,controls,main_policy,materials,fixed,native,primitive,inventory,industry,factory_authority,agent_authority,agents,module_states,_fee:fee})
 }
}
