//! Top Current selector data under the same exclusive child. Only its two
//! additional WorldRules counters are reconciled here; all existing IO,
//! typed/scan/pending lanes remain in the one shared delivery adapter.
use super::*;
use super::delivery::{IndustrialDeliveryErrorV1, IndustrialDeliveryWorkV1};
use crate::world::incremental_normal_tail::NormalTailWorkV1;
use oasis7_formal_protocol::{
    incremental_execution::RootSetV1,
    incremental_value::{ValueNodeReader, ValueTreeError},
};
use oasis7_world_state::{
    incremental_field_overlay::WorldFieldOverlayContextV1,
    incremental_world_rules_family::{
        CurrentWorldRuleLifecycleDirectoryV1, WorldRulesFamilyWorkV1, WorldRulesQualificationV1,
        open_current_world_rule_lifecycle_directory_v1,
    },
};
#[allow(clippy::too_many_arguments)]
pub(super) fn open_top_in_child_v1<M,S,R,E,C>(rules:&ReservedValue<WorldRulesQualificationV1>,roots:&RootSetV1,state:&S,ctx:&mut WorldFieldOverlayContextV1<'_,'_,'_,M,R,E,C>,work:&mut IndustrialDeliveryWorkV1,world:&mut WorldRulesFamilyWorkV1,tail:&mut NormalTailWorkV1<'_,'_>,live:&ExecutionFence)->Result<ReservedValue<CurrentWorldRuleLifecycleDirectoryV1>,IndustrialDeliveryErrorV1>
where M:MerkleNodeReader,S:MerkleNodeReader,R:ValueNodeReader,E:FnMut()->Option<ValueTreeError>,C:FnMut()->bool{
 tail.charge(8192)?;
 let cap=tail.remaining_steps()/4;
 world.outer_records_remaining=world.outer_records_remaining.min(cap);world.remainder.cells_remaining=world.remainder.cells_remaining.min(cap);
 let before=(world.outer_records_remaining,world.remainder.cells_remaining);
 let guard=ctx.transaction.guard();
 let result=open_current_world_rule_lifecycle_directory_v1(rules,roots,state,ctx,&mut work.fixed_scan,world,&mut work.composite_read,&mut work.eligibility.pending_entries_remaining,&guard,live);
 let debit=(||{let steps=before.0.checked_sub(world.outer_records_remaining).and_then(|n|before.1.checked_sub(world.remainder.cells_remaining).and_then(|m|n.checked_add(m))).ok_or(BodyMutationError::Invalid("lifecycle Current extra counter increased"))?;tail.debit(steps,0)})();
 if let Err(accounting)=debit{let _:Result<(),oasis7_formal_protocol::incremental_execution::ExecutionError>=ctx.transaction.contribute(|_|Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted));return match result{Err(original)=>Err(original.into()),Ok(_)=>Err(accounting.into())}}
 Ok(result?)
}
