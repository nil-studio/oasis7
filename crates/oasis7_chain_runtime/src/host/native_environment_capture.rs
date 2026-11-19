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
        Ok(worker::capture_pinned_v1(worker::CaptureModeV1::Ordinary, &mut sink)?.descriptor)
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

#[cfg(target_os = "linux")]
#[path = "native_environment_capture_worker.rs"]
mod worker;
#[cfg(target_os = "linux")]
pub(crate) use worker::worker_main_v1 as native_bound_worker_main_v1;

#[cfg(all(feature = "native-worker-test-support", target_os = "linux"))]
#[path = "native_environment_worker_test_support.rs"]
mod fixture_support;
#[cfg(all(feature = "native-worker-test-support", target_os = "linux"))]
pub(crate) use fixture_support::{fixture_capture_v1, fixture_worker_main_v1};
