//! The existing master ledger funds this first RF subtree. Local verification
//! and SQL below do not perform C3, Source, Native or any implicit callback.
use super::*;
use oasis7_custody_core::{
    execution_fence_dispatch_v2::*,
    native_cleanup_v2::AccountMachinePurposeV2,
    provisioning::{ExecutionPendingDirectoryDispatchV2, PendingFenceDispatchV2},
    time_units::UnixMillis,
};
use worker::Env;
impl OwnerProvisionApprovalStore {
    pub(crate) fn sign_pending_first_fence_v2(
        &self,
        env: &Env,
        directory: ExecutionPendingDirectoryDispatchV2,
    ) -> Result<Option<PendingFenceDispatchV2>> {
        let id = directory.session_id().to_owned();
        let saved = self.snapshot()?;
        let issuer = crate::account_service_issuer::AccountServiceIssuer::load(env)?;
        let name = issuer.pending_fence_receiver_name_v2(&saved.state, &id)?;
        let object = env
            .durable_object("EXECUTION_FENCE_DISPATCHERS")
            .map_err(|_| rejected())?
            .id_from_name(&name)
            .map_err(|_| rejected())?
            .to_string();
        // This is fixed public deployment metadata, not a namespace capability or
        // an installation certificate. The real FENCES receiver compares state.id.
        let fence_object = env
            .var("CUSTODY_EXECUTION_FENCE_OBJECT_ID")
            .map_err(|_| rejected())?
            .to_string();
        if fence_object.len() != 64
            || !fence_object
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(rejected());
        }
        let mut directory = Some(directory);
        self.local_reservation(
            "execution_pending_fence_dispatch_signed",
            &id,
            |state, time| {
                issuer
                    .pending_fence_dispatch_v2(
                        state,
                        directory.take().ok_or_else(rejected)?,
                        object.clone(),
                        fence_object.clone(),
                        UnixMillis::new(time),
                    )
                    .map_err(|_| rejected())
            },
        )
    }
    pub(crate) async fn claim_pending_fence_read_v2(
        &self,
        env: &Env,
        proof: &SignedExecutionFenceDispatchV2,
    ) -> Result<SignedExecutionFenceReadV2> {
        let id = &proof.binding.session_id;
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
        let mut checked = Vec::new();
        for job in saved
            .state
            .test_session_v2_native_restore_checks(id, &fixed.native.as_core())
            .map_err(|_| rejected())?
        {
            checked.push(verify(job).await?);
        }
        let mut cache = NativeExactCache(checked);
        let issuer = crate::account_service_issuer::AccountServiceIssuer::load(env)?;
        self.local_reservation("execution_pending_fence_read_signed", id, |state, time| {
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
            for job in state
                .test_session_v2_native_restore_checks(id, &fixed.native.as_core())
                .map_err(|_| rejected())?
            {
                cache.verify(&job).map_err(|_| rejected())?;
            }
            let (pin, floor) = crate::execution_context_issuer_pin_v2::pin(
                env,
                AccountMachinePurposeV2::ExecutionFenceDispatchV2,
            )?;
            issuer
                .pending_fence_read_v2(state, proof, &pin, &floor, UnixMillis::new(time))
                .map_err(|_| rejected())
        })
    }
}
