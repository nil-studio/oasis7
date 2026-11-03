//! Independent source admission replay from authenticated H and immutable
//! originals. Candidate dependencies supply bytes, never a parent authority.
use super::super::super::super::super::super::storage as common_storage;
use super::*;
use oasis7_distfs::protected_bft_v3::{
    BftPrivateRetentionAdmissionInputV1 as Input, BftPrivateRetentionInputObjectKindV1 as Kind,
};
use std::collections::{BTreeMap, BTreeSet};
use std::time::Instant;

/// Actual original Source parent identity, returned only after complete input
/// and first-reservation budget verification. No World/input payload is copied.
pub(in super::super) struct VerifiedSourceInputParentV1 {
    pub(in super::super) schedule_digest: String,
    pub(in super::super) cut: Option<oasis7_distfs::protected_bft_v3::BftSuccessorWindowCutV1>,
}

impl ActualBftPrivateSourceCreationVerifierV1 {
    pub(super) fn verify_actual_input(
        &self,
        binding: &BftSignerBindingV3,
        input: &Input,
        context: &oasis7_distfs::protected_bft_v3::BftPrivateRetentionVerificationContextV1<'_>,
        deadline: Instant,
    ) -> Result<(), String> {
        self.verify_actual_input_at(
            binding,
            input,
            SourceInputVerification::Current(context),
            deadline,
        )
        .map(|_| ())
    }
    pub(super) fn verify_historical_input(
        &self,
        binding: &BftSignerBindingV3,
        input: &Input,
        context: &oasis7_distfs::protected_bft_v3::BftPrivateRetentionHistoricalInputContextV1<'_>,
        deadline: Instant,
    ) -> Result<(), String> {
        self.verify_actual_input_at(
            binding,
            input,
            SourceInputVerification::Historical(context),
            deadline,
        )
        .map(|_| ())
    }
    pub(super) fn verify_historical_input_parent(
        &self,
        binding: &BftSignerBindingV3,
        input: &Input,
        context: &oasis7_distfs::protected_bft_v3::BftPrivateRetentionHistoricalInputContextV1<'_>,
        deadline: Instant,
    ) -> Result<VerifiedSourceInputParentV1, String> {
        self.verify_actual_input_at(
            binding,
            input,
            SourceInputVerification::Historical(context),
            deadline,
        )
    }
    fn verify_actual_input_at(
        &self,
        binding: &BftSignerBindingV3,
        input: &Input,
        context: SourceInputVerification<'_, '_>,
        deadline: Instant,
    ) -> Result<VerifiedSourceInputParentV1, String> {
        let state = context.state();
        successor_archive::live(deadline)?;
        input.validate().map_err(|e| e.to_string())?;
        let head = state
            .private_retention
            .as_ref()
            .ok_or("source coverage head absent")?;
        if state.binding_digest != binding.digest().map_err(|e| e.to_string())?
            || input.source != head.source
            || input.source.binding_digest != state.binding_digest
            || state.successor_window.is_some()
            || state.pending_driver_apply.is_some()
        {
            return Err("source admission differs from protected original source/parent".into());
        }
        let (origin, all_records) = self.carriers.read_source_history(deadline)?;
        let creation = origin
            .source_creation_identity()
            .ok_or("source admission requires covered Origin")?;
        let material = blake3_hex(
            &serde_cbor::to_vec(&(
                "oasis7.formal.bft.node_material.v3",
                origin.digest()?,
                creation,
                origin.material_pin_digest(),
            ))
            .map_err(|_| "encode source admission original identity")?,
        );
        if binding.material_identity != material
            || input.key.ledger_origin_digest != origin.digest()?
            || all_records.len() < state.history.len()
            || (!context.is_historical() && all_records.len() != state.history.len())
            || all_records[..state.history.len()]
                .iter()
                .map(|r| r.input().certificate)
                .ne(state.history.iter().cloned())
        {
            return Err("source admission H/history differs from protected parent".into());
        }
        // Historical replay consumes the exact authenticated original prefix;
        // later local records remain present and are independently bound below.
        let records = &all_records[..state.history.len()];
        retain_origin_material(&self.store, &self.inputs.material, &origin, false)?;
        let origin_data = node_execution::read_source_origin(&self.records, &self.store)?;
        match &context {
            SourceInputVerification::Current(context) => {
                node_execution::verify_source_input_context_role(
                    &self.records,
                    &self.store,
                    binding,
                    context,
                )?
            }
            SourceInputVerification::Historical(context) => {
                node_execution::verify_source_input_historical_role(
                    &self.records,
                    &self.store,
                    binding,
                    context,
                )?
            }
        }
        let candidate = node_execution::source_admission_candidate(input)?;
        let mut seed = input
            .objects
            .iter()
            .filter(|o| o.kind == Kind::Dependency)
            .map(|o| (o.reference.digest.clone(), o.bytes.clone()))
            .collect::<BTreeMap<_, _>>();
        formal_dependencies::validate_nodes(&seed)?;
        let mut retained = BTreeSet::new();
        for record in records {
            successor_archive::live(deadline)?;
            self.store
                .verify_pin_scope_shard(
                    storage::DIRECTORY,
                    &format!("origin-{}-height-{:020}", origin.digest()?, record.height()),
                    &record.references(),
                )
                .map_err(|_| "source original record retention missing")?;
            retained.extend(record.business_index_refs().iter().cloned());
        }
        for digest in retained {
            let bytes = common_storage::read_cas_bounded(
                &self.store,
                &digest,
                formal_dependencies::BUSINESS_INDEX_MAX_NODE_BYTES,
            )?;
            if let Some(original) = seed.get(&digest) {
                if original != &bytes {
                    return Err("source admission dependency conflicts with retained parent".into());
                }
            } else {
                seed.insert(digest, bytes);
                formal_dependencies::validate_nodes(&seed)?;
            }
        }
        formal_dependencies::validate_nodes(&seed)?;
        let captured_h = FormalBftHeaderExecutionV3::from_handoff(
            self.inputs.anchor.clone(),
            self.inputs.target.clone(),
            &self.inputs.material,
            self.inputs.source.clone(),
            seed.into_iter().collect(),
            false,
        )?;
        if material_pin_digest(&captured_h, &self.inputs.material)? != origin.material_pin_digest()
            || binding.handoff_record_id != captured_h.handoff.handoff_head().record_id
            || binding.activation_digest != captured_h.handoff.activation_digest()
            || binding.initial_schedule().digest()? != captured_h.schedule.digest()?
        {
            return Err("source admission independently restored H differs".into());
        }
        // Reproduce the exact creation-time baseline before any record/action.
        // The supplied bytes must close the real H root; unused caller refs do
        // not enter this independently traversed original inventory.
        node_execution::capture_h_index(&captured_h)?;
        let h_refs = captured_h
            .retained_nodes
            .with_accessed_nodes(|nodes| Ok(nodes.keys().cloned().collect::<BTreeSet<_>>()))?;
        let closed = captured_h.retained_nodes.take_closed_inventory()?;
        drop(captured_h);
        let mut execution = FormalBftHeaderExecutionV3::from_closed_handoff(
            self.inputs.anchor.clone(),
            self.inputs.target.clone(),
            &self.inputs.material,
            closed,
        )?;
        let mut previous = origin.digest()?;
        let mut parent_cut = None;
        for record in records {
            successor_archive::live(deadline)?;
            let entry = record.input();
            let executed = execution.replay_next(&entry.certificate, &entry.actions_cbor)?;
            let observed = execution
                .retained_nodes
                .with_accessed_nodes(|nodes| Ok(nodes.keys().cloned().collect::<Vec<_>>()))?;
            let reproduced =
                storage::Record::from_actual(&origin, &previous, &entry, &executed, observed)?
                    .with_publication_reservation(
                        record.publication_reservation_ref().map(str::to_owned),
                    )?;
            if &reproduced != record {
                return Err("source original record did not reproduce from H".into());
            }
            for (reference, bytes, limit) in [
                (
                    record.snapshot_ref(),
                    &executed.snapshot_bytes,
                    common_storage::MAX_BFT_SNAPSHOT_BYTES,
                ),
                (
                    record.journal_ref(),
                    &executed.journal_bytes,
                    common_storage::MAX_BFT_JOURNAL_BYTES,
                ),
            ] {
                if common_storage::read_cas_bounded(&self.store, reference, limit)? != *bytes {
                    return Err("source original execution publication changed".into());
                }
            }
            if let Some(reference) = record.publication_reservation_ref() {
                super::super::publication_reservation::PublicationReservationV1::read(
                    &self.store,
                    reference,
                )?
                .validate_execution(
                    &origin.digest()?,
                    record.height(),
                    &entry.actions_cbor,
                    &executed,
                )?;
            }
            let certificate = &entry.certificate;
            let commitment = &certificate.execution_commitment;
            let record_digest = record.digest()?;
            parent_cut = Some(oasis7_distfs::protected_bft_v3::BftSuccessorWindowCutV1 {
                height: record.height(),
                header_id: certificate.header.block_id()?,
                certificate_digest: blake3_hex(&certificate.canonical_bytes()?),
                execution_commitment_digest: commitment.digest()?,
                snapshot_ref: oasis7_distfs::protected_bft_v3::BftSuccessorWindowArchiveRefV1 {
                    digest: record.snapshot_ref().to_owned(),
                    bytes: executed.snapshot_bytes.len() as u64,
                },
                journal_ref: oasis7_distfs::protected_bft_v3::BftSuccessorWindowArchiveRefV1 {
                    digest: record.journal_ref().to_owned(),
                    bytes: executed.journal_bytes.len() as u64,
                },
                semantic_state_root: commitment.state_root.clone(),
                post_manifest_hash: commitment.post_manifest_hash.clone(),
                business_result_root: commitment.result_root.clone(),
                driver_install_digest: successor_archive::install_digest(
                    &binding.material_identity,
                    &record_digest,
                    certificate,
                )?,
                driver_record_digest: record_digest.clone(),
            });
            previous = record_digest;
        }
        if execution.schedule.height() != input.key.height {
            return Err("source admission is not the exact next protected height".into());
        }
        let parent_schedule_digest = execution.schedule.digest()?;
        let executed = execution.execute(&candidate.actions_cbor)?;
        if executed.candidate.header != candidate.public_value.header
            || executed.candidate.commitment != candidate.public_value.execution_commitment
        {
            return Err("source input value/actions did not reproduce at protected parent".into());
        }
        execution.retained_nodes.with_accessed_nodes(|observed| {
            let supplied = input.objects.iter().filter(|o| o.kind == Kind::Dependency);
            if supplied.clone().count() != observed.len()
                || supplied
                    .into_iter()
                    .any(|object| observed.get(&object.reference.digest) != Some(&object.bytes))
            {
                return Err("source input dependencies differ from actual closed reads".into());
            }
            let plan = super::super::publication_plan::PublicationPlanV1::from_actual(
                &execution.schedule,
                &executed,
                &candidate.actions_cbor,
                observed,
                &state.history,
                storage::RECORD_DOMAIN,
            )?;
            node_execution::validate_source_admission_input(
                input,
                &origin_data,
                &candidate,
                &plan,
                observed,
            )?;
            let reservation_bytes = match &context {
                SourceInputVerification::Current(context) => {
                    node_execution::source_admission_reservation_bytes(
                        &self.records,
                        &self.store,
                        input,
                        &plan,
                        observed,
                        context,
                        &h_refs,
                    )?
                }
                SourceInputVerification::Historical(context) => {
                    node_execution::source_admission_historical_reservation_bytes(
                        &self.records,
                        &self.store,
                        input,
                        &plan,
                        observed,
                        context,
                        &h_refs,
                    )?
                }
            };
            if reservation_bytes != input.publication_reservation_bytes {
                return Err(
                    "source input publication budget differs from actual original growth".into(),
                );
            }
            Ok(())
        })?;
        let (after_origin, after_records) = self.carriers.read_source_history(deadline)?;
        if after_origin != origin
            || after_records != all_records
            || node_execution::read_source_origin(&self.records, &self.store)? != origin_data
        {
            return Err("source original changed during independent admission".into());
        }
        retain_origin_material(&self.store, &self.inputs.material, &origin, false)?;
        for record in records {
            self.store
                .verify_pin_scope_shard(
                    storage::DIRECTORY,
                    &format!("origin-{}-height-{:020}", origin.digest()?, record.height()),
                    &record.references(),
                )
                .map_err(|_| "source original retention changed during admission")?;
        }
        successor_archive::live(deadline)?;
        Ok(VerifiedSourceInputParentV1 {
            schedule_digest: parent_schedule_digest,
            cut: parent_cut,
        })
    }
}

enum SourceInputVerification<'a, 'b> {
    Current(&'a oasis7_distfs::protected_bft_v3::BftPrivateRetentionVerificationContextV1<'b>),
    Historical(
        &'a oasis7_distfs::protected_bft_v3::BftPrivateRetentionHistoricalInputContextV1<'b>,
    ),
}
impl SourceInputVerification<'_, '_> {
    fn state(&self) -> &BftSignerStateV3 {
        match self {
            Self::Current(context) => &context.signer().state,
            Self::Historical(context) => &context.reservation().signer.state,
        }
    }
    fn is_historical(&self) -> bool {
        matches!(self, Self::Historical(_))
    }
}
