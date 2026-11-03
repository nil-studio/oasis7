//! Same Account local SQL and original sole typed issuer. No implicit C3/RF.
use super::*;
use oasis7_custody_core::{
    execution_pending_source_inspect_v2::*,
    execution_pending_source_result_v2::*,
    provisioning::{PendingSourceDispatchV2, PendingSourceHistoryV2},
    time_units::UnixMillis,
};
use worker::Env;
impl OwnerProvisionApprovalStore {
    pub(crate) async fn sign_pending_source_inspect_v2(
        &self,
        env: &Env,
        current: &mut crate::owner_provision::native::PendingDirectoryFenceObservationV2,
    ) -> Result<PendingSourceDispatchV2> {
        let id = current.source_session_id()?;
        let issuer = crate::account_service_issuer::AccountServiceIssuer::load(env)?;
        let name = issuer.pending_source_receiver_name_v2()?;
        let object = env
            .durable_object("EXECUTION_SOURCE_INSPECT_DISPATCHERS")
            .map_err(|_| rejected())?
            .id_from_name(&name)
            .map_err(|_| rejected())?
            .to_string();
        let saved = self.snapshot()?;
        let sampled_at = crate::approval_store::now()?;
        if sampled_at == 0 {
            return Err(rejected());
        }
        current.check_source_current(env, &saved.state, sampled_at)?;
        let mut checked = Vec::new();
        for job in saved
            .state
            .test_session_v2_native_restore_checks(&id, &issuer.native_scope())
            .map_err(|_| rejected())?
        {
            checked.push(verify(job).await?);
        }
        let mut cache = NativeExactCache(checked);
        self.local_reservation(
            "execution_pending_source_dispatch_signed",
            &id,
            |state, time| {
                current.check_source_current(env, state, time)?;
                let fixed = crate::owner_provision::execution_dispatch_static_config_v2(env, state)
                    .map_err(|_| rejected())?;
                state
                    .test_session_v2_dispatch_static_configuration(
                        &id,
                        &fixed.context,
                        &fixed.world,
                        (&fixed.namespace, fixed.instance_epoch),
                    )
                    .map_err(|_| rejected())?;
                issuer
                    .pending_source_dispatch_v2(
                        env,
                        state,
                        current,
                        object.clone(),
                        UnixMillis::new(time),
                        &mut cache,
                    )
                    .map_err(|_| rejected())
            },
        )
    }
    pub(crate) async fn claim_pending_source_native_v2(
        &self,
        env: &Env,
        proof: &SignedPendingSourceInspectDispatchV2,
    ) -> Result<SignedPendingSourceNativeLeafV2> {
        let id = &proof.binding.session_id;
        let saved = self.snapshot()?;
        let issuer = crate::account_service_issuer::AccountServiceIssuer::load(env)?;
        let mut checked = Vec::new();
        for job in saved
            .state
            .test_session_v2_native_restore_checks(id, &issuer.native_scope())
            .map_err(|_| rejected())?
        {
            checked.push(verify(job).await?);
        }
        let mut cache = NativeExactCache(checked);
        self.local_reservation(
            "execution_pending_source_native_signed",
            id,
            |state, time| {
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
                    .test_session_v2_native_restore_checks(id, &issuer.native_scope())
                    .map_err(|_| rejected())?
                {
                    cache.verify(&job).map_err(|_| rejected())?;
                }
                issuer
                    .pending_source_native_leaf_v2(env, state, proof, UnixMillis::new(time))
                    .map_err(|_| rejected())
            },
        )
    }
    pub(crate) fn archive_pending_source_inspect_history_v2(
        &self,
        env: &Env,
        first: PendingSourceDispatchV2,
        reply: SignedPendingSourceInspectResultV2,
    ) -> Result<PendingSourceHistoryV2> {
        let id = first.request().proof.binding.session_id.clone();
        let issuer = crate::account_service_issuer::AccountServiceIssuer::load(env)?;
        let mut first = Some(first);
        let mut reply = Some(reply);
        self.local_reservation("execution_pending_source_history", &id, |state, time| {
            state
                .archive_pending_source_inspect_result_history_v2(
                    first.take().ok_or_else(rejected)?,
                    reply.take().ok_or_else(rejected)?,
                    &issuer.native_scope(),
                    UnixMillis::new(time),
                )
                .map_err(|_| rejected())
        })
    }
}
