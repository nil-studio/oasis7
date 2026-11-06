//! Supplemental Term3 origin observations, paid from the new signed T cap.
//! Reuses the existing Native InspectOrigin strong-signature contract only;
//! no legacy source_funding or legacy Termination root budget is touched.
use super::*;
pub(super) fn verify_exact(checks: &ExactSignatureChecks) -> Result<()> {
    use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
    for c in checks.checks() {
        let key: [u8; 32] = decode_hex(c.public_key_hex())
            .map_err(|_| invalid())?
            .try_into()
            .map_err(|_| invalid())?;
        let key = ed25519_dalek::VerifyingKey::from_bytes(&key).map_err(|_| invalid())?;
        if key.is_weak() {
            return Err(invalid());
        }
        let sig = URL_SAFE_NO_PAD
            .decode(c.signature_base64url())
            .map_err(|_| invalid())?;
        let sig = ed25519_dalek::Signature::from_slice(&sig).map_err(|_| invalid())?;
        key.verify_strict(c.canonical_bytes(), &sig)
            .map_err(|_| invalid())?;
    }
    Ok(())
}
impl AccountProvisionApprovalState {
    pub(super) fn validate_termination_sources_v3(&self, subject: &str) -> Result<()> {
        let a = self.termination_v3.get(subject).ok_or_else(invalid)?;
        let r = a.book.original(&a.input.operation_id).ok_or_else(invalid)?;
        // Restore validates actual signatures, not a persisted verified boolean.
        verify_exact(&a.book.begin_checks(
            &a.input,
            r.verify_nonce(),
            r.recorded_context(),
            0, // existing-record branch selects its persisted admitted_at internally
        )?)?;
        if let Some(signed) = r.verification() {
            verify_exact(&a.book.commit_checks(
                &a.input.operation_id,
                signed,
                r.recorded_context(),
                r.consumed_at().ok_or_else(invalid)?,
            )?)?;
        }
        let mut nonces = std::collections::BTreeSet::new();
        for (id, source) in &a.sources {
            if id != &source.operation_id
                || !nonces.insert(&source.verify_nonce)
                || source.created_at.0 < a.installed_at.ok_or_else(invalid)?.get()
                || source.expires_at.0
                    != source
                        .created_at
                        .0
                        .checked_add(30000)
                        .ok_or_else(invalid)?
                        .min(a.plan.cleanup_until.get())
                || source.claimed_at.is_some() != source.request_bytes.is_some()
                || source.verification.is_some() != source.verified_at.is_some()
            {
                return Err(invalid());
            }
            if let Some(at) = source.claimed_at {
                if at.0 < source.created_at.0 || at.0 >= source.expires_at.0 {
                    return Err(invalid());
                }
                let mut req = r.native_verify_request()?;
                req.purpose = crate::approval::VerificationPurposeV1::InspectOrigin;
                req.verify_nonce = source.verify_nonce.clone();
                if source.request_bytes.as_deref()
                    != Some(serde_json::to_string(&req).map_err(|_| invalid())?.as_str())
                {
                    return Err(invalid());
                }
            }
            if let (Some(signed), Some(at)) = (&source.verification, source.verified_at) {
                let mut context = r.recorded_context().clone();
                context.source.session_expires_at = signed.verification.source_session_expires_at;
                context.source.session_idle_expires_at =
                    signed.verification.source_session_idle_expires_at;
                verify_exact(&super::source::validate_inspection_record(
                    r, source, signed, &context, at.0,
                )?)?;
            }
        }
        Ok(())
    }
    pub fn reserve_termination_source_v3(
        &mut self,
        subject: &str,
        source_id: &str,
        verify_nonce: &str,
        now: UnixMillis,
    ) -> Result<ProvisionOriginInspectionV1> {
        if !id(source_id)
            || verify_nonce.len() != 64
            || !verify_nonce
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(invalid());
        }
        let a = self.termination_v3.get(subject).ok_or_else(invalid)?;
        if a.installed_at.is_none() || now.get() >= a.plan.cleanup_until.get() {
            return Err(invalid());
        }
        if let Some(old) = a.sources.get(source_id) {
            return if old.verify_nonce == verify_nonce {
                Ok(old.clone())
            } else {
                Err(ProvisionApprovalError::Conflict)
            };
        }
        if self
            .termination_v3
            .values()
            .any(|t| t.sources.values().any(|s| s.verify_nonce == verify_nonce))
        {
            return Err(ProvisionApprovalError::Conflict);
        }
        let source = ProvisionOriginInspectionV1 {
            operation_id: source_id.into(),
            verify_nonce: verify_nonce.into(),
            created_at: DecimalU64(now.get()),
            expires_at: DecimalU64(
                now.get()
                    .checked_add(30000)
                    .ok_or_else(invalid)?
                    .min(a.plan.cleanup_until.get()),
            ),
            claimed_at: None,
            request_bytes: None,
            verified_at: None,
            verification: None,
        };
        let mut next = self.clone();
        // All eight concrete physical ports are listed by this fixed prepayment.
        // This precedes even Directory/RF observations. Failure does not refund.
        next.prepay_termination_source_v3(subject, source_id, now)?;
        next.termination_v3
            .get_mut(subject)
            .ok_or_else(invalid)?
            .sources
            .insert(source_id.into(), source.clone());
        next.validate_termination_v3_persistence()?;
        *self = next;
        Ok(source)
    }
    pub fn claim_termination_source_v3(
        &mut self,
        subject: &str,
        source_id: &str,
        now: UnixMillis,
    ) -> Result<crate::approval::ApprovalVerifyRequestV1> {
        let a = self.termination_v3.get(subject).ok_or_else(invalid)?;
        let source = a.sources.get(source_id).ok_or_else(invalid)?;
        if source.claimed_at.is_some() || now.get() >= source.expires_at.0 {
            return Err(invalid());
        }
        let r = a.book.original(&a.input.operation_id).ok_or_else(invalid)?;
        let mut request = r.native_verify_request()?;
        request.purpose = crate::approval::VerificationPurposeV1::InspectOrigin;
        request.verify_nonce = source.verify_nonce.clone();
        let bytes = serde_json::to_string(&request).map_err(|_| invalid())?;
        let mut next = self.clone();
        let source = next
            .termination_v3
            .get_mut(subject)
            .ok_or_else(invalid)?
            .sources
            .get_mut(source_id)
            .ok_or_else(invalid)?;
        source.claimed_at = Some(DecimalU64(now.get()));
        source.request_bytes = Some(bytes);
        next.validate_termination_v3_persistence()?;
        *self = next;
        Ok(request)
    }
    pub fn record_termination_source_v3(
        &mut self,
        subject: &str,
        source_id: &str,
        signed: crate::approval::SignedApprovalVerificationV1,
        current: &ProvisionApprovalContext<'_>,
        now: UnixMillis,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<()> {
        let a = self.termination_v3.get(subject).ok_or_else(invalid)?;
        let source = a.sources.get(source_id).ok_or_else(invalid)?;
        let record = a.book.original(&a.input.operation_id).ok_or_else(invalid)?;
        record.check_recorded_context(current.approval)?;
        let checks = super::source::validate_inspection_record(
            record,
            source,
            &signed,
            current.approval,
            now.get(),
        )?;
        verifier.verify(&checks)?;
        if let Some(old) = &source.verification {
            return if old == &signed {
                Ok(())
            } else {
                Err(ProvisionApprovalError::Conflict)
            };
        }
        let mut next = self.clone();
        let source = next
            .termination_v3
            .get_mut(subject)
            .ok_or_else(invalid)?
            .sources
            .get_mut(source_id)
            .ok_or_else(invalid)?;
        source.verification = Some(signed);
        source.verified_at = Some(DecimalU64(now.get()));
        next.validate_termination_v3_persistence()?;
        *self = next;
        Ok(())
    }
    pub fn termination_source_checks_v3(
        &self,
        subject: &str,
        source_id: &str,
        current: &ProvisionApprovalContext<'_>,
        now: UnixMillis,
    ) -> Result<ExactSignatureChecks> {
        let a = self.termination_v3.get(subject).ok_or_else(invalid)?;
        let source = a.sources.get(source_id).ok_or_else(invalid)?;
        let signed = source.verification.as_ref().ok_or_else(invalid)?;
        let record = a.book.original(&a.input.operation_id).ok_or_else(invalid)?;
        record.check_recorded_context(current.approval)?;
        super::source::validate_inspection_record(
            record,
            source,
            signed,
            current.approval,
            now.get(),
        )
    }
}

fn decode_hex(value: &str) -> Result<Vec<u8>> {
    if value.len() % 2 != 0
        || !value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(invalid());
    }
    (0..value.len() / 2)
        .map(|i| u8::from_str_radix(&value[i * 2..i * 2 + 2], 16).map_err(|_| invalid()))
        .collect()
}
