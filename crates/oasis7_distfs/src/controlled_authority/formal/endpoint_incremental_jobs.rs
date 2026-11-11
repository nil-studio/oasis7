//! Linear endpoint-local jobs. Staging never changes the accepted parent.
use super::{
    provisional::{AcceptedCandidateCache, PreparedCandidateInstall},
    *,
};
use std::cell::RefCell;
struct Staged {
    record: [u8; 32],
    install: PreparedCandidateInstall,
    material: [u8; 32],
    hash_bytes: u64,
    memory: oasis7_proto::memory_budget::MemoryBudget,
}
pub(super) struct EndpointIwsState {
    staged: RefCell<Option<Staged>>,
    accepted: RefCell<Option<AcceptedCandidateCache>>,
}
impl EndpointIwsState {
    pub(super) fn new() -> Self {
        Self {
            staged: RefCell::new(None),
            accepted: RefCell::new(None),
        }
    }
    pub(super) fn stage(
        &self,
        candidate: &FormalCandidateMaterialV1,
        memory: &oasis7_proto::memory_budget::MemoryBudget,
        install: PreparedCandidateInstall,
        hash_bytes: &mut u64,
        cancelled: &mut impl FnMut() -> bool,
    ) -> Result<(), FormalError> {
        let mut slot = self
            .staged
            .try_borrow_mut()
            .map_err(|_| invalid("IWS candidate job borrow"))?;
        if slot.is_some() {
            return Err(invalid("IWS candidate job already staged"));
        }
        let record = install.candidate_record_id();
        let _phase = memory
            .reserve(8192, oasis7_proto::memory_budget::MemoryClass::Ordinary)
            .map_err(|_| invalid("IWS material digest admission"))?;
        let material = material_digest(candidate, hash_bytes, cancelled)?;
        *slot = Some(Staged {
            record,
            install,
            material,
            hash_bytes: *hash_bytes,
            memory: memory.clone(),
        });
        Ok(())
    }
    pub(super) fn matches(
        &self,
        candidate: &FormalCandidateMaterialV1,
        cancelled: &mut impl FnMut() -> bool,
    ) -> Result<bool, FormalError> {
        let mut slot = self
            .staged
            .try_borrow_mut()
            .map_err(|_| invalid("IWS candidate job borrow"))?;
        let Some(staged) = slot.as_mut() else {
            return Ok(false);
        };
        let _phase = staged
            .memory
            .reserve(8192, oasis7_proto::memory_budget::MemoryClass::Ordinary)
            .map_err(|_| invalid("IWS material reuse admission"))?;
        Ok(staged.material == material_digest(candidate, &mut staged.hash_bytes, cancelled)?)
    }
    pub(super) fn local_data_reference(
        &self,
        commit: &oasis7_formal_protocol::controlled_authority::formal::VerifiedFormalCommitV1,
    ) -> Result<Option<String>, FormalError> {
        let slot = self
            .staged
            .try_borrow()
            .map_err(|_| invalid("IWS staged local closure borrow"))?;
        let staged = slot
            .as_ref()
            .ok_or_else(|| invalid("IWS staged local closure missing"))?;
        if staged.record != commit.verified_record_id() {
            return Err(invalid("IWS staged local closure exact commit"));
        }
        staged.install.local_data_reference(commit)
    }
    pub(super) fn ready(&self, record: [u8; 32]) -> Result<bool, FormalError> {
        Ok(self
            .staged
            .try_borrow()
            .map_err(|_| invalid("IWS job borrow"))?
            .as_ref()
            .map(|s| s.record)
            == Some(record))
    }
    pub(super) fn parent(
        &self,
    ) -> Result<std::cell::Ref<'_, Option<AcceptedCandidateCache>>, FormalError> {
        self.accepted
            .try_borrow()
            .map_err(|_| invalid("IWS parent cache borrow"))
    }
    /// Call only after the durable Qualified record is installed. This consumes
    /// pre-admitted output charges; an unmatched commit leaves the job intact.
    pub(super) fn accepted(&self, commit: &VerifiedFormalCommitV1) -> Result<(), FormalError> {
        let mut staged = self
            .staged
            .try_borrow_mut()
            .map_err(|_| invalid("IWS candidate job borrow"))?;
        let record = commit.verified_record_id();
        if staged.as_ref().map(|s| s.record) != Some(record) {
            return Err(invalid("IWS accepted exact staged candidate"));
        }
        let mut accepted = self
            .accepted
            .try_borrow_mut()
            .map_err(|_| invalid("IWS accepted cache borrow"))?;
        let candidate = staged
            .take()
            .ok_or_else(|| invalid("IWS missing staged candidate"))?;
        let cache = candidate.install.finish(commit)?;
        *accepted = Some(cache);
        Ok(())
    }
}

fn material_digest(
    candidate: &FormalCandidateMaterialV1,
    remaining: &mut u64,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<[u8; 32], FormalError> {
    struct Sink<'a, C> {
        hash: blake3::Hasher,
        left: &'a mut u64,
        cancelled: &'a mut C,
        failed: bool,
    }
    impl<C: FnMut() -> bool> std::io::Write for Sink<'_, C> {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            if self.failed || (self.cancelled)() {
                self.failed = true;
                return Err(std::io::Error::other("IWS material hash stopped"));
            }
            let n = u64::try_from(bytes.len()).map_err(std::io::Error::other)?;
            let Some(left) = self.left.checked_sub(n) else {
                self.failed = true;
                return Err(std::io::Error::other("IWS material hash quota"));
            };
            *self.left = left;
            self.hash.update(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut sink = Sink {
        hash: blake3::Hasher::new(),
        left: remaining,
        cancelled,
        failed: false,
    };
    serde_json::to_writer(&mut sink, candidate)
        .map_err(|_| invalid("IWS streamed exact material digest"))?;
    if sink.failed {
        return Err(invalid("IWS material digest poisoned"));
    }
    Ok(*sink.hash.finalize().as_bytes())
}

impl EndpointIwsState {
    pub(super) fn restore_cold(
        &self,
        cache: AcceptedCandidateCache,
        head: &VerifiedFormalCommitV1,
    ) -> Result<(), FormalError> {
        if cache.commit_record != head.verified_record_id() {
            return Err(invalid("IWS cold install exact accepted head"));
        }
        let staged = self
            .staged
            .try_borrow()
            .map_err(|_| invalid("IWS cold staged job borrow"))?;
        if staged.is_some() {
            return Err(invalid(
                "IWS cold restore cannot replace a staged candidate",
            ));
        }
        let mut parent = self
            .accepted
            .try_borrow_mut()
            .map_err(|_| invalid("IWS cold parent cache borrow"))?;
        *parent = Some(cache);
        Ok(())
    }
}

/// A borrowed private job with complete GEN evidence, not a serving or receipt permit.
pub(in crate::controlled_authority::formal::endpoint) struct GenesisJobLease<'a> {
    slot: std::cell::Ref<'a, Option<Staged>>,
}
impl GenesisJobLease<'_> {
    pub(super) fn reference<'a>(
        &'a self,
        commit: &VerifiedFormalCommitV1,
        memory: &oasis7_proto::memory_budget::MemoryBudget,
    ) -> Result<&'a str, FormalError> {
        let staged = self
            .slot
            .as_ref()
            .ok_or_else(|| invalid("GEN staged source missing"))?;
        if staged.record != commit.verified_record_id() {
            return Err(invalid("GEN exact staged record"));
        }
        staged.install.genesis_reference(commit, memory)
    }
}
impl EndpointIwsState {
    pub(super) fn genesis_job(&self) -> Result<GenesisJobLease<'_>, FormalError> {
        if self
            .accepted
            .try_borrow()
            .map_err(|_| invalid("GEN accepted slot borrowed"))?
            .is_some()
        {
            return Err(invalid("GEN cannot replace an accepted parent"));
        }
        let slot = self
            .staged
            .try_borrow()
            .map_err(|_| invalid("GEN staged job borrowed"))?;
        if slot.is_none() {
            return Err(invalid("GEN complete staged source missing"));
        }
        Ok(GenesisJobLease { slot })
    }
}

impl EndpointIwsState {
    pub(super) fn accepted_genesis(
        &self,
        quorum: oasis7_formal_protocol::controlled_authority::formal::PaidVerifiedGenesisCommitV1,
        parent_fee: oasis7_proto::memory_budget::ReservedValue<()>,
    ) -> Result<(), FormalError> {
        self.accepted(quorum.verified())?;
        let mut slot = self
            .accepted
            .try_borrow_mut()
            .map_err(|_| invalid("GEN accepted slot borrowed"))?;
        let accepted = slot
            .as_mut()
            .ok_or_else(|| invalid("GEN accepted cache missing"))?;
        if accepted.commit_record != quorum.verified().verified_record_id() {
            return Err(invalid("GEN accepted exact quorum"));
        }
        accepted.genesis_quorum = Some(quorum);
        accepted.formal_parent_fee = Some(parent_fee);
        Ok(())
    }
}

impl GenesisJobLease<'_> {
    pub(super) fn receipt_binding(
        &self,
        record: [u8; 32],
        memory: &oasis7_proto::memory_budget::MemoryBudget,
    ) -> Result<(), FormalError> {
        let staged = self
            .slot
            .as_ref()
            .ok_or_else(|| invalid("GEN receipt stage missing"))?;
        if staged.record != record {
            return Err(invalid("GEN receipt exact staged record"));
        }
        staged.install.genesis_receipt_binding(record, memory)
    }
}
