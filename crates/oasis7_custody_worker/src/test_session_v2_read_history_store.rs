//! Actual Account proof/SQL consumer for a fixed internal Owner call. This does
//! not mount public Actions or supply the missing separately paid V2 Read
//! Source/Native Current producer. No Current/proof JSON is accepted.
use super::client::ClientLiveCurrent;
use super::*;
use crate::test_session_client_rpc::TestSessionClientAdmissionRpc;
use oasis7_custody_core::{
    http_pop::{HttpHeaderRepresentation, verify_http_possession},
    provisioning::{
        ReadPrepareHistoryPaidV2, ReadPrepareHistoryReplyV2, SessionProofRequest, SessionPurposeV1,
    },
    time_units::UnixMillis,
};
use worker::Env;
fn prepare(
    env: &Env,
    state: &AccountProvisionApprovalState,
    rpc: &TestSessionClientAdmissionRpc,
    time: u64,
) -> Result<oasis7_custody_core::http_pop::PreparedHttpPop> {
    let q = &rpc.http.selector;
    let draft = state.draft();
    if q.api_version != 2
        || q.custody_account_id != draft.entry.custody_account_id
        || q.provision_id != draft.entry.provision_id
        || q.session_id != rpc.http.original.session_id
        || [
            "CUSTODY_TEST_SESSION_ENABLED",
            "CUSTODY_TEST_SESSION_V2_ENABLED",
        ]
        .into_iter()
        .any(|flag| env.var(flag).ok().map(|v| v.to_string()).as_deref() != Some("true"))
    {
        return Err(rejected());
    }
    let body: oasis7_custody_core::provisioning::SessionActionRequestV2 =
        serde_json::from_str(&rpc.http.original.body).map_err(|_| rejected())?;
    if body.purpose() != SessionPurposeV1::ReconcileRead {
        return Err(rejected());
    }
    state
        .prepare_test_session_v2_action_http(
            &rpc.http.original,
            HttpHeaderRepresentation::WorkerMergedSingletons,
            UnixMillis::new(time),
        )
        .map_err(|_| rejected())
}
impl OwnerProvisionApprovalStore {
    /// Trusted Identity ingress ID and full original PoP reach the fixed Owner
    /// service/Account callback. Host1 and actual inbound Owner2 are separately
    /// committed from Read300 before any future Source/Native Current I/O.
    /// Same ingress cannot reconstruct the first opaque continuation.
    pub(crate) async fn begin_read_prepare_history_v2(
        &self,
        env: &Env,
        rpc: &TestSessionClientAdmissionRpc,
    ) -> Result<Option<ReadPrepareHistoryPaidV2>> {
        let first = prepare(env, &self.snapshot()?.state, rpc, now()?)?;
        let mut cache = verify(first.signature_checks().map_err(|_| rejected())?).await?;
        self.local_reservation(
            "read_prepare_history_host_paid",
            &rpc.http.selector.session_id,
            |state, time| {
                let checked = prepare(env, state, rpc, time)?;
                let possession = verify_http_possession(checked, UnixMillis::new(time), &mut cache)
                    .map_err(|_| rejected())?;
                state
                    .charge_test_session_v2_purpose_http_attempt(
                        &rpc.http.selector.session_id,
                        SessionPurposeV1::ReconcileRead,
                        SessionProofRequest {
                            ingress_attempt_id: &rpc.ingress_attempt_id,
                            original_bytes: rpc.http.original.body.as_bytes(),
                            possession: &possession,
                        },
                        UnixMillis::new(time),
                    )
                    .map_err(|_| rejected())
            },
        )?;
        self.local_reservation(
            "read_prepare_history_owner_paid",
            &rpc.http.selector.session_id,
            |state, time| {
                let checked = prepare(env, state, rpc, time)?;
                let possession = verify_http_possession(checked, UnixMillis::new(time), &mut cache)
                    .map_err(|_| rejected())?;
                state
                    .charge_test_session_v2_prepare_history_owner_attempt(
                        &rpc.http.selector.session_id,
                        SessionProofRequest {
                            ingress_attempt_id: &rpc.ingress_attempt_id,
                            original_bytes: rpc.http.original.body.as_bytes(),
                            possession: &possession,
                        },
                        UnixMillis::new(time),
                    )
                    .map_err(|_| rejected())
            },
        )
    }
    /// The closure reads the actual already-paid trusted Rust observation; it
    /// must not perform I/O or build Current from caller JSON/boolean flags.
    /// Future Source/Directory/Fence/LF/SID leaves are separate Read charges,
    /// never covered by the three ingress transports settled above.
    pub(crate) async fn finish_read_prepare_history_v2<F>(
        &self,
        env: &Env,
        rpc: &TestSessionClientAdmissionRpc,
        paid: ReadPrepareHistoryPaidV2,
        current: F,
    ) -> Result<ReadPrepareHistoryReplyV2>
    where
        F: Fn() -> Result<ClientLiveCurrent>,
    {
        let mut first = Some(paid);
        self.native_verified(
            &|| Ok(current()?.native),
            "read_prepare_history_returned",
            &rpc.http.selector.session_id,
            |state, _, _, time| {
                let prepared = prepare(env, state, rpc, time)?;
                let mut jobs = vec![prepared.signature_checks().map_err(|_| rejected())?];
                jobs.push(current()?.with_core(|c| {
                    state
                        .test_session_v2_prepare_history_checks(
                            &rpc.http.selector.session_id,
                            &rpc.ingress_attempt_id,
                            c,
                            UnixMillis::new(time),
                        )
                        .map_err(|_| rejected())
                })?);
                Ok(jobs)
            },
            |state, _, _, time, cache| {
                let checked = prepare(env, state, rpc, time)?;
                let possession = verify_http_possession(checked, UnixMillis::new(time), cache)
                    .map_err(|_| rejected())?;
                let paid = first.take().ok_or_else(rejected)?;
                current()?.with_core(|c| {
                    state
                        .finish_test_session_v2_prepare_history(
                            &rpc.http.selector.session_id,
                            paid,
                            SessionProofRequest {
                                ingress_attempt_id: &rpc.ingress_attempt_id,
                                original_bytes: rpc.http.original.body.as_bytes(),
                                possession: &possession,
                            },
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
}

impl OwnerProvisionApprovalStore {
    /// Same trusted ingress only: exact immutable result readback after lost
    /// local callback. The deployment pin is Rust configuration, never JSON.
    /// A new actual HTTP ingress must first execute begin and pay its own3.
    pub(crate) async fn read_prepare_history_callback_v2(
        &self,
        env: &Env,
        rpc: &TestSessionClientAdmissionRpc,
        native: &NativeStageScope<'_>,
    ) -> Result<ReadPrepareHistoryReplyV2> {
        let first = prepare(env, &self.snapshot()?.state, rpc, now()?)?;
        let mut cache = verify(first.signature_checks().map_err(|_| rejected())?).await?;
        self.local_reservation(
            "read_prepare_history_callback",
            &rpc.http.selector.session_id,
            |state, time| {
                let checked = prepare(env, state, rpc, time)?;
                let possession = verify_http_possession(checked, UnixMillis::new(time), &mut cache)
                    .map_err(|_| rejected())?;
                state
                    .test_session_v2_prepare_history_paid_reply(
                        &rpc.http.selector.session_id,
                        SessionProofRequest {
                            ingress_attempt_id: &rpc.ingress_attempt_id,
                            original_bytes: rpc.http.original.body.as_bytes(),
                            possession: &possession,
                        },
                        native,
                        UnixMillis::new(time),
                    )
                    .map_err(|_| rejected())
            },
        )
    }
}
