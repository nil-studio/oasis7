//! Same Account SQL signature-check/commit boundary as V1, separate V2 Book.
use super::*;
use oasis7_custody_core::provisioning::*;
impl OwnerProvisionApprovalStore {
    pub async fn begin_owner_setup_v2<F>(
        &self,
        input: ApprovalAdmissionInput,
        plan: OwnerSetupPlanV2,
        current: F,
    ) -> Result<ApprovalAdmissionRecord>
    where
        F: Fn() -> Result<NativeStageCurrent>,
    {
        let saved = self.snapshot()?;
        let nonce = if let Some(old) = saved.state.owner_setup_v2_original(&input.operation_id) {
            if old.input() != &input || saved.state.owner_setup_v2_plan() != Some(&plan) {
                return Err(rejected());
            }
            old.verify_nonce().to_owned()
        } else {
            crate::approval_crypto::verification_nonce().map_err(|_| rejected())?
        };
        let id = input.operation_id.clone();
        self.native_verified(
            &current,
            "owner_setup_v2_pending",
            &id,
            |state, _scope, context, time| {
                Ok(vec![
                    state
                        .begin_owner_setup_v2_checks(&input, &plan, &nonce, context, time)
                        .map_err(|_| rejected())?,
                ])
            },
            |state, _scope, context, time, cache| {
                state
                    .begin_owner_setup_v2(
                        input.clone(),
                        plan.clone(),
                        nonce.clone(),
                        context,
                        time,
                        cache,
                    )
                    .map_err(|_| rejected())
            },
        )
        .await
    }
    pub async fn commit_owner_setup_v2<F>(
        &self,
        id: &str,
        signed: SignedApprovalVerificationV1,
        current: F,
    ) -> Result<ApprovalAdmissionRecord>
    where
        F: Fn() -> Result<NativeStageCurrent>,
    {
        let source_nonce = crate::approval_crypto::verification_nonce().map_err(|_| rejected())?;
        self.native_verified(
            &current,
            "owner_setup_v2_consumed",
            id,
            |state, _scope, context, time| {
                Ok(vec![
                    state
                        .commit_owner_setup_v2_checks(id, &signed, context, time)
                        .map_err(|_| rejected())?,
                ])
            },
            |state, scope, context, time, cache| {
                state
                    .commit_owner_setup_v2(
                        CommitOwnerSetupV2 {
                            id: id.into(),
                            signed: signed.clone(),
                            source_nonce: source_nonce.clone(),
                        },
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
