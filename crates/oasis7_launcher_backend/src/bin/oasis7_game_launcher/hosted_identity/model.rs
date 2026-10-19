//! Versioned server-side identity records. None of these are public API projections.
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct IdentityNamespace {
    pub identity_service_id: String,
    pub environment_id: String,
}
impl IdentityNamespace {
    pub fn key(&self) -> String {
        format!(
            "{}:{}:{}",
            self.identity_service_id.len(),
            self.identity_service_id,
            self.environment_id
        )
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct RpConfig {
    pub rp_id: String,
    pub allowed_origins: Vec<String>,
    pub version: u64,
    pub identity_instance_epoch: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AccountRecord {
    pub account_id: String,
    pub user_handle: Vec<u8>,
    pub display_name: String,
    pub status: String,
    pub policy_version: u64,
    pub security_epoch: u64,
    pub has_ever_public_key: bool,
    pub created_at: u64,
    pub last_authenticated_at: Option<u64>,
    pub last_recovery_id: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PlayerBinding {
    pub account_id: String,
    pub world_identity_scope: String,
    pub player_id: String,
    pub source: String,
    pub binding_version: u64,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CredentialMaterial {
    NativeKey {
        public_key: [u8; 32],
    },
    WebAuthn {
        rp_id: String,
        credential_id: Vec<u8>,
        user_handle: Vec<u8>,
        library_record: serde_json::Value,
        library_record_version: u64,
    },
    Email {
        encrypted_address: String,
        lookup_hmac: String,
        normalization_version: u64,
        verified_at: u64,
        bound_at: u64,
        recovery_enabled: bool,
        recovery_eligible_since: u64,
    },
}
#[derive(Clone, Serialize, Deserialize)]
pub struct CredentialRecord {
    pub credential_record_id: String,
    pub account_id: String,
    pub state: String,
    pub version: u64,
    pub label: String,
    pub created_at: u64,
    pub last_used_at: Option<u64>,
    pub public_material: CredentialMaterial,
    pub registered_via: String,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct CredentialLookup {
    pub scope: String,
    pub key_digest: String,
    pub canonical_key: Vec<u8>,
    pub credential_record_id: String,
    pub account_id: String,
    pub state: String,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct AccountSession {
    pub session_id: String,
    pub token_digest: String,
    pub account_id: String,
    pub origin_credential_id: String,
    pub origin_credential_version: u64,
    pub issued_security_epoch: u64,
    pub identity_instance_epoch: u64,
    pub auth_method: String,
    pub auth_time: u64,
    pub expires_at: u64,
    pub idle_expires_at: u64,
    pub revoked_at: Option<u64>,
    pub recovery_context: Option<String>,
    pub csrf_digest: String,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct CeremonyRecord {
    pub ceremony_id: String,
    pub kind: String,
    pub namespace: IdentityNamespace,
    pub rp_config_version: u64,
    pub owner_context_digest: String,
    pub target_account_id: Option<String>,
    pub target_digest: Option<String>,
    pub expires_at: u64,
    pub failed_attempts: u32,
    pub status: String,
    pub server_state: Option<serde_json::Value>,
    pub result_ref: Option<String>,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct SensitiveAction {
    pub action_id: String,
    pub account_id: String,
    pub session_id: String,
    pub action_type: String,
    pub target_digest: String,
    pub account_policy_version: u64,
    pub proof_credential_id: Option<String>,
    pub proof_time: Option<u64>,
    pub expires_at: u64,
    pub state: String,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct RecoveryRequest {
    pub recovery_id: String,
    pub account_id: String,
    pub email_credential_id: String,
    pub email_credential_version: u64,
    pub candidate_credential_id: String,
    pub candidate_digest: String,
    pub account_policy_version_at_start: u64,
    pub security_epoch_at_start: u64,
    pub created_at: u64,
    pub not_before: u64,
    pub expires_at: u64,
    pub state: String,
    pub approved_by: Option<String>,
    pub notification_accepted_at: Option<u64>,
    pub result_ref: Option<String>,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct OperationReceipt {
    pub operation_id: String,
    pub kind: String,
    pub owner_context_digest: String,
    pub request_digest: String,
    pub state: String,
    pub result_ids: Vec<String>,
    pub committed_at: u64,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct OutboxEvent {
    pub event_id: String,
    pub account_id: String,
    pub event_type: String,
    pub state_version: u64,
    pub safe_payload: serde_json::Value,
    pub attempts: u32,
    pub delivery_state: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct QuotaRecord {
    pub bucket: String,
    pub window_start: u64,
    pub count: u64,
}

impl CredentialMaterial {
    pub fn is_public_key(&self) -> bool {
        matches!(self, Self::WebAuthn { .. } | Self::NativeKey { .. })
    }
    pub fn credential_type(&self) -> &'static str {
        match self {
            Self::WebAuthn { .. } => "passkey",
            Self::NativeKey { .. } => "native_key",
            Self::Email { .. } => "email",
        }
    }
}
