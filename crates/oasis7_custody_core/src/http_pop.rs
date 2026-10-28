//! Fixed RFC 9421/9530 client possession profile. A verified signature is not
//! enrollment, a capability grant, replay consumption, or budget admission.
use crate::{
    approval_admission::{ApprovalSignatureVerifier, ExactSignatureChecks},
    time_units::{UnixMillis, UnixSeconds},
};
use base64::{
    Engine,
    engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;
pub const MAX_HTTP_BODY_BYTES: usize = 32768;
pub const MAX_HTTP_HEADER_BYTES: usize = 8192;
pub const MAX_HTTP_HEADER_VALUE_BYTES: usize = 2048;
pub const MAX_HTTP_HEADER_INSTANCES: usize = 32;
pub const MAX_HTTP_TOKEN_BYTES: usize = 1024;
pub const MAX_HTTP_IDENTIFIER_BYTES: usize = 256;
pub const HTTP_SIGNATURE_LABEL: &str = "oasis7";
pub const MCP_PROTOCOL_VERSION: &str = "2025-11-25";
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HttpPopError {
    Bounds,
    Target,
    Header,
    Binding,
    Digest,
    Time,
    Signature,
    Crypto,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HttpPopProfileV1 {
    Custody,
    Mcp,
    EnrollmentProof,
    CredentialClaim,
    CustodyChallenge,
    McpChallenge,
    ClaimChallenge,
}
impl HttpPopProfileV1 {
    pub fn tag(self) -> &'static str {
        match self {
            Self::Custody => "oasis7-custody-client-v1",
            Self::Mcp => "oasis7-mcp-client-v1",
            Self::EnrollmentProof => "oasis7-custody-enrollment-proof-v1",
            Self::CredentialClaim => "oasis7-custody-enroll-v1",
            Self::CustodyChallenge | Self::ClaimChallenge => "oasis7-custody-challenge-v1",
            Self::McpChallenge => "oasis7-mcp-challenge-v1",
        }
    }
    pub fn is_challenge(self) -> bool {
        matches!(
            self,
            Self::CustodyChallenge | Self::McpChallenge | Self::ClaimChallenge
        )
    }
    pub fn is_mcp(self) -> bool {
        matches!(self, Self::Mcp | Self::McpChallenge)
    }
    fn empty_token(self) -> bool {
        matches!(
            self,
            Self::EnrollmentProof | Self::CredentialClaim | Self::ClaimChallenge
        )
    }
    fn binding_header(self) -> &'static str {
        if self == Self::EnrollmentProof {
            "oasis7-enrollment"
        } else {
            "oasis7-test-session"
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HttpPopRoute {
    ScopedClient {
        provision_id: String,
        custody_account_id: String,
        route: Box<HttpPopRoute>,
    },
    Mcp,
    ProofChallenges,
    EnrollmentProve(String),
    EnrollmentClaim(String),
    Actions,
    Operation(String),
    TestSession(String),
    Initialize(String),
    ToolsList(String),
    Heartbeat(String),
    Finish(String),
    Abort(String),
}
impl HttpPopRoute {
    fn profile_route(&self) -> &Self {
        match self {
            Self::ScopedClient { route, .. } => route,
            _ => self,
        }
    }
    pub fn method(&self) -> &'static str {
        if matches!(self, Self::Operation(_) | Self::TestSession(_)) {
            "GET"
        } else {
            "POST"
        }
    }
    pub fn path(&self) -> Result<String, HttpPopError> {
        if let Self::ScopedClient {
            provision_id,
            custody_account_id,
            route,
        } = self
        {
            if !identifier(provision_id)
                || !identifier(custody_account_id)
                || !matches!(
                    route.as_ref(),
                    Self::ProofChallenges
                        | Self::EnrollmentClaim(_)
                        | Self::Initialize(_)
                        | Self::ToolsList(_)
                        | Self::Heartbeat(_)
                        | Self::Finish(_)
                        | Self::Abort(_)
                )
            {
                return Err(HttpPopError::Target);
            }
            let tail = route.path()?;
            return Ok(format!(
                "/v1/account-provisions/{provision_id}/accounts/{custody_account_id}/client/{}",
                tail.strip_prefix("/v1/").ok_or(HttpPopError::Target)?
            ));
        }
        let dynamic = match self {
            Self::EnrollmentProve(id)
            | Self::EnrollmentClaim(id)
            | Self::Operation(id)
            | Self::TestSession(id)
            | Self::Initialize(id)
            | Self::ToolsList(id)
            | Self::Heartbeat(id)
            | Self::Finish(id)
            | Self::Abort(id) => Some(id),
            _ => None,
        };
        if dynamic.is_some_and(|id| !identifier(id)) {
            return Err(HttpPopError::Target);
        }
        Ok(match self {
            Self::ScopedClient { .. } => return Err(HttpPopError::Target),
            Self::Mcp => "/mcp".into(),
            Self::ProofChallenges => "/v1/proof-challenges".into(),
            Self::EnrollmentProve(id) => format!("/v1/client-enrollments/{id}/prove"),
            Self::EnrollmentClaim(id) => format!("/v1/client-enrollments/{id}/claim"),
            Self::Actions => "/v1/actions".into(),
            Self::Operation(id) => format!("/v1/operations/{id}"),
            Self::TestSession(id) => format!("/v1/test-sessions/{id}"),
            Self::Initialize(id) => format!("/v1/test-sessions/{id}/initialize"),
            Self::ToolsList(id) => format!("/v1/test-sessions/{id}/tools-list"),
            Self::Heartbeat(id) => format!("/v1/test-sessions/{id}/heartbeat"),
            Self::Finish(id) => format!("/v1/test-sessions/{id}/finish"),
            Self::Abort(id) => format!("/v1/test-sessions/{id}/abort"),
        })
    }
}
/// Construct only from fixed deployment origin and a closed handler route.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanonicalHttpTarget {
    origin: String,
    route: HttpPopRoute,
    uri: String,
}
impl CanonicalHttpTarget {
    pub fn new(origin: &str, route: HttpPopRoute) -> Result<Self, HttpPopError> {
        validate_origin(origin)?;
        let uri = format!("{origin}{}", route.path()?);
        Ok(Self {
            origin: origin.into(),
            route,
            uri,
        })
    }
    pub fn uri(&self) -> &str {
        &self.uri
    }
    pub fn method(&self) -> &str {
        self.route.method()
    }
    pub fn origin(&self) -> &str {
        &self.origin
    }
}
#[derive(Clone, Copy)]
pub struct HttpHeader<'a> {
    pub name: &'a str,
    pub value: &'a str,
}
#[derive(Clone, PartialEq, Eq)]
pub struct OwnedHttpHeader {
    pub name: String,
    pub value: String,
}
impl core::fmt::Debug for OwnedHttpHeader {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("OwnedHttpHeader(<redacted>)")
    }
}
/// WorkerMergedSingletons is explicitly a provider-coalesced representation,
/// never a claim to possess original instances. Relevant singleton grammars
/// reject commas; adapters must not split a merged field to invent originals.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HttpHeaderRepresentation {
    RawInstances,
    WorkerMergedSingletons,
}
pub struct HttpPopMessage<'a> {
    pub method: &'a str,
    pub target_uri: &'a str,
    pub headers: &'a [HttpHeader<'a>],
    pub representation: HttpHeaderRepresentation,
    pub body: &'a [u8],
}
/// All expected binding fields come from trusted deployment/registered state,
/// not from request JSON or a key URL supplied by the client.
pub struct HttpPopBinding<'a> {
    pub profile: HttpPopProfileV1,
    pub target: &'a CanonicalHttpTarget,
    pub key_id: &'a str,
    pub public_key_hex: &'a str,
    pub binding_id: &'a str,
    pub request_id: &'a str,
    pub nonce: Option<&'a str>,
    pub credential_digest: &'a [u8; 32],
    pub valid_until: UnixMillis,
}
pub struct HttpPopSigningInput<'a> {
    pub profile: HttpPopProfileV1,
    pub target: &'a CanonicalHttpTarget,
    pub key_id: &'a str,
    pub binding_id: &'a str,
    pub request_id: &'a str,
    pub nonce: Option<&'a str>,
    pub token: Option<&'a str>,
    pub created: UnixSeconds,
    pub expires: UnixSeconds,
    pub body: &'a [u8],
}
pub struct HttpPopSigningMaterial {
    base: Vec<u8>,
    headers: Vec<OwnedHttpHeader>,
}
impl HttpPopSigningMaterial {
    pub fn signature_base(&self) -> &[u8] {
        &self.base
    }
    pub fn headers(&self) -> &[OwnedHttpHeader] {
        &self.headers
    }
    pub fn signed_headers(&self, signature: &[u8; 64]) -> Vec<OwnedHttpHeader> {
        let mut h = self.headers.clone();
        h.push(OwnedHttpHeader {
            name: "signature".into(),
            value: format!("oasis7=:{}:", STANDARD.encode(signature)),
        });
        h
    }
}
pub struct PreparedHttpPop {
    pub(crate) key_id: String,
    pub(crate) public_key_hex: String,
    pub(crate) signature: [u8; 64],
    pub(crate) base: Vec<u8>,
    profile: HttpPopProfileV1,
    binding_id: String,
    request_id: String,
    nonce: Option<String>,
    method: String,
    target_uri: String,
    body_digest: [u8; 32],
    token_digest: [u8; 32],
    created: UnixSeconds,
    expires: UnixSeconds,
    valid_until: UnixMillis,
}
impl core::fmt::Debug for PreparedHttpPop {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("PreparedHttpPop(<redacted>)")
    }
}
impl PreparedHttpPop {
    pub fn signature_checks(&self) -> Result<ExactSignatureChecks, HttpPopError> {
        crate::approval_admission::http_pop_signature_checks(self).map_err(|_| HttpPopError::Crypto)
    }
}
/// Possession only. The receiving Account/inbox must still atomically recheck
/// current binding/purpose/epoch, consume nonce and charge the correct ledger.
pub struct HttpPossessionReceipt {
    proof: PreparedHttpPop,
}
impl core::fmt::Debug for HttpPossessionReceipt {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("HttpPossessionReceipt(<not-authorization>)")
    }
}
impl HttpPossessionReceipt {
    /// Replay/accounting key derives from actual verified signed octets, never a caller ID.
    pub fn transport_attempt_digest(&self) -> String {
        let mut hash = Sha256::new();
        hash.update(b"oasis7-http-pop-attempt-v1\0");
        hash.update(&self.proof.base);
        hash.update(self.proof.signature);
        hash.finalize().iter().map(|b| format!("{b:02x}")).collect()
    }
    pub fn profile(&self) -> HttpPopProfileV1 {
        self.proof.profile
    }
    pub fn binding_id(&self) -> &str {
        &self.proof.binding_id
    }
    pub fn request_id(&self) -> &str {
        &self.proof.request_id
    }
    pub fn key_id(&self) -> &str {
        &self.proof.key_id
    }
    pub fn nonce(&self) -> Option<&str> {
        self.proof.nonce.as_deref()
    }
    pub fn target_uri(&self) -> &str {
        &self.proof.target_uri
    }
    pub fn method(&self) -> &str {
        &self.proof.method
    }
    pub fn public_key_hex(&self) -> &str {
        &self.proof.public_key_hex
    }
    pub fn created(&self) -> UnixSeconds {
        self.proof.created
    }
    pub fn body_digest(&self) -> &[u8; 32] {
        &self.proof.body_digest
    }
    pub fn token_digest(&self) -> &[u8; 32] {
        &self.proof.token_digest
    }
    pub fn expires(&self) -> UnixSeconds {
        self.proof.expires
    }
}
pub fn verify_http_possession(
    proof: PreparedHttpPop,
    now: UnixMillis,
    verifier: &mut impl ApprovalSignatureVerifier,
) -> Result<HttpPossessionReceipt, HttpPopError> {
    validate_time(proof.created, proof.expires, now, proof.valid_until)?;
    verify_ed25519(&proof)?;
    verifier
        .verify(&proof.signature_checks()?)
        .map_err(|_| HttpPopError::Crypto)?;
    Ok(HttpPossessionReceipt { proof })
}
pub fn content_digest(body: &[u8]) -> String {
    format!("sha-256=:{}:", STANDARD.encode(sha256(body)))
}
pub fn token_digest(token: &str) -> Result<[u8; 32], HttpPopError> {
    if token.is_empty()
        || token.len() > MAX_HTTP_TOKEN_BYTES
        || !token
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-._~+/=".contains(&b))
    {
        return Err(HttpPopError::Binding);
    }
    Ok(sha256(token.as_bytes()))
}
fn sha256(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}
fn identifier(s: &str) -> bool {
    !s.is_empty()
        && s != "."
        && s != ".."
        && s.len() <= MAX_HTTP_IDENTIFIER_BYTES
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-._~".contains(&b))
}
fn key_id(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 128
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-._~:".contains(&b))
}
fn nonce_valid(s: &str) -> bool {
    URL_SAFE_NO_PAD
        .decode(s)
        .is_ok_and(|v| v.len() == 32 && URL_SAFE_NO_PAD.encode(&v) == s)
}
fn ct_equal(a: &[u8; 32], b: &[u8; 32]) -> bool {
    bool::from(a.ct_eq(b))
}
#[path = "http_pop_codec.rs"]
mod codec;
pub use codec::{build_http_pop_signing_material, prepare_http_pop};
#[path = "http_pop_validation.rs"]
mod validation;
use validation::*;

#[cfg(test)]
#[path = "http_pop_rfc_vectors.rs"]
mod rfc_vectors;

impl core::fmt::Display for HttpPopError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "invalid fixed HTTP possession: {self:?}")
    }
}
impl std::error::Error for HttpPopError {}

/// Public evidence of an enrollment proof only; no live possession receipt and
/// no raw capability/OAuth token can be serialized into this history record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnrollmentHttpEvidenceV1 {
    pub api_version: u8,
    pub signature_base: String,
    pub signature_base64url: String,
    pub created: UnixSeconds,
    pub expires: UnixSeconds,
}
impl HttpPossessionReceipt {
    pub fn enrollment_evidence(&self) -> Result<EnrollmentHttpEvidenceV1, HttpPopError> {
        if self.profile() != HttpPopProfileV1::EnrollmentProof {
            return Err(HttpPopError::Binding);
        }
        Ok(EnrollmentHttpEvidenceV1 {
            api_version: 1,
            signature_base: String::from_utf8(self.proof.base.clone())
                .map_err(|_| HttpPopError::Binding)?,
            signature_base64url: URL_SAFE_NO_PAD.encode(self.proof.signature),
            created: self.proof.created,
            expires: self.proof.expires,
        })
    }
}
pub(crate) fn enrollment_evidence_checks(
    e: &EnrollmentHttpEvidenceV1,
    input: &HttpPopSigningInput<'_>,
    public_key_hex: &str,
    proved_at: UnixMillis,
    valid_until: UnixMillis,
) -> Result<ExactSignatureChecks, HttpPopError> {
    if e.api_version != 1
        || input.profile != HttpPopProfileV1::EnrollmentProof
        || e.created != input.created
        || e.expires != input.expires
        || e.signature_base.len() > MAX_HTTP_HEADER_BYTES
    {
        return Err(HttpPopError::Binding);
    }
    let material = build_http_pop_signing_material(input)?;
    if material.signature_base() != e.signature_base.as_bytes() {
        return Err(HttpPopError::Binding);
    }
    let signature: [u8; 64] = URL_SAFE_NO_PAD
        .decode(&e.signature_base64url)
        .map_err(|_| HttpPopError::Signature)?
        .try_into()
        .map_err(|_| HttpPopError::Signature)?;
    if URL_SAFE_NO_PAD.encode(signature) != e.signature_base64url {
        return Err(HttpPopError::Signature);
    }
    let p = PreparedHttpPop {
        key_id: input.key_id.into(),
        public_key_hex: public_key_hex.into(),
        signature,
        base: material.base,
        profile: input.profile,
        binding_id: input.binding_id.into(),
        request_id: input.request_id.into(),
        nonce: input.nonce.map(str::to_owned),
        method: input.target.method().into(),
        target_uri: input.target.uri().into(),
        body_digest: sha256(input.body),
        token_digest: sha256(b""),
        created: e.created,
        expires: e.expires,
        valid_until,
    };
    validate_time(e.created, e.expires, proved_at, valid_until)?;
    verify_ed25519(&p)?;
    p.signature_checks()
}

#[path = "http_pop_nonce_hint.rs"]
mod nonce_hint;
pub use nonce_hint::http_pop_nonce_hint;
