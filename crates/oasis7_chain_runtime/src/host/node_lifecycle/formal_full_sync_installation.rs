//! Restore the exact saved installation without granting a network attempt.
use super::operation_snapshot::Snapshot as OperationSnapshot;
use super::*;
use std::time::Instant;

pub(super) struct VerifiedInstallation {
    cut: serde_json::Value,
    roots_digest: String,
    manifest_digest: String,
    pub(super) history_manifest_digest: Option<String>,
}

/// Constructed only from the independently replayed original target and the
/// exact saved decision. It authorizes admission checks, not free repair I/O.
pub(super) struct VerifiedRepair {
    identity: super::operation_snapshot::Identity,
    input_digest: String,
    pub(super) request: serde_json::Value,
}
impl VerifiedRepair {
    pub(super) fn prepare(
        target: attempt::FullSyncTargetV1,
        request: serde_json::Value,
        snapshot: &OperationSnapshot,
        cas: &LocalCasStore,
    ) -> Result<Self, String> {
        snapshot.validate()?;
        let installation = snapshot
            .installation
            .as_ref()
            .filter(|i| !i.completed)
            .ok_or("operation_recovery_required: terminal installation cannot resume I/O")?;
        let publication = snapshot
            .publication
            .as_ref()
            .ok_or("repair has no original decision")?;
        let original = serde_json::to_vec(&(&target.cut, target.anchor.head().record()))
            .map_err(|e| e.to_string())?;
        if snapshot.verified_target_json.as_deref() != Some(original.as_slice())
            || snapshot.original_request_json.as_deref()
                != Some(
                    serde_json::to_vec(&request)
                        .map_err(|e| e.to_string())?
                        .as_slice(),
                )
        {
            return Err(
                "operation_recovery_required: repair differs from authenticated originals".into(),
            );
        }
        let mut missing = std::collections::BTreeSet::new();
        let mut missing_bytes = 0u64;
        for hash in &installation.retained_cas_hashes {
            match verified_local_object(cas, hash)? {
                Some(bytes) if snapshot.materials.get(hash) == Some(&(bytes.len() as u64)) => {}
                Some(_) => {
                    return Err(
                        "operation_recovery_required: original material size changed".into(),
                    );
                }
                None => {
                    missing.insert(hash.clone());
                    missing_bytes = missing_bytes
                        .checked_add(
                            *snapshot
                                .materials
                                .get(hash)
                                .ok_or("original material charge missing")?,
                        )
                        .ok_or("repair material overflow")?;
                }
            }
        }
        let summary = cas_lifecycle::verify_repair_cas(cas, &missing)?;
        if summary["total_bytes"]
            .as_u64()
            .ok_or("CAS byte observation missing")?
            .checked_add(missing_bytes)
            .is_none_or(|total| total > cas_lifecycle::MAX_FULL_SYNC_CAS_BYTES)
        {
            return Err("operation_budget_exhausted: original repair CAS capacity".into());
        }
        Ok(Self {
            identity: snapshot.identity.clone(),
            input_digest: publication
                .installation_input_digest
                .clone()
                .ok_or("original installation inputs are not bound")?,
            request,
        })
    }

    pub(super) fn require_snapshot(&self, snapshot: &OperationSnapshot) -> Result<(), String> {
        snapshot.validate()?;
        if self.identity != snapshot.identity
            || snapshot
                .publication
                .as_ref()
                .and_then(|p| p.installation_input_digest.as_ref())
                != Some(&self.input_digest)
        {
            return Err("operation_recovery_required: repair decision changed".into());
        }
        Ok(())
    }
}
impl VerifiedInstallation {
    pub(super) fn require_snapshot(&self, snapshot: &OperationSnapshot) -> Result<(), String> {
        let installation = snapshot
            .installation
            .as_ref()
            .ok_or("original installation missing")?;
        let publication = snapshot
            .publication
            .as_ref()
            .ok_or("original publication missing")?;
        let original: serde_json::Value = serde_json::from_slice(
            snapshot
                .verified_target_json
                .as_deref()
                .ok_or("original target missing")?,
        )
        .map_err(|e| e.to_string())?;
        if original[0] != self.cut
            || super::super::local_io::digest(&installation.manifest)? != self.manifest_digest
            || super::super::local_io::digest(&installation.retained_cas_hashes)?
                != self.roots_digest
            || publication.intent_digest
                != super::super::local_io::digest(&(&self.cut, &self.roots_digest))?
            || publication.installation_input_digest.as_deref()
                != Some(
                    super::super::local_io::digest(&(
                        &installation.manifest,
                        &installation.retained_cas_hashes,
                    ))?
                    .as_str(),
                )
        {
            return Err(
                "operation_recovery_required: readback differs from original installation".into(),
            );
        }
        Ok(())
    }
}

impl FormalFullSyncSession {
    pub(super) fn confirm_installed(
        &self,
        restored: &RestoredFormalFullSyncWorldV1,
        deadline: Instant,
    ) -> Result<VerifiedInstallation, String> {
        self.check_installation_deadline(deadline)?;
        let bytes = oasis7_world_state::codec::snapshot_cbor(&restored.world.snapshot())
            .map_err(|e| e.to_string())?;
        if blake3::hash(&bytes).to_hex().as_str() != restored.cut.after_state_root {
            return Err(
                "operation_recovery_required: retained World differs from original target".into(),
            );
        }
        cas_lifecycle::verify_loaded_world_pins(&self.cas, &restored.retained_cas_hashes)?;
        let history = self.history.observation()?;
        let latest = self
            .history
            .last_verified_cut()
            .ok_or("installation lacks durable verified history")?;
        if !nondecreasing_cut(&restored.cut, latest) {
            return Err(
                "operation_recovery_required: original installation left verified history".into(),
            );
        }
        self.check_installation_deadline(deadline)?;
        Ok(VerifiedInstallation {
            cut: serde_json::to_value(&restored.cut).map_err(|e| e.to_string())?,
            roots_digest: super::super::local_io::digest(&restored.retained_cas_hashes)?,
            manifest_digest: super::super::local_io::digest(&restored.manifest)?,
            history_manifest_digest: history["manifest_digest"].as_str().map(str::to_owned),
        })
    }

    pub(super) fn recover_original_installation(
        &mut self,
        snapshot: &OperationSnapshot,
        deadline: Instant,
    ) -> Result<(RestoredFormalFullSyncWorldV1, VerifiedInstallation), String> {
        self.check_installation_deadline(deadline)?;
        // Validate the exact durable decision before any local write or root
        // replacement, including a damaged but otherwise legal CAS root set.
        snapshot.validate()?;
        if (self.shared_exchange.is_some() && !snapshot.repair_attempt)
            || snapshot.publication.is_none()
        {
            return Err(
                "operation_recovery_required: local recovery has no original decision".into(),
            );
        }
        let target = self
            .recovery_target
            .clone()
            .ok_or("original authenticated target missing")?;
        let installation = snapshot
            .installation
            .as_ref()
            .ok_or("original installation inputs missing")?;
        let manifest: FormalFullSyncManifestV1 =
            serde_json::from_value(installation.manifest.clone()).map_err(|e| e.to_string())?;
        manifest.validate()?;
        if manifest.cut != target.cut {
            return Err("operation_recovery_required: installation target changed".into());
        }
        // This flag also seals both network exits. Cache misses are explicit
        // blockers; they cannot invoke the ordinary unmetered HTTP client.
        if self.shared_exchange.is_none() {
            self.local_recovery_deadline = Some(deadline);
        }
        let mut budget = CasBudget::new(self.config.max_total_sync_bytes);
        let world =
            self.restore_candidate_world(&target.anchor, &target.cut, &manifest, &mut budget)?;
        if !budget.objects.is_subset(&installation.retained_cas_hashes) {
            return Err(
                "operation_recovery_required: recovered closure differs from saved roots".into(),
            );
        }
        for hash in &installation.retained_cas_hashes {
            self.check_installation_deadline(deadline)?;
            let bytes = verified_local_object(&self.cas, hash)?
                .ok_or("original installation object missing")?;
            if snapshot.materials.get(hash) != Some(&(bytes.len() as u64)) {
                return Err("operation_recovery_required: saved material size changed".into());
            }
        }
        // Re-enter the unchanged strict CAS/history validation after any repair;
        // absent original proof material never reaches the network repair gate.
        cas_lifecycle::verify_read_only_cas(&self.cas)?;
        let providers = self.config.providers();
        let client = providers::build_read_client(
            &self.config,
            providers.first().ok_or("missing provider")?,
            &self.node_private_key_hex,
            Duration::from_millis(self.config.request_timeout_ms),
        )?;
        let (_, replayed) = history_chain::PersistedFormalReadHistoryV1::verify_read_only(
            &self.config,
            &client,
            Arc::clone(&self.cas),
            self.cas.root(),
            Some(&target.cut),
            true,
        )?;
        if replayed.as_ref().map(|a| a.head().record()) != Some(target.anchor.head().record()) {
            return Err(
                "operation_recovery_required: repaired history differs from original target".into(),
            );
        }
        self.check_installation_deadline(deadline)?;
        self.require_operation_active()?;
        cas_lifecycle::replace_loaded_world_pins(&self.cas, &installation.retained_cas_hashes)
            .map_err(|e| e.to_string())?;
        let restored = RestoredFormalFullSyncWorldV1 {
            world,
            cut: target.cut,
            retained_cas_hashes: installation.retained_cas_hashes.clone(),
            manifest,
        };
        let receipt = self.confirm_installed(&restored, deadline)?;
        receipt.require_snapshot(snapshot)?;
        let latest = self
            .history
            .last_verified_cut()
            .cloned()
            .ok_or("verified history missing")?;
        self.status.last_verified_cut = Some(latest.clone());
        self.status.loaded_cut = Some(restored.cut.clone());
        self.status.lag_positions = Some(latest.position - restored.cut.position);
        self.status.catch_up_pending = latest.position > restored.cut.position;
        self.status.state = "qualified_full_state_read_only".into();
        self.status.state_root_verified = true;
        self.status.ready = true;
        self.status.error = None;
        self.check_installation_deadline(deadline)?;
        Ok((restored, receipt))
    }

    fn check_installation_deadline(&self, deadline: Instant) -> Result<(), String> {
        if self
            .cancellation
            .as_ref()
            .is_some_and(|flag| flag.load(Ordering::SeqCst))
        {
            return Err("operation_recovery_required: worker cancelled".into());
        }
        if Instant::now() >= deadline {
            return Err(if self.shared_exchange.is_some() {
                "attempt_expired"
            } else {
                "operation_reopen_expired"
            }
            .into());
        }
        Ok(())
    }
}
