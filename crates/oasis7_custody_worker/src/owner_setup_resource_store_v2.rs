//! The existing paid Admit transaction, explicitly producing the new Resource original.
use super::*;
impl OwnerProvisionApprovalStore {
    pub fn reserve_owner_resource_admission_callback_v2(&self, id: &str) -> Result<()> {
        self.local_reservation("owner_resource_admission_reserved", id, |state, time| {
            state
                .reserve_owner_setup_resource_admission_callback_v2(id, time)
                .map_err(|_| rejected())
        })
    }

    pub async fn admit_and_claim_owner_resource_send_v2<F>(
        &self,
        id: &str,
        evidence: OwnerSetupAdmissionEvidenceV1,
        issue: &ResourceFinalIssueContextV2<'_>,
        current: F,
    ) -> Result<PreparedOwnerSetupResourceDispatchV2>
    where
        F: Fn() -> Result<NativeStageCurrent>,
    {
        self.native_verified(
            &current,
            "owner_setup_resource_effect_admitted",
            id,
            |state, scope, context, time| {
                Ok(vec![
                    state
                        .owner_setup_admission_checks(
                            id,
                            OwnerSetupStageV1::FinalObservation,
                            &evidence,
                            &OwnerSetupCurrent {
                                provision: context,
                                native_scope: scope,
                            },
                            time,
                        )
                        .map_err(|_| rejected())?,
                ])
            },
            |state, scope, context, time, cache| {
                let trusted = OwnerSetupCurrent {
                    provision: context,
                    native_scope: scope,
                };
                state
                    .record_owner_setup_admission(
                        id,
                        OwnerSetupStageV1::FinalObservation,
                        evidence.clone(),
                        &trusted,
                        time,
                        cache,
                    )
                    .map_err(|_| rejected())?;
                state
                    .claim_owner_setup_resource_dispatch_v2(id, issue, &trusted, time, cache)
                    .map_err(|_| rejected())
            },
        )
        .await
    }

    pub fn reserve_owner_resource_result_callback_v2(&self, id: &str) -> Result<()> {
        self.local_reservation("owner_resource_result_reserved", id, |state, time| {
            state
                .reserve_owner_setup_resource_result_callback_v2(id, time)
                .map_err(|_| rejected())
        })
    }

    pub async fn complete_owner_resource_result_v2<F>(
        &self,
        id: &str,
        completed: CompleteOwnerSetupResourceFinalV2,
        current: F,
    ) -> Result<()>
    where
        F: Fn() -> Result<NativeStageCurrent>,
    {
        self.native_verified(
            &current,
            "owner_resource_final_completed",
            id,
            |state, scope, context, time| {
                Ok(vec![
                    state
                        .owner_setup_resource_final_completion_checks_v2(
                            id,
                            &completed,
                            &OwnerSetupCurrent {
                                provision: context,
                                native_scope: scope,
                            },
                            time,
                        )
                        .map_err(|_| rejected())?,
                ])
            },
            |state, scope, context, time, cache| {
                state
                    .complete_owner_setup_resource_final_v2(
                        id,
                        completed.clone(),
                        &OwnerSetupCurrent {
                            provision: context,
                            native_scope: scope,
                        },
                        time,
                        cache,
                    )
                    .map_err(|_| rejected())
            },
        )
        .await
    }
}
