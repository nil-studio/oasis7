//! Complete original Cancelled/Disposed refund, preflight before any write.
use super::*;
use oasis7_world_state::{incremental_inventory_read::LedgerQuantityV1,incremental_inventory_change::InventoryChangeRefV1,incremental_agent_fields::{AgentFieldV1,AgentScalarRefV1},incremental_agent_subtree::*,incremental_world_balances::*,world_rules::{AgentIndustrialPhaseV1,validate_industrial_refund_phase_v1,add_industrial_refund_account_v1,finish_industrial_refund_v1}};
use oasis7_agent_api::world_action::ResourceKind;
use oasis7_world_state::incremental_value_mutation::ValueMutationContextV1;
struct MaterialRefundPointV1{kind:String,before:LedgerQuantityV1,after:i64}
pub(in crate::world) enum CurrentOrderRefundOutcomeV1{Ready(ReadyOrderAdvanceV1),Rejected(ReservedValue<crate::WorldError>)}
#[allow(clippy::too_many_arguments)]
pub(in crate::world) fn refund_current_order_advance_v1<M,P,V,R,W,E,C,PE,PC,CW,ME>(source:RefundOrderAdvanceV1,sources:&IndustrialDeliverySourcesV1<'_,'_>,state:&M,ctx:&mut WorldFieldOverlayContextV1<'_,'_,'_,M,R,E,C>,parent:&mut WorldFieldReadContextV1<'_,P,V,PE,PC>,write:&mut MemberRecordWriteContext<'_,W,CW>,materials:&mut CorePolicyMaterialContextV1<'_,M,ME>,work:&mut IndustrialDeliveryWorkV1,common:&mut FormalBoundedContextV1<'_>,now:u64,live:&ExecutionFence)->Result<CurrentOrderRefundOutcomeV1,IndustrialDeliveryErrorV1>
where M:MerkleNodeReader,P:MerkleNodeReader,V:ValueNodeReader,R:ValueNodeReader,W:ValueNodeReader+ValueNodeWriter+StatePageWriter,E:FnMut()->Option<ValueTreeError>,C:FnMut()->bool,PE:FnMut()->Option<ValueTreeError>,PC:FnMut()->bool,CW:FnMut()->bool,ME:FnMut()->Option<ValueTreeError>{
 let mut source=source.0;
 delivery::paid_io::run(ctx,parent,write,materials,work,common,|ctx,parent,write,_materials,work,tail|{
  ctx.transaction.validate_guard(&source.guard,live)?;
  if source.identity!=tail.identity()||!source.memory.same_pool(tail.memory())||source.now!=now||source.refund.is_none(){return Err(bad().into());}
  tail.charge(8192)?;let rejection=tail.memory().reserve(1024+size_of::<ReservedValue<crate::WorldError>>(),MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;
  if let Err(error)=validate_industrial_refund_phase_v1(&source.after){return Ok(CurrentOrderRefundOutcomeV1::Rejected(rejection.retain(error)));}
  let refund=source.refund.as_ref().ok_or_else(bad)?;tail.charge(refund.order_id().len().min(source.key.len()))?;
  if refund.order_id()!=source.key||!refund.is_current(){return Err(bad().into());}
  let cancelled=source.after.phase==AgentIndustrialPhaseV1::Cancelled;
  let count=if cancelled{source.after.inputs.len()}else{0};
  let mut bytes=count.checked_mul(size_of::<MaterialRefundPointV1>()).ok_or_else(bad)?;
  if cancelled{for(kind,_)in &source.after.inputs{bytes=bytes.checked_add(kind.len()).ok_or_else(bad)?;}}
  tail.charge(bytes)?;let _points_fee=tail.memory().reserve(bytes,MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;
  let mut points=Vec::with_capacity(count);if points.capacity()!=count{return Err(bad().into());}
  if cancelled{for(kind,amount)in &source.after.inputs{
   tail.charge(kind.len())?;let before=ctx.ledger(&source.after.quote.input_ledger,kind)?;
   let quantity=match before{LedgerQuantityV1::Value(v)=>v,_=>0};
   let Some(after)=quantity.checked_add(*amount)else{return Ok(CurrentOrderRefundOutcomeV1::Rejected(rejection.retain(crate::world_rules::invalid("refund_overflow"))))};
   points.push(MaterialRefundPointV1{kind:kind.clone(),before,after});
  }}
  let mut electricity=None;
  if cancelled{
   let guard=ctx.transaction.guard();
   if read_current_agent_presence_v1(&source.after.owner_id,sources.eligibility.agents,sources.eligibility.roots,ctx,parent,&mut work.eligibility.pending_entries_remaining,&guard,live)?!=CurrentAgentPresenceV1::Existing{return Ok(CurrentOrderRefundOutcomeV1::Rejected(rejection.retain(crate::world_rules::invalid("owner_missing"))));}
   let query=AgentSubtreeQualifiedRequestV1{cell:AgentSubtreeScalarRequestV1{agent:&source.after.owner_id,field:AgentFieldV1::ResourceAmount,ordinal:0},qualification:sources.eligibility.agents};
   let guard=ctx.transaction.guard();let before=match read_agent_subtree_scalar_overlay_v1(query,ctx,64,&mut std::io::sink(),&guard,live)?{None=>None,Some(AgentSubtreeValueV1::I64(v))=>Some(v),_=>return Err(bad().into())};
   let after=match oasis7_agent_api::world_resource::checked_resource_add_v1(ResourceKind::Electricity,before.unwrap_or(0),source.after.quote.electricity){Ok(v)=>v,Err(error)=>{tail.charge(256)?;return Ok(CurrentOrderRefundOutcomeV1::Rejected(rejection.retain(crate::world_rules::invalid(format!("electricity_refund:{error:?}")))));}};
   electricity=Some((before,after));
  }
  let(account_before,_account_fee)=delivery::economy::balance(&source.after.owner_id,ctx,work.max_identity_bytes,tail)?;
  let account_bytes=account_before.as_ref().map_or(0,|a|a.account_id.len()).checked_add(size_of::<oasis7_world_state::MainTokenAccountBalance>()).ok_or_else(bad)?;
  tail.charge(account_bytes)?;let _account_clone=tail.memory().reserve(account_bytes,MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;
  let mut account_after=account_before.clone().unwrap_or_default();
  if let Err(error)=add_industrial_refund_account_v1(&source.after,&mut account_after){return Ok(CurrentOrderRefundOutcomeV1::Rejected(rejection.retain(error)));}
  // All original business checks have completed without mutating Current.
  // Commit sparse material cells in original BTree order, preserving unrelated
  // entries and the original None -> Some(empty) ledger behavior.
  let mut created=false;
  if cancelled&&points.is_empty(){
   let before=ctx.ledger(&source.after.quote.input_ledger,"")?;
   if matches!(before,LedgerQuantityV1::LedgerAbsent){let guard=ctx.transaction.guard();ctx.change(InventoryChangeRefV1::LedgerMaterial{ledger:&source.after.quote.input_ledger,material:"",expected:before,after:None,create_ledger:true},write,&guard,live).map_err(|e|BodyMutationError::Original(Box::new(e)))?;}
  }
  for point in points{
   let expected=if created&&matches!(point.before,LedgerQuantityV1::LedgerAbsent){LedgerQuantityV1::MaterialAbsent}else{point.before};
   let create=matches!(expected,LedgerQuantityV1::LedgerAbsent);let guard=ctx.transaction.guard();
   ctx.change(InventoryChangeRefV1::LedgerMaterial{ledger:&source.after.quote.input_ledger,material:&point.kind,expected,after:Some(point.after),create_ledger:create},write,&guard,live).map_err(|e|BodyMutationError::Original(Box::new(e)))?;created|=create;
  }
  if let Some((before,after))=electricity{
   let query=AgentSubtreeQualifiedRequestV1{cell:AgentSubtreeScalarRequestV1{agent:&source.after.owner_id,field:AgentFieldV1::ResourceAmount,ordinal:0},qualification:sources.eligibility.agents};
   let guard=ctx.transaction.guard();let mut output=ValueMutationContextV1{sink:&mut *write.sink,memory:write.memory,io:&mut *write.io,cancelled:&mut write.cancelled};
   change_agent_subtree_scalar_v1(AgentSubtreeCellChangeV1{query,expected:before.map(AgentScalarRefV1::I64),after:(after!=0).then_some(AgentScalarRefV1::I64(after)),max_scalar_bytes:64},ctx,&mut output,&guard,live)?;
  }
  let guard=ctx.transaction.guard();change_world_balance_overlay_v1(WorldBalanceChangeV1{field:WorldBalanceFieldV1::MainToken,key:&source.after.owner_id,expected:account_before.as_ref().map(WorldBalanceRefV1::from),after:Some(WorldBalanceRefV1::from(&account_after)),max_current_identity_bytes:work.max_identity_bytes as u64},ctx,write,&guard,live)?;
  finish_industrial_refund_v1(&mut source.after,now);source.refund=None;source.guard=ctx.transaction.guard();
  Ok(CurrentOrderRefundOutcomeV1::Ready(ReadyOrderAdvanceV1(source)))
 })
}
