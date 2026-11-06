//! Actual process backend: no callback stands in for VM execution.
use oasis7_wasm_abi::{
    module_raw_output::{ModuleRawOutputSinkV1, ModuleSandboxRawOutputV1},
    *,
};
use oasis7_wasm_executor::bounded_worker::*;
use std::{
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
};
struct Pool {
    used: AtomicUsize,
    limit: usize,
}
struct Charge<'a> {
    pool: &'a Pool,
    bytes: usize,
}
impl Drop for Charge<'_> {
    fn drop(&mut self) {
        self.pool.used.fetch_sub(self.bytes, Ordering::AcqRel);
    }
}
// This fixture pool uses the same atomic admission for every simultaneous call;
// production supplies the actual invocation's MemoryBudget adapter instead.
struct Allocator<'a>(&'a Pool);
impl<'a> CanonicalCborAllocator for Allocator<'a> {
    type Charge = Charge<'a>;
    fn reserve(&self, bytes: usize) -> Result<Self::Charge, CanonicalCborStreamError> {
        self.0
            .used
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| {
                n.checked_add(bytes).filter(|n| *n <= self.0.limit)
            })
            .map_err(|_| CanonicalCborStreamError::ResourceLimited("worker test pool"))?;
        Ok(Charge {
            pool: self.0,
            bytes,
        })
    }
}
struct Sink {
    bytes: Vec<u8>,
    calls: usize,
}
impl ModuleRawOutputSinkV1 for Sink {
    fn consume_output(&mut self, b: &[u8]) -> Result<(), ModuleCallFailure> {
        self.calls += 1;
        self.bytes.extend_from_slice(b);
        Ok(())
    }
}
fn request(loop_forever: bool) -> ModuleCallRequest {
    use sha2::{Digest, Sha256};
    let body = if loop_forever {
        "(loop $forever br $forever) i32.const 16 i32.const 1"
    } else {
        "i32.const 16 i32.const 1"
    };
    let wasm=wat::parse_str(format!(r#"(module (memory (export "memory") 1) (data (i32.const 16) "\a0") (func (export "alloc") (param i32) (result i32) i32.const 1024) (func (export "call") (param i32 i32) (result i32 i32) {body}))"#)).unwrap();
    ModuleCallRequest {
        module_id: "worker-fixture".into(),
        wasm_hash: Sha256::digest(&wasm)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect(),
        trace_id: "actual-process".into(),
        entrypoint: "call".into(),
        input: vec![0xa0],
        wasm_bytes: Arc::from(wasm),
        limits: ModuleLimits {
            max_mem_bytes: 1 << 20,
            max_gas: u64::MAX,
            max_output_bytes: 4096,
            max_effects: 1,
            max_emits: 1,
            max_call_rate: 1,
        },
    }
}
fn limits(cap: u64) -> WorkerLimitsV1 {
    WorkerLimitsV1 {
        address_space_bytes: cap,
        max_request_bytes: 1 << 20,
        max_output_bytes: 4096,
        wall_time_ms: 10_000,
    }
}
fn executable() -> &'static Path {
    Path::new(env!("CARGO_BIN_EXE_oasis7-wasm-worker"))
}
#[test]
fn real_worker_is_bounded_or_explicitly_unsupported() {
    let pool = Pool {
        used: AtomicUsize::new(0),
        limit: 600 << 20,
    };
    let allocator = Allocator(&pool);
    let stop = AtomicBool::new(false);
    let mut sandbox = BoundedWorkerSandboxV1 {
        executable: WorkerExecutableV1::FixturePath(executable()),
        allocator: &allocator,
        limits: limits(512 << 20),
        cancelled: &stop,
    };
    let mut sink = Sink {
        bytes: vec![],
        calls: 0,
    };
    let result = sandbox.call_to(&request(false), &mut sink);
    if cfg!(all(
        target_os = "linux",
        any(target_arch = "x86_64", target_arch = "aarch64"),
        feature = "wasmtime"
    )) {
        result.unwrap();
        assert_eq!(sink.calls, 1);
        assert_eq!(sink.bytes, vec![0xa0]);
    } else {
        assert!(result.is_err());
        assert_eq!(sink.calls, 0);
    }
    assert_eq!(pool.used.load(Ordering::Acquire), 0);
}
#[test]
#[cfg(all(
    target_os = "linux",
    any(target_arch = "x86_64", target_arch = "aarch64"),
    feature = "wasmtime"
))]
fn kernel_exhaustion_never_falls_back_or_leaks_worker_charge() {
    let pool = Pool {
        used: AtomicUsize::new(0),
        limit: 16 << 20,
    };
    let allocator = Allocator(&pool);
    let stop = AtomicBool::new(false);
    let mut sandbox = BoundedWorkerSandboxV1 {
        executable: WorkerExecutableV1::FixturePath(executable()),
        allocator: &allocator,
        limits: limits(1 << 20),
        cancelled: &stop,
    };
    let mut sink = Sink {
        bytes: vec![],
        calls: 0,
    };
    assert!(sandbox.call_to(&request(false), &mut sink).is_err());
    assert_eq!(sink.calls, 0);
    assert_eq!(pool.used.load(Ordering::Acquire), 0);
}
#[test]
#[cfg(all(
    target_os = "linux",
    any(target_arch = "x86_64", target_arch = "aarch64"),
    feature = "wasmtime"
))]
fn cancellation_joins_worker_before_releasing_shared_concurrent_admission() {
    let pool = Pool {
        used: AtomicUsize::new(0),
        limit: 600 << 20,
    };
    let stop = AtomicBool::new(false);
    std::thread::scope(|scope| {
        let handle = scope.spawn(|| {
            let allocator = Allocator(&pool);
            let mut sandbox = BoundedWorkerSandboxV1 {
                executable: WorkerExecutableV1::FixturePath(executable()),
                allocator: &allocator,
                limits: limits(512 << 20),
                cancelled: &stop,
            };
            let mut sink = Sink {
                bytes: vec![],
                calls: 0,
            };
            let result = sandbox.call_to(&request(true), &mut sink);
            (result, sink.calls)
        });
        let start = std::time::Instant::now();
        while pool.used.load(Ordering::Acquire) == 0 {
            assert!(start.elapsed().as_secs() < 5);
            std::thread::yield_now();
        }
        let allocator = Allocator(&pool);
        let second_stop = AtomicBool::new(false);
        let mut second = BoundedWorkerSandboxV1 {
            executable: WorkerExecutableV1::FixturePath(executable()),
            allocator: &allocator,
            limits: limits(512 << 20),
            cancelled: &second_stop,
        };
        let mut sink = Sink {
            bytes: vec![],
            calls: 0,
        };
        assert!(second.call_to(&request(false), &mut sink).is_err());
        assert_eq!(sink.calls, 0);
        stop.store(true, Ordering::Release);
        let (result, calls) = handle.join().unwrap();
        assert!(result.is_err());
        assert_eq!(calls, 0);
    });
    assert_eq!(pool.used.load(Ordering::Acquire), 0);
}

#[test]
#[cfg(all(
    target_os = "linux",
    any(target_arch = "x86_64", target_arch = "aarch64"),
    feature = "wasmtime"
))]
fn real_vm_executes_exact_kernel_sealed_fixture_fd() {
    use std::{
        fs::File,
        io::Write,
        os::fd::{AsRawFd, FromRawFd},
    };
    let source = File::open(executable()).unwrap();
    let length = source.metadata().unwrap().len();
    let fee = usize::try_from(length).unwrap().checked_add(4096).unwrap();
    let pool = Pool {
        used: AtomicUsize::new(0),
        limit: fee.checked_add(600 << 20).unwrap(),
    };
    let allocator = Allocator(&pool);
    let charge = allocator.reserve(fee).unwrap();
    unsafe extern "C" {
        fn memfd_create(name: *const std::ffi::c_char, flags: u32) -> i32;
        fn fcntl(fd: i32, command: i32, ...) -> i32;
    }
    let fd = unsafe { memfd_create(c"oasis7-worker-fixture".as_ptr(), 3) };
    assert!(fd >= 0);
    let mut sealed = unsafe { File::from_raw_fd(fd) };
    assert_eq!(std::io::copy(&mut &source, &mut sealed).unwrap(), length);
    assert_eq!(unsafe { fcntl(sealed.as_raw_fd(), 1033, 15) }, 0);
    assert!(sealed.write_all(b"cannot change executable").is_err());
    let stop = AtomicBool::new(false);
    let mut sandbox = BoundedWorkerSandboxV1 {
        executable: WorkerExecutableV1::SealedFd(&sealed),
        allocator: &allocator,
        limits: limits(512 << 20),
        cancelled: &stop,
    };
    let mut sink = Sink {
        bytes: vec![],
        calls: 0,
    };
    sandbox.verify_native_protocol_v1(&request(false)).unwrap();
    sandbox
        .call_to_typed_v1(&request(false), &mut sink)
        .unwrap();
    assert_eq!(sink.bytes, vec![0xa0]);
    assert_eq!(sink.calls, 1);
    drop(sealed);
    drop(charge);
    assert_eq!(pool.used.load(Ordering::Acquire), 0);
}

#[test]
fn fixture_path_is_not_a_typed_native_execution_input() {
    let pool = Pool {
        used: AtomicUsize::new(0),
        limit: 600 << 20,
    };
    let allocator = Allocator(&pool);
    let stop = AtomicBool::new(false);
    let mut sandbox = BoundedWorkerSandboxV1 {
        executable: WorkerExecutableV1::FixturePath(executable()),
        allocator: &allocator,
        limits: limits(512 << 20),
        cancelled: &stop,
    };
    let mut sink = Sink {
        bytes: vec![],
        calls: 0,
    };
    assert!(matches!(
        sandbox.call_to_typed_v1(&request(false), &mut sink),
        Err(WorkerExecutionErrorV1::Host(_))
    ));
    assert_eq!(sink.calls, 0);
    assert_eq!(pool.used.load(Ordering::Acquire), 0);
}

#[test]
#[cfg(all(
    target_os = "linux",
    any(target_arch = "x86_64", target_arch = "aarch64")
))]
fn native_handshake_rejects_wrong_sealed_executable_and_cancellation() {
    use std::{
        fs::File,
        os::fd::{AsRawFd, FromRawFd},
    };
    unsafe extern "C" {
        fn memfd_create(name: *const std::ffi::c_char, flags: u32) -> i32;
        fn fcntl(fd: i32, command: i32, ...) -> i32;
    }
    let fd = unsafe { memfd_create(c"wrong-worker-fixture".as_ptr(), 3) };
    assert!(fd >= 0);
    let mut sealed = unsafe { File::from_raw_fd(fd) };
    let mut wrong = File::open("/bin/true").unwrap();
    std::io::copy(&mut wrong, &mut sealed).unwrap();
    assert_eq!(unsafe { fcntl(sealed.as_raw_fd(), 1033, 15) }, 0);
    let pool = Pool {
        used: AtomicUsize::new(0),
        limit: 600 << 20,
    };
    let allocator = Allocator(&pool);
    let stop = AtomicBool::new(false);
    let sandbox = BoundedWorkerSandboxV1 {
        executable: WorkerExecutableV1::SealedFd(&sealed),
        allocator: &allocator,
        limits: limits(512 << 20),
        cancelled: &stop,
    };
    // The wrong native image genuinely runs/exits but cannot return this ABI.
    assert!(matches!(
        sandbox.verify_native_protocol_v1(&request(false)),
        Err(WorkerExecutionErrorV1::Host(_))
    ));
    assert_eq!(pool.used.load(Ordering::Acquire), 0);
    stop.store(true, Ordering::Release);
    assert!(matches!(
        sandbox.verify_native_protocol_v1(&request(false)),
        Err(WorkerExecutionErrorV1::Host(_))
    ));
    assert_eq!(pool.used.load(Ordering::Acquire), 0);
}

#[test]
#[cfg(all(
    target_os = "linux",
    any(target_arch = "x86_64", target_arch = "aarch64")
))]
fn cloexec_regular_file_is_rejected_before_worker_spawn() {
    use std::{fs::File, os::fd::AsRawFd};
    unsafe extern "C" {
        fn fcntl(fd: i32, command: i32, ...) -> i32;
    }
    let regular = File::open(executable()).unwrap();
    assert!(regular.metadata().unwrap().is_file());
    // SAFETY: read-only queries of the live fixture descriptor.
    let flags = unsafe { fcntl(regular.as_raw_fd(), 1) };
    assert!(flags >= 0 && flags & 1 == 1);
    // SAFETY: an unsupported F_GET_SEALS is an expected negative result for
    // ordinary filesystem files, and must never count as all seals present.
    let seals = unsafe { fcntl(regular.as_raw_fd(), 1034) };
    assert!(seals < 0 || seals & 15 != 15);
    let pool = Pool {
        used: AtomicUsize::new(0),
        limit: 600 << 20,
    };
    let allocator = Allocator(&pool);
    let stop = AtomicBool::new(false);
    let sandbox = BoundedWorkerSandboxV1 {
        executable: WorkerExecutableV1::SealedFd(&regular),
        allocator: &allocator,
        limits: limits(512 << 20),
        cancelled: &stop,
    };
    let error = sandbox
        .verify_native_protocol_v1(&request(false))
        .unwrap_err();
    let WorkerExecutionErrorV1::Host(error) = error else {
        panic!("expected host rejection before spawn")
    };
    assert_eq!(
        error.legacy_failure().code,
        ModuleCallErrorCode::SandboxUnavailable
    );
    // This exact error originates at executable::path before Command::new or
    // spawn; a child exit or absent handshake cannot satisfy this assertion.
    assert_eq!(
        error.legacy_failure().detail,
        "worker executable kernel seals"
    );
    assert_eq!(pool.used.load(Ordering::Acquire), 0);
}
