//! Paid source observations and fixed once-only lifetime transport.
use super::*;
impl OwnerProvisionApprovalStore {
    /// The caller first buys a source ticket locally. This authenticates the
    /// persisted inspection under actual Directory/Fence Current before private reads.
    pub(crate) async fn claim_client_lifetime<F>(
        &self,
        id: &str,
        source_id: &str,
        purpose: SessionPurposeV1,
        current: F,
    ) -> Result<TestSessionLiveDescriptorV1>
    where
        F: Fn() -> Result<NativeStageCurrent>,
    {
        let attempt_id = format!(
            "client-lifetime-{}",
            oasis7_custody_core::approval::bytes_digest(
                &serde_json::to_vec(&(
                    "oasis7.custody.client-lifetime-transport.v1",
                    id,
                    source_id,
                ))
                .map_err(|_| rejected())?
            )
        );
        let signed = self
            .snapshot()?
            .state
            .test_sessions()
            .get(id)
            .and_then(|session| session.sources.get(source_id))
            .and_then(|source| source.verification.clone())
            .ok_or_else(rejected)?;
        self.native_verified(
            &current,
            "test_session_client_lifetime_claimed",
            id,
            |state, _, context, time| {
                lifetime_source(state, id, source_id)?;
                Ok(vec![
                    state
                        .test_session_source_checks(
                            id,
                            source_id,
                            &signed,
                            context,
                            UnixMillis::new(time),
                        )
                        .map_err(|_| rejected())?,
                ])
            },
            |state, _, context, time, verifier| {
                lifetime_source(state, id, source_id)?;
                verifier
                    .verify(
                        &state
                            .test_session_source_checks(
                                id,
                                source_id,
                                &signed,
                                context,
                                UnixMillis::new(time),
                            )
                            .map_err(|_| rejected())?,
                    )
                    .map_err(|_| rejected())?;
                state
                    .charge_test_session_internal_control_transport(
                        id,
                        &attempt_id,
                        context,
                        UnixMillis::new(time),
                    )
                    .map_err(|_| rejected())?;
                state
                    .test_session_live_descriptor(id, purpose, source_id)
                    .map_err(|_| rejected())
            },
        )
        .await
    }
    pub(crate) async fn client_source_descriptor<F>(
        &self,
        id: &str,
        source_id: &str,
        ticket: u8,
        purpose: SessionPurposeV1,
        current: F,
    ) -> Result<TestSessionLiveDescriptorV1>
    where
        F: Fn() -> Result<NativeStageCurrent>,
    {
        let signed = self
            .snapshot()?
            .state
            .test_sessions()
            .get(id)
            .and_then(|session| session.sources.get(source_id))
            .and_then(|source| source.verification.clone())
            .ok_or_else(rejected)?;
        self.native_verified(
            &current,
            "test_session_client_source_read",
            id,
            |state, _, context, time| {
                source_ticket(state, id, source_id, ticket)?;
                Ok(vec![
                    state
                        .test_session_source_checks(
                            id,
                            source_id,
                            &signed,
                            context,
                            UnixMillis::new(time),
                        )
                        .map_err(|_| rejected())?,
                ])
            },
            |state, _, context, time, verifier| {
                source_ticket(state, id, source_id, ticket)?;
                let checks = state
                    .test_session_source_checks(
                        id,
                        source_id,
                        &signed,
                        context,
                        UnixMillis::new(time),
                    )
                    .map_err(|_| rejected())?;
                verifier.verify(&checks).map_err(|_| rejected())?;
                state
                    .test_session_live_descriptor(id, purpose, source_id)
                    .map_err(|_| rejected())
            },
        )
        .await
    }
}
fn source_ticket(
    state: &AccountProvisionApprovalState,
    id: &str,
    source_id: &str,
    ticket: u8,
) -> Result<()> {
    if ticket == 0
        || ticket > 8
        || state
            .test_sessions()
            .get(id)
            .and_then(|session| session.source_oracle_reads.get(source_id))
            .copied()
            != Some(ticket)
    {
        return Err(rejected());
    }
    Ok(())
}

fn lifetime_source(state: &AccountProvisionApprovalState, id: &str, source_id: &str) -> Result<()> {
    source_ticket(state, id, source_id, 3)?;
    if state
        .test_sessions()
        .get(id)
        .and_then(|s| s.source_funding.get(source_id))
        != Some(&SessionSourceFundingV1::Control)
    {
        return Err(rejected());
    }
    Ok(())
}

impl OwnerProvisionApprovalStore {
    pub(crate) async fn revalidate_client_http(
        &self,
        env: &Env,
        rpc: &TestSessionClientAdmissionRpc,
        operation: SessionClientOperation,
    ) -> Result<SessionPurposeV1> {
        self.client_proof_transaction(
            env,
            rpc,
            operation,
            "test_session_client_revalidated",
            |state, proof, time, _| {
                state
                    .charge_test_session_control_http_attempt(
                        &rpc.http.selector.session_id,
                        SessionHttpAdmission {
                            ingress_attempt_id: &rpc.ingress_attempt_id,
                            possession: proof,
                        },
                        time,
                    )
                    .map_err(|_| rejected())?;
                match operation {
                    SessionClientOperation::Challenge => {
                        let request: SessionChallengeRequestV1 =
                            serde_json::from_str(&rpc.http.original.body)
                                .map_err(|_| rejected())?;
                        Ok(request.purpose)
                    }
                    SessionClientOperation::CredentialClaim => {
                        use oasis7_custody_core::client_enrollment::{
                            ClientCredentialClaimRequestV1, TestSessionPurposeV1,
                        };
                        let request: ClientCredentialClaimRequestV1 =
                            serde_json::from_str(&rpc.http.original.body)
                                .map_err(|_| rejected())?;
                        request.validate().map_err(|_| rejected())?;
                        match request.purposes.first() {
                            Some(TestSessionPurposeV1::Execute) => Ok(SessionPurposeV1::Execution),
                            Some(TestSessionPurposeV1::Read) => Ok(SessionPurposeV1::ReconcileRead),
                            _ => Err(rejected()),
                        }
                    }
                    SessionClientOperation::Control(_) => {
                        let digest = hex::encode(proof.token_digest());
                        let session = state
                            .test_sessions()
                            .get(&rpc.http.selector.session_id)
                            .ok_or_else(rejected)?;
                        [SessionPurposeV1::Execution, SessionPurposeV1::ReconcileRead]
                            .into_iter()
                            .find(|purpose| {
                                session
                                    .roots
                                    .get(purpose)
                                    .and_then(|r| r.credential.as_ref())
                                    .is_some_and(|credential| credential.digest == digest)
                            })
                            .ok_or_else(rejected)
                    }
                }
            },
        )
        .await
    }
}

pub(crate) fn client_source_operation_id(rpc: &TestSessionClientAdmissionRpc) -> Result<String> {
    Ok(format!(
        "client-source-{}",
        oasis7_custody_core::approval::bytes_digest(
            &serde_json::to_vec(&(
                "oasis7.custody.client-ingress-source.v1",
                &rpc.http.selector.session_id,
                &rpc.ingress_attempt_id
            ))
            .map_err(|_| rejected())?
        )
    ))
}
