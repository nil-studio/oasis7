//! Account-local first-participant ledger. All Current inputs are Rust-owned.
use super::*;
use oasis7_agent_api::game::{GameParticipantAdmissionV1, GameParticipantReceiptV1};
use oasis7_client_api::world_service::{
    SignedReadRequest, participant_discovery::ParticipantPolicyOriginalV1,
};
use oasis7_custody_core::provisioning::{
    ParticipantCallbackV1, ParticipantFinalResultV1, ParticipantNoncesV1, ParticipantOriginalV1,
    ParticipantPermitViewV1, ParticipantProvisionRequestV1, ParticipantReadTicket,
    ParticipantSessionEvidenceV1, ParticipantSignBundleV1, ParticipantStepV1,
};
impl OwnerProvisionApprovalStore {
    pub fn participant_original(
        &self,
        request: &ParticipantProvisionRequestV1,
    ) -> Result<ParticipantOriginalV1> {
        request.validate().map_err(|_| rejected())?;
        self.snapshot()?
            .state
            .participant_original(&request.operation_id)
            .filter(|v| &v.request == request)
            .cloned()
            .ok_or_else(rejected)
    }
    pub fn reserve_participant_provision(
        &self,
        request: ParticipantProvisionRequestV1,
        nonces: ParticipantNoncesV1,
        scope: &NativeStageScopeCurrent,
    ) -> Result<ParticipantOriginalV1> {
        let id = request.operation_id.clone();
        self.local_reservation("participant_provision_reserved", &id, |state, time| {
            state
                .reserve_participant_provision(
                    request.clone(),
                    nonces.clone(),
                    &scope.as_core(),
                    time,
                )
                .map_err(|_| rejected())
        })
    }
    pub fn claim_participant_source<F>(
        &self,
        id: &str,
        current: F,
    ) -> Result<ApprovalVerifyRequestV1>
    where
        F: Fn() -> Result<NativeStageCurrent>,
    {
        self.native_step(
            &current,
            "participant_source_claimed",
            id,
            |state, _scope, context, time| {
                state
                    .claim_participant_source(id, context, time)
                    .map_err(|_| rejected())
            },
        )
    }
    pub async fn complete_participant_source<F>(
        &self,
        id: &str,
        signed: SignedApprovalVerificationV1,
        current: F,
    ) -> Result<()>
    where
        F: Fn() -> Result<NativeStageCurrent>,
    {
        self.native_verified(
            &current,
            "participant_source_verified",
            id,
            |state, _scope, context, time| {
                Ok(vec![
                    state
                        .participant_source_checks(id, &signed, context, time)
                        .map_err(|_| rejected())?,
                ])
            },
            |state, _scope, context, time, cache| {
                state
                    .complete_participant_source(id, signed.clone(), context, time, cache)
                    .map_err(|_| rejected())
            },
        )
        .await
    }
    pub fn reserve_participant_read(&self, id: &str) -> Result<u8> {
        self.local_reservation("participant_oracle_reserved", id, |state, time| {
            state
                .reserve_participant_read(id, time)
                .map_err(|_| rejected())
        })
    }
    pub fn reserve_participant_callback(
        &self,
        id: &str,
        callback: ParticipantCallbackV1,
    ) -> Result<()> {
        self.local_reservation("participant_callback_reserved", id, |state, time| {
            state
                .reserve_participant_callback(id, callback, time)
                .map_err(|_| rejected())
        })
    }
    pub async fn claim_participant_workflow<F>(&self, id: &str, current: F) -> Result<()>
    where
        F: Fn() -> Result<NativeStageCurrent>,
    {
        self.native_verified(
            &current,
            "claim_participant_workflow",
            id,
            |state, scope, context, time| {
                Ok(vec![
                    state
                        .participant_checks(id, scope, context, time)
                        .map_err(|_| rejected())?,
                ])
            },
            |state, scope, context, time, cache| {
                state
                    .claim_participant_workflow(id, scope, context, time, cache)
                    .map_err(|_| rejected())
            },
        )
        .await
    }
    pub async fn claim_participant_step<F>(
        &self,
        id: &str,
        step: ParticipantStepV1,
        current: F,
    ) -> Result<ParticipantPermitViewV1>
    where
        F: Fn() -> Result<NativeStageCurrent>,
    {
        self.native_verified(
            &current,
            "claim_participant_step",
            id,
            |state, scope, context, time| {
                Ok(vec![
                    state
                        .participant_checks(id, scope, context, time)
                        .map_err(|_| rejected())?,
                ])
            },
            |state, scope, context, time, cache| {
                state
                    .claim_participant_step(id, step, scope, context, time, cache)
                    .map_err(|_| rejected())
            },
        )
        .await
    }
    pub async fn authorize_participant_read<F>(
        &self,
        id: &str,
        ticket: u8,
        step: ParticipantStepV1,
        current: F,
    ) -> Result<ParticipantPermitViewV1>
    where
        F: Fn() -> Result<NativeStageCurrent>,
    {
        self.native_verified(
            &current,
            "authorize_participant_read",
            id,
            |state, scope, context, time| {
                Ok(vec![
                    state
                        .participant_checks(id, scope, context, time)
                        .map_err(|_| rejected())?,
                ])
            },
            |state, scope, context, time, cache| {
                state
                    .authorize_participant_read(
                        id,
                        ParticipantReadTicket { ticket, step },
                        scope,
                        context,
                        time,
                        cache,
                    )
                    .map_err(|_| rejected())
            },
        )
        .await
    }
    pub async fn record_participant_policy<F>(
        &self,
        id: &str,
        original: ParticipantPolicyOriginalV1,
        session: ParticipantSessionEvidenceV1,
        current: F,
    ) -> Result<()>
    where
        F: Fn() -> Result<NativeStageCurrent>,
    {
        self.native_verified(
            &current,
            "record_participant_policy",
            id,
            |state, scope, context, time| {
                Ok(vec![
                    state
                        .participant_policy_checks(id, &original, &session, scope, context, time)
                        .map_err(|_| rejected())?,
                ])
            },
            |state, scope, context, time, cache| {
                state
                    .record_participant_policy(
                        id,
                        original.clone(),
                        session.clone(),
                        scope,
                        context,
                        time,
                        cache,
                    )
                    .map_err(|_| rejected())
            },
        )
        .await
    }
    pub async fn record_participant_admission<F>(
        &self,
        id: &str,
        admission: SignedReadRequest<GameParticipantAdmissionV1>,
        session: ParticipantSessionEvidenceV1,
        current: F,
    ) -> Result<()>
    where
        F: Fn() -> Result<NativeStageCurrent>,
    {
        self.native_verified(
            &current,
            "record_participant_admission",
            id,
            |state, scope, context, time| {
                Ok(vec![
                    state
                        .participant_admission_checks(
                            id, &admission, &session, scope, context, time,
                        )
                        .map_err(|_| rejected())?,
                ])
            },
            |state, scope, context, time, cache| {
                state
                    .record_participant_admission(
                        id,
                        admission.clone(),
                        session.clone(),
                        scope,
                        context,
                        time,
                        cache,
                    )
                    .map_err(|_| rejected())
            },
        )
        .await
    }
    pub async fn complete_participant<F>(
        &self,
        id: &str,
        result: ParticipantFinalResultV1,
        session: ParticipantSessionEvidenceV1,
        current: F,
    ) -> Result<GameParticipantReceiptV1>
    where
        F: Fn() -> Result<NativeStageCurrent>,
    {
        self.native_verified(
            &current,
            "complete_participant",
            id,
            |state, scope, context, time| {
                Ok(vec![
                    state
                        .participant_result_checks(id, &result, &session, scope, context, time)
                        .map_err(|_| rejected())?,
                ])
            },
            |state, scope, context, time, cache| {
                state
                    .complete_participant(
                        id,
                        result.clone(),
                        session.clone(),
                        scope,
                        context,
                        time,
                        cache,
                    )
                    .map_err(|_| rejected())
            },
        )
        .await
    }
    pub async fn publish_participant_bundle<F>(
        &self,
        id: &str,
        ticket: u8,
        bundle: ParticipantSignBundleV1,
        current: F,
    ) -> Result<ParticipantSignBundleV1>
    where
        F: Fn() -> Result<NativeStageCurrent>,
    {
        self.native_verified(
            &current,
            "participant_bundle_published",
            id,
            |state, scope, context, time| {
                Ok(vec![
                    state
                        .participant_bundle_checks(id, &bundle, scope, context, time)
                        .map_err(|_| rejected())?,
                ])
            },
            |state, scope, context, time, cache| {
                state
                    .publish_participant_bundle(
                        id,
                        ticket,
                        bundle.clone(),
                        scope,
                        context,
                        time,
                        cache,
                    )
                    .map_err(|_| rejected())
            },
        )
        .await
    }
}
