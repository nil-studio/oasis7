//! Original executor control verification and receipt construction with a
//! continuing caller meter. The output is receipt DATA, never task authority.
use super::*;
use crate::world_service::{PaidServiceProtocolRefusalV1, ServiceAuthorityWorkErrorV1};
use oasis7_wasm_abi::{CanonicalCborStreamError, CapabilityCanonicalWorkV1};
use std::mem::size_of;
type Error<C> = ServiceAuthorityWorkErrorV1<C>;

pub struct PaidExecutorReceiptDataV1<C> {
    value: ExecutorReceipt,
    _fee: Option<C>,
}
impl<C> PaidExecutorReceiptDataV1<C> {
    pub fn receipt(&self) -> &ExecutorReceipt {
        &self.value
    }
    pub(super) fn into_legacy(self) -> ExecutorReceipt {
        debug_assert!(self._fee.is_none());
        self.value
    }
}
fn text<A: CapabilityCanonicalWorkV1>(source: &A, value: &str) -> Result<(), Error<A::Charge>> {
    for part in value.as_bytes().chunks(8192) {
        source
            .charge_work(1 + part.len() as u64, part.len() as u64)
            .map_err(Error::Work)?;
    }
    Ok(())
}
fn reserve<A: CapabilityCanonicalWorkV1>(
    source: &A,
    n: usize,
) -> Result<Option<A::Charge>, Error<A::Charge>> {
    source.charge_work(1, n as u64).map_err(Error::Work)?;
    if source.is_metered() {
        Ok(Some(source.reserve(n).map_err(Error::Work)?))
    } else {
        Ok(None)
    }
}
fn fail<A: CapabilityCanonicalWorkV1, T>(source: &A, message: &str) -> Result<T, Error<A::Charge>> {
    Err(Error::Protocol(
        crate::world_service::service_protocol_refusal_with_work_v1(source, message)?,
    ))
}
fn copy<A: CapabilityCanonicalWorkV1>(source: &A, value: &str) -> Result<String, Error<A::Charge>> {
    let mut bytes = Vec::with_capacity(value.len());
    for part in value.as_bytes().chunks(8192) {
        source
            .charge_work(1, part.len() as u64)
            .map_err(Error::Work)?;
        bytes.extend_from_slice(part);
    }
    Ok(String::from_utf8(bytes).expect("original UTF-8"))
}
pub fn strong_key_with_work_v1<A: CapabilityCanonicalWorkV1>(
    key: &str,
    source: &A,
) -> Result<(), Error<A::Charge>> {
    // Keep hex decode -> fixed 32 -> curve -> weak-key order, including the
    // original acceptance of uppercase hex in this executor-only helper.
    text(source, key)?;
    let n = (key.len() / 2)
        .checked_add(size_of::<Vec<u8>>() + size_of::<Error<A::Charge>>() + 128)
        .ok_or(Error::Work(CanonicalCborStreamError::ResourceLimited(
            "executor key layout",
        )))?;
    let fee = reserve(source, n)?;
    let bytes = match hex::decode(key) {
        Ok(bytes) => bytes,
        Err(_) => {
            return Err(Error::Protocol(PaidServiceProtocolRefusalV1::owned(
                "invalid executor key".into(),
                fee,
            )));
        }
    };
    let Ok(bytes): Result<[u8; 32], _> = bytes.try_into() else {
        return Err(Error::Protocol(PaidServiceProtocolRefusalV1::owned(
            "invalid executor key".into(),
            fee,
        )));
    };
    source.charge_work(1, 32).map_err(Error::Work)?;
    let key = match ed25519_dalek::VerifyingKey::from_bytes(&bytes) {
        Ok(key) => key,
        Err(_) => {
            return Err(Error::Protocol(PaidServiceProtocolRefusalV1::owned(
                "invalid executor key".into(),
                fee,
            )));
        }
    };
    source.charge_work(1, 32).map_err(Error::Work)?;
    if key.is_weak() {
        return Err(Error::Protocol(PaidServiceProtocolRefusalV1::owned(
            "weak executor key".into(),
            fee,
        )));
    }
    Ok(())
}
pub fn verify_executor_control_with_work_v1<A: CapabilityCanonicalWorkV1>(
    signed: &SignedReadRequest<GameExecutorControlV1>,
    source: &A,
) -> Result<(), Error<A::Charge>> {
    let r = &signed.request;
    // The original validator contains only these fixed String predicates and
    // closed scalar/Option checks. No codec, collection scan or crypto call is
    // hidden in it. Exhaustive destructuring pins all owned children.
    let GameExecutorControlV1 {
        api_rev: _,
        client_operation_id,
        fence: f,
        operation,
    } = r;
    let GameTaskFenceV1 {
        world_id,
        genesis_digest,
        branch_id,
        reorg_epoch: _,
        agent_id,
        owner_binding,
        owner_generation: _,
        owner_public_key,
        task_id,
        task_revision: _,
        task_status: _,
        latest_goal_revision: _,
        active_goal_revision: _,
        active_goal_reference,
        executor_epoch: _,
        executor_binding,
    } = f;
    for value in [
        world_id,
        genesis_digest,
        branch_id,
        agent_id,
        owner_binding,
        owner_public_key,
        task_id,
        client_operation_id,
    ] {
        text(source, value)?;
    }
    if let Some(goal) = active_goal_reference {
        let GameGoalReferenceV1 {
            goal_revision: _,
            goal_ref,
            prepare_operation_id,
            publish_operation_id,
            activation_committed_height: _,
        } = goal;
        for value in [goal_ref, prepare_operation_id, publish_operation_id] {
            text(source, value)?;
        }
    }
    if let Some(binding) = executor_binding {
        let GameExecutorBindingV1 {
            executor_id,
            executor_public_key,
            delegation_generation: _,
            executor_epoch: _,
        } = binding;
        for value in [executor_id, executor_public_key] {
            text(source, value)?;
        }
    }
    if let GameExecutorOperationV1::Bind {
        executor_id,
        executor_public_key,
        delegation_generation: _,
    } = operation
    {
        text(source, executor_id)?;
        text(source, executor_public_key)?;
    }
    source.charge_work(128, 0).map_err(Error::Work)?;
    let fee = reserve(source, size_of::<Error<A::Charge>>() + 128)?;
    if let Err(message) = r.validate() {
        return Err(Error::Protocol(PaidServiceProtocolRefusalV1::owned(
            message, fee,
        )));
    }
    drop(fee);
    if let GameExecutorOperationV1::Bind {
        executor_public_key,
        ..
    } = operation
    {
        strong_key_with_work_v1(executor_public_key, source)?;
    }
    text(source, &signed.subject_public_key)?;
    text(source, owner_public_key)?;
    if signed.subject_public_key != *owner_public_key {
        return fail(source, "executor control requires owner signature");
    }
    crate::world_service::private_goal::verify_owner_signature_strict_with_work_v1(
        GAME_EXECUTOR_CONTROL_DOMAIN,
        r,
        &signed.subject_public_key,
        &signed.signature_hex,
        source,
    )
}
pub fn executor_receipt_with_work_v1<A: CapabilityCanonicalWorkV1>(
    request: &SubmitIntentRequest<WorldServicePayloadV1>,
    source: &A,
) -> Result<PaidExecutorReceiptDataV1<A::Charge>, Error<A::Charge>> {
    let WorldServicePayloadV1::GameExecutorControl(signed) = &request.signed_payload else {
        return fail(source, "not executor control");
    };
    verify_executor_control_with_work_v1(signed, source)?;
    let r = &signed.request;
    source.charge_work(1, 0).map_err(Error::Work)?;
    let Some(epoch) = r.fence.executor_epoch.0.checked_add(1).map(DecimalU64) else {
        return fail(source, "executor epoch overflow");
    };
    let payload = match &r.operation {
        GameExecutorOperationV1::Bind {
            executor_id,
            executor_public_key,
            ..
        } => executor_id.len().checked_add(executor_public_key.len()),
        GameExecutorOperationV1::Stop => Some(0),
    }
    .and_then(|n| n.checked_add(r.fence.task_id.len()))
    .and_then(|n| n.checked_add(size_of::<PaidExecutorReceiptDataV1<A::Charge>>()))
    .ok_or(Error::Work(CanonicalCborStreamError::ResourceLimited(
        "executor receipt layout",
    )))?;
    let fee = reserve(source, payload)?;
    let binding = match &r.operation {
        GameExecutorOperationV1::Bind {
            executor_id,
            executor_public_key,
            delegation_generation,
        } => {
            strong_key_with_work_v1(executor_public_key, source)?;
            Some(GameExecutorBindingV1 {
                executor_id: copy(source, executor_id)?,
                executor_public_key: copy(source, executor_public_key)?,
                delegation_generation: *delegation_generation,
                executor_epoch: epoch,
            })
        }
        GameExecutorOperationV1::Stop => None,
    };
    let task_id = copy(source, &r.fence.task_id)?;
    source.charge_work(1, 0).map_err(Error::Work)?;
    let Some(task_revision) = r.fence.task_revision.0.checked_add(1).map(DecimalU64) else {
        return fail(source, "task revision overflow");
    };
    Ok(PaidExecutorReceiptDataV1 {
        value: ExecutorReceipt {
            task_id,
            task_revision,
            executor_epoch: epoch,
            task_status: if binding.is_some() {
                GameTaskStatusV1::Active
            } else {
                GameTaskStatusV1::Suspended
            },
            executor_binding: binding,
        },
        _fee: fee,
    })
}
