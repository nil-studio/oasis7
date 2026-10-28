use super::*;
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
pub(crate) fn http_pop_signature_checks(
    p: &crate::http_pop::PreparedHttpPop,
) -> Result<ExactSignatureChecks> {
    let scope = canonical_digest(
        "oasis7.http-pop-exact-scope.v1",
        &(&p.key_id, &p.public_key_hex, &p.base),
    )?;
    Ok(ExactSignatureChecks {
        operation_id: format!("http-pop:{scope}"),
        body_digest: scope.clone(),
        context_digest: scope.clone(),
        verify_nonce: scope,
        verification_digest: None,
        checks: vec![SignatureCheck {
            key_id: p.key_id.clone(),
            public_key_hex: p.public_key_hex.clone(),
            signature_base64url: URL_SAFE_NO_PAD.encode(p.signature),
            canonical_bytes: p.base.clone(),
        }],
    })
}
