//! Actual normal Qualified publication, separate from the GEN admission lane.
use super::super::FormalArchiveReadWorkV1;
use super::*;
use crate::controlled_authority::fencing::{
    BoundedAcquireErrorV1, RemoteMonotonicEndpointProvider, TrustedFencingPolicyV1,
};
use oasis7_formal_protocol::controlled_authority::formal::{
    PaidVerifiedNormalCommitV1, verify_normal_qualified_bounded,
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
pub(in crate::controlled_authority::formal::endpoint) struct NormalValidatedInputs {
    quorum: PaidVerifiedNormalCommitV1,
    qualified: store::PaidRequestStateV1,
}
impl NormalValidatedInputs {
    pub(in crate::controlled_authority::formal::endpoint) fn verified(
        &self,
    ) -> &VerifiedFormalCommitV1 {
        self.quorum.verified()
    }
    pub(in crate::controlled_authority::formal::endpoint) fn state(
        &self,
    ) -> &store::PaidRequestStateV1 {
        &self.qualified
    }
    pub(in crate::controlled_authority::formal::endpoint) fn uses_budget(
        &self,
        memory: &MemoryBudget,
    ) -> bool {
        self.quorum.uses_budget(memory) && self.qualified.uses_budget(memory)
    }
    pub(in crate::controlled_authority::formal::endpoint) fn reserved_bytes(&self) -> usize {
        self.quorum.reserved_bytes() + self.qualified.reserved_bytes()
    }
    pub(in crate::controlled_authority::formal::endpoint) fn validate_context(
        &self,
        memory: &MemoryBudget,
        work: &FormalArchiveReadWorkV1,
        stop: &mut dyn FnMut() -> bool,
    ) -> Result<(), FormalError> {
        let mut ctx = work.bounded_context(memory, stop);
        self.quorum.validate_context(&mut ctx)?;
        if !self.qualified.uses_budget(memory) {
            return Err(invalid("normal Qualified foreign pool"));
        }
        Ok(())
    }
    pub(in crate::controlled_authority::formal::endpoint) fn into_quorum(
        self,
    ) -> PaidVerifiedNormalCommitV1 {
        self.quorum
    }
}
pub(in crate::controlled_authority::formal::endpoint) struct NormalInstallPermit<'a> {
    job: jobs::NormalJobLease<'a>,
    inputs: NormalValidatedInputs,
}
impl<'a> NormalInstallPermit<'a> {
    pub(in crate::controlled_authority::formal::endpoint) fn split(
        self,
    ) -> (jobs::NormalJobLease<'a>, NormalValidatedInputs) {
        (self.job, self.inputs)
    }
}
pub struct PaidNormalAcceptanceV1 {
    head: ReservedValue<FormalHeadRefV1>,
}
impl PaidNormalAcceptanceV1 {
    pub fn head(&self) -> &FormalHeadRefV1 {
        &self.head
    }
    pub fn uses_budget(&self, memory: &MemoryBudget) -> bool {
        self.head.uses_budget(memory)
    }
    pub fn reserved_bytes(&self) -> usize {
        self.head.reserved_bytes()
    }
}
fn same<T: serde::Serialize + PartialEq>(
    a: &T,
    b: &T,
    memory: &MemoryBudget,
    work: &FormalArchiveReadWorkV1,
    stop: &mut dyn FnMut() -> bool,
) -> Result<bool, FormalError> {
    store::bounded_transaction::same(a, b, memory, work, stop)
}
fn head(
    verified: &VerifiedFormalCommitV1,
    memory: &MemoryBudget,
    work: &FormalArchiveReadWorkV1,
    stop: &mut dyn FnMut() -> bool,
) -> Result<PaidNormalAcceptanceV1, FormalError> {
    let fee = memory
        .reserve(
            std::mem::size_of::<PaidNormalAcceptanceV1>()
                + std::mem::size_of::<FormalHeadRefV1>()
                + 64,
            MemoryClass::Ordinary,
        )
        .map_err(|_| invalid("normal head output admission"))?;
    work.charge(1, 64, stop)?;
    Ok(PaidNormalAcceptanceV1 {
        head: fee.retain(FormalHeadRefV1 {
            position: verified.record().position,
            record_id: hex::encode(verified.verified_record_id()),
        }),
    })
}
impl FileFormalEndpoint {
    #[allow(clippy::too_many_arguments)]
    pub fn publish_normal_qualified_bounded(
        &mut self,
        candidate: &FormalCandidateMaterialV1,
        proof: &FormalQualifiedCommitProofV1,
        provider: &RemoteMonotonicEndpointProvider,
        policy: &TrustedFencingPolicyV1,
        memory: &MemoryBudget,
        work: &FormalArchiveReadWorkV1,
        stop: &mut dyn FnMut() -> bool,
    ) -> Result<PaidNormalAcceptanceV1, BoundedAcquireErrorV1> {
        let mut lease = None;
        self.publish_normal_qualified_inner(
            candidate, proof, &mut lease, provider, policy, memory, work, stop,
        )
    }
    #[allow(clippy::too_many_arguments)]
    pub(in crate::controlled_authority::formal::endpoint) fn publish_normal_qualified_with_lease_bounded(
        &mut self,
        candidate: &FormalCandidateMaterialV1,
        proof: &FormalQualifiedCommitProofV1,
        lease_slot: &mut Option<crate::controlled_authority::fencing::PaidRemoteLeaseV1>,
        provider: &RemoteMonotonicEndpointProvider,
        policy: &TrustedFencingPolicyV1,
        memory: &MemoryBudget,
        work: &FormalArchiveReadWorkV1,
        stop: &mut dyn FnMut() -> bool,
    ) -> Result<PaidNormalAcceptanceV1, BoundedAcquireErrorV1> {
        if lease_slot.is_none() {
            return Err(FormalError::FencingUnproven.into());
        }
        let result = self.publish_normal_qualified_inner(
            candidate, proof, lease_slot, provider, policy, memory, work, stop,
        );
        if result.is_err() {
            drop(lease_slot.take());
        }
        result
    }
    #[allow(clippy::too_many_arguments)]
    fn publish_normal_qualified_inner(
        &mut self,
        candidate: &FormalCandidateMaterialV1,
        proof: &FormalQualifiedCommitProofV1,
        lease_slot: &mut Option<crate::controlled_authority::fencing::PaidRemoteLeaseV1>,
        provider: &RemoteMonotonicEndpointProvider,
        policy: &TrustedFencingPolicyV1,
        memory: &MemoryBudget,
        work: &FormalArchiveReadWorkV1,
        stop: &mut dyn FnMut() -> bool,
    ) -> Result<PaidNormalAcceptanceV1, BoundedAcquireErrorV1> {
        work.consume_steps(1, stop)?;
        // Historical exact retry returns DATA only, before any fresh-parent/lease
        // gate. Durable native request provenance is enough; no cache is promoted.
        if let Some(old) = self.store.read_request_bounded(
            &candidate.candidate.record.request_digest,
            memory,
            work,
            stop,
        )? && let FormalRequestStateV1::Qualified {
            candidate: stored,
            proof: installed,
            ..
        } = old.state()
        {
            if !same(stored, candidate, memory, work, stop)?
                || !same(installed, proof, memory, work, stop)?
            {
                return Err(FormalError::Conflict(
                    "normal retained Qualified original proof/material".into(),
                )
                .into());
            }
            // Public historical retry returns only DATA. A consuming live
            // holder must still promote its real staged job after an
            // already-durable write; only an actual current quorum may
            // bypass that promotion on this private continued-lease path.
            let mut accepted_retry = lease_slot.is_none();
            if !accepted_retry && let Some(capability) = self.iws_page_store.as_ref() {
                let accepted = capability.jobs.parent()?;
                if let Some(cache) = accepted.as_ref()
                    && let Some(quorum) = cache.normal_quorum.as_ref()
                {
                    quorum.validate_context(&mut work.bounded_context(memory, stop))?;
                    accepted_retry = self
                        .parent
                        .as_ref()
                        .is_some_and(|p| p.verified_record_id() == cache.commit_record)
                        && quorum.verified().verified_record_id() == cache.commit_record
                        && same(quorum.proof(), installed, memory, work, stop)?;
                }
            }
            if accepted_retry {
                let id = store::bounded_record_id::hash_canonical_record_bounded_v1(
                    &installed.decision.record,
                    memory,
                    work,
                    stop,
                )?;
                let fee = memory
                    .reserve(
                        std::mem::size_of::<PaidNormalAcceptanceV1>()
                            + std::mem::size_of::<FormalHeadRefV1>()
                            + id.id().len(),
                        MemoryClass::Ordinary,
                    )
                    .map_err(|_| invalid("normal retry head admission"))?;
                work.charge(1, id.id().len() as u64, stop)?;
                return Ok(PaidNormalAcceptanceV1 {
                    head: fee.retain(FormalHeadRefV1 {
                        position: installed.decision.record.position,
                        record_id: id.id().to_owned(),
                    }),
                });
            }
        }
        if self.poisoned||self.recovery.is_some()||self.authority.definition().profile_version!=oasis7_formal_protocol::controlled_authority::formal::incremental::IWS_AUTHORITY_PROFILE_VERSION||!oasis7_formal_protocol::controlled_authority::formal::incremental::is_iws_record(candidate.candidate.record.schema_version)||candidate.candidate.record.kind!=FormalRecordKindV1::Execution||candidate.branch!=FormalCompletionBranchV1::Normal||candidate.frozen.as_ref().is_none_or(|f|f.reservation.body.schema_version!=4)||candidate.frozen_material.is_none(){return Err(invalid("normal exact schema4 complete Frozen candidate").into());}
        if !same(
            &candidate.candidate.record,
            &proof.decision.record,
            memory,
            work,
            stop,
        )? || !same(
            &candidate.frozen,
            &proof.decision.frozen,
            memory,
            work,
            stop,
        )? || !same(
            &policy.provider_public_key,
            &self.authority.policy().fencing_provider_public_key,
            memory,
            work,
            stop,
        )? || !same(
            &policy.authorized_resource_bindings,
            &self.authority.policy().authorized_fencing_resource_bindings,
            memory,
            work,
            stop,
        )? {
            return Err(FormalError::FencingUnproven.into());
        }
        let capability = self
            .iws_page_store
            .as_ref()
            .ok_or_else(|| invalid("normal real page capability required"))?;
        let actual_parent = self
            .parent
            .as_ref()
            .ok_or_else(|| invalid("normal actual accepted parent required"))?;
        let instance = match self.role {
            EndpointRole::Primary => &self.authority.definition().primary.instance,
            EndpointRole::Replica => &self.authority.definition().replica.instance,
        };
        if let Some(lease) = lease_slot.as_ref() {
            lease.validate_current_binding(
                instance,
                &self.store.checkpoint,
                policy,
                &mut work.bounded_context(memory, stop),
            )?;
            if lease.pending().is_some() {
                return Err(FormalError::FencingUnproven.into());
            }
        } else {
            self.store.reconcile_pending_update_with_scope(
                provider,
                instance,
                policy,
                memory,
                work,
                stop,
                &store::bounded_transaction::RecoveryScope::Cold {
                    authority: &self.authority,
                    role: self.role,
                },
            )?;
        }
        if !capability
            .jobs
            .matches_bounded(candidate, memory, work, stop)?
        {
            return Err(invalid("normal complete exact material job required").into());
        }
        let quorum = verify_normal_qualified_bounded(
            proof,
            &self.authority,
            actual_parent,
            &mut work.bounded_context(memory, stop),
        )?;
        let job = capability.jobs.normal_job(actual_parent, memory)?;
        let closure = job.reference(quorum.verified(), actual_parent, memory)?;
        let ready = self
            .store
            .read_request_bounded(&proof.decision.record.request_digest, memory, work, stop)?
            .ok_or_else(|| invalid("normal durable local Ready required"))?;
        match ready.state() {
            FormalRequestStateV1::Ready {
                candidate: local,
                decision,
                complete: Some(complete),
                ..
            } => {
                let expected = match self.role {
                    EndpointRole::Primary => &proof.primary_complete,
                    EndpointRole::Replica => &proof.replica_complete,
                };
                if !same(local, candidate, memory, work, stop)?
                    || !same(decision, &proof.decision, memory, work, stop)?
                    || !same(complete, expected, memory, work, stop)?
                {
                    return Err(invalid("normal exact local decision/complete required").into());
                }
            }
            FormalRequestStateV1::Qualified {
                candidate: local,
                proof: installed,
                data_closure,
            } => {
                if !same(local, candidate, memory, work, stop)?
                    || !same(installed, proof, memory, work, stop)?
                    || !same(&data_closure.as_deref(), &Some(closure), memory, work, stop)?
                {
                    return Err(invalid("normal durable Qualified recovery source").into());
                }
            }
            _ => return Err(invalid("normal durable local ProofComplete required").into()),
        }
        let (parent, parent_fee) =
            genesis::paid_parent_copy_from_verified(quorum.verified(), memory, work, stop)?;
        let output = head(quorum.verified(), memory, work, stop)?;
        let qualified = genesis::qualified_state_from_proof(
            candidate,
            quorum.proof(),
            closure,
            memory,
            work,
            stop,
        )?;
        let publication = self.store.publish_normal_install_with_lease_bounded(
            NormalInstallPermit {
                job,
                inputs: NormalValidatedInputs { quorum, qualified },
            },
            lease_slot,
            provider,
            instance,
            policy,
            memory,
            work,
            stop,
        )?;
        let (quorum, job) = publication.into_parts();
        drop(job);
        capability.jobs.accepted_normal(quorum, parent_fee)?;
        self.parent = Some(parent);
        Ok(output)
    }
}
