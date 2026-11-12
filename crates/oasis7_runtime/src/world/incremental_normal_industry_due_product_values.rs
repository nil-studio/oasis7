//! Actual closed Product DTO copies and diagnostics. No unbounded formatting
//! or clone is hidden outside a reservation; these are data, never permits.
use super::*;
use oasis7_formal_protocol::{
    controlled_authority::formal::bounded_context::FormalBoundedContextV1,
    incremental_value::ValueTreeError,
};
use oasis7_proto::memory_budget::MemoryClass;
use std::{
    fmt::{self, Write},
    mem::size_of,
};
fn limited() -> BodyMutationError {
    ValueTreeError::ResourceLimited("due product value layout").into()
}
fn charge(ctx: &mut FormalBoundedContextV1<'_>, n: usize) -> Result<(), BodyMutationError> {
    ctx.charge(1, u64::try_from(n).map_err(|_| limited())?)
        .map_err(|e| BodyMutationError::Original(Box::new(e)))
}
pub(super) fn decision_heap_v1(
    value: &oasis7_wasm_abi::ProductValidationDecision,
    ctx: &mut FormalBoundedContextV1<'_>,
) -> Result<usize, BodyMutationError> {
    let mut n = value.product_id.len();
    for list in [&value.quality_levels, &value.notes] {
        n = n
            .checked_add(
                list.len()
                    .checked_mul(size_of::<String>())
                    .ok_or_else(limited)?,
            )
            .ok_or_else(limited)?;
        for text in list {
            charge(ctx, text.len())?;
            n = n.checked_add(text.len()).ok_or_else(limited)?;
        }
    }
    Ok(n)
}
pub(super) fn copy_receipt_v1(
    value: &crate::ProductValidationReceiptV1,
    ctx: &mut FormalBoundedContextV1<'_>,
) -> Result<ReservedValue<crate::ProductValidationReceiptV1>, BodyMutationError> {
    let n = decision_heap_v1(&value.decision, ctx)?
        .checked_add(value.requester_agent_id.len())
        .and_then(|n| n.checked_add(value.module_id.len()))
        .and_then(|n| n.checked_add(value.stack.kind.len()))
        .and_then(|n| n.checked_add(value.failure_detail.as_ref().map_or(0, String::len)))
        .and_then(|n| n.checked_add(size_of::<ReservedValue<crate::ProductValidationReceiptV1>>()))
        .ok_or_else(limited)?;
    charge(ctx, n)?;
    let fee = ctx
        .memory()
        .reserve(n, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    Ok(fee.retain(value.clone()))
}
pub(super) fn copy_text_v1(
    value: &str,
    ctx: &mut FormalBoundedContextV1<'_>,
) -> Result<ReservedValue<String>, BodyMutationError> {
    let n = value
        .len()
        .checked_add(size_of::<ReservedValue<String>>())
        .ok_or_else(limited)?;
    charge(ctx, n)?;
    let fee = ctx
        .memory()
        .reserve(n, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    Ok(fee.retain(value.to_owned()))
}
/// Count exactly the old Debug diagnostic before allocating its String. Both
/// passes stream through the original formatter; no replacement text is used.
pub(super) fn debug_text_v1(
    value: &impl fmt::Debug,
    ctx: &mut FormalBoundedContextV1<'_>,
) -> Result<ReservedValue<String>, BodyMutationError> {
    struct Counter<'a, 'b> {
        ctx: &'a mut FormalBoundedContextV1<'b>,
        n: usize,
        first: Option<BodyMutationError>,
    }
    impl Write for Counter<'_, '_> {
        fn write_str(&mut self, s: &str) -> fmt::Result {
            if let Err(e) = charge(self.ctx, s.len()) {
                self.first = Some(e);
                return Err(fmt::Error);
            }
            self.n = self.n.checked_add(s.len()).ok_or(fmt::Error)?;
            Ok(())
        }
    }
    let mut count = Counter {
        ctx,
        n: 0,
        first: None,
    };
    let counted = write!(&mut count, "{value:?}");
    if let Some(e) = count.first.take() {
        return Err(e);
    }
    if counted.is_err() {
        return Err(limited());
    }
    let n = count.n;
    drop(count);
    let total = n
        .checked_add(size_of::<ReservedValue<String>>())
        .ok_or_else(limited)?;
    charge(ctx, total)?;
    let fee = ctx
        .memory()
        .reserve(total, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let mut text = String::with_capacity(n);
    write!(&mut text, "{value:?}").map_err(|_| limited())?;
    if text.len() != n || text.capacity() != n {
        return Err(limited());
    }
    Ok(fee.retain(text))
}
pub(super) struct PaidProductDecisionV1 {
    pub decision: oasis7_wasm_abi::ProductValidationDecision,
    pub detail: Option<String>,
    _fee: MemoryReservation,
}
pub(super) fn failed_decision_v1(
    module: &str,
    stack: &crate::MaterialStack,
    reason: &str,
    ctx: &mut FormalBoundedContextV1<'_>,
) -> Result<PaidProductDecisionV1, BodyMutationError> {
    const PREFIX: &str = "product validation module failed and output was rejected: module_id=";
    const MIDDLE: &str = " reason=";
    let detail = PREFIX
        .len()
        .checked_add(module.len())
        .and_then(|n| n.checked_add(MIDDLE.len()))
        .and_then(|n| n.checked_add(reason.len()))
        .ok_or_else(limited)?;
    // Original formatter capacity is <2*actual nonempty detail; its notes
    // clone owns one more exact payload. Keep that geometric bound plus
    // the rejected product-id and one original Vec<String> slot.
    let n = detail
        .checked_mul(3)
        .and_then(|n| n.checked_add(stack.kind.len()))
        .and_then(|n| n.checked_add(size_of::<String>() + size_of::<PaidProductDecisionV1>()))
        .ok_or_else(limited)?;
    charge(ctx, n)?;
    let fee = ctx
        .memory()
        .reserve(n, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    // A borrowed reason first becomes a String in the legacy Into<String>
    // boundary; admit that real temporary independently before its copy.
    charge(ctx, reason.len())?;
    let _reason = ctx
        .memory()
        .reserve(reason.len(), MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let (decision, detail) =
        crate::world::World::fail_closed_product_validation(module, stack, reason);
    Ok(PaidProductDecisionV1 {
        decision,
        detail: Some(detail),
        _fee: fee,
    })
}
pub(super) fn copied_decision_v1(
    decision: &oasis7_wasm_abi::ProductValidationDecision,
    ctx: &mut FormalBoundedContextV1<'_>,
) -> Result<PaidProductDecisionV1, BodyMutationError> {
    let n = decision_heap_v1(decision, ctx)?
        .checked_add(size_of::<PaidProductDecisionV1>())
        .ok_or_else(limited)?;
    charge(ctx, n)?;
    let fee = ctx
        .memory()
        .reserve(n, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    Ok(PaidProductDecisionV1 {
        decision: decision.clone(),
        detail: None,
        _fee: fee,
    })
}

pub(super) fn formatted_text_v1(
    args: fmt::Arguments<'_>,
    ctx: &mut FormalBoundedContextV1<'_>,
) -> Result<ReservedValue<String>, BodyMutationError> {
    struct Counter<'a, 'b> {
        ctx: &'a mut FormalBoundedContextV1<'b>,
        n: usize,
        first: Option<BodyMutationError>,
    }
    impl Write for Counter<'_, '_> {
        fn write_str(&mut self, s: &str) -> fmt::Result {
            if let Err(e) = charge(self.ctx, s.len()) {
                self.first = Some(e);
                return Err(fmt::Error);
            }
            self.n = self.n.checked_add(s.len()).ok_or(fmt::Error)?;
            Ok(())
        }
    }
    let mut count = Counter {
        ctx,
        n: 0,
        first: None,
    };
    let counted = fmt::write(&mut count, args);
    if let Some(e) = count.first.take() {
        return Err(e);
    }
    if counted.is_err() {
        return Err(limited());
    }
    let n = count.n;
    drop(count);
    charge(ctx, n)?;
    let fee = ctx
        .memory()
        .reserve(
            n.checked_add(size_of::<ReservedValue<String>>())
                .ok_or_else(limited)?,
            MemoryClass::Ordinary,
        )
        .map_err(ValueTreeError::Memory)?;
    let mut value = String::with_capacity(n);
    fmt::write(&mut value, args).map_err(|_| limited())?;
    if value.len() != n || value.capacity() != n {
        return Err(limited());
    }
    Ok(fee.retain(value))
}

pub(super) fn attempt_v1(
    job: &crate::RecipeJobState,
    module: &str,
    stack: &crate::MaterialStack,
    index: usize,
    ctx: &mut FormalBoundedContextV1<'_>,
) -> Result<ReservedValue<crate::ProductValidationAttemptV1>, BodyMutationError> {
    let n = job
        .requester_agent_id
        .len()
        .checked_add(module.len())
        .and_then(|n| n.checked_add(stack.kind.len()))
        .and_then(|n| n.checked_add(size_of::<ReservedValue<crate::ProductValidationAttemptV1>>()))
        .ok_or_else(limited)?;
    charge(ctx, n)?;
    let fee = ctx
        .memory()
        .reserve(n, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    Ok(fee.retain(crate::ProductValidationAttemptV1 {
        job_id: job.job_id,
        validation_index: Some(index as u32),
        requester_agent_id: job.requester_agent_id.clone(),
        module_id: module.to_owned(),
        stack: stack.clone(),
    }))
}
pub(super) fn receipt_v1(
    job: &crate::RecipeJobState,
    module: &str,
    stack: &crate::MaterialStack,
    index: usize,
    value: &PaidProductDecisionV1,
    ctx: &mut FormalBoundedContextV1<'_>,
) -> Result<ReservedValue<crate::ProductValidationReceiptV1>, BodyMutationError> {
    let n = decision_heap_v1(&value.decision, ctx)?
        .checked_add(job.requester_agent_id.len())
        .and_then(|n| n.checked_add(module.len()))
        .and_then(|n| n.checked_add(stack.kind.len()))
        .and_then(|n| n.checked_add(value.detail.as_ref().map_or(0, String::len)))
        .and_then(|n| n.checked_add(size_of::<ReservedValue<crate::ProductValidationReceiptV1>>()))
        .ok_or_else(limited)?;
    charge(ctx, n)?;
    let fee = ctx
        .memory()
        .reserve(n, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    Ok(fee.retain(crate::ProductValidationReceiptV1 {
        job_id: job.job_id,
        validation_index: Some(index as u32),
        requester_agent_id: job.requester_agent_id.clone(),
        module_id: module.to_owned(),
        stack: stack.clone(),
        decision: value.decision.clone(),
        failure_detail: value.detail.clone(),
    }))
}
