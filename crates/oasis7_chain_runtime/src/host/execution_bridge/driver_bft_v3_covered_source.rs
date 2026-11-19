//! Runtime original H namespace preparation and locally durable activation.
use super::private_store::source_activation::{
    CoveredSourcePublication, SourceActivationRecordV1, check_original_namespace,
};
use super::*;
use oasis7_distfs::protected_bft_v3::{
    BftPrivateRetentionAttestorV1, BftPrivateRetentionCreationMarkerV1,
    BftPrivateRetentionSourceV1, BftSignerProviderV3, BftSignerStateV3,
    BftSuccessorWindowArchiveRefV1, RemoteBftSignerProviderV3,
};
use std::time::Instant;

impl FormalBftHeaderLedgerExecutionV3 {
    pub(crate) fn covered_source_binding_parts(
        &self,
        creation_identity: &str,
    ) -> Result<
        (
            VerifiedBftHandoffTargetV2,
            BftHeaderHeightScheduleV3,
            String,
        ),
        String,
    > {
        BftSuccessorWindowArchiveRefV1 {
            digest: creation_identity.into(),
            bytes: 1,
        }
        .validate()
        .map_err(|e| e.to_string())?;
        self.ledger.verify_carriers()?;
        if self.origin.schedule.active_set().validators().len() != 1
            || self.origin.schedule.pending_transition().is_some()
        {
            return Err("original covered source requires the supported singleton H".into());
        }
        if self.ledger.records().next().is_some() {
            return Err("covered source preparation requires only original qualified H".into());
        }
        let origin = self.ledger.origin()?;
        let material = node_material_identity(
            &origin.digest()?,
            creation_identity,
            origin.material_pin_digest(),
        )?;
        Ok((
            self.origin.handoff.clone(),
            self.origin.schedule.clone(),
            material,
        ))
    }

    pub(crate) fn covered_source_marker(
        &self,
        binding: &BftSignerBindingV3,
        creation_identity: &str,
    ) -> Result<BftPrivateRetentionCreationMarkerV1, String> {
        let (handoff, schedule, material) = self.covered_source_binding_parts(creation_identity)?;
        if binding.handoff().target_digest() != handoff.target_digest()
            || binding.handoff_record_id != handoff.handoff_head().record_id
            || binding.initial_schedule() != &schedule
            || binding.material_identity != material
        {
            return Err("covered source marker binding differs from actual original H".into());
        }
        let initial = BftSignerStateV3::from_handoff(binding).map_err(|error| error.to_string())?;
        let marker = BftPrivateRetentionCreationMarkerV1 {
            schema_version: 1,
            creation_identity: creation_identity.into(),
            source: BftPrivateRetentionSourceV1 {
                binding_digest: initial.binding_digest,
                handoff_digest: binding.handoff_record_id.clone(),
                material_source_digest: material,
            },
            ledger_origin_digest: self.ledger.origin_digest()?,
            activation_digest: binding.activation_digest.clone(),
            initial_state_digest: initial.state_digest,
            initial_height: schedule.height(),
        };
        check_original_namespace(&self.ledger, &self.store, &marker)?;
        Ok(marker)
    }

    pub(crate) fn activate_covered_original_source(
        &mut self,
        binding: &BftSignerBindingV3,
        fixed_attestor: &BftPrivateRetentionAttestorV1,
        marker: &BftPrivateRetentionCreationMarkerV1,
        provider: &RemoteBftSignerProviderV3,
        deadline: Instant,
    ) -> Result<String, String> {
        if &self.covered_source_marker(binding, &marker.creation_identity)? != marker
            || marker.digest().map_err(|e| e.to_string())? != fixed_attestor.creation_marker_digest
            || marker.source != fixed_attestor.source
        {
            return Err("covered source marker differs from independent provider attestor".into());
        }
        // Requalify and walk the complete committed H inventory; no current
        // candidate, WAL reconstruction, or index fallback beyond actual H.
        let inventory = FormalBftHeaderExecutionV3::from_handoff(
            self.inputs.anchor.clone(),
            self.inputs.target.clone(),
            &self.inputs.material,
            self.inputs.source.clone(),
            Vec::new(),
            false,
        )?;
        capture_h_index(&inventory)?;
        let nodes = inventory.retained_nodes.accessed_nodes()?;
        if Instant::now() >= deadline {
            return Err("covered source activation expired before protected lease".into());
        }
        self.ledger.verify_carriers()?;
        let mut lease = provider
            .acquire(binding)
            .map_err(|error| error.to_string())?;
        let activated = lease
            .provision_private_retention(fixed_attestor, marker)
            .map_err(|error| error.to_string())?;
        let checkpoint = lease.checkpoint().map_err(|error| error.to_string())?;
        let activation = SourceActivationRecordV1::from_lease(
            binding,
            fixed_attestor,
            marker,
            activated,
            &checkpoint,
        )?;
        let result = CoveredSourcePublication {
            store: &self.store,
            cursor_height: self.origin.handoff.handoff_position(),
            nodes: &nodes,
            activation: &activation,
            deadline,
        }
        .publish(&mut self.ledger);
        drop(lease);
        result
    }
}
