//! Active adapter over the same original protected publication transaction.
use super::*;
use oasis7_distfs::protected_bft_v3::*;
impl WindowPrivateStore {
    #[expect(
        clippy::too_many_arguments,
        reason = "Exact owner, lease and publication stay explicit."
    )]
    pub(in super::super::super) fn retain_protected(
        &mut self,
        base: &VerifiedPrivateSuccessorBaseV1,
        binding: &BftSignerBindingV3,
        lease: &mut dyn BftSignerLeaseV3,
        candidate: &NodeBftPrivateCandidateV3,
        actual: &super::super::super::super::publication_plan::PublicationPlanV1,
        nodes: &BTreeMap<String, Vec<u8>>,
        local: bool,
        ledger: &mut WindowLedger,
        observation: &ValidatedBftPrivateRetentionObservationV1,
        io: &dyn oasis7_node::NodeBftAdmissionInputPublicationIoV1,
        original_read: Option<BftPrivateRetentionAdmissionInputReadV1>,
        deadline: std::time::Instant,
        verify_owner: &mut impl FnMut() -> Result<(), String>,
    ) -> Result<String, String> {
        let role = base.original_retention_attestor()?;
        let origin = ledger.origin_digest()?;
        let id = candidate.public_value.block_id()?;
        let entry = observation.ledger().map.entries.iter().find(|entry| {
            entry.key.ledger_origin_digest == origin
                && entry.key.height == candidate.public_value.execution_commitment.height
                && entry.key.header_id == id
        });
        let store = self.0.accounting_store();
        let mut reserve = |bytes, future| ledger.reserve_private_material(bytes, future, &store);
        if let Some(entry) = entry {
            if entry.lifecycle == BftPrivateRetentionLifecycleV1::Ready {
                let reference =
                    BftPrivateRetentionAdmissionInputRefV1::from_entry(&role.source, entry)
                        .map_err(|e| e.to_string())?;
                let proof = private_store::ReservedSourceInputV1::from_observation(
                    binding,
                    role,
                    &reference,
                    &entry.fact,
                    observation,
                )?;
                return if local {
                    self.0.select_protected_ready(
                        binding,
                        role,
                        lease,
                        candidate,
                        &proof,
                        observation.ledger(),
                        deadline,
                        &mut reserve,
                        verify_owner,
                        true,
                    )
                } else {
                    self.0
                        .verify_ready_active_candidate(candidate, &reference, &entry.fact)
                };
            }
        }
        base.require_fresh_active_capability()?;
        verify_owner()?;
        let baseline = base.retained_baseline_refs()?;
        // A pending original retains its first Reserve price and read hold.
        // Only a genuinely absent key inventories other protected originals.
        let retained = if entry.is_none() {
            self.0
                .active_retained_refs(&baseline, binding, role, observation, io, deadline)?
        } else {
            baseline
        };
        let kind = if local {
            private_store::SourceSlotKindV1::Local
        } else {
            private_store::SourceSlotKindV1::Received
        };
        let (plan, reference, fact, proof, read) = self.0.prepare_protected_input(
            binding,
            role,
            lease,
            candidate,
            actual,
            nodes,
            kind,
            true,
            &retained,
            observation,
            io,
            original_read,
            deadline,
        )?;
        let (digest, ready, current) = self.0.complete_protected_admission(
            binding,
            role,
            lease,
            plan,
            nodes,
            &proof,
            &reference,
            &fact,
            local,
            deadline,
            io,
            &mut reserve,
            verify_owner,
        )?;
        let digest = if local {
            self.0.select_protected_ready(
                binding,
                role,
                lease,
                candidate,
                &ready,
                &current,
                deadline,
                &mut reserve,
                verify_owner,
                true,
            )?
        } else {
            digest
        };
        drop(read);
        Ok(digest)
    }
}

impl WindowPrivateStore {
    pub(in super::super::super) fn verify_ready_original(
        &self,
        candidate: &NodeBftPrivateCandidateV3,
        reference: &BftPrivateRetentionAdmissionInputRefV1,
        fact: &BftPrivateRetentionFactV1,
    ) -> Result<(), String> {
        self.0
            .verify_ready_active_candidate(candidate, reference, fact)
            .map(|_| ())
    }
}

impl WindowPrivateStore {
    pub(in super::super::super) fn original_is_local(
        input: &BftPrivateRetentionAdmissionInputV1,
    ) -> Result<bool, String> {
        Ok(private_store::active_admission_slot_kind(input)?
            == private_store::SourceSlotKindV1::Local)
    }
}
