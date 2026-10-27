//! Account-local separately approved Owner setup. Private originals never enter audit text.
use super::*;
use oasis7_custody_core::provisioning::*;
impl OwnerProvisionApprovalStore {
    pub fn owner_setup_plan(&self, id: &str) -> Result<OwnerSetupPlanV1> {
        self.snapshot()?
            .state
            .owner_setup_plan(id)
            .cloned()
            .ok_or_else(rejected)
    }
    pub fn owner_setup_outbox(
        &self,
        id: &str,
        stage: OwnerSetupStageV1,
    ) -> Result<OwnerSetupOutboxV1> {
        self.snapshot()?
            .state
            .owner_setup_outbox(id, stage)
            .cloned()
            .ok_or_else(rejected)
    }
    pub async fn begin_owner_setup<F>(
        &self,
        input: ApprovalAdmissionInput,
        plan: OwnerSetupPlanV1,
        current: F,
    ) -> Result<ApprovalAdmissionRecord>
    where
        F: Fn() -> Result<NativeStageCurrent>,
    {
        let saved = self.snapshot()?;
        let nonce = if let Some(old) = saved.state.owner_setup_original(&input.operation_id) {
            if old.input() != &input
                || saved.state.owner_setup_plan(&input.operation_id) != Some(&plan)
            {
                return Err(rejected());
            }
            old.verify_nonce().to_owned()
        } else {
            crate::approval_crypto::verification_nonce().map_err(|_| rejected())?
        };
        let admission = OwnerSetupAdmissionV1 {
            input,
            plan,
            verify_nonce: nonce,
        };
        let id = admission.input.operation_id.clone();
        self.native_verified(
            &current,
            "owner_setup_pending",
            &id,
            |state, _scope, context, time| {
                Ok(vec![
                    state
                        .begin_owner_setup_checks(&admission, context, time)
                        .map_err(|_| rejected())?,
                ])
            },
            |state, _scope, context, time, cache| {
                state
                    .begin_owner_setup(admission.clone(), context, time, cache)
                    .map_err(|_| rejected())
            },
        )
        .await
    }
    pub async fn commit_owner_setup<F>(
        &self,
        id: &str,
        signed: SignedApprovalVerificationV1,
        current: F,
    ) -> Result<OwnerSetupApprovalV1>
    where
        F: Fn() -> Result<NativeStageCurrent>,
    {
        let source_nonce = crate::approval_crypto::verification_nonce().map_err(|_| rejected())?;
        self.native_verified(
            &current,
            "owner_setup_consumed",
            id,
            |state, _scope, context, time| {
                Ok(vec![
                    state
                        .commit_owner_setup_checks(id, &signed, context, time)
                        .map_err(|_| rejected())?,
                ])
            },
            |state, scope, context, time, cache| {
                state
                    .commit_owner_setup(
                        CommitOwnerSetupV1 {
                            id: id.into(),
                            signed: signed.clone(),
                            source_nonce: source_nonce.clone(),
                        },
                        scope,
                        context,
                        time,
                        cache,
                    )
                    .map_err(|_| rejected())
            },
        )
        .await
    }
    pub fn claim_owner_setup_source<F>(
        &self,
        id: &str,
        current: F,
    ) -> Result<ApprovalVerifyRequestV1>
    where
        F: Fn() -> Result<NativeStageCurrent>,
    {
        self.native_step(
            &current,
            "claim_owner_setup_source",
            id,
            |state, _scope, context, time| {
                state
                    .claim_owner_setup_source(id, context, time)
                    .map_err(|_| rejected())
            },
        )
    }
    pub fn claim_owner_setup_stage_source<F>(
        &self,
        id: &str,
        stage: OwnerSetupStageV1,
        current: F,
    ) -> Result<ApprovalVerifyRequestV1>
    where
        F: Fn() -> Result<NativeStageCurrent>,
    {
        self.native_step(
            &current,
            "claim_owner_setup_stage_source",
            id,
            |state, _scope, context, time| {
                state
                    .claim_owner_setup_stage_source(id, stage, context, time)
                    .map_err(|_| rejected())
            },
        )
    }
    pub async fn complete_owner_setup_source<F>(
        &self,
        id: &str,
        signed: SignedApprovalVerificationV1,
        current: F,
    ) -> Result<()>
    where
        F: Fn() -> Result<NativeStageCurrent>,
    {
        self.native_verified(
            &current,
            "complete_owner_setup_source",
            id,
            |state, _scope, context, time| {
                Ok(vec![
                    state
                        .owner_setup_source_checks(id, &signed, context, time)
                        .map_err(|_| rejected())?,
                ])
            },
            |state, _scope, context, time, cache| {
                state
                    .complete_owner_setup_source(id, signed.clone(), context, time, cache)
                    .map_err(|_| rejected())
            },
        )
        .await
    }
    pub async fn complete_owner_setup_stage_source<F>(
        &self,
        id: &str,
        stage: OwnerSetupStageV1,
        signed: SignedApprovalVerificationV1,
        current: F,
    ) -> Result<()>
    where
        F: Fn() -> Result<NativeStageCurrent>,
    {
        self.native_verified(
            &current,
            "complete_owner_setup_stage_source",
            id,
            |state, _scope, context, time| {
                Ok(vec![
                    state
                        .owner_setup_stage_source_checks(id, stage, &signed, context, time)
                        .map_err(|_| rejected())?,
                ])
            },
            |state, _scope, context, time, cache| {
                state
                    .complete_owner_setup_stage_source(
                        id,
                        stage,
                        signed.clone(),
                        context,
                        time,
                        cache,
                    )
                    .map_err(|_| rejected())
            },
        )
        .await
    }
    pub fn reserve_owner_setup_stage(
        &self,
        id: &str,
        stage: OwnerSetupStageV1,
    ) -> Result<OwnerSetupOutboxV1> {
        let nonce = crate::approval_crypto::verification_nonce().map_err(|_| rejected())?;
        self.local_reservation("owner_setup_stage_reserved", id, |state, time| {
            state
                .reserve_owner_setup_stage(id, stage, &nonce, time)
                .map_err(|_| rejected())
        })
    }
    pub fn reserve_owner_setup_read(
        &self,
        id: &str,
        stage: OwnerSetupStageV1,
        purpose: OwnerSetupReadPurposeV1,
        static_role: oasis7_custody_core::http::Role,
    ) -> Result<OwnerSetupReadTicket> {
        self.local_reservation("owner_setup_read_reserved", id, |state, time| {
            state
                .reserve_owner_setup_read(id, stage, purpose, static_role, time)
                .map_err(|_| rejected())
        })
    }
    pub fn reserve_owner_setup_callback(&self, id: &str, stage: OwnerSetupStageV1) -> Result<()> {
        self.local_reservation("owner_setup_callback_reserved", id, |state, time| {
            state
                .reserve_owner_setup_callback(id, stage, time)
                .map_err(|_| rejected())
        })
    }
    pub async fn authorize_owner_setup_read<F>(
        &self,
        id: &str,
        read: OwnerSetupReadTicket,
        static_role: oasis7_custody_core::http::Role,
        current: F,
    ) -> Result<OwnerSetupPermitViewV1>
    where
        F: Fn() -> Result<NativeStageCurrent>,
    {
        if read.purpose != OwnerSetupReadPurposeV1::Session {
            return Err(rejected());
        }
        self.native_verified(
            &current,
            "owner_setup_read_authorized",
            id,
            |state, _scope, context, time| {
                Ok(vec![
                    state
                        .owner_setup_stage_checks(id, read.stage, context, time)
                        .map_err(|_| rejected())?,
                ])
            },
            |state, scope, context, time, cache| {
                let permit = state
                    .authorize_owner_setup_read(
                        id,
                        read,
                        static_role,
                        &OwnerSetupCurrent {
                            provision: context,
                            native_scope: scope,
                        },
                        time,
                        cache,
                    )
                    .map_err(|_| rejected())?;
                if read.ticket == 2 {
                    state
                        .claim_owner_setup_access(id, read.stage, time)
                        .map_err(|_| rejected())?;
                }
                Ok(permit)
            },
        )
        .await
    }

    pub fn mark_owner_setup_unknown(&self, id: &str, stage: OwnerSetupStageV1) -> Result<()> {
        self.tighten("owner_setup_unknown", |state| {
            state
                .mark_owner_setup_unknown(id, stage)
                .map_err(|_| rejected())
        })?;
        Ok(())
    }
}

impl OwnerProvisionApprovalStore {
    pub fn reserve_owner_setup_admission_callback(
        &self,
        id: &str,
        stage: OwnerSetupStageV1,
    ) -> Result<()> {
        self.local_reservation("owner_setup_admission_reserved", id, |state, time| {
            state
                .reserve_owner_setup_admission_callback(id, stage, time)
                .map_err(|_| rejected())
        })
    }
    pub async fn owner_setup_signing_permit<F>(
        &self,
        id: &str,
        read: OwnerSetupReadTicket,
        access: Option<OwnerSetupAccessEvidenceV1>,
        current: F,
    ) -> Result<(OwnerSetupPermitViewV1, OwnerSetupSigningBundleV1)>
    where
        F: Fn() -> Result<NativeStageCurrent>,
    {
        if read.purpose != OwnerSetupReadPurposeV1::Sign
            || (read.ticket == 3) != access.is_some()
            || !matches!(read.ticket, 3 | 4)
        {
            return Err(rejected());
        }
        self.native_verified(
            &current,
            "owner_setup_signing_read",
            id,
            |state, scope, context, time| {
                let current = OwnerSetupCurrent {
                    provision: context,
                    native_scope: scope,
                };
                let checks = if let Some(access) = &access {
                    state.owner_setup_access_checks(id, read.stage, access, &current, time)
                } else {
                    state.owner_setup_access_current_checks(id, read.stage, &current, time)
                }
                .map_err(|_| rejected())?;
                Ok(vec![checks])
            },
            |state, scope, context, time, cache| {
                let current = OwnerSetupCurrent {
                    provision: context,
                    native_scope: scope,
                };
                if let Some(access) = &access {
                    state
                        .record_owner_setup_access(
                            id,
                            read.stage,
                            access.clone(),
                            &current,
                            time,
                            cache,
                        )
                        .map_err(|_| rejected())?;
                }
                let permit = state
                    .authorize_owner_setup_read(
                        id,
                        read,
                        oasis7_custody_core::http::Role::Owner,
                        &current,
                        time,
                        cache,
                    )
                    .map_err(|_| rejected())?;
                let signing = state
                    .owner_setup_signing_request(id, read.stage, &current, time)
                    .map_err(|_| rejected())?;
                Ok((permit, signing))
            },
        )
        .await
    }

    pub async fn publish_owner_setup_original<F>(
        &self,
        id: &str,
        read: OwnerSetupReadTicket,
        candidate: OwnerSetupCandidateV1,
        current: F,
    ) -> Result<OwnerSetupPermitViewV1>
    where
        F: Fn() -> Result<NativeStageCurrent>,
    {
        self.native_verified(
            &current,
            "owner_setup_original_published",
            id,
            |state, scope, context, time| {
                Ok(vec![
                    state
                        .owner_setup_publish_checks(
                            id,
                            read.stage,
                            &candidate,
                            &OwnerSetupCurrent {
                                provision: context,
                                native_scope: scope,
                            },
                            time,
                        )
                        .map_err(|_| rejected())?,
                ])
            },
            |state, scope, context, time, cache| {
                let current = OwnerSetupCurrent {
                    provision: context,
                    native_scope: scope,
                };
                state
                    .publish_owner_setup_original(
                        id,
                        read,
                        candidate.clone(),
                        &current,
                        time,
                        cache,
                    )
                    .map_err(|_| rejected())?;
                state
                    .owner_setup_published_permit(id, read.stage, &current, time, cache)
                    .map_err(|_| rejected())
            },
        )
        .await
    }

    pub async fn complete_owner_setup_stage<F>(
        &self,
        id: &str,
        completion: OwnerSetupCompletionV1,
        current: F,
    ) -> Result<OwnerSetupOutboxV1>
    where
        F: Fn() -> Result<NativeStageCurrent>,
    {
        self.native_verified(
            &current,
            "owner_setup_result_committed",
            id,
            |state, scope, context, time| {
                Ok(vec![
                    state
                        .owner_setup_result_checks(
                            id,
                            completion.stage,
                            &completion.result,
                            &completion.session,
                            &OwnerSetupCurrent {
                                provision: context,
                                native_scope: scope,
                            },
                            time,
                        )
                        .map_err(|_| rejected())?,
                ])
            },
            |state, scope, context, time, cache| {
                state
                    .complete_owner_setup_stage(
                        id,
                        completion.clone(),
                        &OwnerSetupCurrent {
                            provision: context,
                            native_scope: scope,
                        },
                        time,
                        cache,
                    )
                    .map_err(|_| rejected())
            },
        )
        .await
    }

    pub async fn claim_owner_setup_lookup<F>(
        &self,
        id: &str,
        stage: OwnerSetupStageV1,
        current: F,
    ) -> Result<OwnerSetupPermitViewV1>
    where
        F: Fn() -> Result<NativeStageCurrent>,
    {
        self.native_verified(
            &current,
            "claim_owner_setup_lookup",
            id,
            |state, scope, context, time| {
                Ok(vec![
                    state
                        .owner_setup_stored_admission_checks(
                            id,
                            stage,
                            &OwnerSetupCurrent {
                                provision: context,
                                native_scope: scope,
                            },
                            time,
                        )
                        .map_err(|_| rejected())?,
                ])
            },
            |state, scope, context, time, cache| {
                state
                    .claim_owner_setup_lookup(
                        id,
                        stage,
                        &OwnerSetupCurrent {
                            provision: context,
                            native_scope: scope,
                        },
                        time,
                        cache,
                    )
                    .map_err(|_| rejected())?;
                state
                    .owner_setup_effect_permit(
                        id,
                        stage,
                        &OwnerSetupCurrent {
                            provision: context,
                            native_scope: scope,
                        },
                        time,
                        cache,
                    )
                    .map_err(|_| rejected())
            },
        )
        .await
    }
}

impl OwnerProvisionApprovalStore {
    /// Preflight evidence admission and the one-time effect claim commit together.
    pub async fn admit_and_claim_owner_setup_send<F>(
        &self,
        id: &str,
        stage: OwnerSetupStageV1,
        evidence: OwnerSetupAdmissionEvidenceV1,
        current: F,
    ) -> Result<OwnerSetupPermitViewV1>
    where
        F: Fn() -> Result<NativeStageCurrent>,
    {
        self.native_verified(
            &current,
            "owner_setup_effect_admitted",
            id,
            |state, scope, context, time| {
                Ok(vec![
                    state
                        .owner_setup_admission_checks(
                            id,
                            stage,
                            &evidence,
                            &OwnerSetupCurrent {
                                provision: context,
                                native_scope: scope,
                            },
                            time,
                        )
                        .map_err(|_| rejected())?,
                ])
            },
            |state, scope, context, time, cache| {
                let trusted = OwnerSetupCurrent {
                    provision: context,
                    native_scope: scope,
                };
                state
                    .record_owner_setup_admission(
                        id,
                        stage,
                        evidence.clone(),
                        &trusted,
                        time,
                        cache,
                    )
                    .map_err(|_| rejected())?;
                state
                    .claim_owner_setup_send(id, stage, &trusted, time, cache)
                    .map_err(|_| rejected())?;
                state
                    .owner_setup_effect_permit(id, stage, &trusted, time, cache)
                    .map_err(|_| rejected())
            },
        )
        .await
    }
}

impl OwnerProvisionApprovalStore {
    pub fn reserve_owner_setup_lookup(&self, id: &str, stage: OwnerSetupStageV1) -> Result<()> {
        self.local_reservation("owner_setup_lookup_reserved", id, |state, time| {
            state
                .reserve_owner_setup_lookup(id, stage, time)
                .map_err(|_| rejected())
        })
    }
}

impl OwnerProvisionApprovalStore {
    /// Retain only the exact signed canonical fact, including after a freeze or
    /// expired live observation. This cannot issue a permit or advance a phase.
    pub async fn archive_owner_setup_canonical(
        &self,
        id: &str,
        stage: OwnerSetupStageV1,
        fact: OwnerSetupCanonicalFactV1,
        world_public_key: &str,
    ) -> Result<OwnerSetupCanonicalFactV1> {
        let context = OwnerSetupArchiveContext { world_public_key };
        let checks = self
            .snapshot()?
            .state
            .owner_setup_archive_checks(id, stage, &fact, &context)
            .map_err(|_| rejected())?;
        let mut cache = verify(checks).await?;
        let mut returned = None;
        self.tighten("owner_setup_canonical_archived", |state| {
            let checks = state
                .owner_setup_archive_checks(id, stage, &fact, &context)
                .map_err(|_| rejected())?;
            cache.verify(&checks).map_err(|_| rejected())?;
            returned = Some(
                state
                    .archive_owner_setup_canonical(id, stage, fact.clone(), &context, &mut cache)
                    .map_err(|_| rejected())?,
            );
            Ok(())
        })?;
        returned.ok_or_else(rejected)
    }
}
