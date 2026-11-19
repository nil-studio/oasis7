//! Complete threat refresh from authenticated Current selected records. Old
//! heatmap keys come from their own resident directory, never inferred from
//! wars/crises. There is no event, cache promotion or completion capability.
use crate::world::{incremental_body::{BodyMutationError},incremental_body_event::{AgentEventOriginalV1,materials::CorePolicyMaterialContextV1},incremental_module_invocation::{RootModuleExecutionContextV1,RootModuleExecutionErrorV1},incremental_normal_sources::BoundNormalTickParentV1,incremental_normal_tail::{NormalTailSourcesV1,NormalTailWorkV1},incremental_normal_gameplay_cycles::{NormalGameplayCycleSourcesV1,NormalGameplayCycleWorkV1,GameplayCycleErrorV1,with_gameplay_cycle_io_v1,open_current_gameplay_diplomacy_v1,read_current_gameplay_text_v1,read_current_gameplay_scalar_v1}};
use oasis7_formal_protocol::{incremental_execution::{ExecutionFence,ExecutionError},incremental_material::OriginalObjectStreamReader,incremental_state::{MerkleNodeReader,StatePageWriter},incremental_value::{ValueNodeReader,ValueNodeWriter,ValueTreeError,ValueReadContext}};
use oasis7_world_state::{incremental_normal_gameplay_cycles_directory::{GameplayCycleDirectoryV1,GameplayDirectoryKindV1,open_current_gameplay_cycle_directory_v1},incremental_runtime_business::{ThreatKeyDirectoryV1,ThreatKeyDirectoryWorkV1,read_current_threat_key_directory_v1,read_runtime_business_overlay_v1,change_runtime_business_overlay_v1,RuntimeBusinessFieldV1,RuntimeBusinessNumberV1,RuntimeBusinessChangeV1},incremental_field_overlay::WorldFieldOverlayContextV1,incremental_field_read::WorldFieldReadContextV1,incremental_diplomacy::*};
use oasis7_proto::memory_budget::{ReservedValue,MemoryClass};
use std::{collections::BTreeMap,cmp::Ordering};
fn bad()->ValueTreeError{ValueTreeError::Invalid("Current threat refresh source")}
struct SelectedId{war:bool,id:String}
fn compare(a:&SelectedId,b:&SelectedId,tail:&mut NormalTailWorkV1<'_,'_>)->Result<Ordering,BodyMutationError>{tail.charge(a.id.len().min(b.id.len()))?;Ok((!a.war).cmp(&(!b.war)).then_with(||a.id.cmp(&b.id)))}
fn sort(rows:&mut[SelectedId],tail:&mut NormalTailWorkV1<'_,'_>)->Result<(),BodyMutationError>{
 fn sift(rows:&mut[SelectedId],mut root:usize,end:usize,tail:&mut NormalTailWorkV1<'_,'_>)->Result<(),BodyMutationError>{loop{let Some(left)=root.checked_mul(2).and_then(|n|n.checked_add(1)).filter(|n|*n<end)else{return Ok(())};let right=left+1;let child=if right<end&&compare(&rows[left],&rows[right],tail)?==Ordering::Less{right}else{left};if compare(&rows[root],&rows[child],tail)?!=Ordering::Less{return Ok(())}rows.swap(root,child);root=child;}}
 let len=rows.len();for root in(0..len/2).rev(){sift(rows,root,len,tail)?;}for end in(1..rows.len()).rev(){rows.swap(0,end);sift(rows,0,end,tail)?;}Ok(())
}
enum Contribution{War{aggressor:ReservedValue<String>,defender:ReservedValue<String>,intensity:u32},Crisis{kind:ReservedValue<String>,severity:u32}}
impl<'a,'t,'s,'v,M,R,E,C,W,F,O> RootModuleExecutionContextV1<'a,'t,'s,'v,M,R,E,C,W,F,O>
where M:MerkleNodeReader,R:ValueNodeReader,E:FnMut()->Option<ValueTreeError>,C:FnMut()->bool,W:ValueNodeReader+ValueNodeWriter+StatePageWriter,F:FnMut(&mut dyn AgentEventOriginalV1)->Result<(),BodyMutationError>,O:OriginalObjectStreamReader{
 #[allow(clippy::too_many_arguments)]
 pub(in crate::world) fn refresh_normal_current_threat_heatmap_v1<P,CE,CC,PE,PC,ME>(&mut self,parent_source:&BoundNormalTickParentV1<'_>,gameplay_directory:&ReservedValue<GameplayCycleDirectoryV1>,old_keys:&ReservedValue<ThreatKeyDirectoryV1>,retention:&ReservedValue<crate::world::incremental_world_event_counter::WorldJournalRetentionV1>,work:&mut NormalGameplayCycleWorkV1,key_work:&mut ThreatKeyDirectoryWorkV1,materials:&mut CorePolicyMaterialContextV1<'_,M,ME>,current:&mut WorldFieldReadContextV1<'_,P,R,CE,CC>,parent:&mut WorldFieldReadContextV1<'_,P,R,PE,PC>,normal:&mut NormalTailWorkV1<'_,'_>,live_owner:&mut dyn FnMut()->ExecutionFence)->Result<(),RootModuleExecutionErrorV1<O::Error>>
 where P:MerkleNodeReader,CE:FnMut()->Option<ValueTreeError>,CC:FnMut()->bool,PE:FnMut()->Option<ValueTreeError>,PC:FnMut()->bool,ME:FnMut()->Option<ValueTreeError>{
 let outcome=std::panic::catch_unwind(std::panic::AssertUnwindSafe(||{
  normal.validate_parent(parent_source).map_err(RootModuleExecutionErrorV1::Body)?;let bound=NormalTailSourcesV1::from_bound_parent(parent_source);let live=live_owner();let state=self.state;
  let sources=NormalGameplayCycleSourcesV1{roots:bound.roots(),directory:gameplay_directory,diplomacy:bound.native().diplomacy(),governance:bound.native().governance(),native:bound.native().native(),agents:bound.raw().agents(),retention};
  let mut ctx=WorldFieldOverlayContextV1{transaction:&mut*self.ctx.transaction,values:ValueReadContext{source:current.values.source,memory:current.values.memory,class:current.values.class,io:&mut*current.values.io},key_work:&mut*current.key_work,semantic_bytes:current.semantic_bytes,record_bytes:current.record_bytes,take_state_error:&mut current.take_state_error,cancelled:&mut current.cancelled};
  with_gameplay_cycle_io_v1(&mut ctx,parent,self.members,materials,work,normal.common_mut().map_err(RootModuleExecutionErrorV1::Body)?,|ctx,parent,write,_,work,tail|{
   tail.charge(16<<10)?;
   let step_cap=tail.remaining_steps()/3;key_work.visits_remaining=key_work.visits_remaining.min(step_cap);key_work.pending_entries_remaining=key_work.pending_entries_remaining.min(usize::try_from(step_cap).unwrap_or(usize::MAX));key_work.comparison_bytes_remaining=key_work.comparison_bytes_remaining.min(tail.remaining_read_bytes());
   let before=(key_work.visits_remaining,key_work.pending_entries_remaining,key_work.comparison_bytes_remaining);
   let body:Result<(),GameplayCycleErrorV1>=(||{
    let mut read=WorldFieldReadContextV1{root:sources.roots.state_root(oasis7_formal_protocol::incremental_state::StateNamespaceV1::Business),state:parent.state,values:ValueReadContext{source:parent.values.source,memory:parent.values.memory,class:parent.values.class,io:&mut*parent.values.io},key_work:&mut*parent.key_work,semantic_bytes:parent.semantic_bytes,record_bytes:parent.record_bytes,take_state_error:&mut parent.take_state_error,cancelled:&mut parent.cancelled};
    let guard=ctx.transaction.guard();let prior=read_current_threat_key_directory_v1(old_keys,bound.runtime_state().business(),sources.roots,ctx.transaction,&mut read,&mut work.selected_scan,key_work,&guard,&live)?;
    let guard=ctx.transaction.guard();let directory=open_current_gameplay_cycle_directory_v1(sources.directory,sources.diplomacy,sources.governance,sources.roots,ctx.transaction,&mut read,&mut work.selected_scan,&mut work.directory,&guard,&live)?;
    let mut n=0usize;let mut ids_bytes=0usize;directory.visit(&mut work.directory,&mut ctx.cancelled,|row,_|{if matches!(row.kind(),GameplayDirectoryKindV1::War|GameplayDirectoryKindV1::Crisis){n=n.checked_add(1).ok_or_else(bad)?;ids_bytes=ids_bytes.checked_add(row.outer_id().len()).ok_or_else(bad)?;}Ok::<_,ValueTreeError>(())})?;
    let slot_bytes=n.checked_mul(std::mem::size_of::<SelectedId>()+std::mem::size_of::<Contribution>()).and_then(|m|m.checked_add(ids_bytes)).ok_or_else(bad)?;tail.charge(slot_bytes)?;let _slots=tail.reserve(slot_bytes,MemoryClass::Ordinary)?;
    let mut ids=Vec::new();ids.try_reserve_exact(n).map_err(|_|bad())?;let mut contributions=Vec::new();contributions.try_reserve_exact(n).map_err(|_|bad())?;if ids.capacity()>n||contributions.capacity()>n{return Err(bad().into())}
    directory.visit(&mut work.directory,&mut ctx.cancelled,|row,_|{if matches!(row.kind(),GameplayDirectoryKindV1::War|GameplayDirectoryKindV1::Crisis){let id=row.outer_id().to_owned();if id.capacity()>row.outer_id().len(){return Err(bad())}ids.push(SelectedId{war:row.kind()==GameplayDirectoryKindV1::War,id});}Ok::<_,ValueTreeError>(())})?;sort(&mut ids,tail)?;
    let mut output_entries=0usize;let mut output_bytes=0usize;
    for row in ids{
     let field=if row.war{DiplomacyFieldV1::Wars}else{DiplomacyFieldV1::Crises};let proof=open_current_gameplay_diplomacy_v1(field,&row.id,&sources,ctx,parent,work,tail,&live)?.ok_or_else(bad)?;
     if row.war{
      let active=read_current_gameplay_scalar_v1(&row.id,field,DiplomacyScalarV1::War(WarScalarV1::Active),&proof,state,ctx,tail)?;if active!=Some(DiplomacyValueV1::Bool(true)){if active!=Some(DiplomacyValueV1::Bool(false)){return Err(bad().into())}continue}
      let Some(DiplomacyValueV1::Unsigned(intensity))=read_current_gameplay_scalar_v1(&row.id,field,DiplomacyScalarV1::War(WarScalarV1::Intensity),&proof,state,ctx,tail)?else{return Err(bad().into())};let intensity=u32::try_from(intensity).map_err(|_|bad())?;
      let aggressor=read_current_gameplay_text_v1(&row.id,field,DiplomacyScalarV1::War(WarScalarV1::AggressorAllianceId),&proof,state,ctx,work,tail)?.ok_or_else(bad)?;let defender=read_current_gameplay_text_v1(&row.id,field,DiplomacyScalarV1::War(WarScalarV1::DefenderAllianceId),&proof,state,ctx,work,tail)?.ok_or_else(bad)?;
      output_entries=output_entries.checked_add(3).ok_or_else(bad)?;output_bytes=output_bytes.checked_add(aggressor.len()).and_then(|n|n.checked_add(defender.len())).and_then(|n|n.checked_add(2*"alliance:".len()+"global:war".len())).ok_or_else(bad)?;contributions.push(Contribution::War{aggressor,defender,intensity});
     }else{
      let status=read_current_gameplay_text_v1(&row.id,field,DiplomacyScalarV1::Crisis(CrisisScalarV1::Status),&proof,state,ctx,work,tail)?.ok_or_else(bad)?;tail.charge(status.len())?;if status.as_str()!="active"{continue}
      let Some(DiplomacyValueV1::Unsigned(severity))=read_current_gameplay_scalar_v1(&row.id,field,DiplomacyScalarV1::Crisis(CrisisScalarV1::Severity),&proof,state,ctx,tail)?else{return Err(bad().into())};let severity=u32::try_from(severity).map_err(|_|bad())?;
      let kind=read_current_gameplay_text_v1(&row.id,field,DiplomacyScalarV1::Crisis(CrisisScalarV1::Kind),&proof,state,ctx,work,tail)?.ok_or_else(bad)?;output_entries=output_entries.checked_add(2).ok_or_else(bad)?;output_bytes=output_bytes.checked_add(kind.len()).and_then(|n|n.checked_add("crisis:".len()+"global:crisis".len())).ok_or_else(bad)?;contributions.push(Contribution::Crisis{kind,severity});
     }
    }
    let heap=oasis7_world_state::incremental_collection_layout::string_set_output_heap_v1(output_entries)?.checked_add(output_entries.checked_mul(11*std::mem::size_of::<i64>()).ok_or_else(bad)?).and_then(|n|n.checked_add(output_bytes)).ok_or_else(bad)?;
    let comparison=output_bytes.checked_mul(usize::BITS as usize*12).ok_or_else(bad)?;tail.debit(u64::try_from(output_entries.checked_mul(usize::BITS as usize*12).ok_or_else(bad)?).map_err(|_|bad())?,u64::try_from(comparison).map_err(|_|bad())?)?;let _map_fee=tail.reserve(heap,MemoryClass::Ordinary)?;
    let mut after=BTreeMap::new();for contribution in &contributions{match contribution{Contribution::War{aggressor,defender,intensity}=>super::threat_fold::fold_war_v1(&mut after,true,*intensity,aggressor,defender),Contribution::Crisis{kind,severity}=>super::threat_fold::fold_crisis_v1(&mut after,crate::CrisisStatus::Active,*severity,kind)}}
    // Collect the paid old keys before any writes so cancellation and key
    // enumeration cannot observe a partially replaced heatmap.
    let mut old_count=0usize;let mut old_bytes=0usize;prior.visit(key_work,&mut ctx.cancelled,|key,_|{old_count=old_count.checked_add(1).ok_or_else(bad)?;old_bytes=old_bytes.checked_add(key.len()).ok_or_else(bad)?;Ok::<_,ValueTreeError>(())})?;
    let old_heap=old_count.checked_mul(std::mem::size_of::<String>()).and_then(|n|n.checked_add(old_bytes)).ok_or_else(bad)?;tail.charge(old_heap)?;let _old_fee=tail.reserve(old_heap,MemoryClass::Ordinary)?;let mut old=Vec::new();old.try_reserve_exact(old_count).map_err(|_|bad())?;if old.capacity()>old_count{return Err(bad().into())}prior.visit(key_work,&mut ctx.cancelled,|key,_|{let value=key.to_owned();if value.capacity()>key.len(){return Err(bad())}old.push(value);Ok::<_,ValueTreeError>(())})?;
    for pair in old.windows(2){tail.charge(pair[0].len().min(pair[1].len()))?;if pair[0]>=pair[1]{return Err(bad().into())}}
    for key in &old{tail.charge(key.len().checked_mul(usize::BITS as usize*12).ok_or_else(bad)?)?;let before=read_runtime_business_overlay_v1(RuntimeBusinessFieldV1::ThreatHeatmap,key,ctx)?.ok_or_else(bad)?;let next=after.get(key).copied().map(RuntimeBusinessNumberV1::Signed);let guard=ctx.transaction.guard();change_runtime_business_overlay_v1(RuntimeBusinessChangeV1{field:RuntimeBusinessFieldV1::ThreatHeatmap,key,expected:Some(before),after:next},ctx,write,&guard,&live)?;}
    for(key,value)in &after{tail.charge(key.len().checked_mul((usize::BITS-old.len().max(1).leading_zeros())as usize+1).ok_or_else(bad)?)?;if old.binary_search_by(|old|old.as_str().cmp(key)).is_ok(){continue}let before=read_runtime_business_overlay_v1(RuntimeBusinessFieldV1::ThreatHeatmap,key,ctx)?;if before.is_some(){return Err(bad().into())}let guard=ctx.transaction.guard();change_runtime_business_overlay_v1(RuntimeBusinessChangeV1{field:RuntimeBusinessFieldV1::ThreatHeatmap,key,expected:None,after:Some(RuntimeBusinessNumberV1::Signed(*value))},ctx,write,&guard,&live)?;}
    Ok(())
   })();
   let accounted=(||{let steps=before.0.checked_sub(key_work.visits_remaining).and_then(|n|before.1.checked_sub(key_work.pending_entries_remaining).and_then(|m|n.checked_add(m as u64))).ok_or_else(bad)?;let bytes=before.2.checked_sub(key_work.comparison_bytes_remaining).ok_or_else(bad)?;tail.debit(steps,bytes)})();
   match(body,accounted){(Err(first),_)=>Err(first),(Ok(_),Err(error))=>Err(error.into()),(Ok(()),Ok(()))=>Ok(())}
  }).map_err(GameplayCycleErrorV1::into_root)?;Ok(())
 }));
 let result=match outcome{Ok(result)=>result,Err(payload)=>{let _:Result<(),ExecutionError>=self.ctx.transaction.contribute(|_|Err(ExecutionError::Aborted));std::panic::resume_unwind(payload)}};
 if result.is_err(){let _:Result<(),ExecutionError>=self.ctx.transaction.contribute(|_|Err(ExecutionError::Aborted));}result
 }
}
