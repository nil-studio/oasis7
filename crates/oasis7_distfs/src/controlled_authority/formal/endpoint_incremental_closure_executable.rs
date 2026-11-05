//! An immutable in-process executable reconstructed from actual authenticated
//! native originals. This is not a business/ABI permission or durable alias.
use super::*;
#[cfg(target_os = "linux")]
use std::{
    io::Write,
    os::fd::{AsRawFd, FromRawFd},
};
/// No public constructor/Clone. The owning static baseline also retains the
/// actual native namespace pin. Borrowing this file cannot outlive that owner.
pub(crate) struct VerifiedNativeExecutableV1 {
    file: std::fs::File,
    digest: [u8; 32],
    bytes: u64,
}
impl VerifiedNativeExecutableV1 {
    pub(crate) fn file(&self) -> &std::fs::File {
        &self.file
    }
    pub(crate) fn digest(&self) -> [u8; 32] {
        self.digest
    }
    pub(crate) fn byte_len(&self) -> u64 {
        self.bytes
    }
}
/// Only the actual full native consumer calls this after its true Core-bound
/// native proof. The same exact bridge independently authenticates each chunk.
#[cfg(target_os = "linux")]
pub(super) fn prepare<O: OriginalObjectReader, C: FnMut() -> bool>(
    native: &ReservedValue<CheckedNativeEnvironment>,
    descriptor: &oasis7_formal_protocol::native_environment::NativeExecutionEnvironmentV1,
    bridge: &mut native_reader::NativeChunkBridgeV1<'_, O, C>,
    memory: &MemoryBudget,
) -> Result<Option<ReservedValue<VerifiedNativeExecutableV1>>, FormalError> {
    use oasis7_formal_protocol::native_environment::NativeEnvironmentRole;
    if !native.uses_budget(memory)
        || native.reserved_bytes() < std::mem::size_of::<CheckedNativeEnvironment>()
    {
        return Err(invalid("sealed native executable proof admission"));
    }
    let component = descriptor
        .components
        .iter()
        .find(|c| c.role == NativeEnvironmentRole::Executable)
        .ok_or_else(|| invalid("sealed native executable component"))?;
    let bytes = usize::try_from(component.byte_len)
        .map_err(|_| invalid("sealed native executable size"))?;
    let fee = bytes
        .checked_add(4095)
        .map(|n| n & !4095)
        .and_then(|n| n.checked_add(std::mem::size_of::<VerifiedNativeExecutableV1>() + 4096))
        .ok_or_else(|| invalid("sealed native executable admission overflow"))?;
    let charge = memory
        .reserve(fee, MemoryClass::Ordinary)
        .map_err(|_| invalid("sealed native executable admission"))?;
    unsafe extern "C" {
        fn memfd_create(name: *const std::ffi::c_char, flags: u32) -> i32;
        fn fcntl(fd: i32, command: i32, ...) -> i32;
    }
    // SAFETY: static nul-terminated name; success transfers one owned descriptor.
    let fd = unsafe { memfd_create(c"oasis7-native-worker".as_ptr(), 3) };
    if fd < 0 {
        return Err(invalid("sealed native executable memfd unavailable"));
    }
    let mut file = unsafe { std::fs::File::from_raw_fd(fd) };
    let mut hash = blake3::Hasher::new();
    let mut length = 0u64;
    for chunk in &component.chunks {
        if (bridge.cancelled)() {
            return Err(invalid("sealed native executable cancelled"));
        }
        let mut id = [0; 32];
        hex::decode_to_slice(&chunk.digest, &mut id)
            .map_err(|_| invalid("sealed native chunk ID"))?;
        let data = bridge
            .read_original(&id, chunk.byte_len as usize, memory)
            .map_err(|_| invalid("sealed native actual chunk"))?;
        if data.len() != chunk.byte_len as usize {
            return Err(invalid("sealed native chunk length"));
        }
        length = length
            .checked_add(data.len() as u64)
            .filter(|n| *n <= component.byte_len)
            .ok_or_else(|| invalid("sealed native executable length"))?;
        for part in data.chunks(64 << 10) {
            if (bridge.cancelled)() {
                return Err(invalid("sealed native executable cancelled"));
            }
            hash.update(part);
            file.write_all(part)
                .map_err(|_| invalid("sealed native executable write"))?;
        }
    }
    let digest = *hash.finalize().as_bytes();
    if length != component.byte_len || digest != native.executable_id() {
        return Err(invalid("sealed native executable original binding"));
    }
    // No writable mapping or descriptor is exposed before sealing. All four
    // seals are permanent and checked; no replaceable filesystem path is used.
    if unsafe { fcntl(file.as_raw_fd(), 1033, 15) } != 0
        || unsafe { fcntl(file.as_raw_fd(), 1034) } & 15 != 15
    {
        return Err(invalid("sealed native executable kernel seals"));
    }
    if (bridge.cancelled)() {
        return Err(invalid("sealed native executable final cancellation"));
    }
    Ok(Some(charge.retain(VerifiedNativeExecutableV1 {
        file,
        digest,
        bytes: length,
    })))
}
#[cfg(not(target_os = "linux"))]
pub(super) fn prepare<O: OriginalObjectReader, C: FnMut() -> bool>(
    _native: &ReservedValue<CheckedNativeEnvironment>,
    _descriptor: &oasis7_formal_protocol::native_environment::NativeExecutionEnvironmentV1,
    _bridge: &mut native_reader::NativeChunkBridgeV1<'_, O, C>,
    _memory: &MemoryBudget,
) -> Result<Option<ReservedValue<VerifiedNativeExecutableV1>>, FormalError> {
    // Source-content qualification is supported here; actual bounded execution
    // is explicitly unavailable. A driver must require the Some handle.
    Ok(None)
}
