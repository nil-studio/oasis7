#![cfg(all(feature = "wasmtime", not(target_arch = "wasm32")))]
//! Payload capacity only: this does not certify compiler/JIT process memory.
use oasis7_proto::{
    cbor_decode_preflight::CborPreflightWorkV1,
    memory_budget::{MemoryBudget, MemoryClass},
};
use oasis7_runtime::{RootModuleOutputWorkV1, call_admitted_root_module_output_v1};
use oasis7_wasm_abi::{ModuleCallRequest, ModuleLimits, ModuleOutput};
use oasis7_wasm_executor::WasmExecutor;
use sha2::{Digest, Sha256};
use std::sync::Arc;
fn leb(mut n: usize) -> Vec<u8> {
    let mut b = Vec::new();
    loop {
        let v = (n & 127) as u8;
        n >>= 7;
        b.push(v | if n != 0 { 128 } else { 0 });
        if n == 0 {
            return b;
        }
    }
}
fn signed_positive(n: usize) -> Vec<u8> {
    let mut b = leb(n);
    if b.last().unwrap() & 0x40 != 0 {
        *b.last_mut().unwrap() |= 0x80;
        b.push(0);
    }
    b
}
fn section(wasm: &mut Vec<u8>, id: u8, data: &[u8]) {
    wasm.push(id);
    wasm.extend(leb(data.len()));
    wasm.extend(data);
}
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
    let pages = raw.len().max(4097).div_ceil(65536);
    let mut memory = vec![1, 0];
    memory.extend(leb(pages));
    section(&mut wasm, 5, &memory);
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
    call.extend(signed_positive(raw.len()));
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
        module_id: "actual-capacity-module".into(),
        trace_id: "actual-capacity-trace".into(),
        entrypoint: "call".into(),
        input: vec![],
        wasm_hash: hex::encode(Sha256::digest(&wasm)),
        wasm_bytes: Arc::from(wasm),
        limits: ModuleLimits {
            max_mem_bytes: (pages * 65536) as u64,
            max_gas: 10_000_000,
            max_call_rate: 0,
            max_output_bytes: 4 << 20,
            max_effects: 2,
            max_emits: 2,
        },
    }
}
fn roundtrip(bytes: usize) {
    let memory = MemoryBudget::new(512 << 20, 64 << 20).unwrap();
    let _fixture = memory
        .reserve(
            bytes
                .checked_mul(128)
                .unwrap()
                .checked_add(16 << 20)
                .unwrap(),
            MemoryClass::Ordinary,
        )
        .unwrap();
    let output = ModuleOutput {
        new_state: Some((0..bytes).map(|i| (i % 256) as u8).collect()),
        effects: vec![],
        emits: vec![],
        tick_lifecycle: None,
        output_bytes: 0,
    };
    let raw = serde_cbor::to_vec(&output).unwrap();
    assert!(raw.len() < 4 << 20);
    let request = request(&raw);
    let mut sandbox = WasmExecutor::new_for_deterministic_replay(Default::default()).unwrap();
    let mut work = RootModuleOutputWorkV1 {
        calls_remaining: 1,
        structure: CborPreflightWorkV1 {
            bytes_remaining: 32 << 20,
            values_remaining: 16 << 20,
            max_depth: 32,
        },
        max_identity_bytes: 128,
        max_original_output_bytes: 4 << 20,
        max_output_heap_bytes: 256 << 20,
    };
    let before = memory.used();
    let actual=call_admitted_root_module_output_v1(&mut sandbox,&request,&memory,&mut work,||false)
        .unwrap_or_else(|e|panic!("actual Wasm new_state bytes={bytes}, encoded={}, 512MiB-pool/256MiB-heap decode failed: {e:?}",raw.len()));
    assert_eq!(actual.new_state, output.new_state);
    assert_eq!(actual.output_bytes, raw.len() as u64);
    assert!(actual.reserved_bytes() <= 256 << 20);
    assert_eq!(work.calls_remaining, 0);
    drop(actual);
    assert_eq!(memory.used(), before);
}
#[test]
fn actual_wasm_output_state_64kib_fits_512mib_pool() {
    roundtrip(64 << 10);
}
#[test]
fn actual_wasm_output_state_1mib_fits_512mib_pool() {
    roundtrip(1 << 20);
}
