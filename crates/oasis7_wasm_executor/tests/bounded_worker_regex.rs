use oasis7_wasm_abi::{CanonicalCborAllocator, CanonicalCborStreamError};
use oasis7_wasm_executor::bounded_worker::*;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
struct Pool {
    used: AtomicUsize,
    max: usize,
}
struct Charge<'a> {
    pool: &'a Pool,
    n: usize,
}
impl Drop for Charge<'_> {
    fn drop(&mut self) {
        self.pool.used.fetch_sub(self.n, Ordering::AcqRel);
    }
}
impl<'a> CanonicalCborAllocator for &'a Pool {
    type Charge = Charge<'a>;
    fn reserve(&self, n: usize) -> Result<Self::Charge, CanonicalCborStreamError> {
        self.used
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |used| {
                used.checked_add(n).filter(|used| *used <= self.max)
            })
            .map_err(|_| CanonicalCborStreamError::ResourceLimited("regex test pool"))?;
        Ok(Charge { pool: self, n })
    }
}
fn limits() -> WorkerLimitsV1 {
    WorkerLimitsV1 {
        address_space_bytes: 256 << 20,
        max_request_bytes: 1 << 20,
        max_output_bytes: 8192,
        wall_time_ms: 5000,
    }
}
#[test]
fn cancelled_and_unsealed_are_not_false() {
    let pool = Pool {
        used: AtomicUsize::new(0),
        max: 300 << 20,
    };
    let allocator = &pool;
    let stop = AtomicBool::new(false);
    let mut host = BoundedWorkerSandboxV1 {
        executable: WorkerExecutableV1::FixturePath(std::path::Path::new(env!(
            "CARGO_BIN_EXE_oasis7-wasm-worker"
        ))),
        allocator: &allocator,
        limits: limits(),
        cancelled: &stop,
    };
    let probes = [RegexProbeV1 {
        pattern: "x",
        text: Some("x"),
    }];
    assert!(matches!(
        host.match_regex_batch_v1(&probes, &mut || true),
        Err(RegexWorkerErrorV1::Cancelled)
    ));
    assert!(matches!(
        host.match_regex_batch_v1(&probes, &mut || false),
        Err(RegexWorkerErrorV1::Unsupported)
    ));
    assert_eq!(pool.used.load(Ordering::Acquire), 0);
}
#[cfg(all(
    target_os = "linux",
    any(target_arch = "x86_64", target_arch = "aarch64"),
    feature = "wasmtime"
))]
fn sealed() -> std::fs::File {
    use std::os::fd::{AsRawFd, FromRawFd};
    unsafe extern "C" {
        fn memfd_create(name: *const std::ffi::c_char, flags: u32) -> i32;
        fn fcntl(fd: i32, command: i32, ...) -> i32;
    }
    let fd = unsafe { memfd_create(c"regex-worker-fixture".as_ptr(), 3) };
    assert!(fd >= 0);
    let mut file = unsafe { std::fs::File::from_raw_fd(fd) };
    let mut source = std::fs::File::open(env!("CARGO_BIN_EXE_oasis7-wasm-worker")).unwrap();
    std::io::copy(&mut source, &mut file).unwrap();
    assert_eq!(unsafe { fcntl(file.as_raw_fd(), 1033, 15) }, 0);
    file
}
#[test]
#[cfg(all(
    target_os = "linux",
    any(target_arch = "x86_64", target_arch = "aarch64"),
    feature = "wasmtime"
))]
fn sealed_regex_batch_holds_charge_and_preserves_semantics() {
    let file = sealed();
    let pool = Pool {
        used: AtomicUsize::new(0),
        max: 300 << 20,
    };
    let allocator = &pool;
    let stop = AtomicBool::new(false);
    let mut host = BoundedWorkerSandboxV1 {
        executable: WorkerExecutableV1::SealedFd(&file),
        allocator: &allocator,
        limits: limits(),
        cancelled: &stop,
    };
    let probes = [
        RegexProbeV1 {
            pattern: "^a+$",
            text: Some("aaa"),
        },
        RegexProbeV1 {
            pattern: "^a+$",
            text: Some("b"),
        },
        RegexProbeV1 {
            pattern: "[a-z]+",
            text: None,
        },
    ];
    let result = host.match_regex_batch_v1(&probes, &mut || false).unwrap();
    assert_eq!(result.values(), &[true, false, true]);
    assert!(pool.used.load(Ordering::Acquire) > 0);
    drop(result);
    assert_eq!(pool.used.load(Ordering::Acquire), 0);
    assert!(matches!(
        host.match_regex_batch_v1(
            &[RegexProbeV1 {
                pattern: "[",
                text: None
            }],
            &mut || false
        ),
        Err(RegexWorkerErrorV1::InvalidPattern { probe: 0 })
    ));
    assert_eq!(pool.used.load(Ordering::Acquire), 0);
    // Admission failure and kernel exhaustion are errors, with no result owner or retained fee.
    host.limits.address_space_bytes = 400 << 20;
    assert!(matches!(
        host.match_regex_batch_v1(&probes, &mut || false),
        Err(RegexWorkerErrorV1::Admission(_))
    ));
    host.limits.address_space_bytes = 1 << 20;
    assert!(matches!(
        host.match_regex_batch_v1(&probes, &mut || false),
        Err(RegexWorkerErrorV1::Host(_))
    ));
    assert_eq!(pool.used.load(Ordering::Acquire), 0);
}

#[test]
#[cfg(all(
    target_os = "linux",
    any(target_arch = "x86_64", target_arch = "aarch64"),
    feature = "wasmtime"
))]
fn regex_wait_polls_callback_and_releases_after_join() {
    let file = sealed();
    let pool = Pool {
        used: AtomicUsize::new(0),
        max: 300 << 20,
    };
    let allocator = &pool;
    let stop = AtomicBool::new(false);
    let mut host = BoundedWorkerSandboxV1 {
        executable: WorkerExecutableV1::SealedFd(&file),
        allocator: &allocator,
        limits: limits(),
        cancelled: &stop,
    };
    let probes = [RegexProbeV1 {
        pattern: "(?:a|b){10000}",
        text: Some("a"),
    }];
    let start = std::time::Instant::now();
    let result = host.match_regex_batch_v1(&probes, &mut || {
        start.elapsed() >= std::time::Duration::from_millis(1)
    });
    assert!(matches!(result, Err(RegexWorkerErrorV1::Cancelled)));
    assert_eq!(pool.used.load(Ordering::Acquire), 0);
}

#[test]
#[cfg(all(
    target_os = "linux",
    any(target_arch = "x86_64", target_arch = "aarch64"),
    feature = "wasmtime"
))]
fn native_bound_regex_without_actual_verifier_never_falls_back() {
    let file = sealed();
    let pool = Pool {
        used: AtomicUsize::new(0),
        max: 300 << 20,
    };
    let allocator = &pool;
    let stop = AtomicBool::new(false);
    let mut host = BoundedWorkerSandboxV1 {
        executable: WorkerExecutableV1::NativeBoundSealedFd {
            file: &file,
            descriptor: &[0xa0],
        },
        allocator: &allocator,
        limits: limits(),
        cancelled: &stop,
    };
    let probes = [RegexProbeV1 {
        pattern: "x",
        text: Some("x"),
    }];
    // The actual generic fixture can execute regex, but lacks the binary
    // owner's native verifier. A bound request cannot use its generic lane.
    assert!(matches!(
        host.match_regex_batch_v1(&probes, &mut || false),
        Err(RegexWorkerErrorV1::Host(_))
    ));
    assert_eq!(pool.used.load(Ordering::Acquire), 0);
    assert!(matches!(
        host.match_regex_batch_v1(&probes, &mut || true),
        Err(RegexWorkerErrorV1::Cancelled)
    ));
    assert_eq!(pool.used.load(Ordering::Acquire), 0);
}
