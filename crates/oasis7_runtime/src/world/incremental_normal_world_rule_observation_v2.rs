//! Genuine pre-Observe Current cut. This JSON/SHA256 original is explicitly
//! format 2, not the legacy serde_json(WorldState) root and not acceptance.
use crate::world::incremental_normal_sources::BoundNormalTickParentV1;
use oasis7_formal_protocol::{
    controlled_authority::formal::{
        FormalError,
        bounded_context::{FormalBoundedContextV1, WorkIdentityV1},
    },
    incremental_execution::{
        EXECUTION_MAX_READS, EXECUTION_MAX_WRITES, ExecutionError, ExecutionFactKindV1,
        ExecutionFence, ExecutionTransaction, PreparedGuard,
    },
    incremental_state::{MerkleNodeReader, StateNamespaceV1},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, MemoryLimitExceeded, ReservedValue};
use sha2::{Digest, Sha256};
use std::{
    io::{self, Write},
    mem::size_of,
};
#[path = "incremental_normal_world_rule_observation_v2_json.rs"]
mod json;
pub(in crate::world) const OBSERVATION_CURRENT_DOMAIN_V2: &str =
    "oasis7.emergency-observation.current.v2";
#[derive(Debug)]
pub(in crate::world) enum ObservationCurrentSourceErrorV2 {
    Formal(FormalError),
    Execution(ExecutionError),
    Memory(MemoryLimitExceeded),
    Io(io::Error),
    Invalid(&'static str),
}
impl From<FormalError> for ObservationCurrentSourceErrorV2 {
    fn from(e: FormalError) -> Self {
        Self::Formal(e)
    }
}
impl From<ExecutionError> for ObservationCurrentSourceErrorV2 {
    fn from(e: ExecutionError) -> Self {
        Self::Execution(e)
    }
}
impl From<MemoryLimitExceeded> for ObservationCurrentSourceErrorV2 {
    fn from(e: MemoryLimitExceeded) -> Self {
        Self::Memory(e)
    }
}
impl From<io::Error> for ObservationCurrentSourceErrorV2 {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}
fn invalid(reason: &'static str) -> ObservationCurrentSourceErrorV2 {
    ObservationCurrentSourceErrorV2::Invalid(reason)
}
fn limited() -> ObservationCurrentSourceErrorV2 {
    ObservationCurrentSourceErrorV2::Execution(ExecutionError::ResourceLimited(
        "Current observation original",
    ))
}
#[derive(Clone, Copy)]
struct ParentIds {
    record: [u8; 32],
    body: [u8; 32],
    roots: [u8; 32],
}
const NAMESPACES: [StateNamespaceV1; 6] = [
    StateNamespaceV1::Business,
    StateNamespaceV1::Allocator,
    StateNamespaceV1::Scheduler,
    StateNamespaceV1::Authorization,
    StateNamespaceV1::Effects,
    StateNamespaceV1::Artifacts,
];
/// Owns the actual original bytes, digest and capture scope. All getters borrow;
/// no constructor, detached body, clone, physical lease or business permit.
pub(in crate::world) struct ObservationCurrentSourceV2 {
    original: ReservedValue<Vec<u8>>,
    commitment: ReservedValue<String>,
    parent: ParentIds,
    guard: PreparedGuard,
    identity: WorkIdentityV1,
    memory: MemoryBudget,
    _fee: ReservedValue<()>,
}
impl ObservationCurrentSourceV2 {
    pub(in crate::world) fn original_json(&self) -> &[u8] {
        &self.original
    }
    pub(in crate::world) fn current_commitment(&self) -> &str {
        &self.commitment
    }
    pub(in crate::world) fn parent_record_id(&self) -> &[u8; 32] {
        &self.parent.record
    }
    pub(in crate::world) fn parent_body_id(&self) -> &[u8; 32] {
        &self.parent.body
    }
    pub(in crate::world) fn parent_roots_digest(&self) -> &[u8; 32] {
        &self.parent.roots
    }
    /// Validate immediately before the consumer records this pre-write cut.
    /// The guard must not be substituted for the eventual terminal seal guard.
    pub(in crate::world) fn validate_capture<M: MerkleNodeReader>(
        &self,
        parent: &ReservedValue<BoundNormalTickParentV1<'_>>,
        tx: &ExecutionTransaction<'_, M>,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<(), ObservationCurrentSourceErrorV2> {
        let ids = bind_parent(parent, tx, common, live)?;
        tx.validate_guard(&self.guard, live)?;
        if self.identity != common.work().identity()
            || !self.memory.same_pool(common.memory())
            || !self.original.uses_budget(common.memory())
            || !self.commitment.uses_budget(common.memory())
            || !self._fee.uses_budget(common.memory())
            || ids.record != self.parent.record
            || ids.body != self.parent.body
            || ids.roots != self.parent.roots
        {
            return Err(invalid("Current observation source owner"));
        }
        Ok(())
    }
}
fn bind_parent<M: MerkleNodeReader>(
    parent: &ReservedValue<BoundNormalTickParentV1<'_>>,
    tx: &ExecutionTransaction<'_, M>,
    common: &mut FormalBoundedContextV1<'_>,
    live: &ExecutionFence,
) -> Result<ParentIds, ObservationCurrentSourceErrorV2> {
    parent.validate_context(common)?;
    common.charge(1, 32 * 12 + 6 * 128)?;
    if !parent.uses_budget(common.memory()) || !tx.uses_budget(common.memory()) {
        return Err(invalid("Current observation parent pool"));
    }
    let refs = parent.refs();
    let fence = parent.fence();
    let fact = refs.runtime.occurrence();
    let record = refs.head.verified_record_id();
    if refs.core.candidate_record_id() != record
        || fact.head_record_id() != record
        || fact.kind() != ExecutionFactKindV1::Result
        || fact.head_body_id() != fact.fact_body_id()
        || fact.head_body_id() != fence.parent_commit
        || fact.roots_id() != fence.parent_roots
        || refs.core.body().roots != fence.parent_roots
        || fence != tx.parent_fence()
        || fence != *live
    {
        return Err(invalid("Current observation actual parent/fence"));
    }
    // Root descriptors are borrowed from the already-bound actual parent;
    // parent_state_root is explicitly the immutable parent, never Current.
    for namespace in NAMESPACES {
        let actual = tx.parent_state_root(namespace);
        let expected = refs.roots.state_root(namespace);
        common.charge(
            1,
            (actual.node_root.len() + expected.node_root.len()) as u64,
        )?;
        if actual != expected {
            return Err(invalid("Current observation actual parent RootSet"));
        }
    }
    tx.validate_guard(&tx.guard(), live)?;
    Ok(ParentIds {
        record,
        body: fence.parent_commit,
        roots: fence.parent_roots,
    })
}
struct Measured {
    bytes: usize,
    hash: Sha256,
}
impl Write for Measured {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.bytes = self
            .bytes
            .checked_add(bytes.len())
            .ok_or(io::ErrorKind::Other)?;
        self.hash.update(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
struct Buffered {
    bytes: Vec<u8>,
    maximum: usize,
    hash: Sha256,
}
impl Write for Buffered {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self
            .bytes
            .len()
            .checked_add(bytes.len())
            .is_none_or(|n| n > self.maximum)
        {
            return Err(io::Error::other("Current original measured bound"));
        }
        self.bytes.extend_from_slice(bytes);
        self.hash.update(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
pub(in crate::world) fn prepare_observation_current_source_v2<M: MerkleNodeReader>(
    parent: &ReservedValue<BoundNormalTickParentV1<'_>>,
    tx: &mut ExecutionTransaction<'_, M>,
    common: &mut FormalBoundedContextV1<'_>,
    live: &ExecutionFence,
) -> Result<ObservationCurrentSourceV2, ObservationCurrentSourceErrorV2> {
    let result = (|| {
        let ids = bind_parent(parent, tx, common, live)?;
        let guard = tx.guard();
        let owner_fee = common
            .memory()
            .reserve(
                size_of::<ObservationCurrentSourceV2>(),
                MemoryClass::Ordinary,
            )?
            .retain(());
        let mut measured = Measured {
            bytes: 0,
            hash: Sha256::new(),
        };
        json::write_original(tx, ids, &mut measured, common)?;
        tx.validate_guard(&guard, live)?;
        let raw_fee = common.memory().reserve(
            measured
                .bytes
                .checked_add(size_of::<Vec<u8>>())
                .ok_or_else(limited)?,
            MemoryClass::Ordinary,
        )?;
        let digest_fee = common
            .memory()
            .reserve(size_of::<String>() + 64, MemoryClass::Ordinary)?;
        let mut bytes = Vec::new();
        bytes
            .try_reserve_exact(measured.bytes)
            .map_err(|_| limited())?;
        if bytes.capacity() > measured.bytes {
            return Err(limited());
        }
        let mut actual = Buffered {
            bytes,
            maximum: measured.bytes,
            hash: Sha256::new(),
        };
        json::write_original(tx, ids, &mut actual, common)?;
        let expected: [u8; 32] = measured.hash.finalize().into();
        let digest: [u8; 32] = actual.hash.finalize().into();
        if actual.bytes.len() != measured.bytes || digest != expected {
            return Err(invalid("Current observation stable measured original"));
        }
        let mut text = String::with_capacity(64);
        if text.capacity() > 64 {
            return Err(limited());
        }
        const HEX: &[u8; 16] = b"0123456789abcdef";
        for byte in digest {
            text.push(HEX[(byte >> 4) as usize] as char);
            text.push(HEX[(byte & 15) as usize] as char);
        }
        common.charge(1, 64)?;
        tx.validate_guard(&guard, live)?;
        let final_ids = bind_parent(parent, tx, common, live)?;
        if ids.record != final_ids.record
            || ids.body != final_ids.body
            || ids.roots != final_ids.roots
        {
            return Err(invalid("Current observation parent changed"));
        }
        common.consume_steps(0)?;
        Ok(ObservationCurrentSourceV2 {
            original: raw_fee.retain(actual.bytes),
            commitment: digest_fee.retain(text),
            parent: ids,
            guard,
            identity: common.work().identity(),
            memory: common.memory().clone(),
            _fee: owner_fee,
        })
    })();
    if result.is_err() {
        let _: Result<(), ExecutionError> = tx.contribute(|_| Err(ExecutionError::Aborted));
    }
    result
}

#[path = "incremental_normal_world_rule_observation_v2_package.rs"]
mod package;
#[path = "incremental_normal_world_rule_observation_v2_persist.rs"]
mod persist;
pub(in crate::world) use package::{
    PersistedObservationOriginalDataV2, persist_observation_original_data_v2,
};
pub use persist::{
    ObservationJsonOriginalSinkV2, ObservationJsonOriginalStreamV2,
    ObservationOriginalStreamFailedV2,
};
pub(in crate::world) use persist::{
    ObservationOriginalPersistenceErrorV2, PersistedObservationCurrentSourceV2,
};
