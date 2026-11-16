//! Complete SupplyClose preflight from the real Current family directory.
//! Every business refusal precedes writes; reservations retain original batch
//! and beneficiary order, including repeated lineages across different batches.
use super::*;
use super::super::delivery::{self,IndustrialDeliverySourcesV1,IndustrialDeliveryWorkV1,IndustrialDeliveryErrorV1};
use crate::world::incremental_body_event::materials::CorePolicyMaterialContextV1;
use oasis7_world_state::{incremental_field_read::WorldFieldReadContextV1,incremental_world_rules_family::WorldRulesQualificationV1,incremental_world_rule_lineages::*,world_rules::{SupplyLineageV1,supply_lineage_from_fields_v1,release_supply_close_lineage_v1}};
use oasis7_formal_protocol::incremental_state::StatePageWriter;
struct SelectedCloseBatchV1{key:String,qualification:ReservedValue<SupplyRecordQualificationV1>}
struct AllocationReleaseV1{batch:usize,beneficiary:String,lineage:String,allocation:i64,before:SupplyLineageV1,after:SupplyLineageV1,_phase:MemoryReservation}
pub(in crate::world) struct PreparedCompleteSupplyCloseV1{
 event:crate::DomainEvent,auth:CurrentSupplyCloseAuthorizationV1,batches:Vec<SelectedCloseBatchV1>,releases:Vec<AllocationReleaseV1>,credit:Option<credit::SupplyCloseCreditV1>,
 directory:ReservedValue<CurrentSupplyLifecycleDirectoryV1>,guard:PreparedGuard,identity:WorkIdentityV1,memory:MemoryBudget,now:u64,
 _batches_fee:MemoryReservation,_release_slots:MemoryReservation,
}
pub(in crate::world) enum CompleteSupplyClosePreparationV1{Ready(PreparedCompleteSupplyCloseV1),Rejected(ReservedValue<crate::WorldError>)}
struct EqualText<'a>{expected:&'a [u8],offset:usize,equal:bool}
impl std::io::Write for EqualText<'_>{fn write(&mut self,value:&[u8])->std::io::Result<usize>{let end=self.offset.checked_add(value.len()).ok_or_else(||std::io::Error::other("Supply field comparison length"))?;self.equal&=self.expected.get(self.offset..end)==Some(value);self.offset=end;Ok(value.len())}fn flush(&mut self)->std::io::Result<()>{Ok(())}}
#[allow(clippy::too_many_arguments)]
pub(in crate::world) fn prepare_complete_current_supply_close_v1<M,P,V,R,W,E,C,PE,PC,CW,ME>(header:&header::PreparedFreshLifecycleHeaderV1,rules:&ReservedValue<WorldRulesQualificationV1>,sources:&IndustrialDeliverySourcesV1<'_,'_>,state:&M,ctx:&mut WorldFieldOverlayContextV1<'_,'_,'_,M,R,E,C>,parent:&mut WorldFieldReadContextV1<'_,P,V,PE,PC>,write:&mut MemberRecordWriteContext<'_,W,CW>,materials:&mut CorePolicyMaterialContextV1<'_,M,ME>,work:&mut IndustrialDeliveryWorkV1,common:&mut FormalBoundedContextV1<'_>,now:u64,live:&ExecutionFence)->Result<CompleteSupplyClosePreparationV1,IndustrialDeliveryErrorV1>
where M:MerkleNodeReader,P:MerkleNodeReader,V:ValueNodeReader,R:ValueNodeReader,W:ValueNodeReader+ValueNodeWriter+StatePageWriter,E:FnMut()->Option<ValueTreeError>,C:FnMut()->bool,PE:FnMut()->Option<ValueTreeError>,PC:FnMut()->bool,CW:FnMut()->bool,ME:FnMut()->Option<ValueTreeError>{
 header.validate(ctx.transaction,common.memory(),now,common.work().identity(),live)?;
 let crate::DomainEvent::WorldRuleCommitted{command:WorldRuleCommandV1::CloseSupply{authorization_id},..}=header.event()else{return Err(bad().into())};
 delivery::paid_io::run(ctx,parent,write,materials,work,common,|ctx,_parent,_write,_materials,work,tail|{
  tail.charge(8192)?;let guard=ctx.transaction.guard();
  let directory=open_current_supply_lifecycle_directory_v1(rules.supply(),sources.eligibility.roots,state,ctx,&mut work.fixed_scan,&mut work.fixed_cells_remaining,&mut work.eligibility.pending_entries_remaining,work.max_identity_bytes as u64,&guard,live)?;
  directory.validate(ctx.transaction,tail.memory(),live)?;
  let proof=directory.retain_ordinary_record_v1(SupplyRootKeyV1::Authorization(authorization_id),tail.memory(),&mut work.fixed_cells_remaining,&mut ctx.cancelled)?;
  let Some(proof)=proof else{tail.charge(512)?;let fee=tail.memory().reserve(512,MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;return Ok(CompleteSupplyClosePreparationV1::Rejected(fee.retain(crate::world_rules::invalid("authorization_missing"))))};
  let auth=match read_authorization_from_proof_v1(header,proof,state,ctx,tail,work.max_identity_bytes,live)?{OpenSupplyCloseV1::Ready(auth)=>auth,OpenSupplyCloseV1::Rejected(error)=>return Ok(CompleteSupplyClosePreparationV1::Rejected(error))};
  let mut batch_count=0usize;let mut release_count=0usize;let mut batch_bytes=0usize;
  for row in directory.batches(){tail.charge(row.authorization_id.len().min(auth.id.len()))?;if row.authorization_id==auth.id&&row.allocated{batch_count=batch_count.checked_add(1).ok_or_else(bad)?;release_count=release_count.checked_add(row.allocation.len()).ok_or_else(bad)?;batch_bytes=batch_bytes.checked_add(row.outer_id.len()).ok_or_else(bad)?;}}
  let event_bytes=match header.event(){crate::DomainEvent::WorldRuleCommitted{actor_id,request_id,command:WorldRuleCommandV1::CloseSupply{authorization_id},world_binding,..}=>actor_id.len().checked_add(request_id.len()).and_then(|n|n.checked_add(authorization_id.len())).and_then(|n|n.checked_add(world_binding.as_ref().map_or(0,String::len))).ok_or_else(bad)?,_=>return Err(bad().into())};
  batch_bytes=batch_bytes.checked_add(event_bytes).ok_or_else(bad)?;
  batch_bytes=batch_bytes.checked_add(batch_count.checked_mul(size_of::<SelectedCloseBatchV1>()).ok_or_else(bad)?).and_then(|n|n.checked_add(size_of::<PreparedCompleteSupplyCloseV1>())).ok_or_else(bad)?;
  let release_bytes=release_count.checked_mul(size_of::<AllocationReleaseV1>()).ok_or_else(bad)?;
  tail.charge(batch_bytes.checked_add(release_bytes).ok_or_else(bad)?)?;
  let batches_fee=tail.memory().reserve(batch_bytes,MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;let release_slots=tail.memory().reserve(release_bytes,MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;
  let mut batches=Vec::with_capacity(batch_count);let mut releases:Vec<AllocationReleaseV1>=Vec::with_capacity(release_count);
  if batches.capacity()!=batch_count||releases.capacity()!=release_count{return Err(bad().into());}
  for row in directory.batches(){
   tail.charge(row.authorization_id.len().min(auth.id.len()))?;if row.authorization_id!=auth.id||!row.allocated{continue}
   let proof=directory.retain_ordinary_record_v1(SupplyRootKeyV1::Batch(&row.outer_id),tail.memory(),&mut work.fixed_cells_remaining,&mut ctx.cancelled)?.ok_or_else(bad)?;
   let mut compare=EqualText{expected:row.authorization_id.as_bytes(),offset:0,equal:true};tail.charge(row.authorization_id.len())?;
   let guard=ctx.transaction.guard();let value=read_current_supply_lifecycle_scalar_v1(SupplyScalarRequestV1{owner:SupplyRootKeyV1::Batch(&row.outer_id),field:SupplyScalarFieldV1::Batch(SupplyBatchFieldV1::AuthorizationId),qualification:&proof},state,ctx,work.max_identity_bytes as u64,&mut compare,&guard,live)?;
   if !matches!(value,Some(SupplyScalarValueV1::Text{utf8_bytes})if utf8_bytes==row.authorization_id.len() as u64)||!compare.equal||compare.offset!=row.authorization_id.len(){return Err(bad().into());}
   let guard=ctx.transaction.guard();let allocated=read_current_supply_lifecycle_scalar_v1(SupplyScalarRequestV1{owner:SupplyRootKeyV1::Batch(&row.outer_id),field:SupplyScalarFieldV1::Batch(SupplyBatchFieldV1::Allocated),qualification:&proof},state,ctx,1,&mut std::io::sink(),&guard,live)?;
   if !matches!(allocated,Some(SupplyScalarValueV1::Bool(true))){return Err(bad().into());}
   let batch=batches.len();
   for(beneficiary,allocation)in &row.allocation{
    tail.charge(beneficiary.len())?;let guard=ctx.transaction.guard();
    let actual=read_current_supply_lifecycle_member_v1(SupplyMemberRequestV1{owner:SupplyRootKeyV1::Batch(&row.outer_id),member:SupplyMemberKeyV1::Allocation(beneficiary),qualification:&proof},state,ctx,&guard,live)?;
    if actual!=Some(SupplyMemberValueV1::I64(*allocation)){return Err(bad().into());}
    let bytes=auth.crisis.len().checked_add(auth.material.len()).and_then(|n|n.checked_add(beneficiary.len())).and_then(|n|n.checked_add(42)).and_then(|n|n.checked_mul(2)).and_then(|n|n.checked_add(beneficiary.len())).and_then(|n|n.checked_add(size_of::<String>())).ok_or_else(bad)?;
    tail.charge(bytes)?;let phase=tail.memory().reserve(bytes,MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;
    let lineage=supply_lineage_from_fields_v1(&auth.crisis,&auth.material,beneficiary);
    let mut staged=None;for previous in releases.iter().rev(){tail.charge(previous.lineage.len().min(lineage.len()))?;if previous.lineage==lineage{staged=Some(previous.after.clone());break}}
    let before=match staged{Some(value)=>value,None=>match read_world_rule_lineage_overlay_v1(&lineage,ctx)?{WorldRuleLineageValueV1::Value(value)=>value,_=>return Err(bad().into())}};
    let mut after=before.clone();let mut reserved=*allocation;release_supply_close_lineage_v1(&mut after,&mut reserved);
    releases.push(AllocationReleaseV1{batch,beneficiary:beneficiary.clone(),lineage,allocation:*allocation,before,after,_phase:phase});
   }
   batches.push(SelectedCloseBatchV1{key:row.outer_id.clone(),qualification:proof});
  }
  let credit=match credit::prepare_credit_inner_v1(&auth,ctx,tail,live)?{credit::SupplyCloseCreditPreparationV1::Ready(credit)=>credit,credit::SupplyCloseCreditPreparationV1::Rejected(error)=>return Ok(CompleteSupplyClosePreparationV1::Rejected(error))};
  Ok(CompleteSupplyClosePreparationV1::Ready(PreparedCompleteSupplyCloseV1{event:header.event().clone(),auth,batches,releases,credit:Some(credit),directory,guard:ctx.transaction.guard(),identity:tail.identity(),memory:tail.memory().clone(),now,_batches_fee:batches_fee,_release_slots:release_slots}))
 })
}

#[path="incremental_normal_world_rule_lifecycle_supply_close_commit.rs"]
mod commit;

pub(in crate::world) use commit::{CommittedCompleteSupplyCloseV1,commit_complete_current_supply_close_v1};
