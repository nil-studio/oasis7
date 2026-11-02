//! Closed pending Source inspection transport only. Codec verification does not attest payment or Current.
use crate::{
    DecimalU64,
    approval::{ApprovalVerifyRequestV1, VerificationPurposeV1},
    native_cleanup_v2::{
        AccountMachineIssuerFloorV2, AccountMachineIssuerPinV2, AccountMachineIssuerV2,
        AccountMachinePurposeV2, bytes, hash, hex64, verify,
    },
    role_key_slot::RoleKeySlotScopeV1,
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::{Deserialize, Serialize};
pub const SOURCE_INSPECT_DISPATCH_DOMAIN_V2: &str =
    "oasis7.custody.execution-pending-source-inspect-dispatch.v2";
pub const SOURCE_NATIVE_LEAF_DOMAIN_V2: &str =
    "oasis7.custody.execution-pending-source-native-leaf.v2";
pub const SOURCE_INSPECT_DISPATCH_PATH_V2: &str = "/internal/execution-source-inspect-v2";
pub const SOURCE_NATIVE_PATH_V2: &str =
    "/api/public/hosted-account/v2/custody/pending-source-inspect";
// Preserve the existing Identity native transport bound; never enlarge the generic actor helper.
pub const SOURCE_INSPECT_MAX_WIRE_BYTES_V2: usize = 32 * 1024;
fn rejected() -> String {
    "pending source inspect rejected".into()
}
fn id(s: &str) -> bool {
    !s.is_empty() && s.len() <= 256 && s.trim() == s && !s.chars().any(char::is_control)
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PendingSourceInspectBindingV2 {
    pub api_version: u8,
    pub issuer: AccountMachineIssuerV2,
    pub scope: RoleKeySlotScopeV1,
    pub session_id: String,
    pub ingress_attempt_id: String,
    pub signed_attempt_digest: String,
    pub action: crate::provisioning::SessionActionRequestV2,
    pub approved_input_digest: String,
    pub master_digest: String,
    pub source_operation_id: String,
    pub source_verify_nonce: String,
    pub opaque_registration_ref: String,
    pub active_certificate_digest: String,
    pub native_binding: crate::native_binding::NativeBindingReceiptV1,
    pub native_session_ref: String,
    pub native_binding_generation: DecimalU64,
    pub session: crate::provisioning::ParticipantSessionRefV1,
    pub source_request_digest: String,
    pub expected_service_epoch: DecimalU64,
    pub accepted_ordinal: DecimalU64,
    pub dispatcher_claim_ordinal: DecimalU64,
    pub dispatcher_attempt_id: String,
    pub native_attempt_id: String,
    pub dispatcher_object_id: String,
    pub issued_at: DecimalU64,
    pub expires_at: DecimalU64,
}
impl PendingSourceInspectBindingV2 {
    pub fn validate_shape(&self) -> Result<(), String> {
        self.scope.validate().map_err(|_| rejected())?;
        self.native_binding.validate().map_err(|_| rejected())?;
        self.session.scope.validate().map_err(|_| rejected())?;
        if self.api_version != 2
            || !id(&self.session_id)
            || !id(&self.ingress_attempt_id)
            || !id(&self.source_operation_id)
            || !id(&self.dispatcher_attempt_id)
            || !id(&self.native_attempt_id)
            || self.dispatcher_attempt_id == self.native_attempt_id
            || !hex64(&self.signed_attempt_digest)
            || self.action.api_version != 2
            || self.action.session_id != self.session_id
            || !id(&self.action.request_id)
            || self.action.purpose() != crate::provisioning::SessionPurposeV1::Execution
            || !hex64(&self.approved_input_digest)
            || !hex64(&self.master_digest)
            || !hex64(&self.source_verify_nonce)
            || !hex64(&self.source_request_digest)
            || !hex64(&self.dispatcher_object_id)
            || !hex64(&self.active_certificate_digest)
            || !id(&self.opaque_registration_ref)
            || !id(&self.native_session_ref)
            || self.native_binding_generation.0 == 0
            || self.session.session_ref != self.native_session_ref
            || self.session.native_binding_generation != self.native_binding_generation
            || self.session.scope.role_scope != self.scope
            || self.native_binding.environment != self.issuer.environment
            || self.scope.context.identity.account_id != self.native_binding.custody_account_id
            || self.scope.provision_id != self.native_binding.provision_id
            || self.session.scope.identity_namespace != self.native_binding.identity_namespace
            || self.session.scope.identity_instance_epoch
                != self.native_binding.identity_instance_epoch
            || self.expected_service_epoch.0 == 0
            || self.accepted_ordinal.0 == 0
            || self.dispatcher_claim_ordinal.0 <= self.accepted_ordinal.0
            || self.issued_at.0 == 0
            || self.issued_at.0 >= self.expires_at.0
            || self.expires_at.0 - self.issued_at.0 > 5000
            || self.scope.context.identity.role != crate::http::Role::Identity
            || self.scope.context.identity.environment != self.issuer.environment
        {
            return Err(rejected());
        }
        Ok(())
    }
    pub fn receiver_name(&self) -> Result<String, String> {
        source_inspect_receiver_name_v2(&self.issuer)
    }
    pub fn check_request(&self, q: &ApprovalVerifyRequestV1) -> Result<(), String> {
        self.validate_shape()?;
        if q.purpose != VerificationPurposeV1::InspectOrigin
            || q.verify_nonce != self.source_verify_nonce
            || q.approval.proof.context.service_epoch != self.expected_service_epoch
            || q.approval.proof.context.environment != self.issuer.environment
            || q.approval.proof.context.custody_account_id != self.scope.context.identity.account_id
            || q.approval.proof.identity_namespace != self.native_binding.identity_namespace
            || q.approval.proof.identity_instance_epoch
                != self.native_binding.identity_instance_epoch
            || q.proof_digest != q.approval.digest()?
            || q.expected_intent_digest != q.approval.proof.intent_digest
            || q.expected_audience != q.approval.proof.context.audience
            || crate::approval::bytes_digest(&serde_json::to_vec(q).map_err(|_| rejected())?)
                != self.source_request_digest
        {
            return Err(rejected());
        }
        Ok(())
    }
}
/// Stable server partition excludes key/config epochs, sessions and attempts.
/// Namespace and real object ID must still be compared by the receiver.
pub fn source_inspect_receiver_name_v2(issuer: &AccountMachineIssuerV2) -> Result<String, String> {
    if [
        &issuer.namespace,
        &issuer.service_id,
        &issuer.environment,
        &issuer.native_audience_origin,
    ]
    .iter()
    .any(|s| !id(s))
    {
        return Err(rejected());
    }
    hash(&(
        "oasis7.custody.source-inspect-dispatcher-object.v2",
        &issuer.namespace,
        &issuer.service_id,
        &issuer.environment,
        &issuer.native_audience_origin,
    ))
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedPendingSourceInspectDispatchV2 {
    pub binding: PendingSourceInspectBindingV2,
    pub signature_hex: String,
}
impl SignedPendingSourceInspectDispatchV2 {
    pub fn wire_bytes(&self) -> Result<Vec<u8>, String> {
        self.binding.validate_shape()?;
        bounded(self)
    }
    pub fn decode(raw: &[u8]) -> Result<Self, String> {
        let q: Self = decode(raw)?;
        q.wire_bytes()?;
        Ok(q)
    }
    pub fn signing_bytes(binding: &PendingSourceInspectBindingV2) -> Result<Vec<u8>, String> {
        binding.validate_shape()?;
        bytes(SOURCE_INSPECT_DISPATCH_DOMAIN_V2, binding)
    }
    pub fn verify(
        &self,
        pin: &AccountMachineIssuerPinV2,
        floor: &AccountMachineIssuerFloorV2,
        object: &str,
        now: u64,
    ) -> Result<(), String> {
        self.binding.validate_shape()?;
        if object != self.binding.dispatcher_object_id {
            return Err(rejected());
        }
        self.binding.issuer.verify_pin(
            pin,
            floor,
            AccountMachinePurposeV2::ExecutionPendingSourceInspectDispatchV2,
            self.binding.issued_at.0,
            self.binding.expires_at.0,
            now,
        )?;
        verify(
            SOURCE_INSPECT_DISPATCH_DOMAIN_V2,
            &self.binding,
            &self.signature_hex,
            &pin.public_key,
        )
    }
    pub fn digest(&self) -> Result<String, String> {
        self.binding.validate_shape()?;
        hash(self)
    }
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PendingSourceNativeLeafV2 {
    pub dispatch: SignedPendingSourceInspectDispatchV2,
    pub native_claim_ordinal: DecimalU64,
    pub claimed_at: DecimalU64,
    pub expires_at: DecimalU64,
}
impl PendingSourceNativeLeafV2 {
    pub fn validate_shape(&self) -> Result<(), String> {
        let b = &self.dispatch.binding;
        b.validate_shape()?;
        if self.native_claim_ordinal.0 <= b.dispatcher_claim_ordinal.0
            || self.claimed_at.0 < b.issued_at.0
            || self.claimed_at.0 >= self.expires_at.0
            || self.expires_at.0 != b.expires_at.0
        {
            return Err(rejected());
        }
        Ok(())
    }
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedPendingSourceNativeLeafV2 {
    pub claim: PendingSourceNativeLeafV2,
    pub signature_hex: String,
}
impl SignedPendingSourceNativeLeafV2 {
    pub fn signing_bytes(claim: &PendingSourceNativeLeafV2) -> Result<Vec<u8>, String> {
        claim.validate_shape()?;
        bytes(SOURCE_NATIVE_LEAF_DOMAIN_V2, claim)
    }
    pub fn verify(
        &self,
        pin: &AccountMachineIssuerPinV2,
        floor: &AccountMachineIssuerFloorV2,
        native_origin: &str,
        now: u64,
    ) -> Result<(), String> {
        self.claim.validate_shape()?;
        let b = &self.claim.dispatch.binding;
        if native_origin != b.issuer.native_audience_origin {
            return Err(rejected());
        }
        b.issuer.verify_pin(
            pin,
            floor,
            AccountMachinePurposeV2::ExecutionPendingSourceNativeLeafV2,
            self.claim.claimed_at.0,
            self.claim.expires_at.0,
            now,
        )?;
        verify(
            SOURCE_NATIVE_LEAF_DOMAIN_V2,
            &self.claim,
            &self.signature_hex,
            &pin.public_key,
        )
    }
    pub fn digest(&self) -> Result<String, String> {
        self.claim.validate_shape()?;
        hash(self)
    }
    pub fn wire_bytes(&self) -> Result<Vec<u8>, String> {
        self.claim.validate_shape()?;
        bounded(self)
    }
    pub fn decode(raw: &[u8]) -> Result<Self, String> {
        let value: Self = decode(raw)?;
        value.wire_bytes()?;
        Ok(value)
    }
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PendingSourceInspectDispatchRequestV2 {
    pub proof: SignedPendingSourceInspectDispatchV2,
    pub request: ApprovalVerifyRequestV1,
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PendingSourceNativeRequestV2 {
    pub leaf: SignedPendingSourceNativeLeafV2,
    pub request: ApprovalVerifyRequestV1,
}
impl PendingSourceInspectDispatchRequestV2 {
    pub fn wire_bytes(&self) -> Result<Vec<u8>, String> {
        self.proof.binding.check_request(&self.request)?;
        bounded(self)
    }
    pub fn decode(raw: &[u8]) -> Result<Self, String> {
        let q: Self = decode(raw)?;
        q.wire_bytes()?;
        Ok(q)
    }
    pub fn verify_native_approval(&self, key_id: &str, public_key: &str) -> Result<(), String> {
        self.wire_bytes()?;
        if self.request.approval.proof.key_id != key_id {
            return Err(rejected());
        }
        let raw = URL_SAFE_NO_PAD
            .decode(&self.request.approval.signature)
            .map_err(|_| rejected())?;
        if URL_SAFE_NO_PAD.encode(&raw) != self.request.approval.signature {
            return Err(rejected());
        }
        let signature = ed25519_dalek::Signature::from_slice(&raw).map_err(|_| rejected())?;
        let key = ed25519_dalek::VerifyingKey::from_bytes(&crate::native_cleanup_v2::decode::<32>(
            public_key,
        )?)
        .map_err(|_| rejected())?;
        key.verify_strict(&self.request.approval.proof.signing_bytes()?, &signature)
            .map_err(|_| rejected())
    }
}
impl PendingSourceNativeRequestV2 {
    pub fn wire_bytes(&self) -> Result<Vec<u8>, String> {
        self.leaf.claim.validate_shape()?;
        self.leaf
            .claim
            .dispatch
            .binding
            .check_request(&self.request)?;
        bounded(self)
    }
    pub fn decode(raw: &[u8]) -> Result<Self, String> {
        let q: Self = decode(raw)?;
        q.wire_bytes()?;
        Ok(q)
    }
    pub fn digest(&self) -> Result<String, String> {
        self.wire_bytes()?;
        hash(self)
    }
}
pub(super) fn bounded<T: Serialize>(q: &T) -> Result<Vec<u8>, String> {
    let raw = serde_json::to_vec(q).map_err(|_| rejected())?;
    if raw.is_empty() || raw.len() > SOURCE_INSPECT_MAX_WIRE_BYTES_V2 {
        return Err(rejected());
    }
    Ok(raw)
}
pub(super) fn decode<T: serde::de::DeserializeOwned + Serialize>(raw: &[u8]) -> Result<T, String> {
    if raw.is_empty() || raw.len() > SOURCE_INSPECT_MAX_WIRE_BYTES_V2 {
        return Err(rejected());
    }
    // Typed first: no Value conversion can erase duplicate typed fields.
    let q: T = serde_json::from_slice(raw).map_err(|_| rejected())?;
    // Native's existing JSON response serializer may order object keys. Reject
    // duplicates recursively, then compare the complete tree instead of order.
    let original: UniqueJson = serde_json::from_slice(raw).map_err(|_| rejected())?;
    if serde_json::to_value(&q).map_err(|_| rejected())? != original.0 {
        return Err(rejected());
    }
    Ok(q)
}
struct UniqueJson(serde_json::Value);
impl<'de> Deserialize<'de> for UniqueJson {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> serde::de::Visitor<'de> for V {
            type Value = UniqueJson;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("unique JSON object keys")
            }
            fn visit_unit<E: serde::de::Error>(self) -> Result<Self::Value, E> {
                Ok(UniqueJson(serde_json::Value::Null))
            }
            fn visit_none<E: serde::de::Error>(self) -> Result<Self::Value, E> {
                self.visit_unit()
            }
            fn visit_bool<E: serde::de::Error>(self, v: bool) -> Result<Self::Value, E> {
                Ok(UniqueJson(v.into()))
            }
            fn visit_i64<E: serde::de::Error>(self, v: i64) -> Result<Self::Value, E> {
                Ok(UniqueJson(v.into()))
            }
            fn visit_u64<E: serde::de::Error>(self, v: u64) -> Result<Self::Value, E> {
                Ok(UniqueJson(v.into()))
            }
            fn visit_f64<E: serde::de::Error>(self, v: f64) -> Result<Self::Value, E> {
                serde_json::Number::from_f64(v)
                    .map(|n| UniqueJson(serde_json::Value::Number(n)))
                    .ok_or_else(|| E::custom("nonfinite JSON"))
            }
            fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Self::Value, E> {
                Ok(UniqueJson(v.into()))
            }
            fn visit_string<E: serde::de::Error>(self, v: String) -> Result<Self::Value, E> {
                Ok(UniqueJson(v.into()))
            }
            fn visit_seq<A: serde::de::SeqAccess<'de>>(
                self,
                mut a: A,
            ) -> Result<Self::Value, A::Error> {
                let mut v = Vec::new();
                while let Some(n) = a.next_element::<UniqueJson>()? {
                    v.push(n.0);
                }
                Ok(UniqueJson(v.into()))
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                mut a: A,
            ) -> Result<Self::Value, A::Error> {
                let mut m = serde_json::Map::new();
                while let Some(k) = a.next_key::<String>()? {
                    if m.contains_key(&k) {
                        return Err(serde::de::Error::custom("duplicate JSON key"));
                    }
                    m.insert(k, a.next_value::<UniqueJson>()?.0);
                }
                Ok(UniqueJson(serde_json::Value::Object(m)))
            }
        }
        d.deserialize_any(V)
    }
}

#[cfg(test)]
mod decoder_tests {
    use super::*;
    #[derive(Serialize, Deserialize, PartialEq, Debug)]
    #[serde(deny_unknown_fields)]
    struct Original {
        api_version: u8,
        // Legacy inner envelopes contain Value. Typed-first alone cannot
        // detect duplicate keys inside this subtree.
        inner: serde_json::Value,
    }
    #[test]
    fn source_inspect_decode_preserves_tree_and_rejects_nested_duplicates() {
        let original = Original {
            api_version: 2,
            inner: serde_json::json!({"proof": {"digest": "a"}, "items": [1, 2]}),
        };
        let reordered = br#"{"inner":{"items":[1,2],"proof":{"digest":"a"}},"api_version":2}"#;
        assert_eq!(
            decode::<Original>(reordered).expect("same complete tree"),
            original
        );
        for raw in [
            br#"{"api_version":2,"api_version":2,"inner":{}}"#.as_slice(),
            br#"{"api_version":2,"inner":{"proof":{"digest":"a","digest":"a"}}}"#.as_slice(),
            br#"{"api_version":2,"inner":{"items":[{"digest":"a","digest":"b"}]}}"#.as_slice(),
            br#"{"api_version":2,"inner":{},"unexpected":null}"#.as_slice(),
        ] {
            assert!(
                decode::<Original>(raw).is_err(),
                "ambiguous or unknown complete original rejected"
            );
        }
    }
}
