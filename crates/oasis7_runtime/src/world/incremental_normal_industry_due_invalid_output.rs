//! Original invalid-output diagnostic, bound to a real private due call.
use super::*;
use oasis7_formal_protocol::{
    controlled_authority::formal::bounded_context::FormalBoundedContextV1,
    incremental_value::ValueTreeError,
};
use oasis7_proto::memory_budget::MemoryClass;
use std::mem::size_of;
pub(super) struct InvalidDueProductOutputV1 {
    pub(super) failure: ReservedValue<oasis7_wasm_abi::ModuleCallFailure>,
    guard: PreparedGuard,
    identity: WorkIdentityV1,
    memory: MemoryBudget,
    now: u64,
}
impl InvalidDueProductOutputV1 {
    pub(super) fn validate<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        memory: &MemoryBudget,
        now: u64,
        identity: WorkIdentityV1,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        tx.validate_guard(&self.guard, live)?;
        if self.identity != identity
            || !self.memory.same_pool(memory)
            || !self.failure.uses_budget(memory)
            || self.now != now
            || !tx.uses_budget(memory)
        {
            return Err(BodyMutationError::Invalid(
                "due invalid output real call source",
            ));
        }
        Ok(())
    }
}
pub(super) enum DecodedDueProductOutputV1 {
    Decision(ReservedValue<Result<oasis7_wasm_abi::ProductValidationDecision, serde_json::Error>>),
    Invalid(InvalidDueProductOutputV1),
}
pub(super) fn decode_product_output_v1<M: MerkleNodeReader>(
    call: &PreparedDueProductCallV1<'_>,
    output: &oasis7_wasm_abi::ModuleOutput,
    tx: &ExecutionTransaction<'_, M>,
    common: &mut FormalBoundedContextV1<'_>,
    now: u64,
    live: &ExecutionFence,
) -> Result<DecodedDueProductOutputV1, BodyMutationError> {
    use super::decision::{ProductDecisionPayloadV1, decode_actual_product_decision_v1};
    call.validate_evidence_identity(tx, common, now, live)?;
    // The call was consumed by the fixed actual worker; it is retained here as
    // source data, not re-authorized under its pre-execution guard revision.
    let detail = match decode_actual_product_decision_v1(output, common)? {
        ProductDecisionPayloadV1::Decoded(value) if value.is_ok() => {
            return Ok(DecodedDueProductOutputV1::Decision(value));
        }
        ProductDecisionPayloadV1::Decoded(value) => {
            let error = value
                .as_ref()
                .err()
                .ok_or(BodyMutationError::Invalid("due decision decoder result"))?;
            product_values::formatted_text_v1(
                format_args!(
                    "decode {} failed: {error}",
                    crate::world::economy::PRODUCT_VALIDATION_EMIT_KIND
                ),
                common,
            )?
        }
        ProductDecisionPayloadV1::ForbiddenEffects => {
            product_values::copy_text_v1("product module output must not contain effects", common)?
        }
        ProductDecisionPayloadV1::Missing => product_values::formatted_text_v1(
            format_args!(
                "missing {} emit in module output",
                crate::world::economy::PRODUCT_VALIDATION_EMIT_KIND
            ),
            common,
        )?,
        ProductDecisionPayloadV1::Duplicate => product_values::formatted_text_v1(
            format_args!(
                "multiple {} emits in module output",
                crate::world::economy::PRODUCT_VALIDATION_EMIT_KIND
            ),
            common,
        )?,
    };
    let trace = product_values::formatted_text_v1(
        format_args!(
            "action-{}-{}-product-{}",
            call.job().job_id,
            call.module_id(),
            call.output_index()
        ),
        common,
    )?;
    let n = call
        .module_id()
        .len()
        .checked_add(trace.len())
        .and_then(|n| n.checked_add(detail.len()))
        .and_then(|n| n.checked_add(size_of::<InvalidDueProductOutputV1>()))
        .ok_or(BodyMutationError::Invalid("due invalid output layout"))?;
    common
        .charge(
            1,
            u64::try_from(n)
                .map_err(|_| BodyMutationError::Invalid("due invalid output layout"))?,
        )
        .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
    let fee = common
        .memory()
        .reserve(n, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let source = InvalidDueProductOutputV1 {
        failure: fee.retain(oasis7_wasm_abi::ModuleCallFailure {
            module_id: call.module_id().to_owned(),
            trace_id: trace.to_string(),
            code: oasis7_wasm_abi::ModuleCallErrorCode::InvalidOutput,
            detail: detail.to_string(),
        }),
        guard: tx.guard(),
        identity: common.work().identity(),
        memory: common.memory().clone(),
        now,
    };
    source.validate(tx, common.memory(), now, common.work().identity(), live)?;
    Ok(DecodedDueProductOutputV1::Invalid(source))
}
