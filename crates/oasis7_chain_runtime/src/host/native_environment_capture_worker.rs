//! Same-child native binding, reusing the ordinary original capture algorithm.
use super::*;
use oasis7_wasm_executor::bounded_worker::NativeEnvironmentVerifierV1;
use std::fs::{File, Metadata};
#[derive(Clone, Copy)]
pub(super) enum CaptureModeV1 {
    Ordinary,
    BoundWorker,
}
pub(super) struct CapturedNativeEnvironmentV1 {
    pub(super) descriptor: NativeExecutionEnvironmentV1,
    executable: File,
    executable_before: Metadata,
    libraries: Vec<(String, File, Metadata)>,
    inventory: Vec<linux::Mapping>,
}
impl CapturedNativeEnvironmentV1 {
    fn validate_descriptor(
        &self,
        expected: &NativeExecutionEnvironmentV1,
    ) -> Result<(), &'static str> {
        if &self.descriptor != expected {
            return Err("native-bound environment mismatch");
        }
        Ok(())
    }
    fn validate_pins(&self, mode: CaptureModeV1) -> Result<(), String> {
        crate::executable_image::validate_loaded_file(&self.executable)?;
        let inventory = match mode {
            CaptureModeV1::Ordinary => linux::inventory()?,
            CaptureModeV1::BoundWorker => {
                linux::inventory_for_loaded_executable(&self.executable_before)?
            }
        };
        if !crate::executable_image::unchanged(
            &self.executable_before,
            &self.executable.metadata().map_err(|e| e.to_string())?,
        ) || self.inventory != inventory
        {
            return Err("Unavailable: loaded native environment changed during capture".into());
        }
        for (_, file, before) in &self.libraries {
            if !crate::executable_image::unchanged(
                before,
                &file.metadata().map_err(|e| e.to_string())?,
            ) {
                return Err("Unavailable: mapped dependency changed during capture".into());
            }
        }
        Ok(())
    }
}
/// The single original capture implementation serves both ordinary Frozen
/// capture and the bound child's pre/post checks; original component wire,
/// chunk/hash algorithm, build inputs and stability rules remain shared.
pub(super) fn capture_pinned_v1(
    mode: CaptureModeV1,
    mut sink: impl FnMut(&str, &[u8]) -> Result<(), String>,
) -> Result<CapturedNativeEnvironmentV1, String> {
    let mut total_bytes = 0u64;
    let mut bounded_sink = |digest: &str, bytes: &[u8]| {
        total_bytes = total_bytes
            .checked_add(bytes.len() as u64)
            .ok_or("native total length overflow")?;
        if total_bytes > 4 * 1024 * 1024 * 1024u64 {
            return Err("native environment total byte budget".to_string());
        }
        sink(digest, bytes)
    };
    let mut executable = crate::executable_image::open_loaded_image()?;
    let before = executable.metadata().map_err(|e| e.to_string())?;
    let inventory = match mode {
        CaptureModeV1::Ordinary => linux::inventory()?,
        CaptureModeV1::BoundWorker => {
            use std::os::fd::AsRawFd;
            // SAFETY: interrogates the owned descriptor of the actual loaded
            // image. An unsupported command must fail, never pass a bit mask.
            let seals = unsafe { libc::fcntl(executable.as_raw_fd(), 1034) };
            if seals < 0 || seals & 15 != 15 {
                return Err("native bound image requires immutable seals".into());
            }
            linux::inventory_for_loaded_executable(&before)?
        }
    };
    let mut libraries = linux::pin(&inventory, &before)?;
    let mut components = vec![component(
        NativeEnvironmentRole::Executable,
        "loaded-executable",
        &mut executable,
        &mut bounded_sink,
    )?];
    for (name, file, _) in &mut libraries {
        components.push(component(
            NativeEnvironmentRole::LoadedLibrary,
            name,
            file,
            &mut bounded_sink,
        )?);
    }
    for (name, bytes) in [
        (
            "build/metadata",
            include_bytes!(concat!(env!("OUT_DIR"), "/native-build-metadata.txt")).as_slice(),
        ),
        (
            "build/toolchain",
            include_bytes!(concat!(env!("OUT_DIR"), "/native-toolchain.txt")).as_slice(),
        ),
        (
            "build/Cargo.lock",
            include_bytes!(concat!(env!("OUT_DIR"), "/native-Cargo.lock")).as_slice(),
        ),
    ] {
        components.push(component(
            NativeEnvironmentRole::BuildMetadata,
            name,
            &mut &bytes[..],
            &mut bounded_sink,
        )?);
    }
    let descriptor = NativeExecutionEnvironmentV1 {
        schema_version: 1,
        target: env!("NATIVE_BUILD_TARGET").into(),
        os: env!("NATIVE_BUILD_CARGO_CFG_TARGET_OS").into(),
        arch: env!("NATIVE_BUILD_CARGO_CFG_TARGET_ARCH").into(),
        components,
    };
    descriptor.validate()?;
    let captured = CapturedNativeEnvironmentV1 {
        descriptor,
        executable,
        executable_before: before,
        libraries,
        inventory,
    };
    captured.validate_pins(mode)?;
    Ok(captured)
}
pub(super) struct NativeInvocationGuardV1 {
    expected: NativeExecutionEnvironmentV1,
    pins: CapturedNativeEnvironmentV1,
}
pub(super) struct ActualNativeVerifierV1 {
    bytes_remaining: u64,
    captures_remaining: u8,
}
impl ActualNativeVerifierV1 {
    pub(super) fn new() -> Self {
        Self {
            bytes_remaining: 0,
            captures_remaining: 2,
        }
    }
    fn capture(&mut self) -> Result<CapturedNativeEnvironmentV1, String> {
        self.captures_remaining = self
            .captures_remaining
            .checked_sub(1)
            .ok_or("native invocation capture allowance")?;
        capture_pinned_v1(CaptureModeV1::BoundWorker, |_, bytes| {
            self.bytes_remaining = self
                .bytes_remaining
                .checked_sub(bytes.len() as u64)
                .ok_or("native invocation byte allowance")?;
            Ok(())
        })
    }
}
impl NativeEnvironmentVerifierV1 for ActualNativeVerifierV1 {
    type Guard = NativeInvocationGuardV1;
    fn before(&mut self, descriptor: &[u8]) -> Result<Self::Guard, &'static str> {
        let expected = NativeExecutionEnvironmentV1::decode(descriptor)
            .map_err(|_| "native-bound original descriptor")?;
        let total = expected
            .components
            .iter()
            .try_fold(0u64, |total, component| {
                total.checked_add(component.byte_len)
            })
            .ok_or("native-bound descriptor byte overflow")?;
        self.bytes_remaining = total
            .checked_mul(2)
            .ok_or("native-bound capture byte allowance")?;
        let pins = self
            .capture()
            .map_err(|_| "native-bound actual pre-call capture")?;
        pins.validate_descriptor(&expected)?;
        Ok(NativeInvocationGuardV1 { expected, pins })
    }
    fn after(&mut self, _: &[u8], guard: &Self::Guard) -> Result<(), &'static str> {
        guard
            .pins
            .validate_pins(CaptureModeV1::BoundWorker)
            .map_err(|_| "native-bound execution mapping/dependency changed")?;
        let actual = self
            .capture()
            .map_err(|_| "native-bound actual post-call capture")?;
        actual.validate_descriptor(&guard.expected)?;
        if self.bytes_remaining != 0 {
            return Err("native-bound post-call environment mismatch");
        }
        Ok(())
    }
}
pub(crate) fn worker_main_v1() -> Result<(), &'static str> {
    oasis7_wasm_executor::bounded_worker::worker_main_with_native_verifier_v1(
        &mut ActualNativeVerifierV1::new(),
    )
}
#[cfg(test)]
#[path = "native_environment_capture_worker_tests.rs"]
mod tests;
