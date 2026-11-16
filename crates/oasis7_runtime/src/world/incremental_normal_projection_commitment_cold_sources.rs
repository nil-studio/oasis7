//! Fixed actual source capture. Each owner retains its real typed read cut;
//! data folding happens after the single Common IO child has closed.
use super::*;
use crate::world::incremental_module_invocation::*;
use oasis7_formal_protocol::{incremental_execution::ExecutionFence,incremental_material::OriginalObjectStreamReader,incremental_state::{MerkleNodeReader,StatePageWriter},incremental_value::{ValueNodeReader,ValueNodeWriter}};
use oasis7_world_state::incremental_field_read::WorldFieldReadContextV1;
use oasis7_world_state::{incremental_world_rule_projection_delta as native,incremental_owner_closure as primitive,incremental_inventory_projection_delta as inventory,incremental_industry_projection_delta as industry,incremental_agent_instance_projection_delta as agents,incremental_module_state_projection_delta as module_state};
pub(super) struct QualifiedNormalStateSourcesV1{
 pub(super) rules:native::QualifiedWorldRuleProjectionColumnsV1,
 pub(super) marketplace:oasis7_world_state::incremental_owner_closure::QualifiedMarketplaceProjectionColumnsV1,
 pub(super) reward:oasis7_world_state::incremental_reward_budget_records::QualifiedRewardProjectionColumnsV1,
 pub(super) diplomacy:oasis7_world_state::incremental_diplomacy::QualifiedDiplomacyProjectionColumnsV1,
 pub(super) governance:oasis7_world_state::incremental_governance_records::QualifiedGovernanceProjectionColumnsV1,
 pub(super) controls:oasis7_world_state::incremental_world_collection_controls::QualifiedCollectionControlsProjectionColumnsV1,
 pub(super) main_policy:oasis7_world_state::incremental_main_token_policy_records::QualifiedMainTokenPolicyProjectionColumnsV1,
 pub(super) materials:oasis7_world_state::incremental_material_records::QualifiedMaterialProjectionColumnsV1,
 pub(super) fixed:oasis7_world_state::incremental_fixed_economy_records::QualifiedFixedEconomyProjectionColumnsV1,
 pub(super) native:native::QualifiedNativeProjectionColumnsV1,
 pub(super) primitive:primitive::QualifiedPrimitiveProjectionColumnsV1,
 pub(super) inventory:inventory::QualifiedInventoryProjectionColumnsV1,
 pub(super) industry:industry::QualifiedIndustryProjectionColumnsV1,
 pub(super) agent_authority:oasis7_world_state::incremental_agent_authority_records::QualifiedAgentAuthorityProjectionColumnsV1,
 pub(super) factory_authority:oasis7_world_state::incremental_factory_authority_records::QualifiedFactoryAuthorityProjectionColumnsV1,
 pub(super) agents:agents::QualifiedAgentInstanceProjectionColumnsV1,
 pub(super) module_states:module_state::QualifiedModuleStateProjectionColumnsV1,
 _fee:MemoryReservation,
}
fn overlay(error:oasis7_world_state::incremental_field_overlay::WorldFieldOverlayErrorV1)->BodyMutationError{match error{oasis7_world_state::incremental_field_overlay::WorldFieldOverlayErrorV1::Execution(error)=>BodyMutationError::Execution(error),oasis7_world_state::incremental_field_overlay::WorldFieldOverlayErrorV1::Value(error)=>BodyMutationError::Value(error),other=>paid(other)}}
impl<M,R,E,C,W,F,O> RootModuleExecutionContextV1<'_, '_, '_, '_,M,R,E,C,W,F,O>
where M:MerkleNodeReader,R:ValueNodeReader,E:FnMut()->Option<ValueTreeError>,C:FnMut()->bool,W:ValueNodeReader+ValueNodeWriter+StatePageWriter,F:FnMut(&mut dyn crate::world::incremental_body_event::AgentEventOriginalV1)->Result<(),BodyMutationError>,O:OriginalObjectStreamReader{
 pub(super) fn capture_qualified_projection_state_sources_v1<P:MerkleNodeReader,V:ValueNodeReader,PE:FnMut()->Option<ValueTreeError>,PC:FnMut()->bool>(
 &mut self,parent:&crate::world::incremental_normal_sources::BoundNormalTickParentV1<'_>,read:&mut WorldFieldReadContextV1<'_,P,V,PE,PC>,work:&mut NormalProjectionCurrentWorkV1<'_>,common:&mut FormalBoundedContextV1<'_>,live:&mut dyn FnMut()->ExecutionFence,
 )->Result<QualifiedNormalStateSourcesV1,RootModuleExecutionErrorV1<O::Error>>{
 parent.validate_context(common).map_err(paid)?;let refs=parent.refs();let current=refs.storage.current();let mut original_error=None;
 let result=work.with_source_io_v1(self.ctx,read,common,|ctx,read,work|{
 ctx.transaction.validate_bootstrap_read_only_v1(&live())?;
 let rules=native::read_qualified_world_rule_projection_columns_v1(current.native().rules(),refs.roots,read.state,ctx,work.scan,work.runtime_inner_scan,work.typed,work.processing,&live()).map_err(overlay)?;
 let native=native::read_qualified_native_projection_columns_v1(current.native().native(),refs.roots,read,work.scan,work.runtime_inner_scan,work.typed,work.processing)?;
 let primitive=primitive::read_qualified_primitive_projection_columns_v1(current.native().primitive(),refs.roots,read,work.scan,work.primitives,work.processing).map_err(overlay)?;
 let inventory=inventory::read_qualified_inventory_projection_columns_v1(refs.roots,current.native().inventory(),read,work.scan,work.inventory,work.processing)?;
 let industry=industry::read_qualified_industry_projection_columns_v1(refs.roots,current.native().factory(),current.native().industry(),current.native().logistics(),read,work.scan,work.factory,work.industry,work.logistics,work.processing)?;
 let guard=ctx.transaction.guard();let max_text=ctx.record_bytes;let marketplace=oasis7_world_state::incremental_owner_closure::read_qualified_marketplace_projection_columns_v1(current.native().market(),refs.roots,read.state,ctx,work.scan,&mut work.typed.json.cells_remaining,work.processing,max_text,&guard,&live())?;
 let guard=ctx.transaction.guard();let max_text=ctx.record_bytes;let reward=oasis7_world_state::incremental_reward_budget_records::read_qualified_reward_projection_columns_v1(current.native().reward(),refs.roots,read.state,ctx,work.scan,&mut work.typed.json.cells_remaining,work.processing,max_text,&guard,&live())?;
 let guard=ctx.transaction.guard();let max_text=ctx.record_bytes;let diplomacy=oasis7_world_state::incremental_diplomacy::read_qualified_diplomacy_projection_columns_v1(current.native().diplomacy(),refs.roots,read.state,ctx,work.scan,&mut work.typed.json.cells_remaining,work.processing,max_text,&guard,&live())?;
 let guard=ctx.transaction.guard();let max_text=ctx.record_bytes;let governance=oasis7_world_state::incremental_governance_records::read_qualified_governance_projection_columns_v1(current.native().governance(),refs.roots,read.state,ctx,work.scan,&mut work.typed.json.cells_remaining,work.processing,max_text,&guard,&live())?;
 let guard=ctx.transaction.guard();let controls=oasis7_world_state::incremental_world_collection_controls::read_qualified_collection_controls_projection_columns_v1(current.native().controls(),refs.roots,read.state,ctx,work.scan,work.controls,work.processing,&guard,&live())?;
 let guard=ctx.transaction.guard();let max_text=ctx.record_bytes;let main_policy=oasis7_world_state::incremental_main_token_policy_records::read_qualified_main_token_policy_projection_columns_v1(current.native().main_policy(),refs.roots,read.state,ctx,work.scan,&mut work.typed.json.cells_remaining,work.processing,max_text,&guard,&live())?;
 let guard=ctx.transaction.guard();let max_text=ctx.record_bytes;let materials=oasis7_world_state::incremental_material_records::read_qualified_material_projection_columns_v1(current.native().material(),refs.roots,read.state,ctx,work.scan,&mut work.typed.json.cells_remaining,work.processing,max_text,&guard,&live())?;
 let guard=ctx.transaction.guard();let fixed=oasis7_world_state::incremental_fixed_economy_records::read_qualified_fixed_economy_projection_columns_v1(current.native().fixed(),refs.roots,read.state,ctx,work.scan,&mut work.typed.json.cells_remaining,work.processing,&guard,&live())?;
 let guard=ctx.transaction.guard();let max_text=ctx.record_bytes;let factory_authority=oasis7_world_state::incremental_factory_authority_records::read_qualified_factory_authority_projection_columns_v1(current.native().factory_authority(),refs.roots,read.state,ctx,work.scan,&mut work.typed.json.cells_remaining,work.processing,max_text,&guard,&live())?;
 let guard=ctx.transaction.guard();let max_text=ctx.record_bytes;let agent_authority=oasis7_world_state::incremental_agent_authority_records::read_qualified_agent_authority_projection_columns_v1(current.native().agent_authority(),refs.roots,read.state,ctx,work.scan,&mut work.typed.json.cells_remaining,work.processing,max_text,&guard,&live())?;
 let guard=ctx.transaction.guard();let agents=match agents::read_qualified_agent_instance_projection_columns_v1(current.raw().agents(),current.raw().instances(),refs.roots,ctx,read,work.scan,work.processing,&mut*self.originals,work.originals,work.events,&live()){
 Ok(value)=>value,Err(agents::AgentInstanceProjectionErrorV1::Value(error))=>return Err(BodyMutationError::Value(error)),Err(agents::AgentInstanceProjectionErrorV1::Overlay(error))=>return Err(overlay(error)),Err(agents::AgentInstanceProjectionErrorV1::Body(error))=>return Err(error),Err(agents::AgentInstanceProjectionErrorV1::Original(error))=>{original_error=Some(error);return Err(invalid("projection actual Original source transport"))}};
 let guard=ctx.transaction.guard();let module_states=module_state::read_qualified_module_state_projection_columns_v1(current.raw().module_states(),refs.roots,read.state,ctx,work.scan,work.module_states,work.processing,&guard,&live())?;
 Ok((rules,marketplace,reward,diplomacy,governance,controls,main_policy,materials,fixed,native,primitive,inventory,industry,factory_authority,agent_authority,agents,module_states))
 });
 if let Some(error)=original_error{return Err(RootModuleExecutionErrorV1::ArtifactOriginalStorage(error))}
 let(rules,marketplace,reward,diplomacy,governance,controls,main_policy,materials,fixed,native,primitive,inventory,industry,factory_authority,agent_authority,agents,module_states)=result?;let n=std::mem::size_of::<QualifiedNormalStateSourcesV1>();common.charge(1,n as u64).map_err(paid)?;let fee=common.memory().reserve(n,MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;
 Ok(QualifiedNormalStateSourcesV1{rules,marketplace,reward,diplomacy,governance,controls,main_policy,materials,fixed,native,primitive,inventory,industry,factory_authority,agent_authority,agents,module_states,_fee:fee})
 }
}

impl QualifiedNormalStateSourcesV1{
 pub(super) fn validate_v1<M:MerkleNodeReader>(&self,roots:&oasis7_formal_protocol::incremental_execution::RootSetV1,tx:&oasis7_formal_protocol::incremental_execution::ExecutionTransaction<'_,M>,common:&mut FormalBoundedContextV1<'_>,live:&ExecutionFence)->Result<(),BodyMutationError>{
 tx.validate_bootstrap_read_only_v1(live)?;if !self._fee.uses_budget(common.memory()){return Err(invalid("qualified complete State source pool"))}
 self.rules.validate_context(roots,common.memory())?;self.native.validate_context(roots,common.memory())?;self.primitive.validate_context(roots,common.memory())?;self.inventory.validate_context(roots,common.memory())?;self.industry.validate_context(roots,common.memory())?;
 common.consume_steps(1).map_err(paid)?;self.marketplace.validate(tx,common.memory(),live)?;
 common.consume_steps(1).map_err(paid)?;self.reward.validate(tx,common.memory(),live)?;
 common.consume_steps(1).map_err(paid)?;self.diplomacy.validate(tx,common.memory(),live)?;
 common.consume_steps(1).map_err(paid)?;self.governance.validate(tx,common.memory(),live)?;
 common.consume_steps(1).map_err(paid)?;self.controls.validate(tx,common.memory(),live)?;
 common.consume_steps(1).map_err(paid)?;self.main_policy.validate(tx,common.memory(),live)?;
 common.consume_steps(1).map_err(paid)?;self.materials.validate(tx,common.memory(),live)?;
 common.consume_steps(1).map_err(paid)?;self.fixed.validate(tx,common.memory(),live)?;
 common.consume_steps(1).map_err(paid)?;self.agent_authority.validate(tx,common.memory(),live)?;
 common.consume_steps(1).map_err(paid)?;self.factory_authority.validate(tx,common.memory(),live)?;
 common.consume_steps(1).map_err(paid)?;self.agents.validate(tx,common.memory(),live).map_err(|e|match e{oasis7_world_state::incremental_field_overlay::WorldFieldOverlayErrorV1::Execution(e)=>BodyMutationError::Execution(e),oasis7_world_state::incremental_field_overlay::WorldFieldOverlayErrorV1::Value(e)=>BodyMutationError::Value(e),oasis7_world_state::incremental_field_overlay::WorldFieldOverlayErrorV1::Member(e)=>BodyMutationError::Original(Box::new(e))})?;
 common.consume_steps(1).map_err(paid)?;self.module_states.validate(tx,common.memory(),live)?;
Ok(())
}
}