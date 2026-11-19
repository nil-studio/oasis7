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
    return_hash_work: bool,
    memory: oasis7_proto::memory_budget::MemoryBudget,
}
struct ReturnedHashWork {
    record: [u8; 32],
    bytes: u64,
}
pub(super) struct EndpointIwsState {
    staged: RefCell<Option<Staged>>,
    accepted: RefCell<Option<AcceptedCandidateCache>>,
    returned_hash_work: RefCell<Option<ReturnedHashWork>>,
}

#[cfg(test)]
mod returned_work_tests {
    use super::*;

    #[test]
    fn returned_hash_account_is_record_bound_and_consumed_once() {
        let primary = EndpointIwsState::new();
        let replica = EndpointIwsState::new();
        let record = [7; 32];
        // A private bookkeeping fixture only; it creates no accepted source,
        // quorum, current qualification or public access to this work slot.
        *primary.returned_hash_work.borrow_mut() = Some(ReturnedHashWork { record, bytes: 19 });
        *replica.returned_hash_work.borrow_mut() = Some(ReturnedHashWork { record, bytes: 23 });
        let (mut first, mut second) = (0, 0);
        assert!(
            primary
                .return_source_hash_work_pair(&replica, [8; 32], (&mut first, &mut second))
                .is_err()
        );
        assert_eq!((first, second), (0, 0));
        second = 1;
        assert!(
            primary
                .return_source_hash_work_pair(&replica, record, (&mut first, &mut second))
                .is_err()
        );
        assert_eq!((first, second), (0, 1));
        second = 0;
        primary
            .return_source_hash_work_pair(&replica, record, (&mut first, &mut second))
            .unwrap();
        assert_eq!((first, second), (19, 23));
        let (mut again_first, mut again_second) = (0, 0);
        assert!(
            primary
                .return_source_hash_work_pair(
                    &replica,
                    record,
                    (&mut again_first, &mut again_second)
                )
                .is_err()
        );
        assert_eq!((again_first, again_second), (0, 0));
    }

    #[test]
    fn unavailable_returned_account_never_restores_a_ceiling() {
        let primary = EndpointIwsState::new();
        let replica = EndpointIwsState::new();
        let record = [9; 32];
        let (mut first, mut second) = (0, 0);
        *primary.returned_hash_work.borrow_mut() = Some(ReturnedHashWork { record, bytes: 0 });
        assert!(
            primary
                .return_source_hash_work_pair(&replica, record, (&mut first, &mut second))
                .is_err()
        );
        *replica.returned_hash_work.borrow_mut() = Some(ReturnedHashWork { record, bytes: 0 });
        let held = replica.returned_hash_work.borrow_mut();
        assert!(
            primary
                .return_source_hash_work_pair(&replica, record, (&mut first, &mut second))
                .is_err()
        );
        assert_eq!((first, second), (0, 0));
        drop(held);
        primary
            .return_source_hash_work_pair(&replica, record, (&mut first, &mut second))
            .unwrap();
        assert_eq!(
            (first, second),
            (0, 0),
            "exhausted staged accounts remain exhausted"
        );
    }

    #[test]
    fn second_endpoint_refusal_retains_both_original_accounts() {
        let primary = EndpointIwsState::new();
        let replica = EndpointIwsState::new();
        let record = [11; 32];
        *primary.returned_hash_work.borrow_mut() = Some(ReturnedHashWork { record, bytes: 17 });
        *replica.returned_hash_work.borrow_mut() = Some(ReturnedHashWork {
            record: [12; 32],
            bytes: 31,
        });
        let (mut first, mut second) = (0, 0);
        assert!(
            primary
                .return_source_hash_work_pair(&replica, record, (&mut first, &mut second))
                .is_err()
        );
        assert_eq!((first, second), (0, 0));
        assert_eq!(
            primary.returned_hash_work.borrow().as_ref().unwrap().bytes,
            17
        );
        assert_eq!(
            replica.returned_hash_work.borrow().as_ref().unwrap().bytes,
            31
        );
        replica
            .returned_hash_work
            .borrow_mut()
            .as_mut()
            .unwrap()
            .record = record;
        let held = replica.returned_hash_work.borrow_mut();
        assert!(
            primary
                .return_source_hash_work_pair(&replica, record, (&mut first, &mut second))
                .is_err()
        );
        assert_eq!((first, second), (0, 0));
        assert_eq!(
            primary.returned_hash_work.borrow().as_ref().unwrap().bytes,
            17
        );
        drop(held);
        primary
            .return_source_hash_work_pair(&replica, record, (&mut first, &mut second))
            .unwrap();
        assert_eq!((first, second), (17, 31));
    }
}
impl EndpointIwsState {
    pub(super) fn new() -> Self {
        Self {
            staged: RefCell::new(None),
            accepted: RefCell::new(None),
            returned_hash_work: RefCell::new(None),
        }
    }
    pub(super) fn stage(
        &self,
        candidate: &FormalCandidateMaterialV1,
        memory: &oasis7_proto::memory_budget::MemoryBudget,
        install: PreparedCandidateInstall,
        hash_bytes: &mut u64,
        return_hash_work: bool,
        cancelled: &mut impl FnMut() -> bool,
    ) -> Result<(), FormalError> {
        let mut slot = self
            .staged
            .try_borrow_mut()
            .map_err(|_| invalid("IWS candidate job borrow"))?;
        if slot.is_some() {
            return Err(invalid("IWS candidate job already staged"));
        }
        if self
            .returned_hash_work
            .try_borrow()
            .map_err(|_| invalid("IWS returned source work borrowed"))?
            .is_some()
        {
            return Err(invalid(
                "IWS accepted source work must be returned before staging",
            ));
        }
        let record = install.candidate_record_id();
        let _phase = memory
            .reserve(8192, oasis7_proto::memory_budget::MemoryClass::Ordinary)
            .map_err(|_| invalid("IWS material digest admission"))?;
        let material = material_digest(candidate, hash_bytes, cancelled, &mut |_| Ok(()))?;
        *slot = Some(Staged {
            record,
            install,
            material,
            // All subsequent exact-material checks spend this one account.
            // The outer work cannot also spend its staged remainder.
            hash_bytes: std::mem::take(hash_bytes),
            return_hash_work,
            memory: memory.clone(),
        });
        Ok(())
    }
    pub(super) fn matches(
        &self,
        candidate: &FormalCandidateMaterialV1,
        cancelled: &mut impl FnMut() -> bool,
    ) -> Result<bool, FormalError> {
        self.matches_with_write_work(candidate, cancelled, &mut |_| Ok(()), None)
    }
    /// Actual bounded consumers charge serialization/hash writes to the same
    /// original Common work, in addition to the existing continuous byte cap.
    pub(super) fn matches_bounded(
        &self,
        candidate: &FormalCandidateMaterialV1,
        memory: &oasis7_proto::memory_budget::MemoryBudget,
        work: &super::super::FormalArchiveReadWorkV1,
        stop: &mut dyn FnMut() -> bool,
    ) -> Result<bool, FormalError> {
        let stop = RefCell::new(stop);
        let mut cancelled = || (stop.borrow_mut())();
        let mut before_write = |n| work.charge(1, n, &mut || (stop.borrow_mut())());
        self.matches_with_write_work(candidate, &mut cancelled, &mut before_write, Some(memory))
    }
    fn matches_with_write_work(
        &self,
        candidate: &FormalCandidateMaterialV1,
        cancelled: &mut impl FnMut() -> bool,
        before_write: &mut impl FnMut(u64) -> Result<(), FormalError>,
        memory: Option<&oasis7_proto::memory_budget::MemoryBudget>,
    ) -> Result<bool, FormalError> {
        let mut slot = self
            .staged
            .try_borrow_mut()
            .map_err(|_| invalid("IWS candidate job borrow"))?;
        let Some(staged) = slot.as_mut() else {
            return Ok(false);
        };
        if memory.is_some_and(|memory| !staged.memory.same_pool(memory)) {
            return Err(invalid("IWS material reuse pool"));
        }
        let _phase = staged
            .memory
            .reserve(8192, oasis7_proto::memory_budget::MemoryClass::Ordinary)
            .map_err(|_| invalid("IWS material reuse admission"))?;
        Ok(staged.material
            == material_digest(candidate, &mut staged.hash_bytes, cancelled, before_write)?)
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
        let mut returned = self
            .returned_hash_work
            .try_borrow_mut()
            .map_err(|_| invalid("IWS returned source work borrowed"))?;
        if returned.is_some() {
            return Err(invalid("IWS accepted source work already retained"));
        }
        let candidate = staged
            .take()
            .ok_or_else(|| invalid("IWS missing staged candidate"))?;
        let remaining = candidate.return_hash_work.then_some(ReturnedHashWork {
            record,
            bytes: candidate.hash_bytes,
        });
        let cache = candidate.install.finish(commit)?;
        *accepted = Some(cache);
        *returned = remaining;
        Ok(())
    }
    /// Only a real accepted source may return the exact staged account. The
    /// zero outer lane is the hole left by stage's move, never a fresh ceiling.
    pub(super) fn return_source_hash_work(
        &self,
        record: [u8; 32],
        target: &mut u64,
    ) -> Result<(), FormalError> {
        let mut returned = self
            .returned_hash_work
            .try_borrow_mut()
            .map_err(|_| invalid("IWS returned source work borrowed"))?;
        let remaining = returned
            .as_ref()
            .ok_or_else(|| invalid("IWS accepted source work missing"))?;
        if remaining.record != record || *target != 0 {
            return Err(invalid("IWS accepted source work exact record/empty lane"));
        }
        *target = returned.take().expect("checked original source work").bytes;
        Ok(())
    }

    /// Complete accepted pairs return both accounts in one indivisible move.
    pub(super) fn return_source_hash_work_pair(
        &self,
        replica: &Self,
        record: [u8; 32],
        targets: (&mut u64, &mut u64),
    ) -> Result<(), FormalError> {
        let mut primary_returned = self
            .returned_hash_work
            .try_borrow_mut()
            .map_err(|_| invalid("IWS returned source work borrowed"))?;
        let mut replica_returned = replica
            .returned_hash_work
            .try_borrow_mut()
            .map_err(|_| invalid("IWS returned source work borrowed"))?;
        let primary_remaining = primary_returned
            .as_ref()
            .ok_or_else(|| invalid("IWS accepted source work missing"))?;
        let replica_remaining = replica_returned
            .as_ref()
            .ok_or_else(|| invalid("IWS accepted source work missing"))?;
        if primary_remaining.record != record
            || replica_remaining.record != record
            || *targets.0 != 0
            || *targets.1 != 0
        {
            return Err(invalid("IWS accepted source work exact record/empty lane"));
        }
        // Both original slots are exclusively held and both targets were
        // checked before either move. No cancellation, charge or fallible
        // operation may split this one-shot return into a partial transfer.
        let primary = primary_returned
            .take()
            .expect("checked primary source work");
        let replica = replica_returned
            .take()
            .expect("checked replica source work");
        *targets.0 = primary.bytes;
        *targets.1 = replica.bytes;
        Ok(())
    }
}

fn material_digest(
    candidate: &FormalCandidateMaterialV1,
    remaining: &mut u64,
    cancelled: &mut impl FnMut() -> bool,
    before_write: &mut impl FnMut(u64) -> Result<(), FormalError>,
) -> Result<[u8; 32], FormalError> {
    struct Sink<'a, C, B> {
        hash: blake3::Hasher,
        left: &'a mut u64,
        cancelled: &'a mut C,
        before_write: &'a mut B,
        first: Option<FormalError>,
    }
    impl<C: FnMut() -> bool, B: FnMut(u64) -> Result<(), FormalError>> std::io::Write
        for Sink<'_, C, B>
    {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            if self.first.is_some() {
                return Err(std::io::Error::other("IWS material hash stopped"));
            }
            if (self.cancelled)() {
                self.first = Some(invalid("IWS streamed exact material digest"));
                return Err(std::io::Error::other("IWS material hash stopped"));
            }
            let n = u64::try_from(bytes.len()).map_err(std::io::Error::other)?;
            let Some(left) = self.left.checked_sub(n) else {
                self.first = Some(invalid("IWS streamed exact material digest"));
                return Err(std::io::Error::other("IWS material hash quota"));
            };
            *self.left = left;
            // Charge before hashing any of this actual serializer write. A
            // Common error survives serde's IO adapter as its original type.
            if let Err(error) = (self.before_write)(n) {
                self.first = Some(error);
                return Err(std::io::Error::other("IWS material work stopped"));
            }
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
        before_write,
        first: None,
    };
    let encoded = serde_json::to_writer(&mut sink, candidate);
    if let Some(first) = sink.first.take() {
        return Err(first);
    }
    encoded.map_err(|_| invalid("IWS streamed exact material digest"))?;
    Ok(*sink.hash.finalize().as_bytes())
}

impl EndpointIwsState {
    /// A retained Ready may replace only its already accepted parent cache.
    /// The actual staged job, its hash account and its candidate qualification
    /// remain in place. Ordinary cold startup still refuses every staged job.
    pub(super) fn restore_ready_parent(
        &self,
        cache: AcceptedCandidateCache,
        head: &VerifiedFormalCommitV1,
        ready: &oasis7_formal_protocol::controlled_authority::formal::PaidVerifiedNormalCommitV1,
        memory: &oasis7_proto::memory_budget::MemoryBudget,
    ) -> Result<(), FormalError> {
        if cache.commit_record != head.verified_record_id() || cache.authenticated_cold().is_none()
        {
            return Err(invalid("IWS Ready cold exact authenticated parent"));
        }
        {
            let job = self.normal_job(head, memory)?;
            let (core, _, _, _) =
                job.normal_candidate_read_sources_v1(ready.verified(), head, memory)?;
            if core.candidate_record_id() != ready.verified().verified_record_id() {
                return Err(invalid("IWS Ready cold exact retained candidate"));
            }
        }
        let mut parent = self
            .accepted
            .try_borrow_mut()
            .map_err(|_| invalid("IWS Ready cold parent cache borrow"))?;
        if parent.as_ref().map(|value| value.commit_record) != Some(cache.commit_record) {
            return Err(invalid("IWS Ready cold cannot replace a different parent"));
        }
        *parent = Some(cache);
        Ok(())
    }

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

/// Pins the original complete candidate AND its actual live accepted parent.
pub(in crate::controlled_authority::formal::endpoint) struct NormalJobLease<'a> {
    slot: std::cell::Ref<'a, Option<Staged>>,
    parent: std::cell::Ref<'a, Option<AcceptedCandidateCache>>,
    parent_record: [u8; 32],
}
impl NormalJobLease<'_> {
    pub(super) fn reference<'a>(
        &'a self,
        commit: &VerifiedFormalCommitV1,
        parent: &VerifiedFormalCommitV1,
        memory: &oasis7_proto::memory_budget::MemoryBudget,
    ) -> Result<&'a str, FormalError> {
        let staged = self
            .slot
            .as_ref()
            .ok_or_else(|| invalid("normal complete stage missing"))?;
        let accepted = self
            .parent
            .as_ref()
            .ok_or_else(|| invalid("normal actual parent missing"))?;
        if staged.record != commit.verified_record_id()
            || accepted.commit_record != self.parent_record
            || self.parent_record != parent.verified_record_id()
        {
            return Err(invalid("normal job exact candidate/parent"));
        }
        staged.install.normal_reference(commit, parent, memory)
    }
}
impl EndpointIwsState {
    pub(super) fn normal_job(
        &self,
        parent: &VerifiedFormalCommitV1,
        memory: &oasis7_proto::memory_budget::MemoryBudget,
    ) -> Result<NormalJobLease<'_>, FormalError> {
        let accepted = self
            .accepted
            .try_borrow()
            .map_err(|_| invalid("normal parent slot borrowed"))?;
        let cache = accepted
            .as_ref()
            .ok_or_else(|| invalid("normal requires actual accepted cache"))?;
        let id = parent.verified_record_id();
        if cache.commit_record != id
            || (cache
                .genesis_quorum
                .as_ref()
                .is_none_or(|q| q.verified().verified_record_id() != id)
                && cache
                    .normal_quorum
                    .as_ref()
                    .is_none_or(|q| q.verified().verified_record_id() != id)
                && !cache.authenticated_cold().is_some_and(|q| {
                    q.checkpoint().head().verified_record_id() == id
                        && q.checkpoint().uses_budget(memory)
                        && q.core().uses_budget(memory)
                        && q.source().uses_budget(memory)
                }))
            || !cache.current().uses_budget(memory)
        {
            return Err(invalid("normal genuine accepted quorum parent"));
        }
        let slot = self
            .staged
            .try_borrow()
            .map_err(|_| invalid("normal staged slot borrowed"))?;
        let staged = slot
            .as_ref()
            .ok_or_else(|| invalid("normal complete candidate missing"))?;
        if staged.install.provisional_parent_record() != Some(id) {
            return Err(invalid("normal staged actual parent record"));
        }
        Ok(NormalJobLease {
            slot,
            parent: accepted,
            parent_record: id,
        })
    }
    pub(super) fn accepted_normal(
        &self,
        quorum: oasis7_formal_protocol::controlled_authority::formal::PaidVerifiedNormalCommitV1,
        parent_fee: oasis7_proto::memory_budget::ReservedValue<()>,
    ) -> Result<(), FormalError> {
        self.accepted(quorum.verified())?;
        let mut slot = self
            .accepted
            .try_borrow_mut()
            .map_err(|_| invalid("normal accepted slot borrowed"))?;
        let cache = slot
            .as_mut()
            .ok_or_else(|| invalid("normal accepted cache missing"))?;
        if cache.commit_record != quorum.verified().verified_record_id() {
            return Err(invalid("normal accepted exact quorum"));
        }
        cache.normal_quorum = Some(quorum);
        cache.formal_parent_fee = Some(parent_fee);
        Ok(())
    }
}

pub(super) type NormalCandidateReadSources<'a> = (
    &'a oasis7_proto::memory_budget::ReservedValue<
        oasis7_formal_protocol::incremental_material::CheckedCommitCore,
    >,
    &'a oasis7_proto::memory_budget::ReservedValue<
        oasis7_world_state::incremental_world_storage_source::CheckedWorldStorageSourceV1,
    >,
    &'a oasis7_proto::memory_budget::ReservedValue<
        oasis7_formal_protocol::incremental_execution::RootSetV1,
    >,
    Option<
        &'a oasis7_proto::memory_budget::ReservedValue<
            oasis7_world_state::incremental_cognition_proof_ledger::CheckedCognitionProofLedgerV1,
        >,
    >,
);

impl NormalJobLease<'_> {
    pub(super) fn normal_candidate_read_sources_v1<'a>(
        &'a self,
        commit: &VerifiedFormalCommitV1,
        parent: &VerifiedFormalCommitV1,
        memory: &oasis7_proto::memory_budget::MemoryBudget,
    ) -> Result<NormalCandidateReadSources<'a>, FormalError> {
        self.reference(commit, parent, memory)?;
        let staged = self
            .slot
            .as_ref()
            .ok_or_else(|| invalid("normal candidate stage missing"))?;
        Ok(staged.install.normal_candidate_read_sources_v1())
    }
}
