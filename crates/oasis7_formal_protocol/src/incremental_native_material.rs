//! Original native descriptor/chunks bound to the checked candidate. No
//! current-environment equivalence, manifest semantics or receipt is granted.
use super::*;
use crate::native_environment::{
    MAX_NATIVE_DESCRIPTOR_BYTES, NativeEnvironmentRole, NativeExecutionEnvironmentV1,
    NativeVerifyError,
};

pub const NATIVE_MATERIAL_WORKSPACE_BYTES: usize = 16 << 20;

#[derive(Debug)]
pub struct CheckedNativeEnvironment {
    body: [u8; 32],
    environment: [u8; 32],
    executable: [u8; 32],
    component_bytes: u64,
    chunk_occurrences: u64,
}
impl CheckedNativeEnvironment {
    pub fn body_id(&self) -> [u8; 32] {
        self.body
    }
    pub fn environment_id(&self) -> [u8; 32] {
        self.environment
    }
    pub fn executable_id(&self) -> [u8; 32] {
        self.executable
    }
    pub fn component_bytes(&self) -> u64 {
        self.component_bytes
    }
    pub fn chunk_occurrences(&self) -> u64 {
        self.chunk_occurrences
    }
}
struct Bytes(Arc<ReservedValue<Vec<u8>>>);
impl AsRef<[u8]> for Bytes {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}
/// Decode at most the existing 1 MiB descriptor and hold only one admitted
/// 1 MiB chunk. Repeated references are read/charged as occurrences. Metadata
/// admission covers bounded strings/vectors, canonical encoding and hashing.
pub fn verify_native_environment<O: OriginalObjectReader>(
    core: &CheckedCommitCore,
    objects: &mut O,
    memory: &MemoryBudget,
    budget: &mut CoreBudget,
    mut cancelled: impl FnMut() -> bool,
) -> Result<ReservedValue<CheckedNativeEnvironment>, CoreError<O::Error>> {
    let mut phase = memory
        .reserve(NATIVE_MATERIAL_WORKSPACE_BYTES, MemoryClass::Ordinary)
        .map_err(ExecutionError::Memory)?;
    if !core.catalog.uses_budget(memory) || !objects.allocation_budget().same_pool(memory) {
        return Err(CoreError::Invalid("native source memory pools"));
    }
    let id = core.execution_artifacts.native;
    if id != core.body.execution_environment {
        return Err(CoreError::Invalid("native candidate binding"));
    }
    let bytes = load(
        objects,
        &id,
        MAX_NATIVE_DESCRIPTOR_BYTES,
        memory,
        budget,
        &mut cancelled,
    )?;
    let descriptor = NativeExecutionEnvironmentV1::decode(&bytes)
        .map_err(|_| CoreError::Artifact("native descriptor format/canonical bytes"))?;
    let mut result = CheckedNativeEnvironment {
        body: core.catalog.entry.commit,
        environment: id,
        executable: [0; 32],
        component_bytes: 0,
        chunk_occurrences: 0,
    };
    for component in &descriptor.components {
        if component.role == NativeEnvironmentRole::Executable {
            result.executable = hex::decode(&component.content_digest)
                .map_err(|_| CoreError::Invalid("native executable digest"))?
                .try_into()
                .map_err(|_| CoreError::Invalid("native executable length"))?;
        }
        result.component_bytes += component.byte_len;
        result.chunk_occurrences += component.chunks.len() as u64;
    }
    descriptor
        .verify_with(|id| {
            let hash: [u8; 32] = hex::decode(id)
                .map_err(|_| CoreError::Invalid("native chunk digest"))?
                .try_into()
                .map_err(|_| CoreError::Invalid("native chunk digest length"))?;
            load(
                objects,
                &hash,
                crate::native_environment::MAX_NATIVE_CHUNK_BYTES,
                memory,
                budget,
                &mut cancelled,
            )
            .map(Bytes)
        })
        .map_err(|error| match error {
            NativeVerifyError::Source(error) => error,
            NativeVerifyError::Descriptor(_) => CoreError::Artifact("native descriptor validation"),
            NativeVerifyError::ChunkIntegrity => CoreError::Artifact("native chunk bytes/hash"),
            NativeVerifyError::ComponentIntegrity => {
                CoreError::Artifact("native component content hash")
            }
        })?;
    if cancelled() {
        return Err(CoreError::Cancelled);
    }
    let charge = phase
        .split(256)
        .map_err(|_| CoreError::Invalid("native result admission"))?;
    Ok(charge.retain(result))
}
