//! Original industrial observation predicates over real selected rows.
use oasis7_agent_api::world_action::*;
use std::ops::ControlFlow;
/// Only processing is supplied by the caller. Membership queries refer to the
/// actual stage of the original filter, never an authorization result.
pub fn industrial_root_reader_with_work_v1<E>(root:&RootOperationV1,owner:&str,work:&mut impl FnMut(u64,usize)->Result<(),E>,lot:&mut impl FnMut(&IndustrialRefV1,&str)->Result<bool,E>)->Result<bool,E>{
 work(1,root.owner_id.len()+owner.len())?;if root.owner_id!=owner{return Ok(false)}
 for slot in root.slots.values(){work(1,0)?;for parent in &slot.parents{work(1,0)?;if !lot(&parent.lot_ref,owner)?{return Ok(false)}}}Ok(true)
}
pub fn industrial_receipt_reader_with_work_v1<E>(receipt:&IndustrialReceiptV1,work:&mut impl FnMut(u64,usize)->Result<(),E>,root:&mut impl FnMut(&IndustrialRefV1)->Result<bool,E>,lot:&mut impl FnMut(&IndustrialRefV1)->Result<bool,E>)->Result<bool,E>{
 work(1,0)?;if !root(&receipt.root_ref)?{return Ok(false)}
 for parent in &receipt.input{work(1,0)?;if !lot(&parent.lot_ref)?{return Ok(false)}}
 for reference in &receipt.output{work(1,0)?;if !lot(reference)?&&!root(reference)?{return Ok(false)}}Ok(true)
}
pub fn industrial_binding_reader_with_work_v1<E>(binding:&ParentBindingRevisionV1,work:&mut impl FnMut(u64,usize)->Result<(),E>,root:&mut impl FnMut(&IndustrialRefV1)->Result<bool,E>,lot:&mut impl FnMut(&IndustrialRefV1)->Result<bool,E>)->Result<bool,E>{
 work(1,0)?;if !root(&binding.root_ref)?{return Ok(false)}for parent in &binding.parents{work(1,0)?;if !lot(&parent.lot_ref)?{return Ok(false)}}Ok(true)
}
/// Original six-field reference equality, including revision; no hash-only join.
pub fn industrial_reference_equal_with_work_v1<E>(a:&IndustrialRefV1,b:&IndustrialRefV1,work:&mut impl FnMut(u64,usize)->Result<(),E>)->Result<bool,E>{
 for(a,b)in [(&a.world_binding,&b.world_binding),(&a.domain,&b.domain),(&a.schema,&b.schema),(&a.id,&b.id)]{work(1,a.len()+b.len())?;if a!=b{return Ok(false)}}work(1,0)?;if a.revision!=b.revision{return Ok(false)}work(1,a.digest.len()+b.digest.len())?;Ok(a.digest==b.digest)
}
