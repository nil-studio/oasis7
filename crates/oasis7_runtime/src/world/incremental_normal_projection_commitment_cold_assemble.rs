//! The complete fixed qualified State column fold. All inputs are concrete
//! accepted-source owners; no caller JSON graph or root tuple initializes it.
use super::*;
use super::cold_sources::QualifiedNormalStateSourcesV1;
use crate::world::{incremental_normal_sources::BoundNormalTickParentV1,incremental_normal_projection_runtime::ActualCurrentNormalProjectionAuxV1};
use oasis7_formal_protocol::{incremental_execution::{ExecutionTransaction,ExecutionFence},incremental_state::MerkleNodeReader};
use oasis7_world_state::incremental_world_rule_projection_delta::QualifiedWorldRuleProjectionColumnsV1;
use std::{cell::RefCell,sync::Arc};
pub(super) fn assemble_qualified_projection_seed_v1<M:MerkleNodeReader>(parent:&BoundNormalTickParentV1<'_>,sources:&QualifiedNormalStateSourcesV1,aux:&ActualCurrentNormalProjectionAuxV1,retained_json:&[u8],tx:&ExecutionTransaction<'_,M>,max_json:usize,common:&mut FormalBoundedContextV1<'_>,live:&ExecutionFence)->Result<NormalProjectionColumnSeedDataV1,BodyMutationError>{
 parent.validate_context(common).map_err(paid)?;let refs=parent.refs();sources.validate_v1(refs.roots,tx,common,live)?;aux.validate(parent,tx,common,live)?;
 let mut out=NormalProjectionColumnInitializerV1::new(common)?;
 out.capture_qualified_industry_columns_v1(&sources.industry,refs.roots)?;
 out.capture_qualified_native_columns_v1(&sources.native,refs.roots)?;
 out.capture_qualified_primitive_columns_v1(&sources.primitive,refs.roots)?;
 out.capture_qualified_inventory_columns_v1(&sources.inventory,refs.roots)?;
 out.capture_qualified_agent_instance_columns_v1(&sources.agents,tx,live)?;
 out.capture_qualified_factory_authority_columns_v1(&sources.factory_authority,tx,live)?;
 out.capture_qualified_agent_authority_columns_v1(&sources.agent_authority,tx,live)?;
 out.capture_qualified_fixed_columns_v1(&sources.fixed,tx,live)?;
 out.capture_qualified_material_columns_v1(&sources.materials,tx,live)?;
 out.capture_qualified_main_policy_columns_v1(&sources.main_policy,tx,live)?;
 out.capture_qualified_controls_columns_v1(&sources.controls,tx,live)?;
 out.capture_qualified_module_state_columns_v1(&sources.module_states,tx,live)?;
 out.capture_qualified_governance_columns_v1(&sources.governance,tx,live,max_json)?;
 out.capture_qualified_diplomacy_columns_v1(&sources.diplomacy,tx,live,max_json)?;
 out.capture_qualified_reward_columns_v1(&sources.reward,tx,live,max_json)?;
 out.capture_qualified_marketplace_columns_v1(&sources.marketplace,tx,live,max_json)?;
 out.capture_qualified_world_rule_columns_v1(&sources.rules,refs.roots,max_json)?;
 out.capture_qualified_auxiliary_columns_v1(aux.columns_v1())?;
 out.capture_qualified_original_schema_v1(retained_json)?;
 let mut seed=out.finish()?;
 let mut original=super::state_columns::OriginalStateColumnsDataV1::from_original_seed_data_v1(&seed,common)?;
 original.finalize_qualified_seed_emissions_v1(&mut seed,common,max_json)?;
 sources.validate_v1(refs.roots,tx,common,live)?;aux.validate(parent,tx,common,live)?;Ok(seed)
}
