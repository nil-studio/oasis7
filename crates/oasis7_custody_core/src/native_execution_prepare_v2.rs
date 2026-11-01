//! New fixed funded Prepare transport. Wire data alone is never Account authority.
//! Ordinary EntryExecutorControlRequestV1 and its signed preimages are unchanged.
use crate::{
    DecimalU64,
    approval::ApprovedWorldV1,
    native_binding::NativeBindingReceiptV1,
    native_cleanup_v2::{
        AccountMachineIssuerFloorV2, AccountMachineIssuerPinV2, AccountMachineIssuerV2,
        AccountMachinePurposeV2,
    },
    provisioning::ExecutionPrepareSignedOriginalV2,
};
use serde::{Deserialize, Serialize};
pub const NATIVE_EXECUTION_PREPARE_DOMAIN_V2: &str = "oasis7.custody.native-execution-prepare.v2";
pub const NATIVE_EXECUTION_PREPARE_PATH_V2: &str =
    "/api/public/hosted-account/v2/entry-executor/prepare-funded-v2";
pub const MAX_NATIVE_EXECUTION_PREPARE_WIRE_BYTES_V2: usize = 64 * 1024;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionPreparePhysicalEdgeV2 {
    AccountToIdentityService,
    IdentityServiceToNativeActor,
    SidCipherReadBefore,
    SidCipherReadAfter,
    CsrfCipherReadBefore,
    CsrfCipherReadAfter,
    IdentityActorToNative,
    NativeToWorldCurrent,
    NativeToWorldPrepare,
    IdentityActorToContextBody,
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionPreparePhysicalPortV2 {
    pub edge: ExecutionPreparePhysicalEdgeV2,
    pub attempt_id: String,
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AccountNativeExecutionPrepareV2 {
    pub api_version: u8,
    pub issuer: AccountMachineIssuerV2,
    pub session_id: String,
    pub approved_input_digest: String,
    pub session_intent_digest: String,
    pub execution_deadline: DecimalU64,
    pub action_request_id: String,
    pub stage_operation_id: String,
    pub source_operation_id: String,
    pub source_nonce: String,
    pub source_record_digest: String,
    pub source_observed_at: DecimalU64,
    pub source_expires_at: DecimalU64,
    pub credential_generation: DecimalU64,
    pub accepted_ordinal: DecimalU64,
    pub factory_record_key: String,
    pub factory_ports_digest: String,
    pub private_current_proof_digest: String,
    pub opaque_registration_ref: String,
    pub active_certificate_digest: String,
    pub native_binding: NativeBindingReceiptV1,
    pub native_session_ref: String,
    pub private_sid_generation: DecimalU64,
    pub native_binding_generation: DecimalU64,
    pub session: crate::provisioning::ParticipantSessionRefV1,
    pub world: ApprovedWorldV1,
    pub original_digest: String,
    pub issued_at: DecimalU64,
    pub expires_at: DecimalU64,
    pub ports: [ExecutionPreparePhysicalPortV2; 10],
}
fn invalid() -> String {
    "funded execution Prepare rejected".into()
}
fn id(v: &str) -> bool {
    !v.is_empty() && v.len() <= 256 && !v.chars().any(char::is_control)
}
fn hex(v: &str) -> bool {
    v.len() == 64
        && v.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
impl AccountNativeExecutionPrepareV2 {
    pub fn validate_shape(&self) -> Result<(), String> {
        self.native_binding.validate().map_err(|_| invalid())?;
        let expected = [
            ExecutionPreparePhysicalEdgeV2::AccountToIdentityService,
            ExecutionPreparePhysicalEdgeV2::IdentityServiceToNativeActor,
            ExecutionPreparePhysicalEdgeV2::SidCipherReadBefore,
            ExecutionPreparePhysicalEdgeV2::SidCipherReadAfter,
            ExecutionPreparePhysicalEdgeV2::CsrfCipherReadBefore,
            ExecutionPreparePhysicalEdgeV2::CsrfCipherReadAfter,
            ExecutionPreparePhysicalEdgeV2::IdentityActorToNative,
            ExecutionPreparePhysicalEdgeV2::NativeToWorldCurrent,
            ExecutionPreparePhysicalEdgeV2::NativeToWorldPrepare,
            ExecutionPreparePhysicalEdgeV2::IdentityActorToContextBody,
        ];
        if self.api_version != 2
            || !self.world.valid()
            || ![
                &self.session_id,
                &self.action_request_id,
                &self.stage_operation_id,
                &self.source_operation_id,
                &self.native_session_ref,
                &self.opaque_registration_ref,
            ]
            .iter()
            .all(|v| id(v))
            || ![
                &self.approved_input_digest,
                &self.session_intent_digest,
                &self.source_nonce,
                &self.source_record_digest,
                &self.original_digest,
                &self.factory_ports_digest,
                &self.private_current_proof_digest,
                &self.active_certificate_digest,
            ]
            .iter()
            .all(|v| hex(v))
            || self.factory_record_key.len() <= 256
            || self.factory_record_key.len() > 1024
            || self.factory_record_key.chars().any(char::is_control)
            || self.credential_generation.0 == 0
            || self.private_sid_generation.0 == 0
            || self.native_binding_generation.0 == 0
            || self.accepted_ordinal.0 == 0
            || self.source_observed_at.0 > self.issued_at.0
            || self.issued_at.0 >= self.expires_at.0
            || self.expires_at.0 > self.execution_deadline.0
            || self.expires_at.0 > self.source_expires_at.0
            || self.expires_at.0
                > self
                    .source_observed_at
                    .0
                    .checked_add(5000)
                    .ok_or_else(invalid)?
            || self.native_binding.environment != self.issuer.environment
            || self.session.session_ref != self.native_session_ref
            || self.session.private_sid_generation != self.private_sid_generation
            || self.session.native_binding_generation != self.native_binding_generation
            || self.session.scope.role_scope.context.identity.account_id
                != self.native_binding.custody_account_id
            || self.session.scope.role_scope.provision_id != self.native_binding.provision_id
            || self.session.scope.identity_namespace != self.native_binding.identity_namespace
            || self.session.scope.identity_instance_epoch
                != self.native_binding.identity_instance_epoch
        {
            return Err(invalid());
        }
        self.session.scope.validate().map_err(|_| invalid())?;
        let mut unique = std::collections::BTreeSet::new();
        for (port, edge) in self.ports.iter().zip(expected) {
            if port.edge != edge || !hex(&port.attempt_id) || !unique.insert(&port.attempt_id) {
                return Err(invalid());
            }
        }
        Ok(())
    }
    pub fn signing_bytes(&self) -> Result<Vec<u8>, String> {
        self.validate_shape()?;
        oasis7_wasm_abi::encode_canonical_cbor(&(NATIVE_EXECUTION_PREPARE_DOMAIN_V2, self))
            .map_err(|_| invalid())
    }
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedAccountNativeExecutionPrepareV2 {
    pub certificate: AccountNativeExecutionPrepareV2,
    pub signature_hex: String,
}
impl core::fmt::Debug for SignedAccountNativeExecutionPrepareV2 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("SignedAccountNativeExecutionPrepareV2(<unverified>)")
    }
}
impl SignedAccountNativeExecutionPrepareV2 {
    pub fn wire_bytes(&self) -> Result<Vec<u8>, String> {
        self.certificate.validate_shape()?;
        if self.signature_hex.len() != 128
            || !self
                .signature_hex
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(invalid());
        }
        let bytes = serde_json::to_vec(self).map_err(|_| invalid())?;
        if bytes.len() > MAX_NATIVE_EXECUTION_PREPARE_WIRE_BYTES_V2 {
            return Err(invalid());
        }
        Ok(bytes)
    }
    /// Verify only the fixed Account issuer's exact public commitment. This is
    /// deliberately not a local Current/managed-SID or physical-once grant.
    pub fn verify_issuer(
        &self,
        pin: &AccountMachineIssuerPinV2,
        floor: &AccountMachineIssuerFloorV2,
        now: u64,
    ) -> Result<(), String> {
        self.wire_bytes()?;
        self.certificate.issuer.verify_pin(
            pin,
            floor,
            AccountMachinePurposeV2::NativeExecutionPrepareV2,
            self.certificate.issued_at.0,
            self.certificate.expires_at.0,
            now,
        )?;
        crate::native_cleanup_v2::verify(
            NATIVE_EXECUTION_PREPARE_DOMAIN_V2,
            &self.certificate,
            &self.signature_hex,
            &pin.public_key,
        )
    }
    pub fn digest(&self) -> Result<String, String> {
        self.wire_bytes()?;
        oasis7_wasm_abi::canonical_hash(self).map_err(|_| invalid())
    }
}

/// Actual Native transport always carries and verifies the complete exact SDK
/// original. The signed commitment never stands in for this original.
#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeExecutionPrepareRequestV2 {
    pub proof: SignedAccountNativeExecutionPrepareV2,
    pub original: Box<ExecutionPrepareSignedOriginalV2>,
}
impl NativeExecutionPrepareRequestV2 {
    pub fn wire_bytes(&self) -> Result<Vec<u8>, String> {
        self.proof.wire_bytes()?;
        if self.proof.certificate.original_digest
            != self.original.commitment_digest().map_err(|_| invalid())?
        {
            return Err(invalid());
        }
        let bytes = serde_json::to_vec(self).map_err(|_| invalid())?;
        if bytes.len() > MAX_NATIVE_EXECUTION_PREPARE_WIRE_BYTES_V2 {
            return Err(invalid());
        }
        Ok(bytes)
    }
}

pub const NATIVE_EXECUTION_PREPARE_RECEIPT_DOMAIN_V2: &str =
    "oasis7.custody.native-execution-prepare-receipt.v2";
/// Only the actual Native adapter may sign completion. This public fact is not
/// fresh Current, a context admission, a World business effect or model usage.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeExecutionPrepareReceiptV2 {
    pub api_version: u8,
    pub native_key_id: String,
    pub proof_digest: String,
    pub native_attempt_id: String,
    pub current_attempt_id: String,
    pub prepare_attempt_id: String,
    pub result_digest: String,
    pub public_projection_digest: String,
    pub completed_at: DecimalU64,
}
impl NativeExecutionPrepareReceiptV2 {
    pub fn validate_relative(
        &self,
        request: &NativeExecutionPrepareRequestV2,
    ) -> Result<(), String> {
        request.wire_bytes()?;
        let c = &request.proof.certificate;
        if self.api_version != 2
            || !id(&self.native_key_id)
            || self.proof_digest != request.proof.digest()?
            || !hex(&self.result_digest)
            || !hex(&self.public_projection_digest)
            || self.native_attempt_id != c.ports[6].attempt_id
            || self.current_attempt_id != c.ports[7].attempt_id
            || self.prepare_attempt_id != c.ports[8].attempt_id
            || self.completed_at.0 < c.issued_at.0
            || self.completed_at.0 >= c.expires_at.0
        {
            return Err(invalid());
        }
        Ok(())
    }
    pub fn signing_bytes(
        &self,
        request: &NativeExecutionPrepareRequestV2,
    ) -> Result<Vec<u8>, String> {
        self.validate_relative(request)?;
        crate::native_cleanup_v2::bytes(NATIVE_EXECUTION_PREPARE_RECEIPT_DOMAIN_V2, self)
    }
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedNativeExecutionPrepareReceiptV2 {
    pub receipt: NativeExecutionPrepareReceiptV2,
    pub signature_hex: String,
}
impl SignedNativeExecutionPrepareReceiptV2 {
    pub fn verify_native_public(
        &self,
        request: &NativeExecutionPrepareRequestV2,
        key_id: &str,
        public_key: &str,
        expected_public_projection_digest: &str,
    ) -> Result<(), String> {
        self.receipt.validate_relative(request)?;
        if self.receipt.native_key_id != key_id
            || self.receipt.public_projection_digest != expected_public_projection_digest
        {
            return Err(invalid());
        }
        crate::native_cleanup_v2::verify(
            NATIVE_EXECUTION_PREPARE_RECEIPT_DOMAIN_V2,
            &self.receipt,
            &self.signature_hex,
            public_key,
        )
    }
    pub fn verify_native(
        &self,
        request: &NativeExecutionPrepareRequestV2,
        key_id: &str,
        public_key: &str,
        expected_result_digest: &str,
    ) -> Result<(), String> {
        self.receipt.validate_relative(request)?;
        if self.receipt.native_key_id != key_id
            || self.receipt.result_digest != expected_result_digest
        {
            return Err(invalid());
        }
        crate::native_cleanup_v2::verify(
            NATIVE_EXECUTION_PREPARE_RECEIPT_DOMAIN_V2,
            &self.receipt,
            &self.signature_hex,
            public_key,
        )
    }
}
impl core::fmt::Debug for SignedNativeExecutionPrepareReceiptV2 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("SignedNativeExecutionPrepareReceiptV2(<unverified-public-fact>)")
    }
}

pub const MAX_NATIVE_EXECUTION_PREPARE_REPLY_BYTES_V2: usize = 256 * 1024;
/// Identity-only wire delivery. Account consumers accept the independently
/// signed public reference/receipt, never this private-body type.
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeExecutionPrepareReplyV2 {
    pub api_version:u8,
    pub world_response:oasis7_client_api::world_service::task_context_projection::WorldContextAttestationV2<
        oasis7_client_api::world_service::task_context_projection::TaskContextPreparedDeliveryV2>,
    pub physical_receipt:SignedNativeExecutionPrepareReceiptV2,
}
impl NativeExecutionPrepareReplyV2 {
    pub fn wire_bytes(&self) -> Result<Vec<u8>, String> {
        if self.api_version != 2 {
            return Err(invalid());
        }
        let bytes = serde_json::to_vec(self).map_err(|_| invalid())?;
        if bytes.len() > MAX_NATIVE_EXECUTION_PREPARE_REPLY_BYTES_V2 {
            return Err(invalid());
        }
        Ok(bytes)
    }
}

/// Public immutable facts only. The Identity-only full response has a distinct
/// type and is never accepted by this Account-facing DTO.
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionPreparedPublicResultV2 {
    pub api_version: u8,
    pub proof_digest: String,
    pub reference: crate::context_body_v2::AttestedSlotRefV2,
    pub native_receipt: SignedNativeExecutionPrepareReceiptV2,
    pub storage: crate::context_body_v2::ContextBodyStorageFactV2,
}

impl ExecutionPreparedPublicResultV2 {
    pub fn validate_public(&self, q: &NativeExecutionPrepareRequestV2) -> Result<(), String> {
        use crate::context_body_v2::AttestedContextEvidenceV2;
        let c = &q.proof.certificate;
        let r = &self.reference;
        let s = &r.scope;
        r.validate_provenance(&c.world).map_err(|_| invalid())?;
        self.storage.validate_for(s).map_err(|_| invalid())?;
        let stored = self.storage.stored.as_ref().ok_or_else(invalid)?;
        let AttestedContextEvidenceV2::Prepared {
            request,
            projection,
        } = &r.evidence
        else {
            return Err(invalid());
        };
        if self.api_version != 2 || self.proof_digest != q.proof.digest()?
            || request.as_ref() != &q.original.request
            || s.identity_scope != c.session.scope.role_scope || s.world != c.world
            || s.session_id != c.session_id || s.context_operation_id != c.stage_operation_id
            || s.prepare_operation_id != q.original.request.original.request.client_operation_id
            || s.session_intent_digest != c.session_intent_digest
            || s.native_binding_digest != c.native_binding.digest().map_err(|_| invalid())?
            || s.version.0 != 1 || s.write_deadline != c.execution_deadline
            || stored.envelope_digest != r.envelope_digest
            || stored.record_wire_bytes != r.envelope_wire_bytes
            || self.native_receipt.receipt.public_projection_digest !=
                oasis7_client_api::world_service::task_context_projection::public_projection_digest_v2(projection)
                    .map_err(|_|invalid())?
        { return Err(invalid()); }
        self.native_receipt.receipt.validate_relative(q)?;
        if oasis7_wasm_abi::encode_canonical_cbor(self)
            .map_err(|_| invalid())?
            .len()
            > 65536
        {
            return Err(invalid());
        }
        Ok(())
    }
    pub fn verify_native(
        &self,
        q: &NativeExecutionPrepareRequestV2,
        key_id: &str,
        public_key: &str,
    ) -> Result<(), String> {
        self.validate_public(q)?;
        self.native_receipt.verify_native_public(
            q,
            key_id,
            public_key,
            &self.reference.scope.public_projection_digest,
        )
    }
}

/// Exact complete signed World delivery digest for the Native receipt. This
/// typed codec neither verifies a signer nor grants Current/dispatch authority.
pub fn prepared_response_digest_v2(
    value: &oasis7_client_api::world_service::task_context_projection::WorldContextAttestationV2<
        oasis7_client_api::world_service::task_context_projection::TaskContextPreparedDeliveryV2,
    >,
) -> Result<String, String> {
    oasis7_wasm_abi::canonical_hash(value).map_err(|_| invalid())
}

/// Explicit per-leaf delivery. The legacy Native10 reply is not upgraded by
/// attaching caller metadata; both World claims come from Account SQL.
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeExecutionPrepareGatedReplyV2 {
    pub api_version: u8,
    pub reply: NativeExecutionPrepareReplyV2,
    pub world_claims: [crate::execution_dispatch_gate_v2::SignedExecutionDispatchClaimV2; 2],
}
impl NativeExecutionPrepareGatedReplyV2 {
    pub fn wire_bytes(&self) -> Result<Vec<u8>, String> {
        if self.api_version != 2 {
            return Err(invalid());
        }
        self.reply.wire_bytes()?;
        let bytes = serde_json::to_vec(self).map_err(|_| invalid())?;
        if bytes.len() > MAX_NATIVE_EXECUTION_PREPARE_REPLY_BYTES_V2 {
            return Err(invalid());
        }
        Ok(bytes)
    }
}
