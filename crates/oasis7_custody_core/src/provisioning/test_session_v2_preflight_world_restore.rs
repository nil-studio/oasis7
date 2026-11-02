//! Strict two-port inventory and independent fixed-Native receipt verification.
use super::*;
use crate::native_cleanup_v2::{NativePreflightWorldOperationV2, SignedAccountPreflightAccessV2};
impl TestSessionPreflightWorldV2 {
    pub(in crate::provisioning) fn transport_id_in_use(&self, id: &str) -> bool {
        self.certificate
            .ports
            .iter()
            .any(|p| p.physical_attempt_id == id)
    }
    pub(in crate::provisioning) fn signed(&self) -> Result<SignedAccountPreflightAccessV2> {
        Ok(SignedAccountPreflightAccessV2 {
            certificate: self.certificate.as_ref().clone(),
            signature_hex: self.signature_hex.clone().ok_or_else(invalid)?,
        })
    }
    pub(in crate::provisioning) fn admitted(&self) -> bool {
        self.admitted_at.is_some() && self.receipts.iter().all(Option::is_some)
    }
    pub(in crate::provisioning) fn validate_session(
        &self,
        s: &TestSessionAuthorityV2,
    ) -> Result<()> {
        let c = &self.certificate;
        c.validate_shape().map_err(|_| invalid())?;
        if c.input.as_ref() != &s.input
            || c.approved_input_digest != s.input_digest()?
            || c.descriptor.session != s.session
            || s.consumed_at.map(|t| t.get()) != Some(c.consumed_at.0)
            || s.preflight_steps
                .get(&SessionPreflightStepV1::FreshAccessPair)
                .map(|t| t.get())
                != Some(c.fresh_access_claimed_at.0)
            || c.issued_at != c.fresh_access_claimed_at
            || c.source_record_digest
                != digest(
                    s.sources
                        .get(&c.descriptor.source_operation_id)
                        .ok_or_else(invalid)?,
                )?
        {
            return Err(invalid());
        }
        c.issuer
            .verify_pin(
                &self.issuer_pin,
                &self.issuer_floor,
                crate::native_cleanup_v2::AccountMachinePurposeV2::NativePreflightAccessV2,
                c.issued_at.0,
                c.expires_at.0,
                c.issued_at.0,
            )
            .map_err(|_| invalid())?;
        if let Some(signature) = &self.signature_hex {
            self.signed()?
                .verify_issuer(&self.issuer_pin, &self.issuer_floor, c.issued_at.0)
                .map_err(|_| invalid())?;
            if signature.is_empty() {
                return Err(invalid());
            }
        }
        let count = self.receipts.iter().filter(|r| r.is_some()).count();
        if count != 0 && count != 2
            || (count == 2) != self.admitted_at.is_some()
            || count == 2 && (self.signature_hex.is_none() || s.preflight_access.is_none())
        {
            return Err(invalid());
        }
        if count == 2 {
            let access = s.preflight_access.as_ref().ok_or_else(invalid)?;
            let proof = self.signed()?;
            for (index, operation, result) in [
                (
                    0,
                    NativePreflightWorldOperationV2::PolicyOriginal,
                    digest(&access.policy)?,
                ),
                (
                    1,
                    NativePreflightWorldOperationV2::AuthorizePolicy,
                    digest(&access.admission)?,
                ),
            ] {
                let r = self.receipts[index].as_ref().ok_or_else(invalid)?;
                if r.receipt.operation != operation {
                    return Err(invalid());
                }
                r.receipt
                    .validate_relative(&proof, &result)
                    .map_err(|_| invalid())?;
                if self.admitted_at.ok_or_else(invalid)?.get() < r.receipt.completed_at.0 {
                    return Err(invalid());
                }
            }
        }
        Ok(())
    }
    /// Shape replay above cannot establish this fixed external Root by itself.
    pub(in crate::provisioning) fn verify_native_history(
        &self,
        s: &TestSessionAuthorityV2,
        native: &NativeStageScope<'_>,
    ) -> Result<()> {
        if !self.admitted() {
            return Ok(());
        }
        let e = s.preflight_access.as_ref().ok_or_else(invalid)?;
        let original = self.signed()?;
        for (i, result) in [digest(&e.policy)?, digest(&e.admission)?]
            .into_iter()
            .enumerate()
        {
            self.receipts[i]
                .as_ref()
                .ok_or_else(invalid)?
                .verify_native(
                    &original,
                    &result,
                    native.native_root_key_id,
                    native.native_root_public_key,
                )
                .map_err(|_| invalid())?;
        }
        Ok(())
    }
}
impl AccountProvisionApprovalState {
    pub(in crate::provisioning) fn validate_session_v2_preflight_world(
        &self,
        id: &str,
        s: &TestSessionAuthorityV2,
    ) -> Result<()> {
        let CustodyIntentV1::TestSessionV2 { plan, .. } = &s.input.request.intent else {
            return Err(invalid());
        };
        let new = plan.termination.cleanup_transport_policy
            == CleanupTransportPolicyV2::TermWideFiveSecondOwnedWorld3V2;
        let Some(w) = &s.preflight_world_access else {
            return if new
                && s.preflight_steps
                    .contains_key(&SessionPreflightStepV1::FreshAccessPair)
            {
                Err(invalid())
            } else {
                Ok(())
            };
        };
        if !new {
            return Err(invalid());
        }
        w.validate_session(s)?;
        if s.preflight_access.is_some() != w.admitted() {
            return Err(invalid());
        }
        let c = &w.certificate;
        if c.descriptor.as_ref() != &self.test_session_v2_preflight_descriptor(id)?
            || c.certificate_id
                != digest(&(
                    "oasis7.custody.preflight-world-allocation.v2",
                    id,
                    s.input_digest()?,
                    &c.source_record_digest,
                    UnixMillis::new(c.fresh_access_claimed_at.0),
                ))?
        {
            return Err(invalid());
        }
        let source = s
            .sources
            .get(&c.descriptor.source_operation_id)
            .ok_or_else(invalid)?;
        let v = &source
            .source
            .verification
            .as_ref()
            .ok_or_else(invalid)?
            .verification;
        if c.expires_at.0 > source.source.expires_at.0
            || c.expires_at.0 > v.observed_at.0.checked_add(5000).ok_or_else(invalid)?
            || c.expires_at.0 > v.source_session_expires_at.0
            || c.expires_at.0 > v.source_session_idle_expires_at.0
            || c.expires_at.0 > v.proof_expires_at.0
            || c.issued_at.0 < v.observed_at.0
        {
            return Err(invalid());
        }
        for p in &c.ports {
            let expected = digest(&(
                "oasis7.custody.preflight-world-physical.v2",
                &c.certificate_id,
                p.operation,
                p.native_path.as_str(),
                &p.body_digest,
            ))?;
            if p.physical_attempt_id != expected
                || super::source_v2::transport_id_in_non_world2_state_v2(s, &expected)
            {
                return Err(invalid());
            }
        }
        Ok(())
    }
}
