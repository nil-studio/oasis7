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
    use super::*;
    use crate::{ModuleCallErrorCode, ModuleEffectIntent, ModuleEmit, ModuleLimits, ModuleOutput};

    #[test]
    fn count_exceeds_limit_treats_usize_overflow_as_exceeded() {
        assert!(super::count_exceeds_limit(usize::MAX, 1));
        assert!(!super::count_exceeds_limit(1, 1));
        assert!(!super::count_exceeds_limit(u32::MAX as usize, u32::MAX));
        if let Some(too_many) = (u32::MAX as usize).checked_add(1) {
            assert!(super::count_exceeds_limit(too_many, u32::MAX));
        }
    }

    #[test]
    fn output_limit_kernel_preserves_first_error_and_inclusive_bounds() {
        let mut request = ModuleCallRequest {
            module_id: "module".into(),
            wasm_hash: "hash".into(),
            trace_id: "trace".into(),
            entrypoint: "reduce".into(),
            input: Vec::new(),
            limits: ModuleLimits {
                max_effects: 0,
                max_emits: 0,
                max_output_bytes: 0,
                ..ModuleLimits::unbounded()
            },
            wasm_bytes: Default::default(),
        };
        let mut output = ModuleOutput {
            new_state: None,
            effects: vec![ModuleEffectIntent {
                kind: "effect".into(),
                params: serde_json::Value::Null,
                cap_ref: "cap".into(),
                cap_slot: None,
            }],
            emits: vec![ModuleEmit {
                kind: "event".into(),
                payload: serde_json::Value::Null,
            }],
            tick_lifecycle: None,
            output_bytes: 1,
        };
        assert_eq!(
            validate_module_output_limits_v1(&request, &output),
            Err((ModuleCallErrorCode::EffectLimitExceeded, "effects exceeded"))
        );
        request.limits.max_effects = 1;
        assert_eq!(
            validate_module_output_limits_v1(&request, &output),
            Err((ModuleCallErrorCode::EmitLimitExceeded, "emits exceeded"))
        );
        request.limits.max_emits = 1;
        assert_eq!(
            validate_module_output_limits_v1(&request, &output),
            Err((ModuleCallErrorCode::OutputTooLarge, "output bytes exceeded"))
        );
        request.limits.max_output_bytes = 1;
        assert!(validate_module_output_limits_v1(&request, &output).is_ok());
        output.effects.clear();
        output.emits.clear();
        output.output_bytes = 0;
        request.limits.max_effects = 0;
        request.limits.max_emits = 0;
        request.limits.max_output_bytes = 0;
        assert!(validate_module_output_limits_v1(&request, &output).is_ok());
    }
}
