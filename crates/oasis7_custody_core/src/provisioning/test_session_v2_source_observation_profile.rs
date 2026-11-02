//! Paid immutable first-admit profiles; absent fields retain the legacy contract.
use super::*;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(in crate::provisioning) enum ClientSourceObservationProfileV2 {
    ClientCredentialFiveOracleV2,
    ClientControlFiveOracleV2,
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::provisioning) struct ClientSourceAdmissionV2 {
    pub(in crate::provisioning) ingress_attempt_id: String,
    signed_attempt_digest: String,
}
impl ClientSourceAdmissionV2 {
    pub(in crate::provisioning) fn matches(&self, ingress: &str, digest: &str) -> bool {
        self.ingress_attempt_id == ingress && self.signed_attempt_digest == digest
    }
}
fn source_id(p: ClientSourceObservationProfileV2, id: &str, ingress: &str) -> Result<String> {
    let (prefix, domain) = match p {
        ClientSourceObservationProfileV2::ClientCredentialFiveOracleV2 => {
            ("client", "oasis7.custody.client-ingress-source.v2")
        }
        ClientSourceObservationProfileV2::ClientControlFiveOracleV2 => {
            ("control", "oasis7.custody.control-ingress-source.v2")
        }
    };
    Ok(format!(
        "{prefix}-v2-source-{}",
        crate::approval::bytes_digest(
            &serde_json::to_vec(&(domain, id, ingress)).map_err(|_| invalid())?
        )
    ))
}
pub(in crate::provisioning) fn validate_client_source_profile_v2(
    s: &TestSessionAuthorityV2,
    id: &str,
) -> Result<()> {
    let w = s.sources.get(id).ok_or_else(invalid)?;
    match (w.observation_profile, &w.client_admission) {
        (None, None) => Ok(()),
        (Some(p), Some(b)) => {
            let paid = s
                .client_http_admissions
                .get(&b.ingress_attempt_id)
                .ok_or_else(invalid)?;
            if source_id(p, &s.input.operation_id, &b.ingress_attempt_id)? != id
                || w.approved_input_digest != s.input_digest()?
                || s.source_funding.get(id) != Some(&SessionSourceFundingV1::Control)
                || paid.signed_attempt_digest != b.signed_attempt_digest
                || paid.hop != SessionControlHopV1::Host
                || s.control_transports.get(&b.ingress_attempt_id)
                    != Some(&SessionControlHopV1::Host)
                || w.source.created_at.0 < paid.charged_at.get()
            {
                return Err(invalid());
            }
            Ok(())
        }
        _ => Err(invalid()),
    }
}
pub(in crate::provisioning) fn client_source_tickets_v2(
    s: &TestSessionAuthorityV2,
    id: &str,
    p: SessionJournalProfileV2,
) -> Result<(u8, u8)> {
    validate_client_source_profile_v2(s, id)?;
    match (
        s.sources.get(id).ok_or_else(invalid)?.observation_profile,
        p,
    ) {
        (None, _) => Ok((3, 4)),
        (
            Some(ClientSourceObservationProfileV2::ClientCredentialFiveOracleV2),
            SessionJournalProfileV2::Credential,
        )
        | (
            Some(ClientSourceObservationProfileV2::ClientControlFiveOracleV2),
            SessionJournalProfileV2::Control,
        ) => Ok((4, 5)),
        _ => Err(invalid()),
    }
}
impl AccountProvisionApprovalState {
    /// Stored metadata check only, not Current or dispatch authority.
    pub fn require_test_session_v2_credential_five_source(
        &self,
        id: &str,
        source: &str,
    ) -> Result<()> {
        self.require_client_five_source(
            id,
            source,
            ClientSourceObservationProfileV2::ClientCredentialFiveOracleV2,
        )
    }
    /// Stored metadata check only, not Current or dispatch authority.
    pub fn require_test_session_v2_control_five_source(
        &self,
        id: &str,
        source: &str,
    ) -> Result<()> {
        self.require_client_five_source(
            id,
            source,
            ClientSourceObservationProfileV2::ClientControlFiveOracleV2,
        )
    }
    fn require_client_five_source(
        &self,
        id: &str,
        source: &str,
        p: ClientSourceObservationProfileV2,
    ) -> Result<()> {
        let s = self.test_session_v2_record(id)?;
        validate_client_source_profile_v2(s, source)?;
        if s.sources
            .get(source)
            .ok_or_else(invalid)?
            .observation_profile
            != Some(p)
        {
            return Err(invalid());
        }
        Ok(())
    }
    pub fn reserve_test_session_v2_credential_five_source(
        &mut self,
        id: &str,
        r: SessionSourceReservation<'_>,
        proof: SessionProofRequest<'_>,
        now: UnixMillis,
    ) -> Result<TestSessionSourceV2> {
        let s = self.test_session_v2_record(id)?;
        super::v2_credential_http::public_receipt(s, proof.possession, now)?;
        match proof.possession.profile() {
            HttpPopProfileV1::ClaimChallenge => {
                let q: SessionChallengeRequestV1 = super::lifecycle::original(&proof)?;
                if q.api_version != 1
                    || q.session_id != id
                    || q.kind != SessionChallengeKindV1::CredentialClaim
                    || q.purpose == SessionPurposeV1::TerminationPlan
                    || q.request_id != proof.possession.request_id()
                {
                    return Err(invalid());
                }
            }
            HttpPopProfileV1::CredentialClaim => {
                let q: crate::client_enrollment::ClientCredentialClaimRequestV1 =
                    super::lifecycle::original(&proof)?;
                q.validate().map_err(|_| invalid())?;
                if q.test_session_id != id
                    || q.enrollment_id != s.header()?.1.client_pairing.enrollment_id
                    || q.request_id != proof.possession.request_id()
                {
                    return Err(invalid());
                }
            }
            _ => return Err(invalid()),
        }
        self.reserve_client_five_source(
            id,
            r,
            proof,
            now,
            ClientSourceObservationProfileV2::ClientCredentialFiveOracleV2,
        )
    }
    pub fn reserve_test_session_v2_control_five_source(
        &mut self,
        id: &str,
        r: SessionSourceReservation<'_>,
        proof: SessionProofRequest<'_>,
        now: UnixMillis,
    ) -> Result<TestSessionSourceV2> {
        self.test_session_v2_control_purpose(id, proof.possession, now)?;
        match proof.possession.profile() {
            HttpPopProfileV1::CustodyChallenge => {
                let q: SessionChallengeRequestV1 = super::lifecycle::original(&proof)?;
                if q.api_version != 1
                    || q.session_id != id
                    || !matches!(
                        q.kind,
                        SessionChallengeKindV1::Control(
                            SessionControlKindV1::Initialize
                                | SessionControlKindV1::ToolsList
                                | SessionControlKindV1::Heartbeat
                                | SessionControlKindV1::Finish
                                | SessionControlKindV1::Abort
                        )
                    )
                    || q.request_id != proof.possession.request_id()
                {
                    return Err(invalid());
                }
            }
            HttpPopProfileV1::Custody => {
                let q: SessionControlRequestV1 = super::lifecycle::original(&proof)?;
                if q.api_version != 1
                    || q.session_id != id
                    || !matches!(
                        q.kind,
                        SessionControlKindV1::Initialize
                            | SessionControlKindV1::ToolsList
                            | SessionControlKindV1::Heartbeat
                    )
                    || q.request_id != proof.possession.request_id()
                {
                    return Err(invalid());
                }
            }
            _ => return Err(invalid()),
        }
        self.reserve_client_five_source(
            id,
            r,
            proof,
            now,
            ClientSourceObservationProfileV2::ClientControlFiveOracleV2,
        )
    }
    fn reserve_client_five_source(
        &mut self,
        id: &str,
        r: SessionSourceReservation<'_>,
        proof: SessionProofRequest<'_>,
        now: UnixMillis,
        p: ClientSourceObservationProfileV2,
    ) -> Result<TestSessionSourceV2> {
        self.require_test_session_v2_claim_http_paid(
            id,
            proof.possession,
            proof.ingress_attempt_id,
        )?;
        let source = source_id(p, id, proof.ingress_attempt_id)?;
        if r.funding != SessionSourceFundingV1::Control || r.operation_id != source {
            return Err(invalid());
        }
        if let Some(old) = self.test_session_v2_record(id)?.sources.get(&source) {
            self.require_client_five_source(id, &source, p)?;
            if old.source.verify_nonce != r.nonce
                || old.client_admission.as_ref().is_none_or(|b| {
                    b.ingress_attempt_id != proof.ingress_attempt_id
                        || b.signed_attempt_digest != proof.possession.transport_attempt_digest()
                })
            {
                return Err(ProvisionApprovalError::Conflict);
            }
            return Ok(old.clone());
        }
        let mut next = self.clone();
        next.reserve_test_session_v2_source_inner(id, r, now)?;
        let w = next
            .test_session_v2_record_mut(id)?
            .sources
            .get_mut(&source)
            .ok_or_else(invalid)?;
        w.observation_profile = Some(p);
        w.client_admission = Some(ClientSourceAdmissionV2 {
            ingress_attempt_id: proof.ingress_attempt_id.into(),
            signed_attempt_digest: proof.possession.transport_attempt_digest(),
        });
        let result = w.clone();
        let s = next.test_session_v2_record_mut(id)?;
        let source_reserved_ordinal = session_event_ordinal_v2(
            s,
            &SessionEventKindV2::SourceReserved {
                source_operation_id: source.clone(),
            },
        )?
        .ok_or_else(invalid)?;
        append_session_event_v2(
            s,
            SessionEventKindV2::ClientSourceProfileBorn {
                source_operation_id: source,
                profile: p,
                ingress_attempt_id: proof.ingress_attempt_id.into(),
                signed_attempt_digest: proof.possession.transport_attempt_digest(),
                source_reserved_ordinal,
            },
            now,
        )?;
        next.validate_persistence()?;
        *self = next;
        Ok(result)
    }
}
