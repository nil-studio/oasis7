use super::*;
use subtle::ConstantTimeEq;
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionChallengeRequestV1 {
    pub api_version: u8,
    pub session_id: String,
    pub request_id: String,
    pub purpose: SessionPurposeV1,
    pub kind: SessionChallengeKindV1,
    pub logical_request_id: String,
    pub operation_request_digest: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "request",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum SessionToolActionV1 {
    Execute(crate::approval::ApprovedActionV1),
    ReconcileOperation { operation_id: String },
    ReadCloseState,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionToolRequestV1 {
    pub api_version: u8,
    pub session_id: String,
    pub request_id: String,
    pub action: SessionToolActionV1,
}
/// Supplied by a fixed private pricing/World adapter, never read from HTTP body.
pub struct SessionActionCost {
    pub business_units: u64,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionCredentialAllocation {
    pub purpose: SessionPurposeV1,
    pub digest: String,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionCredentialClaimOutcome {
    pub newly_claimed: bool,
    pub roots: Vec<(SessionPurposeV1, DecimalU64)>,
}
pub(super) fn hex_bytes(s: &str) -> Result<[u8; 32]> {
    if !hex64(s) {
        return Err(invalid());
    }
    let mut v = [0; 32];
    for (i, p) in s.as_bytes().chunks_exact(2).enumerate() {
        v[i] = u8::from_str_radix(core::str::from_utf8(p).map_err(|_| invalid())?, 16)
            .map_err(|_| invalid())?;
    }
    Ok(v)
}
pub(super) fn public_receipt(
    s: &TestSessionAuthorityV1,
    receipt: &HttpPossessionReceipt,
    now: UnixMillis,
) -> Result<()> {
    let (_, p, _, _, _) = admission::pairing(&s.input.request.intent)?;
    if receipt.binding_id() != s.input.operation_id
        || receipt.key_id() != p.client_key_id
        || receipt.public_key_hex() != p.client_public_key
        || receipt
            .expires()
            .checked_millis()
            .map_err(|_| invalid())?
            .get()
            <= now.get()
    {
        return Err(invalid());
    }
    Ok(())
}
pub(super) fn authenticated_receipt(
    s: &TestSessionAuthorityV1,
    purpose: SessionPurposeV1,
    receipt: &HttpPossessionReceipt,
    now: UnixMillis,
) -> Result<()> {
    public_receipt(s, receipt, now)?;
    let root = s.roots.get(&purpose).ok_or_else(invalid)?;
    let credential = root.credential.as_ref().ok_or_else(invalid)?;
    if purpose == SessionPurposeV1::TerminationPlan
        || !matches!(
            receipt.profile(),
            HttpPopProfileV1::Custody
                | HttpPopProfileV1::CustodyChallenge
                | HttpPopProfileV1::Mcp
                | HttpPopProfileV1::McpChallenge
        )
        || hex_bytes(&credential.digest)?
            .ct_eq(receipt.token_digest())
            .unwrap_u8()
            != 1
    {
        return Err(invalid());
    }
    Ok(())
}
impl AccountProvisionApprovalState {
    pub(super) fn test_session_local(
        &self,
        id: &str,
        purpose: SessionPurposeV1,
        now: UnixMillis,
    ) -> Result<&TestSessionAuthorityV1> {
        let s = self.test_sessions.get(id).ok_or_else(invalid)?;
        let root = s.roots.get(&purpose).ok_or_else(invalid)?;
        if self.session_inventory != SessionInventoryV1::Known
            || self.frozen()
            || s.approved_at.is_none()
            || root.revoked
            || now.get() >= root.deadline.get()
            || s.input.request.context.security_version != self.provision.security_epoch()
            || (purpose == SessionPurposeV1::Execution
                && (s.closing.is_some()
                    || session_control_exhausted(s)
                    || s.lease_until.is_none_or(|v| now.get() >= v.get())))
        {
            return Err(invalid());
        }
        Ok(s)
    }
    pub fn test_session_live_checks(
        &self,
        id: &str,
        purpose: SessionPurposeV1,
        current: &TestSessionLiveCurrent<'_, '_>,
        now: UnixMillis,
    ) -> Result<ExactSignatureChecks> {
        let s = self.test_session_local(id, purpose, now)?;
        let c = current.provision.approval;
        let r = s
            .book
            .original(id)
            .filter(|r| r.stage() == ApprovalAdmissionStage::Consumed)
            .ok_or_else(invalid)?;
        r.check_recorded_context(c)?;
        let source = s
            .sources
            .get(current.source_operation_id)
            .ok_or_else(invalid)?;
        let signed = source.verification.as_ref().ok_or_else(invalid)?;
        if current.source_ticket == 0
            || current.source_ticket
                > *s.source_oracle_reads
                    .get(current.source_operation_id)
                    .ok_or_else(invalid)?
        {
            return Err(invalid());
        }
        if c.authority != s.input.request.context
            || c.frozen
            || c.fence.check_epoch(c.authority.service_epoch.0).is_err()
            || current.provision.world_identity != &self.draft.world_identity
            || current.observed_at.get() > now.get()
            || now.get() - current.observed_at.get() > 5000
            || now.get() >= c.source.session_expires_at.0
            || now.get() >= c.source.session_idle_expires_at.0
            || current.provision.policy.version != self.draft.entry.bootstrap_config_version.0
            || !current.provision.policy.managers.iter().any(|m| {
                m.principal == self.draft.entry.manager
                    && m.environments.contains(&self.draft.entry.environment)
            })
        {
            return Err(invalid());
        }
        let checks = super::source::validate_inspection_record(r, source, signed, c, now.get())?;
        let status = &current.lifetime.lifetime.status;
        let nonce = self
            .test_session_live_descriptor(id, purpose, current.source_operation_id)?
            .lifetime_nonce;
        let original = s.native_lifetime.as_ref().ok_or_else(invalid)?;
        current
            .lifetime
            .lifetime
            .validate_for(
                &original.lifetime.status.receipt,
                &nonce,
                s.session.native_binding_generation,
                c.authority.service_epoch,
                now.get(),
            )
            .map_err(|_| invalid())?;
        current
            .lifetime
            .lifetime
            .covers(s.roots.get(&purpose).ok_or_else(invalid)?.deadline)
            .map_err(|_| invalid())?;
        if current.private_sid_generation != s.session.private_sid_generation
            || status.session_ref != s.session.session_ref
            || status.receipt.identity_namespace != current.provision.native_identity_namespace
        {
            return Err(invalid());
        }
        Ok(append_native_history_checks(
            checks,
            c,
            NativeFact::Lifetime(current.lifetime),
            current.native_scope.native_root_key_id,
            current.native_scope.native_root_public_key,
            &current.lifetime.lifetime.digest().map_err(|_| invalid())?,
        )?)
    }
    pub fn reserve_test_session_source(
        &mut self,
        id: &str,
        reservation: SessionSourceReservation<'_>,
        now: UnixMillis,
    ) -> Result<ProvisionOriginInspectionV1> {
        let SessionSourceReservation {
            operation_id,
            nonce,
            funding,
        } = reservation;
        let s = self.test_sessions.get(id).ok_or_else(invalid)?;
        if let Some(old) = s.sources.get(operation_id) {
            return if old.verify_nonce == nonce
                && s.source_funding.get(operation_id) == Some(&funding)
            {
                Ok(old.clone())
            } else {
                Err(ProvisionApprovalError::Conflict)
            };
        }
        if self.frozen()
            || !id_nonempty(operation_id)
            || !hex64(nonce)
            || s.consumed_at.is_none()
            || s.sources.len() >= 160
        {
            return Err(invalid());
        }
        let (_, _, _, read, cleanup) = admission::pairing(&s.input.request.intent)?;
        let deadline = if let Some(purpose) = funding.purpose() {
            self.test_session_local(id, purpose, now)?
                .roots
                .get(&purpose)
                .ok_or_else(invalid)?
                .deadline
                .get()
        } else {
            read.max(cleanup)
        };
        if now.get() >= deadline {
            return Err(invalid());
        }
        let mut next = self.clone();
        let s = next.test_sessions.get_mut(id).ok_or_else(invalid)?;
        if let Some(purpose) = funding.purpose() {
            s.roots
                .get_mut(&purpose)
                .ok_or_else(invalid)?
                .budgets
                .transport
                .charge(1)
                .map_err(|_| invalid())?
        } else {
            s.identity_transport
                .as_mut()
                .ok_or_else(invalid)?
                .charge(1)
                .map_err(|_| invalid())?
        }
        let source = ProvisionOriginInspectionV1 {
            operation_id: operation_id.into(),
            verify_nonce: nonce.into(),
            created_at: DecimalU64(now.get()),
            expires_at: DecimalU64(
                now.get()
                    .checked_add(30_000)
                    .ok_or_else(invalid)?
                    .min(deadline),
            ),
            claimed_at: None,
            request_bytes: None,
            verification: None,
            verified_at: None,
        };
        s.sources.insert(operation_id.into(), source.clone());
        s.source_oracle_reads.insert(operation_id.into(), 0);
        s.source_funding.insert(operation_id.into(), funding);
        next.validate_persistence()?;
        *self = next;
        Ok(source)
    }
    /// Local tightening before Directory/Fence I/O; a ticket conveys no authority.
    pub fn reserve_test_session_source_read(
        &mut self,
        id: &str,
        source_id: &str,
        now: UnixMillis,
    ) -> Result<u8> {
        if self.frozen() {
            return Err(invalid());
        }
        let mut next = self.clone();
        let s = next.test_sessions.get_mut(id).ok_or_else(invalid)?;
        let source = s.sources.get(source_id).ok_or_else(invalid)?;
        if now.get() >= source.expires_at.0 {
            return Err(invalid());
        }
        let reads = s
            .source_oracle_reads
            .get_mut(source_id)
            .ok_or_else(invalid)?;
        if *reads >= 8 {
            return Err(invalid());
        }
        *reads += 1;
        let result = *reads;
        next.validate_persistence()?;
        *self = next;
        Ok(result)
    }
    pub fn claim_test_session_source(
        &mut self,
        id: &str,
        source_id: &str,
        current: &ProvisionApprovalContext<'_>,
        now: UnixMillis,
    ) -> Result<crate::approval::ApprovalVerifyRequestV1> {
        let mut next = self.clone();
        let s = next.test_sessions.get_mut(id).ok_or_else(invalid)?;
        let r = s.book.original(id).ok_or_else(invalid)?;
        r.check_recorded_context(current.approval)?;
        let source = s.sources.get_mut(source_id).ok_or_else(invalid)?;
        if self.frozen()
            || current.approval.frozen
            || current
                .approval
                .fence
                .check_epoch(current.approval.authority.service_epoch.0)
                .is_err()
            || current.approval.authority != s.input.request.context
            || source.claimed_at.is_some()
            || now.get() >= source.expires_at.0
        {
            return Err(invalid());
        }
        let mut request = r.native_verify_request()?;
        request.purpose = crate::approval::VerificationPurposeV1::InspectOrigin;
        request.verify_nonce = source.verify_nonce.clone();
        source.claimed_at = Some(DecimalU64(now.get()));
        source.request_bytes = Some(serde_json::to_string(&request).map_err(|_| invalid())?);
        next.validate_persistence()?;
        *self = next;
        Ok(request)
    }
    pub fn test_session_source_checks(
        &self,
        id: &str,
        source_id: &str,
        signed: &SignedApprovalVerificationV1,
        current: &ProvisionApprovalContext<'_>,
        now: UnixMillis,
    ) -> Result<ExactSignatureChecks> {
        let s = self.test_sessions.get(id).ok_or_else(invalid)?;
        let r = s.book.original(id).ok_or_else(invalid)?;
        r.check_recorded_context(current.approval)?;
        if self.frozen()
            || current.approval.authority != s.input.request.context
            || current.approval.frozen
            || current
                .approval
                .fence
                .check_epoch(current.approval.authority.service_epoch.0)
                .is_err()
        {
            return Err(invalid());
        }
        super::source::validate_inspection_record(
            r,
            s.sources.get(source_id).ok_or_else(invalid)?,
            signed,
            current.approval,
            now.get(),
        )
    }
    pub fn complete_test_session_source(
        &mut self,
        id: &str,
        source_id: &str,
        signed: SignedApprovalVerificationV1,
        current: &ProvisionApprovalContext<'_>,
        now: UnixMillis,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<()> {
        verifier.verify(&self.test_session_source_checks(id, source_id, &signed, current, now)?)?;
        let mut next = self.clone();
        let source = next
            .test_sessions
            .get_mut(id)
            .and_then(|s| s.sources.get_mut(source_id))
            .ok_or_else(invalid)?;
        source.verification = Some(signed);
        source.verified_at = Some(DecimalU64(now.get()));
        next.validate_persistence()?;
        *self = next;
        Ok(())
    }
}
#[path = "test_session_credentials.rs"]
mod credentials;

fn id_nonempty(v: &str) -> bool {
    !v.is_empty() && v.len() <= 256 && !v.chars().any(char::is_control)
}

pub struct SessionCredentialClaim<'a> {
    pub allocations: &'a [SessionCredentialAllocation],
    pub proof: SessionProofRequest<'a>,
}
