use super::*;
impl OwnerProvisionApprovalStore {
    pub(crate) async fn begin_test_session<F>(
        &self,
        input: ApprovalAdmissionInput,
        termination: TerminationPlanV1,
        current: F,
    ) -> Result<ApprovalAdmissionRecord>
    where
        F: Fn() -> Result<TestSessionApprovalOwned>,
    {
        let saved = self.snapshot()?;
        let nonce = match saved.state.test_session_original(&input.operation_id) {
            Some(old) if old.input() == &input => old.verify_nonce().to_owned(),
            Some(_) => return Err(rejected()),
            None => crate::approval_crypto::verification_nonce().map_err(|_| rejected())?,
        };
        let admission = TestSessionAdmissionV1 {
            input,
            termination,
            verify_nonce: nonce,
        };
        let id = admission.input.operation_id.clone();
        self.native_verified(
            &|| Ok(current()?.native),
            "test_session_pending",
            &id,
            |state, _, _, time| {
                current()?.with_core(|c| {
                    state
                        .begin_test_session_checks(&admission, c, UnixMillis::new(time))
                        .map_err(|_| rejected())
                })
            },
            |state, _, _, time, cache| {
                current()?.with_core(|c| {
                    state
                        .begin_test_session(admission.clone(), c, UnixMillis::new(time), cache)
                        .map_err(|_| rejected())
                })
            },
        )
        .await
    }
    pub(crate) async fn consume_test_session<F>(
        &self,
        commit: CommitTestSessionV1,
        current: F,
    ) -> Result<TestSessionAuthorityV1>
    where
        F: Fn() -> Result<TestSessionApprovalOwned>,
    {
        let id = commit.session_id.clone();
        self.native_verified(
            &|| Ok(current()?.native),
            "test_session_consumed",
            &id,
            |state, _, _, time| {
                current()?.with_core(|c| {
                    state
                        .consume_test_session_approval_checks(&commit, c, UnixMillis::new(time))
                        .map_err(|_| rejected())
                })
            },
            |state, _, _, time, cache| {
                current()?.with_core(|c| {
                    state
                        .consume_test_session_approval(
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
    pub(crate) fn claim_test_session_approval(&self, id: &str) -> Result<ApprovalVerifyRequestV1> {
        self.test_session_local(id, |s, t| {
            s.claim_test_session_approval_verification(id, t)
                .map_err(|_| rejected())
        })
    }
    pub(crate) fn reserve_test_session_source(
        &self,
        id: &str,
        operation: &str,
        funding: SessionSourceFundingV1,
    ) -> Result<ProvisionOriginInspectionV1> {
        let saved = self.snapshot()?;
        let nonce = match saved
            .state
            .test_sessions()
            .get(id)
            .and_then(|s| s.sources.get(operation))
        {
            Some(old) => old.verify_nonce.clone(),
            None => crate::approval_crypto::verification_nonce().map_err(|_| rejected())?,
        };
        self.test_session_local(id, |s, t| {
            s.reserve_test_session_source(
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
    pub(crate) fn reserve_test_session_source_read(&self, id: &str, operation: &str) -> Result<u8> {
        self.test_session_local(id, |s, t| {
            s.reserve_test_session_source_read(id, operation, t)
                .map_err(|_| rejected())
        })
    }
    pub(crate) fn claim_test_session_source<F>(
        &self,
        id: &str,
        operation: &str,
        current: F,
    ) -> Result<ApprovalVerifyRequestV1>
    where
        F: Fn() -> Result<NativeStageCurrent>,
    {
        self.native_step(&current, "test_session_source_claimed", id, |s, _, c, t| {
            s.claim_test_session_source(id, operation, c, UnixMillis::new(t))
                .map_err(|_| rejected())
        })
    }
    pub(crate) async fn complete_test_session_source<F>(
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
                Ok(vec![
                    s.test_session_source_checks(id, operation, &signed, c, UnixMillis::new(t))
                        .map_err(|_| rejected())?,
                ])
            },
            |s, _, c, t, v| {
                s.complete_test_session_source(
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

impl OwnerProvisionApprovalStore {
    pub(crate) fn create_test_session_invite(
        &self,
        request: PairingInviteCreateRequestV1,
        current: &OwnerSetupManagementCurrent<'_>,
    ) -> Result<PairingInviteOriginalV1> {
        let id = crate::approval_crypto::verification_nonce().map_err(|_| rejected())?;
        let reference = request.request_id.clone();
        self.test_session_local(&reference, |s, time| {
            s.create_pairing_invite(request.clone(), &id, current, time)
                .map_err(|_| invite_store_diagnostic("core_create", rejected()))
        })
        .map_err(|e| invite_store_diagnostic("transaction", e))
    }
}

fn invite_store_diagnostic(stage: &str, error: JsValue) -> JsValue {
    #[cfg(feature = "approval-harness")]
    worker::console_warn!("test_session.invite.store.{}", stage);
    #[cfg(not(feature = "approval-harness"))]
    let _ = stage;
    error
}
