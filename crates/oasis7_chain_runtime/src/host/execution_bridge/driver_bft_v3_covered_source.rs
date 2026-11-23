//! Runtime original H namespace preparation and locally durable activation.
use super::private_store::source_activation::{
    CoveredSourcePublication, SourceActivationRecordV1, check_original_namespace,
    read_authenticated_original,
};
use super::*;
use oasis7_distfs::protected_bft_v3::{
    BftPrivateRetentionAdmissionInputV1, BftPrivateRetentionAttestorV1,
    BftPrivateRetentionCreationMarkerV1, BftPrivateRetentionSourceV1, BftSignerProviderV3,
    BftSignerStateV3, BftSuccessorWindowArchiveRefV1, BftSuccessorWindowSourceV1,
    RemoteBftSignerProviderV3,
};
use std::time::Instant;

impl FormalBftHeaderLedgerExecutionV3 {
    pub(in super::super) fn original_source_archive_material(
        &self,
        binding: &BftSignerBindingV3,
        attestor: &BftPrivateRetentionAttestorV1,
        marker: &BftPrivateRetentionCreationMarkerV1,
    ) -> Result<(Vec<u8>, Vec<u8>), String> {
        read_authenticated_original(&self.ledger, &self.store, binding, attestor, marker, None)?
            .archive_material(&self.store)
    }
    #[cfg(test)]
    pub(crate) fn inject_covered_source_reservation_cut() {
        super::private_store::source_activation::inject_reservation_cut();
    }

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
        let marker = self.derive_covered_source_marker(binding, creation_identity)?;
        check_original_namespace(&self.ledger, &self.store, &marker)?;
        Ok(marker)
    }

    fn derive_covered_source_marker(
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
        Ok(marker)
    }

    pub(in super::super) fn original_source_for_candidate(
        &self,
        binding: &BftSignerBindingV3,
        attestor: &BftPrivateRetentionAttestorV1,
        marker: &BftPrivateRetentionCreationMarkerV1,
    ) -> Result<(String, std::collections::BTreeSet<String>), String> {
        if &self.covered_source_marker(binding, &marker.creation_identity)? != marker {
            return Err("candidate source marker differs from qualified H".into());
        }
        read_authenticated_original(&self.ledger, &self.store, binding, attestor, marker, None)?
            .inventory()
    }

    /// Write the local original WAL only after an authenticated provider lease
    /// proves its exact original Pending result. The staged archive is immutable input.
    #[expect(
        clippy::too_many_arguments,
        reason = "The independent source and lease boundaries remain explicit."
    )]
    pub(crate) fn reserve_first_covered_original(
        &mut self,
        binding: &BftSignerBindingV3,
        attestor: &BftPrivateRetentionAttestorV1,
        creation_identity: &str,
        source: &BftSuccessorWindowSourceV1,
        pin: &OriginalCandidateArchivePinV1,
        input: Arc<BftPrivateRetentionAdmissionInputV1>,
        provider: &RemoteBftSignerProviderV3,
        deadline: Instant,
    ) -> Result<(String, String, bool), String> {
        let marker = self.derive_covered_source_marker(binding, creation_identity)?;
        if input.key.height != marker.initial_height {
            return Err("first original Reserve requires the exact original H height".into());
        }
        let basename = super::private_store::source_activation::pending_original_basename(&input)?;
        let original = read_authenticated_original(
            &self.ledger,
            &self.store,
            binding,
            attestor,
            &marker,
            Some(&basename),
        )?;
        let (origin, _) = original.inventory()?;
        if pin.original_private_origin_digest() != origin {
            return Err(
                "first original Reserve archive does not bind the original private Origin".into(),
            );
        }
        pin.verify_first_price(
            self,
            &input,
            source,
            &marker,
            &attestor.creation_marker_digest,
        )?;
        let publication =
            original.prepare_pending_original(&input, &mut self.ledger, &self.store)?;
        let received = publication.is_received();
        if Instant::now() >= deadline {
            return Err("first original Reserve deadline expired before protected lease".into());
        }
        self.ledger.verify_carriers()?;
        let mut lease = provider
            .acquire(binding)
            .map_err(|error| error.to_string())?;
        let pending = lease
            .reserve_first_private_retention(attestor, &marker, &original.activation, &input)
            .map_err(|error| error.to_string())?;
        let hash = publication.publish(&self.ledger, &self.store, deadline)?;
        drop(lease);
        Ok((hash, pending.receipt.body.after_state_digest, received))
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

pub(in super::super) fn verify_archived_original_source(
    original: &[u8],
    capsule: &[u8],
    binding: &BftSignerBindingV3,
    attestor: &BftPrivateRetentionAttestorV1,
    marker: &BftPrivateRetentionCreationMarkerV1,
) -> Result<
    (
        std::collections::BTreeSet<String>,
        oasis7_distfs::protected_bft_v3::BftPrivateRetentionActivationV1,
    ),
    String,
> {
    super::private_store::source_activation::verify_archived_original(
        original, capsule, binding, attestor, marker,
    )
}
