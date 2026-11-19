use super::*;
use oasis7_wasm_abi::ModuleLimits;
#[cfg(feature = "wasmtime")]
use oasis7_wasm_abi::ModuleSandbox;
#[cfg(feature = "wasmtime")]
use oasis7_wasm_executor::{WasmExecutor, WasmExecutorConfig};
use sha2::{Digest, Sha256};
use std::sync::Arc;

fn leb(mut n: usize) -> Vec<u8> {
    let mut out = Vec::new();
    loop {
        let b = (n & 127) as u8;
        n >>= 7;
        out.push(b | if n != 0 { 128 } else { 0 });
        if n == 0 {
            break;
        }
    }
    out
}
fn section(wasm: &mut Vec<u8>, id: u8, payload: &[u8]) {
    wasm.push(id);
    wasm.extend(leb(payload.len()));
    wasm.extend(payload);
}
// Actual binary WebAssembly: alloc returns 4096, call returns (0, actual length).
// The VM's active data segment holds the untrusted raw CBOR output.
fn request(raw: &[u8]) -> ModuleCallRequest {
    let mut wasm = b"\0asm\x01\0\0\0".to_vec();
    section(
        &mut wasm,
        1,
        &[
            2, 0x60, 1, 0x7f, 1, 0x7f, 0x60, 2, 0x7f, 0x7f, 2, 0x7f, 0x7f,
        ],
    );
    section(&mut wasm, 3, &[2, 0, 1]);
    section(&mut wasm, 5, &[1, 0, 1]);
    section(
        &mut wasm,
        7,
        &[
            3, 6, b'm', b'e', b'm', b'o', b'r', b'y', 2, 0, 5, b'a', b'l', b'l', b'o', b'c', 0, 0,
            4, b'c', b'a', b'l', b'l', 0, 1,
        ],
    );
    let alloc = [0, 0x41, 0x80, 0x20, 0x0b];
    let mut call = vec![0, 0x41, 0, 0x41];
    // Positive signed LEB: append zero sign group when its last bit6 is set.
    let mut length = leb(raw.len());
    if length.last().unwrap() & 0x40 != 0 {
        *length.last_mut().unwrap() |= 0x80;
        length.push(0);
    }
    call.extend(length);
    call.push(0x0b);
    let mut code = vec![2];
    code.extend(leb(alloc.len()));
    code.extend(alloc);
    code.extend(leb(call.len()));
    code.extend(call);
    section(&mut wasm, 10, &code);
    let mut data = vec![1, 0, 0x41, 0, 0x0b];
    data.extend(leb(raw.len()));
    data.extend(raw);
    section(&mut wasm, 11, &data);
    ModuleCallRequest {
        module_id: "actual-module".into(),
        trace_id: "tick-1".into(),
        entrypoint: "call".into(),
        wasm_hash: hex::encode(Sha256::digest(&wasm)),
        wasm_bytes: Arc::from(wasm),
        input: vec![],
        limits: ModuleLimits {
            max_mem_bytes: 65536,
            max_gas: 100000,
            max_call_rate: 0,
            max_output_bytes: 4096,
            max_effects: 2,
            max_emits: 2,
        },
    }
}
fn work() -> RootModuleOutputWorkV1 {
    RootModuleOutputWorkV1 {
        calls_remaining: 10,
        structure: CborPreflightWorkV1 {
            bytes_remaining: 1 << 20,
            values_remaining: 10000,
            max_depth: 32,
        },
        max_identity_bytes: 128,
        max_original_output_bytes: 4096,
        max_output_heap_bytes: 4 << 20,
    }
}
fn output() -> Vec<u8> {
    serde_cbor::to_vec(&ModuleOutput {
        new_state: Some(vec![1, 2]),
        effects: vec![],
        emits: vec![],
        tick_lifecycle: Some(ModuleTickLifecycleDirective::Suspend),
        output_bytes: 999999,
    })
    .unwrap()
}
#[test]
#[cfg(feature = "wasmtime")]
fn admitted_module_output_executes_actual_wasm_before_charged_decode() {
    let raw = output();
    let request = request(&raw);
    let memory = MemoryBudget::new(16 << 20, 1 << 20).unwrap();
    let mut engine =
        WasmExecutor::new_for_deterministic_replay(WasmExecutorConfig::default()).unwrap();
    let out =
        call_admitted_root_module_output_v1(&mut engine, &request, &memory, &mut work(), || false)
            .unwrap();
    assert_eq!(out.new_state.as_deref(), Some([1, 2].as_slice()));
    assert_eq!(out.output_bytes, raw.len() as u64);
    assert!(out.reserved_bytes() > 0);
    let legacy = engine.call(&request).unwrap();
    assert_eq!(legacy.output_bytes, out.output_bytes);
}
#[test]
#[cfg(feature = "wasmtime")]
fn admitted_module_output_rejects_actual_wasm_huge_length_and_depth_before_serde() {
    for raw in [
        vec![0x9b, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff],
        {
            let mut x = vec![0x81; 40];
            x.push(0);
            x
        },
    ] {
        let mut engine = WasmExecutor::new_for_deterministic_replay(Default::default()).unwrap();
        let memory = MemoryBudget::new(16 << 20, 1 << 20).unwrap();
        assert!(matches!(
            call_admitted_root_module_output_v1(
                &mut engine,
                &request(&raw),
                &memory,
                &mut work(),
                || false
            ),
            Err(RootModuleOutputErrorV1::Value(_))
        ));
    }
}
#[test]
fn admitted_output_protocol_requires_exactly_one_result_and_finite_work() {
    let raw = output();
    let request = request(&raw);
    let memory = MemoryBudget::new(16 << 20, 1 << 20).unwrap();
    let mut w = work();
    let empty = OutputSink::new(&request, &memory, &mut w, || false).unwrap();
    assert!(empty.finish(Ok(())).is_err());
    let mut w = work();
    let mut duplicate = OutputSink::new(&request, &memory, &mut w, || false).unwrap();
    duplicate.consume_output(&raw).unwrap();
    assert!(duplicate.consume_output(&raw).is_err());
    assert!(duplicate.finish(Ok(())).is_err());
    let mut w = work();
    w.max_output_heap_bytes = 0;
    let mut limited = OutputSink::new(&request, &memory, &mut w, || false).unwrap();
    assert!(limited.consume_output(&raw).is_err());
    assert!(matches!(
        limited.finish(Ok(())),
        Err(RootModuleOutputErrorV1::Value(
            ValueTreeError::ResourceLimited(_)
        ))
    ));
    let mut w = work();
    let mut cancelled = OutputSink::new(&request, &memory, &mut w, || true).unwrap();
    assert!(cancelled.consume_output(&raw).is_err());
    assert!(matches!(
        cancelled.finish(Ok(())),
        Err(RootModuleOutputErrorV1::Value(ValueTreeError::Cancelled))
    ));
}

#[test]
#[cfg(feature = "wasmtime")]
fn admitted_module_output_uses_legacy_limit_kernel_and_error_identity() {
    let raw = serde_cbor::to_vec(&ModuleOutput {
        new_state: None,
        effects: vec![],
        emits: (0..3)
            .map(|_| oasis7_wasm_abi::ModuleEmit {
                kind: "test".into(),
                payload: serde_json::Value::Null,
            })
            .collect(),
        tick_lifecycle: None,
        output_bytes: 0,
    })
    .unwrap();
    let request = request(&raw);
    let memory = MemoryBudget::new(16 << 20, 1 << 20).unwrap();
    let mut engine = WasmExecutor::new_for_deterministic_replay(Default::default()).unwrap();
    let legacy = engine.call(&request).unwrap_err();
    let Err(RootModuleOutputErrorV1::Sandbox(actual)) =
        call_admitted_root_module_output_v1(&mut engine, &request, &memory, &mut work(), || false)
    else {
        panic!("expected actual admitted sandbox limit failure");
    };
    assert_eq!(legacy.code, actual.code);
    assert_eq!(legacy.detail, actual.detail);
    assert_eq!(legacy.module_id, actual.module_id);
    assert_eq!(legacy.trace_id, actual.trace_id);
}
