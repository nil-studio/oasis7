//! Fail closed until every dirty semantic State family has a real typed
//! projection reader. This routing gate never replaces any family validator.
use super::*;
use oasis7_formal_protocol::{incremental_execution::{ExecutionTransaction,RootSetV1},incremental_state::MerkleNodeReader};
use oasis7_world_state::{incremental_runtime_revocation::BusinessDiffProcessingWorkV1,incremental_world_field_key::WorldFieldKeyV1};
use std::cell::RefCell;
fn pay(work:&RefCell<&mut BusinessDiffProcessingWorkV1>,bytes:usize,stop:&RefCell<&mut dyn FnMut()->bool>)->Result<(),BodyMutationError>{if(stop.borrow_mut())(){return Err(ValueTreeError::Cancelled.into())}let n=u64::try_from(bytes).map_err(|_|invalid("projection coverage work layout"))?;let mut work=work.borrow_mut();work.steps_remaining=work.steps_remaining.checked_sub(n.checked_add(1).ok_or_else(||invalid("projection coverage steps layout"))?).ok_or_else(||BodyMutationError::Value(ValueTreeError::ResourceLimited("projection coverage steps")))?;work.bytes_remaining=work.bytes_remaining.checked_sub(n).ok_or_else(||BodyMutationError::Value(ValueTreeError::ResourceLimited("projection coverage bytes")))?;Ok(())}
pub(super) fn validate_actual_state_write_coverage_v1<M:MerkleNodeReader>(roots:&RootSetV1,tx:&ExecutionTransaction<'_,M>,pending:&mut usize,processing:&mut BusinessDiffProcessingWorkV1,stop:&mut dyn FnMut()->bool)->Result<(),BodyMutationError>{
 let work=RefCell::new(processing);let stop=RefCell::new(stop);
 for root in &roots.state{pay(&work,0,&stop)?;tx.visit_pending_writes(root.namespace,pending,||pay(&work,0,&stop),|key,before,after|{
  pay(&work,key.len(),&stop)?;if before==after||!WorldFieldKeyV1::is_field_key_v1(key){return Ok(())}
  let parsed=WorldFieldKeyV1::from_bytes(key).map_err(invalid)?;if parsed.owner!=FieldOwner::WorldState{return Ok(())}
  if parsed.namespace!=root.namespace{return Err(invalid("projection State key namespace"))}
  pay(&work,parsed.name.len(),&stop)?;
  if oasis7_world_state::incremental_module_marketplace::ModuleMarketplaceFieldV1::from_name(parsed.name).is_some()||parsed.name=="world_rules"||oasis7_world_state::incremental_reward_budget_records::is_reward_projection_field_v1(parsed.name)||oasis7_world_state::incremental_diplomacy::is_diplomacy_projection_field_v1(parsed.name)||oasis7_world_state::incremental_governance_records::is_governance_projection_field_v1(parsed.name)||oasis7_world_state::incremental_world_collection_controls::is_collection_controls_projection_field_v1(parsed.name)||oasis7_world_state::incremental_main_token_policy_records::is_main_token_policy_projection_field_v1(parsed.name)||oasis7_world_state::incremental_material_records::is_material_projection_field_v1(parsed.name)||oasis7_world_state::incremental_fixed_economy_records::is_fixed_economy_projection_field_v1(parsed.name)||oasis7_world_state::incremental_agent_authority_records::is_agent_authority_projection_field_v1(parsed.name)||oasis7_world_state::incremental_factory_authority_records::is_factory_authority_projection_field_v1(parsed.name)||oasis7_world_state::incremental_world_rule_projection_delta::is_native_projection_field_v1(parsed.name)||oasis7_world_state::incremental_industry_projection_delta::is_industry_projection_field_v1(parsed.name)||oasis7_world_state::incremental_inventory_projection_delta::is_inventory_projection_field_v1(parsed.name){return Ok(())}
  for descriptor in semantic_state_columns_v1(){pay(&work,parsed.name.len().checked_add(descriptor.name.len()).ok_or_else(||invalid("projection field comparison layout"))?,&stop)?;if descriptor.name==parsed.name&&descriptor.owner==parsed.owner{if oasis7_world_state::incremental_owner_closure::is_primitive_projection_field_v1(*descriptor){return Ok(())}break}}
  Err(invalid("dirty State family has no typed projection source"))
 })?}
 Ok(())
}
