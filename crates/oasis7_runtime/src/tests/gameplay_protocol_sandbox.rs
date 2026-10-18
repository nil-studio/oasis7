use super::M5_GAMEPLAY_GOVERNANCE_MODULE_ID;
use oasis7_wasm_abi::{
    ModuleCallFailure, ModuleCallRequest, ModuleEmit, ModuleOutput, ModuleSandbox,
    ModuleTickLifecycleDirective,
};

#[cfg(all(feature = "wasmtime", feature = "test_tier_full"))]
pub(super) struct GameplayDirectiveSandbox {
    governance_directives: Vec<serde_json::Value>,
}

#[cfg(all(feature = "wasmtime", feature = "test_tier_full"))]
impl GameplayDirectiveSandbox {
    pub(super) fn empty() -> Self {
        Self {
            governance_directives: Vec::new(),
        }
    }

    pub(super) fn with_governance_directive(payload: serde_json::Value) -> Self {
        Self {
            governance_directives: vec![payload],
        }
    }
}

#[cfg(all(feature = "wasmtime", feature = "test_tier_full"))]
impl ModuleSandbox for GameplayDirectiveSandbox {
    fn call(&mut self, request: &ModuleCallRequest) -> Result<ModuleOutput, ModuleCallFailure> {
        let mut emits = Vec::new();
        if request.module_id == M5_GAMEPLAY_GOVERNANCE_MODULE_ID
            && (request.trace_id.starts_with("tick-")
                || request.trace_id.starts_with("infra-tick-"))
            && let Some(payload) = self.governance_directives.pop()
        {
            emits.push(ModuleEmit {
                kind: "gameplay.lifecycle.directives".to_string(),
                payload,
            });
        }
        Ok(ModuleOutput {
            new_state: None,
            effects: Vec::new(),
            emits,
            tick_lifecycle: Some(ModuleTickLifecycleDirective::WakeAfterTicks { ticks: 1 }),
            output_bytes: 512,
        })
    }
}
