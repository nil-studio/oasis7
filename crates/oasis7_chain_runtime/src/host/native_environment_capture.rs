//! Capture original native bytes. A descriptor is returned only for a complete supported closure.
use oasis7_formal_protocol::native_environment::NativeExecutionEnvironmentV1;
#[cfg(any(target_os = "linux", test))]
use oasis7_formal_protocol::native_environment::{
    MAX_NATIVE_CHUNK_BYTES, NativeEnvironmentChunkV1, NativeEnvironmentComponentV1,
    NativeEnvironmentRole,
};
#[cfg(any(target_os = "linux", test))]
use std::io::Read;

pub(crate) fn capture(
    mut sink: impl FnMut(&str, &[u8]) -> Result<(), String>,
) -> Result<NativeExecutionEnvironmentV1, String> {
    #[cfg(target_os = "linux")]
    {
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
        let inventory = linux::inventory()?;
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
        crate::executable_image::validate_loaded_file(&executable)?;
        if !crate::executable_image::unchanged(
            &before,
            &executable.metadata().map_err(|e| e.to_string())?,
        ) || inventory != linux::inventory()?
        {
            return Err("Unavailable: loaded native environment changed during capture".into());
        }
        for (_, file, before) in &libraries {
            if !crate::executable_image::unchanged(
                before,
                &file.metadata().map_err(|e| e.to_string())?,
            ) {
                return Err("Unavailable: mapped dependency changed during capture".into());
            }
        }
        let descriptor = NativeExecutionEnvironmentV1 {
            schema_version: 1,
            target: env!("NATIVE_BUILD_TARGET").into(),
            os: env!("NATIVE_BUILD_CARGO_CFG_TARGET_OS").into(),
            arch: env!("NATIVE_BUILD_CARGO_CFG_TARGET_ARCH").into(),
            components,
        };
        descriptor.validate()?;
        Ok(descriptor)
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = &mut sink;
        Err("Unavailable: original loaded-library bytes are unsupported on this platform (including macOS shared-cache images)".into())
    }
}

#[cfg(any(target_os = "linux", test))]
fn component(
    role: NativeEnvironmentRole,
    name: &str,
    reader: &mut impl Read,
    sink: &mut impl FnMut(&str, &[u8]) -> Result<(), String>,
) -> Result<NativeEnvironmentComponentV1, String> {
    let mut buffer = [0u8; 64 * 1024];
    let mut chunk = Vec::with_capacity(MAX_NATIVE_CHUNK_BYTES);
    let mut chunks = Vec::new();
    let mut hash = blake3::Hasher::new();
    let mut byte_len = 0u64;
    loop {
        let remaining = (MAX_NATIVE_CHUNK_BYTES - chunk.len()).min(buffer.len());
        let count = reader
            .read(&mut buffer[..remaining])
            .map_err(|e| format!("native component {name}: {e}"))?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
        byte_len = byte_len
            .checked_add(count as u64)
            .ok_or("native component length overflow")?;
        if byte_len > 4 * 1024 * 1024 * 1024u64 {
            return Err("native component exceeds byte budget".into());
        }
        chunk.extend_from_slice(&buffer[..count]);
        if chunk.len() == MAX_NATIVE_CHUNK_BYTES {
            publish(&mut chunk, &mut chunks, sink)?;
        }
    }
    if !chunk.is_empty() {
        publish(&mut chunk, &mut chunks, sink)?;
    }
    if chunks.is_empty() {
        return Err(format!("native component {name} is empty"));
    }
    Ok(NativeEnvironmentComponentV1 {
        role,
        name: name.into(),
        byte_len,
        content_digest: hash.finalize().to_hex().to_string(),
        chunks,
    })
}
#[cfg(any(target_os = "linux", test))]
fn publish(
    bytes: &mut Vec<u8>,
    chunks: &mut Vec<NativeEnvironmentChunkV1>,
    sink: &mut impl FnMut(&str, &[u8]) -> Result<(), String>,
) -> Result<(), String> {
    if chunks.len() >= 4096 {
        return Err("native component chunk budget".into());
    }
    let digest = blake3::hash(bytes).to_hex().to_string();
    sink(&digest, bytes)?;
    chunks.push(NativeEnvironmentChunkV1 {
        digest,
        byte_len: bytes.len() as u32,
    });
    bytes.clear();
    Ok(())
}
#[cfg(target_os = "linux")]
#[path = "native_environment_capture_linux.rs"]
mod linux;
#[cfg(test)]
#[path = "native_environment_capture_tests.rs"]
mod tests;
