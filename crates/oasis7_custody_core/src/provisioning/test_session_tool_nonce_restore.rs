use super::*;
impl AccountProvisionApprovalState {
    pub(in crate::provisioning) fn validate_session_tool_nonce_persistence(
        &self,
        s: &TestSessionAuthorityV1,
    ) -> Result<()> {
        if s.tool_nonce_ingresses.is_empty()
            && s.tool_nonce_links.is_empty()
            && !s
                .challenges
                .values()
                .any(|c| c.kind == SessionChallengeKindV1::ToolMetadata)
            && !s.purpose_http_admissions.values().any(|h| {
                matches!(
                    h.class,
                    SessionPurposeHttpAdmissionClassV1::ToolNonceThree { .. }
                )
            })
        {
            return Ok(());
        }
        native_registration(s)?;
        if s.tool_nonce_ingresses.len() > 700 || s.tool_nonce_links.len() > 1024 {
            return Err(invalid());
        }
        let (metadata_uri, action_uri) =
            targets(&self.draft, &s.enrollment.registration.public_origin)?;
        let mut internals = BTreeSet::new();
        for (ingress, record) in &s.tool_nonce_ingresses {
            request_shape(&record.request, &s.input.operation_id)?;
            let root = s.roots.get(&record.request.purpose).ok_or_else(invalid)?;
            let credential = root.credential.as_ref().ok_or_else(invalid)?;
            let pair = attempts(&s.input.operation_id, ingress)?;
            let host = s.purpose_http_admissions.get(ingress).ok_or_else(invalid)?;
            let class = SessionPurposeHttpAdmissionClassV1::ToolNonceThree {
                owner_attempt: pair.0.clone(),
                account_attempt: pair.1.clone(),
            };
            if !id(ingress)
                || !hex64(&record.signed_attempt_digest)
                || !hex64(&record.metadata_body_digest)
                || record.metadata_uri != metadata_uri
                || record.action_uri != action_uri
                || record.owner_attempt != pair.0
                || record.account_attempt != pair.1
                || record.credential_generation.0 == 0
                || record.credential_generation.0 > credential.generation.0
                || record.charged_at.get() < s.approved_at.ok_or_else(invalid)?.get()
                || record.charged_at.get() >= root.deadline.get()
                || host.class != class
                || host.signed_attempt_digest != record.signed_attempt_digest
                || host.purpose != record.request.purpose
                || host.hop != SessionControlHopV1::Host
                || host.charged_at != record.charged_at
            {
                return Err(invalid());
            }
            for attempt in [&record.owner_attempt, &record.account_attempt] {
                if !internals.insert(attempt)
                    || s.purpose_transports.get(attempt)
                        != Some(&SessionPurposeTransportV1::GenericOne(
                            record.request.purpose,
                        ))
                    || s.purpose_http_admissions.contains_key(attempt)
                    || s.sources.contains_key(attempt)
                    || s.source_funding.contains_key(attempt)
                    || s.control_transports.contains_key(attempt)
                    || s.client_http_admissions.contains_key(attempt)
                    || s.actions
                        .values()
                        .any(|a| a.transport_attempts.contains(attempt))
                    || Self::termination_attempt_occupied(s, attempt)
                {
                    return Err(invalid());
                }
            }
        }
        for (ingress, record) in &s.purpose_http_admissions {
            if matches!(
                record.class,
                SessionPurposeHttpAdmissionClassV1::ToolNonceThree { .. }
            ) && !s.tool_nonce_ingresses.contains_key(ingress)
            {
                return Err(invalid());
            }
        }
        for (key, link) in &s.tool_nonce_links {
            let paid = s
                .tool_nonce_ingresses
                .get(&link.first_ingress)
                .ok_or_else(invalid)?;
            let c = s.challenges.get(key).ok_or_else(invalid)?;
            let root = s.roots.get(&link.request.purpose).ok_or_else(invalid)?;
            let bytes = URL_SAFE_NO_PAD.decode(&link.nonce).map_err(|_| invalid())?;
            if key != &link.request.request_id
                || paid.request != link.request
                || paid.credential_generation != link.credential_generation
                || paid.signed_attempt_digest != link.signed_attempt_digest
                || paid.metadata_body_digest != link.metadata_body_digest
                || paid.metadata_uri != link.metadata_uri
                || paid.action_uri != link.action_uri
                || paid.charged_at.get() > link.created_at.get()
                || c.kind != SessionChallengeKindV1::ToolMetadata
                || c.request_id != *key
                || c.purpose != link.request.purpose
                || c.logical_request_id != link.request.logical_request_id
                || c.body_digest != link.request.operation_request_digest
                || c.credential_generation != link.credential_generation
                || c.nonce != link.nonce
                || c.created_at != link.created_at
                || c.expires_at != link.expires_at
                || c.consumed_by.is_some()
                || bytes.len() != 32
                || URL_SAFE_NO_PAD.encode(bytes) != link.nonce
                || link.expires_at.get()
                    != link
                        .created_at
                        .get()
                        .checked_add(30_000)
                        .ok_or_else(invalid)?
                        .min(root.deadline.get())
            {
                return Err(invalid());
            }
        }
        for (key, c) in &s.challenges {
            if c.kind == SessionChallengeKindV1::ToolMetadata
                && !s.tool_nonce_links.contains_key(key)
            {
                return Err(invalid());
            }
        }
        Ok(())
    }
    /// Shared reverse guard for the two internally paid hops. Host has its existing map guard.
    pub(in crate::provisioning) fn tool_nonce_internal_attempt(
        s: &TestSessionAuthorityV1,
        attempt: &str,
    ) -> bool {
        Self::context_attempt_occupied(s, attempt)
            || s.tool_nonce_ingresses
                .values()
                .any(|r| r.owner_attempt == attempt || r.account_attempt == attempt)
    }
}
