//! Fresh status recovery is funded locally before any remote observation.
use super::*;
use oasis7_custody_core::provisioning::{
    SessionStatusRecoveryOriginalV1, SessionStatusRecoveryRequestV1,
};
impl OwnerProvisionApprovalStore {
    pub fn session_status_recovery_original(
        &self,
        request: &SessionStatusRecoveryRequestV1,
    ) -> Result<SessionStatusRecoveryOriginalV1> {
        request.validate().map_err(|_| rejected())?;
        self.snapshot()?
            .state
            .session_status_recoveries()
            .get(&request.operation_id)
            .filter(|saved| &saved.request == request)
            .cloned()
            .ok_or_else(rejected)
    }
    pub fn reserve_session_status_recovery(
        &self,
        request: SessionStatusRecoveryRequestV1,
        status_nonce: &str,
        origin_nonce: &str,
        scope: &NativeStageScopeCurrent,
    ) -> Result<SessionStatusRecoveryOriginalV1> {
        let id = request.operation_id.clone();
        self.local_reservation("session_status_recovery_reserved", &id, |state, time| {
            state
                .reserve_session_status_recovery(
                    request.clone(),
                    status_nonce,
                    origin_nonce,
                    &scope.as_core(),
                    time,
                )
                .map_err(|_| rejected())
        })
    }
    pub fn reserve_session_status_recovery_read(&self, id: &str) -> Result<u8> {
        self.local_reservation(
            "session_status_recovery_read_reserved",
            id,
            |state, time| {
                state
                    .reserve_session_status_recovery_read(id, time)
                    .map_err(|_| rejected())
            },
        )
    }
    pub fn claim_session_recovery_origin<F>(
        &self,
        id: &str,
        current: F,
    ) -> Result<ApprovalVerifyRequestV1>
    where
        F: Fn() -> Result<NativeStageCurrent>,
    {
        self.native_step(
            &current,
            "session_status_recovery_origin_claimed",
            id,
            |state, _scope, context, time| {
                state
                    .claim_session_recovery_origin(id, context, time)
                    .map_err(|_| rejected())
            },
        )
    }
}

impl OwnerProvisionApprovalStore {
    pub async fn complete_session_recovery_origin<F>(
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
            "session_recovery_origin_verified",
            id,
            |state, _scope, context, time| {
                Ok(vec![
                    state
                        .session_recovery_origin_checks(id, &signed, context, time)
                        .map_err(|_| rejected())?,
                ])
            },
            |state, _scope, context, time, cache| {
                state
                    .complete_session_recovery_origin(id, signed.clone(), context, time, cache)
                    .map_err(|_| rejected())
            },
        )
        .await
    }
    pub async fn claim_session_status_recovery<F>(
        &self,
        id: &str,
        current: F,
    ) -> Result<oasis7_custody_core::provisioning::SessionStatusRecoveryPermitViewV1>
    where
        F: Fn() -> Result<NativeStageCurrent>,
    {
        self.native_verified(
            &current,
            "session_status_recovery_claimed",
            id,
            |state, scope, context, time| {
                Ok(vec![
                    state
                        .session_status_recovery_checks(id, scope, context, time)
                        .map_err(|_| rejected())?,
                ])
            },
            |state, scope, context, time, cache| {
                state
                    .claim_session_status_recovery(id, scope, context, time, cache)
                    .map_err(|_| rejected())
            },
        )
        .await
    }
    pub async fn authorize_session_status_recovery_read<F>(
        &self,
        id: &str,
        ticket: u8,
        current: F,
    ) -> Result<oasis7_custody_core::provisioning::SessionStatusRecoveryPermitViewV1>
    where
        F: Fn() -> Result<NativeStageCurrent>,
    {
        self.native_verified(
            &current,
            "session_status_recovery_read_authorized",
            id,
            |state, scope, context, time| {
                Ok(vec![
                    state
                        .session_status_recovery_checks(id, scope, context, time)
                        .map_err(|_| rejected())?,
                ])
            },
            |state, scope, context, time, cache| {
                state
                    .authorize_session_status_recovery_read(id, ticket, scope, context, time, cache)
                    .map_err(|_| rejected())
            },
        )
        .await
    }
    pub async fn complete_session_status_recovery<F>(
        &self,
        id: &str,
        signed: oasis7_custody_core::native_binding::SignedManagedNativeSessionStatusV1,
        session: &PersistedNativeSessionAttestation<'_>,
        current: F,
    ) -> Result<oasis7_custody_core::native_binding::SignedManagedNativeSessionStatusV1>
    where
        F: Fn() -> Result<NativeStageCurrent>,
    {
        // A completed exact fact can be read after its observation expires, but
        // never reactivates Ready, changes the SID, or creates another grant.
        let saved = self.snapshot()?;
        let original = saved
            .state
            .session_status_recoveries()
            .get(id)
            .ok_or_else(rejected)?;
        if let Some(old) = &original.result {
            let result = NativeStageResultV1::SessionStatus(signed.clone());
            historical_result(
                Some(&NativeStageResultV1::SessionStatus(old.clone())),
                original.result_private_sid_generation,
                &result,
                Some(session),
            )?;
            let mut recovered = None;
            self.tighten("session_status_recovery_original_read", |state| {
                let original = state
                    .session_status_recoveries()
                    .get(id)
                    .ok_or_else(rejected)?;
                let old = original.result.as_ref().ok_or_else(rejected)?;
                historical_result(
                    Some(&NativeStageResultV1::SessionStatus(old.clone())),
                    original.result_private_sid_generation,
                    &result,
                    Some(session),
                )?;
                recovered = Some(old.clone());
                Ok(())
            })?;
            return recovered.ok_or_else(rejected);
        }
        self.native_verified(
            &current,
            "session_status_recovery_completed",
            id,
            |state, scope, context, time| {
                Ok(vec![
                    state
                        .session_status_recovery_result_checks(id, &signed, scope, context, time)
                        .map_err(|_| rejected())?,
                ])
            },
            |state, scope, context, time, cache| {
                state
                    .complete_session_status_recovery(
                        id,
                        signed.clone(),
                        scope,
                        context,
                        time,
                        session,
                        cache,
                    )
                    .map_err(|_| rejected())
            },
        )
        .await
    }
}
