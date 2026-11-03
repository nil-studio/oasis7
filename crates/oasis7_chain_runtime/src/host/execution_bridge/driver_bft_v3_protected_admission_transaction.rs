//! Original protected admission transaction; shared sequencing stays in one owner.
use super::*;

impl FormalBftHeaderNodeDriverV3 {
    pub(super) fn retain_source_input_with_original(
        &mut self,
        binding: &BftSignerBindingV3,
        lease: &mut dyn oasis7_distfs::protected_bft_v3::BftSignerLeaseV3,
        expected: &oasis7_node::NodeBftExecutionContextV3,
        candidate: &oasis7_node::NodeBftPrivateCandidateV3,
        deadline: Instant,
        kind: private_store::SourceSlotKindV1,
        mut original_read: Option<
            oasis7_distfs::protected_bft_v3::BftPrivateRetentionAdmissionInputReadV1,
        >,
    ) -> Result<String, String> {
        use oasis7_distfs::protected_bft_v3::BftPrivateRetentionLifecycleV1;
        self.check_binding(binding)?;
        self.validate_candidate(expected, candidate)?;
        let owner = self
            .covered_source
            .as_ref()
            .ok_or("protected Some requires original covered source")?;
        owner.verify_at(deadline)?;
        super::super::super::successor_archive::live(deadline)?;
        let observation = lease
            .observe_private_retention(deadline)
            .map_err(|e| e.to_string())?;
        self.covered_source
            .as_ref()
            .ok_or("original covered source owner absent")?
            .verify_observation(binding, &observation)?;
        let source = observation.ledger().map.source.clone();
        // A complete exact Ready retry requires original local material, never
        // another unreserved publication or a new fact.
        if let Some(entry) = observation.ledger().map.entries.iter().find(|e| {
            e.key.height == expected.height
                && e.key.header_id == candidate.public_value.block_id().unwrap_or_default()
        }) {
            if entry.lifecycle == BftPrivateRetentionLifecycleV1::Ready {
                let reference = oasis7_distfs::protected_bft_v3::BftPrivateRetentionAdmissionInputRefV1::from_entry(&source, entry).map_err(|e| e.to_string())?;
                let proof = private_store::ReservedSourceInputV1::from_observation(
                    binding,
                    &self
                        .covered_source
                        .as_ref()
                        .ok_or("original source owner missing")?
                        .fixed_attestor,
                    &reference,
                    &entry.fact,
                    &observation,
                )?;
                return if kind == private_store::SourceSlotKindV1::Local {
                    self.select_source_ready(
                        binding,
                        lease,
                        candidate,
                        &proof,
                        observation.ledger(),
                        deadline,
                    )
                } else {
                    self.private
                        .verify_ready_source_candidate(candidate, &reference, &entry.fact)
                };
            }
        }
        let executed = self.execution.origin.execute(&candidate.actions_cbor)?;
        let reader = self.execution.origin.retained_nodes.clone();
        reader.with_accessed_nodes(|nodes| {
            let prefix = self
                .execution
                .ledger
                .records()
                .map(|r| r.input().certificate)
                .collect::<Vec<_>>();
            let plan =
                super::super::super::super::publication_plan::PublicationPlanV1::from_actual(
                    &self.execution.origin.schedule,
                    &executed,
                    &candidate.actions_cbor,
                    nodes,
                    &prefix,
                    storage::RECORD_DOMAIN,
                )?;
            let io = self
                .admission_input_io
                .as_ref()
                .ok_or("original admission input IO not configured")?
                .clone();
            // Same original permit/reservation path is shared with Active producers.
            let (plan, reference, fact, proof, recovered) = self.private.prepare_protected_input(
                binding,
                &self
                    .covered_source
                    .as_ref()
                    .ok_or("original source owner absent")?
                    .fixed_attestor,
                lease,
                candidate,
                &plan,
                nodes,
                kind,
                false,
                &BTreeSet::new(),
                &observation,
                io.as_ref(),
                original_read.take(),
                deadline,
            )?;
            let owner = self
                .covered_source
                .as_ref()
                .ok_or("original covered source missing")?;
            let (digest, proof, ready) = self.private.complete_protected_admission(
                binding,
                &owner.fixed_attestor,
                lease,
                plan,
                nodes,
                &proof,
                &reference,
                &fact,
                kind == private_store::SourceSlotKindV1::Local,
                deadline,
                io.as_ref(),
                &mut |bytes, future| {
                    self.execution.ledger.reserve_private_material(
                        bytes,
                        future.clone(),
                        &self.execution.store,
                    )
                },
                &mut || {
                    super::super::super::successor_archive::live(deadline)?;
                    owner.verify_at(deadline)
                },
            )?;
            let digest = if kind == private_store::SourceSlotKindV1::Local {
                self.select_source_ready(binding, lease, candidate, &proof, &ready, deadline)?
            } else {
                digest
            };
            drop(recovered);
            Ok(digest)
        })
    }
}
