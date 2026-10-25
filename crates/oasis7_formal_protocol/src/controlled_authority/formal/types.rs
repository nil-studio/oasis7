use super::fencing::EndpointInstanceBindingV1;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FormalError {
    Invalid(String),
    /// A qualified historical cut or its retained closure is unavailable.
    HistoryUnavailable(String),
    Io(String),
    Conflict(String),
    Rollback(String),
    FencingUnproven,
    IssuerUnproven,
}
impl std::fmt::Display for FormalError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for FormalError {}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FormalWorldIdentityV1 {
    pub world_id: String,
    pub chain_id: String,
    pub genesis_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FormalHeadRefV1 {
    pub position: u64,
    pub record_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EndpointSignerBindingV1 {
    pub instance: EndpointInstanceBindingV1,
    /// An issuer-bound deployment declaration; cryptography does not itself
    /// prove that physical failures are independent.
    pub fault_domain_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthorityDefinitionV1 {
    pub schema_version: u32,
    pub world: FormalWorldIdentityV1,
    pub profile: String,
    pub profile_version: u32,
    pub epoch: u64,
    pub writer_public_key: String,
    pub primary: EndpointSignerBindingV1,
    pub replica: EndpointSignerBindingV1,
    pub recovery_policy_digest: String,
    pub predecessor_authority_id: Option<String>,
    pub valid_from_position: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SuccessorRuleV1 {
    IncrementEpochDistinctBindings,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecoveryPolicyV1 {
    pub schema_version: u32,
    pub recovery_issuer_public_key: String,
    pub fencing_provider_public_key: String,
    /// Independently pinned external monotonic-provider ledger namespaces.
    /// These are not endpoint storage fingerprints.
    pub authorized_fencing_resource_bindings: Vec<String>,
    pub issuer_anchor_public_key: String,
    pub allow_complete_prepared: bool,
    pub allow_replay_frozen: bool,
    pub successor_rule: SuccessorRuleV1,
    pub terminates_at_profile_transition: bool,
}

/// A pending handoff's exact target is part of the original reservation/record.
/// This foundation can close the old profile but cannot activate the next one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProfileHandoffV1 {
    pub next_profile: String,
    pub next_profile_version: u32,
    pub next_authority_digest: String,
    pub valid_from_position: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RecoveryTailV1 {
    Empty,
    Frozen {
        position: u64,
        reservation_id: String,
        handoff: Option<ProfileHandoffV1>,
    },
    Prepared {
        position: u64,
        reservation_id: String,
        record_id: String,
        handoff: Option<ProfileHandoffV1>,
    },
    Decision {
        position: u64,
        reservation_id: String,
        record_id: String,
        handoff: Option<ProfileHandoffV1>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecoveryCertificateBodyV1 {
    pub schema_version: u32,
    pub world: FormalWorldIdentityV1,
    pub operation_id: String,
    pub predecessor_authority_id: String,
    pub predecessor_epoch: u64,
    pub sealed_cut: FormalHeadRefV1,
    pub tail: RecoveryTailV1,
    pub fencing_evidence_digest: String,
    pub successor: AuthorityDefinitionV1,
    /// None for a tail that hands off to another profile: recovery cannot grant
    /// fresh controlled-profile writes beyond that already-bound handoff.
    pub first_normal_position: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retained_context: Option<super::RetainedRecoveryContextV2>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedRecoveryCertificateV1 {
    pub body: RecoveryCertificateBodyV1,
    pub signature_hex: String,
}
