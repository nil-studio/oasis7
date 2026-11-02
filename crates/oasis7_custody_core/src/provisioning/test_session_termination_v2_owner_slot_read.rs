//! First owned invocation pays the fact-only private Actor read before Source6.
//! This record is history. It cannot recreate the returned first dispatch token.
use super::*;
use crate::term_owner_slot_lineage_v2::TermOwnerSlotLineageV2;
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TermOwnerSlotReadRecordV2 {
    pub(in crate::provisioning) attempt_id: String,
    pub(in crate::provisioning) paid_index: u32,
    claimed_at: UnixMillis,
    invocation_digest: String,
    owner_scope: RoleKeySlotScopeV1,
    owner_key: PublicRoleKey,
    lineage: TermOwnerSlotLineageV2,
    until: UnixMillis,
    request_digest: String,
}
/// Only the first paid SQL commit returns this to the same Account handler.
/// No Clone/Serde/Debug or public constructor exists.
pub struct PreparedTermOwnerSlotReadV2 {
    session_id: String,
    record: TermOwnerSlotReadRecordV2,
}
impl PreparedTermOwnerSlotReadV2 {
    pub fn session_id(&self) -> &str {
        &self.session_id
    }
    pub fn attempt_id(&self) -> &str {
        &self.record.attempt_id
    }
    pub fn scope(&self) -> &RoleKeySlotScopeV1 {
        &self.record.owner_scope
    }
    pub fn key(&self) -> &PublicRoleKey {
        &self.record.owner_key
    }
    pub fn issued_at(&self) -> UnixMillis {
        self.record.claimed_at
    }
    pub fn until(&self) -> UnixMillis {
        self.record.until
    }
    pub fn request_digest(&self) -> &str {
        &self.record.request_digest
    }
    pub fn lineage(&self) -> &TermOwnerSlotLineageV2 {
        &self.record.lineage
    }
}
fn request_digest(r: &TermOwnerSlotReadRecordV2) -> Result<String> {
    // Same fixed closed wire as the private Actor request; no source/Current grant.
    #[derive(Serialize)]
    struct Request<'a> {
        api_version: u8,
        paid_attempt_id: &'a str,
        owner_scope: &'a RoleKeySlotScopeV1,
        owner_key: &'a PublicRoleKey,
        issued_at: DecimalU64,
        until: DecimalU64,
    }
    digest(&(
        "oasis7.custody.term-owner-slot-qualification-request.v2",
        Request {
            api_version: 2,
            paid_attempt_id: &r.attempt_id,
            owner_scope: &r.owner_scope,
            owner_key: &r.owner_key,
            issued_at: DecimalU64(r.claimed_at.get()),
            until: DecimalU64(r.until.get()),
        },
    ))
}
impl TermOwnerSlotReadRecordV2 {
    pub(super) fn validate_shape(&self, owned: &TermOwnedInvocationRecordV2) -> Result<()> {
        if self.attempt_id.is_empty()
            || self.attempt_id.len() > 256
            || self.attempt_id.chars().any(char::is_control)
            || self.paid_index >= 24
            || self.claimed_at.get() == 0
            || self.until.get() <= self.claimed_at.get()
            || self.until.get()
                > self
                    .claimed_at
                    .get()
                    .checked_add(5000)
                    .ok_or_else(invalid)?
            || self.invocation_digest != owned.invocation_id()
            || self.request_digest != request_digest(self)?
            || self.lineage.validate_for(&self.owner_scope).is_err()
        {
            return Err(invalid());
        }
        Ok(())
    }
}
impl AccountProvisionApprovalState {
    /// No remote reads occur here. The next actual Actor RPC is paid once.
    pub fn claim_test_session_term_owner_slot_read_v2(
        &mut self,
        owned: &TermOwnedInvocationV2,
        attempt: &str,
        now: UnixMillis,
    ) -> Result<PreparedTermOwnerSlotReadV2> {
        owned.validate_local(self)?;
        let id = owned.session_id();
        self.test_session_v2_transport_id_available(id, attempt)?;
        let s = self.test_session_v2_local(id, SessionPurposeV1::TerminationPlan, now)?;
        if s.termination_state.qualification_read.is_some()
            || s.termination_state.stop.is_some()
            || s.termination_state.guarded_stop.is_some()
            || s.termination_state.close.is_some()
            || !s.termination_state.transports.is_empty()
        {
            return Err(invalid());
        }
        if now.get()
            < s.termination_state
                .owned_invocation
                .as_ref()
                .ok_or_else(invalid)?
                .acknowledged_at(s)?
                .get()
        {
            return Err(invalid());
        }
        let term = &s.header()?.0.termination;
        let (owner_scope, owner_key) = self.term_owner_slot(term)?;
        let root = s
            .roots
            .get(&SessionPurposeV1::TerminationPlan)
            .ok_or_else(invalid)?;
        let index = 24u64
            .checked_sub(root.budgets.transport.remaining().map_err(|_| invalid())?)
            .ok_or_else(invalid)?;
        let until = now
            .get()
            .checked_add(5000)
            .ok_or_else(invalid)?
            .min(term.cleanup_until.get())
            .min(root.deadline.get());
        let mut record = TermOwnerSlotReadRecordV2 {
            attempt_id: attempt.into(),
            paid_index: u32::try_from(index).map_err(|_| invalid())?,
            claimed_at: now,
            invocation_digest: owned.invocation_id().into(),
            lineage: TermOwnerSlotLineageV2::from_actual_account(self, &owner_scope)?,
            owner_scope,
            owner_key,
            until: UnixMillis::new(until),
            request_digest: String::new(),
        };
        record.request_digest = request_digest(&record)?;
        record.validate_shape(
            s.termination_state
                .owned_invocation
                .as_ref()
                .ok_or_else(invalid)?,
        )?;
        let mut next = self.clone();
        let s = next.test_session_v2_record_mut(id)?;
        s.roots
            .get_mut(&SessionPurposeV1::TerminationPlan)
            .ok_or_else(invalid)?
            .budgets
            .transport
            .charge(1)
            .map_err(|_| invalid())?;
        s.termination_state.qualification_read = Some(record.clone());
        next.validate_persistence()?;
        *self = next;
        Ok(PreparedTermOwnerSlotReadV2 {
            session_id: id.into(),
            record,
        })
    }
    pub(in crate::provisioning) fn validate_term_owned_qualification_v2(
        &self,
        s: &TestSessionAuthorityV2,
    ) -> Result<()> {
        if let Some(owned) = &s.termination_state.owned_invocation {
            owned.validate_for_session(s)?;
        }
        // The two new identities occupy the same namespace as Source nonces.
        // This is new-profile history only; no legacy Term record is upgraded.
        if s.sources.values().any(|w| {
            s.termination_state
                .owned_invocation
                .as_ref()
                .is_some_and(|v| v.invocation_id() == w.source.verify_nonce)
                || s.termination_state
                    .qualification_read
                    .as_ref()
                    .is_some_and(|v| v.attempt_id == w.source.verify_nonce)
        }) {
            return Err(invalid());
        }

        if let Some(read) = &s.termination_state.qualification_read {
            let owned = s
                .termination_state
                .owned_invocation
                .as_ref()
                .ok_or_else(invalid)?;
            read.validate_shape(owned)?;
            if read.claimed_at.get() < owned.acknowledged_at(s)?.get() {
                return Err(invalid());
            }
            let term = &s.header()?.0.termination;
            let (scope, key) = self.term_owner_slot(term)?;
            let root = s
                .roots
                .get(&SessionPurposeV1::TerminationPlan)
                .ok_or_else(invalid)?;
            if read.owner_scope != scope
                || read.owner_key != key
                || read.lineage != TermOwnerSlotLineageV2::from_actual_account(self, &scope)?
                || read.until.get()
                    != read
                        .claimed_at
                        .get()
                        .checked_add(5000)
                        .ok_or_else(invalid)?
                        .min(term.cleanup_until.get())
                        .min(root.deadline.get())
            {
                return Err(invalid());
            }
        }
        Ok(())
    }
}
