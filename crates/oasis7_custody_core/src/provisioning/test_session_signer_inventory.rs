//! One account-wide allocated-original inventory shared by legacy and V2 cleanup.
//! A missing send/signature/result never removes a reserved high-water value.
use super::*;
impl AccountProvisionApprovalState {
    pub(in crate::provisioning) fn allocated_signer_high_water(&self) -> Result<(u64, u64)> {
        let (mut generation, mut nonce) = (0u64, 0u64);
        for setup in self.owner_setups.values() {
            generation = generation.max(setup.plan.delegation_generation.0);
            nonce = nonce.max(setup.plan.delegation_nonce.0);
        }
        if let Some(plan) = self.owner_setup_v2_plan() {
            generation = generation.max(plan.delegation_generation.0);
            nonce = nonce.max(plan.delegation_nonce.0);
        }
        for session in self.test_sessions.values() {
            for original in session.termination_slots.values() {
                if let TerminationPayloadV1::Revoke(d) = &original.payload {
                    generation = generation.max(d.generation);
                    nonce = nonce.max(d.nonce);
                }
            }
        }
        for session in self.test_sessions_v2.values() {
            session
                .termination_state
                .validate_input_binding(&session.input)?;
            session
                .termination_state
                .validate_source_binding(&session.sources, &session.source_funding)?;
            if let Some(close) = &session.termination_state.close {
                generation = generation.max(close.payload.revoke_generation.0);
                nonce = nonce.max(close.payload.revoke_nonce.0);
            }
        }
        Ok((generation, nonce))
    }
}
