//! One real VM invocation in a kernel-address-space-limited process.
//! Unsupported hosts never fall back to the in-process compiler. The host
//! allocator must be its actual invocation pool; its charge survives wait().
use oasis7_wasm_abi::{
    CanonicalCborAllocator, ModuleCallErrorCode, ModuleCallFailure, ModuleCallRequest,
    ModuleOutput, ModuleSandbox,
    module_raw_output::{ModuleRawOutputSinkV1, ModuleSandboxRawOutputV1},
};
#[cfg(all(
    target_os = "linux",
    any(target_arch = "x86_64", target_arch = "aarch64")
))]
use std::io::{Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
#[path = "bounded_worker_child.rs"]
mod child;
pub use child::{worker_main_v1, worker_main_with_native_verifier_v1};
#[path = "bounded_worker_native_bound.rs"]
mod native_bound;
pub use native_bound::NativeEnvironmentVerifierV1;
#[path = "bounded_worker_exec.rs"]
mod executable;
pub use executable::WorkerExecutableV1;
#[path = "bounded_worker_handshake.rs"]
mod handshake;
#[path = "bounded_worker_regex.rs"]
mod regex_batch;
pub use regex_batch::{RegexProbeV1, RegexResultsV1, RegexWorkerErrorV1};

pub struct WorkerLimitsV1 {
    pub address_space_bytes: u64,
    pub max_request_bytes: u64,
    pub max_output_bytes: u64,
    pub wall_time_ms: u64,
}
/// The executable is an installed trusted oasis7-wasm-worker binary, not a
/// module-selected command. Namespace installation/authentication is the host's
/// responsibility. Limits are explicit; no machine-wide quota is increased.
pub struct BoundedWorkerSandboxV1<'a, A> {
    pub executable: WorkerExecutableV1<'a>,
    pub allocator: &'a A,
    pub limits: WorkerLimitsV1,
    pub cancelled: &'a AtomicBool,
}
fn short(s: &str) -> String {
    let mut n = s.len().min(256);
    while !s.is_char_boundary(n) {
        n -= 1;
    }
    s[..n].to_owned()
}
fn failure(
    r: &ModuleCallRequest,
    code: ModuleCallErrorCode,
    message: &'static str,
) -> ModuleCallFailure {
    ModuleCallFailure {
        module_id: short(&r.module_id),
        trace_id: short(&r.trace_id),
        code,
        detail: message.into(),
    }
}
#[cfg(all(
    target_os = "linux",
    any(target_arch = "x86_64", target_arch = "aarch64")
))]
fn encode_failure(code: &ModuleCallErrorCode) -> u8 {
    match code {
        ModuleCallErrorCode::Trap => 1,
        ModuleCallErrorCode::Timeout => 2,
        ModuleCallErrorCode::OutOfFuel => 3,
        ModuleCallErrorCode::Interrupted => 4,
        ModuleCallErrorCode::OutputTooLarge => 5,
        ModuleCallErrorCode::EffectLimitExceeded => 6,
        ModuleCallErrorCode::EmitLimitExceeded => 7,
        ModuleCallErrorCode::CapsDenied => 8,
        ModuleCallErrorCode::PolicyDenied => 9,
        ModuleCallErrorCode::SandboxUnavailable => 10,
        ModuleCallErrorCode::InvalidOutput => 11,
    }
}
#[cfg(all(
    target_os = "linux",
    any(target_arch = "x86_64", target_arch = "aarch64")
))]
fn decode_failure(code: u8) -> Option<ModuleCallErrorCode> {
    Some(match code {
        1 => ModuleCallErrorCode::Trap,
        2 => ModuleCallErrorCode::Timeout,
        3 => ModuleCallErrorCode::OutOfFuel,
        4 => ModuleCallErrorCode::Interrupted,
        5 => ModuleCallErrorCode::OutputTooLarge,
        6 => ModuleCallErrorCode::EffectLimitExceeded,
        7 => ModuleCallErrorCode::EmitLimitExceeded,
        8 => ModuleCallErrorCode::CapsDenied,
        9 => ModuleCallErrorCode::PolicyDenied,
        10 => ModuleCallErrorCode::SandboxUnavailable,
        11 => ModuleCallErrorCode::InvalidOutput,
        _ => return None,
    })
}
/// A genuine host/resource/protocol failure in this exact invocation. It must
/// poison a formal transaction, not become a committable business failure.
#[derive(Debug)]
pub struct WorkerHostFailureV1 {
    failure: ModuleCallFailure,
}
impl WorkerHostFailureV1 {
    pub fn legacy_failure(&self) -> &ModuleCallFailure {
        &self.failure
    }
}
#[derive(Debug)]
pub enum WorkerExecutionErrorV1 {
    Host(WorkerHostFailureV1),
    Vm(ModuleCallFailure),
}
pub(crate) fn host_failure(failure: ModuleCallFailure) -> WorkerExecutionErrorV1 {
    WorkerExecutionErrorV1::Host(WorkerHostFailureV1 { failure })
}
impl WorkerExecutionErrorV1 {
    fn legacy(self) -> ModuleCallFailure {
        match self {
            Self::Host(host) => host.failure,
            Self::Vm(vm) => vm,
        }
    }
}
impl<A: CanonicalCborAllocator> BoundedWorkerSandboxV1<'_, A> {
    pub fn call_to_typed_v1(
        &mut self,
        request: &ModuleCallRequest,
        sink: &mut dyn ModuleRawOutputSinkV1,
    ) -> Result<(), WorkerExecutionErrorV1> {
        if !matches!(
            self.executable,
            WorkerExecutableV1::SealedFd(_) | WorkerExecutableV1::NativeBoundSealedFd { .. }
        ) {
            return Err(WorkerExecutionErrorV1::Host(WorkerHostFailureV1 {
                failure: failure(
                    request,
                    ModuleCallErrorCode::SandboxUnavailable,
                    "typed worker requires sealed native executable",
                ),
            }));
        }
        self.component_typed(request, sink)
    }
    fn component_typed(
        &mut self,
        request: &ModuleCallRequest,
        sink: &mut dyn ModuleRawOutputSinkV1,
    ) -> Result<(), WorkerExecutionErrorV1> {
        match self.execute_component(request, sink) {
            Ok(None) => Ok(()),
            Ok(Some(vm)) => Err(WorkerExecutionErrorV1::Vm(vm)),
            Err(failure) => Err(WorkerExecutionErrorV1::Host(WorkerHostFailureV1 {
                failure,
            })),
        }
    }
}
impl<A: CanonicalCborAllocator> ModuleSandboxRawOutputV1 for BoundedWorkerSandboxV1<'_, A> {
    fn call_to(
        &mut self,
        request: &ModuleCallRequest,
        sink: &mut dyn ModuleRawOutputSinkV1,
    ) -> Result<(), ModuleCallFailure> {
        self.component_typed(request, sink)
            .map_err(WorkerExecutionErrorV1::legacy)
    }
}
impl<A: CanonicalCborAllocator> ModuleSandbox for BoundedWorkerSandboxV1<'_, A> {
    fn call(&mut self, r: &ModuleCallRequest) -> Result<ModuleOutput, ModuleCallFailure> {
        Err(failure(
            r,
            ModuleCallErrorCode::SandboxUnavailable,
            "bounded worker requires admitted raw output consumer",
        ))
    }
}
impl<A: CanonicalCborAllocator> BoundedWorkerSandboxV1<'_, A> {
    fn execute_component(
        &mut self,
        r: &ModuleCallRequest,
        sink: &mut dyn ModuleRawOutputSinkV1,
    ) -> Result<Option<ModuleCallFailure>, ModuleCallFailure> {
        let fee = self
            .limits
            .address_space_bytes
            .checked_add(self.limits.max_output_bytes)
            .and_then(|n| n.checked_add(128 << 10))
            .and_then(|n| usize::try_from(n).ok())
            .ok_or_else(|| {
                failure(
                    r,
                    ModuleCallErrorCode::SandboxUnavailable,
                    "worker admission overflow",
                )
            })?;
        let _charge = self.allocator.reserve(fee).map_err(|_| {
            failure(
                r,
                ModuleCallErrorCode::SandboxUnavailable,
                "worker pool admission",
            )
        })?;
        if self.cancelled.load(Ordering::Acquire) {
            return Err(failure(
                r,
                ModuleCallErrorCode::Interrupted,
                "worker cancelled",
            ));
        }
        if r.module_id.len() > 256
            || r.trace_id.len() > 256
            || r.entrypoint.len() > 256
            || r.wasm_hash.len() != 64
            || self.limits.address_space_bytes == 0
            || self.limits.max_request_bytes == 0
            || self.limits.max_output_bytes == 0
            || self.limits.wall_time_ms == 0
            || r.limits.max_output_bytes > self.limits.max_output_bytes
        {
            return Err(failure(
                r,
                ModuleCallErrorCode::SandboxUnavailable,
                "worker explicit limits",
            ));
        }
        #[cfg(all(
            target_os = "linux",
            any(target_arch = "x86_64", target_arch = "aarch64")
        ))]
        {
            run(self, r, sink)
        }
        #[cfg(not(all(
            target_os = "linux",
            any(target_arch = "x86_64", target_arch = "aarch64")
        )))]
        {
            let _ = sink;
            Err(failure(
                r,
                ModuleCallErrorCode::SandboxUnavailable,
                "kernel worker limit unsupported on this platform",
            ))
        }
    }
}
#[cfg(all(
    target_os = "linux",
    any(target_arch = "x86_64", target_arch = "aarch64")
))]
fn run<A>(
    host: &BoundedWorkerSandboxV1<'_, A>,
    r: &ModuleCallRequest,
    sink: &mut dyn ModuleRawOutputSinkV1,
) -> Result<Option<ModuleCallFailure>, ModuleCallFailure> {
    if let WorkerExecutableV1::NativeBoundSealedFd { descriptor, .. } = &host.executable {
        if descriptor.is_empty() || descriptor.len() > 1 << 20 {
            return Err(failure(
                r,
                ModuleCallErrorCode::SandboxUnavailable,
                "native-bound descriptor limit",
            ));
        }
        return run_shared(
            host,
            r,
            &native_bound::NativeBoundRequestV1 {
                request: r,
                descriptor,
            },
            sink,
            WorkerProtocolV1 {
                mode: Some("--oasis7-native-bound-worker-v1"),
                request_max: host.limits.max_request_bytes,
                output_max: host.limits.max_output_bytes,
            },
            &mut || false,
        );
    }
    run_shared(
        host,
        r,
        r,
        sink,
        WorkerProtocolV1 {
            mode: None,
            request_max: host.limits.max_request_bytes,
            output_max: host.limits.max_output_bytes,
        },
        &mut || false,
    )
}
#[cfg(all(
    target_os = "linux",
    any(target_arch = "x86_64", target_arch = "aarch64")
))]
struct WorkerProtocolV1<'a> {
    mode: Option<&'a str>,
    request_max: u64,
    output_max: u64,
}
#[cfg(all(
    target_os = "linux",
    any(target_arch = "x86_64", target_arch = "aarch64")
))]
fn run_shared<A, T: serde::Serialize, C: FnMut() -> bool>(
    host: &BoundedWorkerSandboxV1<'_, A>,
    r: &ModuleCallRequest,
    payload: &T,
    sink: &mut dyn ModuleRawOutputSinkV1,
    protocol: WorkerProtocolV1<'_>,
    cancelled: &mut C,
) -> Result<Option<ModuleCallFailure>, ModuleCallFailure> {
    let WorkerProtocolV1 {
        mode,
        request_max,
        output_max,
    } = protocol;
    use std::{
        fs::{DirBuilder, File, OpenOptions},
        os::unix::{
            fs::{DirBuilderExt, OpenOptionsExt},
            process::CommandExt,
        },
        path::Path,
        process::{Command, Stdio},
        time::{Duration, Instant},
    };
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    struct Directory(std::path::PathBuf);
    impl Drop for Directory {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let directory = Directory(std::env::temp_dir().join(format!(
        "oasis7-worker-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    )));
    DirBuilder::new()
        .mode(0o700)
        .create(&directory.0)
        .map_err(|_| {
            failure(
                r,
                ModuleCallErrorCode::SandboxUnavailable,
                "worker private directory",
            )
        })?;
    let request_path = directory.0.join("request");
    let result_path = directory.0.join("result");
    let open = |p: &Path| {
        OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(p)
    };
    let input = open(&request_path).map_err(|_| {
        failure(
            r,
            ModuleCallErrorCode::SandboxUnavailable,
            "worker request file",
        )
    })?;
    let output = open(&result_path).map_err(|_| {
        failure(
            r,
            ModuleCallErrorCode::SandboxUnavailable,
            "worker result file",
        )
    })?;
    struct Bounded<'a, C> {
        file: File,
        left: u64,
        cancelled: &'a AtomicBool,
        extra: &'a mut C,
    }
    impl<C: FnMut() -> bool> Write for Bounded<'_, C> {
        fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
            if self.cancelled.load(Ordering::Acquire) || (self.extra)() {
                return Err(std::io::Error::other("cancelled"));
            }
            let b = &b[..b.len().min(64 << 10)];
            self.left = self
                .left
                .checked_sub(b.len() as u64)
                .ok_or_else(|| std::io::Error::other("request bound"))?;
            self.file.write_all(b)?;
            Ok(b.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            self.file.flush()
        }
    }
    let mut writer = Bounded {
        file: input,
        left: request_max,
        cancelled: host.cancelled,
        extra: cancelled,
    };
    serde_cbor::to_writer(&mut writer, payload).map_err(|_| {
        failure(
            r,
            ModuleCallErrorCode::SandboxUnavailable,
            "worker request bounded encoding",
        )
    })?;
    writer.flush().map_err(|_| {
        failure(
            r,
            ModuleCallErrorCode::SandboxUnavailable,
            "worker request flush",
        )
    })?;
    drop(writer);
    let program = executable::path(&host.executable).map_err(|_| {
        failure(
            r,
            ModuleCallErrorCode::SandboxUnavailable,
            "worker executable kernel seals",
        )
    })?;
    let mut command = Command::new(program);
    command
        .arg(if mode == Some("--oasis7-wasm-worker-handshake-v1") {
            "--oasis7-wasm-worker-handshake-v1"
        } else if mode == Some("--oasis7-native-bound-worker-v1") {
            "--oasis7-native-bound-worker-v1"
        } else if mode == Some("--oasis7-native-bound-regex-worker-v1") {
            "--oasis7-native-bound-regex-worker-v1"
        } else {
            "--oasis7-wasm-worker-v1"
        })
        .arg(request_max.to_string())
        .arg(output_max.to_string())
        .arg(host.limits.address_space_bytes.to_string());
    if let Some(mode) = mode.filter(|mode| {
        *mode != "--oasis7-wasm-worker-handshake-v1"
            && *mode != "--oasis7-native-bound-worker-v1"
            && *mode != "--oasis7-native-bound-regex-worker-v1"
    }) {
        command.arg(mode);
    }
    command
        .stdin(
            OpenOptions::new()
                .read(true)
                .custom_flags(0x20000)
                .open(&request_path)
                .map_err(|_| {
                    failure(
                        r,
                        ModuleCallErrorCode::SandboxUnavailable,
                        "worker input reopen",
                    )
                })?,
        )
        .stdout(output)
        .stderr(Stdio::null())
        .env_clear();
    let cap = host.limits.address_space_bytes;
    #[repr(C)]
    struct Limit {
        soft: u64,
        hard: u64,
    }
    unsafe extern "C" {
        fn setrlimit(resource: i32, limit: *const Limit) -> i32;
        fn prctl(option: i32, arg2: usize, arg3: usize, arg4: usize, arg5: usize) -> i32;
        fn getppid() -> i32;
    }
    let parent_pid = std::process::id() as i32;
    // SAFETY: after fork this closure makes only direct Linux resource/process syscalls;
    // no allocator, lock, diagnostic formatting or Wasmtime runs before exec.
    unsafe {
        command.pre_exec(move || {
            // Kill an orphan even if the host dies without running Rust Drop.
            // Recheck the parent to close death-before-prctl registration.
            if prctl(1, 9, 0, 0, 0) != 0 || getppid() != parent_pid {
                return Err(std::io::Error::from_raw_os_error(10));
            }
            let limit = Limit {
                soft: cap,
                hard: cap,
            };
            if setrlimit(9, &limit) != 0 {
                return Err(std::io::Error::last_os_error());
            }
            let core = Limit { soft: 0, hard: 0 };
            if setrlimit(4, &core) != 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    struct Process(std::process::Child);
    impl Drop for Process {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    let mut process = Process(command.spawn().map_err(|_| {
        failure(
            r,
            ModuleCallErrorCode::SandboxUnavailable,
            "limited worker spawn",
        )
    })?);
    let start = Instant::now();
    let status = loop {
        if host.cancelled.load(Ordering::Acquire) || cancelled() {
            return Err(failure(
                r,
                ModuleCallErrorCode::Interrupted,
                "worker cancelled",
            ));
        }
        if start.elapsed() >= Duration::from_millis(host.limits.wall_time_ms) {
            return Err(failure(
                r,
                ModuleCallErrorCode::Timeout,
                "worker wall deadline",
            ));
        }
        if let Some(status) = process
            .0
            .try_wait()
            .map_err(|_| failure(r, ModuleCallErrorCode::SandboxUnavailable, "worker wait"))?
        {
            break status;
        }
        std::thread::sleep(Duration::from_millis(2));
    };
    if !status.success() {
        return Err(failure(
            r,
            ModuleCallErrorCode::SandboxUnavailable,
            "worker failed within hard limit",
        ));
    }
    let mut result = OpenOptions::new()
        .read(true)
        .custom_flags(0x20000)
        .open(&result_path)
        .map_err(|_| failure(r, ModuleCallErrorCode::InvalidOutput, "worker result"))?;
    let metadata = result.metadata().map_err(|_| {
        failure(
            r,
            ModuleCallErrorCode::InvalidOutput,
            "worker result metadata",
        )
    })?;
    if !metadata.is_file()
        || metadata.len() < 9
        || metadata.len() > output_max.max(4096).saturating_add(9)
    {
        return Err(failure(
            r,
            ModuleCallErrorCode::InvalidOutput,
            "worker result file bound",
        ));
    }
    let mut header = [0; 9];
    result.read_exact(&mut header).map_err(|_| {
        failure(
            r,
            ModuleCallErrorCode::InvalidOutput,
            "worker truncated result",
        )
    })?;
    let length = u64::from_be_bytes(header[1..].try_into().unwrap());
    if length > if header[0] == 0 { output_max } else { 4096 }
        || result
            .metadata()
            .map_err(|_| {
                failure(
                    r,
                    ModuleCallErrorCode::InvalidOutput,
                    "worker result metadata",
                )
            })?
            .len()
            != length + 9
    {
        return Err(failure(
            r,
            ModuleCallErrorCode::InvalidOutput,
            "worker rejected or invalid result",
        ));
    }
    if host.cancelled.load(Ordering::Acquire) || cancelled() {
        return Err(failure(
            r,
            ModuleCallErrorCode::Interrupted,
            "worker final cancellation",
        ));
    }
    if header[0] != 0 {
        let code = decode_failure(header[0]).ok_or_else(|| {
            failure(
                r,
                ModuleCallErrorCode::InvalidOutput,
                "worker unknown failure",
            )
        })?;
        let mut detail = vec![0; length as usize];
        result.read_exact(&mut detail).map_err(|_| {
            failure(
                r,
                ModuleCallErrorCode::InvalidOutput,
                "worker failure frame read",
            )
        })?;
        let detail = std::str::from_utf8(&detail)
            .map_err(|_| failure(r, ModuleCallErrorCode::InvalidOutput, "worker failure UTF8"))?
            .to_owned();
        if host.cancelled.load(Ordering::Acquire) || cancelled() {
            return Err(failure(
                r,
                ModuleCallErrorCode::Interrupted,
                "worker final cancellation",
            ));
        }
        return Ok(Some(ModuleCallFailure {
            module_id: short(&r.module_id),
            trace_id: short(&r.trace_id),
            code,
            detail,
        }));
    }
    let mut bytes = vec![
        0;
        usize::try_from(length).map_err(|_| failure(
            r,
            ModuleCallErrorCode::OutputTooLarge,
            "worker output size"
        ))?
    ];
    for part in bytes.chunks_mut(64 << 10) {
        if host.cancelled.load(Ordering::Acquire) || cancelled() {
            return Err(failure(
                r,
                ModuleCallErrorCode::Interrupted,
                "worker result cancellation",
            ));
        }
        result
            .read_exact(part)
            .map_err(|_| failure(r, ModuleCallErrorCode::InvalidOutput, "worker result read"))?;
    }
    if host.cancelled.load(Ordering::Acquire) || cancelled() {
        return Err(failure(
            r,
            ModuleCallErrorCode::Interrupted,
            "worker final cancellation",
        ));
    }
    sink.consume_output(&bytes).map(|_| None)
}
