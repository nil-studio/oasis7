//! The first successor record changes authority, never the game's execution.
use super::*;
use oasis7_distfs::controlled_authority::formal::business_material::{
    FORMAL_BUSINESS_ENVELOPE_DOMAIN_V1, FormalBusinessResultEnvelopeV1,
};
use std::collections::BTreeMap;

impl<P: FormalDurabilityEndpoint, R: FormalDurabilityEndpoint> FormalWriter<P, R> {
    /// Called while the writer directory is exclusively held, before returning
    /// a writer to Node or opening admission. Both endpoints must retain the
    /// same qualified control record before the new epoch can execute a request.
    pub(super) fn complete_recovery_activation(
        &mut self,
        permission: &VerifiedRecoveryBootstrapV1,
        observed: &[&FormalCandidateMaterialV1],
    ) -> Result<(), String> {
        let result = self.complete_recovery_activation_inner(permission, observed);
        if result.is_err() {
            self.poisoned = true;
        }
        result
    }

    fn complete_recovery_activation_inner(
        &mut self,
        permission: &VerifiedRecoveryBootstrapV1,
        observed: &[&FormalCandidateMaterialV1],
    ) -> Result<(), String> {
        if self.poisoned
            || permission.recovery().body().schema_version != 3
            || self.anchor.head().head() != permission.recovery().body().sealed_cut
            || self.anchor.authority().definition()
                != permission.recovery().successor().definition()
        {
            return Err("recovery activation requires the exact drained cut and successor".into());
        }
        // A local terminal Frozen cache may outlive the old epoch's actual
        // qualification. Retire it only against that exact published result;
        // unresolved or substituted originals still block activation.
        if self.pending.is_some() {
            let cut = self.anchor.head().head().record_id;
            self.acknowledge(&cut)?;
        }
        let candidate = self.recovery_activation_candidate(permission)?;
        if observed.iter().any(|original| **original != candidate) {
            return Err("recovery activation differs from an endpoint's durable original".into());
        }
        let request = &candidate.candidate.record.request_digest;
        let path = self
            .root
            .join(format!("recovery-activation-{request}.cbor"));
        match std::fs::symlink_metadata(&path) {
            Ok(meta) if meta.is_file() && !meta.file_type().is_symlink() => {
                crate::controlled_live_config::require_private_owned(&path, false)?;
                let original: FormalCandidateMaterialV1 = read(&path)?;
                if original != candidate {
                    return Err("recovery activation local original conflicts with the plan".into());
                }
            }
            Ok(_) => return Err("recovery activation cache is not a private regular file".into()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let bytes = to_cbor(&candidate)?;
                capture::validate_package_budget(&bytes)?;
                durable_transaction::write_private_file_durable(&path, &bytes)?;
            }
            Err(error) => return Err(error.to_string()),
        }
        if let Some(environment) =
            super::super::formal_native_environment::descriptor(&candidate.material)?
        {
            let (primary, replica) = self.coordinator.endpoints_mut();
            super::super::formal_native_environment::verify_registered(
                primary,
                replica,
                &environment,
            )?;
        }
        let proof = self
            .coordinator
            .submit(candidate.clone())
            .map_err(|error| error.to_string())?;
        let head = verify_formal_qualified(
            &proof,
            self.anchor.authority(),
            Some(self.anchor.head()),
            None,
        )
        .map_err(|error| error.to_string())?;
        let next_anchor = self
            .anchor
            .after_verified_commit(self.anchor.authority().clone(), head)
            .map_err(|error| error.to_string())?;
        let reader = self
            .business_reader
            .as_ref()
            .ok_or("recovery activation business reader missing")?;
        let envelope = super::super::formal_business_package::envelope(&candidate.material, true)?
            .ok_or("recovery activation business envelope missing")?;
        for node in &envelope.delta.node_writes {
            reader.0.cache_content_node(&node.digest, &node.bytes)?;
        }
        let world = restore_qualified(&candidate, &self.release, &next_anchor, reader.0.clone())?;
        if snapshot_to_cbor(&world.snapshot())? != snapshot_to_cbor(&self.world.snapshot())?
            || super::super::journal_to_cbor(world.journal())?
                != super::super::journal_to_cbor(self.world.journal())?
        {
            return Err("recovery activation changed the original World or Journal".into());
        }
        let publication = Published {
            schema_version: 1,
            candidate,
            proof,
        };
        let bytes = to_cbor(&publication)?;
        capture::validate_package_budget(&bytes)?;
        durable_transaction::write_private_file_durable(
            &self
                .root
                .join("published")
                .join(format!("{:020}.cbor", next_anchor.head().record().position)),
            &bytes,
        )?;
        self.anchor = next_anchor;
        self.world = world;
        self.reclaim_published_attempts(&publication);
        Ok(())
    }

    fn recovery_activation_candidate(
        &self,
        permission: &VerifiedRecoveryBootstrapV1,
    ) -> Result<FormalCandidateMaterialV1, String> {
        let parent = self.anchor.head().record();
        let authority = self.anchor.authority();
        let authorization = authority
            .recovery_activation()
            .ok_or("recovery activation authorization missing")?;
        let publication: Published = read(
            &self
                .root
                .join("published")
                .join(format!("{:020}.cbor", parent.position)),
        )?;
        if publication.schema_version != 1
            || &publication.candidate.candidate.record != parent
            || publication.proof.decision.record != *parent
            || permission.cut().record() != parent
            || publication.proof.decision.writer_signature_hex
                != publication.candidate.candidate.writer_signature_hex
            || publication.proof.decision.branch != publication.candidate.branch
            || publication.proof.decision.frozen != publication.candidate.frozen
        {
            return Err(
                "recovery activation requires the exact original qualified publication".into(),
            );
        }
        verify_record_material(parent, &publication.candidate.material)
            .map_err(|error| error.to_string())?;
        let reader = self
            .business_reader
            .as_ref()
            .ok_or("recovery activation business reader missing")?;
        let previous = super::super::formal_business_package::tree(
            &publication.candidate.material,
            parent,
            &self.world,
            true,
        )?
        .ok_or("recovery activation predecessor business tree missing")?;
        let old_envelope =
            super::super::formal_business_package::envelope(&publication.candidate.material, true)?
                .ok_or("recovery activation predecessor Result missing")?;
        let mut material = publication.candidate.material.clone();
        let mut objects: BTreeMap<_, _> = material
            .objects
            .iter()
            .map(|object| (object.content_hash.clone(), object.clone()))
            .collect();
        let input = recovery_activation_input_bytes(&permission.permission_evidence().certificate)
            .map_err(|error| error.to_string())?;
        let payload = formal_package::insert(
            &mut objects,
            input,
            vec![material.roots[&ArtifactRole::Input].clone()],
        );
        material.roots.insert(ArtifactRole::Input, payload.clone());
        material.payload_digest = payload.clone();
        material.before_state_root = parent.after_state_root.clone();
        material.after_state_root = parent.after_state_root.clone();
        let mut record = parent.clone();
        record.schema_version = if material
            .roots
            .contains_key(&ArtifactRole::NativeExecutionEnvironment)
        {
            5
        } else {
            3
        };
        record.kind = FormalRecordKindV1::RecoveryActivation {
            certificate_digest: authorization.certificate_digest.clone(),
            plan_digest: authorization.plan_digest.clone(),
            pending_handoff_root: authorization.pending_handoff_root.clone(),
        };
        record.position = parent
            .position
            .checked_add(1)
            .ok_or("recovery activation position overflow")?;
        record.parent_record_id = parent.record_id().map_err(|error| error.to_string())?;
        record.original_authority_id = authority
            .authority_id()
            .map_err(|error| error.to_string())?;
        record.original_epoch = authority.definition().epoch;
        record.activation_ref = authority.activation_ref().to_owned();
        record.request_digest =
            recovery_activation_request_digest(authority).map_err(|error| error.to_string())?;
        record.payload_digest = payload;
        record.frozen_reservation_id = None;
        record.before_state_root = parent.after_state_root.clone();
        record.handoff = None;
        record.durable_set_transition_digest = None;
        let update = oasis7_world_service::formal_result::FormalBusinessResultTreeV1::extend_with_parent_proof(
            &self.world, &record, &previous, &publication.proof, &[], reader,
        )?;
        record.result_root = update.tree.root()?;
        let result = FormalBusinessResultEnvelopeV1 {
            schema_version: 1,
            domain: FORMAL_BUSINESS_ENVELOPE_DOMAIN_V1.into(),
            // This is the old Node boundary verbatim, not a new execution.
            execution_result_cbor: old_envelope.execution_result_cbor,
            tree_descriptor_cbor: update.tree.encode_cbor()?,
            delta: update.delta,
            retained_history_delta: None,
        }
        .encode()
        .map_err(|error| error.to_string())?;
        let result_ref = formal_package::insert(
            &mut objects,
            result,
            vec![
                material.roots[&ArtifactRole::Snapshot].clone(),
                material.roots[&ArtifactRole::Journal].clone(),
                material.roots[&ArtifactRole::Result].clone(),
            ],
        );
        material.roots.insert(ArtifactRole::Result, result_ref);
        material.objects = objects.into_values().collect();
        record.artifact_roots = material.roots.clone();
        verify_record_material(&record, &material).map_err(|error| error.to_string())?;
        let writer_signature_hex = self
            .writer
            .sign_candidate(&record, None)
            .map_err(|error| error.to_string())?;
        let candidate = FormalCandidateMaterialV1 {
            candidate: SignedCanonicalCandidateV1 {
                record,
                writer_signature_hex,
            },
            material,
            frozen: None,
            frozen_material: None,
            branch: FormalCompletionBranchV1::Normal,
        };
        verify_formal_candidate(
            &candidate.candidate,
            None,
            authority,
            Some(self.anchor.head()),
            None,
            candidate.branch,
        )
        .map_err(|error| error.to_string())?;
        Ok(candidate)
    }

    pub(crate) fn recovery_activation_has_initial_bootstrap(&self) -> Result<bool, String> {
        if !matches!(
            self.anchor.head().record().kind,
            FormalRecordKindV1::RecoveryActivation { .. }
        ) {
            return Err("recovery Node bootstrap requires an actual control head".into());
        }
        let publication: Published = read(
            &self
                .root
                .join("published")
                .join(format!("{:020}.cbor", self.anchor.head().record().position)),
        )?;
        if super::super::formal_business_package::execution_result::<
            super::super::formal_initial::InitialResultV1,
        >(&publication.candidate.material, true)
        .is_ok()
        {
            self.committed_initial_bootstrap()?;
            Ok(true)
        } else {
            self.committed_execution_record()?;
            Ok(false)
        }
    }
}
