use super::*;
pub(crate) fn source_id(rpc: &TestSessionClientAdmissionRpc) -> Result<String> {
    Ok(format!(
        "client-v2-source-{}",
        oasis7_custody_core::approval::bytes_digest(
            &serde_json::to_vec(&(
                "oasis7.custody.client-ingress-source.v2",
                &rpc.http.selector.session_id,
                &rpc.ingress_attempt_id
            ))
            .map_err(|_| rejected())?
        )
    ))
}
impl OwnerProvisionApprovalStore {
    pub(crate) async fn admit_credential_v2_source(
        &self,
        env: &Env,
        rpc: &TestSessionClientAdmissionRpc,
        operation: SessionClientOperation,
    ) -> Result<Option<String>> {
        self.charge_credential_v2_http(env, rpc, operation).await?;
        if let Some(history) = self.credential_v2_history(env, rpc, operation).await? {
            return Ok(Some(history));
        }
        let id = source_id(rpc)?;
        let random = crate::approval_crypto::verification_nonce().map_err(|_| rejected())?;
        self.credential_v2_proof_transaction(
            env,
            rpc,
            operation,
            "test_session_v2_client_source",
            |state, proof, time, _| {
                state
                    .charge_test_session_v2_claim_http_attempt(
                        &rpc.http.selector.session_id,
                        SessionHttpAdmission {
                            possession: proof,
                            ingress_attempt_id: &rpc.ingress_attempt_id,
                        },
                        time,
                    )
                    .map_err(|_| rejected())?;
                let nonce = state
                    .test_session_v2_source(&rpc.http.selector.session_id, &id)
                    .map(|saved| saved.source.verify_nonce.clone())
                    .unwrap_or_else(|| random.clone());
                state
                    .reserve_test_session_v2_source(
                        &rpc.http.selector.session_id,
                        SessionSourceReservation {
                            operation_id: &id,
                            nonce: &nonce,
                            funding: SessionSourceFundingV1::Control,
                        },
                        time,
                    )
                    .map_err(|_| rejected())?;
                Ok(())
            },
        )
        .await?;
        Ok(None)
    }

    /// Source-only strong checks, no not-yet-observed Native lifetime is invented.
    pub(crate) async fn observe_credential_v2_source<F>(
        &self,
        env: &Env,
        rpc: &TestSessionClientAdmissionRpc,
        operation: SessionClientOperation,
        purpose: SessionPurposeV1,
        ticket: u8,
        lifetime_claim: bool,
        current: F,
    ) -> Result<TestSessionCredentialLiveDescriptorV2>
    where
        F: Fn() -> Result<NativeStageCurrent>,
    {
        let id = &rpc.http.selector.session_id;
        let source = source_id(rpc)?;
        self.native_verified(
            &current,
            "test_session_v2_client_observe",
            id,
            |state, scope, c, time| {
                selector_v2(state, rpc)?;
                let prepared = prepare_v2(env, state, rpc, operation, UnixMillis::new(time))?;
                let saved = state
                    .test_session_v2_source(id, &source)
                    .ok_or_else(rejected)?;
                let mut jobs = state
                    .test_session_v2_native_restore_checks(id, scope)
                    .map_err(|_| rejected())?;
                jobs.extend([
                    prepared.signature_checks().map_err(|_| rejected())?,
                    state
                        .test_session_v2_source_checks(
                            id,
                            &source,
                            saved.source.verification.as_ref().ok_or_else(rejected)?,
                            c,
                            UnixMillis::new(time),
                        )
                        .map_err(|_| rejected())?,
                ]);
                Ok(jobs)
            },
            |state, _, c, time, cache| {
                selector_v2(state, rpc)?;
                let prepared = prepare_v2(env, state, rpc, operation, UnixMillis::new(time))?;
                let proof = verify_http_possession(prepared, UnixMillis::new(time), cache)
                    .map_err(|_| rejected())?;
                state
                    .charge_test_session_v2_claim_http_attempt(
                        id,
                        SessionHttpAdmission {
                            possession: &proof,
                            ingress_attempt_id: &rpc.ingress_attempt_id,
                        },
                        UnixMillis::new(time),
                    )
                    .map_err(|_| rejected())?;
                if state
                    .test_session_v2_credential_source_ticket(id, &source)
                    .map_err(|_| rejected())?
                    != ticket
                {
                    return Err(rejected());
                }
                if lifetime_claim {
                    if ticket != 3 {
                        return Err(rejected());
                    }
                    state
                        .claim_test_session_v2_credential_lifetime(
                            id,
                            &source,
                            SessionHttpAdmission {
                                ingress_attempt_id: &rpc.ingress_attempt_id,
                                possession: &proof,
                            },
                            c,
                            UnixMillis::new(time),
                            cache,
                        )
                        .map_err(|_| rejected())?;
                } else if !matches!(ticket, 1 | 2) {
                    return Err(rejected());
                }
                state
                    .test_session_v2_credential_live_descriptor(id, purpose, &source)
                    .map_err(|_| rejected())
            },
        )
        .await
    }
}
