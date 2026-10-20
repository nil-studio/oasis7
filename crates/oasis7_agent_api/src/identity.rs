//! Shared identity proof codec, not an authentication or registration service.
//! Callers must validate the trusted service, audience, intended operation and
//! challenge lifetime before signing. Authorities still atomically consume the
//! challenge and enforce registration policy, quotas and credential revocation.

/// Existing native identity signing domain. Keep browser/imported-key proofs
/// byte compatible when adding machine clients.
pub const NATIVE_IDENTITY_DOMAIN_V1: &[u8] = b"oasis7.identity.native.v1\0";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativeIdentityPurposeV1 {
    Login,
    Bind,
    Reauthenticate,
    /// Reserved for autonomous entry. Codec support does not enable a route.
    Register,
    /// Reserved by the native identity contract; not an enabled recovery flow.
    RecoveryCandidate,
}

impl NativeIdentityPurposeV1 {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Login => "login",
            Self::Bind => "bind",
            Self::Reauthenticate => "reauth",
            Self::Register => "register",
            Self::RecoveryCandidate => "recovery_candidate",
        }
    }
}

/// All fields in the signed preimage. Strings are UTF-8 with big-endian u32
/// byte lengths; keys/nonces are exactly 32 bytes; times are big-endian u64
/// milliseconds. JSON, base64 and hexadecimal are transport encodings only.
/// The account context is empty for unauthenticated login/registration.
pub struct NativeIdentityChallengeV1<'a> {
    pub identity_service_id: &'a str,
    pub environment_id: &'a str,
    pub audience: &'a str,
    pub purpose: NativeIdentityPurposeV1,
    pub account_context: &'a str,
    pub operation_id: &'a str,
    pub public_key: &'a [u8; 32],
    pub nonce: &'a [u8; 32],
    pub issued_at: u64,
    pub expires_at: u64,
}

impl NativeIdentityChallengeV1<'_> {
    /// Produces protocol bytes only; it does not verify trust, authorize an
    /// operation, validate an Ed25519 key or issue a signature/session.
    pub fn signing_bytes(&self) -> Result<Vec<u8>, &'static str> {
        let mut out = NATIVE_IDENTITY_DOMAIN_V1.to_vec();
        out.push(1);
        for value in [
            self.identity_service_id,
            self.environment_id,
            self.audience,
            self.purpose.as_str(),
            self.account_context,
            self.operation_id,
        ] {
            let length = u32::try_from(value.len()).map_err(|_| "identity_field_too_long")?;
            out.extend_from_slice(&length.to_be_bytes());
            out.extend_from_slice(value.as_bytes());
        }
        out.extend_from_slice(self.public_key);
        out.extend_from_slice(self.nonce);
        out.extend_from_slice(&self.issued_at.to_be_bytes());
        out.extend_from_slice(&self.expires_at.to_be_bytes());
        Ok(out)
    }
}
