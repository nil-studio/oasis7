use super::*;
impl OwnerProvisionApprovalStore {
    pub(crate) async fn begin_test_session_v2<F>(
        &self,
        input: ApprovalAdmissionInput,
        current: F,
    ) -> Result<ApprovalAdmissionRecord>
    where
        F: Fn() -> Result<TestSessionApprovalOwned>,
    {
        let saved = self.snapshot()?;
        let nonce = match saved.state.test_session_v2_original(&input.operation_id) {
            Some(old) if old.input() == &input => old.verify_nonce().to_owned(),
            Some(_) => return Err(rejected()),
            None => crate::approval_crypto::verification_nonce().map_err(|_| rejected())?,
        };
        let admission = TestSessionAdmissionV2 {
            input,
            verify_nonce: nonce,
        };
        let id = admission.input.operation_id.clone();
        let deadline = super::super::alarm::write_deadline(&admission.input)?;
        let deadline = UnixMillis::new(
            deadline
                .get()
                .min(admission.input.approval.proof.expires_at.0),
        );
        self.native_verified_with_alarm(
            &|| Ok(current()?.native),
            "test_session_pending",
            &id,
            deadline,
            |state, _, _, time| {
                current()?.with_core(|c| {
                    state
                        .begin_test_session_v2_checks(&admission, c, UnixMillis::new(time))
                        .map_err(|_| rejected())
                })
            },
            |state, _, _, time, cache| {
                current()?.with_core(|c| {
                    state
                        .begin_test_session_v2(admission.clone(), c, UnixMillis::new(time), cache)
                        .map_err(|_| rejected())
                })
            },
        )
        .await
    }
    pub(crate) async fn consume_test_session_v2<F>(
        &self,
        commit: CommitTestSessionV2,
        current: F,
    ) -> Result<TestSessionAuthorityV2>
    where
        F: Fn() -> Result<TestSessionApprovalOwned>,
    {
        let id = commit.session_id.clone();
        let saved = self.snapshot()?;
        let session = saved
            .state
            .test_session_v2_snapshot(&id)
            .map_err(|_| rejected())?;
        let deadline = super::super::alarm::write_deadline(session.input())?;
        self.native_verified_with_alarm(
            &|| Ok(current()?.native),
            "test_session_consumed",
            &id,
            deadline,
            |state, _, _, time| {
                current()?.with_core(|c| {
                    let mut checks = state
                        .test_session_v2_native_restore_checks(&id, c.native_scope)
                        .map_err(|_| rejected())?;
                    checks.extend(
                        state
                            .consume_test_session_v2_approval_checks(
                                &commit,
                                c,
                                UnixMillis::new(time),
                            )
                            .map_err(|_| rejected())?,
                    );
                    Ok(checks)
                })
            },
            |state, _, _, time, cache| {
                current()?.with_core(|c| {
                    state
                        .consume_test_session_v2_approval(
                            commit.clone(),
                            c,
                            UnixMillis::new(time),
                            cache,
                        )
                        .map_err(|_| rejected())
                })
            },
        )
        .await
    }
    pub(crate) fn claim_test_session_v2_approval(
        &self,
        id: &str,
    ) -> Result<ApprovalVerifyRequestV1> {
        self.test_session_local(id, |s, t| {
            s.claim_test_session_v2_approval_verification(id, t)
                .map_err(|_| rejected())
        })
    }
    pub(crate) fn reserve_test_session_v2_source(
        &self,
        id: &str,
        operation: &str,
        funding: SessionSourceFundingV1,
    ) -> Result<TestSessionSourceV2> {
        let saved = self.snapshot()?;
        let nonce = match saved.state.test_session_v2_source(id, operation) {
            Some(old) => old.source.verify_nonce.clone(),
            None => crate::approval_crypto::verification_nonce().map_err(|_| rejected())?,
        };
        self.test_session_local(id, |s, t| {
            s.reserve_test_session_v2_source(
                id,
                SessionSourceReservation {
                    operation_id: operation,
                    nonce: &nonce,
                    funding,
                },
                t,
            )
            .map_err(|_| rejected())
        })
    }
    pub(crate) fn reserve_test_session_v2_source_read(
        &self,
        id: &str,
        operation: &str,
    ) -> Result<u8> {
        self.test_session_local(id, |s, t| {
            s.reserve_test_session_v2_source_read(id, operation, t)
                .map_err(|_| rejected())
        })
    }
    pub(crate) fn claim_test_session_v2_source<F>(
        &self,
        id: &str,
        operation: &str,
        current: F,
    ) -> Result<ApprovalVerifyRequestV1>
    where
        F: Fn() -> Result<NativeStageCurrent>,
    {
        self.native_step(&current, "test_session_source_claimed", id, |s, _, c, t| {
            s.claim_test_session_v2_source(id, operation, c, UnixMillis::new(t))
                .map_err(|_| rejected())
        })
    }
    pub(crate) async fn complete_test_session_v2_source<F>(
        &self,
        id: &str,
        operation: &str,
        signed: SignedApprovalVerificationV1,
        current: F,
    ) -> Result<()>
    where
        F: Fn() -> Result<NativeStageCurrent>,
    {
        self.native_verified(
            &current,
            "test_session_source_verified",
            id,
            |s, _, c, t| {
                let owned = current()?;
                let scope = owned.scope.as_core();
                let mut checks = s
                    .test_session_v2_native_restore_checks(id, &scope)
                    .map_err(|_| rejected())?;
                checks.push(
                    s.test_session_v2_source_checks(id, operation, &signed, c, UnixMillis::new(t))
                        .map_err(|_| rejected())?,
                );
                Ok(checks)
            },
            |s, _, c, t, v| {
                s.complete_test_session_v2_source(
                    id,
                    operation,
                    signed.clone(),
                    c,
                    UnixMillis::new(t),
                    v,
                )
                .map_err(|_| rejected())
            },
        )
        .await
    }
}
