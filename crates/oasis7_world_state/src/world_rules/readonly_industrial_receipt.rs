//! One original receipt validation kernel for actual maps and readonly facts.
//! Outcome is insertion DATA only; it cannot install or authorize a receipt.
use super::invalid;
use crate::WorldError;
use oasis7_agent_api::world_action::*;
use std::{mem::size_of,ops::ControlFlow};
pub trait IndustrialReceiptReadonlyFactsV1{
 type Error;
 fn receipt_metered(&self)->bool{true}
 fn receipt_work(&self,steps:u64,bytes:usize,heap:usize)->Result<(),Self::Error>;
 fn receipt_count(&self)->Result<usize,Self::Error>;
 fn receipt_with_root<T>(&self,id:&str,visit:impl FnOnce(Option<&RootOperationV1>)->Result<T,Self::Error>)->Result<T,Self::Error>;
 fn receipt_with_existing<T>(&self,id:&str,visit:impl FnOnce(Option<&IndustrialReceiptV1>)->Result<T,Self::Error>)->Result<T,Self::Error>;
 fn receipt_visit(&self,visit:impl FnMut(&IndustrialReceiptV1)->Result<ControlFlow<()>,Self::Error>)->Result<(),Self::Error>;
}
pub(crate) enum ReceiptInsertDataV1{Existing,Fresh}
fn fail<S:IndustrialReceiptReadonlyFactsV1,T>(s:&S,note:&str)->Result<Result<T,WorldError>,S::Error>{s.receipt_work(1,note.len(),size_of::<WorldError>()+note.len())?;Ok(Err(invalid(note)))}
fn ref_work<S:IndustrialReceiptReadonlyFactsV1>(s:&S,value:&IndustrialRefV1)->Result<(),S::Error>{if !s.receipt_metered(){return Ok(())}for text in [&value.world_binding,&value.domain,&value.schema,&value.id,&value.digest]{for part in text.as_bytes().chunks(8192){s.receipt_work(1,part.len(),0)?;}}s.receipt_work(1,0,0)}
fn qualified<S:IndustrialReceiptReadonlyFactsV1>(s:&S,value:&IndustrialRefV1,world:&str)->Result<Result<(),WorldError>,S::Error>{ref_work(s,value)?;s.receipt_work(1,world.len(),size_of::<WorldError>()+"industrial_reference_invalid".len())?;Ok(super::industrial_work::validate_ref(value,world))}
fn len<S:IndustrialReceiptReadonlyFactsV1,T:serde::Serialize>(s:&S,value:&T)->Result<Result<usize,WorldError>,S::Error>{match crate::util::measure_json_with_work_v1(value,&mut|steps,bytes|s.receipt_work(steps,bytes as usize,0)){Ok(n)=>Ok(Ok(n)),Err(crate::util::JsonHashWorkErrorV1::Work(error))=>Err(error),Err(crate::util::JsonHashWorkErrorV1::Codec(error))=>{let note=super::format_readonly_projection_data_v1(format_args!("{}",error),&mut|steps,bytes,heap|s.receipt_work(steps,bytes,heap))?;Ok(Err(WorldError::Serde(note)))}}}
macro_rules! domain{($value:expr)=>{match $value{Ok(value)=>value,Err(error)=>return Ok(Err(error))}};}
pub(crate) fn validate<S:IndustrialReceiptReadonlyFactsV1>(s:&S,receipt:&IndustrialReceiptV1,world:&str)->Result<Result<ReceiptInsertDataV1,WorldError>,S::Error>{
 domain!(qualified(s,&receipt.receipt_ref,world)?);
 if s.receipt_count()?>=65536{let exists=s.receipt_with_existing(&receipt.receipt_ref.id,|v|Ok(v.is_some()))?;if !exists{return fail(s,"industrial_capacity_exceeded")}}
 domain!(qualified(s,&receipt.child_ref,world)?);
 let root_ok=s.receipt_with_root(&receipt.root_ref.id,|root|{if let Some(root)=root{ref_work(s,&root.root_ref)?;ref_work(s,&receipt.root_ref)?;}Ok(root.is_some_and(|r|r.root_ref==receipt.root_ref))})?;
 s.receipt_work(1,0,0)?;if !root_ok||receipt.effect_role.is_empty()||receipt.payload_digest.len()!=64{return fail(s,"industrial_receipt_invalid")}
 s.receipt_with_existing(&receipt.receipt_ref.id,|old|{
  if let Some(old)=old{if s.receipt_metered(){let a=domain!(len(s,old)?);let b=domain!(len(s,receipt)?);let n=a.checked_add(b).expect("resident receipt equality layout");s.receipt_work(n as u64+1,n,0)?;}if old==receipt{return Ok(Ok(ReceiptInsertDataV1::Existing))}return fail(s,"industrial_receipt_identity_conflict")}
  let mut conflict=false;s.receipt_visit(|old|{ref_work(s,&old.child_ref)?;ref_work(s,&receipt.child_ref)?;if super::industrial_work::same_identity(&old.child_ref,&receipt.child_ref)&&old.child_ref.digest!=receipt.child_ref.digest{conflict=true;return Ok(ControlFlow::Break(()))}Ok(ControlFlow::Continue(()))})?;if conflict{return fail(s,"industrial_child_identity_conflict")}
  s.receipt_visit(|old|{ref_work(s,&old.root_ref)?;ref_work(s,&receipt.root_ref)?;ref_work(s,&old.child_ref)?;ref_work(s,&receipt.child_ref)?;s.receipt_work(1,old.effect_role.len()+receipt.effect_role.len(),0)?;if old.root_ref==receipt.root_ref&&old.child_ref==receipt.child_ref&&old.effect_role==receipt.effect_role{conflict=true;return Ok(ControlFlow::Break(()))}Ok(ControlFlow::Continue(()))})?;if conflict{return fail(s,"industrial_effect_already_committed")}Ok(Ok(ReceiptInsertDataV1::Fresh))
 })
}
pub(super) struct Resident<'a>(pub &'a IndustrialWorkStateV1);
impl IndustrialReceiptReadonlyFactsV1 for Resident<'_>{type Error=std::convert::Infallible;fn receipt_metered(&self)->bool{false}fn receipt_work(&self,_:u64,_:usize,_:usize)->Result<(),Self::Error>{Ok(())}fn receipt_count(&self)->Result<usize,Self::Error>{Ok(self.0.receipts.len())}fn receipt_with_root<T>(&self,id:&str,visit:impl FnOnce(Option<&RootOperationV1>)->Result<T,Self::Error>)->Result<T,Self::Error>{visit(self.0.roots.get(id))}fn receipt_with_existing<T>(&self,id:&str,visit:impl FnOnce(Option<&IndustrialReceiptV1>)->Result<T,Self::Error>)->Result<T,Self::Error>{visit(self.0.receipts.get(id))}fn receipt_visit(&self,mut visit:impl FnMut(&IndustrialReceiptV1)->Result<ControlFlow<()>,Self::Error>)->Result<(),Self::Error>{for value in self.0.receipts.values(){if visit(value)?.is_break(){break}}Ok(())}}

impl IndustrialReceiptReadonlyFactsV1 for crate::WorldState{type Error=std::convert::Infallible;
 fn receipt_metered(&self)->bool{false}
 fn receipt_work(&self,_:u64,_:usize,_:usize)->Result<(),Self::Error>{Ok(())}
 fn receipt_count(&self)->Result<usize,Self::Error>{Ok(self.world_rules.as_ref().map_or(0,|r|r.industrial_work.receipts.len()))}
 fn receipt_with_root<T>(&self,id:&str,visit:impl FnOnce(Option<&RootOperationV1>)->Result<T,Self::Error>)->Result<T,Self::Error>{visit(self.world_rules.as_ref().and_then(|r|r.industrial_work.roots.get(id)))}
 fn receipt_with_existing<T>(&self,id:&str,visit:impl FnOnce(Option<&IndustrialReceiptV1>)->Result<T,Self::Error>)->Result<T,Self::Error>{visit(self.world_rules.as_ref().and_then(|r|r.industrial_work.receipts.get(id)))}
 fn receipt_visit(&self,mut visit:impl FnMut(&IndustrialReceiptV1)->Result<ControlFlow<()>,Self::Error>)->Result<(),Self::Error>{if let Some(rules)=&self.world_rules{for receipt in rules.industrial_work.receipts.values(){if visit(receipt)?.is_break(){break}}}Ok(())}
}
