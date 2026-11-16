//! SupplyClose selected authorization source. All scalar values come from
//! the genuine qualified Current record; no empty authorization DTO is made.
use super::*;
use oasis7_formal_protocol::{controlled_authority::formal::bounded_context::FormalBoundedContextV1,incremental_value::{ValueNodeReader,ValueNodeWriter,ValueTreeError},incremental_state::{StatePageWriter,StateScanBudget}};
use oasis7_proto::memory_budget::MemoryClass;
use oasis7_world_state::{incremental_field_overlay::{WorldFieldOverlayContextV1,WorldFieldOverlayErrorV1},incremental_field_member::MemberRecordWriteContext,incremental_world_rule_supply_root::*};
use std::mem::size_of;
fn bad()->BodyMutationError{BodyMutationError::Invalid("Current SupplyClose source")}
fn map(error:WorldFieldOverlayErrorV1)->BodyMutationError{match error{WorldFieldOverlayErrorV1::Execution(e)=>e.into(),WorldFieldOverlayErrorV1::Value(e)=>e.into(),e=>BodyMutationError::Original(Box::new(e))}}
pub(super) struct CurrentSupplyCloseAuthorizationV1{
 pub(super) key:String,pub(super) id:String,pub(super) provider:String,pub(super) material:String,pub(super) crisis:String,
 pub(super) escrow:i64,pub(super) active:bool,
 pub(super) qualification:ReservedValue<SupplyRecordQualificationV1>,
 pub(super) guard:PreparedGuard,pub(super) identity:WorkIdentityV1,pub(super) memory:MemoryBudget,
 _phase:MemoryReservation,
}
pub(super) enum OpenSupplyCloseV1{Ready(CurrentSupplyCloseAuthorizationV1),Rejected(ReservedValue<crate::WorldError>)}
/// The caller's header has already performed actor/request/scope admission.
/// This function derives its key solely from that private actual header and
/// then checks the parent-record proof against the real Current outer edge.
#[allow(clippy::too_many_arguments)]
pub(super) fn open_current_supply_close_v1<M,S,R,E,C,W,CW>(header:&header::PreparedFreshLifecycleHeaderV1,family:&ReservedValue<SupplyFamilyQualificationV1>,roots:&oasis7_formal_protocol::incremental_execution::RootSetV1,state:&S,ctx:&mut WorldFieldOverlayContextV1<'_,'_,'_,M,R,E,C>,write:&mut MemberRecordWriteContext<'_,W,CW>,scan:&mut StateScanBudget,cells:&mut u64,common:&mut FormalBoundedContextV1<'_>,now:u64,max_text:usize,live:&ExecutionFence)->Result<OpenSupplyCloseV1,BodyMutationError>
where M:MerkleNodeReader,S:MerkleNodeReader,R:ValueNodeReader,E:FnMut()->Option<ValueTreeError>,C:FnMut()->bool,W:ValueNodeReader+ValueNodeWriter+StatePageWriter,CW:FnMut()->bool{
 header.validate(ctx.transaction,common.memory(),now,common.work().identity(),live)?;
 let crate::DomainEvent::WorldRuleCommitted{command:WorldRuleCommandV1::CloseSupply{authorization_id},..}=header.event()else{return Err(bad())};
 super::io_record::paid(ctx,write,scan,cells,common,|ctx,_write,scan,cells,tail|{
  let memory=tail.memory().clone();if !family.uses_budget(&memory){return Err(bad());}
  tail.charge(authorization_id.len())?;
  let mut comparisons=(authorization_id.len() as u64).checked_mul(u64::from(usize::BITS)+2).and_then(|n|n.checked_add(512)).ok_or_else(bad)?;
  tail.charge(usize::try_from(comparisons).map_err(|_|bad())?)?;
  let proof=family.retain_ordinary_lifecycle_record_v1(SupplyRootKeyV1::Authorization(authorization_id),&memory,&mut comparisons,|| (ctx.cancelled)()).map_err(BodyMutationError::from)?;
  let reject=memory.reserve(128+size_of::<ReservedValue<crate::WorldError>>(),MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;
  let metadata=family.record_counts();tail.charge(4096+metadata.len()*std::mem::size_of::<u64>())?;
  let guard=ctx.transaction.guard();
  let proof=qualify_current_supply_lifecycle_record_v1(SupplyRootKeyV1::Authorization(authorization_id),proof,roots,family,state,ctx,scan,cells,max_text as u64,&guard,live).map_err(map)?;
  let Some(qualification)=proof else{return Ok(OpenSupplyCloseV1::Rejected(reject.retain(crate::world_rules::invalid("authorization_missing"))))};
  read_authorization_from_proof_v1(header,qualification,state,ctx,tail,max_text,live)

 })
}

fn read_authorization_from_proof_v1<M,S,R,E,C>(header:&header::PreparedFreshLifecycleHeaderV1,qualification:ReservedValue<SupplyRecordQualificationV1>,state:&S,ctx:&mut WorldFieldOverlayContextV1<'_,'_,'_,M,R,E,C>,tail:&mut crate::world::incremental_normal_tail::NormalTailWorkV1<'_,'_>,max_text:usize,live:&ExecutionFence)->Result<OpenSupplyCloseV1,BodyMutationError>
where M:MerkleNodeReader,S:MerkleNodeReader,R:ValueNodeReader,E:FnMut()->Option<ValueTreeError>,C:FnMut()->bool{
 let crate::DomainEvent::WorldRuleCommitted{command:WorldRuleCommandV1::CloseSupply{authorization_id},..}=header.event()else{return Err(bad())};
 let memory=tail.memory().clone();tail.charge(128+size_of::<ReservedValue<crate::WorldError>>())?;
 let reject=memory.reserve(128+size_of::<ReservedValue<crate::WorldError>>(),MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;
  let fields=[SupplyAuthorizationFieldV1::Id,SupplyAuthorizationFieldV1::ProviderId,SupplyAuthorizationFieldV1::Material,SupplyAuthorizationFieldV1::CrisisId];
  let mut lengths=[0usize;4];
  for(field,length)in fields.into_iter().zip(&mut lengths){
   let guard=ctx.transaction.guard();let value=read_current_supply_lifecycle_scalar_v1(SupplyScalarRequestV1{owner:SupplyRootKeyV1::Authorization(authorization_id),field:SupplyScalarFieldV1::Authorization(field),qualification:&qualification},state,ctx,max_text as u64,&mut std::io::sink(),&guard,live).map_err(map)?;
   let Some(SupplyScalarValueV1::Text{utf8_bytes})=value else{return Err(bad())};*length=usize::try_from(utf8_bytes).map_err(|_|bad())?;
  }
  let capacity=lengths.into_iter().try_fold(authorization_id.len().checked_add(size_of::<CurrentSupplyCloseAuthorizationV1>()).ok_or_else(bad)?,|sum,n|sum.checked_add(n).ok_or_else(bad))?;
  tail.charge(capacity)?;let phase=memory.reserve(capacity,MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;
  let mut text=std::array::from_fn::<_,4,_>(|i|Vec::with_capacity(lengths[i]));
  if text.iter().zip(lengths).any(|(v,n)|v.capacity()!=n){return Err(bad());}
  for((field,output),length)in fields.into_iter().zip(&mut text).zip(lengths){
   let guard=ctx.transaction.guard();let value=read_current_supply_lifecycle_scalar_v1(SupplyScalarRequestV1{owner:SupplyRootKeyV1::Authorization(authorization_id),field:SupplyScalarFieldV1::Authorization(field),qualification:&qualification},state,ctx,length as u64,output,&guard,live).map_err(map)?;
   if !matches!(value,Some(SupplyScalarValueV1::Text{utf8_bytes}) if utf8_bytes==length as u64)||output.len()!=length{return Err(bad());}
  }
  let guard=ctx.transaction.guard();let active=read_current_supply_lifecycle_scalar_v1(SupplyScalarRequestV1{owner:SupplyRootKeyV1::Authorization(authorization_id),field:SupplyScalarFieldV1::Authorization(SupplyAuthorizationFieldV1::Active),qualification:&qualification},state,ctx,1,&mut std::io::sink(),&guard,live).map_err(map)?;
  let Some(SupplyScalarValueV1::Bool(active))=active else{return Err(bad())};
  // The original draft material contract's sole validity predicate is false;
  // Close therefore rejects exactly inactive records, independent of provider.
  if !active{return Ok(OpenSupplyCloseV1::Rejected(reject.retain(crate::world_rules::invalid("revocation_not_authorized"))))}
  let guard=ctx.transaction.guard();let escrow=read_current_supply_lifecycle_scalar_v1(SupplyScalarRequestV1{owner:SupplyRootKeyV1::Authorization(authorization_id),field:SupplyScalarFieldV1::Authorization(SupplyAuthorizationFieldV1::RemainingEscrow),qualification:&qualification},state,ctx,8,&mut std::io::sink(),&guard,live).map_err(map)?;
  let Some(SupplyScalarValueV1::I64(escrow))=escrow else{return Err(bad())};
  let [id,provider,material,crisis]=text;let id=String::from_utf8(id).map_err(|_|bad())?;let provider=String::from_utf8(provider).map_err(|_|bad())?;let material=String::from_utf8(material).map_err(|_|bad())?;let crisis=String::from_utf8(crisis).map_err(|_|bad())?;
  Ok(OpenSupplyCloseV1::Ready(CurrentSupplyCloseAuthorizationV1{key:authorization_id.clone(),id,provider,material,crisis,escrow,active,qualification,guard:ctx.transaction.guard(),identity:tail.identity(),memory,_phase:phase}))
}

#[path="incremental_normal_world_rule_lifecycle_supply_close_credit.rs"]
mod credit;

#[path="incremental_normal_world_rule_lifecycle_supply_close_plan.rs"]
mod plan;

pub(super) use plan::{CompleteSupplyClosePreparationV1,PreparedCompleteSupplyCloseV1,CommittedCompleteSupplyCloseV1,prepare_complete_current_supply_close_v1,commit_complete_current_supply_close_v1};
