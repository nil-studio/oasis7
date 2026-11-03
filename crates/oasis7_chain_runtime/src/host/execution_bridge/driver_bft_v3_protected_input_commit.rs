//! Common actual original publication, Ready readback and input release.
use super::*;

impl NodePrivateStore {
    /// One real post-Reserve transaction shared by Source and Active adapters.
    #[expect(
        clippy::too_many_arguments,
        reason = "Original ownership, backend operation and bounded publication remain explicit."
    )]
    pub(in super::super::super) fn complete_protected_admission(
        &mut self,
        binding: &BftSignerBindingV3,
        role: &BftPrivateRetentionAttestorV1,
        lease: &mut dyn oasis7_distfs::protected_bft_v3::BftSignerLeaseV3,
        plan: SourceAdmissionPlanV1,
        nodes: &BTreeMap<String, Vec<u8>>,
        proof: &ReservedSourceInputV1,
        reference: &InputRef,
        fact: &BftPrivateRetentionFactV1,
        local: bool,
        deadline: Instant,
        io: &dyn oasis7_node::NodeBftAdmissionInputPublicationIoV1,
        reserve: &mut impl FnMut(
            usize,
            super::super::super::super::super::formal_ledger::PublicationPromiseInventoryV1,
        ) -> Result<(), String>,
        verify_owner: &mut impl FnMut() -> Result<(), String>,
    ) -> Result<
        (
            String,
            ReservedSourceInputV1,
            oasis7_distfs::protected_bft_v3::BftPrivateRetentionLedgerV1,
        ),
        String,
    > {
        let digest =
            self.publish_source_admission(plan, nodes, proof, deadline, reserve, verify_owner)?;
        if local && self.original_source_slot_kind(reference)? == SourceSlotKindV1::Local {
            lease.ready_private_retention_original_local(reference.key.clone(), deadline)
        } else {
            lease.ready_private_retention(reference.key.clone(), deadline)
        }
        .map_err(|e| e.to_string())?;
        let ready = lease
            .observe_private_retention(deadline)
            .map_err(|e| e.to_string())?;
        let proof =
            ReservedSourceInputV1::from_observation(binding, role, reference, fact, &ready)?;
        proof.require_ready()?;
        if io.contains(reference, deadline)? {
            io.clear(reference)?;
        }
        Ok((digest, proof, ready.ledger().clone()))
    }
}

impl NodePrivateStore {
    #[expect(
        clippy::too_many_arguments,
        reason = "The original operation, material and actual lease remain explicit across Source and Active."
    )]
    pub(in super::super::super) fn prepare_protected_input(
        &mut self,
        binding: &BftSignerBindingV3,
        role: &BftPrivateRetentionAttestorV1,
        lease: &mut dyn oasis7_distfs::protected_bft_v3::BftSignerLeaseV3,
        candidate: &NodeBftPrivateCandidateV3,
        actual: &super::super::super::super::super::publication_plan::PublicationPlanV1,
        nodes: &BTreeMap<String, Vec<u8>>,
        kind: SourceSlotKindV1,
        active: bool,
        retained: &BTreeSet<String>,
        observation: &ValidatedBftPrivateRetentionObservationV1,
        io: &dyn oasis7_node::NodeBftAdmissionInputPublicationIoV1,
        original_read: Option<
            oasis7_distfs::protected_bft_v3::BftPrivateRetentionAdmissionInputReadV1,
        >,
        deadline: Instant,
    ) -> Result<
        (
            SourceAdmissionPlanV1,
            InputRef,
            BftPrivateRetentionFactV1,
            ReservedSourceInputV1,
            Option<oasis7_distfs::protected_bft_v3::BftPrivateRetentionAdmissionInputReadV1>,
        ),
        String,
    > {
        let head = observation
            .checkpoint()
            .state
            .private_retention
            .as_ref()
            .ok_or("protected observed head missing")?;
        if observation.checkpoint().provider_public_key != role.provider_public_key
            || observation.checkpoint().state.binding_digest
                != binding.digest().map_err(|e| e.to_string())?
            || head.capability_digest != role.digest().map_err(|e| e.to_string())?
            || head.source != role.source
            || observation.ledger().map.source != role.source
        {
            return Err(
                "protected observation differs from original fixed role before material".into(),
            );
        }
        if active && !role.allow_authenticated_successor_origins {
            return Err("fixed original role denies fresh Active input before material".into());
        }
        let origin = &self
            .origin
            .as_ref()
            .ok_or("protected original origin absent")?
            .data;
        if origin.schema_version != if active { 3 } else { 8 } {
            return Err("protected producer owner family mismatch".into());
        }
        let key = oasis7_distfs::protected_bft_v3::BftPrivateRetentionKeyV1 {
            ledger_origin_digest: origin.ledger_origin_digest.clone(),
            height: candidate.public_value.execution_commitment.height,
            header_id: candidate.public_value.block_id()?,
        };
        if let Some(entry) = observation
            .ledger()
            .map
            .entries
            .iter()
            .find(|entry| entry.key == key)
        {
            if entry.lifecycle != BftPrivateRetentionLifecycleV1::ReservedPending {
                return Err("existing Ready original must be reused before preparation".into());
            }
            let reference = InputRef::from_entry(&observation.ledger().map.source, entry)
                .map_err(|e| e.to_string())?;
            let proof = ReservedSourceInputV1::from_observation(
                binding,
                role,
                &reference,
                &entry.fact,
                observation,
            )?;
            let read = match original_read {
                Some(read) => read,
                None => io.read_original(&reference, deadline)?,
            };
            reference
                .validate_input(read.input())
                .map_err(|e| e.to_string())?;
            let plan = if active {
                self.resume_active_admission_plan(
                    candidate,
                    actual,
                    nodes,
                    read.input(),
                    &proof,
                    retained,
                )?
            } else {
                self.resume_source_admission_plan(candidate, actual, nodes, read.input(), &proof)?
            };
            return Ok((plan, reference, entry.fact.clone(), proof, Some(read)));
        }
        let plan = if active {
            self.plan_active_admission(candidate, actual, nodes, kind, retained)?
        } else {
            self.plan_source_admission(candidate, actual, nodes, kind)?
        };
        let slot = io.reserve()?;
        let input = plan.input(&observation.ledger().map.source, nodes.clone())?;
        let fact = input.fact().map_err(|e| e.to_string())?;
        let reference = slot.publish(input)?;
        lease
            .reserve_private_retention_input(reference.clone(), deadline)
            .map_err(|e| e.to_string())?;
        let reserved = lease
            .observe_private_retention(deadline)
            .map_err(|e| e.to_string())?;
        let proof =
            ReservedSourceInputV1::from_observation(binding, role, &reference, &fact, &reserved)?;
        Ok((plan, reference, fact, proof, None))
    }
}

impl NodePrivateStore {
    #[expect(
        clippy::too_many_arguments,
        reason = "Both adapters share the original protected selection RPC and exact typed receipt."
    )]
    pub(in super::super::super) fn select_protected_ready(
        &mut self,
        binding: &BftSignerBindingV3,
        role: &BftPrivateRetentionAttestorV1,
        lease: &mut dyn oasis7_distfs::protected_bft_v3::BftSignerLeaseV3,
        candidate: &NodeBftPrivateCandidateV3,
        proof: &ReservedSourceInputV1,
        current: &oasis7_distfs::protected_bft_v3::BftPrivateRetentionLedgerV1,
        deadline: Instant,
        reserve: &mut impl FnMut(
            usize,
            super::super::super::super::super::formal_ledger::PublicationPromiseInventoryV1,
        ) -> Result<(), String>,
        verify_owner: &mut impl FnMut() -> Result<(), String>,
        active: bool,
    ) -> Result<String, String> {
        verify_owner()?;
        proof.require_ready()?;
        let mut selected = proof.clone();
        if !selected.has_original_selection() {
            if let Some(operation) = selected.current_selection_operation() {
                let original = lease
                    .read_private_retention_selection_original(current, operation, deadline)
                    .map_err(|e| e.to_string())?;
                selected.verify_original_selection(&original)?;
            } else {
                let observation = lease
                    .observe_private_retention(deadline)
                    .map_err(|e| e.to_string())?;
                let _ = ReservedSourceInputV1::from_observation(
                    binding,
                    role,
                    &proof.reference,
                    &proof.entry.fact,
                    &observation,
                )?;
                let domain = if active {
                    "oasis7.formal.active.select.v13"
                } else {
                    "oasis7.formal.source.select.v1"
                };
                let request = oasis7_distfs::protected_bft_v3::BftPrivateRetentionSelectRequestV1 {
                    operation_id: blake3_hex(
                        &serde_cbor::to_vec(&(domain, &proof.reference))
                            .map_err(|_| "encode original selection operation")?,
                    ),
                    key: proof.reference.key.clone(),
                    reference: proof.reference.clone(),
                    expected_revision: observation.checkpoint().state.revision,
                    expected_state_digest: observation.checkpoint().state.state_digest.clone(),
                    expected_head_digest: observation
                        .ledger()
                        .receipt
                        .body
                        .head
                        .digest()
                        .map_err(|e| e.to_string())?,
                    expected_choice: None,
                };
                let result = lease
                    .select_private_retention(request, deadline)
                    .map_err(|e| e.to_string())?;
                let observed = lease
                    .observe_private_retention(deadline)
                    .map_err(|e| e.to_string())?;
                verify_owner()?;
                if observed.ledger() != &result.current {
                    return Err("selection current readback changed".into());
                }
                selected = ReservedSourceInputV1::from_observation(
                    binding,
                    role,
                    &proof.reference,
                    &proof.entry.fact,
                    &observed,
                )?;
                selected.verify_original_selection(&result.original)?;
            }
        }
        if active {
            self.select_ready_active_candidate(
                candidate,
                &selected,
                deadline,
                reserve,
                verify_owner,
            )
        } else {
            self.select_ready_source_candidate(
                candidate,
                &selected,
                deadline,
                reserve,
                verify_owner,
            )
        }
    }
}
