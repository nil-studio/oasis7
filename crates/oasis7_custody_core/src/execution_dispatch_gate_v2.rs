//! Fixed Native10 dispatch claims. Reservations and signatures alone are not send authority.
use crate::{DecimalU64, native_cleanup_v2::*, native_execution_prepare_v2::*};
use serde::{Deserialize, Serialize};
pub const EXECUTION_DISPATCH_CLAIM_DOMAIN_V2: &str = "oasis7.custody.execution-dispatch-claim.v2";
pub const NATIVE_EXECUTION_REENTRY_DOMAIN_V2: &str = "oasis7.custody.native-execution-reentry.v2";
pub const NATIVE_EXECUTION_REENTRY_PATH_V2: &str =
    "/api/custody/execution/native-dispatch-claim-v2";
fn invalid() -> String {
    "execution dispatch claim rejected".into()
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionReentryHopV2 {
    NativeToIdentity,
    AbuseProviderActor,
    AbuseRealmActor,
    IdentityToOwnerService,
    OwnerToAccountActor,
}
pub fn reentry_hops(edge: ExecutionPreparePhysicalEdgeV2) -> &'static [ExecutionReentryHopV2] {
    use ExecutionReentryHopV2::*;
    match edge {
        ExecutionPreparePhysicalEdgeV2::AccountToIdentityService => &[],
        ExecutionPreparePhysicalEdgeV2::NativeToWorldCurrent
        | ExecutionPreparePhysicalEdgeV2::NativeToWorldPrepare => &[
            NativeToIdentity,
            AbuseProviderActor,
            AbuseRealmActor,
            IdentityToOwnerService,
            OwnerToAccountActor,
        ],
        ExecutionPreparePhysicalEdgeV2::IdentityServiceToNativeActor
        | ExecutionPreparePhysicalEdgeV2::SidCipherReadBefore
        | ExecutionPreparePhysicalEdgeV2::SidCipherReadAfter
        | ExecutionPreparePhysicalEdgeV2::CsrfCipherReadBefore
        | ExecutionPreparePhysicalEdgeV2::CsrfCipherReadAfter
        | ExecutionPreparePhysicalEdgeV2::IdentityActorToNative
        | ExecutionPreparePhysicalEdgeV2::IdentityActorToContextBody => {
            &[IdentityToOwnerService, OwnerToAccountActor]
        }
    }
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionReentryPortV2 {
    pub hop: ExecutionReentryHopV2,
    pub attempt_id: String,
}
/// Public Account-signed receipt for one physical business leaf, never the subtree.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionDispatchClaimV2 {
    pub api_version: u8,
    pub issuer: AccountMachineIssuerV2,
    pub native_proof_digest: String,
    pub reservation_ordinal: DecimalU64,
    pub claim_ordinal: DecimalU64,
    pub session_id: String,
    pub approved_input_digest: String,
    pub source_record_digest: String,
    pub leaf: ExecutionPreparePhysicalPortV2,
    pub body_digest: String,
    pub callbacks: Vec<ExecutionReentryPortV2>,
    pub issued_at: DecimalU64,
    pub expires_at: DecimalU64,
}
impl ExecutionDispatchClaimV2 {
    pub fn validate_relative(
        &self,
        request: &NativeExecutionPrepareRequestV2,
    ) -> Result<(), String> {
        request.wire_bytes()?;
        let c = &request.proof.certificate;
        let hops = reentry_hops(self.leaf.edge);
        if self.api_version != 2
            || self.issuer != c.issuer
            || self.native_proof_digest != request.proof.digest()?
            || self.reservation_ordinal.0 == 0
            || self.claim_ordinal.0 <= self.reservation_ordinal.0
            || self.session_id != c.session_id
            || self.approved_input_digest != c.approved_input_digest
            || self.source_record_digest != c.source_record_digest
            || !c.ports.contains(&self.leaf)
            || !hex64(&self.body_digest)
            || self.callbacks.len() != hops.len()
            || self.issued_at.0 < c.issued_at.0
            || self.issued_at.0 >= self.expires_at.0
            || self.expires_at.0 > c.expires_at.0
            || self
                .callbacks
                .iter()
                .zip(hops)
                .any(|(p, h)| p.hop != *h || !hex64(&p.attempt_id))
        {
            return Err(invalid());
        }
        let expected: Vec<_> = execution_reentry_inventory(request)?
            .into_iter()
            .filter(|p| p.edge == self.leaf.edge)
            .map(|p| p.port)
            .collect();
        if self.callbacks != expected
            || request.proof.certificate.accepted_ordinal.0.checked_add(1)
                != Some(self.reservation_ordinal.0)
        {
            return Err(invalid());
        }
        let mut seen = std::collections::BTreeSet::new();
        if self.callbacks.iter().any(|p| {
            !seen.insert(&p.attempt_id)
                || c.ports.iter().any(|leaf| leaf.attempt_id == p.attempt_id)
        }) {
            return Err(invalid());
        }
        Ok(())
    }
    pub fn signing_bytes(
        &self,
        request: &NativeExecutionPrepareRequestV2,
    ) -> Result<Vec<u8>, String> {
        self.validate_relative(request)?;
        bytes(EXECUTION_DISPATCH_CLAIM_DOMAIN_V2, self)
    }
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedExecutionDispatchClaimV2 {
    pub claim: ExecutionDispatchClaimV2,
    pub signature_hex: String,
}
impl SignedExecutionDispatchClaimV2 {
    pub fn verify_issuer(
        &self,
        request: &NativeExecutionPrepareRequestV2,
        pin: &AccountMachineIssuerPinV2,
        floor: &AccountMachineIssuerFloorV2,
        now: u64,
    ) -> Result<(), String> {
        self.claim.validate_relative(request)?;
        self.claim.issuer.verify_pin(
            pin,
            floor,
            AccountMachinePurposeV2::ExecutionDispatchClaimV2,
            self.claim.issued_at.0,
            self.claim.expires_at.0,
            now,
        )?;
        verify(
            EXECUTION_DISPATCH_CLAIM_DOMAIN_V2,
            &self.claim,
            &self.signature_hex,
            &pin.public_key,
        )
    }
    pub fn digest(&self) -> Result<String, String> {
        hash(self)
    }
}
impl core::fmt::Debug for SignedExecutionDispatchClaimV2 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("SignedExecutionDispatchClaimV2(<one physical leaf>)")
    }
}

/// Native's fixed World-leaf callback request. This signs no caller-selected
/// URL, account key or World message and grants nothing without Account SQL.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeExecutionReentryQueryV2 {
    pub api_version: u8,
    pub native_key_id: String,
    pub identity_origin: String,
    pub native_proof_digest: String,
    pub leaf: ExecutionPreparePhysicalPortV2,
    pub body_digest: String,
    pub callback_attempt_ids: [String; 5],
    pub nonce: String,
    pub issued_at: DecimalU64,
    pub expires_at: DecimalU64,
}
impl NativeExecutionReentryQueryV2 {
    pub fn validate_relative(
        &self,
        request: &NativeExecutionPrepareRequestV2,
    ) -> Result<(), String> {
        request.wire_bytes()?;
        let c = &request.proof.certificate;
        if self.api_version != 2
            || !id(&self.native_key_id)
            || self.native_proof_digest != request.proof.digest()?
            || !matches!(
                self.leaf.edge,
                ExecutionPreparePhysicalEdgeV2::NativeToWorldCurrent
                    | ExecutionPreparePhysicalEdgeV2::NativeToWorldPrepare
            )
            || !c.ports.contains(&self.leaf)
            || !hex64(&self.body_digest)
            || self.nonce != native_reentry_nonce(request, self.leaf.edge)?
            || crate::http_pop::CanonicalHttpTarget::new(
                &self.identity_origin,
                crate::http_pop::HttpPopRoute::ProofChallenges,
            )
            .is_err()
            || self.issued_at.0 < c.issued_at.0
            || self.issued_at.0 >= self.expires_at.0
            || self.expires_at.0 > c.expires_at.0
        {
            return Err(invalid());
        }
        let expected: Vec<_> = execution_reentry_inventory(request)?
            .into_iter()
            .filter(|p| p.edge == self.leaf.edge)
            .map(|p| p.port.attempt_id)
            .collect();
        if self.callback_attempt_ids.as_slice() != expected.as_slice() {
            return Err(invalid());
        }
        let mut unique = std::collections::BTreeSet::new();
        if self.callback_attempt_ids.iter().any(|id| {
            !hex64(id) || !unique.insert(id) || c.ports.iter().any(|p| p.attempt_id == *id)
        }) {
            return Err(invalid());
        }
        Ok(())
    }
    pub fn signing_bytes(
        &self,
        request: &NativeExecutionPrepareRequestV2,
    ) -> Result<Vec<u8>, String> {
        self.validate_relative(request)?;
        bytes(NATIVE_EXECUTION_REENTRY_DOMAIN_V2, self)
    }
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedNativeExecutionReentryQueryV2 {
    pub query: NativeExecutionReentryQueryV2,
    pub signature_hex: String,
}
impl SignedNativeExecutionReentryQueryV2 {
    pub fn verify_native(
        &self,
        request: &NativeExecutionPrepareRequestV2,
        key_id: &str,
        public_key: &str,
        expected_identity_origin: &str,
        now: u64,
    ) -> Result<(), String> {
        self.query.validate_relative(request)?;
        if self.query.native_key_id != key_id
            || self.query.identity_origin != expected_identity_origin
            || now < self.query.issued_at.0
            || now >= self.query.expires_at.0
        {
            return Err(invalid());
        }
        verify(
            NATIVE_EXECUTION_REENTRY_DOMAIN_V2,
            &self.query,
            &self.signature_hex,
            public_key,
        )
    }
}
impl core::fmt::Debug for SignedNativeExecutionReentryQueryV2 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("SignedNativeExecutionReentryQueryV2(<fixed World leaf>)")
    }
}

impl core::fmt::Debug for ExecutionReentryPortV2 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("ExecutionReentryPortV2")
            .field("hop", &self.hop)
            .finish_non_exhaustive()
    }
}

pub fn native_reentry_nonce(
    request: &NativeExecutionPrepareRequestV2,
    edge: ExecutionPreparePhysicalEdgeV2,
) -> Result<String, String> {
    if !matches!(
        edge,
        ExecutionPreparePhysicalEdgeV2::NativeToWorldCurrent
            | ExecutionPreparePhysicalEdgeV2::NativeToWorldPrepare
    ) {
        return Err(invalid());
    }
    let leaf = request
        .proof
        .certificate
        .ports
        .iter()
        .find(|p| p.edge == edge)
        .ok_or_else(invalid)?;
    hash(&(
        "oasis7.custody.native-execution-reentry-nonce.v2",
        request.proof.digest()?,
        &leaf.attempt_id,
        edge,
    ))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionReentryInventoryPortV2 {
    pub edge: ExecutionPreparePhysicalEdgeV2,
    pub port: ExecutionReentryPortV2,
}
pub fn execution_reentry_inventory(
    request: &NativeExecutionPrepareRequestV2,
) -> Result<Vec<ExecutionReentryInventoryPortV2>, String> {
    request.wire_bytes()?;
    let proof = request.proof.digest()?;
    let mut out = Vec::new();
    for leaf in &request.proof.certificate.ports {
        for hop in reentry_hops(leaf.edge) {
            out.push(ExecutionReentryInventoryPortV2 {
                edge: leaf.edge,
                port: ExecutionReentryPortV2 {
                    hop: *hop,
                    attempt_id: hash(&(
                        "oasis7.custody.execution-native45-callback.v2",
                        &proof,
                        &leaf.attempt_id,
                        leaf.edge,
                        hop,
                    ))?,
                },
            });
        }
    }
    if out.len() != 24 {
        return Err(invalid());
    }
    Ok(out)
}

/// New fixed Native callback wire. No Cookie, private body or caller-selected
/// callback path is carried; the previous receipt is an exact causal witness.
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeExecutionDispatchCallbackV2 {
    pub api_version: u8,
    pub request: Box<NativeExecutionPrepareRequestV2>,
    pub previous: SignedExecutionDispatchClaimV2,
    pub query: SignedNativeExecutionReentryQueryV2,
}
impl NativeExecutionDispatchCallbackV2 {
    pub fn wire_bytes(&self) -> Result<Vec<u8>, String> {
        if self.api_version != 2 {
            return Err(invalid());
        }
        self.request.wire_bytes()?;
        self.previous.claim.validate_relative(&self.request)?;
        self.query.query.validate_relative(&self.request)?;
        let raw = serde_json::to_vec(self).map_err(|_| invalid())?;
        if raw.len() > 128 * 1024 {
            return Err(invalid());
        }
        Ok(raw)
    }
}
