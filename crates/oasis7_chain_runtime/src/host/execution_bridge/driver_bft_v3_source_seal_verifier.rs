//! Actual original-source cut replay, including heights with no private input.
use super::super::super::super::super::super::storage as common_storage;
use super::*;
use std::collections::{BTreeMap, BTreeSet};
use std::time::Instant;

impl ActualBftPrivateSourceCreationVerifierV1 {
    pub(super) fn verify_actual_seal_cut(
        &self,
        binding: &BftSignerBindingV3,
        context: &oasis7_distfs::protected_bft_v3::BftPrivateRetentionSealVerificationContextV1<'_>,
        deadline: Instant,
    ) -> Result<(), String> {
        let state = &context.signer().state;
        if state.successor_window.is_some() || state.pending_driver_apply.is_some() {
            return Err("source seal cut requires original applied source history".into());
        }
        let (origin, records) = self.carriers.read_source_history(deadline)?;
        let creation = origin
            .source_creation_identity()
            .ok_or("source seal requires original covered Origin")?;
        let material = blake3_hex(
            &serde_cbor::to_vec(&(
                "oasis7.formal.bft.node_material.v3",
                origin.digest()?,
                creation,
                origin.material_pin_digest(),
            ))
            .map_err(|_| "encode source seal original identity")?,
        );
        if binding.material_identity != material
            || state.binding_digest != binding.digest().map_err(|error| error.to_string())?
            || records.len() != state.history.len()
            || records
                .iter()
                .map(|record| record.input().certificate)
                .ne(state.history.iter().cloned())
        {
            return Err("source seal exact current original history differs".into());
        }
        retain_origin_material(&self.store, &self.inputs.material, &origin, false)?;
        node_execution::verify_source_seal_context_role(
            &self.records,
            &self.store,
            binding,
            context,
        )?;
        let original_data = node_execution::read_source_origin(&self.records, &self.store)?;
        let mut references = BTreeSet::new();
        for record in &records {
            successor_archive::live(deadline)?;
            self.store
                .verify_pin_scope_shard(
                    storage::DIRECTORY,
                    &format!("origin-{}-height-{:020}", origin.digest()?, record.height()),
                    &record.references(),
                )
                .map_err(|_| "source seal original record retention missing")?;
            references.extend(record.business_index_refs().iter().cloned());
        }
        let mut nodes = BTreeMap::new();
        for reference in references {
            successor_archive::live(deadline)?;
            let bytes = common_storage::read_cas_bounded(
                &self.store,
                &reference,
                formal_dependencies::BUSINESS_INDEX_MAX_NODE_BYTES,
            )?;
            nodes.insert(reference, bytes);
            formal_dependencies::validate_nodes(&nodes)?;
        }
        let captured_h = FormalBftHeaderExecutionV3::from_handoff(
            self.inputs.anchor.clone(),
            self.inputs.target.clone(),
            &self.inputs.material,
            self.inputs.source.clone(),
            nodes.into_iter().collect(),
            false,
        )?;
        if material_pin_digest(&captured_h, &self.inputs.material)? != origin.material_pin_digest()
            || binding.handoff_record_id != captured_h.handoff.handoff_head().record_id
            || binding.activation_digest != captured_h.handoff.activation_digest()
            || binding.initial_schedule().digest()? != captured_h.schedule.digest()?
        {
            return Err("source seal independently restored H differs".into());
        }
        node_execution::capture_h_index(&captured_h)?;
        let closed = captured_h.retained_nodes.take_closed_inventory()?;
        drop(captured_h);
        let mut execution = FormalBftHeaderExecutionV3::from_closed_handoff(
            self.inputs.anchor.clone(),
            self.inputs.target.clone(),
            &self.inputs.material,
            closed,
        )?;
        let mut previous = origin.digest()?;
        for record in &records {
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
                return Err("source seal original record did not actually reproduce".into());
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
                    return Err("source seal original execution bytes changed".into());
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
            previous = record.digest()?;
        }
        if let Some(certificate) = state.history.last() {
            let next = certificate
                .execution_commitment
                .height
                .checked_add(1)
                .ok_or("source seal height overflow")?;
            if execution.schedule.height() != next {
                return Err("source seal replay did not reach exact applied cut".into());
            }
        }
        let (after_origin, after_records) = self.carriers.read_source_history(deadline)?;
        if after_origin != origin
            || after_records != records
            || node_execution::read_source_origin(&self.records, &self.store)? != original_data
        {
            return Err("source seal original changed during cut replay".into());
        }
        retain_origin_material(&self.store, &self.inputs.material, &origin, false)?;
        for record in &records {
            self.store
                .verify_pin_scope_shard(
                    storage::DIRECTORY,
                    &format!("origin-{}-height-{:020}", origin.digest()?, record.height()),
                    &record.references(),
                )
                .map_err(|_| "source seal record retention changed during replay")?;
        }
        successor_archive::live(deadline)
    }
}
