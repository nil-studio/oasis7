use super::*;

fn require_original_recovery_control(
    candidate: &FormalCandidateMaterialV1,
    permission: &VerifiedRecoveryBootstrapV1,
    anchor: &VerifiedFormalReadAnchorV1,
) -> Result<(), String> {
    if !matches!(
        candidate.candidate.record.kind,
        FormalRecordKindV1::RecoveryActivation { .. }
    ) || candidate.branch != FormalCompletionBranchV1::Normal
        || candidate.frozen.is_some()
        || candidate.frozen_material.is_some()
        || anchor.head().head() != permission.recovery().body().sealed_cut
    {
        return Err("recovery endpoint asymmetry is not the exact planned control record".into());
    }
    verify_formal_candidate(
        &candidate.candidate,
        None,
        anchor.authority(),
        Some(anchor.head()),
        None,
        candidate.branch,
    )
    .map_err(|error| error.to_string())?;
    verify_record_material(&candidate.candidate.record, &candidate.material)
        .map_err(|error| error.to_string())?;
    Ok(())
}

impl<P: FormalDurabilityEndpoint, R: FormalDurabilityEndpoint> FormalWriter<P, R> {
    #[expect(
        clippy::too_many_arguments,
        reason = "Recovery opening validates the original pin, permission, dual restores, endpoint pair, and authenticated reader independently."
    )]
    pub(crate) fn open_native_recovery(
        root: PathBuf,
        writer: impl Into<WriterSigningCapabilityV1>,
        release: ReleaseSecurityPolicy,
        trusted: VerifiedFormalReadAnchorV1,
        permission: &VerifiedRecoveryBootstrapV1,
        primary_restore: &VerifiedNativeCheckpointRestoreV1,
        replica_restore: &VerifiedNativeCheckpointRestoreV1,
        primary: P,
        replica: R,
        reader: std::sync::Arc<dyn business_index::BusinessIndexNodeReader + Send + Sync>,
    ) -> Result<Self, String> {
        Self::open_native_context(
            root,
            writer,
            release,
            trusted,
            Some(permission),
            primary_restore,
            replica_restore,
            primary,
            replica,
            reader,
        )
    }
    #[expect(
        clippy::too_many_arguments,
        reason = "The restore context preserves independently verified recovery and dual-endpoint evidence."
    )]
    pub(super) fn open_native_context(
        root: PathBuf,
        writer: impl Into<WriterSigningCapabilityV1>,
        release: ReleaseSecurityPolicy,
        trusted: VerifiedFormalReadAnchorV1,
        permission: Option<&VerifiedRecoveryBootstrapV1>,
        primary_restore: &VerifiedNativeCheckpointRestoreV1,
        replica_restore: &VerifiedNativeCheckpointRestoreV1,
        primary: P,
        replica: R,
        reader: std::sync::Arc<dyn business_index::BusinessIndexNodeReader + Send + Sync>,
    ) -> Result<Self, String> {
        let lineage = if let Some(permission) = permission {
            if permission.external_anchor().checkpoint() != trusted.checkpoint() {
                return Err("recovery external origin mismatch".into());
            }
            trusted
                .with_pending_recovery(permission.recovery())
                .map_err(|e| e.to_string())?
        } else {
            trusted.clone()
        };
        let mut anchor = primary_restore
            .advance_read_anchor(&lineage)
            .map_err(|e| e.to_string())?;
        let mut other = replica_restore
            .advance_read_anchor(&lineage)
            .map_err(|e| e.to_string())?;
        if let Some(permission) = permission {
            if permission.external_anchor().checkpoint() != trusted.checkpoint()
                || primary_restore.active_authority().definition()
                    != permission.recovery().successor().definition()
                || replica_restore.active_authority().definition()
                    != permission.recovery().successor().definition()
            {
                return Err(
                    "native recovery exact external origin/active authority mismatch".into(),
                );
            }
            for restored in [&mut anchor, &mut other] {
                if restored
                    .authority()
                    .authority_id()
                    .map_err(|error| error.to_string())?
                    != permission
                        .recovery()
                        .successor()
                        .authority_id()
                        .map_err(|error| error.to_string())?
                {
                    *restored = restored
                        .with_pending_recovery(permission.recovery())
                        .map_err(|error| error.to_string())?;
                }
            }
        }
        if anchor
            .authority()
            .authority_id()
            .map_err(|e| e.to_string())?
            != other
                .authority()
                .authority_id()
                .map_err(|e| e.to_string())?
        {
            return Err("native endpoint active authorities differ".into());
        }
        let mut source = primary_restore;
        let mut activate = false;
        let mut observed_control = Vec::new();
        if let Some(permission) =
            permission.filter(|permission| permission.recovery().body().schema_version == 3)
        {
            let cut = &permission.recovery().body().sealed_cut;
            let primary_at_cut = anchor.head().head() == *cut;
            let replica_at_cut = other.head().head() == *cut;
            if primary_at_cut || replica_at_cut {
                activate = true;
                if primary_at_cut && replica_at_cut {
                    if primary_restore.candidate() != replica_restore.candidate()
                        || primary_restore.proof() != replica_restore.proof()
                    {
                        return Err(
                            "recovery endpoints disagree on the original drained publication"
                                .into(),
                        );
                    }
                } else {
                    let advanced = if primary_at_cut {
                        replica_restore.candidate()
                    } else {
                        source = replica_restore;
                        anchor = other.clone();
                        primary_restore.candidate()
                    };
                    // This is the single permissible asymmetric startup: one
                    // already-qualified control publication, with its exact C
                    // still retained by the other endpoint. No endpoint is
                    // rewound and no business result is selected by height.
                    require_original_recovery_control(advanced, permission, &anchor)?;
                    observed_control.push(advanced);
                }
                for restored in [primary_restore, replica_restore] {
                    if let Some(state) = restored.pending() {
                        let candidate = state.candidate().ok_or(
                            "recovery activation cannot overwrite an unresolved Frozen original",
                        )?;
                        require_original_recovery_control(candidate, permission, &anchor)?;
                        observed_control.push(candidate);
                    }
                }
            }
        }
        if !activate
            && (anchor.head().head() != other.head().head()
                || primary_restore.candidate() != replica_restore.candidate()
                || primary_restore.proof() != replica_restore.proof())
        {
            return Err(
                "OutcomeUnknown: native endpoint heads require original-request reconciliation"
                    .into(),
            );
        }
        let world = restore_qualified(source.candidate(), &release, &anchor, reader.clone())?;
        let publication = Published {
            schema_version: 1,
            candidate: source.candidate().clone(),
            proof: source.proof().clone(),
        };
        let mut pending = if activate {
            None
        } else {
            restored_pending(primary_restore.pending())?
        };
        if !activate && let Some(other) = restored_pending(replica_restore.pending())? {
            if let Some(first) = &pending {
                if first.original != other.original
                    || first.material != other.material
                    || first
                        .candidate
                        .as_ref()
                        .zip(other.candidate.as_ref())
                        .is_some_and(|(a, b)| a != b)
                {
                    return Err("native endpoints retain conflicting original requests".into());
                }
                if first.candidate.is_none() {
                    pending = Some(other);
                }
            } else {
                pending = Some(other);
            }
        }
        if let Some(permission) = permission
            && let Some(pending) = pending.as_mut()
        {
            fill_original_certificate(pending, permission)?;
        }
        let mut instance = Self::open_inner(
            root,
            writer,
            release,
            world,
            anchor,
            primary,
            replica,
            Some(NativePublication {
                publication,
                pending,
                recovery: permission.map(|context| context.recovery().clone()),
            }),
        )?
        .with_business_reader(reader)?;
        if activate {
            instance.complete_recovery_activation(
                permission.ok_or("recovery activation permission missing")?,
                &observed_control,
            )?;
        }
        Ok(instance)
    }

    pub(super) fn close_pending_reservation(&mut self) -> Result<(), String> {
        let pending = self
            .pending
            .as_ref()
            .ok_or("formal pending original missing")?
            .clone();
        let active = self
            .anchor
            .authority()
            .authority_id()
            .map_err(|e| e.to_string())?;
        if let Some(environment) =
            super::super::formal_native_environment::descriptor(&pending.material)?
        {
            let (primary, replica) = self.coordinator.endpoints_mut();
            super::super::formal_native_environment::verify_registered(
                primary,
                replica,
                &environment,
            )?;
        }
        if pending.original.body.authority_id != active {
            let recovery = self
                .recovery
                .as_ref()
                .ok_or("old original lacks verified recovery")?;
            if pending.original.body.authority_id
                != recovery
                    .original_authority()
                    .authority_id()
                    .map_err(|e| e.to_string())?
            {
                return Err("pending recovery original authority mismatch".into());
            }
            if pending.candidate.is_some() {
                // The native restore authenticated the retained Prepared entry;
                // never replace it with a new Frozen reservation or execute it.
                return Ok(());
            }
            let certificate = pending
                .frozen_certificate
                .ok_or("recovery original Frozen certificate missing")?;
            if certificate.reservation != pending.original {
                return Err("recovery Frozen original mismatch".into());
            }
            return self
                .coordinator
                .replay_frozen(certificate, pending.material)
                .map(|_| ())
                .map_err(|e| e.to_string());
        }
        self.coordinator
            .freeze(pending.original, pending.material)
            .map(|_| ())
            .map_err(|e| e.to_string())
    }

    pub(super) fn candidate_for_resume(&mut self) -> Result<FormalCandidateMaterialV1, String> {
        let timing = super::super::formal_execution_timing::FormalExecutionTiming::begin(
            "candidate",
            self.anchor.head().record().position,
        );
        let pending = self
            .pending
            .as_ref()
            .ok_or("formal original missing")?
            .clone();
        let active_id = self
            .anchor
            .authority()
            .authority_id()
            .map_err(|e| e.to_string())?;
        if let Some(environment) =
            super::super::formal_native_environment::descriptor(&pending.material)?
        {
            let (primary, replica) = self.coordinator.endpoints_mut();
            timing.step(
                "verify_native",
                Some(&pending.original.body.request_digest),
                || {
                    super::super::formal_native_environment::verify_registered(
                        primary,
                        replica,
                        &environment,
                    )
                },
            )?;
        }
        let recovery = self
            .recovery
            .as_ref()
            .filter(|_| pending.original.body.authority_id != active_id)
            .cloned();
        if let Some(recovery) = recovery {
            if pending.original.body.authority_id
                != recovery
                    .original_authority()
                    .authority_id()
                    .map_err(|error| error.to_string())?
            {
                return Err(
                    "formal pending original authority differs from checked recovery".into(),
                );
            }
            if let Some(mut candidate) = pending.candidate {
                // An already prepared replay of this successor keeps its exact
                // ReplayFrozen branch. A retained old candidate is completed
                // under the new authority without changing its signed body.
                if matches!(
                    recovery.body().tail,
                    RecoveryTailV1::Prepared { .. } | RecoveryTailV1::Decision { .. }
                ) {
                    candidate.branch = FormalCompletionBranchV1::CompletePrepared;
                }
                verify_formal_candidate(
                    &candidate.candidate,
                    candidate.frozen.as_ref(),
                    recovery.original_authority(),
                    Some(self.anchor.head()),
                    Some(&recovery),
                    candidate.branch,
                )
                .map_err(|error| error.to_string())?;
                return Ok(candidate);
            }
            let certificate = pending
                .frozen_certificate
                .ok_or("RecoveryUnavailable: original Frozen certificate missing")?;
            if certificate.reservation != pending.original {
                return Err("recovery original Frozen certificate mismatch".into());
            }
            let frozen = self
                .coordinator
                .replay_frozen(certificate, pending.material)
                .map_err(|error| error.to_string())?;
            return formal_executor::execute_recovery_replay(
                &frozen,
                &recovery,
                self.anchor.head(),
                &self.root.join("attempts"),
                &self.writer,
                self.business_reader.as_ref(),
                Some(&self.anchor),
            );
        }
        let executable = timing
            .step("dual_freeze", None, || {
                self.coordinator.freeze(pending.original, pending.material)
            })
            .map_err(|error| error.to_string())?;
        if let Some(candidate) = pending.candidate {
            return Ok(candidate);
        }
        timing.step(
            "execute_normal",
            Some(&executable.input().body.request_digest),
            || {
                formal_executor::execute_normal_with_business_reader(
                    &executable,
                    self.anchor.authority(),
                    self.anchor.head(),
                    &self.root.join("attempts"),
                    &self.writer,
                    self.business_reader.as_ref(),
                    Some(&self.anchor),
                )
            },
        )
    }
}

fn fill_original_certificate(
    pending: &mut Pending,
    permission: &VerifiedRecoveryBootstrapV1,
) -> Result<(), String> {
    if pending.original.body.authority_id
        == permission
            .recovery()
            .successor()
            .authority_id()
            .map_err(|error| error.to_string())?
    {
        return Ok(());
    }
    let frozen = match &permission.permission_evidence().tail {
        RecoveryTailWitnessV1::Empty => {
            return Err("empty recovery cannot import an old pending request".into());
        }
        RecoveryTailWitnessV1::Frozen(cert) => cert,
        RecoveryTailWitnessV1::Prepared { frozen, .. }
        | RecoveryTailWitnessV1::Decision { frozen, .. } => frozen,
    };
    if frozen.reservation != pending.original {
        return Err("checked recovery witness differs from native pending original".into());
    }
    if pending
        .frozen_certificate
        .as_ref()
        .is_some_and(|cert| cert != frozen)
    {
        return Err("native Frozen certificate conflicts with checked recovery".into());
    }
    pending.frozen_certificate = Some(frozen.clone());
    Ok(())
}
pub(super) fn pending_original_authority<'a>(
    anchor: &'a VerifiedFormalReadAnchorV1,
    recovery: Option<&'a VerifiedRecovery>,
    pending: &Pending,
) -> Result<&'a VerifiedAuthorityV1, String> {
    if pending.original.body.authority_id
        == anchor
            .authority()
            .authority_id()
            .map_err(|error| error.to_string())?
    {
        return Ok(anchor.authority());
    }
    let recovery = recovery.ok_or("old pending request lacks checked recovery context")?;
    if pending.original.body.authority_id
        != recovery
            .original_authority()
            .authority_id()
            .map_err(|error| error.to_string())?
    {
        return Err("pending original authority mismatch".into());
    }
    Ok(recovery.original_authority())
}

pub(super) fn same_candidate_payload(
    a: &FormalCandidateMaterialV1,
    b: &FormalCandidateMaterialV1,
) -> bool {
    a.candidate == b.candidate
        && a.material == b.material
        && a.frozen == b.frozen
        && a.frozen_material == b.frozen_material
}
