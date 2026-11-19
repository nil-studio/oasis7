//! Request-bound admission/rejection checks; accepted Host execution is tested separately.
use super::*;
use oasis7_proto::cbor_decode_preflight::CborPreflightWorkV1;
use oasis7_wasm_abi::{ModuleCallErrorCode, ModuleLimits};
use std::{
    fs::File,
    sync::{Arc, atomic::AtomicBool},
};

fn request() -> ModuleCallRequest {
    ModuleCallRequest {
        module_id: "scoped-admission".into(),
        wasm_hash: "00".repeat(32),
        trace_id: "real-boundary-rejection".into(),
        entrypoint: "call".into(),
        input: vec![],
        wasm_bytes: Arc::from([]),
        limits: ModuleLimits {
            max_mem_bytes: 65536,
            max_gas: 1000,
            max_call_rate: 0,
            max_output_bytes: 4096,
            max_effects: 0,
            max_emits: 0,
        },
    }
}
fn work() -> RootModuleOutputWorkV1 {
    RootModuleOutputWorkV1 {
        calls_remaining: 1,
        structure: CborPreflightWorkV1 {
            bytes_remaining: 4096,
            values_remaining: 128,
            max_depth: 8,
        },
        max_identity_bytes: 128,
        max_original_output_bytes: 4096,
        max_output_heap_bytes: 65536,
    }
}
fn limits() -> WorkerLimitsV1 {
    WorkerLimitsV1 {
        address_space_bytes: 1 << 20,
        max_request_bytes: 4096,
        max_output_bytes: 4096,
        wall_time_ms: 1000,
    }
}
#[test]
fn native_protocol_zero_calls_rejects_before_process_admission() {
    let memory = MemoryBudget::new(4 << 20, 1 << 20).unwrap();
    let allocator = RootModuleWorkerAllocatorV1::new(&memory);
    let stop = AtomicBool::new(false);
    let file = File::open(std::env::current_exe().unwrap()).unwrap();
    let worker = BoundedWorkerSandboxV1 {
        executable: WorkerExecutableV1::NativeBoundSealedFd {
            file: &file,
            descriptor: &[],
        },
        allocator: &allocator,
        limits: limits(),
        cancelled: &stop,
    };
    let mut work = work();
    work.calls_remaining = 0;
    assert!(matches!(
        verify_scoped_native_protocol_v1(&worker, &request(), &memory, &work),
        Err(RootModuleWorkerErrorV1::Value(
            ValueTreeError::ResourceLimited("native protocol invocation admission")
        ))
    ));
    assert_eq!(work.calls_remaining, 0);
    assert_eq!(work.structure.bytes_remaining, 4096);
}
#[test]
fn native_protocol_cancellation_retains_host_failure_without_vm_frame() {
    let memory = MemoryBudget::new(4 << 20, 1 << 20).unwrap();
    let allocator = RootModuleWorkerAllocatorV1::new(&memory);
    let stop = AtomicBool::new(true);
    let file = File::open(std::env::current_exe().unwrap()).unwrap();
    let worker = BoundedWorkerSandboxV1 {
        executable: WorkerExecutableV1::NativeBoundSealedFd {
            file: &file,
            descriptor: &[],
        },
        allocator: &allocator,
        limits: limits(),
        cancelled: &stop,
    };
    let work = work();
    match verify_scoped_native_protocol_v1(&worker, &request(), &memory, &work) {
        Err(RootModuleWorkerErrorV1::Host(error)) => {
            assert!(error.uses_budget(&memory));
            assert_eq!(
                error.legacy_failure().code,
                ModuleCallErrorCode::Interrupted
            );
            assert_eq!(error.legacy_failure().module_id, "scoped-admission");
        }
        other => panic!("expected charged host cancellation: {other:?}"),
    }
    assert_eq!(work.calls_remaining, 1);
}
#[test]
#[cfg(all(
    target_os = "linux",
    any(target_arch = "aarch64", target_arch = "x86_64")
))]
fn native_protocol_regular_file_rejects_before_child_spawn() {
    let memory = MemoryBudget::new(4 << 20, 1 << 20).unwrap();
    let allocator = RootModuleWorkerAllocatorV1::new(&memory);
    let stop = AtomicBool::new(false);
    let file = File::open(std::env::current_exe().unwrap()).unwrap();
    let worker = BoundedWorkerSandboxV1 {
        executable: WorkerExecutableV1::NativeBoundSealedFd {
            file: &file,
            descriptor: &[],
        },
        allocator: &allocator,
        limits: limits(),
        cancelled: &stop,
    };
    match verify_scoped_native_protocol_v1(&worker, &request(), &memory, &work()) {
        Err(RootModuleWorkerErrorV1::Host(error)) => {
            assert!(error.uses_budget(&memory));
            assert_eq!(
                error.legacy_failure().code,
                ModuleCallErrorCode::SandboxUnavailable
            );
            assert_eq!(
                error.legacy_failure().detail,
                "worker executable kernel seals"
            );
        }
        other => panic!("expected pre-spawn immutable-file rejection: {other:?}"),
    }
}
