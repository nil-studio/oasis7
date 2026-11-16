//! Exact five-lane command/request construction. These are selection DATA,
//! never a prepared transition or a counted/post-write publication source.
use super::*;
use oasis7_formal_protocol::{controlled_authority::formal::bounded_context::FormalBoundedContextV1,incremental_value::ValueTreeError};
use oasis7_proto::memory_budget::MemoryClass;
use std::{fmt::Write,mem::size_of};
pub(in crate::world) enum LifecycleRequestV1<'a>{SupplyClose(&'a str),SupplySettle(u64),Observe{region:&'a str,now:u64},Order{order:&'a str,now:u64,command:OrderLifecycleChoiceV1},EmergencyClose(&'a str)}
#[derive(Clone,Copy)]pub(in crate::world) enum OrderLifecycleChoiceV1{Advance,Deliver}
impl LifecycleRequestV1<'_>{
 fn operation(&self)->LifecycleOperationV1{match self{Self::SupplyClose(_)=>LifecycleOperationV1::SupplyClose,Self::SupplySettle(_)=>LifecycleOperationV1::SupplySettle,Self::Observe{..}=>LifecycleOperationV1::Observe,Self::Order{..}=>LifecycleOperationV1::OrderProgress,Self::EmergencyClose(_)=>LifecycleOperationV1::EmergencyClose}}
 fn hash_input(&self)->Option<&str>{match self{Self::SupplyClose(s)|Self::EmergencyClose(s)=>Some(s),Self::Observe{region,..}=>Some(region),Self::Order{order,..}=>Some(order),Self::SupplySettle(_)=>None}}
 fn request_len(&self)->usize{match self{Self::SupplyClose(_)=>6+64,Self::SupplySettle(_)=>7+20,Self::Observe{..}=>8+64+1+20,Self::Order{..}=>9+64+1+20,Self::EmergencyClose(_)=>9+64}}
 fn command_len(&self)->usize{self.hash_input().map_or(0,str::len)}
 fn build(&self,digest:&str)->(String,WorldRuleCommandV1){
  let mut id=String::with_capacity(self.request_len());
  let command=match self{
   Self::SupplyClose(auth)=>{write!(&mut id,"close:{digest}").expect("String formatter");WorldRuleCommandV1::CloseSupply{authorization_id:(*auth).to_owned()}},
   Self::SupplySettle(job)=>{write!(&mut id,"settle:{job}").expect("String formatter");WorldRuleCommandV1::SettleSupply{job_id:*job}},
   Self::Observe{region,now}=>{write!(&mut id,"observe:{digest}:{now}").expect("String formatter");WorldRuleCommandV1::EmergencySupply{command:EmergencySupplyCommandV1::Observe{region_id:(*region).to_owned()}}},
   Self::Order{order,now,command}=>{write!(&mut id,"progress:{digest}:{now}").expect("String formatter");match command{OrderLifecycleChoiceV1::Advance=>WorldRuleCommandV1::AdvanceAgentOrder{order_id:(*order).to_owned()},OrderLifecycleChoiceV1::Deliver=>WorldRuleCommandV1::DeliverAgentOrder{order_id:(*order).to_owned()}}},
   Self::EmergencyClose(auth)=>{write!(&mut id,"es-close:{digest}").expect("String formatter");WorldRuleCommandV1::EmergencySupply{command:EmergencySupplyCommandV1::Close{authorization_id:(*auth).to_owned()}}},
  };(id,command)
 }
}
pub(super) struct LifecycleWorkDataV1{pub actor:String,pub request:String,pub command:WorldRuleCommandV1,pub operation:LifecycleOperationV1}
pub(super) fn prepare_lifecycle_work_v1(actor:&str,request:LifecycleRequestV1<'_>,common:&mut FormalBoundedContextV1<'_>)->Result<ReservedValue<LifecycleWorkDataV1>,BodyMutationError>{
 let limit=||BodyMutationError::from(ValueTreeError::ResourceLimited("world-rule lifecycle work item"));
 let bytes=actor.len().checked_add(request.request_len()).and_then(|n|n.checked_add(request.command_len())).and_then(|n|n.checked_add(size_of::<ReservedValue<LifecycleWorkDataV1>>())).ok_or_else(limit)?;
 common.charge(1,u64::try_from(bytes).map_err(|_|limit())?).map_err(|e|BodyMutationError::Original(Box::new(e)))?;
 let fee=common.memory().reserve(bytes,MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;
 let digest=match request.hash_input(){Some(s)=>Some(crate::world::incremental_normal_action_diplomacy::fixed_json_digest_v1(s,common)?),None=>None};
 let(request_id,command)=request.build(digest.as_deref().map_or("",String::as_str));
 Ok(fee.retain(LifecycleWorkDataV1{actor:actor.to_owned(),request:request_id,command,operation:request.operation()}))
}
/// The old World wrapper shares exact request/command construction. It remains
/// a legacy oracle/source, not a bounded Current execution entry.
pub(in crate::world) fn legacy_lifecycle_work_v1(actor:&str,request:LifecycleRequestV1<'_>)->Result<(String,String,WorldRuleCommandV1),crate::WorldError>{
 let digest=request.hash_input().map(|s|crate::util::hash_json(&s)).transpose()?;
 let(id,command)=request.build(digest.as_deref().unwrap_or(""));Ok((actor.to_owned(),id,command))
}
