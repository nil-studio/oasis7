//! Fixed complete original world-rule status, including Runtime-owned current
//! control and bounded historical business outcomes. Output is explanation DATA.
use super::*;
use crate::world_rules::{status_readonly,action_results::readonly as results};
use oasis7_world_state::world_rules::WorldRuleStatusReadonlyFactsV1;
use serde_json::Value;
#[path="incremental_normal_projection_commitment_status_facts.rs"]mod raw_facts;
#[path="incremental_normal_projection_commitment_status_sources.rs"]mod source;
pub(in crate::world)struct CurrentWorldRuleStatusFeesV1{_facts:CurrentProjectionFactsFeesV1}
pub(in crate::world)struct PaidCurrentWorldRuleStatusDataV1{value:Option<Value>,fees:CurrentWorldRuleStatusFeesV1}
impl PaidCurrentWorldRuleStatusDataV1{
 pub(in crate::world)fn status(&self)->Option<&Value>{self.value.as_ref()}
 pub(in crate::world)fn into_parts_v1(self)->(Option<Value>,CurrentWorldRuleStatusFeesV1){(self.value,self.fees)}
}
#[allow(clippy::too_many_arguments)]
pub(in crate::world)fn prepare_current_world_rule_status_data_v1<M:MerkleNodeReader>(state:&OriginalStateColumnsDataV1,electricity:&OriginalProjectionRowsDataV1,aux:&ProjectionColumnDataV1,metadata:&OriginalProjectionReadMetadataV1<'_>,raw_state:&raw_hash::PaidOriginalStateHashDataV1,reader:Option<&str>,tx:&ExecutionTransaction<'_,M>,common:&mut FormalBoundedContextV1<'_>,canonical:&mut oasis7_wasm_abi::CanonicalCborWorkV1,live:&mut dyn FnMut()->ExecutionFence)->Result<PaidCurrentWorldRuleStatusDataV1,BodyMutationError>{
 raw_state.validate_context(common)?;let facts=CurrentProjectionReadFactsV1::for_source_data_v1(state,electricity,metadata,tx,common,live)?;facts.before_owned(size_of::<PaidCurrentWorldRuleStatusDataV1>())?;
 let view=values::View::new(&facts,aux)?;let source=source::CurrentStatus::new(&view,canonical);
 let value=status_readonly::status_from_sources_v1(&source,reader)?;source.finish()?;facts.check()?;
 drop(source);drop(view);Ok(PaidCurrentWorldRuleStatusDataV1{value,fees:CurrentWorldRuleStatusFeesV1{_facts:facts.into_fees()}})
}
