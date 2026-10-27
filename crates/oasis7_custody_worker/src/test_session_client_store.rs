//! Account-side client proof transactions. A verified PoP is never a paid action or grant.
use super::*;
use crate::{
    test_session_client_authority::ClientHttpConfiguration,
    test_session_client_rpc::TestSessionClientAdmissionRpc,
};
use oasis7_custody_core::{
    http_pop::{HttpPossessionReceipt, verify_http_possession},
    native_binding::SignedManagedNativeSessionLifetimeV1,
    provisioning::*,
    time_units::UnixMillis,
};
use worker::Env;
/// Constructed only by the fixed private Source/Lifetime/Manifest coordinator.
/// No Deserialize, boolean approval, public key, credential digest or caller budget.
#[derive(Clone)]
pub(crate) struct ClientLiveCurrent {
    pub native: NativeStageCurrent,
    pub lifetime: SignedManagedNativeSessionLifetimeV1,
    pub private_sid_generation: DecimalU64,
    pub source_operation_id: String,
    pub source_ticket: u8,
    pub observed_at: u64,
}
/// Local reservation facts only; these do not authorize a native send.
pub(crate) struct ClientReservation {
    pub source: Option<ProvisionOriginInspectionV1>,
    pub descriptor: Option<TestSessionLiveDescriptorV1>,
}
impl ClientLiveCurrent {
    fn with_core<R>(&self, read: impl FnOnce(&TestSessionLiveCurrent<'_, '_>) -> R) -> R {
        let provision = self.native.provision.as_core();
        let native_scope = self.native.scope.as_core();
        read(&TestSessionLiveCurrent {
            provision: &provision,
            native_scope: &native_scope,
            lifetime: &self.lifetime,
            private_sid_generation: self.private_sid_generation,
            source_ticket: self.source_ticket,
            source_operation_id: &self.source_operation_id,
            observed_at: UnixMillis::new(self.observed_at),
        })
    }
}
fn selector(
    state: &AccountProvisionApprovalState,
    rpc: &TestSessionClientAdmissionRpc,
) -> Result<()> {
    let selected = &rpc.http.selector;
    let draft = state.draft();
    if selected.api_version != 1
        || selected.custody_account_id != draft.entry.custody_account_id
        || selected.provision_id != draft.entry.provision_id
        || selected.session_id != rpc.http.original.session_id
    {
        return Err(rejected());
    }
    Ok(())
}
impl OwnerProvisionApprovalStore {
    /// PoP first, then a final SQL reread and strict re-verification. The caller
    /// supplies only a closed Rust transition, never a serialized verified flag.
    async fn client_proof_transaction<R, T>(
        &self,
        env: &Env,
        rpc: &TestSessionClientAdmissionRpc,
        operation: SessionClientOperation,
        event: &str,
        mut transition: T,
    ) -> Result<R>
    where
        T: FnMut(
            &mut AccountProvisionApprovalState,
            &HttpPossessionReceipt,
            UnixMillis,
            &mut crate::approval_store::ExactCache,
        ) -> Result<R>,
    {
        let initial = self.snapshot()?;
        selector(&initial.state, rpc)?;
        let configuration = ClientHttpConfiguration::load(env)?;
        let prepared = configuration.prepare(
            &initial.state,
            &rpc.http.original,
            operation,
            UnixMillis::new(now()?),
        )?;
        let mut cache = verify(prepared.signature_checks().map_err(|_| rejected())?).await?;
        self.local_reservation(event, &rpc.http.selector.session_id, |state, time| {
            selector(state, rpc)?;
            let prepared = ClientHttpConfiguration::load(env)?.prepare(
                state,
                &rpc.http.original,
                operation,
                UnixMillis::new(time),
            )?;
            let proof = verify_http_possession(prepared, UnixMillis::new(time), &mut cache)
                .map_err(|_| rejected())?;
            transition(state, &proof, UnixMillis::new(time), &mut cache)
        })
    }
    /// All Source/Root signature jobs and this same raw HTTP signature are
    /// verified asynchronously, then re-derived from the final Account SQL row.
    async fn client_live_transaction<F, R, C, T>(
        &self,
        env: &Env,
        rpc: &TestSessionClientAdmissionRpc,
        lane: (SessionClientOperation, &str),
        current: F,
        checks: C,
        mut transition: T,
    ) -> Result<R>
    where
        F: Fn() -> Result<ClientLiveCurrent>,
        C: Fn(
            &AccountProvisionApprovalState,
            &TestSessionLiveCurrent<'_, '_>,
            UnixMillis,
        ) -> Result<Vec<ExactSignatureChecks>>,
        T: FnMut(
            &mut AccountProvisionApprovalState,
            &HttpPossessionReceipt,
            &TestSessionLiveCurrent<'_, '_>,
            UnixMillis,
            &mut NativeExactCache,
        ) -> Result<R>,
    {
        let (operation, event) = lane;
        self.native_verified(
            &|| Ok(current()?.native),
            event,
            &rpc.http.selector.session_id,
            |state, _, _, time| {
                selector(state, rpc)?;
                let prepared = ClientHttpConfiguration::load(env)?.prepare(
                    state,
                    &rpc.http.original,
                    operation,
                    UnixMillis::new(time),
                )?;
                let mut jobs = vec![prepared.signature_checks().map_err(|_| rejected())?];
                jobs.extend(
                    current()?
                        .with_core(|context| checks(state, context, UnixMillis::new(time)))?,
                );
                Ok(jobs)
            },
            |state, _, _, time, cache| {
                selector(state, rpc)?;
                let prepared = ClientHttpConfiguration::load(env)?.prepare(
                    state,
                    &rpc.http.original,
                    operation,
                    UnixMillis::new(time),
                )?;
                let proof = verify_http_possession(prepared, UnixMillis::new(time), cache)
                    .map_err(|_| rejected())?;
                current()?.with_core(|context| {
                    transition(state, &proof, context, UnixMillis::new(time), cache)
                })
            },
        )
        .await
    }
}

impl OwnerProvisionApprovalStore {
    /// Each trusted ingress pays after actual PoP, before any remote source work.
    pub(crate) async fn admit_client_http(
        &self,
        env: &Env,
        rpc: &TestSessionClientAdmissionRpc,
        operation: SessionClientOperation,
        purpose: SessionPurposeV1,
    ) -> Result<ClientReservation> {
        let source_id = client_source_operation_id(rpc)?;
        let candidate_nonce =
            crate::approval_crypto::verification_nonce().map_err(|_| rejected())?;
        let local_only = matches!(
            operation,
            SessionClientOperation::Control(
                SessionControlKindV1::Finish | SessionControlKindV1::Abort
            )
        );
        self.client_proof_transaction(
            env,
            rpc,
            operation,
            "test_session_client_http",
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
                if local_only {
                    return Ok(ClientReservation {
                        source: None,
                        descriptor: None,
                    });
                }
                let nonce = state
                    .test_sessions()
                    .get(&rpc.http.selector.session_id)
                    .and_then(|session| session.sources.get(&source_id))
                    .map(|source| source.verify_nonce.as_str())
                    .unwrap_or(&candidate_nonce)
                    .to_owned();
                let source = state
                    .reserve_test_session_source(
                        &rpc.http.selector.session_id,
                        SessionSourceReservation {
                            operation_id: &source_id,
                            nonce: &nonce,
                            funding: SessionSourceFundingV1::Control,
                        },
                        time,
                    )
                    .map_err(|_| rejected())?;
                let descriptor = state
                    .test_session_live_descriptor(
                        &rpc.http.selector.session_id,
                        purpose,
                        &source_id,
                    )
                    .map_err(|_| rejected())?;
                Ok(ClientReservation {
                    source: Some(source),
                    descriptor: Some(descriptor),
                })
            },
        )
        .await
    }
    pub(crate) async fn issue_client_challenge<F>(
        &self,
        env: &Env,
        rpc: &TestSessionClientAdmissionRpc,
        nonce: &str,
        purpose: SessionPurposeV1,
        current: F,
    ) -> Result<SessionChallengeV1>
    where
        F: Fn() -> Result<ClientLiveCurrent>,
    {
        self.client_live_transaction(
            env,
            rpc,
            (
                SessionClientOperation::Challenge,
                "test_session_client_challenge",
            ),
            current,
            |state, context, time| {
                Ok(vec![
                    state
                        .test_session_live_checks(
                            &rpc.http.selector.session_id,
                            purpose,
                            context,
                            time,
                        )
                        .map_err(|_| rejected())?,
                ])
            },
            |state, proof, context, time, cache| {
                state
                    .issue_test_session_challenge(
                        SessionProofRequest {
                            ingress_attempt_id: &rpc.ingress_attempt_id,
                            original_bytes: rpc.http.original.body.as_bytes(),
                            possession: proof,
                        },
                        nonce,
                        context,
                        time,
                        cache,
                    )
                    .map_err(|_| rejected())
            },
        )
        .await
    }
    pub(crate) async fn claim_client_credentials<F>(
        &self,
        env: &Env,
        rpc: &TestSessionClientAdmissionRpc,
        allocations: &[SessionCredentialAllocation],
        current: F,
    ) -> Result<SessionCredentialClaimOutcome>
    where
        F: Fn() -> Result<ClientLiveCurrent>,
    {
        self.client_live_transaction(
            env,
            rpc,
            (
                SessionClientOperation::CredentialClaim,
                "test_session_client_credentials",
            ),
            current,
            |state, context, time| {
                allocations
                    .iter()
                    .map(|a| {
                        state
                            .test_session_live_checks(
                                &rpc.http.selector.session_id,
                                a.purpose,
                                context,
                                time,
                            )
                            .map_err(|_| rejected())
                    })
                    .collect()
            },
            |state, proof, context, time, cache| {
                state
                    .claim_test_session_credentials(
                        &rpc.http.selector.session_id,
                        SessionCredentialClaim {
                            allocations,
                            proof: SessionProofRequest {
                                ingress_attempt_id: &rpc.ingress_attempt_id,
                                original_bytes: rpc.http.original.body.as_bytes(),
                                possession: proof,
                            },
                        },
                        context,
                        time,
                        cache,
                    )
                    .map_err(|_| rejected())
            },
        )
        .await
    }
    pub(crate) async fn control_client_session<F>(
        &self,
        env: &Env,
        rpc: &TestSessionClientAdmissionRpc,
        kind: SessionControlKindV1,
        purpose: SessionPurposeV1,
        current: F,
    ) -> Result<SessionControlAckV1>
    where
        F: Fn() -> Result<ClientLiveCurrent>,
    {
        if matches!(
            kind,
            SessionControlKindV1::Finish | SessionControlKindV1::Abort
        ) {
            return self
                .client_proof_transaction(
                    env,
                    rpc,
                    SessionClientOperation::Control(kind),
                    "test_session_client_close",
                    |state, proof, time, cache| {
                        state
                            .control_test_session(
                                SessionProofRequest {
                                    ingress_attempt_id: &rpc.ingress_attempt_id,
                                    original_bytes: rpc.http.original.body.as_bytes(),
                                    possession: proof,
                                },
                                None,
                                time,
                                cache,
                            )
                            .map_err(|_| rejected())
                    },
                )
                .await;
        }
        self.client_live_transaction(
            env,
            rpc,
            (
                SessionClientOperation::Control(kind),
                "test_session_client_control",
            ),
            current,
            |state, context, time| {
                Ok(vec![
                    state
                        .test_session_live_checks(
                            &rpc.http.selector.session_id,
                            purpose,
                            context,
                            time,
                        )
                        .map_err(|_| rejected())?,
                ])
            },
            |state, proof, context, time, cache| {
                state
                    .control_test_session(
                        SessionProofRequest {
                            ingress_attempt_id: &rpc.ingress_attempt_id,
                            original_bytes: rpc.http.original.body.as_bytes(),
                            possession: proof,
                        },
                        Some(context),
                        time,
                        cache,
                    )
                    .map_err(|_| rejected())
            },
        )
        .await
    }
}

#[cfg(feature = "approval-harness")]
#[path = "test_session_client_store_harness.rs"]
mod client_harness;
#[path = "test_session_client_store_source.rs"]
mod client_source;
pub(crate) use client_source::client_source_operation_id;
