//! Account-owned durable three-edge reservation followed by real off-TX reads.
//! The fixed factory ingress has already been paid separately. No route is
//! opened here, and this returns no signer permission or private Native body.
use super::*;
use crate::owner_provision::native::test_session::{
    ExecutionPrepareObservationV2, execution_prepare_observe_v2,
};
use crate::owner_test_session_rpc::TestSessionSelectorV1;
use worker::Env;
impl OwnerProvisionApprovalStore {
    pub(crate) async fn execution_prepare_current_v2(
        &self,
        env: &Env,
        request: &TestSessionSelectorV1,
        factory_key: &str,
    ) -> Result<Option<ExecutionPrepareObservationV2>> {
        if env
            .var("CUSTODY_TEST_SESSION_V2_ENABLED")
            .ok()
            .map(|v| v.to_string())
            .as_deref()
            != Some("true")
        {
            return Err(rejected());
        }
        // All three physical claims and Source ticket5 are committed in the
        // same actual Account SQL callback before the first outbound request.
        let funding = self.local_reservation(
            "execution_prepare_current_ports",
            &request.session_id,
            |state, time| {
                let draft = state.draft();
                if request.api_version != 2
                    || request.custody_account_id != draft.entry.custody_account_id
                    || request.provision_id != draft.entry.provision_id
                {
                    return Err(rejected());
                }
                state
                    .claim_test_session_v2_prepare_current_ports(
                        &request.session_id,
                        factory_key,
                        oasis7_custody_core::time_units::UnixMillis::new(time),
                    )
                    .map_err(|_| rejected())
            },
        )?;
        let Some(funding) = funding else {
            // Includes unknown earlier reply: never perform the reads again.
            return Ok(None);
        };
        let source = funding.read().source_operation_id.clone();
        let saved = self.snapshot()?;
        // Any configuration, transport, response, expiry or later SQL failure
        // leaves the already committed three once claims and charges intact.
        execution_prepare_observe_v2(env, &saved, request, &source, funding)
            .await
            .map(Some)
            .map_err(|_| rejected())
    }
}

impl OwnerProvisionApprovalStore {
    /// The private SID/LF caller is the existing fixed Identity coordinator,
    /// never a serialized Current/proof flag. Source/LF/Actor fees remain its
    /// separate already-paid graph; these three reads supply only Dir/RF.
    pub(crate) async fn complete_execution_prepare_permits_v2<F>(
        &self,
        env: &Env,
        request: &TestSessionSelectorV1,
        factory_key: &str,
        observed: &ExecutionPrepareObservationV2,
        private_current: F,
    ) -> Result<Option<oasis7_custody_core::provisioning::ExecutionPrepareSignedOriginalV2>>
    where
        F: Fn() -> Result<super::client::ClientLiveCurrent>,
    {
        if env
            .var("CUSTODY_TEST_SESSION_V2_ENABLED")
            .ok()
            .map(|v| v.to_string())
            .as_deref()
            != Some("true")
        {
            return Err(rejected());
        }
        let saved = self.snapshot()?;
        let draft = saved.state.draft();
        if request.api_version != 2
            || request.custody_account_id != draft.entry.custody_account_id
            || request.provision_id != draft.entry.provision_id
        {
            return Err(rejected());
        }
        if let Some(old) = saved
            .state
            .test_session_v2_prepare_service_permits(&request.session_id)
        {
            if old.sign.permit.physical_ingress_key != factory_key {
                return Err(rejected());
            }
            observed
                .check_service_history(old)
                .map_err(|_| rejected())?;
            // Exact public history only. No seed load, signature, timestamp
            // refresh, Source request, RKV dispatch, or opaque reconstruction.
            return Ok(None);
        }
        let current = || {
            let mut value = private_current()?;
            value.native = observed.current(&self.state, env)?;
            Ok(value)
        };
        let paid = self
            .native_verified(
                &|| Ok(current()?.native),
                "execution_prepare_service_permits",
                &request.session_id,
                |state, _, _, time| {
                    let owned = current()?;
                    let mut jobs = state
                        .test_session_v2_native_restore_checks(
                            &request.session_id,
                            &owned.native.scope.as_core(),
                        )
                        .map_err(|_| rejected())?;
                    jobs.push(owned.with_core(|c| {
                        state
                            .test_session_v2_live_scope(
                                &request.session_id,
                                oasis7_custody_core::provisioning::SessionPurposeV1::Execution,
                                c,
                                oasis7_custody_core::time_units::UnixMillis::new(time),
                            )
                            .map_err(|_| rejected())
                    })?);
                    Ok(jobs)
                },
                |state, _, _, time, cache| {
                    let at = oasis7_custody_core::time_units::UnixMillis::new(time);
                    let authority = current()?.with_core(|c| {
                        state
                            .allocate_test_session_v2_execution_prepare(
                                &request.session_id,
                                factory_key,
                                c,
                                at,
                                cache,
                            )
                            .map_err(|_| rejected())
                    })?;
                    let issuer = crate::account_service_issuer::AccountServiceIssuer::load(env)
                        .map_err(|_| rejected())?;
                    issuer
                        .execution_prepare_recorded(
                            state,
                            &request.session_id,
                            authority,
                            observed,
                            at,
                        )
                        .map_err(|_| rejected())
                },
            )
            .await?;
        let Some(paid) = paid else {
            return Ok(None);
        };
        // SQL and all four physical claims have committed before either service
        // call. Failure/unknown never recreates this token or refunds a leaf.
        crate::execution_prepare_rpc_v2::dispatch(env, paid)
            .await
            .map(Some)
    }
}
