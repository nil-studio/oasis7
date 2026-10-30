//! Session-local nonce history. Nonce uniqueness never substitutes for authority.
use super::*;
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
fn nonce_bytes(value: &str) -> Option<Vec<u8>> {
    if hex64(value) {
        value
            .as_bytes()
            .chunks_exact(2)
            .map(|v| u8::from_str_radix(core::str::from_utf8(v).ok()?, 16).ok())
            .collect()
    } else if value.len() == 43 {
        let bytes = URL_SAFE_NO_PAD.decode(value).ok()?;
        (bytes.len() == 32 && URL_SAFE_NO_PAD.encode(&bytes) == value).then_some(bytes)
    } else {
        None
    }
}
fn matches(a: &str, b: &str) -> bool {
    a == b
        || nonce_bytes(a)
            .zip(nonce_bytes(b))
            .is_some_and(|(a, b)| a == b)
}
impl AccountProvisionApprovalState {
    pub(in crate::provisioning) fn live_context_nonce_reserved(
        s: &TestSessionAuthorityV1,
        nonce: &str,
    ) -> bool {
        s.context.as_ref().is_some_and(|c| {
            c.live_reads.values().any(|r| {
                matches(&r.request.observation_nonce, nonce)
                    || r.gateway_read
                        .observation_nonce
                        .as_deref()
                        .is_some_and(|v| matches(v, nonce))
            })
        })
    }
    pub(super) fn live_context_nonce_occupied(
        id: &str,
        s: &TestSessionAuthorityV1,
        nonce: &str,
    ) -> Result<bool> {
        Ok(Self::live_context_nonce_reserved(s, nonce)
            || Self::live_context_legacy_nonce(id, s, nonce)?)
    }
    pub(super) fn live_context_legacy_nonce(
        id: &str,
        s: &TestSessionAuthorityV1,
        nonce: &str,
    ) -> Result<bool> {
        let original = s
            .book
            .original(id)
            .ok_or_else(invalid)?
            .native_verify_request()
            .map_err(|_| invalid())?;
        if matches(&original.verify_nonce, nonce)
            || s.preflight_source_nonce
                .as_deref()
                .is_some_and(|v| matches(v, nonce))
            || s.native_lifetime
                .as_ref()
                .is_some_and(|v| matches(&v.lifetime.status.nonce, nonce))
            || s.preflight_lifetime
                .as_ref()
                .is_some_and(|v| matches(&v.lifetime.status.nonce, nonce))
            || s.preflight_read
                .as_ref()
                .and_then(|v| v.read.request.observation_nonce.as_deref())
                .is_some_and(|v| matches(v, nonce))
            || s.challenges.values().any(|v| matches(&v.nonce, nonce))
            || s.context
                .as_ref()
                .is_some_and(|c| matches(&c.original.request.client_nonce, nonce))
        {
            return Ok(true);
        }
        for (source, record) in &s.sources {
            if matches(&record.verify_nonce, nonce)
                || matches(
                    &digest(&(
                        "oasis7.custody.test-session-current-lifetime.v1",
                        id,
                        source,
                        &record.verify_nonce,
                    ))?,
                    nonce,
                )
            {
                return Ok(true);
            }
        }
        Ok(false)
    }
}
