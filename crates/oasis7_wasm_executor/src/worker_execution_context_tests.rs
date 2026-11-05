//! These execute real VM code to distinguish host resources from guest traps.
//! They do not claim that a non-Linux process has an OS address-space limit.
use super::*;
use bounded_worker::WorkerExecutionErrorV1;
fn request(body: &str, extra: &str) -> ModuleCallRequest {
    let wasm=wat::parse_str(format!(r#"(module (memory (export "memory") 1 1) {extra} (data (i32.const 16) "\a0") (func (export "alloc") (param i32) (result i32) i32.const 1024) (func (export "call") (param i32 i32) (result i32 i32) {body}))"#)).unwrap();
    ModuleCallRequest {
        module_id: "typed-worker".into(),
        trace_id: "actual-vm".into(),
        entrypoint: "call".into(),
        wasm_hash: sha256_hex(&wasm),
        wasm_bytes: Arc::from(wasm),
        input: vec![0xa0],
        limits: ModuleLimits {
            max_mem_bytes: 1 << 20,
            max_gas: 1_000_000,
            max_output_bytes: 4096,
            max_effects: 1,
            max_emits: 1,
            max_call_rate: 1,
        },
    }
}
fn executor() -> WasmExecutor {
    WasmExecutor::new_for_bounded_worker_v1(WasmExecutorConfig {
        max_mem_bytes: 1 << 20,
        max_output_bytes: 4096,
        max_fuel: 1_000_000,
        ..Default::default()
    })
    .unwrap()
}
#[test]
fn real_worker_guest_trap_and_fuel_are_vm_failures() {
    let executor = executor();
    for (body, code) in [
        ("unreachable", ModuleCallErrorCode::Trap),
        (
            "(loop $forever br $forever) i32.const 16 i32.const 1",
            ModuleCallErrorCode::OutOfFuel,
        ),
    ] {
        let result = executor.call_for_bounded_worker_v1(&request(body, ""), |_| Ok(()));
        assert!(matches!(result,Err(WorkerExecutionErrorV1::Vm(failure)) if failure.code==code));
    }
}
#[test]
fn actual_worker_compile_failure_is_host_not_guest_trap() {
    let executor = executor();
    let mut request = request("i32.const 16 i32.const 1", "");
    request.wasm_bytes = Arc::from([0u8]);
    request.wasm_hash = sha256_hex(&request.wasm_bytes);
    assert!(matches!(
        executor.call_for_bounded_worker_v1(&request, |_| Ok(())),
        Err(WorkerExecutionErrorV1::Host(_))
    ));
}
fn assert_declared_limit_matches_legacy(request: ModuleCallRequest) {
    let worker = executor();
    let legacy = WasmExecutor::new(WasmExecutorConfig {
        max_mem_bytes: 1 << 20,
        max_output_bytes: 4096,
        max_fuel: 1_000_000,
        ..Default::default()
    })
    .unwrap();
    let mut legacy_calls = 0;
    let reference = legacy
        .call_with_raw_output(&request, |_| {
            legacy_calls += 1;
            Ok(())
        })
        .unwrap_err();
    assert_eq!(reference.code, ModuleCallErrorCode::Trap);
    assert_eq!(legacy_calls, 0);
    let mut worker_calls = 0;
    let actual = worker.call_for_bounded_worker_v1(&request, |_| {
        worker_calls += 1;
        Ok(())
    });
    assert!(
        matches!(actual,Err(WorkerExecutionErrorV1::Vm(failure)) if failure.code==reference.code && failure.module_id==reference.module_id && failure.trace_id==reference.trace_id)
    );
    assert_eq!(worker_calls, 0);
}
#[test]
fn declared_memory_maximum_refusal_preserves_legacy_trap() {
    assert_declared_limit_matches_legacy(request(
        "i32.const 1 memory.grow drop i32.const 16 i32.const 1",
        "",
    ));
}
#[test]
fn declared_table_maximum_refusal_preserves_legacy_trap() {
    assert_declared_limit_matches_legacy(request(
        "ref.null func i32.const 1 table.grow drop i32.const 16 i32.const 1",
        "(table 1 1 funcref)",
    ));
}
#[test]
fn worker_output_consumer_failure_is_host_after_real_vm_success() {
    let executor = executor();
    let request = request("i32.const 16 i32.const 1", "");
    let mut calls = 0;
    let result = executor.call_for_bounded_worker_v1(&request, |_| {
        calls += 1;
        Err::<(), _>(ModuleCallFailure {
            module_id: request.module_id.clone(),
            trace_id: request.trace_id.clone(),
            code: ModuleCallErrorCode::OutputTooLarge,
            detail: "actual sink resource limit".into(),
        })
    });
    assert!(matches!(result, Err(WorkerExecutionErrorV1::Host(_))));
    assert_eq!(calls, 1);
}

#[test]
fn actual_approved_memory_allocator_failure_is_host_and_never_reaches_sink() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    struct FailingMemory {
        data: Vec<u8>,
        current: usize,
        attempts: Arc<AtomicUsize>,
    }
    // SAFETY: owned zero-filled fixed allocation, guard size zero, pointer is
    // stable, current bytes stay accessible, no modification outside Wasmtime.
    unsafe impl wasmtime::LinearMemory for FailingMemory {
        fn byte_size(&self) -> usize {
            self.current
        }
        fn byte_capacity(&self) -> usize {
            self.data.len()
        }
        fn as_ptr(&self) -> *mut u8 {
            self.data.as_ptr().cast_mut()
        }
        fn grow_to(&mut self, desired: usize) -> wasmtime::Result<()> {
            if desired <= self.current {
                return Ok(());
            }
            self.attempts.fetch_add(1, Ordering::AcqRel);
            Err(std::io::Error::from_raw_os_error(12).into())
        }
    }
    struct Creator(Arc<AtomicUsize>);
    // SAFETY: fulfils the requested reservation in one stable owned buffer and
    // rejects nonzero guard requests instead of claiming an absent guard.
    unsafe impl wasmtime::MemoryCreator for Creator {
        fn new_memory(
            &self,
            _ty: wasmtime::MemoryType,
            minimum: usize,
            maximum: Option<usize>,
            reserved: Option<usize>,
            guard: usize,
        ) -> Result<Box<dyn wasmtime::LinearMemory>, String> {
            if guard != 0 {
                return Err("fixture guard must be zero".into());
            }
            let capacity = reserved.unwrap_or(maximum.unwrap_or(minimum)).max(minimum);
            if capacity > 1 << 20 {
                return Err("fixture finite memory capacity".into());
            }
            Ok(Box::new(FailingMemory {
                data: vec![0; capacity],
                current: minimum,
                attempts: self.0.clone(),
            }))
        }
    }
    let attempts = Arc::new(AtomicUsize::new(0));
    let mut engine = wasmtime::Config::new();
    engine
        .consume_fuel(true)
        .epoch_interruption(true)
        .wasm_gc(false)
        .wasm_threads(false)
        .memory_init_cow(false)
        .memory_reservation(1 << 20)
        .memory_guard_size(0)
        .memory_reservation_for_growth(0)
        .with_host_memory(Arc::new(Creator(attempts.clone())));
    let mut executor = executor();
    executor.engine = wasmtime::Engine::new(&engine).unwrap();
    let wasm=wat::parse_str(r#"(module (memory (export "memory") 1 2) (data (i32.const 16) "\a0") (func (export "alloc") (param i32) (result i32) i32.const 1024) (func (export "call") (param i32 i32) (result i32 i32) i32.const 1 memory.grow drop i32.const 16 i32.const 1))"#).unwrap();
    let mut request = request("i32.const 16 i32.const 1", "");
    request.wasm_hash = sha256_hex(&wasm);
    request.wasm_bytes = Arc::from(wasm);
    let mut calls = 0;
    let result = executor.call_for_bounded_worker_v1(&request, |_| {
        calls += 1;
        Ok(())
    });
    assert!(matches!(result, Err(WorkerExecutionErrorV1::Host(_))));
    assert_eq!(
        attempts.load(Ordering::Acquire),
        1,
        "real allocator must be reached after declared-limit approval"
    );
    assert_eq!(calls, 0);
}
