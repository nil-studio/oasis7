//! Original complete terminal original/witness validation. The fixed source
//! adapter supplies accounting and the original codec/crypto kernels only.
use super::*;
pub(crate) trait TerminalSignatureOpsV2{
 type Error;type Witness:std::borrow::Borrow<admission::CanonicalIntentResultV1>;
 fn record_work(&self,record:&TaskTerminalRecordV2)->Result<(),Self::Error>;
 fn verify_original(&self,original:&SignedReadRequest<GameTaskRevokeAndCloseV2>)->Result<(),Self::Error>;
 fn validate_cleanup(&self,value:&GameTaskRevokeAndCloseV2)->Result<(),Self::Error>;
 fn validate_fence(&self,value:&GameTaskFenceV1)->Result<(),Self::Error>;
 fn hash<T:serde::Serialize>(&self,value:&T)->Result<String,Self::Error>;
 fn raw_digest(&self,bytes:&[u8])->Result<String,Self::Error>;
 fn decode_witness(&self,bytes:&[u8])->Result<Self::Witness,Self::Error>;
 fn validate_result(&self,value:&admission::CanonicalIntentResultV1)->Result<(),Self::Error>;
 fn executor_receipt(&self,request:&admission::SubmitIntentRequest<WorldServicePayloadV1>)->Result<admission::game_task::ExecutorReceipt,Self::Error>;
 fn clone_fence(&self,fence:&GameTaskFenceV1)->Result<GameTaskFenceV1,Self::Error>;
 fn business(&self,note:&'static str)->Self::Error;
}
pub(crate) fn signature_with_work_v2<O:TerminalSignatureOpsV2>(record:&TaskTerminalRecordV2,ops:&O)->Result<(),O::Error>{
    ops.record_work(record)?;
    ops.verify_original(&record.original)?;
    let p = &record.original.request;
    ops.validate_cleanup(p)?;
    let c = &record.closed;
    ops.validate_fence(&p.permitted_branches.exact_initial_active)?;
    if record.original.subject_public_key
        != p.permitted_branches.exact_initial_active.owner_public_key
        || c.close_original_digest != ops.hash(&record.original)?
        || record.committed_at_unix_ms > p.cleanup_until_ms.0
    {
        return Err(ops.business("terminal original or deadline mismatch"));
    }
    match &c.selected_predicate {
        GameTaskClosePredicateV2::InitialActive => {
            if c.actual_before_fence != p.permitted_branches.exact_initial_active
                || record.stop_predecessor_canonical_cbor.is_some()
            {
                return Err(ops.business("terminal initial predicate mismatch"));
            }
        }
        GameTaskClosePredicateV2::OwnStopSuspended(reference) => {
            let intent = p
                .permitted_branches
                .own_stop_intent
                .as_ref()
                .ok_or_else(||ops.business("OwnStop was not allowed by the signed conditions"))?;
            let bytes = record
                .stop_predecessor_canonical_cbor
                .as_ref()
                .ok_or_else(||ops.business("terminal own-Stop full witness missing"))?;
            let result_owner=ops.decode_witness(bytes)?;
            let result=std::borrow::Borrow::<admission::CanonicalIntentResultV1>::borrow(&result_owner);
            ops.validate_result(result)?;
            let WorldServicePayloadV1::GameExecutorControl(stop) = &result.request.signed_payload
            else {
                return Err(ops.business("terminal witness is not Stop"));
            };
            if stop.request.operation != GameExecutorOperationV1::Stop
                || stop.request.fence != p.permitted_branches.exact_initial_active
                || stop.subject_public_key != record.original.subject_public_key
                || stop.request.client_operation_id != intent.operation_id
                || ops.hash(&(
                    GAME_EXECUTOR_CONTROL_DOMAIN,
                    &stop.subject_public_key,
                    &stop.request,
                ))? != intent.intent_digest
                || result.rejected.is_some()
                || result.committed_height > record.committed_height
                || reference.operation_id != stop.request.client_operation_id
                || reference.original_digest != ops.hash(stop)?
                || reference.canonical_result_digest != ops.raw_digest(bytes)?
                || reference.post_fence != c.actual_before_fence
            {
                return Err(ops.business("terminal own-Stop lineage mismatch"));
            }
            let receipt = ops.executor_receipt(&result.request)?;
            let mut expected = ops.clone_fence(&stop.request.fence)?;
            expected.task_revision = receipt.task_revision;
            expected.executor_epoch = receipt.executor_epoch;
            expected.executor_binding = receipt.executor_binding;
            expected.task_status = receipt.task_status;
            if expected != c.actual_before_fence {
                return Err(ops.business("terminal own-Stop derived fence mismatch"));
            }
        }
    }
    Ok(())
}
pub(super) struct Legacy;
impl TerminalSignatureOpsV2 for Legacy{
 type Error=String;type Witness=admission::CanonicalIntentResultV1;
 fn record_work(&self,_:&TaskTerminalRecordV2)->Result<(),String>{Ok(())}
 fn verify_original(&self,original:&SignedReadRequest<GameTaskRevokeAndCloseV2>)->Result<(),String>{oasis7_client_api::world_service::private_goal::verify_owner_signature_strict(GAME_TASK_CLEANUP_DOMAIN_V2,&original.request,&original.subject_public_key,&original.signature_hex)}
 fn validate_cleanup(&self,v:&GameTaskRevokeAndCloseV2)->Result<(),String>{v.validate()}
 fn validate_fence(&self,v:&GameTaskFenceV1)->Result<(),String>{v.validate()}
 fn hash<T:serde::Serialize>(&self,value:&T)->Result<String,String>{super::hash(value)}
 fn raw_digest(&self,bytes:&[u8])->Result<String,String>{Ok(blake3::hash(bytes).to_hex().to_string())}
 fn decode_witness(&self,bytes:&[u8])->Result<Self::Witness,String>{let result:admission::CanonicalIntentResultV1=serde_cbor::from_slice(bytes).map_err(|_|"invalid terminal Stop witness codec")?;if serde_cbor::to_vec(&result).map_err(|e|e.to_string())?!=bytes{return Err("terminal Stop witness is not exact canonical result bytes".into())}Ok(result)}
 fn validate_result(&self,result:&admission::CanonicalIntentResultV1)->Result<(),String>{let key=admission::correlation::key_digest(&result.request.correlation.key)?;admission::correlation::validate_result(&key,&serde_json::to_value(result).map_err(|e|e.to_string())?)}
 fn executor_receipt(&self,request:&admission::SubmitIntentRequest<WorldServicePayloadV1>)->Result<admission::game_task::ExecutorReceipt,String>{admission::game_task::executor_receipt(request)}
 fn clone_fence(&self,fence:&GameTaskFenceV1)->Result<GameTaskFenceV1,String>{Ok(fence.clone())}
 fn business(&self,note:&'static str)->String{note.into()}
}
