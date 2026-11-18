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
#[cfg(not(test))]
pub(crate) use oasis7_executable_image::executable_digest;

#[cfg(test)]
thread_local! {
    static FIXTURE_IMAGE_DIGEST: std::cell::RefCell<Option<String>> = const {
        std::cell::RefCell::new(None)
    };
}

#[cfg(test)]
pub(crate) fn executable_digest() -> Result<String, String> {
    match FIXTURE_IMAGE_DIGEST.with(|digest| digest.borrow().clone()) {
        Some(digest) => Ok(digest),
        None => oasis7_executable_image::executable_digest(),
    }
}

/// Build and inspect a synthetic fixture for a separate shipping executable.
/// The child independently verifies its own loaded image through the normal CLI.
#[cfg(test)]
pub(crate) fn with_fixture_image_digest<T>(digest: String, run: impl FnOnce() -> T) -> T {
    struct Restore(Option<String>);
    impl Drop for Restore {
        fn drop(&mut self) {
            FIXTURE_IMAGE_DIGEST.with(|digest| digest.replace(self.0.take()));
        }
    }

    let _restore = Restore(FIXTURE_IMAGE_DIGEST.with(|value| value.replace(Some(digest))));
    run()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fixture_image_digest_is_thread_local_and_restored() {
        let original = executable_digest();
        with_fixture_image_digest("fixture-image".into(), || {
            assert_eq!(executable_digest().as_deref(), Ok("fixture-image"));
            let original = original.clone();
            std::thread::spawn(move || assert_eq!(executable_digest(), original))
                .join()
                .unwrap();
        });
        assert_eq!(executable_digest(), original);
    }

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
