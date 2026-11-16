//! Original intent DTO assembly shared by two genuine closed column readers.
//! Scalars are DATA; this kernel neither chooses policy nor grants admission.
use super::IntentFieldV1 as F;
use crate::AgentIntentV2;
use oasis7_formal_protocol::incremental_value::ValueTreeError;
pub(crate) enum OriginalIntentProjectionScalarV1{U32(u32),U64(u64),Text(String),Unit}
pub(crate) fn assemble_original_intent_projection_v1<E:From<ValueTreeError>>(mut read:impl FnMut(F)->Result<OriginalIntentProjectionScalarV1,E>,wrong_type:&'static str)->Result<AgentIntentV2,E>{
 use OriginalIntentProjectionScalarV1 as V;
 macro_rules! text{($field:ident)=>{match read(F::$field)?{V::Text(value)=>value,_=>return Err(ValueTreeError::Invalid(wrong_type).into())}};}
 macro_rules! number{($field:ident)=>{match read(F::$field)?{V::U64(value)=>value,_=>return Err(ValueTreeError::Invalid(wrong_type).into())}};}
 macro_rules! optional_text{($field:ident)=>{match read(F::$field)?{V::Unit=>None,V::Text(value)=>Some(value),_=>return Err(ValueTreeError::Invalid(wrong_type).into())}};}
 macro_rules! optional_number{($field:ident)=>{match read(F::$field)?{V::Unit=>None,V::U64(value)=>Some(value),_=>return Err(ValueTreeError::Invalid(wrong_type).into())}};}
 let schema_version=match read(F::SchemaVersion)?{V::U32(value)=>value,_=>return Err(ValueTreeError::Invalid(wrong_type).into())};
 Ok(AgentIntentV2{
  schema_version,agent_id:text!(AgentId),intent_id:text!(IntentId),kind:text!(Kind),summary:text!(Summary),target_id:optional_text!(TargetId),effect_intent_id:optional_text!(EffectIntentId),intent_tick:optional_number!(IntentTick),world_id:optional_text!(WorldId),reorg_epoch:optional_number!(ReorgEpoch),authority_scope:optional_text!(AuthorityScope),status:text!(Status),source:text!(Source),logical_time:number!(LogicalTime),event_seq:number!(EventSeq),updated_at:number!(UpdatedAt),receipt_ref:optional_text!(ReceiptRef),reason_code:optional_text!(ReasonCode),reason_summary:optional_text!(ReasonSummary),replaced_by:optional_text!(ReplacedBy),actor_id:text!(ActorId),request_digest:text!(RequestDigest),
 })
}
