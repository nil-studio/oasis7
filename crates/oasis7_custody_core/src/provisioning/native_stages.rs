//! Native phase originals and budget claims. All authority inputs are trusted Rust ports.
use super::*;
use crate::{approval::bytes_digest, native_binding::*};
use serde::{Deserialize, Serialize};
pub const NATIVE_STAGE_TRANSPORT_COST: u64 = 64;
pub const NATIVE_STAGE_MAX_ORACLE_READS: u8 = 8;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NativeStageKindV1 {
    Registration,
    Binding,
    Login,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeStageRequestV1 {
    pub api_version: u8,
    pub custody_account_id: String,
    pub provision_id: String,
    pub operation_id: String,
    pub binding_operation_id: String,
    pub stage: NativeStageKindV1,
}
impl NativeStageRequestV1 {
    pub fn validate(&self) -> Result<()> {
        if self.api_version != 1
            || [
                &self.custody_account_id,
                &self.provision_id,
                &self.operation_id,
                &self.binding_operation_id,
            ]
            .iter()
            .any(|v| !super::super::types::valid_id(v))
        {
            return Err(ProvisionApprovalError::BindingMismatch);
        }
        Ok(())
    }
    pub fn digest(&self) -> Result<String> {
        self.validate()?;
        Ok(bytes_digest(
            &oasis7_wasm_abi::encode_canonical_cbor(&(
                "oasis7-custody-native-stage-request-v1",
                self,
            ))
            .map_err(|_| ProvisionApprovalError::BindingMismatch)?,
        ))
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeStageDescriptorV1 {
    pub registration: NativeRegistrationRequestV1,
    pub login_request_id: String,
    pub identity_key: PublicRoleKey,
    pub source_audience: String,
    pub http_origin: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeStagePermitViewV1 {
    pub request: NativeStageRequestV1,
    pub descriptor: NativeStageDescriptorV1,
    pub identity_scope: RoleKeySlotScopeV1,
    pub authority_revision: String,
    pub source_revision: String,
    pub attempt_id: String,
    pub request_bytes: String,
    pub request_digest: String,
    pub target_security_epoch: DecimalU64,
    pub service_epoch: DecimalU64,
    pub nonce: String,
    pub expires_at: DecimalU64,
    pub observed_at: DecimalU64,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "result",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum NativeStageResultV1 {
    RegistrationChallenge(SignedNativeBindingChallengeV1),
    BindingReceipt(SignedNativeBindingReceiptV1),
    SessionStatus(SignedManagedNativeSessionStatusV1),
    ExplicitLoginRequired,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BootstrapActivationV1 {
    pub binding_operation_id: String,
    pub receipt_digest: String,
    pub from_epoch: DecimalU64,
    pub to_epoch: DecimalU64,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeStageOriginalV1 {
    pub request: NativeStageRequestV1,
    pub descriptor: NativeStageDescriptorV1,
    pub inspection_id: String,
    pub nonce: String,
    pub reserved_at: DecimalU64,
    pub target_security_epoch: DecimalU64,
    pub claimed_at: Option<DecimalU64>,
    pub request_bytes: Option<String>,
    pub reads: u8,
    pub result: Option<NativeStageResultV1>,
    pub binding_statement: Option<SignedOwnerNativeBindingStatementV1>,
    pub binding_challenge: Option<SignedNativeBindingChallengeV1>,
    pub private_sid_generation: Option<DecimalU64>,
}

/// The endpoint origin is supplied by fixed native HTTPS configuration, not the approval audience.
pub struct NativeStageScope<'a> {
    pub http_origin: &'a str,

    pub native_root_key_id: &'a str,
    pub native_root_public_key: &'a str,
    pub owner_binding_pin: &'a NativeBindingPinV1,
}
/// Stable base64url SHA-256 identity for an approved logical native operation.
pub fn derive_native_operation_id(account: &str, request_id: &str) -> Result<String> {
    use sha2::{Digest, Sha256};
    let bytes = oasis7_wasm_abi::encode_canonical_cbor(&(
        "oasis7-custody-native-operation-v1",
        account,
        request_id,
    ))
    .map_err(|_| ProvisionApprovalError::BindingMismatch)?;
    let digest = Sha256::digest(bytes);
    const TABLE: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::new();
    let mut buffer = 0u32;
    let mut bits = 0u8;
    for byte in digest {
        buffer = (buffer << 8) | u32::from(byte);
        bits += 8;
        while bits >= 6 {
            bits -= 6;
            out.push(char::from(TABLE[((buffer >> bits) & 63) as usize]));
        }
    }
    if bits > 0 {
        out.push(char::from(TABLE[((buffer << (6 - bits)) & 63) as usize]));
    }
    Ok(out)
}
impl AccountProvisionApprovalState {
    pub fn native_stages(&self) -> &BTreeMap<String, NativeStageOriginalV1> {
        &self.native_stages
    }
    pub fn bootstrap_activation(&self) -> Option<&BootstrapActivationV1> {
        self.bootstrap_activation.as_ref()
    }
    pub fn native_stage_descriptor(
        &self,
        scope: &NativeStageScope<'_>,
    ) -> Result<NativeStageDescriptorV1> {
        self.native_descriptor_with_origin(scope.http_origin)
    }
    pub(super) fn native_descriptor_with_origin(
        &self,
        http_origin: &str,
    ) -> Result<NativeStageDescriptorV1> {
        let approved = self.accepted_approval()?;
        let CustodyIntentV1::Provision {
            native_identity_namespace,
            register_request_id,
            login_request_id,
            ..
        } = &approved.input().request.intent
        else {
            return Err(ProvisionApprovalError::BindingMismatch);
        };
        let key = self
            .provision
            .role_key(Role::Identity)
            .ok_or(ApprovalAdmissionError::Missing)?
            .clone();
        let operation_scope = super::super::types::digest(&(
            "oasis7-custody-native-op-scope-v1",
            &self.draft.entry.environment,
            &self.draft.entry.custody_account_id,
            &self.draft.entry.provision_id,
            native_identity_namespace,
            approved.input().approval.proof.identity_instance_epoch,
            &key.public_key,
        ))?;
        let registration = NativeRegistrationRequestV1 {
            identity_namespace: native_identity_namespace.clone(),
            identity_instance_epoch: approved.input().approval.proof.identity_instance_epoch,
            registration_operation_id: derive_native_operation_id(
                &operation_scope,
                register_request_id,
            )?,
            login_public_key: key.public_key.clone(),
            display_name: "Oasis7 managed account".into(),
            audience: http_origin.into(),
        };
        registration
            .validate()
            .map_err(|_| ProvisionApprovalError::BindingMismatch)?;
        if !http_origin.starts_with("https://")
            || http_origin.ends_with('/')
            || http_origin.contains('?')
            || http_origin.contains('#')
        {
            return Err(ProvisionApprovalError::BindingMismatch);
        }
        Ok(NativeStageDescriptorV1 {
            registration,
            login_request_id: derive_native_operation_id(&operation_scope, login_request_id)?,
            identity_key: key,
            source_audience: approved.input().request.context.audience.clone(),
            http_origin: http_origin.into(),
        })
    }
}

#[path = "native_stages/attempts.rs"]
mod attempts;
#[path = "native_stages/results.rs"]
mod results;
#[path = "native_stages/source.rs"]
mod source;
pub use results::PersistedNativeSessionAttestation;

#[path = "native_stages/managed_login.rs"]
mod managed_login;
pub use managed_login::*;

#[path = "native_stages/binding_recovery.rs"]
mod binding_recovery;
#[path = "native_stages/managed_login_runtime.rs"]
mod managed_login_runtime;

#[path = "native_stages/status_recovery_types.rs"]
mod status_recovery_types;
pub use status_recovery_types::*;

#[path = "native_stages/history_archive.rs"]
mod history_archive;
#[path = "native_stages/missing_session.rs"]
mod missing_session;

#[path = "native_stages/status_recovery.rs"]
mod status_recovery;

#[path = "native_stages/status_recovery_source.rs"]
mod status_recovery_source;

#[path = "native_stages/status_recovery_result.rs"]
mod status_recovery_result;

#[path = "native_stages/status_recovery_restore.rs"]
mod status_recovery_restore;

pub use history_archive::MAX_NATIVE_HISTORY_ENTRIES;
pub use status_recovery::MAX_SESSION_STATUS_RECOVERIES;
