//! Actual selected supply-settlement preparation and Current CAS. Business
//! refusal is data; a post-write event still requires the common rule header.
use super::*;
use crate::world::incremental_normal_tail::NormalTailWorkV1;
use oasis7_formal_protocol::{controlled_authority::formal::bounded_context::FormalBoundedContextV1,incremental_execution::RootSetV1,incremental_state::StatePageWriter,incremental_value::{ValueNodeReader,ValueNodeWriter,ValueTreeError}};
use oasis7_proto::memory_budget::MemoryClass;
use oasis7_world_state::{incremental_field_overlay::{WorldFieldOverlayContextV1,WorldFieldOverlayErrorV1},incremental_field_member::MemberRecordWriteContext,incremental_world_rule_shipments::*,incremental_world_rule_lineages::*,incremental_industry_jobs::{IndustryFamilyQualificationV1,IndustryRecordKeyV1,IndustryRecordKindV1,read_industry_record_root_overlay_v1},incremental_world_id_sets::{WorldIdSetFieldV1,WorldIdSetKeyRefV1,world_id_set_contains_overlay_v1},incremental_world_rules_family::WorldRulesQualificationV1};
use std::mem::size_of;
fn bad()->BodyMutationError{ValueTreeError::Invalid("lifecycle supply Current source").into()}
fn limit()->BodyMutationError{ValueTreeError::ResourceLimited("lifecycle supply selected layout").into()}
fn map(e:WorldFieldOverlayErrorV1)->BodyMutationError{match e{WorldFieldOverlayErrorV1::Execution(e)=>e.into(),WorldFieldOverlayErrorV1::Value(e)=>e.into(),e=>BodyMutationError::Original(Box::new(e))}}
pub(super) enum SupplyPreparationV1<T>{Ready(T),Rejected(ReservedValue<crate::WorldError>)}
pub(super) struct PreparedSupplySettlementV1{
 id:u64,shipment:ReservedValue<oasis7_world_state::world_rules::SupplyShipmentV1>,
 after:oasis7_world_state::world_rules::SupplyShipmentV1,
 before_lineage:oasis7_world_state::world_rules::SupplyLineageV1,after_lineage:oasis7_world_state::world_rules::SupplyLineageV1,
 guard:PreparedGuard,identity:WorkIdentityV1,memory:MemoryBudget,_copies:MemoryReservation,
}
pub(super) struct CommittedSupplySettlementV1{pub(super) guard:PreparedGuard,pub(super) identity:WorkIdentityV1,_source:PreparedSupplySettlementV1}
fn rejected(fee:MemoryReservation,reason:&str)->SupplyPreparationV1<PreparedSupplySettlementV1>{SupplyPreparationV1::Rejected(fee.retain(crate::world_rules::invalid(reason)))}
#[allow(clippy::too_many_arguments)]
pub(super) fn prepare_settlement_v1<M,R,E,C,W,CW>(id:u64,rules:&ReservedValue<WorldRulesQualificationV1>,industry:&ReservedValue<IndustryFamilyQualificationV1>,roots:&RootSetV1,max_text:usize,ctx:&mut WorldFieldOverlayContextV1<'_,'_,'_,M,R,E,C>,write:&mut MemberRecordWriteContext<'_,W,CW>,common:&mut FormalBoundedContextV1<'_>,live:&ExecutionFence)->Result<SupplyPreparationV1<PreparedSupplySettlementV1>,BodyMutationError>
where M:MerkleNodeReader,R:ValueNodeReader,E:FnMut()->Option<ValueTreeError>,C:FnMut()->bool,W:ValueNodeReader+ValueNodeWriter,CW:FnMut()->bool{
 super::io::paid(ctx,write,common,|ctx,_write,tail|{
  let memory=tail.memory().clone();let guard=ctx.transaction.guard();ctx.transaction.validate_guard(&guard,live)?;
  if !rules.uses_budget(&memory)||!industry.uses_budget(&memory)||rules.allocation_class()!=MemoryClass::Ordinary||industry.allocation_class()!=MemoryClass::Ordinary{return Err(bad());}
  let metadata=roots.world_id.len().checked_add(6*oasis7_formal_protocol::incremental_state::STATE_ROOT_BYTES+1024).and_then(|n|n.checked_mul(8)).ok_or_else(limit)?;tail.charge(metadata)?;
  let _metadata=memory.reserve(4096,MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;rules.validate_binding(roots)?;industry.validate_binding(&roots.digest()?)?;
  let error_fee=memory.reserve(128+size_of::<ReservedValue<crate::WorldError>>(),MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;
  let bytes=max_text.checked_mul(4).and_then(|n|n.checked_add(4*size_of::<Vec<u8>>()+size_of::<ReservedValue<oasis7_world_state::world_rules::SupplyShipmentV1>>())).ok_or_else(limit)?;tail.charge(bytes)?;
  let fee=memory.reserve(bytes,MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;
  let mut buffers:[Vec<u8>;4]=std::array::from_fn(|_|Vec::with_capacity(max_text));if buffers.iter().any(|v|v.capacity()!=max_text){return Err(limit());}
  let [a,b,c,d]=&mut buffers;let mut outputs:[&mut dyn std::io::Write;4]=[a,b,c,d];
  let value=read_world_rule_shipment_overlay_v1(id,ctx,max_text as u64,&mut outputs).map_err(map)?;drop(outputs);
  let WorldRuleShipmentValueV1::Value{input_amount,delivered_amount,settled,..}=value else{return Ok(rejected(error_fee,"shipment_missing"));};
  if settled{return Ok(rejected(error_fee,"await_logistics_receipt_or_already_settled"));}
  let receipt=read_industry_record_root_overlay_v1(IndustryRecordKeyV1{kind:IndustryRecordKindV1::Settlement,id},ctx).map_err(map)?.is_some();
  if !receipt{return Ok(rejected(error_fee,"await_logistics_receipt_or_already_settled"));}
  let settled_id=world_id_set_contains_overlay_v1(WorldIdSetFieldV1::SettledLogisticsTransits,WorldIdSetKeyRefV1::Action(id),ctx).map_err(map)?;
  if !settled_id{return Ok(rejected(error_fee,"await_logistics_receipt_or_already_settled"));}
  let [a,b,c,d]=buffers;let strings=[a,b,c,d].map(|v|String::from_utf8(v).map_err(|_|bad()));let[a,b,c,d]=strings;
  let shipment=fee.retain(oasis7_world_state::world_rules::SupplyShipmentV1{authorization_id:a?,batch_id:b?,beneficiary_id:c?,lineage_id:d?,input_amount,delivered_amount,settled});
  let before_lineage=match read_world_rule_lineage_overlay_v1(&shipment.lineage_id,ctx).map_err(map)?{WorldRuleLineageValueV1::Value(value)=>value,_=>return Ok(rejected(error_fee,"lineage_missing"))};
  let mut after_lineage=before_lineage.clone();
  if let Err(error)=oasis7_world_state::world_rules::settle_supply_lineage_v1(&mut after_lineage,&shipment){return Ok(SupplyPreparationV1::Rejected(error_fee.retain(error)));}
  let copies=shipment.authorization_id.len().checked_add(shipment.batch_id.len()).and_then(|n|n.checked_add(shipment.beneficiary_id.len())).and_then(|n|n.checked_add(shipment.lineage_id.len())).and_then(|n|n.checked_add(size_of::<PreparedSupplySettlementV1>())).ok_or_else(limit)?;tail.charge(copies)?;
  let copies=memory.reserve(copies,MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;let mut after=(*shipment).clone();after.settled=true;
  Ok(SupplyPreparationV1::Ready(PreparedSupplySettlementV1{id,shipment,after,before_lineage,after_lineage,guard:ctx.transaction.guard(),identity:tail.identity(),memory,_copies:copies}))
 })
}
pub(super) fn apply_settlement_v1<M,R,E,C,W,CW>(source:PreparedSupplySettlementV1,ctx:&mut WorldFieldOverlayContextV1<'_,'_,'_,M,R,E,C>,write:&mut MemberRecordWriteContext<'_,W,CW>,common:&mut FormalBoundedContextV1<'_>,max_text:u64,live:&ExecutionFence)->Result<CommittedSupplySettlementV1,BodyMutationError>
where M:MerkleNodeReader,R:ValueNodeReader,E:FnMut()->Option<ValueTreeError>,C:FnMut()->bool,W:ValueNodeReader+ValueNodeWriter+StatePageWriter,CW:FnMut()->bool{
 super::io::paid(ctx,write,common,|ctx,write,tail|{
  ctx.transaction.validate_guard(&source.guard,live)?;
  if source.identity!=tail.identity()||!source.memory.same_pool(tail.memory())||!source.shipment.uses_budget(tail.memory()){return Err(bad());}
  tail.charge(source.shipment.lineage_id.len())?;let guard=ctx.transaction.guard();
  change_world_rule_lineage_overlay_v1(WorldRuleLineageChangeV1{key:&source.shipment.lineage_id,expected:WorldRuleLineageExpectedV1::Value(&source.before_lineage),after:Some(&source.after_lineage)},ctx,write,&guard,live).map_err(map)?;
  let guard=ctx.transaction.guard();change_world_rule_shipment_overlay_v1(WorldRuleShipmentChangeV1{key:source.id,expected:WorldRuleShipmentExpectedV1::Value(&source.shipment),after:Some(&source.after),max_current_payload_bytes:max_text},ctx,write,&guard,live).map_err(map)?;
  Ok(CommittedSupplySettlementV1{guard:ctx.transaction.guard(),identity:tail.identity(),_source:source})
 })
}
