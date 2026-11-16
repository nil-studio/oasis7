//! The complete original receipt readback rule over one immutable parsed
//! Cognition DTO. This is observation DATA; a receipt ID grants no authority.
use super::*;
use oasis7_wasm_abi::{CapabilityCanonicalWorkV1,CanonicalCborStreamError};
use oasis7_world_state::cognition_feedback_contract::readonly_work as paid_work;
#[derive(Debug)]pub(in crate::world)enum ReceiptReadErrorV1{World(WorldError),Work(CanonicalCborStreamError)}
impl From<WorldError>for ReceiptReadErrorV1{fn from(e:WorldError)->Self{Self::World(e)}}
impl From<CanonicalCborStreamError>for ReceiptReadErrorV1{fn from(e:CanonicalCborStreamError)->Self{Self::Work(e)}}
impl From<paid_work::FeedbackReadErrorV1>for ReceiptReadErrorV1{fn from(e:paid_work::FeedbackReadErrorV1)->Self{match e{paid_work::FeedbackReadErrorV1::Business(e)=>Self::World(cognition_validation(e.code())),paid_work::FeedbackReadErrorV1::Work(e)=>Self::Work(e)}}}
pub(in crate::world)struct ParsedReceiptObservationDataV1<C>{projection:CognitionProjection,_fee:Option<C>}
pub(in crate::world)struct PaidReadRuntimeReceiptLineageV1<C>{lineage:RuntimeReceiptLineageV1,_fee:Option<C>}
impl<C>PaidReadRuntimeReceiptLineageV1<C>{pub(in crate::world)fn value(&self)->&RuntimeReceiptLineageV1{&self.lineage}}
pub(in crate::world)fn parse_receipt_observation_with_work_v1<A:CapabilityCanonicalWorkV1>(value:JsonValue,source:&A)->Result<ParsedReceiptObservationDataV1<A::Charge>,ReceiptReadErrorV1>{
 let fee=paid_work::phase(&value,source)?;
 let projection=serde_json::from_value(value).map_err(|error|cognition_error("invalid_cognition_projection",error))?;
 Ok(ParsedReceiptObservationDataV1{projection,_fee:fee})
}
pub(in crate::world)fn read_receipt_observation_with_work_v1<A:CapabilityCanonicalWorkV1>(snapshot:&ParsedReceiptObservationDataV1<A::Charge>,receipt_id:&str,source:&A)->Result<PaidReadRuntimeReceiptLineageV1<A::Charge>,ReceiptReadErrorV1>{
 let parsed=&snapshot.projection;
 let mut selected=None;
 for lineage in &parsed.receipt_lineage_registry{source.charge_work(1,(lineage.receipt_id.len()+receipt_id.len())as u64)?;if lineage.receipt_id==receipt_id{selected=Some(lineage);break}}
 let lineage=selected.ok_or_else(||cognition_validation("receipt_lineage_missing"))?;
 lineage.validate_with_work_v1(source)?;
 let mut selected=None;
 for marker in &parsed.commit_records{source.charge_work(1,(marker.status.len()+marker.receipt_id.len()+receipt_id.len())as u64)?;if marker.status=="committed"&&marker.receipt_id==receipt_id{selected=Some(marker);break}}
 let marker=selected.ok_or_else(||cognition_validation("receipt_commit_marker_missing"))?;
 super::super::cognition_persistence_support::validate_marker_with_work_v1(marker,source)?;
 let mut selected=None;
 for response in &parsed.responses{source.charge_work(1,(response.envelope_digest.len()+marker.envelope_digest.len())as u64)?;if response.envelope_digest==marker.envelope_digest{selected=Some(response);break}}
 if let Some(response)=selected{crate::world::cognition_persistence_validation::validate_response_lineage_binding_with_work_v1(response,marker,source)?;}
 super::super::cognition_persistence_support::validate_lineage_binding_with_work_v1(parsed,marker,lineage,source)?;
 let derived=RuntimeReceiptLineageV1::from_durable_commit_record_with_work_v1(marker,source)?;
 let derived=derived.value().ok_or_else(||cognition_validation("receipt_lineage_dense_marker_required"))?;
 let _comparison=paid_work::phase(lineage,source)?;
 if derived!=lineage{return Err(cognition_validation("receipt_lineage_binding_mismatch").into())}
 let fee=paid_work::phase(lineage,source)?;
 Ok(PaidReadRuntimeReceiptLineageV1{lineage:lineage.clone(),_fee:fee})
}
pub(super)fn legacy(value:JsonValue,receipt_id:&str)->Result<RuntimeReceiptLineageV1,WorldError>{
 let source=oasis7_wasm_abi::LegacyCapabilityCanonicalWorkV1;
 let result=parse_receipt_observation_with_work_v1(value,&source).and_then(|snapshot|read_receipt_observation_with_work_v1(&snapshot,receipt_id,&source));
 match result{Ok(value)=>Ok(value.lineage),Err(ReceiptReadErrorV1::World(e))=>Err(e),Err(ReceiptReadErrorV1::Work(_))=>unreachable!("legacy receipt work cannot fail")}
}
