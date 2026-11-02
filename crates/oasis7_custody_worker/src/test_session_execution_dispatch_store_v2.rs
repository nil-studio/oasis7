//! Fixed paid reentry uses local strong-history verification, never free Current.
use super::*;
use oasis7_custody_core::{
    execution_dispatch_gate_v2::SignedExecutionDispatchClaimV2,
    native_cleanup_v2::AccountMachinePurposeV2,
    provisioning::{
        ExecutionDispatchIssueContextV2, ExecutionNativeGatedDispatchV2,
        ExecutionNativeLeafSigningAuthorityV2,
    },
    time_units::UnixMillis,
};
use worker::Env;
impl OwnerProvisionApprovalStore {
    async fn execution_dispatch_verified_v2<R, F>(
        &self,
        env: &Env,
        id: &str,
        event: &str,
        transition: F,
    ) -> Result<R>
    where
        F: FnOnce(
            &mut AccountProvisionApprovalState,
            &ExecutionDispatchIssueContextV2<'_>,
            UnixMillis,
            &mut NativeExactCache,
        ) -> Result<R>,
    {
        let saved = self.snapshot()?;
        let fixed = crate::owner_provision::execution_dispatch_static_config_v2(env, &saved.state)
            .map_err(|_| rejected())?;
        saved
            .state
            .test_session_v2_dispatch_static_configuration(
                id,
                &fixed.context,
                &fixed.world,
                (&fixed.namespace, fixed.instance_epoch),
            )
            .map_err(|_| rejected())?;
        let jobs = saved
            .state
            .test_session_v2_native_restore_checks(id, &fixed.native.as_core())
            .map_err(|_| rejected())?;
        let mut verified = Vec::with_capacity(jobs.len());
        for job in jobs {
            verified.push(verify(job).await?);
        }
        let mut cache = NativeExactCache(verified);
        let mut transition = Some(transition);
        self.local_reservation(event, id, |state, time| {
            // Reenter the actual SQL state after local WebCrypto completed.
            let fixed = crate::owner_provision::execution_dispatch_static_config_v2(env, state)
                .map_err(|_| rejected())?;
            state
                .test_session_v2_dispatch_static_configuration(
                    id,
                    &fixed.context,
                    &fixed.world,
                    (&fixed.namespace, fixed.instance_epoch),
                )
                .map_err(|_| rejected())?;
            let native = fixed.native.as_core();
            for job in state
                .test_session_v2_native_restore_checks(id, &native)
                .map_err(|_| rejected())?
            {
                cache.verify(&job).map_err(|_| rejected())?;
            }
            let (np, nf) = crate::execution_context_issuer_pin_v2::pin(
                env,
                AccountMachinePurposeV2::NativeExecutionPrepareV2,
            )?;
            let (dp, df) = crate::execution_context_issuer_pin_v2::pin(
                env,
                AccountMachinePurposeV2::ExecutionDispatchClaimV2,
            )?;
            let cfg = ExecutionDispatchIssueContextV2 {
                native_scope: &native,
                native_prepare_pin: &np,
                native_prepare_floor: &nf,
                dispatch_pin: &dp,
                dispatch_floor: &df,
            };
            transition.take().ok_or_else(rejected)?(state, &cfg, UnixMillis::new(time), &mut cache)
        })
    }
    pub(crate) async fn execution_native_first_leaf_v2(
        &self,
        env: &Env,
        id: &str,
        dispatch: ExecutionNativeGatedDispatchV2,
    ) -> Result<SignedExecutionDispatchClaimV2> {
        let authority = self
            .execution_dispatch_verified_v2(
                env,
                id,
                "execution_native_first_leaf_claimed",
                move |state, configured, at, cache| {
                    state
                        .claim_test_session_v2_native_first_leaf(
                            id, dispatch, configured, at, cache,
                        )
                        .map_err(|_| rejected())
                },
            )
            .await?;
        self.execution_native_leaf_signed_v2(env, id, authority)
    }
    pub(crate) async fn execution_native_identity_leaf_v2(
        &self,
        env: &Env,
        q: &crate::execution_dispatch_rpc_v2::IdentityExecutionDispatchGateV2,
    ) -> Result<SignedExecutionDispatchClaimV2> {
        let id = &q.request.proof.certificate.session_id;
        let authority = self
            .execution_dispatch_verified_v2(
                env,
                id,
                "execution_native_identity_leaf_claimed",
                |state, cfg, at, cache| {
                    state
                        .claim_test_session_v2_native_identity_receipted_leaf(
                            id,
                            (&q.request, &q.previous),
                            (q.edge, &q.body_digest),
                            cfg,
                            at,
                            cache,
                        )
                        .map_err(|_| rejected())
                },
            )
            .await?;
        self.execution_native_leaf_signed_v2(env, id, authority)
    }
    pub(crate) async fn execution_native_world_leaf_v2(
        &self,
        env: &Env,
        q: oasis7_custody_core::execution_dispatch_gate_v2::NativeExecutionDispatchCallbackV2,
    ) -> Result<SignedExecutionDispatchClaimV2> {
        let id = q.request.proof.certificate.session_id.clone();
        let authority = self
            .execution_dispatch_verified_v2(
                env,
                &id,
                "execution_native_world_leaf_claimed",
                |state, cfg, at, cache| {
                    state
                        .claim_test_session_v2_native_receipted_world_leaf(&id, q, cfg, at, cache)
                        .map_err(|_| rejected())
                },
            )
            .await?;
        self.execution_native_leaf_signed_v2(env, &id, authority)
    }
    fn execution_native_leaf_signed_v2(
        &self,
        env: &Env,
        id: &str,
        authority: ExecutionNativeLeafSigningAuthorityV2,
    ) -> Result<SignedExecutionDispatchClaimV2> {
        let signer = crate::account_service_issuer::AccountServiceIssuer::load(env)
            .map_err(|_| rejected())?;
        let mut authority = Some(authority);
        // The preceding successful SQL is the physical-claim linearization.
        // The issuer sees its typed opaque only after that commit returned.
        self.local_reservation(
            "execution_native_leaf_receipt_recorded",
            id,
            |state, time| {
                signer
                    .execution_leaf_receipt_recorded(
                        state,
                        id,
                        authority.take().ok_or_else(rejected)?,
                        UnixMillis::new(time),
                    )
                    .map_err(|_| rejected())
            },
        )
    }
}
