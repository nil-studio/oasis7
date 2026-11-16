//! Concrete Current/member I/O under one exclusive existing Common child.
use super::*;
use crate::world::incremental_normal_tail::NormalTailWorkV1;
use oasis7_formal_protocol::{controlled_authority::formal::bounded_context::FormalBoundedContextV1,incremental_value::{ValueNodeReader,ValueNodeWriter,ValueReadContext,ValueTreeError}};
use oasis7_world_state::{incremental_field_overlay::WorldFieldOverlayContextV1,incremental_field_member::MemberRecordWriteContext};
use oasis7_proto::memory_budget::MemoryClass;
use std::cell::RefCell;
fn bad()->BodyMutationError{ValueTreeError::ResourceLimited("world-rule lifecycle I/O accounting").into()}
#[allow(clippy::too_many_arguments)]
pub(super) fn paid<M,R,E,C,W,CW,T>(ctx:&mut WorldFieldOverlayContextV1<'_,'_,'_,M,R,E,C>,write:&mut MemberRecordWriteContext<'_,W,CW>,common:&mut FormalBoundedContextV1<'_>,run:impl FnOnce(&mut WorldFieldOverlayContextV1<'_,'_,'_,M,R,&mut E,&mut dyn FnMut()->bool>,&mut MemberRecordWriteContext<'_,W,&mut dyn FnMut()->bool>,&mut NormalTailWorkV1<'_,'_>)->Result<T,BodyMutationError>)->Result<T,BodyMutationError>
where M:MerkleNodeReader,R:ValueNodeReader,E:FnMut()->Option<ValueTreeError>,C:FnMut()->bool,W:ValueNodeReader+ValueNodeWriter,CW:FnMut()->bool{
 if !ctx.values.memory.same_pool(common.memory())||!write.memory.same_pool(common.memory())||ctx.values.class!=MemoryClass::Ordinary||!ctx.transaction.uses_budget(common.memory()){return Err(BodyMutationError::Invalid("lifecycle I/O same pool/class"));}
 common.consume_steps(1).map_err(|e|BodyMutationError::Original(Box::new(e)))?;
 let(memory,meter,stop)=common.parts();let child=meter.begin_child().map_err(|e|BodyMutationError::Original(Box::new(e)))?;
 let steps=child.remaining_steps()/2;let bytes=child.remaining_read_bytes()/6;
 ctx.values.io.nodes_remaining=ctx.values.io.nodes_remaining.min(steps);write.io.nodes_remaining=write.io.nodes_remaining.min(steps);
 for n in [&mut ctx.values.io.read_bytes_remaining,&mut ctx.values.io.write_bytes_remaining,&mut write.io.read_bytes_remaining,&mut write.io.write_bytes_remaining,&mut ctx.key_work.bytes_remaining,&mut write.key_work.bytes_remaining]{*n=(*n).min(bytes);}
 let before=([ctx.values.io.nodes_remaining,write.io.nodes_remaining],[ctx.values.io.read_bytes_remaining,ctx.values.io.write_bytes_remaining,write.io.read_bytes_remaining,write.io.write_bytes_remaining,ctx.key_work.bytes_remaining,write.key_work.bytes_remaining]);
 let result={
  let stop=RefCell::new(stop);let mut cancel=||(*stop.borrow_mut())()||(ctx.cancelled)();let mut write_cancel=||(*stop.borrow_mut())()||(write.cancelled)();let mut tail_cancel=||(*stop.borrow_mut())();
  let mut tail=NormalTailWorkV1::from_existing_child(memory,&child,&mut tail_cancel);
  ctx.transaction.contribute(|tx|{
   let mut current=WorldFieldOverlayContextV1{transaction:tx,values:ValueReadContext{source:ctx.values.source,memory:ctx.values.memory,class:ctx.values.class,io:&mut *ctx.values.io},key_work:&mut *ctx.key_work,semantic_bytes:ctx.semantic_bytes,record_bytes:ctx.record_bytes,take_state_error:&mut ctx.take_state_error,cancelled:&mut cancel as &mut dyn FnMut()->bool};
   let mut output=MemberRecordWriteContext{sink:&mut *write.sink,memory:write.memory,io:&mut *write.io,key_work:&mut *write.key_work,limits:write.limits,cancelled:&mut write_cancel as &mut dyn FnMut()->bool};
   run(&mut current,&mut output,&mut tail)
  })
 };
 let after=([ctx.values.io.nodes_remaining,write.io.nodes_remaining],[ctx.values.io.read_bytes_remaining,ctx.values.io.write_bytes_remaining,write.io.read_bytes_remaining,write.io.write_bytes_remaining,ctx.key_work.bytes_remaining,write.key_work.bytes_remaining]);
 let accounting=(||{let mut s=0u64;let mut b=0u64;for(a,z)in before.0.into_iter().zip(after.0){s=s.checked_add(a.checked_sub(z).ok_or_else(bad)?).ok_or_else(bad)?;}for(a,z)in before.1.into_iter().zip(after.1){b=b.checked_add(a.checked_sub(z).ok_or_else(bad)?).ok_or_else(bad)?;}child.consume(s,b).map_err(|e|BodyMutationError::Original(Box::new(e)))})();
 if let Err(e)=accounting{let _:Result<(),oasis7_formal_protocol::incremental_execution::ExecutionError>=ctx.transaction.contribute(|_|Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted));return match result{Err(original)=>Err(original),Ok(_)=>Err(e)};}child.finish();result
}
