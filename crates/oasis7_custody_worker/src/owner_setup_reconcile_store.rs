//! Independently funded reconciliation transactions. Never reissues an effect request.
use super::*;
use oasis7_custody_core::provisioning::*;
impl OwnerProvisionApprovalStore {
    async fn reconcile_verified<F, R, C, T>(
        &self,
        current: &F,
        event: &str,
        id: &str,
        checks: C,
        mut transition: T,
    ) -> Result<R>
    where
        F: Fn() -> Result<NativeStageCurrent>,
        C: Fn(
            &AccountProvisionApprovalState,
            &OwnerSetupCurrent<'_, '_>,
            u64,
        ) -> Result<Vec<ExactSignatureChecks>>,
        T: FnMut(
            &mut AccountProvisionApprovalState,
            &OwnerSetupCurrent<'_, '_>,
            u64,
            &mut NativeExactCache,
        ) -> Result<R>,
    {
        self.native_verified(
            current,
            event,
            id,
            |state, scope, context, time| {
                checks(
                    state,
                    &OwnerSetupCurrent {
                        provision: context,
                        native_scope: scope,
                    },
                    time,
                )
            },
            |state, scope, context, time, cache| {
                transition(
                    state,
                    &OwnerSetupCurrent {
                        provision: context,
                        native_scope: scope,
                    },
                    time,
                    cache,
                )
            },
        )
        .await
    }
    pub fn reconcile_approval(&self, id: &str) -> Result<OwnerSetupReconcileApprovalV1> {
        self.snapshot()?
            .state
            .owner_setup_reconciles()
            .get(id)
            .cloned()
            .ok_or_else(rejected)
    }
    pub fn reconcile_attempt(
        &self,
        id: &str,
        attempt: &str,
    ) -> Result<OwnerSetupReconcileAttemptV1> {
        self.snapshot()?
            .state
            .owner_setup_reconcile_attempt(id, attempt)
            .cloned()
            .ok_or_else(rejected)
    }
    pub async fn begin_owner_setup_reconcile<F>(
        &self,
        input: ApprovalAdmissionInput,
        plan: OwnerSetupReconcilePlanV1,
        current: F,
    ) -> Result<ApprovalAdmissionRecord>
    where
        F: Fn() -> Result<NativeStageCurrent>,
    {
        let saved = self.snapshot()?;
        let nonce = if let Some(old) = saved
            .state
            .owner_setup_reconcile_original(&input.operation_id)
        {
            if old.input() != &input
                || saved
                    .state
                    .owner_setup_reconciles()
                    .get(&input.operation_id)
                    .is_none_or(|v| v.plan != plan)
            {
                return Err(rejected());
            }
            old.verify_nonce().to_owned()
        } else {
            crate::approval_crypto::verification_nonce().map_err(|_| rejected())?
        };
        let a = OwnerSetupReconcileAdmissionV1 {
            input,
            plan,
            verify_nonce: nonce,
        };
        let id = a.input.operation_id.clone();
        self.reconcile_verified(
            &current,
            "setup_reconcile_pending",
            &id,
            |state, c, t| {
                Ok(vec![
                    state
                        .begin_owner_setup_reconcile_checks(&a, c.provision, t)
                        .map_err(|_| rejected())?,
                ])
            },
            |state, c, t, v| {
                state
                    .begin_owner_setup_reconcile(a.clone(), c.provision, t, v)
                    .map_err(|_| rejected())
            },
        )
        .await
    }
    pub async fn commit_owner_setup_reconcile<F>(
        &self,
        id: &str,
        signed: SignedApprovalVerificationV1,
        current: F,
    ) -> Result<OwnerSetupReconcileApprovalV1>
    where
        F: Fn() -> Result<NativeStageCurrent>,
    {
        let nonce = crate::approval_crypto::verification_nonce().map_err(|_| rejected())?;
        self.reconcile_verified(
            &current,
            "setup_reconcile_consumed",
            id,
            |state, c, t| {
                Ok(vec![
                    state
                        .commit_owner_setup_reconcile_checks(id, &signed, c.provision, t)
                        .map_err(|_| rejected())?,
                ])
            },
            |state, c, t, v| {
                state
                    .commit_owner_setup_reconcile(
                        CommitOwnerSetupV1 {
                            id: id.into(),
                            signed: signed.clone(),
                            source_nonce: nonce.clone(),
                        },
                        c.native_scope,
                        c.provision,
                        t,
                        v,
                    )
                    .map_err(|_| rejected())
            },
        )
        .await
    }
    pub fn reserve_owner_setup_reconcile_attempt(
        &self,
        id: &str,
        attempt: &str,
    ) -> Result<OwnerSetupReconcileAttemptV1> {
        let nonce = crate::approval_crypto::verification_nonce().map_err(|_| rejected())?;
        let origin = crate::approval_crypto::verification_nonce().map_err(|_| rejected())?;
        self.local_reservation("setup_reconcile_attempt_reserved", id, |state, t| {
            state
                .reserve_owner_setup_reconcile_attempt(id, attempt, &nonce, &origin, t)
                .map_err(|_| rejected())
        })
    }
    pub fn claim_owner_setup_reconcile_source<F>(
        &self,
        id: &str,
        attempt: Option<&str>,
        current: F,
    ) -> Result<ApprovalVerifyRequestV1>
    where
        F: Fn() -> Result<NativeStageCurrent>,
    {
        self.native_step(
            &current,
            "setup_reconcile_source_claimed",
            id,
            |state, _s, c, t| {
                state
                    .claim_owner_setup_reconcile_source(id, attempt, c, t)
                    .map_err(|_| rejected())
            },
        )
    }
    pub async fn complete_owner_setup_reconcile_source<F>(
        &self,
        id: &str,
        attempt: Option<&str>,
        signed: SignedApprovalVerificationV1,
        current: F,
    ) -> Result<()>
    where
        F: Fn() -> Result<NativeStageCurrent>,
    {
        self.reconcile_verified(
            &current,
            "setup_reconcile_source_verified",
            id,
            |state, c, t| {
                Ok(vec![
                    state
                        .owner_setup_reconcile_source_checks(id, attempt, &signed, c.provision, t)
                        .map_err(|_| rejected())?,
                ])
            },
            |state, c, t, v| {
                state
                    .complete_owner_setup_reconcile_source(
                        id,
                        attempt,
                        signed.clone(),
                        c.provision,
                        t,
                        v,
                    )
                    .map_err(|_| rejected())
            },
        )
        .await
    }
    pub fn reserve_reconcile_read(
        &self,
        id: &str,
        attempt: &str,
        purpose: OwnerSetupReadPurposeV1,
        role: oasis7_custody_core::http::Role,
    ) -> Result<OwnerSetupReadTicket> {
        self.local_reservation("setup_reconcile_read_reserved", id, |state, t| {
            state
                .reserve_owner_setup_reconcile_read(id, attempt, purpose, role, t)
                .map_err(|_| rejected())
        })
    }
    pub fn reserve_reconcile_callback(
        &self,
        id: &str,
        attempt: &str,
        kind: OwnerSetupReconcileCallbackV1,
    ) -> Result<()> {
        self.local_reservation("setup_reconcile_callback_reserved", id, |state, t| {
            state
                .reserve_owner_setup_reconcile_callback(id, attempt, kind, t)
                .map_err(|_| rejected())
        })
    }
    pub async fn reconcile_permit<F>(
        &self,
        id: &str,
        attempt: &str,
        read: OwnerSetupReadTicket,
        role: oasis7_custody_core::http::Role,
        access: Option<OwnerSetupAccessEvidenceV1>,
        current: F,
    ) -> Result<OwnerSetupReconcilePermitV1>
    where
        F: Fn() -> Result<NativeStageCurrent>,
    {
        if (read.ticket == 3) != access.is_some() || read.ticket == 5 {
            return Err(rejected());
        }
        self.reconcile_verified(
            &current,
            "setup_reconcile_read_authorized",
            id,
            |state, c, t| {
                Ok(vec![
                    if read.purpose == OwnerSetupReadPurposeV1::Session {
                        state.owner_setup_reconcile_checks(id, attempt, c.provision, t)
                    } else {
                        let old = state
                            .owner_setup_reconcile_attempt(id, attempt)
                            .and_then(|a| a.access.as_ref());
                        state.owner_setup_reconcile_access_checks(
                            id,
                            attempt,
                            access.as_ref().or(old).ok_or_else(rejected)?,
                            c,
                            t,
                        )
                    }
                    .map_err(|_| rejected())?,
                ])
            },
            |state, c, t, v| {
                if let Some(e) = &access {
                    state
                        .record_owner_setup_reconcile_access(id, attempt, e.clone(), c, t, v)
                        .map_err(|_| rejected())?;
                }
                let permit = state
                    .authorize_owner_setup_reconcile_read(
                        id,
                        OwnerSetupReconcileRead {
                            attempt_id: attempt,
                            read,
                            role,
                        },
                        c,
                        t,
                        v,
                    )
                    .map_err(|_| rejected())?;
                if read.ticket == 2 {
                    state
                        .claim_owner_setup_reconcile_access(id, attempt, t)
                        .map_err(|_| rejected())?;
                }
                Ok(permit)
            },
        )
        .await
    }
    pub async fn publish_owner_setup_reconcile_reads<F>(
        &self,
        id: &str,
        attempt: &str,
        e: OwnerSetupReconcileReadCandidateV1,
        current: F,
    ) -> Result<OwnerSetupReconcilePermitV1>
    where
        F: Fn() -> Result<NativeStageCurrent>,
    {
        self.reconcile_verified(
            &current,
            "publish_owner_setup_reconcile_reads",
            id,
            |state, c, t| {
                Ok(vec![
                    state
                        .owner_setup_reconcile_publish_checks(id, attempt, &e, c, t)
                        .map_err(|_| rejected())?,
                ])
            },
            |state, c, t, v| {
                state
                    .publish_owner_setup_reconcile_reads(id, attempt, e.clone(), c, t, v)
                    .map_err(|_| rejected())
            },
        )
        .await
    }
    pub async fn record_owner_setup_reconcile_admission<F>(
        &self,
        id: &str,
        attempt: &str,
        e: OwnerSetupAdmissionEvidenceV1,
        current: F,
    ) -> Result<OwnerSetupReconcilePermitV1>
    where
        F: Fn() -> Result<NativeStageCurrent>,
    {
        self.reconcile_verified(
            &current,
            "record_owner_setup_reconcile_admission",
            id,
            |state, c, t| {
                Ok(vec![
                    state
                        .owner_setup_reconcile_admission_checks(id, attempt, &e, c, t)
                        .map_err(|_| rejected())?,
                ])
            },
            |state, c, t, v| {
                state
                    .record_owner_setup_reconcile_admission(id, attempt, e.clone(), c, t, v)
                    .map_err(|_| rejected())
            },
        )
        .await
    }
    pub async fn complete_owner_setup_reconcile<F>(
        &self,
        id: &str,
        attempt: &str,
        e: OwnerSetupReconcileCompletionV1,
        current: F,
    ) -> Result<OwnerSetupOutboxV1>
    where
        F: Fn() -> Result<NativeStageCurrent>,
    {
        self.reconcile_verified(
            &current,
            "complete_owner_setup_reconcile",
            id,
            |state, c, t| {
                Ok(vec![
                    state
                        .owner_setup_reconcile_result_checks(id, attempt, &e, c, t)
                        .map_err(|_| rejected())?,
                ])
            },
            |state, c, t, v| {
                state
                    .complete_owner_setup_reconcile(id, attempt, e.clone(), c, t, v)
                    .map_err(|_| rejected())
            },
        )
        .await
    }
    pub async fn claim_owner_setup_reconcile_lookup<F>(
        &self,
        id: &str,
        attempt: &str,
        current: F,
    ) -> Result<OwnerSetupReconcilePermitV1>
    where
        F: Fn() -> Result<NativeStageCurrent>,
    {
        self.reconcile_verified(
            &current,
            "setup_reconcile_lookup_claimed",
            id,
            |state, c, t| {
                let a = state
                    .owner_setup_reconcile_attempt(id, attempt)
                    .ok_or_else(rejected)?;
                Ok(vec![
                    state
                        .owner_setup_reconcile_admission_checks(
                            id,
                            attempt,
                            a.admission.as_ref().ok_or_else(rejected)?,
                            c,
                            t,
                        )
                        .map_err(|_| rejected())?,
                ])
            },
            |state, c, t, v| {
                state
                    .claim_owner_setup_reconcile_lookup(id, attempt, c, t, v)
                    .map_err(|_| rejected())?;
                state
                    .owner_setup_reconcile_lookup_permit(id, attempt, c, t, v)
                    .map_err(|_| rejected())
            },
        )
        .await
    }
    pub async fn archive_reconcile_fact(
        &self,
        id: &str,
        attempt: &str,
        fact: OwnerSetupCanonicalFactV1,
        key: &str,
    ) -> Result<OwnerSetupCanonicalFactV1> {
        let context = OwnerSetupArchiveContext {
            world_public_key: key,
        };
        let checks = self
            .snapshot()?
            .state
            .owner_setup_reconcile_archive_checks(id, attempt, &fact, &context)
            .map_err(|_| rejected())?;
        let mut v = verify(checks).await?;
        let mut result = None;
        self.tighten("setup_reconcile_fact_archived", |state| {
            let checks = state
                .owner_setup_reconcile_archive_checks(id, attempt, &fact, &context)
                .map_err(|_| rejected())?;
            v.verify(&checks).map_err(|_| rejected())?;
            result = Some(
                state
                    .archive_owner_setup_reconcile_fact(id, attempt, fact.clone(), &context, &mut v)
                    .map_err(|_| rejected())?,
            );
            Ok(())
        })?;
        result.ok_or_else(rejected)
    }
}

impl OwnerProvisionApprovalStore {
    pub async fn owner_setup_reconcile_poststate_permit<F>(
        &self,
        id: &str,
        attempt: &str,
        current: F,
    ) -> Result<OwnerSetupReconcilePermitV1>
    where
        F: Fn() -> Result<NativeStageCurrent>,
    {
        self.reconcile_verified(
            &current,
            "setup_reconcile_poststate_claimed",
            id,
            |state, c, t| {
                let a = state
                    .owner_setup_reconcile_attempt(id, attempt)
                    .ok_or_else(rejected)?;
                Ok(vec![
                    state
                        .owner_setup_reconcile_admission_checks(
                            id,
                            attempt,
                            a.admission.as_ref().ok_or_else(rejected)?,
                            c,
                            t,
                        )
                        .map_err(|_| rejected())?,
                ])
            },
            |state, c, t, v| {
                state
                    .owner_setup_reconcile_poststate_permit(id, attempt, c, t, v)
                    .map_err(|_| rejected())
            },
        )
        .await
    }
}
