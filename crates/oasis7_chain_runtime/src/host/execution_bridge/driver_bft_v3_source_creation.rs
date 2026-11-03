//! Actual H/OS-carrier preparation, before any managed private candidate promise.
use super::*;
#[path = "driver_bft_v3_source_activation.rs"]
pub(super) mod activation;
#[path = "driver_bft_v3_source_input_verifier.rs"]
mod input_verifier;
pub(super) use input_verifier::VerifiedSourceInputParentV1;
#[path = "driver_bft_v3_inherited_source_carry.rs"]
mod inherited_carry;
#[path = "driver_bft_v3_source_seal_verifier.rs"]
mod seal_verifier;
#[cfg(test)]
pub(crate) use activation::ActivateBftPrivateSourceV1;
use oasis7_distfs::protected_bft_v3::{
    BftPrivateRetentionEntryV1, BftPrivateRetentionMaterialVerifierV1, BftSignerErrorV3,
    BftSignerStateV3, ValidatedBftPrivateRetentionCreationV1,
};
use oasis7_distfs::protected_bft_v3::{
    BftPrivateRetentionFreshNamespaceV1, BftPrivateRetentionFreshRecoveryV1,
    BftPrivateRetentionFreshSeedV1, BftSignerBindingV3,
};

#[derive(Clone)]
pub(crate) struct FreshBftPrivateSourceInputsV1 {
    pub(crate) records: PathBuf,
    pub(crate) store: LocalCasStore,
    pub(crate) anchor: VerifiedFormalReadAnchorV1,
    pub(crate) target: BftProfileTargetV2,
    pub(crate) material: ClosedRecord,
    pub(crate) source: Arc<dyn BusinessIndexNodeReader + Send + Sync>,
}

/// This is an immutable bootstrap identity, deliberately not a NodeExecutionHook.
/// It cannot publish a candidate or expose a writable driver before activation.
pub(crate) struct PreparedBftPrivateSourceOriginV1 {
    records: PathBuf,
    store: LocalCasStore,
    inputs: Inputs,
    carriers: Arc<super::successor_carriers::InstallationRootV1>,
    origin: storage::Origin,
    origin_bytes: usize,
    creation_identity: String,
    recovery: BftPrivateRetentionFreshRecoveryV1,
    handoff: VerifiedBftHandoffTargetV2,
    schedule: BftHeaderHeightScheduleV3,
    material_identity: String,
}

impl PreparedBftPrivateSourceOriginV1 {
    #[cfg(test)]
    pub(crate) fn interrupt_before_h_retention_for_test(
        request: FreshBftPrivateSourceInputsV1,
        seed: BftPrivateRetentionFreshSeedV1,
    ) -> Result<Self, String> {
        struct Reset;
        impl Drop for Reset {
            fn drop(&mut self) {
                FAIL_BEFORE_H_RETENTION.with(|flag| flag.set(false));
            }
        }
        FAIL_BEFORE_H_RETENTION.with(|flag| flag.set(true));
        let _reset = Reset;
        Self::prepare(request, seed)
    }
    pub(crate) fn prepare(
        request: FreshBftPrivateSourceInputsV1,
        seed: BftPrivateRetentionFreshSeedV1,
    ) -> Result<Self, String> {
        let creation_identity = seed.creation_identity().to_owned();
        let recovery = seed.recovery_handle();
        let records = canonical_storage_path(&request.records)?;
        validate_storage_paths(&records, request.store.root())?;
        require_unpublished_private(&records)?;
        let inputs = Inputs {
            anchor: request.anchor,
            target: request.target,
            material: request.material,
            source: request.source,
        };
        let execution = FormalBftHeaderLedgerExecutionV3::open_for_fresh_node(
            &records,
            request.store.clone(),
            inputs.clone(),
            &creation_identity,
        )?;
        if execution.ledger.records().next().is_some() {
            return Err("fresh source cannot adopt an existing private history".into());
        }
        execution.ledger.require_completed()?;
        let origin = execution.ledger.origin()?.clone();
        if origin.source_creation_identity() != Some(creation_identity.as_str()) {
            return Err("fresh source Origin did not retain original creation identity".into());
        }
        let material_identity = blake3_hex(
            &serde_cbor::to_vec(&(
                "oasis7.formal.bft.node_material.v3",
                origin.digest()?,
                &creation_identity,
                origin.material_pin_digest(),
            ))
            .map_err(|_| "encode fresh source material identity")?,
        );
        let handoff = execution.origin.handoff.clone();
        let schedule = execution.origin.schedule.clone();
        let origin_bytes = execution.ledger.origin_reserved_bytes();
        let carriers = Arc::new(super::successor_carriers::InstallationRootV1::new(
            records.clone(),
        )?);
        drop(execution);
        let prepared = Self {
            records,
            store: request.store,
            inputs,
            carriers,
            origin,
            origin_bytes,
            creation_identity,
            recovery,
            handoff,
            schedule,
            material_identity,
        };
        prepared.verify_unchanged()?;
        Ok(prepared)
    }

    pub(crate) fn handoff(&self) -> &VerifiedBftHandoffTargetV2 {
        &self.handoff
    }
    pub(crate) fn initial_schedule(&self) -> &BftHeaderHeightScheduleV3 {
        &self.schedule
    }
    pub(crate) fn material_identity(&self) -> &str {
        &self.material_identity
    }
    pub(crate) fn creation_identity(&self) -> &str {
        &self.creation_identity
    }
    pub(crate) fn origin_digest(&self) -> Result<String, String> {
        self.origin.digest()
    }

    pub(crate) fn verify_unchanged(&self) -> Result<(), String> {
        let deadline = std::time::Instant::now()
            .checked_add(std::time::Duration::from_secs(120))
            .ok_or("source identity verification deadline overflow")?;
        self.carriers
            .verify_origin_carriers(&self.origin, deadline)?;
        let ledger = storage::Ledger::open(&self.records, false, self.origin_bytes)?;
        ledger.require_completed()?;
        if ledger.origin()? != &self.origin || ledger.records().next().is_some() {
            return Err("fresh source original initialization changed".into());
        }
        require_unpublished_private(&self.records)?;
        // Source/target/material are independently admitted H inputs. The pin
        // retained by the real initialization must still name those exact bytes.
        retain_origin_material(&self.store, &self.inputs.material, &self.origin, false)?;
        let handoff =
            verify_formal_bft_handoff_v2(&self.inputs.anchor, self.inputs.target.clone())?;
        if handoff.handoff_head() != self.handoff.handoff_head()
            || handoff.target_digest() != self.handoff.target_digest()
        {
            return Err("fresh source H changed".into());
        }
        self.carriers
            .verify_origin_carriers(&self.origin, deadline)?;
        Ok(())
    }

    /// Finalizes only the protected namespace binding. Managed private Origin
    /// publication remains unavailable until the original empty coverage receipt
    /// has been authenticated by the following activation stage.
    pub(crate) fn finalize_namespace(
        &self,
        binding: &BftSignerBindingV3,
    ) -> Result<BftPrivateRetentionFreshNamespaceV1, String> {
        self.verify_unchanged()?;
        binding.validate().map_err(|e| e.to_string())?;
        if binding.material_identity != self.material_identity
            || binding.handoff_record_id != self.handoff.handoff_head().record_id
            || binding.activation_digest != self.handoff.activation_digest()
            || binding.initial_schedule().digest()? != self.schedule.digest()?
        {
            return Err("fresh source binding differs from original initialization".into());
        }
        self.recovery
            .recover_seed()
            .bind_source(binding, self.origin.digest()?)
            .map_err(|e| e.to_string())
    }

    pub(crate) fn source_verifier(&self) -> ActualBftPrivateSourceCreationVerifierV1 {
        ActualBftPrivateSourceCreationVerifierV1 {
            records: self.records.clone(),
            store: self.store.clone(),
            inputs: self.inputs.clone(),
            carriers: self.carriers.clone(),
            origin_bytes: self.origin_bytes,
            active_admission_verifier: None,
        }
    }
}

#[cfg(test)]
thread_local! { static FAIL_BEFORE_H_RETENTION: std::cell::Cell<bool> = const { std::cell::Cell::new(false) }; }

#[cfg(test)]
pub(super) fn before_h_retention_for_test() -> Result<(), String> {
    if FAIL_BEFORE_H_RETENTION.with(|flag| flag.get()) {
        Err("injected fresh source initialization before H retention".into())
    } else {
        Ok(())
    }
}

/// Actual independent H/carrier/source-origin verification. Candidate admission
/// remains closed until the separately implemented pure plan/retention adapter.
pub(crate) struct ActualBftPrivateSourceCreationVerifierV1 {
    records: PathBuf,
    store: LocalCasStore,
    inputs: Inputs,
    carriers: Arc<super::successor_carriers::InstallationRootV1>,
    origin_bytes: usize,
    active_admission_verifier:
        Option<Arc<super::successor_verifier::ActualBftSuccessorWindowDriverVerifierV1>>,
}

impl BftPrivateRetentionMaterialVerifierV1 for ActualBftPrivateSourceCreationVerifierV1 {
    fn verify_source_creation(
        &self,
        binding: &BftSignerBindingV3,
        state: &BftSignerStateV3,
        creation: &ValidatedBftPrivateRetentionCreationV1,
        deadline: std::time::Instant,
    ) -> Result<(), BftSignerErrorV3> {
        self.verify_actual(binding, state, creation, deadline)
            .map_err(BftSignerErrorV3)
    }
    fn verify_reservation(
        &self,
        _: &BftSignerBindingV3,
        _: &BftSignerStateV3,
        _: &BftPrivateRetentionEntryV1,
        _: std::time::Instant,
    ) -> Result<(), BftSignerErrorV3> {
        Err(BftSignerErrorV3(
            "actual source candidate reservation adapter unavailable".into(),
        ))
    }
    fn verify_ready(
        &self,
        _: &BftSignerBindingV3,
        _: &BftSignerStateV3,
        _: &BftPrivateRetentionEntryV1,
        _: std::time::Instant,
    ) -> Result<(), BftSignerErrorV3> {
        Err(BftSignerErrorV3(
            "actual source candidate publication adapter unavailable".into(),
        ))
    }
    fn verify_reservation_input_with_context(
        &self,
        binding: &BftSignerBindingV3,
        input: &oasis7_distfs::protected_bft_v3::BftPrivateRetentionAdmissionInputV1,
        context: &oasis7_distfs::protected_bft_v3::BftPrivateRetentionVerificationContextV1<'_>,
        deadline: std::time::Instant,
    ) -> Result<(), BftSignerErrorV3> {
        if context.signer().state.active_window.is_some() {
            self.verify_fresh_active_input(binding, input, context, false, deadline)
                .map(|_| ())
                .map_err(BftSignerErrorV3)
        } else {
            self.verify_actual_input(binding, input, context, deadline)
                .map_err(BftSignerErrorV3)
        }
    }
    fn verify_seal_with_context(
        &self,
        binding: &BftSignerBindingV3,
        cut: &oasis7_distfs::protected_bft_v3::BftSuccessorWindowCutV1,
        context: &oasis7_distfs::protected_bft_v3::BftPrivateRetentionSealVerificationContextV1<'_>,
        deadline: std::time::Instant,
    ) -> Result<(), BftSignerErrorV3> {
        let result: Result<(), String> = (|| {
            context
                .validate_cut(cut)
                .map_err(|error| error.to_string())?;
            self.verify_actual_seal_cut(binding, context, deadline)?;
            for entry in context.current_entries() {
                successor_archive::live(deadline)?;
                if entry.lifecycle
                    != oasis7_distfs::protected_bft_v3::BftPrivateRetentionLifecycleV1::Ready
                {
                    return Err("source seal original is not Ready".into());
                }
                let oasis7_distfs::protected_bft_v3::BftPrivateRetentionFactV1::Some {
                    admission_digest,
                    ..
                } = &entry.fact
                else {
                    return Err("actual source seal cannot infer legacy None material".into());
                };
                let historical = context
                    .original_ready(&entry.key)
                    .map_err(|error| error.to_string())?;
                let input = historical
                    .reservation()
                    .inputs
                    .get(admission_digest)
                    .ok_or("source seal original reservation input missing")?;
                if input.key != entry.key
                    || input.fact().map_err(|error| error.to_string())? != entry.fact
                {
                    return Err("source seal original key/fact differs".into());
                }
                self.verify_historical_input(binding, input, &historical, deadline)?;
                node_execution::verify_source_admission_ready(&self.records, &self.store, input)?;
            }
            successor_archive::live(deadline)
        })();
        result.map_err(BftSignerErrorV3)
    }
    fn verify_ready_input_with_context(
        &self,
        binding: &BftSignerBindingV3,
        entry: &BftPrivateRetentionEntryV1,
        input: &oasis7_distfs::protected_bft_v3::BftPrivateRetentionAdmissionInputV1,
        context: &oasis7_distfs::protected_bft_v3::BftPrivateRetentionVerificationContextV1<'_>,
        deadline: std::time::Instant,
    ) -> Result<(), BftSignerErrorV3> {
        if context.signer().state.active_window.is_some() {
            return self
                .verify_fresh_active_ready(binding, entry, input, context, false, deadline)
                .map(|_| ())
                .map_err(BftSignerErrorV3);
        }
        let result = (|| {
            self.verify_actual_input(binding, input, context, deadline)?;
            if entry.key != input.key || entry.fact != input.fact().map_err(|e| e.to_string())? {
                return Err("source ready entry differs from atomic original input".into());
            }
            node_execution::verify_source_admission_ready(&self.records, &self.store, input)?;
            successor_archive::live(deadline)
        })();
        result.map_err(BftSignerErrorV3)
    }
    fn verify_signing_input_with_context(
        &self,
        binding: &BftSignerBindingV3,
        entry: &BftPrivateRetentionEntryV1,
        input: &oasis7_distfs::protected_bft_v3::BftPrivateRetentionAdmissionInputV1,
        context: &oasis7_distfs::protected_bft_v3::BftPrivateRetentionVerificationContextV1<'_>,
        purpose: oasis7_distfs::protected_bft_v3::BftPrivateRetentionSigningPurposeV1,
        deadline: std::time::Instant,
    ) -> Result<(), BftSignerErrorV3> {
        if context.signer().state.active_window.is_some()
            && context.active_parent()?.active_segment_origin().digest
                == input.key.ledger_origin_digest
        {
            return self.verify_fresh_active_ready(binding,entry,input,context,purpose == oasis7_distfs::protected_bft_v3::BftPrivateRetentionSigningPurposeV1::LocalProposal,deadline).map(|_|()).map_err(BftSignerErrorV3);
        }
        if context.signer().state.active_window.is_some() {
            self.verify_inherited_source_carry(binding, entry, input, context, purpose, deadline)
                .map_err(BftSignerErrorV3)
        } else {
            self.verify_ready_input_with_context(binding, entry, input, context, deadline)
        }
    }
    fn verify_selection_input_with_context(
        &self,
        binding: &BftSignerBindingV3,
        entry: &BftPrivateRetentionEntryV1,
        input: &oasis7_distfs::protected_bft_v3::BftPrivateRetentionAdmissionInputV1,
        context: &oasis7_distfs::protected_bft_v3::BftPrivateRetentionVerificationContextV1<'_>,
        deadline: std::time::Instant,
    ) -> Result<
        oasis7_distfs::protected_bft_v3::VerifiedBftPrivateRetentionSelectionIdentityV1,
        BftSignerErrorV3,
    > {
        if context.signer().state.active_window.is_some() {
            return self
                .verify_fresh_active_ready(binding, entry, input, context, false, deadline)
                .map(|grant| grant.into_selection_identity())
                .map_err(BftSignerErrorV3);
        }
        self.verify_ready_input_with_context(binding, entry, input, context, deadline)?;
        node_execution::verify_source_selection_current(&self.records, &self.store, input)
            .map_err(BftSignerErrorV3)?;
        node_execution::source_selection_identity(input).map_err(BftSignerErrorV3)
    }
}

impl ActualBftPrivateSourceCreationVerifierV1 {
    pub(crate) fn for_original_source(
        request: FreshBftPrivateSourceInputsV1,
    ) -> Result<Self, String> {
        let records = canonical_storage_path(&request.records)?;
        let carriers = Arc::new(super::successor_carriers::InstallationRootV1::new(
            records.clone(),
        )?);
        Ok(Self {
            records,
            store: request.store,
            inputs: Inputs {
                anchor: request.anchor,
                target: request.target,
                material: request.material,
                source: request.source,
            },
            carriers,
            origin_bytes: MAX_H_BYTES,
            active_admission_verifier: None,
        })
    }

    fn verify_actual(
        &self,
        binding: &BftSignerBindingV3,
        state: &BftSignerStateV3,
        creation: &ValidatedBftPrivateRetentionCreationV1,
        deadline: std::time::Instant,
    ) -> Result<(), String> {
        successor_archive::live(deadline)?;
        let ledger = storage::Ledger::open(&self.records, false, self.origin_bytes)?;
        ledger.require_completed()?;
        let origin = ledger.origin()?;
        self.carriers.verify_origin_carriers(origin, deadline)?;
        let marker = creation.marker();
        if origin.source_creation_identity() != Some(marker.creation_identity.as_str())
            || origin.digest()? != marker.ledger_origin_digest
        {
            return Err("actual source Origin is legacy or differs from original creation".into());
        }
        if ledger.records().next().is_some() || state.private_retention.is_some() {
            return Err(
                "actual source already has managed history; cannot infer fresh coverage".into(),
            );
        }
        require_unpublished_private(&self.records)?;
        let handoff =
            verify_formal_bft_handoff_v2(&self.inputs.anchor, self.inputs.target.clone())?;
        retain_origin_material(&self.store, &self.inputs.material, origin, false)?;
        let expected_material = blake3_hex(
            &serde_cbor::to_vec(&(
                "oasis7.formal.bft.node_material.v3",
                origin.digest()?,
                &marker.creation_identity,
                origin.material_pin_digest(),
            ))
            .map_err(|_| "encode actual source material identity")?,
        );
        if binding.material_identity != expected_material
            || binding.handoff_record_id != handoff.handoff_head().record_id
            || marker.source.binding_digest != binding.digest().map_err(|e| e.to_string())?
            || marker.activation_digest != handoff.activation_digest()
            || state != &BftSignerStateV3::from_handoff(binding).map_err(|e| e.to_string())?
        {
            return Err("actual source creation differs from original H/material/State".into());
        }
        self.carriers.verify_origin_carriers(origin, deadline)?;
        successor_archive::live(deadline)
    }
}

fn require_unpublished_private(records: &Path) -> Result<(), String> {
    let directory = records
        .join(storage::DIRECTORY)
        .join(storage::NODE_PRIVATE_DIRECTORY);
    match std::fs::symlink_metadata(&directory) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err("inspect original private source initialization".into()),
        Ok(metadata) => {
            if !metadata.is_dir() || metadata.file_type().is_symlink() {
                return Err("fresh source private directory type".into());
            }
            crate::controlled_live_config::require_private_owned(&directory, true)?;
            if std::fs::read_dir(directory)
                .map_err(|_| "inspect original private source inventory")?
                .next()
                .is_some()
            {
                return Err(
                    "fresh source cannot relabel existing private Origin or unsigned material"
                        .into(),
                );
            }
            Ok(())
        }
    }
}

#[path = "driver_bft_v3_fresh_active_service.rs"]
mod fresh_active_service;
