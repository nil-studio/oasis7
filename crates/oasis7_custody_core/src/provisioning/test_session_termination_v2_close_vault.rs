//! Independent Close Owner Actor edge. Historical Stop deadlines never extend or clip this new original.
//! No ordinary signer callback and no extra World lookup.
use super::*;
use crate::http::Role;
use oasis7_agent_api::game::{GAME_TASK_CLEANUP_DOMAIN_V2, GameTaskRevokeAndCloseV2};
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TerminationCloseVaultClaimV2 {
    pub(in crate::provisioning) attempt_id: String,
    pub(in crate::provisioning) claimed_at: UnixMillis,
    pub(in crate::provisioning) paid_index: u32,
    original_digest: String,
    pub(in crate::provisioning) native_lifetime: SignedManagedNativeSessionLifetimeV1,
    current_observed_at: UnixMillis,
    handoff_until: UnixMillis,
    manager_session_expires_at: UnixMillis,
    manager_session_idle_expires_at: UnixMillis,
    dispatch_until: UnixMillis,
    owner_scope: RoleKeySlotScopeV1,
    owner_key: PublicRoleKey,
}
/// Only the first actual Account commit may hand this to its typed issuer child.
/// History/JSON cannot reconstruct it. Public getters contain no private key.
pub struct PreparedTermCloseSignatureV2 {
    lineage: crate::term_owner_slot_lineage_v2::TermOwnerSlotLineageV2,
    payload: GameTaskRevokeAndCloseV2,
    owner_scope: RoleKeySlotScopeV1,
    owner_key: PublicRoleKey,
    attempt_id: String,
    input_digest: String,
    paid_record_digest: String,
    session_id: String,
    issued_at: UnixMillis,
    source_operation_id: String,
    source_record_digest: String,
    until: UnixMillis,
}
impl PreparedTermCloseSignatureV2 {
    pub fn lineage(&self) -> &crate::term_owner_slot_lineage_v2::TermOwnerSlotLineageV2 {
        &self.lineage
    }
    pub fn payload(&self) -> &GameTaskRevokeAndCloseV2 {
        &self.payload
    }
    pub fn owner_scope(&self) -> &RoleKeySlotScopeV1 {
        &self.owner_scope
    }
    pub fn owner_key(&self) -> &PublicRoleKey {
        &self.owner_key
    }
    pub fn attempt_id(&self) -> &str {
        &self.attempt_id
    }
    pub fn input_digest(&self) -> &str {
        &self.input_digest
    }
    pub fn paid_record_digest(&self) -> &str {
        &self.paid_record_digest
    }
    pub fn session_id(&self) -> &str {
        &self.session_id
    }
    pub fn issued_at(&self) -> UnixMillis {
        self.issued_at
    }
    pub fn source_operation_id(&self) -> &str {
        &self.source_operation_id
    }
    pub fn source_record_digest(&self) -> &str {
        &self.source_record_digest
    }
    pub fn until(&self) -> UnixMillis {
        self.until
    }
}
fn id_nonempty(value: &str) -> bool {
    !value.is_empty() && value.len() <= 256 && !value.chars().any(char::is_control)
}
fn original_digest(p: &GameTaskRevokeAndCloseV2) -> Result<String> {
    digest(&(
        GAME_TASK_CLEANUP_DOMAIN_V2,
        &p.permitted_branches.exact_initial_active.owner_public_key,
        p,
    ))
}
impl TerminationCloseVaultClaimV2 {
    pub(in crate::provisioning) fn validate_session(
        &self,
        slot: &TerminationCloseAllocationV2,
        s: &TestSessionAuthorityV2,
    ) -> Result<()> {
        let source = s
            .sources
            .get(&slot.source_operation_id)
            .ok_or_else(invalid)?;
        let nonce = digest(&(
            "oasis7.custody.test-session-current-lifetime.v2",
            &s.header()?.0.termination.session_id,
            &s.termination_state.approved_input_digest,
            &slot.source_operation_id,
            &source.source.verify_nonce,
        ))?;
        let original = s.native_lifetime.as_ref().ok_or_else(invalid)?;
        self.native_lifetime
            .lifetime
            .validate_for(
                &original.lifetime.status.receipt,
                &nonce,
                s.session.native_binding_generation,
                s.input.request.context.service_epoch,
                self.claimed_at.get(),
            )
            .map_err(|_| invalid())?;
        if self.native_lifetime.lifetime.status.session_ref != s.session.session_ref {
            return Err(invalid());
        }
        Ok(())
    }

    pub(super) fn validate(&self, s: &TerminationCloseAllocationV2) -> Result<()> {
        if !id_nonempty(&self.attempt_id)
            || self.paid_index >= 24
            || self.claimed_at.get() < s.allocated_at.get()
            || self.native_lifetime.validate().is_err()
            || self.current_observed_at.get() == 0
            || self.current_observed_at.get() > self.claimed_at.get()
            || self.claimed_at.get() >= self.dispatch_until.get()
            || self.dispatch_until.get()
                != s.payload
                    .cleanup_until_ms
                    .0
                    .min(self.native_lifetime.lifetime.status.expires_at.0)
                    .min(self.native_lifetime.lifetime.session_expires_at.0)
                    .min(self.native_lifetime.lifetime.session_idle_expires_at.0)
                    .min(
                        self.current_observed_at
                            .get()
                            .checked_add(5000)
                            .ok_or_else(invalid)?,
                    )
                    .min(self.handoff_until.get())
                    .min(self.manager_session_expires_at.get())
                    .min(self.manager_session_idle_expires_at.get())
            || self.original_digest != original_digest(&s.payload)?
            || self.owner_scope.validate().is_err()
            || self.owner_scope.context.identity.role != Role::Owner
            || self.owner_scope.context.identity.key_id != self.owner_key.key_id
            || self.owner_scope.context.identity.key_version != self.owner_key.key_version
            || self.owner_key.public_key
                != s.payload
                    .permitted_branches
                    .exact_initial_active
                    .owner_public_key
        {
            return Err(invalid());
        }
        Ok(())
    }
}
impl AccountProvisionApprovalState {
    /// Prepay exactly one direct Actor request, before any Vault/key operation.
    /// Duplicates fail; a successful previous allocation never yields a new handle.
    pub fn claim_test_session_close_vault_v2(
        &mut self,
        id: &str,
        attempt: &str,
        current: &TestSessionLiveCurrent<'_, '_>,
        now: UnixMillis,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<PreparedTermCloseSignatureV2> {
        verifier.verify(&self.test_session_v2_live_scope(
            id,
            SessionPurposeV1::TerminationPlan,
            current,
            now,
        )?)?;
        self.test_session_v2_transport_id_available(id, attempt)?;
        if !id_nonempty(attempt) {
            return Err(invalid());
        }
        let s = self.test_session_v2_local(id, SessionPurposeV1::TerminationPlan, now)?;
        let slot = s.termination_state.close.as_ref().ok_or_else(invalid)?;
        let high_water = self.allocated_signer_high_water()?;
        if s.closing.is_none()
            || high_water
                != (
                    slot.payload.revoke_generation.0,
                    slot.payload.revoke_nonce.0,
                )
            || slot.signature_claim.is_some()
            || slot.signed.is_some()
            || slot.source_operation_id != current.source_operation_id
            || now.get() >= slot.payload.cleanup_until_ms.0
        {
            return Err(invalid());
        }
        let (scope, key) = self.term_owner_slot(&s.header()?.0.termination)?;
        let root = s
            .roots
            .get(&SessionPurposeV1::TerminationPlan)
            .ok_or_else(invalid)?;
        let paid_index = 24u64
            .checked_sub(root.budgets.transport.remaining().map_err(|_| invalid())?)
            .ok_or_else(invalid)?;
        let actual = current.provision.approval;
        let dispatch_until = slot
            .payload
            .cleanup_until_ms
            .0
            .min(current.lifetime.lifetime.status.expires_at.0)
            .min(current.lifetime.lifetime.session_expires_at.0)
            .min(current.lifetime.lifetime.session_idle_expires_at.0)
            .min(
                current
                    .observed_at
                    .get()
                    .checked_add(5000)
                    .ok_or_else(invalid)?,
            )
            .min(actual.handoff_until.0)
            .min(actual.source.session_expires_at.0)
            .min(actual.source.session_idle_expires_at.0);
        if now.get() >= dispatch_until {
            return Err(invalid());
        }
        let claim = TerminationCloseVaultClaimV2 {
            attempt_id: attempt.into(),
            claimed_at: now,
            paid_index: u32::try_from(paid_index).map_err(|_| invalid())?,
            original_digest: original_digest(&slot.payload)?,
            native_lifetime: current.lifetime.clone(),
            current_observed_at: current.observed_at,
            handoff_until: UnixMillis::new(actual.handoff_until.0),
            manager_session_expires_at: UnixMillis::new(actual.source.session_expires_at.0),
            manager_session_idle_expires_at: UnixMillis::new(
                actual.source.session_idle_expires_at.0,
            ),
            dispatch_until: UnixMillis::new(dispatch_until),
            owner_scope: scope.clone(),
            owner_key: key.clone(),
        };
        let token = PreparedTermCloseSignatureV2 {
            lineage:
                crate::term_owner_slot_lineage_v2::TermOwnerSlotLineageV2::from_actual_account(
                    self, &scope,
                )?,
            payload: slot.payload.clone(),
            owner_scope: scope,
            owner_key: key,
            attempt_id: attempt.into(),
            input_digest: s.input_digest()?,
            paid_record_digest: digest(&(
                "oasis7.custody.term-close-owner-vault-paid.v2",
                s.input_digest()?,
                &claim,
            ))?,
            session_id: id.into(),
            issued_at: now,
            source_operation_id: slot.source_operation_id.clone(),
            source_record_digest: slot.source_record_digest.clone(),
            until: UnixMillis::new(dispatch_until),
        };
        let mut next = self.clone();
        let s = next.test_session_v2_record_mut(id)?;
        s.roots
            .get_mut(&SessionPurposeV1::TerminationPlan)
            .ok_or_else(invalid)?
            .budgets
            .transport
            .charge(1)
            .map_err(|_| invalid())?;
        s.termination_state
            .close
            .as_mut()
            .ok_or_else(invalid)?
            .signature_claim = Some(claim);
        next.validate_persistence()?;
        *self = next;
        Ok(token)
    }
    pub(in crate::provisioning) fn validate_close_owner_slot(
        &self,
        s: &TestSessionAuthorityV2,
    ) -> Result<()> {
        if let Some(claim) = s
            .termination_state
            .close
            .as_ref()
            .and_then(|s| s.signature_claim.as_ref())
        {
            let (scope, key) = self.term_owner_slot(&s.header()?.0.termination)?;
            if claim.owner_scope != scope || claim.owner_key != key {
                return Err(invalid());
            }
        }
        Ok(())
    }
}

impl AccountProvisionApprovalState {
    /// Historical strong Owner bytes only; never a new Current/send permit.
    /// Late arrival retains the original Until and cannot refresh either slot.
    pub fn record_test_session_close_v2_signature_history(
        &mut self,
        id: &str,
        signed: oasis7_client_api::world_service::SignedReadRequest<GameTaskRevokeAndCloseV2>,
    ) -> Result<()> {
        let s = self.test_session_v2_record(id)?;
        s.termination_state.validate_input_binding(&s.input)?;
        s.termination_state
            .validate_source_binding(&s.sources, &s.source_funding)?;
        let slot = s.termination_state.close.as_ref().ok_or_else(invalid)?;
        if slot.signature_claim.is_none()
            || signed.request != slot.payload
            || signed.subject_public_key
                != slot
                    .payload
                    .permitted_branches
                    .exact_initial_active
                    .owner_public_key
        {
            return Err(invalid());
        }
        oasis7_client_api::world_service::private_goal::verify_owner_signature_strict(
            GAME_TASK_CLEANUP_DOMAIN_V2,
            &signed.request,
            &signed.subject_public_key,
            &signed.signature_hex,
        )
        .map_err(|_| invalid())?;
        if let Some(old) = &slot.signed {
            return if old == &signed {
                Ok(())
            } else {
                Err(invalid())
            };
        }
        let mut next = self.clone();
        next.test_session_v2_record_mut(id)?
            .termination_state
            .close
            .as_mut()
            .ok_or_else(invalid)?
            .signed = Some(signed);
        next.validate_persistence()?;
        *self = next;
        Ok(())
    }
}
