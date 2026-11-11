//! Explicit raw-output sandbox interface for admitted host decoders.
//! Existing ModuleSandbox implementers do not acquire this interface implicitly.
use crate::{ModuleCallFailure, ModuleCallRequest, ModuleSandbox};
/// Diagnostic bytes retained across the raw sandbox/host boundary. Identity
/// strings remain the original request identity and are admitted by its owner.
pub const RAW_SANDBOX_FAILURE_DETAIL_MAX_BYTES_V1: usize = 4096;
pub trait ModuleRawOutputSinkV1 {
    /// Exactly one complete, actually executed result. A failed/repeated call
    /// must poison the sink; reporting Ok without a result cannot finish it.
    fn consume_output(&mut self, bytes: &[u8]) -> Result<(), ModuleCallFailure>;
}
/// The actual backend executes the original request and offers its raw bytes
/// before typed output deserialization. There is deliberately no legacy/fake
/// default which could turn a no-op callback into execution evidence.
pub trait ModuleSandboxRawOutputV1: ModuleSandbox {
    fn call_to(
        &mut self,
        request: &ModuleCallRequest,
        sink: &mut dyn ModuleRawOutputSinkV1,
    ) -> Result<(), ModuleCallFailure>;
}

fn count_exceeds_limit(count: usize, limit: u32) -> bool {
    u32::try_from(count).map_or(true, |n| n > limit)
}

/// Shared allocation-free output limit kernel used by legacy and admitted hosts.
pub fn validate_module_output_limits_v1(
    request: &ModuleCallRequest,
    output: &crate::ModuleOutput,
) -> Result<(), (crate::ModuleCallErrorCode, &'static str)> {
    use crate::ModuleCallErrorCode;
    if count_exceeds_limit(output.effects.len(), request.limits.max_effects) {
        return Err((ModuleCallErrorCode::EffectLimitExceeded, "effects exceeded"));
    }
    if count_exceeds_limit(output.emits.len(), request.limits.max_emits) {
        return Err((ModuleCallErrorCode::EmitLimitExceeded, "emits exceeded"));
    }
    if output.output_bytes > request.limits.max_output_bytes {
        return Err((ModuleCallErrorCode::OutputTooLarge, "output bytes exceeded"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn count_exceeds_limit_treats_usize_overflow_as_exceeded() {
        assert!(super::count_exceeds_limit(usize::MAX, 1));
        assert!(!super::count_exceeds_limit(1, 1));
    }
}
