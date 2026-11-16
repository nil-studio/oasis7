//! Explicit IWS condition/evaluation DATA. These DTOs do not authenticate a
//! Current cut, provider, request, lease or accepted execution. Runtime binds
//! them to actual typed source owners before signing/admitting/consuming them.
use crate::legacy_scheduler::{WakeConditionV1,WakeConditionValidator,WakeConditionError,WakeSchemeV2,WakeEvaluationContext,PreconditionSubjectV1};
use serde::{Serialize,Deserialize};
pub const IWS_WAKE_CONDITION_SCHEMA_V2:&str="iws-wake-condition.v2";
pub const IWS_WAKE_EVALUATION_SCHEMA_V2:&str="iws-wake-evaluation.v2";
pub const IWS_CURRENT_COMMITMENT_PATH_V1:&str="world.iws_current_commitment.v1";
pub const IWS_WAKE_CONDITIONS_DOMAIN_V2:&str="oasis7.cognition.iws-wake-conditions.v2";
pub const IWS_CURRENT_COMMITMENT_SCHEME_V1:&str="oasis7.iws.normal-current-commitment.v1/sha256";
pub const IWS_CONTINUATION_PROPOSAL_DOMAIN_V2:&str="oasis7.cognition.iws-continuation-proposal.v2";
pub const IWS_WAKE_EVALUATION_DOMAIN_V2:&str="oasis7.cognition.iws-wake-evaluation.v2";
#[derive(Debug,Clone,PartialEq,Eq,Serialize,Deserialize)]
#[serde(transparent)]
pub struct IwsWakeConditionV2{raw:WakeConditionV1}
impl TryFrom<WakeConditionV1> for IwsWakeConditionV2{type Error=WakeConditionError;fn try_from(raw:WakeConditionV1)->Result<Self,Self::Error>{validate_v2_row(&raw)?;Ok(Self{raw})}}
impl IwsWakeConditionV2{
 /// The shared raw DATA layout does not confer Legacy condition semantics.
 pub fn as_data(&self)->&WakeConditionV1{&self.raw}
 pub fn kind(&self)->&str{&self.raw.kind}
 pub fn logical_tick(&self)->Option<u64>{self.raw.logical_tick}
 pub fn event_digest(&self)->Option<&str>{self.raw.event_digest.as_deref()}
 pub fn receipt_id(&self)->Option<&str>{self.raw.receipt_id.as_deref()}
 pub fn subject(&self)->Option<&PreconditionSubjectV1>{self.raw.subject.as_ref()}
 pub fn path(&self)->Option<&str>{self.raw.path_or_rule.as_deref()}
 pub fn at_or_after_tick(tick:u64)->Self{Self{raw:WakeConditionV1{schema_version:IWS_WAKE_CONDITION_SCHEMA_V2.into(),kind:"at_or_after_tick".into(),logical_tick:Some(tick),event_digest:None,receipt_id:None,subject:None,path_or_rule:None,operator:None,expected_value_bytes:None}}}
 /// Explicit V2 input: neither a V1 schema nor the old world.state_root path
 /// is converted to the IWS commitment path.
 pub fn state_predicate(subject:PreconditionSubjectV1,path:String,operator:String,expected_value_bytes:Vec<u8>)->Result<Self,WakeConditionError>{Self::try_from(WakeConditionV1{schema_version:IWS_WAKE_CONDITION_SCHEMA_V2.into(),kind:"state_predicate".into(),logical_tick:None,event_digest:None,receipt_id:None,subject:Some(subject),path_or_rule:Some(path),operator:Some(operator),expected_value_bytes:Some(expected_value_bytes)})}
}
// V2 bounds the encoded predicate before invoking the shared decoder. The
// legacy validator retains its original predicate order and acceptance domain.
fn validate_v2_row(condition:&WakeConditionV1)->Result<(),WakeConditionError>{
 if condition.expected_value_bytes.as_ref().is_some_and(|bytes|bytes.len()>512){return Err(WakeConditionError::new("wake_condition_invalid"))}
 WakeConditionValidator::validate_one_in_scheme(condition,WakeSchemeV2::IwsCurrent)
}
fn raw_validated(conditions:&[IwsWakeConditionV2])->Result<Vec<WakeConditionV1>,WakeConditionError>{
 if conditions.is_empty(){return Err(WakeConditionError::new("wake_conditions_empty"))}if conditions.len()>16{return Err(WakeConditionError::new("wake_condition_invalid"))}
 for condition in conditions{validate_v2_row(&condition.raw)?;}
 Ok(conditions.iter().map(|condition|condition.raw.clone()).collect())
}
pub struct IwsWakeConditionValidatorV2;
impl IwsWakeConditionValidatorV2{
 /// Borrowed structural validation. This is DATA validation, not source
 /// admission; callers must separately meter the bounded shared decoder.
 pub fn validate_data_rows(conditions:&[WakeConditionV1])->Result<(),WakeConditionError>{
  if conditions.is_empty(){return Err(WakeConditionError::new("wake_conditions_empty"))}
  if conditions.len()>16{return Err(WakeConditionError::new("wake_condition_invalid"))}
  for condition in conditions{validate_v2_row(condition)?;}
  WakeConditionValidator::validate_internal_in_scheme(conditions,true,WakeSchemeV2::IwsCurrent)
 }

 pub fn validate(conditions:&[IwsWakeConditionV2])->Result<(),WakeConditionError>{WakeConditionValidator::validate_internal_in_scheme(&raw_validated(conditions)?,true,WakeSchemeV2::IwsCurrent)}
 pub fn canonicalize(conditions:Vec<IwsWakeConditionV2>)->Result<Vec<IwsWakeConditionV2>,WakeConditionError>{let raw=raw_validated(&conditions)?;Ok(WakeConditionValidator::canonicalize_in_scheme(raw,WakeSchemeV2::IwsCurrent)?.into_iter().map(|raw|IwsWakeConditionV2{raw}).collect())}
 pub fn conditions_digest(conditions:&[IwsWakeConditionV2])->Result<String,WakeConditionError>{let raw=raw_validated(conditions)?;WakeConditionValidator::validate_internal_in_scheme(&raw,false,WakeSchemeV2::IwsCurrent)?;Ok(WakeConditionValidator::conditions_digest_in_scheme(&raw,WakeSchemeV2::IwsCurrent))}
 pub fn next_wake_tick_at(conditions:&[IwsWakeConditionV2],tick:u64)->Result<Option<u64>,WakeConditionError>{WakeConditionValidator::next_wake_tick_in_scheme(&raw_validated(conditions)?,tick,WakeSchemeV2::IwsCurrent)}
 pub fn evaluate(conditions:&[IwsWakeConditionV2],context:&IwsWakeEvaluationContextV2)->Result<IwsWakeEvaluationV2,WakeConditionError>{
  let evaluated=WakeConditionValidator::evaluate_in_scheme(&raw_validated(conditions)?,&context.raw,WakeSchemeV2::IwsCurrent)?;
  Ok(IwsWakeEvaluationV2{schema_version:IWS_WAKE_EVALUATION_SCHEMA_V2.into(),current_commitment:context.current_commitment.clone(),status:evaluated.status,reason:evaluated.reason,evaluation_tick:evaluated.evaluation_tick,conditions_digest:evaluated.conditions_digest,evaluation_digest:evaluated.evaluation_digest})
 }
}
#[derive(Debug,Clone,PartialEq,Eq,Serialize,Deserialize)]
pub struct IwsWakeEvaluationV2{pub schema_version:String,pub current_commitment:String,pub status:String,pub reason:String,pub evaluation_tick:u64,pub conditions_digest:String,pub evaluation_digest:String}
/// Pure context DATA. Only Runtime's private typed-source builder may elevate
/// an evaluation to a durable service result; arbitrary DTOs confer no right.
pub struct IwsWakeEvaluationContextV2{raw:WakeEvaluationContext,current_commitment:String}
impl IwsWakeEvaluationContextV2{
 pub fn at(tick:u64,reorg_epoch:u64,current_commitment:&str)->Result<Self,WakeConditionError>{
  if !current_commitment.strip_prefix("sha256:").is_some_and(|s|s.len()==64&&s.bytes().all(|b|b.is_ascii_digit()||(b'a'..=b'f').contains(&b))){return Err(WakeConditionError::new("wake_condition_invalid"))}
  Ok(Self{raw:WakeEvaluationContext::at(tick).with_reorg_epoch(reorg_epoch).with_evaluation_head(current_commitment),current_commitment:current_commitment.into()})
 }
 pub fn with_event(mut self,digest:&str)->Self{self.raw=self.raw.with_event(digest);self}
 pub fn with_receipt(mut self,id:&str)->Self{self.raw=self.raw.with_receipt(id);self}
 pub fn with_subject_predicate_value(mut self,subject:&PreconditionSubjectV1,path:&str,value:&[u8])->Self{self.raw=self.raw.with_subject_predicate_value(subject,path,value);self}
 pub fn with_gc_references(mut self,refs:&[&str])->Self{self.raw=self.raw.with_gc_references(refs);self}
}

/// An untrusted locator, never a Journal/source permission. Runtime joins it
/// to an actual accepted index point or the real same-TX pending Journal.
#[derive(Debug,Clone,PartialEq,Eq,Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IwsJournalReferenceV2{pub journal_ordinal:u64,pub fact_height:u64,pub fact_ordinal:u64,pub original:[u8;32],pub encoded_bytes:u64}
/// Explicit V2 transport, separate from the legacy flat proposal. Source
/// admission must validate the actual registered turn named by this locator.
#[derive(Debug,Clone,PartialEq,Eq,Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IwsContinuationProposalV2{schema_version:String,registration:IwsJournalReferenceV2,proposal:crate::legacy_scheduler::CognitionContinuationProposalV1}
impl IwsContinuationProposalV2{
 pub fn try_new(registration:IwsJournalReferenceV2,proposal:crate::legacy_scheduler::CognitionContinuationProposalV1)->Result<Self,WakeConditionError>{let value=Self{schema_version:"iws-continuation-proposal.v2".into(),registration,proposal};value.validate_data()?;Ok(value)}
 pub fn validate_data(&self)->Result<(),WakeConditionError>{
  let raw=&self.proposal;
  if self.schema_version!="iws-continuation-proposal.v2"||raw.schema_version!=2||self.registration.encoded_bytes==0{return Err(WakeConditionError::new("continuation_proposal_invalid"))}
  if raw.wake_conditions.is_empty()||raw.wake_conditions.len()>16{return Err(WakeConditionError::new("wake_conditions_empty"))}
  raw.validate_identity_for_schema(2)?;
  for value in [&raw.branch_id,&raw.finality_status,&raw.runtime_manifest_hash]{if !crate::legacy_scheduler::is_canonical_identifier(value,128){return Err(WakeConditionError::new("continuation_proposal_invalid"))}}
  IwsWakeConditionValidatorV2::validate_data_rows(&raw.wake_conditions)?;
  if raw.proposal_digest!=self.proposal_digest_v2(){return Err(WakeConditionError::new("continuation_proposal_invalid"))}Ok(())
 }
 /// The registration locator and explicit commitment scheme participate in
 /// the V2 identity. Runtime uses this borrowed payload with its paid encoder.
 pub fn digest_input_v2(&self)->impl Serialize+'_ {
  #[derive(Serialize)]struct Payload<'a,T>{schema_version:&'static str,commitment_scheme:&'static str,registration:&'a IwsJournalReferenceV2,body:T}
  Payload{schema_version:"iws-continuation-proposal.v2",commitment_scheme:IWS_CURRENT_COMMITMENT_SCHEME_V1,registration:&self.registration,body:self.proposal.proposal_digest_input()}
 }
 pub fn proposal_digest_v2(&self)->String{crate::legacy_scheduler::digest(IWS_CONTINUATION_PROPOSAL_DOMAIN_V2,&self.digest_input_v2())}
 /// Provider-side DATA construction only; no Runtime source is issued here.
 pub fn with_computed_digest_v2(registration:IwsJournalReferenceV2,proposal:crate::legacy_scheduler::CognitionContinuationProposalV1)->Result<Self,WakeConditionError>{let mut value=Self{schema_version:"iws-continuation-proposal.v2".into(),registration,proposal};value.proposal.proposal_digest=value.proposal_digest_v2();value.validate_data()?;Ok(value)}
 pub fn registration(&self)->&IwsJournalReferenceV2{&self.registration}
 pub fn as_data(&self)->&crate::legacy_scheduler::CognitionContinuationProposalV1{&self.proposal}
}
