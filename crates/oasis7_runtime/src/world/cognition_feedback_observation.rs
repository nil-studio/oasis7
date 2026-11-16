//! Complete ordered outbox validation shared by World and Current observers.
use super::*;
use crate::world::cognition_persistence::receipt_observation::ReceiptReadErrorV1 as Error;
use oasis7_wasm_abi::CapabilityCanonicalWorkV1;
use oasis7_world_state::cognition_feedback_contract::readonly_work;
pub(in crate::world)struct PaidFeedbackOutboxDataV1<C>{records:BTreeMap<String,RuntimeFeedbackOutboxRecordV1>,fee:Option<C>}
impl<C>PaidFeedbackOutboxDataV1<C>{pub(in crate::world)fn into_parts_v1(self)->(BTreeMap<String,RuntimeFeedbackOutboxRecordV1>,Option<C>){(self.records,self.fee)}}
pub(in crate::world)fn parse_outbox_field_with_work_v1<A:CapabilityCanonicalWorkV1>(value:Option<&JsonValue>,source:&A)->Result<PaidFeedbackOutboxDataV1<A::Charge>,Error>{
 let Some(value)=value else{return Ok(PaidFeedbackOutboxDataV1{records:BTreeMap::new(),fee:None})};
 let fee=readonly_work::phase(value,source)?;
 let records:BTreeMap<String,RuntimeFeedbackOutboxRecordV1>=serde_json::from_value(value.clone()).map_err(|_|feedback_error("feedback_outbox_invalid"))?;
 for(key,record)in &records{
  source.charge_work(1,(key.len()+record.feedback_id.len())as u64)?;
  if key!=&record.feedback_id{return Err(feedback_error("feedback_outbox_key_mismatch").into())}
  match record.validate_with_work_v1(source){Ok(())=>{},Err(readonly_work::FeedbackReadErrorV1::Business(error))=>return Err(feedback_error(error.code()).into()),Err(readonly_work::FeedbackReadErrorV1::Work(error))=>return Err(error.into())}
 }
 Ok(PaidFeedbackOutboxDataV1{records,fee})
}
pub(super)fn legacy(value:Option<&JsonValue>)->Result<BTreeMap<String,RuntimeFeedbackOutboxRecordV1>,WorldError>{match parse_outbox_field_with_work_v1(value,&oasis7_wasm_abi::LegacyCapabilityCanonicalWorkV1){Ok(value)=>Ok(value.records),Err(Error::World(e))=>Err(e),Err(Error::Work(_))=>unreachable!("legacy outbox work cannot fail")}}
