use super::*;

#[test]
fn wasm_executor_preserves_nan_bits_and_reference_calls() {
    for executor in [
        WasmExecutor::new(Default::default()).unwrap(),
        WasmExecutor::new_for_deterministic_replay(Default::default()).unwrap(),
    ] {
        // Pass WAT directly to Module::new to exercise its retained text support.
        // Noncanonical NaNs arrive as parameters so constant folding cannot hide
        // a change to the guest's floating-point lowering.
        let module = wasmtime::Module::new(
            executor.engine(),
            r#"(module
                (type $f32_bits (func (param f32) (result i32)))
                (table 1 funcref)
                (elem (i32.const 0) func $canonical_f32)
                (func $canonical_f32 (type $f32_bits) (param f32) (result i32)
                    local.get 0
                    f32.const 1
                    f32.add
                    i32.reinterpret_f32)
                (func (export "f32_bits") (param f32) (result i32)
                    local.get 0
                    ref.func $canonical_f32
                    call_ref $f32_bits)
                (func (export "f64_bits") (param f64) (result i64)
                    local.get 0
                    f64.const 1
                    f64.add
                    i64.reinterpret_f64)
                (func (export "has_reference") (result i32)
                    i32.const 0
                    table.get 0
                    ref.is_null
                    i32.eqz))"#,
        )
        .unwrap();
        let mut store = wasmtime::Store::new(executor.engine(), ());
        store.set_fuel(1024).unwrap();
        store.set_epoch_deadline(u64::MAX);
        let instance = wasmtime::Instance::new(&mut store, &module, &[]).unwrap();
        let f32_bits = instance
            .get_typed_func::<f32, i32>(&mut store, "f32_bits")
            .unwrap();
        let f64_bits = instance
            .get_typed_func::<f64, i64>(&mut store, "f64_bits")
            .unwrap();
        let has_reference = instance
            .get_typed_func::<(), i32>(&mut store, "has_reference")
            .unwrap();

        assert_eq!(has_reference.call(&mut store, ()).unwrap(), 1);
        for bits in [0x7fa1_2345, 0xffc1_2345] {
            assert_eq!(
                f32_bits.call(&mut store, f32::from_bits(bits)).unwrap() as u32,
                0x7fc0_0000
            );
        }
        for bits in [0x7ff0_1234_5678_9abc, 0xfff8_1234_5678_9abc] {
            assert_eq!(
                f64_bits.call(&mut store, f64::from_bits(bits)).unwrap() as u64,
                0x7ff8_0000_0000_0000
            );
        }
    }
}

#[test]
fn deterministic_replay_preserves_synchronous_abis_after_disk_cache_restart() {
    let cache_dir = unique_temp_cache_dir("sync-abi-restart");
    let config = WasmExecutorConfig {
        max_cache_entries: 0,
        compiled_cache_dir: Some(cache_dir.clone()),
        ..Default::default()
    };
    let mut expected = ModuleOutput {
        new_state: Some(vec![0, 1, 127, 128, 255]),
        effects: Vec::new(),
        emits: Vec::new(),
        tick_lifecycle: None,
        output_bytes: 0,
    };
    let output_bytes = serde_cbor::to_vec(&expected).unwrap();
    expected.output_bytes = output_bytes.len() as u64;
    let escaped = output_bytes
        .iter()
        .map(|byte| format!("\\{byte:02x}"))
        .collect::<String>();
    let wasm = wat::parse_str(format!(
        r#"(module
            (memory (export "memory") 1)
            (data (i32.const 16) "{escaped}")
            (global $next (mut i32) (i32.const 1024))
            (func (export "alloc") (param $len i32) (result i32)
                global.get $next
                global.get $next
                local.get $len
                i32.add
                global.set $next)
            (func (export "multi") (param i32 i32) (result i32 i32)
                i32.const 16
                i32.const {len})
            (func (export "packed") (param i32 i32) (result i64)
                i64.const {packed})
            (func (export "sret") (param $out i32) (param i32 i32)
                local.get $out
                i32.const 16
                i32.store
                local.get $out
                i32.const {len}
                i32.store offset=4))"#,
        len = output_bytes.len(),
        packed = ((output_bytes.len() as u64) << 32) | 16,
    ))
    .unwrap();
    let request = ModuleCallRequest {
        module_id: "synchronous-abi".into(),
        wasm_hash: sha256_hex_for_test(&wasm),
        trace_id: "synchronous-abi".into(),
        entrypoint: "multi".into(),
        input: vec![1, 2, 3],
        limits: ModuleLimits {
            max_mem_bytes: 64 * 1024,
            max_gas: 1024,
            max_call_rate: 0,
            max_output_bytes: 1024,
            max_effects: 0,
            max_emits: 0,
        },
        wasm_bytes: Arc::from(wasm),
    };

    // A fresh engine must restore the project's serialized artifact without
    // Wasmtime's optional built-in global cache. Memory caching stays disabled.
    for expected_cache_path in [
        CompileCachePathKind::CompileMiss,
        CompileCachePathKind::DiskHit,
    ] {
        let mut executor = WasmExecutor::new_for_deterministic_replay(config.clone()).unwrap();
        let outcome = executor
            .compile_module_cached_with_metrics(&request.wasm_hash, &request.wasm_bytes)
            .unwrap();
        assert_eq!(outcome.cache_path, expected_cache_path);
        for entrypoint in ["multi", "packed", "sret"] {
            let mut request = request.clone();
            request.entrypoint = entrypoint.into();
            let actual = executor.call(&request).unwrap();
            assert_eq!(actual, expected, "{expected_cache_path:?}: {entrypoint}");
        }
    }
    fs::remove_dir_all(cache_dir).unwrap();
}
