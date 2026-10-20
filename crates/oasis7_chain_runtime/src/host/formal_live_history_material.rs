//! Exact publication closure recovery after typed history replay.
use super::replay::ReplayedHistoryCut;
use super::replay::decode_role;
use super::*;

impl NativeHistoryResolver {
    pub(super) fn restore_exact_history_cut(
        &self,
        replayed: &ReplayedHistoryCut,
        deadline: Instant,
    ) -> Result<VerifiedNativeHistoricalRestoreV1, String> {
        let record = replayed.anchor.head().record();
        let historical_authority = self.authority_for_record(record).ok_or_else(|| {
            "HistoryUnavailable: exact origin authority is not in verified lineage".to_owned()
        })?;
        if historical_authority
            .authority_id()
            .map_err(|e| e.to_string())?
            != record.original_authority_id
            || historical_authority.definition().epoch != record.original_epoch
        {
            return Err("HistoryUnavailable: exact origin record authority binding differs".into());
        }
        if replayed.target_proof.as_ref().is_some_and(|proof| {
            proof.decision.record != *record
                || proof
                    .decision
                    .record
                    .record_id()
                    .map(|id| id != replayed.anchor.head().head().record_id)
                    .unwrap_or(true)
        }) {
            return Err("HistoryUnavailable: replayed origin proof differs from exact cut".into());
        }
        let permission = replayed
            .target_recovery
            .map(|index| &self.permissions[index]);
        let recovery = permission.map(|p| p.recovery().clone());
        if replayed
            .target_proof
            .as_ref()
            .is_some_and(|proof| proof.decision.branch != FormalCompletionBranchV1::Normal)
            && recovery.is_none()
        {
            return Err(
                "HistoryUnavailable: exact origin completion lacks recovery context".into(),
            );
        }
        let original = permission.map_or(historical_authority, |p| p.original_authority());
        let target = FormalHeadRefV1 {
            position: record.position,
            record_id: replayed.anchor.head().head().record_id.clone(),
        };
        let restored = self.restore_native_position(
            target.position,
            Some(&target.record_id),
            historical_authority,
            original,
            recovery,
            deadline,
        )?;
        if restored.commit().record() != record
            || restored.commit().finalizing_authority_id()
                != replayed.anchor.head().finalizing_authority_id()
            || replayed
                .target_proof
                .as_ref()
                .is_some_and(|proof| restored.proof() != proof)
        {
            return Err(
                "HistoryUnavailable: native full closure differs from replayed exact cut".into(),
            );
        }
        Ok(restored)
    }

    pub(super) fn publication_from_replayed_cut(
        &self,
        replayed: ReplayedHistoryCut,
    ) -> Result<FormalReadPublication, String> {
        let material = self.material_for_replayed_cut(&replayed)?;
        let world = self.world_for(&replayed.anchor, &material)?;
        FormalReadPublication::new(replayed.anchor, world)
    }

    pub(super) fn restore_native_position(
        &self,
        position: u64,
        expected_record_id: Option<&str>,
        historical_authority: &VerifiedAuthorityV1,
        original: &VerifiedAuthorityV1,
        recovery: Option<oasis7_distfs::controlled_authority::formal::VerifiedRecovery>,
        deadline: Instant,
    ) -> Result<VerifiedNativeHistoricalRestoreV1, String> {
        if Instant::now() >= deadline {
            return Err("HistoryUnavailable: native restore deadline exceeded".into());
        }
        let minimum = self.external_origin.head().head().clone();
        let restore = |address, role| {
            let mut endpoint = RemoteFormalEndpoint::new(
                address,
                self.timeout,
                self.active.clone(),
                role,
                self.authentication.clone(),
                minimum.clone(),
            )
            .map_err(|error| error.to_string())?;
            endpoint.set_operation_deadline(deadline);
            let restored = if let Some(record_id) = expected_record_id {
                endpoint.restore_native_at_cached(
                    &FormalHeadRefV1 {
                        position,
                        record_id: record_id.to_owned(),
                    },
                    historical_authority,
                    original,
                    recovery.as_ref(),
                    &*self.business_reader,
                )
            } else {
                endpoint.locate_native_position_cached(
                    position,
                    historical_authority,
                    original,
                    recovery.as_ref(),
                    &*self.business_reader,
                )
            }
            .map_err(|error| error.to_string())?;
            if Instant::now() >= deadline {
                return Err("HistoryUnavailable: native restore deadline exceeded".into());
            }
            Ok::<_, String>(restored)
        };
        let primary = restore(self.addresses[0], EndpointRole::Primary)?;
        let replica = restore(self.addresses[1], EndpointRole::Replica)?;
        if primary.commit().record() != replica.commit().record()
            || primary.candidate() != replica.candidate()
            || primary.proof() != replica.proof()
            || primary.commit().record().position != position
            || primary.minimum() != &minimum
            || replica.minimum() != &minimum
        {
            return Err("OutcomeUnknown: native historical replicas disagree at exact cut".into());
        }
        Ok(primary)
    }

    pub(super) fn authority_for_record(
        &self,
        record: &oasis7_distfs::controlled_authority::formal::CanonicalRecordV1,
    ) -> Option<&VerifiedAuthorityV1> {
        if self
            .external_origin
            .authority()
            .authority_id()
            .ok()
            .as_deref()
            == Some(record.original_authority_id.as_str())
        {
            return Some(self.external_origin.authority());
        }
        for permission in &self.permissions {
            let recovery = permission.recovery();
            for authority in [
                permission.original_authority(),
                recovery.predecessor(),
                recovery.successor(),
            ] {
                if authority.authority_id().ok().as_deref()
                    == Some(record.original_authority_id.as_str())
                {
                    return Some(authority);
                }
            }
        }
        (self.active.authority_id().ok().as_deref() == Some(record.original_authority_id.as_str()))
            .then_some(&self.active)
    }

    pub(super) fn restore(
        &self,
        position: u64,
        expected: Option<&str>,
    ) -> Result<VerifiedNativeHistoricalRestoreV1, String> {
        self.restore_before(position, expected, Instant::now() + self.timeout)
    }
    pub(super) fn restore_before(
        &self,
        position: u64,
        expected: Option<&str>,
        deadline: Instant,
    ) -> Result<VerifiedNativeHistoricalRestoreV1, String> {
        if position < self.trusted.head().record().position {
            return Err(
                "HistoryUnavailable: requested cut precedes externally configured minimum".into(),
            );
        }
        let authority = &self.active;
        let original = self
            .permission
            .as_ref()
            .map_or(authority, |p| p.original_authority());
        let at_cut = self
            .permission
            .as_ref()
            .is_some_and(|p| p.cut().record().position == position);
        let historical = if at_cut { original } else { authority };
        let recovery = self.permission.as_ref().map(|p| p.recovery());
        // Each independent restore begins at the same external minimum. The
        // RPC client advances its transport floor after every response, so a
        // reused client would silently change the publication trust boundary.
        let restore = |address, role| {
            let mut endpoint = RemoteFormalEndpoint::new(
                address,
                self.timeout,
                authority.clone(),
                role,
                self.authentication.clone(),
                self.trusted.head().head(),
            )
            .map_err(|error| error.to_string())?;
            endpoint.set_operation_deadline(deadline);
            if let Some(record_id) = expected {
                endpoint.restore_native_at_cached(
                    &FormalHeadRefV1 {
                        position,
                        record_id: record_id.into(),
                    },
                    historical,
                    original,
                    recovery,
                    &*self.business_reader,
                )
            } else {
                endpoint.locate_native_position_cached(
                    position,
                    historical,
                    original,
                    recovery,
                    &*self.business_reader,
                )
            }
            .map_err(|error| error.to_string())
        };
        let primary = restore(self.addresses[0], EndpointRole::Primary)?;
        let replica = restore(self.addresses[1], EndpointRole::Replica)?;
        if primary.commit().record() != replica.commit().record()
            || primary.candidate() != replica.candidate()
            || primary.proof() != replica.proof()
        {
            return Err("OutcomeUnknown: native historical replicas disagree".into());
        }
        Ok(primary)
    }
    pub(super) fn publication(
        &self,
        restored: &VerifiedNativeHistoricalRestoreV1,
    ) -> Result<FormalReadPublication, String> {
        let anchor = self.anchor_for(restored)?;
        let world = self.world_for(&anchor, restored.material())?;
        FormalReadPublication::new(anchor, world)
    }
    pub(super) fn anchor_for(
        &self,
        restored: &VerifiedNativeHistoricalRestoreV1,
    ) -> Result<VerifiedFormalReadAnchorV1, String> {
        if let Some(permission) = &self.permission
            && restored.commit().head() == permission.cut().head()
        {
            if restored.commit().record() != permission.cut().record()
                || restored.minimum() != &self.trusted.head().head()
            {
                return Err("historical imported cut differs from checked recovery".into());
            }
            return Ok(self.trusted.clone());
        }
        restored
            .service_read_anchor(&self.trusted)
            .map_err(|e| e.to_string())
    }
    pub(super) fn world_for(
        &self,
        anchor: &VerifiedFormalReadAnchorV1,
        material: &ClosedRecord,
    ) -> Result<World, String> {
        let snapshot: Snapshot = decode_role(material, ArtifactRole::Snapshot)?;
        let journal: Journal = decode_role(material, ArtifactRole::Journal)?;
        let world = if snapshot.state.retained_history_root.is_some() {
            let bytes = serde_cbor::to_vec(&snapshot).map_err(|e| e.to_string())?;
            let parent =
                oasis7_runtime::VerifiedFormalRetainedHistoryParentV1::verify(anchor, &bytes)?;
            oasis7::runtime_host::world::from_verified_formal_snapshot(
                snapshot,
                journal,
                parent,
                self.business_reader.clone(),
            )
        } else {
            if anchor.authority().definition().profile_version == 2 {
                return Err(
                    "profile2 historical restore requires retained-history descriptor".into(),
                );
            }
            oasis7::runtime_host::world::from_snapshot(snapshot, journal)
        }
        .map_err(|e| format!("{e:?}"))?
        .with_release_security_policy(self.release.clone());
        Ok(world)
    }

    pub(super) fn material_for_replayed_cut(
        &self,
        replayed: &ReplayedHistoryCut,
    ) -> Result<ClosedRecord, String> {
        let anchor = &replayed.anchor;
        let record = anchor.head().record();
        let (candidate, cached_proof) = super::super::execution_bridge::formal_writer::read_published_candidate_at(
            &self.writer_directory,
            record.position,
        )?
        .ok_or_else(|| {
            format!(
                "HistoryUnavailable: exact local publication material is missing at position {}",
                record.position
            )
        })?;
        let proof = if let Some(proof) = &replayed.target_proof {
            if &cached_proof != proof {
                return Err(
                    "HistoryUnavailable: local proof differs from externally replayed exact cut"
                        .into(),
                );
            }
            proof
        } else {
            let verified = verify_formal_qualified(&cached_proof, anchor.authority(), None, None)
                .map_err(|error| error.to_string())?;
            if verified.head() != anchor.head().head() {
                return Err(
                    "HistoryUnavailable: local initial proof differs from exact anchor".into(),
                );
            }
            &cached_proof
        };
        let record_id = anchor.head().head().record_id.clone();
        let receipts_match = [
            (&proof.decision.primary.body, EndpointRole::Primary),
            (&proof.decision.replica.body, EndpointRole::Replica),
        ]
        .into_iter()
        .all(|(receipt, role)| {
            receipt.role == role
                && receipt.position == record.position
                && receipt.record_id.as_str() == record_id.as_str()
                && receipt.finalizing_authority_id == anchor.head().finalizing_authority_id()
                && receipt.branch == proof.decision.branch
                && receipt.recovery_certificate_digest
                    == proof.decision.primary.body.recovery_certificate_digest
        });
        if candidate.candidate.record != *record
            || proof.decision.record != *record
            || candidate.branch != proof.decision.branch
            || candidate.candidate.writer_signature_hex != proof.decision.writer_signature_hex
            || candidate.frozen != proof.decision.frozen
            || !receipts_match
        {
            return Err(
                "HistoryUnavailable: local publication material differs from exact qualified cut"
                    .into(),
            );
        }
        verify_record_material(record, &candidate.material)
            .map_err(|error| format!("HistoryUnavailable: local publication closure: {error}"))?;
        match (&candidate.frozen, &candidate.frozen_material) {
            (Some(certificate), Some(material)) => {
                verify_frozen_material(&certificate.reservation.body, material)
                    .map_err(|error| format!("HistoryUnavailable: local Frozen closure: {error}"))?
            }
            (None, None) if record.kind == FormalRecordKindV1::InitialActivation => {}
            _ => {
                return Err(
                    "HistoryUnavailable: local publication Frozen closure is incomplete".into(),
                );
            }
        }
        Ok(candidate.material)
    }

    pub(super) fn anchor_ref_for_position(
        &self,
        position: u64,
        expected_record_id: Option<&str>,
    ) -> Result<FormalReadAnchorRefV1, String> {
        if position == self.external_origin.head().record().position {
            let anchor = &self.external_origin;
            let record_id = anchor.head().head().record_id.clone();
            if expected_record_id.is_some_and(|expected| expected != record_id.as_str()) {
                return Err(
                    "HistoryUnavailable: requested commit differs from external origin".into(),
                );
            }
            return Ok(FormalReadAnchorRefV1 {
                position,
                record_id,
                active_authority_id: anchor
                    .authority()
                    .authority_id()
                    .map_err(|error| error.to_string())?,
            });
        }
        let proof = super::super::execution_bridge::formal_writer::read_published_proof_at(
            &self.writer_directory,
            position,
        )?
        .ok_or_else(|| {
            format!("HistoryUnavailable: exact publication proof is missing at position {position}")
        })?;
        let record = &proof.decision.record;
        let record_id = record.record_id().map_err(|error| error.to_string())?;
        if record.position != position
            || expected_record_id.is_some_and(|expected| expected != record_id.as_str())
        {
            return Err("HistoryUnavailable: requested commit differs from exact proof".into());
        }

        // Replay independently verifies whichever signed transition selects
        // the active authority at this position.
        let mut active_authority_id = record.original_authority_id.clone();
        let recovery_successors = self
            .permissions
            .iter()
            .filter(|permission| {
                permission.cut().record().position == position
                    && permission.cut().head().record_id.as_str() == record_id.as_str()
            })
            .map(|permission| {
                permission
                    .recovery()
                    .successor()
                    .authority_id()
                    .map_err(|error| error.to_string())
            })
            .collect::<Result<Vec<_>, _>>()?;
        if recovery_successors.len() > 1 {
            return Err("HistoryUnavailable: ambiguous recovery authority at exact cut".into());
        }
        if let Some(recovery_successor) = recovery_successors.first() {
            active_authority_id = recovery_successor.clone();
        }
        if let Some(marker) = self.activation_markers.read(position)? {
            if !recovery_successors.is_empty() {
                return Err(
                    "HistoryUnavailable: healthy activation overlaps recovery at exact cut".into(),
                );
            }
            active_authority_id = marker
                .step
                .transition
                .body
                .successor
                .authority_id()
                .map_err(|error| error.to_string())?;
        }
        Ok(FormalReadAnchorRefV1 {
            position,
            record_id,
            active_authority_id,
        })
    }

    pub(super) fn publication_from_exact_replay(
        &self,
        requested: &FormalReadAnchorRefV1,
    ) -> Result<FormalReadPublication, String> {
        let deadline = Instant::now()
            .checked_add(self.timeout)
            .ok_or("HistoryUnavailable: exact publication replay deadline overflow")?;
        let replayed = self.replay_exact_origin(requested, deadline)?;
        if Instant::now() >= deadline {
            return Err("HistoryUnavailable: exact publication replay deadline exceeded".into());
        }
        self.publication_from_replayed_cut(replayed)
    }
}
