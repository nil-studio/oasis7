//! Exact original provider material credit after SupplyClose reservation
//! preflight. It is a selected point CAS, never a manufactured permission.
use super::*;
use oasis7_world_state::{MaterialLedgerId,incremental_inventory_read::LedgerQuantityV1,incremental_inventory_change::InventoryChangeRefV1};
pub(super) struct SupplyCloseCreditV1{
 ledger:MaterialLedgerId,material:String,expected:LedgerQuantityV1,after:i64,
 memory:MemoryBudget,identity:WorkIdentityV1,guard:PreparedGuard,_phase:MemoryReservation,
}
pub(super) enum SupplyCloseCreditPreparationV1{Ready(SupplyCloseCreditV1),Rejected(ReservedValue<crate::WorldError>)}
pub(super) fn prepare_credit_v1<M,R,E,C,W,CW>(source:&super::CurrentSupplyCloseAuthorizationV1,ctx:&mut WorldFieldOverlayContextV1<'_,'_,'_,M,R,E,C>,write:&mut MemberRecordWriteContext<'_,W,CW>,common:&mut FormalBoundedContextV1<'_>,live:&ExecutionFence)->Result<SupplyCloseCreditPreparationV1,BodyMutationError>
where M:MerkleNodeReader,R:ValueNodeReader,E:FnMut()->Option<ValueTreeError>,C:FnMut()->bool,W:ValueNodeReader+ValueNodeWriter,CW:FnMut()->bool{
 super::super::io::paid(ctx,write,common,|ctx,_write,tail|{
  prepare_credit_inner_v1(source,ctx,tail,live)

 })
}
pub(super) struct CommittedSupplyCloseCreditV1{pub(super) guard:PreparedGuard,pub(super) identity:WorkIdentityV1,_source:SupplyCloseCreditV1}
pub(super) fn apply_credit_v1<M,R,E,C,W,CW>(source:SupplyCloseCreditV1,ctx:&mut WorldFieldOverlayContextV1<'_,'_,'_,M,R,E,C>,write:&mut MemberRecordWriteContext<'_,W,CW>,common:&mut FormalBoundedContextV1<'_>,live:&ExecutionFence)->Result<CommittedSupplyCloseCreditV1,BodyMutationError>
where M:MerkleNodeReader,R:ValueNodeReader,E:FnMut()->Option<ValueTreeError>,C:FnMut()->bool,W:ValueNodeReader+ValueNodeWriter,CW:FnMut()->bool{
 super::super::io::paid(ctx,write,common,|ctx,write,tail|{
  apply_credit_inner_v1(source,ctx,write,tail,live)

 })
}

pub(super) fn prepare_credit_inner_v1<M,R,E,C>(source:&super::CurrentSupplyCloseAuthorizationV1,ctx:&mut WorldFieldOverlayContextV1<'_,'_,'_,M,R,E,C>,tail:&mut crate::world::incremental_normal_tail::NormalTailWorkV1<'_,'_>,live:&ExecutionFence)->Result<SupplyCloseCreditPreparationV1,BodyMutationError>
where M:MerkleNodeReader,R:ValueNodeReader,E:FnMut()->Option<ValueTreeError>,C:FnMut()->bool{
  ctx.transaction.validate_transaction_identity(&source.guard)?;
  if source.identity!=tail.identity()||!source.memory.same_pool(tail.memory()){return Err(bad());}
  tail.charge(128+size_of::<ReservedValue<crate::WorldError>>())?;
  let rejection=tail.memory().reserve(128+size_of::<ReservedValue<crate::WorldError>>(),MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;
  if source.escrow<0{return Ok(SupplyCloseCreditPreparationV1::Rejected(rejection.retain(crate::world_rules::invalid("negative_credit"))))}
  let bytes=source.provider.len().checked_add(source.material.len()).and_then(|n|n.checked_add(size_of::<SupplyCloseCreditV1>())).ok_or_else(bad)?;
  tail.charge(bytes)?;let phase=tail.memory().reserve(bytes,MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;
  let ledger=MaterialLedgerId::agent(&source.provider);let expected=ctx.ledger(&ledger,&source.material).map_err(map)?;
  let current=match expected{LedgerQuantityV1::Value(value)=>value,_=>0};
  let Some(after)=current.checked_add(source.escrow)else{return Ok(SupplyCloseCreditPreparationV1::Rejected(rejection.retain(crate::world_rules::invalid("material_overflow"))))};
  ctx.transaction.validate_guard(&ctx.transaction.guard(),live)?;
  Ok(SupplyCloseCreditPreparationV1::Ready(SupplyCloseCreditV1{ledger,material:source.material.clone(),expected,after,memory:tail.memory().clone(),identity:tail.identity(),guard:ctx.transaction.guard(),_phase:phase}))
}

pub(super) fn apply_credit_inner_v1<M,R,E,C,W,CW>(source:SupplyCloseCreditV1,ctx:&mut WorldFieldOverlayContextV1<'_,'_,'_,M,R,E,C>,write:&mut MemberRecordWriteContext<'_,W,CW>,tail:&mut crate::world::incremental_normal_tail::NormalTailWorkV1<'_,'_>,live:&ExecutionFence)->Result<CommittedSupplyCloseCreditV1,BodyMutationError>
where M:MerkleNodeReader,R:ValueNodeReader,E:FnMut()->Option<ValueTreeError>,C:FnMut()->bool,W:ValueNodeReader+ValueNodeWriter,CW:FnMut()->bool{
  ctx.transaction.validate_transaction_identity(&source.guard)?;
  if source.identity!=tail.identity()||!source.memory.same_pool(tail.memory()){return Err(bad());}
  tail.charge(source.material.len())?;let guard=ctx.transaction.guard();
  ctx.change(InventoryChangeRefV1::LedgerMaterial{ledger:&source.ledger,material:&source.material,expected:source.expected,after:Some(source.after),create_ledger:matches!(source.expected,LedgerQuantityV1::LedgerAbsent)},write,&guard,live)
   .map_err(|e|BodyMutationError::Original(Box::new(e)))?;
  Ok(CommittedSupplyCloseCreditV1{guard:ctx.transaction.guard(),identity:tail.identity(),_source:source})
}
