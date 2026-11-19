//! Work-only forms of the original task receipt identities. No returned DATA
//! is a current task, audience authorization, accepted result or execution.
use super::*;
use crate::world_service::service_protocol_refusal_with_work_v1 as refusal;
use crate::world_service::{
    PaidServiceProtocolRefusalV1, ServiceAuthorityWorkErrorV1, request_digest_with_work_v1,
};
use oasis7_wasm_abi::CapabilityCanonicalWorkV1;
use std::mem::size_of;
pub struct PaidGameTaskControlDataV1<C> {
    value: GameTaskControlV1,
    _fee: Option<C>,
}
impl<C> PaidGameTaskControlDataV1<C> {
    pub fn control(&self) -> &GameTaskControlV1 {
        &self.value
    }
    pub(super) fn into_legacy(self) -> GameTaskControlV1 {
        debug_assert!(self._fee.is_none());
        self.value
    }
}
pub struct PaidPublishReceiptDataV1<C> {
    value: PublishReceipt,
    _fee: Option<C>,
}
impl<C> PaidPublishReceiptDataV1<C> {
    pub fn receipt(&self) -> &PublishReceipt {
        &self.value
    }
    pub(super) fn into_legacy(self) -> PublishReceipt {
        debug_assert!(self._fee.is_none());
        self.value
    }
}
fn fail<A: CapabilityCanonicalWorkV1, T>(
    source: &A,
    message: &str,
) -> Result<T, ServiceAuthorityWorkErrorV1<A::Charge>> {
    Err(ServiceAuthorityWorkErrorV1::Protocol(refusal(
        source, message,
    )?))
}
fn copy<A: CapabilityCanonicalWorkV1>(
    source: &A,
    text: &str,
) -> Result<String, ServiceAuthorityWorkErrorV1<A::Charge>> {
    let mut bytes = Vec::with_capacity(text.len());
    for chunk in text.as_bytes().chunks(8192) {
        source
            .charge_work(1, chunk.len() as u64)
            .map_err(ServiceAuthorityWorkErrorV1::Work)?;
        bytes.extend_from_slice(chunk)
    }
    Ok(String::from_utf8(bytes).expect("original UTF-8"))
}
fn reserve<A: CapabilityCanonicalWorkV1>(
    source: &A,
    n: usize,
) -> Result<Option<A::Charge>, ServiceAuthorityWorkErrorV1<A::Charge>> {
    source
        .charge_work(1, n as u64)
        .map_err(ServiceAuthorityWorkErrorV1::Work)?;
    if source.is_metered() {
        Ok(Some(
            source
                .reserve(n)
                .map_err(ServiceAuthorityWorkErrorV1::Work)?,
        ))
    } else {
        Ok(None)
    }
}
pub fn create_control_with_work_v1<A: CapabilityCanonicalWorkV1>(
    request: &SubmitIntentRequest<WorldServicePayloadV1>,
    source: &A,
) -> Result<PaidGameTaskControlDataV1<A::Charge>, ServiceAuthorityWorkErrorV1<A::Charge>> {
    let WorldServicePayloadV1::GameTaskCreate(signed) = &request.signed_payload else {
        return fail(source, "not a Game task create");
    };
    let value = &signed.request;
    let digest = request_digest_with_work_v1("game-task-id", &request.correlation.key, source)?;
    let mut n = size_of::<PaidGameTaskControlDataV1<A::Charge>>()
        + "game-task-".len()
        + digest.as_str().len();
    for text in [
        &value.world_id,
        &value.genesis_digest,
        &value.branch_id,
        &value.agent_id,
        &value.owner_binding,
        &signed.subject_public_key,
        &value.client_operation_id,
    ] {
        source
            .charge_work(1, 0)
            .map_err(ServiceAuthorityWorkErrorV1::Work)?;
        n = n
            .checked_add(text.len())
            .ok_or(ServiceAuthorityWorkErrorV1::Work(
                oasis7_wasm_abi::CanonicalCborStreamError::ResourceLimited("task control graph"),
            ))?;
    }
    let fee = reserve(source, n)?;
    source
        .charge_work(1, ("game-task-".len() + digest.as_str().len()) as u64)
        .map_err(ServiceAuthorityWorkErrorV1::Work)?;
    let mut task_id = String::with_capacity("game-task-".len() + digest.as_str().len());
    task_id.push_str("game-task-");
    task_id.push_str(digest.as_str());
    Ok(PaidGameTaskControlDataV1 {
        value: GameTaskControlV1 {
            schema_revision: 1,
            task_id,
            world_id: copy(source, &value.world_id)?,
            genesis_digest: copy(source, &value.genesis_digest)?,
            branch_id: copy(source, &value.branch_id)?,
            reorg_epoch: value.reorg_epoch,
            agent_id: copy(source, &value.agent_id)?,
            owner_binding: copy(source, &value.owner_binding)?,
            owner_generation: value.owner_generation,
            owner_public_key: copy(source, &signed.subject_public_key)?,
            task_revision: DecimalU64(1),
            task_status: GameTaskStatusV1::Suspended,
            latest_goal_revision: DecimalU64(0),
            active_goal_revision: DecimalU64(0),
            active_goal_reference: None,
            executor_epoch: DecimalU64(0),
            executor_id: None,
            executor_binding: None,
            created_operation_id: copy(source, &value.client_operation_id)?,
        },
        _fee: fee,
    })
}
pub fn verify_publish_with_work_v1<A: CapabilityCanonicalWorkV1>(
    signed: &SignedReadRequest<GameGoalPublishV1>,
    source: &A,
) -> Result<(), ServiceAuthorityWorkErrorV1<A::Charge>> {
    // The original validator is a closed flat record: bounded scalar checks plus
    // these exact String trim/hex traversals; no nested collection or IO call.
    let r = &signed.request;
    for text in [
        &r.world_id,
        &r.genesis_digest,
        &r.task_id,
        &r.agent_id,
        &r.owner_binding,
        &r.branch_id,
        &r.client_operation_id,
        &r.prepare_operation_id,
        &r.owner_public_key,
        &r.goal_ref,
    ] {
        source
            .charge_work(1, text.len() as u64)
            .map_err(ServiceAuthorityWorkErrorV1::Work)?;
    }
    let validation_fee = reserve(
        source,
        size_of::<ServiceAuthorityWorkErrorV1<A::Charge>>() + 4096,
    )?;
    if let Err(message) = r.validate() {
        return Err(ServiceAuthorityWorkErrorV1::Protocol(
            PaidServiceProtocolRefusalV1 {
                message,
                fee: validation_fee,
            },
        ));
    }
    drop(validation_fee);
    source
        .charge_work(
            1,
            (signed.subject_public_key.len() + r.owner_public_key.len()) as u64,
        )
        .map_err(ServiceAuthorityWorkErrorV1::Work)?;
    if signed.subject_public_key != r.owner_public_key {
        return fail(source, "goal publish signer differs from owner");
    }
    if !source.is_metered() {
        return verify_owner_signature_strict(
            GAME_GOAL_PUBLISH_DOMAIN,
            r,
            &signed.subject_public_key,
            &signed.signature_hex,
        )
        .map_err(|message| {
            ServiceAuthorityWorkErrorV1::Protocol(PaidServiceProtocolRefusalV1 {
                message,
                fee: None,
            })
        });
    }
    crate::world_service::private_goal::verify_owner_signature_strict_with_work_v1(
        GAME_GOAL_PUBLISH_DOMAIN,
        r,
        &signed.subject_public_key,
        &signed.signature_hex,
        source,
    )
}
pub fn publish_receipt_with_work_v1<A: CapabilityCanonicalWorkV1>(
    request: &SubmitIntentRequest<WorldServicePayloadV1>,
    height: u64,
    source: &A,
) -> Result<PaidPublishReceiptDataV1<A::Charge>, ServiceAuthorityWorkErrorV1<A::Charge>> {
    let WorldServicePayloadV1::GameGoalPublish(signed) = &request.signed_payload else {
        return fail(source, "not a goal publication");
    };
    verify_publish_with_work_v1(signed, source)?;
    if height == 0 {
        return fail(source, "goal activation height must be committed");
    }
    let r = &signed.request;
    // Preserve original struct evaluation: task_id clone occurs before checked
    // task_revision; remaining reference clones occur only after it succeeds.
    let mut n = size_of::<PaidPublishReceiptDataV1<A::Charge>>();
    for text in [
        &r.task_id,
        &r.goal_ref,
        &r.prepare_operation_id,
        &r.client_operation_id,
    ] {
        source
            .charge_work(1, 0)
            .map_err(ServiceAuthorityWorkErrorV1::Work)?;
        n = n
            .checked_add(text.len())
            .ok_or(ServiceAuthorityWorkErrorV1::Work(
                oasis7_wasm_abi::CanonicalCborStreamError::ResourceLimited("publish receipt graph"),
            ))?;
    }
    let fee = reserve(source, n)?;
    let task_id = copy(source, &r.task_id)?;
    let Some(revision) = r.expected_task_revision.0.checked_add(1) else {
        return fail(source, "task revision overflow");
    };
    Ok(PaidPublishReceiptDataV1 {
        value: PublishReceipt {
            task_id,
            task_revision: DecimalU64(revision),
            goal_reference: GameGoalReferenceV1 {
                goal_revision: r.goal_revision,
                goal_ref: copy(source, &r.goal_ref)?,
                prepare_operation_id: copy(source, &r.prepare_operation_id)?,
                publish_operation_id: copy(source, &r.client_operation_id)?,
                activation_committed_height: DecimalU64(height),
            },
        },
        _fee: fee,
    })
}
