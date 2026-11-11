//! One execution policy drives both frozen binding and actual executor setup.
use oasis7_wasm_executor::{WasmEngineKind, WasmExecutor, WasmExecutorConfig};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub(crate) struct GuardedExecutionPolicy {
    pub profile_version: u32,
    pub engine: &'static str,
    pub max_fuel: u64,
    pub max_mem_bytes: u64,
    pub max_output_bytes: u64,
    pub wall_clock_results: bool,
    pub relaxed_simd_deterministic: bool,
    pub nan_canonicalization: bool,
    pub memory_init_cow: bool,
    pub hot_head_heights: u64,
    pub checkpoint_interval: u64,
    pub checkpoint_keep: usize,
}
impl GuardedExecutionPolicy {
    pub(crate) fn current() -> Self {
        Self {
            profile_version: 3,
            engine: "wasmtime",
            max_fuel: 10_000_000,
            max_mem_bytes: 64 * 1024 * 1024,
            max_output_bytes: 4 * 1024 * 1024,
            wall_clock_results: false,
            relaxed_simd_deterministic: true,
            nan_canonicalization: true,
            memory_init_cow: false,
            hot_head_heights: 32,
            checkpoint_interval: 32,
            checkpoint_keep: 4,
        }
    }
    pub(crate) fn executor(&self) -> Result<WasmExecutor, String> {
        if self.engine != "wasmtime"
            || self.wall_clock_results
            || !self.relaxed_simd_deterministic
            || !self.nan_canonicalization
            || self.memory_init_cow
            || self.profile_version != 3
        {
            return Err("unsupported frozen execution policy".into());
        }
        let config = WasmExecutorConfig {
            engine: WasmEngineKind::Wasmtime,
            max_fuel: self.max_fuel,
            max_mem_bytes: self.max_mem_bytes,
            max_output_bytes: self.max_output_bytes,
            ..WasmExecutorConfig::default()
        };
        WasmExecutor::new_for_deterministic_replay(config).map_err(|e| e.to_string())
    }
}
pub(crate) use oasis7_executable_image::executable_digest;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_native_policy_binds_anonymous_heap_initialization() {
        let policy = GuardedExecutionPolicy::current();
        assert_eq!(policy.profile_version, 3);
        let original = serde_cbor::to_vec(&policy).unwrap();
        let mut unsupported = policy.clone();
        unsupported.memory_init_cow = true;
        assert_ne!(original, serde_cbor::to_vec(&unsupported).unwrap());
        assert!(unsupported.executor().is_err());
        unsupported = policy;
        unsupported.profile_version = 2;
        assert!(unsupported.executor().is_err());
    }
}
