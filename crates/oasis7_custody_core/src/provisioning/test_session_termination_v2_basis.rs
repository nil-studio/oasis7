//! Actual V2 Prepared provenance factory, separate from the pure signed plan codec.
use super::*;

impl AccountProvisionApprovalState {
    /// Metadata from the actual V2 phase chain, not an approval/dispatch grant.
    /// Management caller admission remains a separate mandatory outer boundary.
    pub fn test_session_termination_basis_v2(
        &self,
        setup_id: &str,
        session_id: &str,
        cleanup_until: UnixMillis,
    ) -> Result<TerminationPlanV2> {
        if !id(session_id) || cleanup_until.get() == 0 {
            return Err(ProvisionApprovalError::BindingMismatch);
        }
        let prepared = self.owner_setup_prepared_v2(setup_id)?;
        if prepared.plan.setup_id != setup_id
            || prepared.plan.provision_id != self.draft.entry.provision_id
        {
            return Err(ProvisionApprovalError::BindingMismatch);
        }
        let plan = TerminationPlanV2 {
            api_version: 2,
            session_id: session_id.into(),
            custody_account_id: self.draft.entry.custody_account_id.clone(),
            provision_id: self.draft.entry.provision_id.clone(),
            setup_id: setup_id.into(),
            prepared_result_digest: prepared.result_digest.clone(),
            world: prepared.plan.world_identity.clone(),
            owner_key_ref: prepared.plan.owner_key_ref.clone(),
            executor_key_ref: prepared.plan.executor_key_ref.clone(),
            initial_fence: GameTaskFenceV1::from_control(prepared.control),
            original_delegation: prepared.delegation.clone(),
            stop_operation_id: super::termination_id("stop", session_id, &prepared.result_digest)?,
            close_operation_id: super::termination_id(
                "close-v2",
                session_id,
                &prepared.result_digest,
            )?,
            cleanup_until,
            request_limit: DecimalU64(2),
            transport_limit: DecimalU64(24),
            trigger: TerminationTriggerV1::ClosingOnlyV1,
            nonce_policy: TerminationNoncePolicyV1::AccountAllocatedHighWaterV1,
            stop_policy: crate::provisioning::WorldTaskStopPolicyV2::GuardedCommittedUtcStopV2,
            cleanup_transport_policy:
                crate::provisioning::CleanupTransportPolicyV2::TermWideFiveSecondOwnedV2,
            close_policy: WorldTaskClosePolicyV2::LiteralPersistentClosedV2,
        };
        plan.validate()
            .map_err(|_| ProvisionApprovalError::BindingMismatch)?;
        Ok(plan)
    }
}
