//! Original SupplyClose atomic contribution: release lineage then allocation,
//! actual provider credit, and only then authorization inactive/escrow zero.
use super::*;
use crate::world::incremental_normal_tail::NormalTailWorkV1;
pub(in crate::world) struct CommittedCompleteSupplyCloseV1{
 source:PreparedCompleteSupplyCloseV1,credit:credit::CommittedSupplyCloseCreditV1,
 after:ReservedValue<SupplyRecordQualificationV1>,guard:PreparedGuard,
}
impl CommittedCompleteSupplyCloseV1{
 pub(in crate::world) fn result_id(&self)->&str{&self.source.auth.key}
 pub(in crate::world) fn validate<M:MerkleNodeReader>(&self,header:&header::PreparedFreshLifecycleHeaderV1,tx:&ExecutionTransaction<'_,M>,now:u64,tail:&mut NormalTailWorkV1<'_,'_>,live:&ExecutionFence)->Result<(),BodyMutationError>{
  tx.validate_guard(&self.guard,live)?;
  if self.source.identity!=tail.identity()||!self.source.memory.same_pool(tail.memory())||!self.after.uses_budget(tail.memory())||!self.source.directory.uses_budget(tail.memory())||self.source.now!=now||self.source.credit.is_some(){return Err(bad());}
  compare_header(&self.source.event,header.event(),tail)
 }
}
fn compare_header(before:&crate::DomainEvent,after:&crate::DomainEvent,tail:&mut NormalTailWorkV1<'_,'_>)->Result<(),BodyMutationError>{
 let(crate::DomainEvent::WorldRuleCommitted{action_id:a,actor_id:aa,request_id:ar,command:WorldRuleCommandV1::CloseSupply{authorization_id:ao},governance_epoch:ae,world_binding:ab},crate::DomainEvent::WorldRuleCommitted{action_id:b,actor_id:ba,request_id:br,command:WorldRuleCommandV1::CloseSupply{authorization_id:bo},governance_epoch:be,world_binding:bb})=(before,after)else{return Err(bad())};
 let n=aa.len().min(ba.len()).checked_add(ar.len().min(br.len())).and_then(|n|n.checked_add(ao.len().min(bo.len()))).and_then(|n|n.checked_add(match(ab,bb){(Some(a),Some(b))=>a.len().min(b.len()),_=>0})).ok_or_else(bad)?;tail.charge(n)?;
 if a!=b||aa!=ba||ar!=br||ao!=bo||ae!=be||ab!=bb{return Err(bad());}Ok(())
}
#[allow(clippy::too_many_arguments)]
pub(in crate::world) fn commit_complete_current_supply_close_v1<M,P,V,R,W,E,C,PE,PC,CW,ME>(mut source:PreparedCompleteSupplyCloseV1,header:&header::PreparedFreshLifecycleHeaderV1,ctx:&mut WorldFieldOverlayContextV1<'_,'_,'_,M,R,E,C>,parent:&mut WorldFieldReadContextV1<'_,P,V,PE,PC>,write:&mut MemberRecordWriteContext<'_,W,CW>,materials:&mut CorePolicyMaterialContextV1<'_,M,ME>,work:&mut IndustrialDeliveryWorkV1,common:&mut FormalBoundedContextV1<'_>,now:u64,live:&ExecutionFence)->Result<CommittedCompleteSupplyCloseV1,IndustrialDeliveryErrorV1>
where M:MerkleNodeReader,P:MerkleNodeReader,V:ValueNodeReader,R:ValueNodeReader,W:ValueNodeReader+ValueNodeWriter+StatePageWriter,E:FnMut()->Option<ValueTreeError>,C:FnMut()->bool,PE:FnMut()->Option<ValueTreeError>,PC:FnMut()->bool,CW:FnMut()->bool,ME:FnMut()->Option<ValueTreeError>{
 delivery::paid_io::run(ctx,parent,write,materials,work,common,|ctx,_parent,write,_materials,work,tail|{
  ctx.transaction.validate_guard(&source.guard,live)?;
  if source.identity!=tail.identity()||!source.memory.same_pool(tail.memory())||source.now!=now{return Err(bad().into());}
  compare_header(&source.event,header.event(),tail)?;tail.charge(8192)?;
  for release in &source.releases{
   tail.charge(release.lineage.len().checked_add(release.beneficiary.len()).ok_or_else(bad)?)?;
   let guard=ctx.transaction.guard();change_world_rule_lineage_overlay_v1(WorldRuleLineageChangeV1{key:&release.lineage,expected:WorldRuleLineageExpectedV1::Value(&release.before),after:Some(&release.after)},ctx,write,&guard,live)?;
   let batch=source.batches.get_mut(release.batch).ok_or_else(bad)?;let guard=ctx.transaction.guard();
   let after=change_qualified_supply_member_v1(SupplyMemberChangeV1{request:SupplyMemberRequestV1{owner:SupplyRootKeyV1::Batch(&batch.key),member:SupplyMemberKeyV1::Allocation(&release.beneficiary),qualification:&batch.qualification},expected:Some(SupplyMemberValueV1::I64(release.allocation)),after:Some(SupplyMemberValueV1::I64(0)),max_pending_node_bytes:work.max_pending_node_bytes},ctx,write,&guard,live)?;
   if let Some(after)=after{batch.qualification=after;}
  }
  let credit=credit::apply_credit_inner_v1(source.credit.take().ok_or_else(bad)?,ctx,write,tail,live)?;
  let guard=ctx.transaction.guard();
  let after=close_current_supply_lifecycle_columns_v1(SupplyLifecycleCloseChangeV1{id:&source.auth.key,qualification:&source.auth.qualification,expected_active:source.auth.active,expected_escrow:source.auth.escrow,max_pending_node_bytes:work.max_pending_node_bytes},ctx,write,&guard,live)?;
  Ok(CommittedCompleteSupplyCloseV1{source,credit,after,guard:ctx.transaction.guard()})
 })
}
