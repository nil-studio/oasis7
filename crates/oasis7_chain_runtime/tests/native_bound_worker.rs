//! Actual same-image Linux capture → sealed ELF → same-child native checks.
#![cfg(all(
    target_os = "linux",
    any(target_arch = "x86_64", target_arch = "aarch64")
))]
use oasis7_formal_protocol::native_environment::{
    NativeEnvironmentRole, NativeExecutionEnvironmentV1,
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass};
use oasis7_runtime::incremental_module_execution_worker::RootModuleWorkerAllocatorV1;
use oasis7_wasm_abi::{
    ModuleCallFailure, ModuleCallRequest, ModuleLimits, module_raw_output::ModuleRawOutputSinkV1,
};
use oasis7_wasm_executor::bounded_worker::{
    BoundedWorkerSandboxV1, RegexProbeV1, RegexWorkerErrorV1, WorkerExecutableV1,
    WorkerExecutionErrorV1, WorkerLimitsV1,
};
use std::{
    fs::{File, OpenOptions},
    io::Write,
    os::fd::{AsRawFd, FromRawFd},
    path::{Path, PathBuf},
    process::Command,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};
fn source_image() -> &'static Path {
    Path::new(env!("CARGO_BIN_EXE_oasis7-native-bound-fixture"))
}
struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        // Docker Desktop host-bind mappings can expose a different st_dev
        // through /proc/maps and stat. Use a real Linux-local filesystem for
        // the evidence fixture, keeping production identity checks strict.
        let path = Path::new("/tmp").join(format!(
            "oasis7-native-bound-integration-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn image(&self) -> PathBuf {
        let image = self.0.join("native-fixture-image");
        std::fs::copy(source_image(), &image).unwrap();
        let source = std::fs::read(source_image()).unwrap();
        let copied = std::fs::read(&image).unwrap();
        assert_eq!(blake3::hash(&source), blake3::hash(&copied));
        image
    }
    fn dso(&self, changing: bool) -> PathBuf {
        let source = self.0.join("dependency.c");
        std::fs::write(&source, b"int native_fixture_value(void) { return 17; }\n").unwrap();
        let path = self.0.join(if changing {
            "oasis7-native-bound-test-changing.so"
        } else {
            "oasis7-native-bound-test-stable.so"
        });
        let result = Command::new("cc")
            .args(["-shared", "-fPIC"])
            .arg(&source)
            .arg("-o")
            .arg(&path)
            .output()
            .expect("native integration requires the Linux C toolchain");
        assert!(
            result.status.success(),
            "actual DSO compile failed: {}",
            String::from_utf8_lossy(&result.stderr)
        );
        path
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}
fn descriptor(image: &Path, dependency: Option<&Path>) -> Vec<u8> {
    let mut command = Command::new(image);
    command.arg("--capture-native-fixture-v1");
    if let Some(path) = dependency {
        command.arg(path);
    }
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "actual fixture capture: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let descriptor = NativeExecutionEnvironmentV1::decode(&output.stdout).unwrap();
    let executable = descriptor
        .components
        .iter()
        .find(|component| component.role == NativeEnvironmentRole::Executable)
        .unwrap();
    let bytes = std::fs::read(image).unwrap();
    assert_eq!(executable.byte_len, bytes.len() as u64);
    assert_eq!(
        executable.content_digest,
        blake3::hash(&bytes).to_hex().as_str()
    );
    if let Some(path) = dependency {
        assert!(descriptor.components.iter().any(|component| component.role
            == NativeEnvironmentRole::LoadedLibrary
            && component.name.ends_with(path.to_str().unwrap())));
    }
    output.stdout
}
fn request() -> ModuleCallRequest {
    use sha2::{Digest, Sha256};
    let wasm = wat::parse_str(r#"(module (memory (export "memory") 1) (data (i32.const 16) "\a0") (func (export "alloc") (param i32) (result i32) i32.const 1024) (func (export "call") (param i32 i32) (result i32 i32) i32.const 16 i32.const 1))"#).unwrap();
    ModuleCallRequest {
        module_id: "native-bound-integration".into(),
        wasm_hash: hex::encode(Sha256::digest(&wasm)),
        trace_id: "same-image-original".into(),
        entrypoint: "call".into(),
        input: vec![0xa0],
        wasm_bytes: Arc::from(wasm),
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
#[derive(Default)]
struct Sink {
    calls: usize,
    bytes: Vec<u8>,
}
impl ModuleRawOutputSinkV1 for Sink {
    fn consume_output(&mut self, bytes: &[u8]) -> Result<(), ModuleCallFailure> {
        self.calls += 1;
        self.bytes.extend_from_slice(bytes);
        Ok(())
    }
}
fn with_worker<T>(
    image: &Path,
    bytes: &[u8],
    stop: &AtomicBool,
    operation: impl FnOnce(&mut BoundedWorkerSandboxV1<'_, RootModuleWorkerAllocatorV1>) -> T,
) -> T {
    let memory = MemoryBudget::new(1024 << 20, 32 << 20).unwrap();
    let length = std::fs::metadata(image).unwrap().len();
    let image_fee = memory
        .reserve(
            usize::try_from(length).unwrap() + 4096,
            MemoryClass::Ordinary,
        )
        .unwrap();
    unsafe extern "C" {
        fn memfd_create(name: *const std::ffi::c_char, flags: u32) -> i32;
        fn fcntl(fd: i32, command: i32, ...) -> i32;
    }
    // SAFETY: static nul-terminated name; success transfers an owned descriptor.
    let fd = unsafe { memfd_create(c"native-bound-same-image".as_ptr(), 3) };
    assert!(fd >= 0);
    // SAFETY: fd is the newly allocated descriptor owned by this File.
    let mut sealed = unsafe { File::from_raw_fd(fd) };
    assert_eq!(
        std::io::copy(&mut File::open(image).unwrap(), &mut sealed).unwrap(),
        length
    );
    // SAFETY: applies irreversible seals to our owned memfd.
    assert_eq!(unsafe { fcntl(sealed.as_raw_fd(), 1033, 15) }, 0);
    assert!(sealed.write_all(b"immutable").is_err());
    let allocator = RootModuleWorkerAllocatorV1::new(&memory);
    let mut worker = BoundedWorkerSandboxV1 {
        executable: WorkerExecutableV1::NativeBoundSealedFd {
            file: &sealed,
            descriptor: bytes,
        },
        allocator: &allocator,
        limits: WorkerLimitsV1 {
            address_space_bytes: 512 << 20,
            max_request_bytes: 2 << 20,
            max_output_bytes: 4096,
            wall_time_ms: 30_000,
        },
        cancelled: stop,
    };
    let output = operation(&mut worker);
    drop(sealed);
    drop(image_fee);
    output
}
fn call(
    image: &Path,
    bytes: &[u8],
    stop: &AtomicBool,
) -> (Result<(), WorkerExecutionErrorV1>, Sink) {
    with_worker(image, bytes, stop, |worker| {
        let mut sink = Sink::default();
        let result = worker.call_to_typed_v1(&request(), &mut sink);
        (result, sink)
    })
}
fn regex_call(
    image: &Path,
    bytes: &[u8],
    stop: &AtomicBool,
    invalid: bool,
) -> (Result<(), RegexWorkerErrorV1>, Option<Vec<bool>>) {
    with_worker(image, bytes, stop, |worker| {
        let probes = [
            RegexProbeV1 {
                pattern: "^abc[0-9]+$",
                text: Some("abc17"),
            },
            RegexProbeV1 {
                pattern: if invalid { "(" } else { "^abc[0-9]+$" },
                text: Some("no"),
            },
            RegexProbeV1 {
                pattern: "a+",
                text: None,
            },
        ];
        let mut output = None;
        let result = worker
            .match_regex_batch_v1(&probes, &mut || false)
            .map(|values| {
                output = Some(values.values().to_vec());
            });
        (result, output)
    })
}
// Keep this fixture's local ELF and genuine DSO footprint bounded to one
// case, independent of the runner's global libtest thread count.
static FIXTURE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
#[test]
fn actual_same_image_and_real_dso_produce_typed_output() {
    let _serial = FIXTURE_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let directory = Directory::new();
    let image = directory.image();
    let dso = directory.dso(false);
    let original = descriptor(&image, Some(&dso));
    let (result, sink) = call(&image, &original, &AtomicBool::new(false));
    result.unwrap();
    assert_eq!(sink.calls, 1);
    assert_eq!(sink.bytes, vec![0xa0]);
}
#[test]
fn wrong_original_descriptor_has_no_parent_output() {
    let _serial = FIXTURE_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let directory = Directory::new();
    let image = directory.image();
    let mut actual = NativeExecutionEnvironmentV1::decode(&descriptor(&image, None)).unwrap();
    actual
        .components
        .iter_mut()
        .find(|component| component.role == NativeEnvironmentRole::Executable)
        .unwrap()
        .content_digest = "00".repeat(32);
    let (result, sink) = call(&image, &actual.encode().unwrap(), &AtomicBool::new(false));
    assert!(matches!(result, Err(WorkerExecutionErrorV1::Host(_))));
    assert_eq!(sink.calls, 0);
}
#[test]
fn changed_real_dso_is_rejected_before_invocation_output() {
    let _serial = FIXTURE_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let directory = Directory::new();
    let image = directory.image();
    let dso = directory.dso(false);
    let original = descriptor(&image, Some(&dso));
    let file = OpenOptions::new().write(true).open(&dso).unwrap();
    file.set_len(file.metadata().unwrap().len() + 1).unwrap();
    file.sync_all().unwrap();
    let (result, sink) = call(&image, &original, &AtomicBool::new(false));
    assert!(matches!(result, Err(WorkerExecutionErrorV1::Host(_))));
    assert_eq!(sink.calls, 0);
}
#[test]
fn actual_post_call_dso_change_prevents_all_parent_output() {
    let _serial = FIXTURE_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let directory = Directory::new();
    let image = directory.image();
    let dso = directory.dso(true);
    let original = descriptor(&image, Some(&dso));
    let length = std::fs::metadata(&dso).unwrap().len();
    let (result, sink) = call(&image, &original, &AtomicBool::new(false));
    assert!(matches!(result, Err(WorkerExecutionErrorV1::Host(_))));
    assert_eq!(sink.calls, 0);
    // The fixture changes this backing file only in its actual same-child
    // after-call callback, proving this was not merely a pre-call rejection.
    assert_eq!(std::fs::metadata(&dso).unwrap().len(), length + 1);
}
#[test]
fn cancellation_has_no_parent_output_or_dso_mutation() {
    let _serial = FIXTURE_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let directory = Directory::new();
    let image = directory.image();
    let dso = directory.dso(true);
    let original = descriptor(&image, Some(&dso));
    let length = std::fs::metadata(&dso).unwrap().len();
    let (result, sink) = call(&image, &original, &AtomicBool::new(true));
    assert!(matches!(result, Err(WorkerExecutionErrorV1::Host(_))));
    assert_eq!(sink.calls, 0);
    assert_eq!(std::fs::metadata(&dso).unwrap().len(), length);
}

#[test]
fn native_bound_regex_matches_under_actual_same_child_environment() {
    let _serial = FIXTURE_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let directory = Directory::new();
    let image = directory.image();
    let dso = directory.dso(false);
    let original = descriptor(&image, Some(&dso));
    let (result, output) = regex_call(&image, &original, &AtomicBool::new(false), false);
    result.unwrap();
    assert_eq!(output, Some(vec![true, false, true]));
}
#[test]
fn native_bound_regex_invalid_pattern_retains_original_index() {
    let _serial = FIXTURE_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let directory = Directory::new();
    let image = directory.image();
    let original = descriptor(&image, None);
    let (result, output) = regex_call(&image, &original, &AtomicBool::new(false), true);
    assert!(matches!(
        result,
        Err(RegexWorkerErrorV1::InvalidPattern { probe: 1 })
    ));
    assert!(output.is_none());
}
#[test]
fn native_bound_regex_wrong_descriptor_has_no_parent_results() {
    let _serial = FIXTURE_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let directory = Directory::new();
    let image = directory.image();
    let mut original = NativeExecutionEnvironmentV1::decode(&descriptor(&image, None)).unwrap();
    original
        .components
        .iter_mut()
        .find(|component| component.role == NativeEnvironmentRole::Executable)
        .unwrap()
        .content_digest = "00".repeat(32);
    let (result, output) = regex_call(
        &image,
        &original.encode().unwrap(),
        &AtomicBool::new(false),
        false,
    );
    assert!(matches!(result, Err(RegexWorkerErrorV1::Host(_))));
    assert!(output.is_none());
}
#[test]
fn native_bound_regex_changed_real_dso_has_no_parent_results() {
    let _serial = FIXTURE_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let directory = Directory::new();
    let image = directory.image();
    let dso = directory.dso(false);
    let original = descriptor(&image, Some(&dso));
    let file = OpenOptions::new().write(true).open(&dso).unwrap();
    file.set_len(file.metadata().unwrap().len() + 1).unwrap();
    file.sync_all().unwrap();
    let (result, output) = regex_call(&image, &original, &AtomicBool::new(false), false);
    assert!(matches!(result, Err(RegexWorkerErrorV1::Host(_))));
    assert!(output.is_none());
}
#[test]
fn native_bound_regex_post_call_dso_change_blocks_success_and_pattern_errors() {
    let _serial = FIXTURE_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    for invalid in [false, true] {
        let directory = Directory::new();
        let image = directory.image();
        let dso = directory.dso(true);
        let original = descriptor(&image, Some(&dso));
        let length = std::fs::metadata(&dso).unwrap().len();
        let (result, output) = regex_call(&image, &original, &AtomicBool::new(false), invalid);
        assert!(matches!(result, Err(RegexWorkerErrorV1::Host(_))));
        assert!(output.is_none());
        assert_eq!(std::fs::metadata(&dso).unwrap().len(), length + 1);
    }
}
#[test]
fn native_bound_regex_cancelled_has_no_parent_results_or_dso_change() {
    let _serial = FIXTURE_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let directory = Directory::new();
    let image = directory.image();
    let dso = directory.dso(true);
    let original = descriptor(&image, Some(&dso));
    let length = std::fs::metadata(&dso).unwrap().len();
    let (result, output) = regex_call(&image, &original, &AtomicBool::new(true), false);
    assert!(matches!(result, Err(RegexWorkerErrorV1::Cancelled)));
    assert!(output.is_none());
    assert_eq!(std::fs::metadata(&dso).unwrap().len(), length);
}
