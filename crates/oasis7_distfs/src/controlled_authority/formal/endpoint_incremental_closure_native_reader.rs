//! Exact-length bridge to the already registered profile-two native chunks.
//! A corrupt physical selector is an error, never a reason to try legacy bytes.
use super::*;
use oasis7_formal_protocol::{
    incremental_material::{OriginalObjectReader, OriginalObjectStreamReader},
    native_environment::{MAX_NATIVE_CHUNK_BYTES, NativeExecutionEnvironmentV1},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
use std::{
    fmt,
    io::{self, Write},
    path::Path,
    sync::Arc,
};
/// Existing physical pack metadata is bounded by 16MiB encoded and 65536
/// entries in each of two inventories. This independent fixed phase covers
/// both encoded metadata, <=512B per decoded inventory entry including owned
/// hash text/allocator overhead, bounded batch/frame scratch and split buffers.
/// It is not an input-dependent multiplier or a change to pack format limits.
const PACK_WORKSPACE: usize = 128 << 20;
pub(crate) struct NativeChunkReadWorkV1 {
    pub(crate) occurrences_remaining: u64,
    pub(crate) bytes_remaining: u64,
    pub(crate) frozen_wire_bytes_remaining: u64,
}
#[derive(Debug)]
pub(crate) enum NativeChunkReadErrorV1<E> {
    Original(E),
    Storage(FormalError),
    Memory(oasis7_proto::memory_budget::MemoryLimitExceeded),
    Invalid(&'static str),
    Cancelled,
    Io(io::Error),
}
impl<E: fmt::Display> fmt::Display for NativeChunkReadErrorV1<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Original(error) => write!(f, "native original read failed: {error}"),
            Self::Storage(error) => write!(f, "native durable storage read failed: {error}"),
            Self::Memory(error) => write!(f, "native chunk memory admission failed: {error}"),
            Self::Invalid(reason) => write!(f, "invalid native chunk read: {reason}"),
            Self::Cancelled => f.write_str("native chunk read cancelled"),
            Self::Io(error) => write!(f, "native chunk output failed: {error}"),
        }
    }
}
impl<E: std::error::Error + 'static> std::error::Error for NativeChunkReadErrorV1<E> {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Original(error) => Some(error),
            Self::Storage(error) => Some(error),
            Self::Memory(error) => Some(error),
            Self::Io(error) => Some(error),
            Self::Invalid(_) | Self::Cancelled => None,
        }
    }
}
/// The owner holds the real native physical pin throughout this bridge's life.
pub(crate) struct NativeChunkBridgeV1<'a, O, C> {
    pub(crate) originals: &'a mut O,
    pub(crate) descriptor: &'a NativeExecutionEnvironmentV1,
    pub(crate) descriptor_id: [u8; 32],
    pub(crate) descriptor_bytes: &'a [u8],
    pub(crate) directory: &'a Path,
    pub(crate) memory: &'a MemoryBudget,
    pub(crate) work: &'a mut NativeChunkReadWorkV1,
    pub(crate) cancelled: C,
}
impl<O: OriginalObjectReader, C: FnMut() -> bool> NativeChunkBridgeV1<'_, O, C> {
    fn declared(&self, id: &[u8; 32]) -> Option<usize> {
        let mut text = [0; 64];
        hex::encode_to_slice(id, &mut text).ok()?;
        self.descriptor
            .components
            .iter()
            .flat_map(|c| &c.chunks)
            .find(|c| c.digest.as_bytes() == text)
            .map(|c| c.byte_len as usize)
    }
}
impl<O: OriginalObjectReader, C: FnMut() -> bool> OriginalObjectReader
    for NativeChunkBridgeV1<'_, O, C>
{
    type Error = NativeChunkReadErrorV1<O::Error>;
    fn allocation_budget(&self) -> &MemoryBudget {
        self.memory
    }
    fn allocation_class(&self) -> MemoryClass {
        MemoryClass::Ordinary
    }
    fn read_original(
        &mut self,
        id: &[u8; 32],
        max_bytes: usize,
        memory: &MemoryBudget,
    ) -> Result<Arc<ReservedValue<Vec<u8>>>, Self::Error> {
        if (self.cancelled)() {
            return Err(NativeChunkReadErrorV1::Cancelled);
        }
        if !memory.same_pool(self.memory)
            || !self.originals.allocation_budget().same_pool(memory)
            || self.originals.allocation_class() != MemoryClass::Ordinary
        {
            return Err(NativeChunkReadErrorV1::Invalid("native bridge pool/class"));
        }
        if id == &self.descriptor_id {
            let raw = self.descriptor_bytes;
            if raw.len() > max_bytes
                || raw.len()
                    > oasis7_formal_protocol::native_environment::MAX_NATIVE_DESCRIPTOR_BYTES
            {
                return Err(NativeChunkReadErrorV1::Invalid("native descriptor cap"));
            }
            self.work.occurrences_remaining =
                self.work.occurrences_remaining.checked_sub(1).ok_or(
                    NativeChunkReadErrorV1::Invalid("native descriptor occurrence"),
                )?;
            self.work.bytes_remaining = self
                .work
                .bytes_remaining
                .checked_sub(raw.len() as u64)
                .ok_or(NativeChunkReadErrorV1::Invalid("native descriptor bytes"))?;
            let charge = memory
                .reserve(
                    raw.len()
                        .checked_add(256)
                        .ok_or(NativeChunkReadErrorV1::Invalid("native descriptor fee"))?,
                    MemoryClass::Ordinary,
                )
                .map_err(NativeChunkReadErrorV1::Memory)?;
            let mut bytes = Vec::with_capacity(raw.len());
            let mut hash = blake3::Hasher::new();
            for part in raw.chunks(64 << 10) {
                if (self.cancelled)() {
                    return Err(NativeChunkReadErrorV1::Cancelled);
                }
                hash.update(part);
                bytes.extend_from_slice(part);
            }
            if hash.finalize().as_bytes() != id {
                return Err(NativeChunkReadErrorV1::Invalid(
                    "native descriptor actual bytes",
                ));
            }
            if (self.cancelled)() {
                return Err(NativeChunkReadErrorV1::Cancelled);
            }
            return Ok(Arc::new(charge.retain(bytes)));
        }
        let Some(expected) = self.declared(id) else {
            return self
                .originals
                .read_original(id, max_bytes, memory)
                .map_err(NativeChunkReadErrorV1::Original);
        };
        if expected == 0 || expected > MAX_NATIVE_CHUNK_BYTES || expected > max_bytes {
            return Err(NativeChunkReadErrorV1::Invalid("native chunk expected cap"));
        }
        self.work.occurrences_remaining = self.work.occurrences_remaining.checked_sub(1).ok_or(
            NativeChunkReadErrorV1::Invalid("native bridge occurrence allowance"),
        )?;
        self.work.bytes_remaining = self
            .work
            .bytes_remaining
            .checked_sub(expected as u64)
            .ok_or(NativeChunkReadErrorV1::Invalid(
                "native bridge byte allowance",
            ))?;
        let output = memory
            .reserve(
                expected
                    .checked_add(256)
                    .ok_or(NativeChunkReadErrorV1::Invalid(
                        "native output fee overflow",
                    ))?,
                MemoryClass::Ordinary,
            )
            .map_err(NativeChunkReadErrorV1::Memory)?;
        let _phase = memory
            .reserve(PACK_WORKSPACE, MemoryClass::Ordinary)
            .map_err(NativeChunkReadErrorV1::Memory)?;
        let text = hex::encode(id);
        let path = crate::controlled_authority::formal::physical::selected_path_uncached(
            &self.directory.join(&text),
            &text,
        )
        .map_err(NativeChunkReadErrorV1::Storage)?;
        let requests = [(path.clone(), text.clone(), expected)];
        let mut physical =
            crate::controlled_authority::formal::physical::read_durable_batch_uncached(
                self.directory,
                &requests,
            )
            .map_err(NativeChunkReadErrorV1::Storage)?;
        if physical.len() != 1 {
            return Err(NativeChunkReadErrorV1::Invalid(
                "native exact batch coverage",
            ));
        }
        let bytes = match physical.pop().flatten() {
            Some(bytes) => bytes,
            None => {
                crate::controlled_authority::formal::business_pack::read_legacy_durable_expected(
                    &path, &text, expected,
                )
                .map_err(NativeChunkReadErrorV1::Storage)?
                .ok_or(NativeChunkReadErrorV1::Invalid(
                    "native durable chunk missing",
                ))?
            }
        };
        if bytes.len() != expected
            || bytes.capacity() > output.bytes()
            || blake3::hash(&bytes).as_bytes() != id
        {
            return Err(NativeChunkReadErrorV1::Invalid(
                "native exact bytes/length/hash",
            ));
        }
        if (self.cancelled)() {
            return Err(NativeChunkReadErrorV1::Cancelled);
        }
        Ok(Arc::new(output.retain(bytes)))
    }
}
impl<O: OriginalObjectStreamReader, C: FnMut() -> bool> OriginalObjectStreamReader
    for NativeChunkBridgeV1<'_, O, C>
{
    fn stream_original(
        &mut self,
        id: &[u8; 32],
        max_bytes: u64,
        memory: &MemoryBudget,
        output: &mut dyn Write,
    ) -> Result<u64, Self::Error> {
        if id != &self.descriptor_id && self.declared(id).is_none() {
            return self
                .originals
                .stream_original(id, max_bytes, memory, output)
                .map_err(NativeChunkReadErrorV1::Original);
        }
        let max = usize::try_from(max_bytes)
            .map_err(|_| NativeChunkReadErrorV1::Invalid("native stream length"))?;
        let bytes = self.read_original(id, max, memory)?;
        for chunk in bytes.chunks(64 << 10) {
            if (self.cancelled)() {
                return Err(NativeChunkReadErrorV1::Cancelled);
            }
            output
                .write_all(chunk)
                .map_err(NativeChunkReadErrorV1::Io)?;
        }
        if (self.cancelled)() {
            return Err(NativeChunkReadErrorV1::Cancelled);
        }
        Ok(bytes.len() as u64)
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use oasis7_formal_protocol::native_environment::{
        NativeEnvironmentChunkV1, NativeEnvironmentComponentV1, NativeEnvironmentRole,
    };
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    struct Directory(std::path::PathBuf);
    impl Drop for Directory {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(self.0.parent().unwrap());
        }
    }
    struct Empty(MemoryBudget);
    impl OriginalObjectReader for Empty {
        type Error = &'static str;
        fn allocation_budget(&self) -> &MemoryBudget {
            &self.0
        }
        fn allocation_class(&self) -> MemoryClass {
            MemoryClass::Ordinary
        }
        fn read_original(
            &mut self,
            _: &[u8; 32],
            _: usize,
            _: &MemoryBudget,
        ) -> Result<Arc<ReservedValue<Vec<u8>>>, Self::Error> {
            Err("not in page originals")
        }
    }
    impl OriginalObjectStreamReader for Empty {
        fn stream_original(
            &mut self,
            _: &[u8; 32],
            _: u64,
            _: &MemoryBudget,
            _: &mut dyn Write,
        ) -> Result<u64, Self::Error> {
            Err("not in page originals")
        }
    }
    #[test]
    fn native_chunk_bridge_exact_legacy_and_physical_reject_corrupt_selector_and_wrong_cap() {
        for physical in [false, true] {
            let parent = std::env::temp_dir().join(format!(
                "oasis7-native-bridge-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            ));
            std::fs::create_dir(&parent).unwrap();
            std::fs::write(parent.join("formal-checkpoint-v1.cbor"), b"checkpoint").unwrap();
            let directory = Directory(parent.join("formal-objects-v1"));
            std::fs::create_dir(&directory.0).unwrap();
            crate::controlled_authority::formal::physical::initialize(&directory.0).unwrap();
            let memory = MemoryBudget::new(256 << 20, 16 << 20).unwrap();
            let _fixture = memory.reserve(8 << 20, MemoryClass::Ordinary).unwrap();
            let bytes = vec![73; 32 << 10];
            let id = *blake3::hash(&bytes).as_bytes();
            let text = hex::encode(id);
            if physical {
                assert!(
                    crate::controlled_authority::formal::physical::persist(
                        &directory.0,
                        &[(text.clone(), bytes.clone())]
                    )
                    .unwrap()
                );
            }
            // Keep genuine matching legacy bytes present even under a selector.
            // A corrupt selected representation must never fall back to these.
            if !physical {
                std::fs::write(directory.0.join(&text), &bytes).unwrap();
            }
            let descriptor = NativeExecutionEnvironmentV1 {
                schema_version: 1,
                target: "fixture".into(),
                os: "unix".into(),
                arch: "fixture".into(),
                components: [
                    NativeEnvironmentRole::Executable,
                    NativeEnvironmentRole::BuildMetadata,
                ]
                .into_iter()
                .enumerate()
                .map(|(n, role)| NativeEnvironmentComponentV1 {
                    role,
                    name: format!("component-{n}"),
                    byte_len: bytes.len() as u64,
                    content_digest: text.clone(),
                    chunks: vec![NativeEnvironmentChunkV1 {
                        digest: text.clone(),
                        byte_len: bytes.len() as u32,
                    }],
                })
                .collect(),
            };
            let raw = descriptor.encode().unwrap();
            let descriptor_id = *blake3::hash(&raw).as_bytes();
            let _pin = crate::controlled_authority::formal::physical::pin(&directory.0, false)
                .unwrap()
                .unwrap();
            let mut objects = Empty(memory.clone());
            let mut work = NativeChunkReadWorkV1 {
                occurrences_remaining: 16,
                bytes_remaining: 1 << 20,
                frozen_wire_bytes_remaining: 1 << 20,
            };
            let mut bridge = NativeChunkBridgeV1 {
                originals: &mut objects,
                descriptor: &descriptor,
                descriptor_id,
                descriptor_bytes: &raw,
                directory: &directory.0,
                memory: &memory,
                work: &mut work,
                cancelled: || false,
            };
            let used = memory.used();
            assert!(matches!(
                bridge.read_original(&id, bytes.len() - 1, &memory),
                Err(NativeChunkReadErrorV1::Invalid("native chunk expected cap"))
            ));
            let cached_before =
                crate::controlled_authority::formal::physical::admitted_cache_accesses();
            let actual = bridge.read_original(&id, bytes.len(), &memory).unwrap();
            assert_eq!(
                crate::controlled_authority::formal::physical::admitted_cache_accesses(),
                cached_before,
                "admitted reads cannot grow uncharged process caches"
            );
            assert_eq!(actual.as_slice(), bytes.as_slice());
            drop(actual);
            assert_eq!(memory.used(), used);
            assert_eq!(
                bridge
                    .read_original(&descriptor_id, raw.len(), &memory)
                    .unwrap()
                    .as_slice(),
                raw.as_slice()
            );
            if physical {
                match std::fs::remove_file(directory.0.join(&text)) {
                    Ok(()) => {}
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                    Err(e) => panic!("remove raw fixture alias: {e}"),
                }
                std::fs::write(directory.0.join(&text), &bytes).unwrap();
                std::fs::write(
                    directory.0.join(".representation-v1").join(&text),
                    b"corrupt selector",
                )
                .unwrap();
                assert!(
                    matches!(
                        bridge.read_original(&id, bytes.len(), &memory),
                        Err(NativeChunkReadErrorV1::Storage(_))
                    ),
                    "matching raw bytes cannot rescue corrupt selector"
                );
            } else {
                std::fs::write(directory.0.join(&text), &bytes[..bytes.len() - 1]).unwrap();
                assert!(bridge.read_original(&id, bytes.len(), &memory).is_err());
            }
            assert_eq!(memory.used(), used);
        }
    }
}
