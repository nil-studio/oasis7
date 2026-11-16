//! Fixed History4 DATA from the actual accepted/captured index scope. Ordinary
//! reads only the genuine newly appended suffix; explicit cold reads ALL once.
use super::*;
use oasis7_world_state::{incremental_cognition_history::{CognitionHistoryFieldV1,CognitionHistoryIndexRefV1,CognitionHistoryIndexPointV1,CognitionHistoryIndexWorkV1,read_cognition_history_index_point_overlay_data_v1},incremental_cognition_history_cbor::{CognitionHistoryCborWorkV1,CognitionHistoryRecordSchemaV1,CognitionOriginalCborRequestV1,materialize_cognition_history_original_json_v1}};
use oasis7_formal_protocol::{incremental_material::{OriginalObjectStreamReader,FactBudget},incremental_state::StateRootV1};
use std::sync::Arc;
pub(in crate::world) struct CognitionHistoryProjectionAuxWorkV1<'a>{pub index:&'a mut CognitionHistoryIndexWorkV1,pub cbor:&'a mut CognitionHistoryCborWorkV1,pub originals:&'a mut FactBudget}
impl CognitionHistoryProjectionAuxWorkV1<'_>{
 pub(super) fn steps(&self)->[u64;4]{[self.index.cells_remaining,self.cbor.nodes_remaining,self.cbor.source_reads_remaining,self.originals.facts_remaining]}
 pub(super) fn bytes(&self)->[u64;3]{[self.cbor.bytes_remaining,self.cbor.key_bytes_remaining,self.originals.original_bytes_remaining]}
 pub(super) fn clamp(&mut self,s:u64,b:u64){self.index.cells_remaining=self.index.cells_remaining.min(s);self.cbor.nodes_remaining=self.cbor.nodes_remaining.min(s);self.cbor.source_reads_remaining=self.cbor.source_reads_remaining.min(s);self.originals.facts_remaining=self.originals.facts_remaining.min(s);self.cbor.bytes_remaining=self.cbor.bytes_remaining.min(b);self.cbor.key_bytes_remaining=self.cbor.key_bytes_remaining.min(b);self.originals.original_bytes_remaining=self.originals.original_bytes_remaining.min(b);}
 pub(super) fn restore(&mut self,original_s:[u64;4],before_s:[u64;4],after_s:[u64;4],original_b:[u64;3],before_b:[u64;3],after_b:[u64;3]){let s=std::array::from_fn::<_,4,_>(|i|original_s[i].saturating_sub(before_s[i].saturating_sub(after_s[i])));let b=std::array::from_fn::<_,3,_>(|i|original_b[i].saturating_sub(before_b[i].saturating_sub(after_b[i])));self.index.cells_remaining=s[0];self.cbor.nodes_remaining=s[1];self.cbor.source_reads_remaining=s[2];self.originals.facts_remaining=s[3];self.cbor.bytes_remaining=b[0];self.cbor.key_bytes_remaining=b[1];self.originals.original_bytes_remaining=b[2];}
}
pub(super) trait OriginalDataReaderV1{fn validate(&self,memory:&MemoryBudget)->Result<(),BodyMutationError>;fn read(&mut self,id:&[u8;32],max:usize,memory:&MemoryBudget)->Result<Arc<ReservedValue<Vec<u8>>>,BodyMutationError>;}
struct TypedOriginalReader<'a,O:OriginalObjectStreamReader>{reader:&'a mut O,first:&'a mut Option<O::Error>}
impl<O:OriginalObjectStreamReader> OriginalDataReaderV1 for TypedOriginalReader<'_,O>{
 fn validate(&self,memory:&MemoryBudget)->Result<(),BodyMutationError>{if !self.reader.allocation_budget().same_pool(memory)||self.reader.allocation_class()!=MemoryClass::Ordinary{return Err(bad())}Ok(())}
 fn read(&mut self,id:&[u8;32],max:usize,memory:&MemoryBudget)->Result<Arc<ReservedValue<Vec<u8>>>,BodyMutationError>{match self.reader.read_original(id,max,memory){Ok(value)=>Ok(value),Err(error)=>{*self.first=Some(error);Err(BodyMutationError::Invalid("Cognition Original IO typed transport"))}}}
}
#[derive(Clone,Copy)]pub(super) enum RowOrigin{Accepted,Current{height:u64}}
pub(super) struct HistoryRowRequestV1<'a>{pub(super) root:&'a StateRootV1,pub(super) field:CognitionHistoryFieldV1,pub(super) ordinal:u64,pub(super) origin:RowOrigin}
pub(super) struct HistoryPacketV1{point:ReservedValue<CognitionHistoryIndexPointV1>,schema:ReservedValue<CognitionHistoryRecordSchemaV1>,raw:Arc<ReservedValue<Vec<u8>>>,value:ReservedValue<serde_json::Value>}
struct ProcessingValues<'a,R>{source:&'a R,bank:std::cell::RefCell<&'a mut oasis7_world_state::incremental_runtime_revocation::BusinessDiffProcessingWorkV1>}
fn processing(work:&mut oasis7_world_state::incremental_runtime_revocation::BusinessDiffProcessingWorkV1,n:usize)->Result<(),ValueTreeError>{let n=u64::try_from(n).map_err(|_|ValueTreeError::ResourceLimited("Cognition projection processing"))?;let s=n.checked_add(1).ok_or(ValueTreeError::ResourceLimited("Cognition projection processing"))?;let before=(work.steps_remaining,work.bytes_remaining);work.steps_remaining=work.steps_remaining.saturating_sub(s);work.bytes_remaining=work.bytes_remaining.saturating_sub(n);if before.0<s||before.1<n{return Err(ValueTreeError::ResourceLimited("Cognition projection processing"))}Ok(())}
impl<R:ValueNodeReader> ValueNodeReader for ProcessingValues<'_,R>{fn read_node(&self,id:&[u8;32],max:usize,memory:&MemoryBudget,class:MemoryClass)->Result<Arc<ReservedValue<Vec<u8>>>,ValueTreeError>{processing(&mut self.bank.borrow_mut(),0)?;let value=self.source.read_node(id,max,memory,class)?;processing(&mut self.bank.borrow_mut(),value.len().checked_mul(16).and_then(|n|n.checked_add(128)).ok_or(ValueTreeError::ResourceLimited("Cognition projection page work"))?)?;Ok(value)}}
pub(super) fn read_packet<M:MerkleNodeReader,R:ValueNodeReader,E:FnMut()->Option<ValueTreeError>,C:FnMut()->bool>(request:&HistoryRowRequestV1<'_>,ctx:&mut WorldFieldOverlayContextV1<'_, '_, '_,M,R,E,C>,source:&mut dyn OriginalDataReaderV1,work:&mut CognitionProjectionAuxWorkV1<'_>,history:&mut CognitionHistoryProjectionAuxWorkV1<'_>,live:&ExecutionFence)->Result<HistoryPacketV1,BodyMutationError>{
 source.validate(ctx.values.memory)?;
 let point={let wrapped=ProcessingValues{source:ctx.values.source,bank:std::cell::RefCell::new(&mut *work.processing)};let mut selected=WorldFieldOverlayContextV1{transaction:&mut *ctx.transaction,values:ValueReadContext{source:&wrapped,memory:ctx.values.memory,class:ctx.values.class,io:&mut *ctx.values.io},key_work:&mut *ctx.key_work,semantic_bytes:ctx.semantic_bytes,record_bytes:ctx.record_bytes,take_state_error:&mut ctx.take_state_error,cancelled:&mut ctx.cancelled};read_cognition_history_index_point_overlay_data_v1(request.root,request.field,request.ordinal,&mut selected,history.index,live).map_err(overlay)?};
 let row=point.row();if let RowOrigin::Current{height}=request.origin{let position=usize::try_from(row.location.ordinal).map_err(|_|bad())?;if row.location.height!=height||!ctx.transaction.ordered_fact(position).is_some_and(|fact|fact.kind==request.field.fact_kind()&&fact.id==row.location.original){return Err(bad())}}
 if row.encoded_bytes>history.originals.max_fact_bytes{return Err(ValueTreeError::ResourceLimited("Cognition projection Original bound").into())}
 history.originals.facts_remaining=history.originals.facts_remaining.checked_sub(1).ok_or(ValueTreeError::ResourceLimited("Cognition projection Original count"))?;
 history.originals.original_bytes_remaining=history.originals.original_bytes_remaining.checked_sub(row.encoded_bytes).ok_or(ValueTreeError::ResourceLimited("Cognition projection Original bytes"))?;
 let raw=source.read(&row.location.original,usize::try_from(row.encoded_bytes).map_err(|_|bad())?,ctx.values.memory)?;
 if !raw.uses_budget(ctx.values.memory)||raw.reserved_bytes()<raw.capacity()||raw.len()as u64!=row.encoded_bytes{return Err(bad())}
 let(schema,value)=materialize_cognition_history_original_json_v1(CognitionOriginalCborRequestV1{field:request.field,id:row.location.original,bytes:row.encoded_bytes},&mut raw.as_slice(),ctx.values.memory,MemoryClass::Ordinary,history.cbor,work.processing,&mut ctx.cancelled)?;
 Ok(HistoryPacketV1{point,schema,raw,value})
}
#[derive(Debug)]pub(in crate::world) enum CognitionHistoryProjectionErrorV1<E>{Body(BodyMutationError),Original(E)}
impl<E> From<BodyMutationError> for CognitionHistoryProjectionErrorV1<E>{fn from(e:BodyMutationError)->Self{Self::Body(e)}}
impl<E> From<ValueTreeError> for CognitionHistoryProjectionErrorV1<E>{fn from(e:ValueTreeError)->Self{Self::Body(e.into())}}
#[derive(Clone,Copy)]struct Window{field:CognitionHistoryFieldV1,before:u64,after:u64,present:bool}
struct Record{field:CognitionHistoryFieldV1,ordinal:u64,json:ReservedValue<Vec<u8>>}
pub(in crate::world) struct PaidCognitionHistoryProjectionAuxV1{windows:[Window;4],records:Vec<Record>,guard:oasis7_formal_protocol::incremental_execution::PreparedGuard,core:WorkIdentityV1,memory:MemoryBudget,_slots:MemoryReservation,_fee:MemoryReservation}
impl PaidCognitionHistoryProjectionAuxV1{
 pub(in crate::world) fn validate<M:MerkleNodeReader>(&self,tx:&ExecutionTransaction<'_,M>,common:&mut FormalBoundedContextV1<'_>,live:&ExecutionFence)->Result<(),BodyMutationError>{common.consume_steps(1).map_err(formal)?;tx.validate_read_source_guard(&self.guard,live)?;if self.core!=common.work().identity()||!self.memory.same_pool(common.memory())||!self._slots.uses_budget(common.memory())||!self._fee.uses_budget(common.memory()){return Err(bad())}Ok(())}
 pub(in crate::world) fn visit_windows_with_work_v1<E>(&self,mut work:impl FnMut(u64,u64)->Result<(),E>,mut emit:impl FnMut(CognitionHistoryFieldV1,u64,u64,bool)->Result<(),E>)->Result<(),E>{for row in &self.windows{work(1,0)?;emit(row.field,row.before,row.after,row.present)?}Ok(())}
 pub(in crate::world) fn visit_records_with_work_v1<E>(&self,mut work:impl FnMut(u64,u64)->Result<(),E>,mut emit:impl FnMut(CognitionHistoryFieldV1,u64,&[u8])->Result<(),E>)->Result<(),E>{for row in &self.records{work(1,0)?;emit(row.field,row.ordinal,&row.json)?}Ok(())}
}

use crate::world::incremental_normal_final_sources::NormalCognitionProjectionInputsV1;
enum CaptureScope<'a>{Current(&'a NormalCognitionProjectionInputsV1<'a>),Qualified}
fn current_origin(parent:&BoundNormalTickParentV1<'_>)->Result<RowOrigin,BodyMutationError>{Ok(RowOrigin::Current{height:parent.refs().core.body().height.checked_add(1).ok_or_else(bad)?})}
#[allow(clippy::too_many_arguments)]
fn capture_inner<M:MerkleNodeReader,R:ValueNodeReader,E:FnMut()->Option<ValueTreeError>,C:FnMut()->bool,O:OriginalObjectStreamReader>(parent:&BoundNormalTickParentV1<'_>,ctx:&mut WorldFieldOverlayContextV1<'_, '_, '_,M,R,E,C>,objects:&mut O,work:&mut CognitionProjectionAuxWorkV1<'_>,history:&mut CognitionHistoryProjectionAuxWorkV1<'_>,common:&mut FormalBoundedContextV1<'_>,live:&ExecutionFence,scope:CaptureScope<'_>)->Result<PaidCognitionHistoryProjectionAuxV1,CognitionHistoryProjectionErrorV1<O::Error>>{
 parent.validate_context(common).map_err(formal)?;
 if ctx.transaction.parent_fence()!=parent.fence()||!ctx.transaction.uses_budget(common.memory())||!ctx.values.memory.same_pool(common.memory())||ctx.values.class!=MemoryClass::Ordinary||!objects.allocation_budget().same_pool(common.memory())||objects.allocation_class()!=MemoryClass::Ordinary{return Err(bad().into())}
 let before=parent.refs().cognition.selected();if !before.uses_budget(common.memory())||before.allocation_class()!=MemoryClass::Ordinary||before.manifest().roots!=ctx.transaction.parent_fence().parent_roots||before.occurrence().head_body_id()!=ctx.transaction.parent_fence().parent_commit{return Err(bad().into())}
 match &scope{CaptureScope::Current(inputs)=>inputs.validate(ctx.transaction,common,live)?,CaptureScope::Qualified=>ctx.transaction.validate_bootstrap_read_only_v1(live).map_err(BodyMutationError::from)?}
 let guard=ctx.transaction.guard();
 let old=&before.manifest().indexes;let current=match &scope{CaptureScope::Current(inputs)=>inputs.indexes(),CaptureScope::Qualified=>old};
 let mut windows=std::array::from_fn(|i|Window{field:CognitionHistoryFieldV1::ALL[i],before:0,after:0,present:false});let mut count=0usize;
 for(i,field)in CognitionHistoryFieldV1::ALL.iter().copied().enumerate(){common.consume_steps(1).map_err(formal)?;let selected=&current[i];let base=if matches!(&scope,CaptureScope::Qualified){0}else{old[i].records};
  if (!selected.present&&selected.records!=0)||selected.root.namespace!=old[i].root.namespace{return Err(bad().into())}
  if let CaptureScope::Current(_)=&scope{if old[i].present&&!selected.present||selected.records<old[i].records||old[i].present&&selected.present&&old[i].records==selected.records&&old[i].root!=selected.root{return Err(BodyMutationError::Invalid("Cognition History requires actual append/prefix-trim source").into())}}
  let n=selected.records.checked_sub(base).ok_or_else(bad)?;count=count.checked_add(usize::try_from(n).map_err(|_|bad())?).ok_or_else(bad)?;windows[i]=Window{field,before:base,after:selected.records,present:selected.present};
 }
 let bytes=count.checked_mul(std::mem::size_of::<Record>()).ok_or_else(bad)?;common.charge(1,bytes as u64).map_err(formal)?;let slots=common.memory().reserve(bytes,MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;
 let outer=std::mem::size_of::<PaidCognitionHistoryProjectionAuxV1>();common.charge(1,outer as u64).map_err(formal)?;let fee=common.memory().reserve(outer,MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;
 let mut records=Vec::new();records.try_reserve_exact(count).map_err(|_|bad())?;if records.capacity()>count{return Err(bad().into())}
 for(i,window)in windows.iter().enumerate(){if !window.present{continue}let origin=match &scope{CaptureScope::Current(_)=>current_origin(parent)?,CaptureScope::Qualified=>RowOrigin::Accepted};for ordinal in window.before..window.after{
   match &scope{CaptureScope::Current(inputs)=>inputs.validate(ctx.transaction,common,live)?,CaptureScope::Qualified=>ctx.transaction.validate_bootstrap_read_only_v1(live).map_err(BodyMutationError::from)?}
   let request=HistoryRowRequestV1{root:&current[i].root,field:window.field,ordinal,origin};let mut first=None;
   let result={let mut reader=TypedOriginalReader{reader:&mut *objects,first:&mut first};super::capture(parent,ctx,work,common,live,Scope::HistoryRow{request,source:&mut reader,work:history})};
   if let Some(error)=first{return Err(CognitionHistoryProjectionErrorV1::Original(error))}
   let PaidComponent{source,core,memory,_fee:packet_fee}=result?;
   if core!=common.work().identity()||!memory.same_pool(common.memory())||!packet_fee.uses_budget(common.memory()){return Err(bad().into())}
   let packet=match source{Component::History(packet)=>packet,Component::State(_)|Component::Proof(_)=>return Err(bad().into())};
   let HistoryPacketV1{point,schema,raw,value}=packet;
   if point.ordinal()!=ordinal||schema.semantic().field()!=window.field||schema.semantic().id()!=&point.row().location.original||!point.uses_budget(common.memory())||!schema.uses_budget(common.memory())||!raw.uses_budget(common.memory())||!value.uses_budget(common.memory()){return Err(bad().into())}
   let json=crate::world::incremental_normal_projection_commitment::capture_projection_json_data_v1(&*value,common)?;
   // These are temporary decode/original carriers. Retained JSON has its own
   // actual allocation fee; consensus work is never returned with their Drop.
   drop(value);drop(raw);drop(schema);drop(point);drop(packet_fee);
   common.charge(1,std::mem::size_of::<Record>()as u64).map_err(formal)?;records.push(Record{field:window.field,ordinal,json});
  }}
 ctx.transaction.validate_read_source_guard(&guard,live).map_err(BodyMutationError::from)?;if let CaptureScope::Current(inputs)=&scope{inputs.validate(ctx.transaction,common,live)?}else{ctx.transaction.validate_bootstrap_read_only_v1(live).map_err(BodyMutationError::from)?}
 Ok(PaidCognitionHistoryProjectionAuxV1{windows,records,guard:ctx.transaction.guard(),core:common.work().identity(),memory:common.memory().clone(),_slots:slots,_fee:fee})
}
#[allow(clippy::too_many_arguments)]
fn capture<M:MerkleNodeReader,R:ValueNodeReader,E:FnMut()->Option<ValueTreeError>,C:FnMut()->bool,O:OriginalObjectStreamReader>(parent:&BoundNormalTickParentV1<'_>,ctx:&mut WorldFieldOverlayContextV1<'_, '_, '_,M,R,E,C>,objects:&mut O,work:&mut CognitionProjectionAuxWorkV1<'_>,history:&mut CognitionHistoryProjectionAuxWorkV1<'_>,common:&mut FormalBoundedContextV1<'_>,live:&ExecutionFence,scope:CaptureScope<'_>)->Result<PaidCognitionHistoryProjectionAuxV1,CognitionHistoryProjectionErrorV1<O::Error>>{
 let outcome=std::panic::catch_unwind(std::panic::AssertUnwindSafe(||capture_inner(parent,ctx,objects,work,history,common,live,scope)));
 match outcome{Ok(result)=>{let result=if let Some(error)=(ctx.take_state_error)(){Err(error.into())}else{result};if result.is_err(){let _:Result<(),ExecutionError>=ctx.transaction.contribute(|_|Err(ExecutionError::Aborted));}result},Err(panic)=>{let _:Result<(),ExecutionError>=ctx.transaction.contribute(|_|Err(ExecutionError::Aborted));std::panic::resume_unwind(panic)}}
}
pub(in crate::world) fn capture_current_cognition_history_projection_aux_bounded_v1<M:MerkleNodeReader,R:ValueNodeReader,E:FnMut()->Option<ValueTreeError>,C:FnMut()->bool,O:OriginalObjectStreamReader>(parent:&BoundNormalTickParentV1<'_>,inputs:&NormalCognitionProjectionInputsV1<'_>,ctx:&mut WorldFieldOverlayContextV1<'_, '_, '_,M,R,E,C>,objects:&mut O,work:&mut CognitionProjectionAuxWorkV1<'_>,history:&mut CognitionHistoryProjectionAuxWorkV1<'_>,common:&mut FormalBoundedContextV1<'_>,live:&ExecutionFence)->Result<PaidCognitionHistoryProjectionAuxV1,CognitionHistoryProjectionErrorV1<O::Error>>{capture(parent,ctx,objects,work,history,common,live,CaptureScope::Current(inputs))}
pub(in crate::world) fn capture_qualified_cognition_history_projection_aux_bounded_v1<M:MerkleNodeReader,R:ValueNodeReader,E:FnMut()->Option<ValueTreeError>,C:FnMut()->bool,O:OriginalObjectStreamReader>(parent:&BoundNormalTickParentV1<'_>,ctx:&mut WorldFieldOverlayContextV1<'_, '_, '_,M,R,E,C>,objects:&mut O,work:&mut CognitionProjectionAuxWorkV1<'_>,history:&mut CognitionHistoryProjectionAuxWorkV1<'_>,common:&mut FormalBoundedContextV1<'_>,live:&ExecutionFence)->Result<PaidCognitionHistoryProjectionAuxV1,CognitionHistoryProjectionErrorV1<O::Error>>{capture(parent,ctx,objects,work,history,common,live,CaptureScope::Qualified)}
