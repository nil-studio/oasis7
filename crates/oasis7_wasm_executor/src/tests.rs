use super::*;

#[test]
fn deterministic_replay_requires_finite_budgets_and_preserves_mode_on_clone() {
    let config = WasmExecutorConfig {
        max_fuel: 0,
        ..Default::default()
    };
    assert!(WasmExecutor::new_for_deterministic_replay(config).is_err());
    let executor =
        WasmExecutor::new_for_deterministic_replay(WasmExecutorConfig::default()).unwrap();
    assert!(!executor.wall_clock_timeout_enabled);
    assert!(!executor.clone().wall_clock_timeout_enabled);
    #[cfg(feature = "wasmtime")]
    {
        let legacy = WasmExecutor::new(WasmExecutorConfig::default()).unwrap();
        assert!(!executor.engine().get_memory_init_cow());
        assert!(legacy.engine().get_memory_init_cow());
        // A legacy compiled artifact cannot bypass the deterministic lowering.
        assert_ne!(
            compiled_engine_fingerprint(executor.engine(), executor.config()),
            compiled_engine_fingerprint(legacy.engine(), legacy.config())
        );
    }
    assert!(
        WasmExecutor::new(WasmExecutorConfig::default())
            .unwrap()
            .wall_clock_timeout_enabled
    );
}

#[test]
#[cfg(all(feature = "wasmtime", target_os = "linux"))]
fn deterministic_replay_keeps_derived_heap_out_of_native_backing_file_inventory() {
    let before = std::fs::read_to_string("/proc/self/maps").unwrap();
    let executor = WasmExecutor::new_for_deterministic_replay(Default::default()).unwrap();
    let module = wasmtime::Module::new(
        executor.engine(),
        wat::parse_str(
            r#"
        (module
            (memory (export "memory") 2)
            (data (i32.const 0) "original-state")
            (func (export "read") (result i32) i32.const 0 i32.load8_u))
    "#,
        )
        .unwrap(),
    )
    .unwrap();
    let mut store = wasmtime::Store::new(executor.engine(), ());
    store.set_fuel(1024).unwrap();
    store.set_epoch_deadline(u64::MAX);
    let instance = wasmtime::Instance::new(&mut store, &module, &[]).unwrap();
    assert_eq!(
        instance
            .get_typed_func::<(), i32>(&mut store, "read")
            .unwrap()
            .call(&mut store, ())
            .unwrap(),
        b'o' as i32
    );
    let after = std::fs::read_to_string("/proc/self/maps").unwrap();
    for mapping in after
        .lines()
        .filter(|line| !before.lines().any(|old| old == *line))
    {
        // A missing pathname or deleted file with a nonzero inode would make
        // the strict original-native-file capturer correctly refuse this run.
        let fields = mapping.split_whitespace().collect::<Vec<_>>();
        if fields[4] != "0" {
            assert!(fields.len() >= 6 && fields[5].starts_with('/'), "{mapping}");
            assert!(!mapping.ends_with(" (deleted)"), "{mapping}");
        }
    }
}

#[test]
#[cfg(feature = "wasmtime")]
fn deterministic_replay_canonicalizes_relaxed_simd_out_of_range_swizzle() {
    let executor = WasmExecutor::new_for_deterministic_replay(Default::default()).unwrap();
    let module = wasmtime::Module::new(
        executor.engine(),
        wat::parse_str(
            r#"
        (module (func (export "swizzle") (result i32)
            v128.const i8x16 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16
            v128.const i8x16 16 16 16 16 16 16 16 16 16 16 16 16 16 16 16 16
            i8x16.relaxed_swizzle
            i8x16.extract_lane_u 0))
    "#,
        )
        .unwrap(),
    )
    .unwrap();
    let mut store = wasmtime::Store::new(executor.engine(), ());
    store.set_fuel(1024).unwrap();
    store.set_epoch_deadline(u64::MAX);
    let instance = wasmtime::Instance::new(&mut store, &module, &[]).unwrap();
    let function = instance
        .get_typed_func::<(), i32>(&mut store, "swizzle")
        .unwrap();
    // The deterministic lowering follows strict swizzle: indices >= 16 yield 0.
    // Native relaxed implementations may instead wrap these indices.
    assert_eq!(function.call(&mut store, ()).unwrap(), 0);
}

#[test]
#[cfg(feature = "wasmtime")]
fn deterministic_replay_uses_fuel_without_committing_wall_clock_interruption() {
    let mut executor = WasmExecutor::new_for_deterministic_replay(WasmExecutorConfig {
        max_fuel: 1024,
        max_call_ms: 0,
        ..Default::default()
    })
    .unwrap();
    let wasm = call_ref_fuel_exhaustion_wasm();
    let request = ModuleCallRequest {
        module_id: "frozen-fuel".into(),
        wasm_hash: sha256_hex_for_test(&wasm),
        trace_id: "frozen-fuel".into(),
        entrypoint: "call".into(),
        input: vec![],
        limits: ModuleLimits {
            max_mem_bytes: 64 * 1024,
            max_gas: 1024,
            max_call_rate: 0,
            max_output_bytes: 1024,
            max_effects: 0,
            max_emits: 0,
        },
        wasm_bytes: Arc::<[u8]>::from(wasm),
    };
    for _ in 0..2 {
        let failure = executor.call(&request).unwrap_err();
        assert_eq!(failure.code, ModuleCallErrorCode::OutOfFuel);
    }
}
use oasis7_wasm_abi::ModuleLimits;
#[cfg(feature = "wasmtime")]
use std::fs;
#[cfg(feature = "wasmtime")]
use std::path::PathBuf;
use std::sync::Arc;
#[cfg(feature = "wasmtime")]
use std::time::{Instant, SystemTime, UNIX_EPOCH};

fn make_request(limits: ModuleLimits) -> ModuleCallRequest {
    ModuleCallRequest {
        module_id: "m.test".to_string(),
        wasm_hash: "hash".to_string(),
        trace_id: "trace-1".to_string(),
        entrypoint: "call".to_string(),
        input: vec![],
        limits,
        wasm_bytes: Arc::<[u8]>::from([]),
    }
}

fn test_executor(config: WasmExecutorConfig) -> WasmExecutor {
    WasmExecutor::new(config).expect("initialize wasm executor")
}

#[cfg(feature = "wasmtime")]
fn test_executor_with_metrics(
    config: WasmExecutorConfig,
    metrics: SharedWasmExecutorMetrics,
) -> WasmExecutor {
    WasmExecutor::new_with_metrics(config, metrics).expect("initialize wasm executor")
}

#[cfg(feature = "wasmtime")]
fn trivial_output_bytes() -> Vec<u8> {
    serde_cbor::to_vec(&ModuleOutput {
        new_state: None,
        effects: Vec::new(),
        emits: Vec::new(),
        tick_lifecycle: None,
        output_bytes: 0,
    })
    .expect("encode trivial module output")
}

#[cfg(feature = "wasmtime")]
fn trivial_success_wasm() -> Vec<u8> {
    let data = trivial_output_bytes();
    let escaped = data
        .iter()
        .map(|byte| format!("\\{:02x}", byte))
        .collect::<String>();
    let wat = format!(
        r#"(module
             (memory (export "memory") 1)
             (data (i32.const 16) "{escaped}")
             (func (export "alloc") (param i32) (result i32)
               i32.const 1024)
             (func (export "call") (param i32 i32) (result i32 i32)
               i32.const 16
               i32.const {len}))"#,
        len = data.len(),
    );
    wat::parse_str(wat).expect("compile trivial success wat")
}

#[cfg(feature = "wasmtime")]
fn invalid_cbor_output_wasm() -> Vec<u8> {
    let wat = r#"(module
         (memory (export "memory") 1)
         (data (i32.const 16) "\ff")
         (func (export "alloc") (param i32) (result i32)
           i32.const 1024)
         (func (export "call") (param i32 i32) (result i32 i32)
           i32.const 16
           i32.const 1))"#;
    wat::parse_str(wat).expect("compile invalid cbor wat")
}

#[cfg(feature = "wasmtime")]
fn call_ref_fuel_exhaustion_wasm() -> Vec<u8> {
    let data = trivial_output_bytes();
    let escaped = data
        .iter()
        .map(|byte| format!("\\{:02x}", byte))
        .collect::<String>();
    let wat = format!(
        r#"(module
             (type $burn_type (func (param i32) (result i32)))
             (memory (export "memory") 1)
             (data (i32.const 16) "{escaped}")
             (elem declare func $burn)
             (func $burn (type $burn_type) (param $iterations i32) (result i32)
               (local $i i32)
               (block $break
                 (loop $loop
                   local.get $i
                   local.get $iterations
                   i32.ge_u
                   br_if $break
                   local.get $i
                   i32.const 1
                   i32.add
                   local.set $i
                   br $loop))
               local.get $i)
             (func (export "alloc") (param i32) (result i32)
               i32.const 1024)
             (func (export "call") (param i32 i32) (result i32 i32)
               i32.const 10000
               ref.func $burn
               call_ref $burn_type
               drop
               i32.const 16
               i32.const {len}))"#,
        len = data.len(),
    );
    wat::parse_str(wat).expect("compile call_ref fuel exhaustion wat")
}

#[cfg(feature = "wasmtime")]
fn fuel_limited_start_wasm() -> Vec<u8> {
    wat::parse_str(
        r#"(module
             (memory (export "memory") 1)
             (func $start
               (loop $forever
                 br $forever))
             (start $start)
             (func (export "alloc") (param i32) (result i32)
               i32.const 0)
             (func (export "call") (param i32 i32) (result i32 i32)
               i32.const 0
               i32.const 0))"#,
    )
    .expect("compile fuel-limited start wat")
}

#[cfg(feature = "wasmtime")]
fn fuel_limited_bulk_fill_wasm() -> Vec<u8> {
    wat::parse_str(
        r#"(module
             (memory (export "memory") 64)
             (func (export "alloc") (param i32) (result i32)
               i32.const 0)
             (func (export "call") (param i32 i32) (result i32 i32)
               i32.const 0
               i32.const 0
               i32.const 262144
               memory.fill
               i32.const 0
               i32.const 0))"#,
    )
    .expect("compile fuel-limited bulk fill wat")
}

#[cfg(feature = "wasmtime")]
fn sha256_hex_for_test(bytes: &[u8]) -> String {
    let digest = <sha2::Sha256 as sha2::Digest>::digest(bytes);
    let mut out = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(&mut out, "{byte:02x}");
    }
    out
}

#[test]
fn fixed_sandbox_succeed_returns_cloned_output() {
    let output = ModuleOutput {
        new_state: Some(vec![1, 2, 3]),
        effects: Vec::new(),
        emits: Vec::new(),
        tick_lifecycle: None,
        output_bytes: 3,
    };
    let mut sandbox = FixedSandbox::succeed(output.clone());
    let request = make_request(ModuleLimits::default());

    let first = sandbox.call(&request).unwrap();
    assert_eq!(first, output);

    let second = sandbox.call(&request).unwrap();
    assert_eq!(second, output);
}

#[test]
fn fixed_sandbox_fail_returns_cloned_failure() {
    let failure = ModuleCallFailure {
        module_id: "m.test".to_string(),
        trace_id: "trace-err".to_string(),
        code: ModuleCallErrorCode::Trap,
        detail: "boom".to_string(),
    };
    let mut sandbox = FixedSandbox::fail(failure.clone());
    let request = make_request(ModuleLimits::default());

    let first = sandbox.call(&request).unwrap_err();
    assert_eq!(first, failure);

    let second = sandbox.call(&request).unwrap_err();
    assert_eq!(second, failure);
}

#[cfg(feature = "wasmtime")]
#[test]
fn wasm_executor_returns_disk_cache_init_error() {
    let cache_root = std::env::temp_dir().join("oasis7-wasm-init-error-file");
    fs::write(&cache_root, b"not-a-directory").expect("create temp cache file");

    let err = WasmExecutor::new(WasmExecutorConfig {
        compiled_cache_dir: Some(cache_root.clone()),
        ..WasmExecutorConfig::default()
    })
    .expect_err("file path should fail cache dir initialization");

    assert!(matches!(err, WasmExecutorInitError::DiskCacheInit(_)));

    let _ = fs::remove_file(cache_root);
}

#[test]
fn wasm_executor_rejects_output_limit_overflow() {
    let executor = test_executor(WasmExecutorConfig::default());
    let request = make_request(ModuleLimits {
        max_mem_bytes: executor.config().max_mem_bytes,
        max_gas: executor.config().max_fuel,
        max_call_rate: 0,
        max_output_bytes: 4,
        max_effects: 0,
        max_emits: 0,
    });
    let output = ModuleOutput {
        new_state: None,
        effects: Vec::new(),
        emits: Vec::new(),
        tick_lifecycle: None,
        output_bytes: 8,
    };

    let err = executor
        .validate_output_limits(&request, &output)
        .unwrap_err();
    assert_eq!(err.code, ModuleCallErrorCode::OutputTooLarge);
}

#[test]
fn count_exceeds_limit_treats_usize_overflow_as_exceeded() {
    assert!(count_exceeds_limit(usize::MAX, 1));
    assert!(!count_exceeds_limit(1, 1));
}

#[test]
fn wasm_executor_rejects_fuel_limit_as_timeout() {
    let executor = test_executor(WasmExecutorConfig {
        max_fuel: 10,
        ..WasmExecutorConfig::default()
    });
    let request = make_request(ModuleLimits {
        max_mem_bytes: executor.config().max_mem_bytes,
        max_gas: 11,
        max_call_rate: 0,
        max_output_bytes: executor.config().max_output_bytes,
        max_effects: 0,
        max_emits: 0,
    });

    let err = executor.validate_request_limits(&request).unwrap_err();
    assert_eq!(err.code, ModuleCallErrorCode::Timeout);
}

#[test]
fn wasm_executor_uses_executor_max_fuel_when_request_limit_is_zero() {
    let executor = test_executor(WasmExecutorConfig {
        max_fuel: 123,
        ..WasmExecutorConfig::default()
    });
    let request = make_request(ModuleLimits {
        max_mem_bytes: executor.config().max_mem_bytes,
        max_gas: 0,
        max_call_rate: 0,
        max_output_bytes: executor.config().max_output_bytes,
        max_effects: 0,
        max_emits: 0,
    });

    assert_eq!(executor.requested_fuel(&request), 123);
}

#[test]
fn wasm_executor_rejects_memory_limit_overflow_as_trap() {
    let executor = test_executor(WasmExecutorConfig {
        max_mem_bytes: 64,
        ..WasmExecutorConfig::default()
    });
    let request = make_request(ModuleLimits {
        max_mem_bytes: 65,
        max_gas: executor.config().max_fuel,
        max_call_rate: 0,
        max_output_bytes: executor.config().max_output_bytes,
        max_effects: 0,
        max_emits: 0,
    });

    let err = executor.validate_request_limits(&request).unwrap_err();
    assert_eq!(err.code, ModuleCallErrorCode::Trap);
}

#[test]
fn wasm_executor_rejects_requested_output_limit_over_executor_max() {
    let executor = test_executor(WasmExecutorConfig {
        max_output_bytes: 16,
        ..WasmExecutorConfig::default()
    });
    let request = make_request(ModuleLimits {
        max_mem_bytes: executor.config().max_mem_bytes,
        max_gas: executor.config().max_fuel,
        max_call_rate: 0,
        max_output_bytes: 17,
        max_effects: 0,
        max_emits: 0,
    });

    let err = executor.validate_request_limits(&request).unwrap_err();
    assert_eq!(err.code, ModuleCallErrorCode::OutputTooLarge);
}

#[cfg(feature = "wasmtime")]
#[test]
fn wasm_executor_maps_interrupt_trap_to_interrupted() {
    let executor = test_executor(WasmExecutorConfig::default());
    let request = make_request(ModuleLimits {
        max_mem_bytes: executor.config().max_mem_bytes,
        max_gas: executor.config().max_fuel,
        max_call_rate: 0,
        max_output_bytes: executor.config().max_output_bytes,
        max_effects: 0,
        max_emits: 0,
    });

    let err = executor.map_wasmtime_error(&request, wasmtime::Trap::Interrupt.into());
    assert_eq!(err.code, ModuleCallErrorCode::Interrupted);
}

#[cfg(feature = "wasmtime")]
#[test]
fn wasm_executor_maps_out_of_fuel_trap_to_out_of_fuel() {
    let executor = test_executor(WasmExecutorConfig::default());
    let request = make_request(ModuleLimits {
        max_mem_bytes: executor.config().max_mem_bytes,
        max_gas: executor.config().max_fuel,
        max_call_rate: 0,
        max_output_bytes: executor.config().max_output_bytes,
        max_effects: 0,
        max_emits: 0,
    });

    let err = executor.map_wasmtime_error(&request, wasmtime::Trap::OutOfFuel.into());
    assert_eq!(err.code, ModuleCallErrorCode::OutOfFuel);
}

#[cfg(feature = "wasmtime")]
#[test]
fn wasm_executor_charges_fuel_for_call_ref_callee() {
    let mut executor = test_executor(WasmExecutorConfig {
        max_fuel: 1_024,
        max_call_ms: 2_000,
        ..WasmExecutorConfig::default()
    });
    let wasm = call_ref_fuel_exhaustion_wasm();
    let wasm_hash = sha256_hex_for_test(&wasm);
    let request = ModuleCallRequest {
        module_id: "m.call-ref-fuel".to_string(),
        wasm_hash,
        trace_id: "trace-call-ref-fuel".to_string(),
        entrypoint: "call".to_string(),
        input: Vec::new(),
        limits: ModuleLimits {
            max_mem_bytes: executor.config().max_mem_bytes,
            max_gas: executor.config().max_fuel,
            max_call_rate: 0,
            max_output_bytes: executor.config().max_output_bytes,
            max_effects: 0,
            max_emits: 0,
        },
        wasm_bytes: Arc::<[u8]>::from(wasm),
    };

    let err = executor
        .call(&request)
        .expect_err("call_ref callee work must consume the guest fuel budget");
    assert_eq!(err.code, ModuleCallErrorCode::OutOfFuel, "{err:?}");
}

#[cfg(feature = "wasmtime")]
#[test]
fn wasm_executor_charges_fuel_for_start_function() {
    let mut executor = test_executor(WasmExecutorConfig {
        max_fuel: 100,
        ..WasmExecutorConfig::default()
    });
    let wasm = fuel_limited_start_wasm();
    let request = ModuleCallRequest {
        module_id: "m.start-fuel".to_string(),
        wasm_hash: sha256_hex_for_test(&wasm),
        trace_id: "trace-start-fuel".to_string(),
        entrypoint: "call".to_string(),
        input: Vec::new(),
        limits: ModuleLimits {
            max_mem_bytes: executor.config().max_mem_bytes,
            max_gas: executor.config().max_fuel,
            max_call_rate: 0,
            max_output_bytes: executor.config().max_output_bytes,
            max_effects: 0,
            max_emits: 0,
        },
        wasm_bytes: Arc::<[u8]>::from(wasm),
    };

    let err = executor
        .call(&request)
        .expect_err("start function work must consume the guest fuel budget");
    assert_eq!(err.code, ModuleCallErrorCode::OutOfFuel, "{err:?}");
}

#[cfg(feature = "wasmtime")]
#[test]
fn wasm_executor_charges_fuel_for_bulk_memory_fill() {
    let mut executor = test_executor(WasmExecutorConfig {
        max_fuel: 100,
        max_mem_bytes: 4 * 1024 * 1024,
        ..WasmExecutorConfig::default()
    });
    let wasm = fuel_limited_bulk_fill_wasm();
    let request = ModuleCallRequest {
        module_id: "m.bulk-fill-fuel".to_string(),
        wasm_hash: sha256_hex_for_test(&wasm),
        trace_id: "trace-bulk-fill-fuel".to_string(),
        entrypoint: "call".to_string(),
        input: Vec::new(),
        limits: ModuleLimits {
            max_mem_bytes: executor.config().max_mem_bytes,
            max_gas: executor.config().max_fuel,
            max_call_rate: 0,
            max_output_bytes: executor.config().max_output_bytes,
            max_effects: 0,
            max_emits: 0,
        },
        wasm_bytes: Arc::<[u8]>::from(wasm),
    };

    let err = executor
        .call(&request)
        .expect_err("bulk memory work must consume the guest fuel budget");
    assert_eq!(err.code, ModuleCallErrorCode::OutOfFuel, "{err:?}");
}

#[cfg(feature = "wasmtime")]
#[test]
fn wasm_executor_only_charges_guest_const_ops_not_module_initializers() {
    let executor = test_executor(WasmExecutorConfig::default());
    let wasm = wat::parse_str(
        r#"(module
             (global $value i32 (i32.const 7))
             (func (export "read") (result i32)
               i32.const 123))"#,
    )
    .expect("compile constant-expression fuel wat");
    let module = wasmtime::Module::new(executor.engine(), &wasm)
        .expect("compile constant-expression fuel module");
    let mut store = wasmtime::Store::new(executor.engine(), ());
    store.set_epoch_deadline(u64::MAX);
    let initial_fuel = 100;
    store
        .set_fuel(initial_fuel)
        .expect("set constant-expression fuel");
    let instance = wasmtime::Linker::new(executor.engine())
        .instantiate(&mut store, &module)
        .expect("instantiate constant-expression fuel module");
    assert_eq!(
        store.get_fuel().expect("read remaining fuel"),
        initial_fuel,
        "module constant initializers are not guest execution"
    );
    let read = instance
        .get_typed_func::<(), i32>(&mut store, "read")
        .expect("read export");

    assert_eq!(read.call(&mut store, ()).expect("read constant"), 123);
    assert!(
        store.get_fuel().expect("read remaining fuel") < initial_fuel,
        "guest const instructions must consume fuel"
    );
}

#[cfg(feature = "wasmtime")]
#[test]
fn wasm_executor_disables_wide_arithmetic_proposal() {
    let executor = test_executor(WasmExecutorConfig::default());
    let wasm = wat::parse_str(
        r#"(module
             (func (result i64 i64)
               i64.const 1
               i64.const 0
               i64.const 2
               i64.const 0
               i64.add128))"#,
    )
    .expect("parse wide arithmetic proposal wat");

    assert!(
        wasmtime::Module::new(executor.engine(), &wasm).is_err(),
        "wide arithmetic must remain disabled for module validation"
    );
}

#[cfg(feature = "wasmtime")]
#[test]
fn wasm_executor_store_limits_enforce_requested_memory_cap() {
    let executor = test_executor(WasmExecutorConfig::default());
    let request = make_request(ModuleLimits {
        max_mem_bytes: 64,
        max_gas: executor.config().max_fuel,
        max_call_rate: 0,
        max_output_bytes: executor.config().max_output_bytes,
        max_effects: 0,
        max_emits: 0,
    });
    let mut limits = executor.build_store_limits(&request);

    let allow = <wasmtime::StoreLimits as wasmtime::ResourceLimiter>::memory_growing(
        &mut limits,
        32,
        64,
        Some(128),
    )
    .expect("memory growth decision");
    assert!(allow);

    let deny = <wasmtime::StoreLimits as wasmtime::ResourceLimiter>::memory_growing(
        &mut limits,
        64,
        65,
        Some(128),
    );
    assert!(deny.is_err());
}

#[cfg(feature = "wasmtime")]
#[test]
fn wasm_executor_epoch_watchdog_preempts_infinite_loop() {
    let mut executor = test_executor(WasmExecutorConfig {
        max_call_ms: 20,
        max_fuel: u64::MAX,
        ..WasmExecutorConfig::default()
    });
    let wasm = wat::parse_str(
        r#"(module
             (memory (export "memory") 1)
             (func (export "alloc") (param i32) (result i32)
               i32.const 0)
             (func (export "call") (param i32 i32) (result i64)
               (loop $l
                 br $l)
               i64.const 0))"#,
    )
    .expect("compile test wat");
    let wasm_hash = sha256_hex_for_test(&wasm);
    let request = ModuleCallRequest {
        module_id: "m.loop".to_string(),
        wasm_hash,
        trace_id: "trace-loop".to_string(),
        entrypoint: "call".to_string(),
        input: Vec::new(),
        limits: ModuleLimits {
            max_mem_bytes: 64 * 1024,
            max_gas: 0,
            max_call_rate: 0,
            max_output_bytes: 1024,
            max_effects: 0,
            max_emits: 0,
        },
        wasm_bytes: Arc::<[u8]>::from(wasm),
    };

    let started = std::time::Instant::now();
    let err = executor
        .call(&request)
        .expect_err("infinite loop should be interrupted by watchdog");
    assert_eq!(err.code, ModuleCallErrorCode::Interrupted);
    assert!(
        started.elapsed().as_millis() < 3_000,
        "watchdog timeout should preempt quickly"
    );
}

#[cfg(feature = "wasmtime")]
#[test]
fn wasm_executor_metrics_track_compile_call_and_failure_paths() {
    let metrics = init_shared_wasm_executor_metrics();
    let mut executor = test_executor_with_metrics(WasmExecutorConfig::default(), metrics.clone());
    let wasm = trivial_success_wasm();
    let wasm_hash = sha256_hex_for_test(&wasm);
    let request = ModuleCallRequest {
        module_id: "m.metrics".to_string(),
        wasm_hash,
        trace_id: "trace-metrics".to_string(),
        entrypoint: "call".to_string(),
        input: Vec::new(),
        limits: ModuleLimits {
            max_mem_bytes: 64 * 1024,
            max_gas: 10_000_000,
            max_call_rate: 0,
            max_output_bytes: 1024,
            max_effects: 0,
            max_emits: 0,
        },
        wasm_bytes: Arc::<[u8]>::from(wasm),
    };

    executor.call(&request).expect("first call should succeed");
    executor
        .call(&request)
        .expect("second call should hit memory cache");
    let missing_wasm = ModuleCallRequest {
        trace_id: "trace-missing".to_string(),
        wasm_hash: "hash-missing".to_string(),
        wasm_bytes: Arc::<[u8]>::from([]),
        ..request.clone()
    };
    let failure = executor
        .call(&missing_wasm)
        .expect_err("missing wasm bytes should fail");
    assert_eq!(failure.code, ModuleCallErrorCode::Trap);

    let snapshot = snapshot_wasm_executor_metrics(&metrics);
    assert_eq!(snapshot.calls_total, 3);
    assert_eq!(snapshot.compile_misses, 1);
    assert_eq!(snapshot.memory_cache_hits, 1);
    assert_eq!(
        snapshot.failure_by_code.get("trap").copied().unwrap_or(0),
        1
    );
    assert_eq!(
        snapshot.calls_total - snapshot.failure_by_code.get("trap").copied().unwrap_or(0),
        2
    );
    assert_eq!(
        snapshot.call_wall_ms_buckets.values().copied().sum::<u64>(),
        snapshot.calls_total
    );
    assert_eq!(snapshot.module_hotspots.len(), 1);
    assert_eq!(snapshot.module_hotspots[0].module_id, "m.metrics");
    assert_eq!(snapshot.module_hotspots[0].calls_total, 3);
    assert_eq!(snapshot.module_hotspots[0].failure_count, 1);
    assert!(snapshot.module_hotspots[0].wall_ms_total > 0);
}

#[test]
fn wasm_executor_module_hotspot_labels_are_byte_budgeted() {
    let metrics = init_shared_wasm_executor_metrics();
    let long_module_id = format!("m.{}", "模块".repeat(100));

    super::metrics::observe_wasm_executor_call_result(&metrics, &long_module_id, 42, None);

    let snapshot = snapshot_wasm_executor_metrics(&metrics);
    assert_eq!(snapshot.module_hotspots.len(), 1);
    assert!(snapshot.module_hotspots[0].module_id.ends_with("..."));
    assert!(snapshot.module_hotspots[0].module_id.len() <= 120);
    assert!(
        snapshot.module_hotspots[0]
            .module_id
            .is_char_boundary(snapshot.module_hotspots[0].module_id.len() - "...".len())
    );
}

#[test]
fn wasm_executor_module_hotspots_are_top_n_with_overflow_bucket() {
    let metrics = init_shared_wasm_executor_metrics();

    for index in 0..270 {
        let module_id = format!("m.hotspot.{index:03}");
        super::metrics::observe_wasm_executor_call_result(
            &metrics,
            &module_id,
            index as u64 + 1,
            None,
        );
    }

    let snapshot = snapshot_wasm_executor_metrics(&metrics);
    assert_eq!(snapshot.calls_total, 270);
    assert_eq!(snapshot.module_hotspots.len(), 10);
    assert!(
        snapshot
            .module_hotspots
            .iter()
            .any(|hotspot| hotspot.module_id == "__overflow__")
    );
    assert!(snapshot.module_hotspots.iter().all(|hotspot| {
        hotspot.module_id == "__overflow__" || hotspot.module_id.starts_with("m.hotspot.")
    }));
}

#[cfg(feature = "wasmtime")]
#[test]
fn wasm_executor_metrics_track_decode_timing_for_invalid_output() {
    let metrics = init_shared_wasm_executor_metrics();
    let mut executor = test_executor_with_metrics(WasmExecutorConfig::default(), metrics.clone());
    let wasm = invalid_cbor_output_wasm();
    let wasm_hash = sha256_hex_for_test(&wasm);
    let request = ModuleCallRequest {
        module_id: "m.invalid-output".to_string(),
        wasm_hash,
        trace_id: "trace-invalid-output".to_string(),
        entrypoint: "call".to_string(),
        input: Vec::new(),
        limits: ModuleLimits {
            max_mem_bytes: 64 * 1024,
            max_gas: 10_000_000,
            max_call_rate: 0,
            max_output_bytes: 1024,
            max_effects: 0,
            max_emits: 0,
        },
        wasm_bytes: Arc::<[u8]>::from(wasm),
    };

    let failure = executor
        .call(&request)
        .expect_err("invalid cbor output should fail");
    assert_eq!(failure.code, ModuleCallErrorCode::InvalidOutput);

    let snapshot = snapshot_wasm_executor_metrics(&metrics);
    assert_eq!(snapshot.calls_total, 1);
    assert_eq!(snapshot.compile_misses, 1);
    assert_eq!(
        snapshot
            .failure_by_code
            .get("invalid_output")
            .copied()
            .unwrap_or(0),
        1
    );
    assert_eq!(
        snapshot.call_wall_ms_buckets.values().copied().sum::<u64>(),
        snapshot.calls_total
    );
}

#[cfg(feature = "wasmtime")]
#[test]
fn wasm_executor_compiled_cache_evicts_old_entries() {
    let executor = test_executor(WasmExecutorConfig {
        max_cache_entries: 1,
        ..WasmExecutorConfig::default()
    });
    let wasm_a = [0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
    let wasm_b = trivial_success_wasm();
    let wasm_a_hash = sha256_hex_for_test(&wasm_a);
    let wasm_b_hash = sha256_hex_for_test(&wasm_b);

    executor
        .compile_module_cached(wasm_a_hash.as_str(), &wasm_a)
        .unwrap();
    assert_eq!(executor.compiled_cache_len(), 1);

    executor
        .compile_module_cached(wasm_b_hash.as_str(), &wasm_b)
        .unwrap();
    assert_eq!(executor.compiled_cache_len(), 1);
}

#[cfg(feature = "wasmtime")]
#[test]
fn wasm_executor_compiled_cache_zero_capacity_stays_empty() {
    let executor = test_executor(WasmExecutorConfig {
        max_cache_entries: 0,
        ..WasmExecutorConfig::default()
    });
    let wasm = [0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
    let wasm_hash = sha256_hex_for_test(&wasm);

    executor
        .compile_module_cached(wasm_hash.as_str(), &wasm)
        .unwrap();
    assert_eq!(executor.compiled_cache_len(), 0);

    executor
        .compile_module_cached(wasm_hash.as_str(), &wasm)
        .unwrap();
    assert_eq!(executor.compiled_cache_len(), 0);
}

#[cfg(feature = "wasmtime")]
fn unique_temp_cache_dir(suffix: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock drift")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("oasis7-wasm-cache-{suffix}-{nonce}"));
    fs::create_dir_all(&dir).expect("create temp cache dir");
    dir
}

#[cfg(feature = "wasmtime")]
#[test]
fn wasm_executor_disk_cache_hits_when_memory_cache_disabled() {
    let cache_dir = unique_temp_cache_dir("hit");
    let executor = test_executor(WasmExecutorConfig {
        max_cache_entries: 0,
        compiled_cache_dir: Some(cache_dir.clone()),
        ..WasmExecutorConfig::default()
    });
    let wasm = [0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
    let wasm_hash = sha256_hex_for_test(&wasm);

    executor
        .compile_module_cached(wasm_hash.as_str(), &wasm)
        .unwrap();
    executor
        .compile_module_cached(wasm_hash.as_str(), &wasm)
        .expect("load compiled module from disk cache");

    let _ = fs::remove_dir_all(cache_dir);
}

#[cfg(feature = "wasmtime")]
#[test]
fn wasm_executor_rejects_wasm_hash_bytes_mismatch_before_cache_hit() {
    let cache_dir = unique_temp_cache_dir("hash-mismatch");
    let executor = test_executor(WasmExecutorConfig {
        compiled_cache_dir: Some(cache_dir.clone()),
        ..WasmExecutorConfig::default()
    });
    let wasm = trivial_success_wasm();
    let wasm_hash = sha256_hex_for_test(&wasm);
    executor
        .compile_module_cached(wasm_hash.as_str(), &wasm)
        .expect("populate compiled cache");

    let err = executor
        .compile_module_cached(wasm_hash.as_str(), &[0x01, 0x02, 0x03])
        .expect_err("mismatched bytes must not hit compiled cache");
    assert_eq!(err.code, ModuleCallErrorCode::Trap);
    assert!(
        err.detail.contains("wasm hash mismatch"),
        "unexpected error detail: {}",
        err.detail
    );

    let _ = fs::remove_dir_all(cache_dir);
}

#[cfg(feature = "wasmtime")]
#[test]
fn wasm_executor_disk_cache_persists_serialized_compiled_artifact() {
    let cache_dir = unique_temp_cache_dir("serialized");
    let executor = test_executor(WasmExecutorConfig {
        max_cache_entries: 0,
        compiled_cache_dir: Some(cache_dir.clone()),
        ..WasmExecutorConfig::default()
    });
    let wasm = trivial_success_wasm();
    let wasm_hash = sha256_hex_for_test(&wasm);

    executor
        .compile_module_cached(wasm_hash.as_str(), &wasm)
        .unwrap();

    let cache_file = executor
        .compiled_disk_cache_path_for_test(wasm_hash.as_str())
        .expect("cache path");
    let cache_namespace = cache_file
        .parent()
        .and_then(|path| path.file_name())
        .and_then(|name| name.to_str())
        .expect("cache namespace");
    assert!(
        cache_namespace.starts_with("wasmtime-cf-v3-key"),
        "compiled cache namespace must include the engine compatibility key: {cache_namespace}"
    );
    let cached_bytes = fs::read(&cache_file).expect("read serialized cache");
    assert_ne!(cached_bytes, wasm);
    assert!(
        !cached_bytes.starts_with(&[0x00, 0x61, 0x73, 0x6d]),
        "serialized compiled artifact should not start with the raw wasm magic header"
    );

    let _ = fs::remove_dir_all(cache_dir);
}

#[cfg(feature = "wasmtime")]
#[test]
fn wasm_executor_disk_cache_recovers_from_corruption() {
    let cache_dir = unique_temp_cache_dir("corrupt");
    let executor = test_executor(WasmExecutorConfig {
        max_cache_entries: 0,
        compiled_cache_dir: Some(cache_dir.clone()),
        ..WasmExecutorConfig::default()
    });
    let wasm = [0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
    let wasm_hash = sha256_hex_for_test(&wasm);

    executor
        .compile_module_cached(wasm_hash.as_str(), &wasm)
        .unwrap();
    let cache_file = executor
        .compiled_disk_cache_path_for_test(wasm_hash.as_str())
        .expect("cache path");
    fs::write(&cache_file, b"corrupt-bytes").expect("write corrupt cache");

    executor
        .compile_module_cached(wasm_hash.as_str(), &wasm)
        .expect("recompile after corrupt cache");

    let repaired = fs::read(&cache_file).expect("read repaired cache");
    assert_ne!(repaired, b"corrupt-bytes");

    let _ = fs::remove_dir_all(cache_dir);
}

#[cfg(feature = "wasmtime")]
#[test]
#[ignore = "local perf probe"]
fn perf_probe_executor_call_and_watchdog_overhead() {
    let mut executor = test_executor(WasmExecutorConfig {
        max_call_ms: 2_000,
        ..WasmExecutorConfig::default()
    });
    let wasm = trivial_success_wasm();
    let wasm_hash = sha256_hex_for_test(&wasm);
    let expected_output_bytes = trivial_output_bytes().len() as u64;
    let request = ModuleCallRequest {
        module_id: "m.perf".to_string(),
        wasm_hash,
        trace_id: "trace-perf".to_string(),
        entrypoint: "call".to_string(),
        input: Vec::new(),
        limits: ModuleLimits {
            max_mem_bytes: 64 * 1024,
            max_gas: 100_000,
            max_call_rate: 0,
            max_output_bytes: 4 * 1024,
            max_effects: 4,
            max_emits: 4,
        },
        wasm_bytes: Arc::<[u8]>::from(wasm),
    };

    executor.call(&request).expect("warm trivial wasm call");

    let iterations = 2_000u32;
    let started = Instant::now();
    for _ in 0..iterations {
        let output = executor.call(&request).expect("perf trivial wasm call");
        assert_eq!(output.output_bytes, expected_output_bytes);
    }
    let call_elapsed = started.elapsed();

    let started = Instant::now();
    for _ in 0..iterations {
        let _watchdog = executor.watchdog.arm(executor.config().max_call_ms);
    }
    let watchdog_elapsed = started.elapsed();

    let call_per_op_us = call_elapsed.as_secs_f64() * 1_000_000.0 / f64::from(iterations);
    let watchdog_per_op_us = watchdog_elapsed.as_secs_f64() * 1_000_000.0 / f64::from(iterations);
    eprintln!(
        "perf_probe_executor_call_and_watchdog_overhead: iterations={iterations} warm_call_total_ms={:.3} warm_call_us_per_op={:.3} watchdog_total_ms={:.3} watchdog_us_per_op={:.3} watchdog_share_of_call={:.2}%",
        call_elapsed.as_secs_f64() * 1_000.0,
        call_per_op_us,
        watchdog_elapsed.as_secs_f64() * 1_000.0,
        watchdog_per_op_us,
        if call_per_op_us == 0.0 {
            0.0
        } else {
            (watchdog_per_op_us / call_per_op_us) * 100.0
        }
    );
}
