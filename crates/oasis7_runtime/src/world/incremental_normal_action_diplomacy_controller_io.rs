//! Sole Common child for lazy controller source and owner reads, with every
//! actual eligibility/order lane reconciled on success, error and unwind.
use super::*;
use crate::world::incremental_factory_directory_orders::{FactoryOrderDirectoryWorkV1,FactoryOrderDirectoryErrorV1};
use oasis7_formal_protocol::incremental_value::ValueReadContext;
fn error(error:FactoryOrderDirectoryErrorV1)->BodyMutationError{match error{FactoryOrderDirectoryErrorV1::Execution(e)=>e.into(),FactoryOrderDirectoryErrorV1::Value(e)=>e.into(),FactoryOrderDirectoryErrorV1::Body(e)=>e,FactoryOrderDirectoryErrorV1::Overlay(e)=>match e{oasis7_world_state::incremental_field_overlay::WorldFieldOverlayErrorV1::Execution(e)=>e.into(),oasis7_world_state::incremental_field_overlay::WorldFieldOverlayErrorV1::Value(e)=>e.into(),oasis7_world_state::incremental_field_overlay::WorldFieldOverlayErrorV1::Member(e)=>BodyMutationError::Original(Box::new(e))}}}
#[allow(clippy::too_many_arguments)]
pub(super) fn paid<M:MerkleNodeReader,R:ValueNodeReader,E:FnMut()->Option<ValueTreeError>,C:FnMut()->bool,T>(ctx:&mut WorldFieldOverlayContextV1<'_, '_, '_,M,R,E,C>,work:&mut IndustrialEligibilityWorkV1,orders:&mut FactoryOrderDirectoryWorkV1,common:&mut FormalBoundedContextV1<'_>,run:impl FnOnce(&mut WorldFieldOverlayContextV1<'_, '_, '_,M,R,&mut E,&mut dyn FnMut()->bool>,&mut IndustrialEligibilityWorkV1,&mut FactoryOrderDirectoryWorkV1)->Result<T,FactoryOrderDirectoryErrorV1>)->Result<T,BodyMutationError>{
 if !ctx.values.memory.same_pool(common.memory())||!ctx.transaction.uses_budget(common.memory())||ctx.values.class!=MemoryClass::Ordinary{return Err(bad())}
 common.charge(1,1024).map_err(|e|BodyMutationError::Original(Box::new(e)))?;
 let _phase=common.memory().reserve(1024,MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;
 macro_rules! apply {
     (clamp_bytes; $($args:tt)*) => { clamp_bytes!($($args)*) };
     (clamp_steps; $($args:tt)*) => { clamp_steps!($($args)*) };
     (restore_bytes; $($args:tt)*) => { restore_bytes!($($args)*) };
     (restore_steps; $($args:tt)*) => { restore_steps!($($args)*) };
     (snapshot; $($args:tt)*) => { snapshot!($($args)*) };
 }
 macro_rules! steps{($m:ident)=>{apply!($m;ctx.values.io.nodes_remaining,work.read.cells_remaining,work.scan.nodes_remaining,work.scan.leaves_remaining,work.typed.json.cells_remaining,work.typed.json.json_nodes_remaining,work.revocation.json.cells_remaining,work.revocation.json.json_nodes_remaining)}};
 macro_rules! bytes{($m:ident)=>{apply!($m;ctx.values.io.read_bytes_remaining,ctx.values.io.write_bytes_remaining,ctx.key_work.bytes_remaining,work.read.comparison_bytes_remaining,work.scan.read_bytes_remaining,orders.ids.comparison_bytes_remaining,orders.merge_comparison_bytes_remaining)}};
 macro_rules! snapshot{($($v:expr),*)=>{[$($v),*]}};
 let original_steps=steps!(snapshot);let original_bytes=bytes!(snapshot);let original_pending=[work.pending_entries_remaining,orders.pending_entries_remaining];
 let(_,meter,stop)=common.parts();let child=meter.begin_child().map_err(|e|BodyMutationError::Original(Box::new(e)))?;
 let step_cap=child.remaining_steps()/10;let byte_cap=child.remaining_read_bytes()/7;
 macro_rules! clamp_steps{($($v:expr),*)=>{$(($v)=($v).min(step_cap);)*}};
 macro_rules! clamp_bytes{($($v:expr),*)=>{$(($v)=($v).min(byte_cap);)*}};
 steps!(clamp_steps);bytes!(clamp_bytes);let pending_cap=usize::try_from(step_cap).unwrap_or(usize::MAX);work.pending_entries_remaining=work.pending_entries_remaining.min(pending_cap);orders.pending_entries_remaining=orders.pending_entries_remaining.min(pending_cap);
 let before_steps=steps!(snapshot);let before_bytes=bytes!(snapshot);let before_pending=[work.pending_entries_remaining,orders.pending_entries_remaining];
 let result=std::panic::catch_unwind(std::panic::AssertUnwindSafe(||{
   let mut cancel=||stop()||(ctx.cancelled)();
   let mut local=WorldFieldOverlayContextV1{transaction:&mut*ctx.transaction,values:ValueReadContext{source:ctx.values.source,memory:ctx.values.memory,class:ctx.values.class,io:&mut*ctx.values.io},key_work:&mut*ctx.key_work,semantic_bytes:ctx.semantic_bytes,record_bytes:ctx.record_bytes,take_state_error:&mut ctx.take_state_error,cancelled:&mut cancel as &mut dyn FnMut()->bool};
   run(&mut local,work,orders)
 }));
 let after_steps=steps!(snapshot);let after_bytes=bytes!(snapshot);let after_pending=[work.pending_entries_remaining,orders.pending_entries_remaining];
 let accounted=(||->Result<(),BodyMutationError>{
   let mut steps=0u64;let mut bytes=0u64;
   for(a,b)in before_steps.into_iter().zip(after_steps){steps=steps.checked_add(a.checked_sub(b).ok_or_else(bad)?).ok_or_else(bad)?;}
   for(a,b)in before_pending.into_iter().zip(after_pending){steps=steps.checked_add(u64::try_from(a.checked_sub(b).ok_or_else(bad)?).map_err(|_|bad())?).ok_or_else(bad)?;}
   for(a,b)in before_bytes.into_iter().zip(after_bytes){bytes=bytes.checked_add(a.checked_sub(b).ok_or_else(bad)?).ok_or_else(bad)?;}
   child.consume(steps,bytes).map_err(|e|BodyMutationError::Original(Box::new(e)))?;Ok(())
 })();
 if accounted.is_ok(){
   child.finish();
   let mut at=0usize;macro_rules! restore_steps{($($v:expr),*)=>{$(($v)=original_steps[at].saturating_sub(before_steps[at].saturating_sub(after_steps[at]));at+=1;)*}};steps!(restore_steps);let _=at;
   let mut at=0usize;macro_rules! restore_bytes{($($v:expr),*)=>{$(($v)=original_bytes[at].saturating_sub(before_bytes[at].saturating_sub(after_bytes[at]));at+=1;)*}};bytes!(restore_bytes);let _=at;
   work.pending_entries_remaining=original_pending[0].saturating_sub(before_pending[0].saturating_sub(after_pending[0]));orders.pending_entries_remaining=original_pending[1].saturating_sub(before_pending[1].saturating_sub(after_pending[1]));
 }else{drop(child)}
 let value=match result{Err(payload)=>{let _:Result<(),ExecutionError>=ctx.transaction.contribute(|_|Err(ExecutionError::Aborted));std::panic::resume_unwind(payload)},Ok(Err(source))=>Err(error(source)),Ok(Ok(value))=>accounted.and_then(|_|common.consume_steps(0).map_err(|e|BodyMutationError::Original(Box::new(e)))).map(|_|value)};
 if value.is_err(){let _:Result<(),ExecutionError>=ctx.transaction.contribute(|_|Err(ExecutionError::Aborted));}value
}
