//! Disjoint legacy/attested persistence. Neither deserialization branch grants use.
use super::*;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum SessionContextRecordV1 {
    Legacy(SessionContextV1),
    Attested(AttestedMoveContextLedgerV1),
}
impl SessionContextRecordV1 {
    pub(in crate::provisioning) fn legacy(&self) -> Result<&SessionContextV1> {
        match self {
            Self::Legacy(v) => Ok(v),
            Self::Attested(_) => Err(invalid()),
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttestedMoveStageV1 {
    Challenge,
    Prepare,
    Prefix,
    Get,
    MoveProjection,
    Cognition,
    Lookup,
    Result,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttestedMovePortKindV1 {
    AttestedMovePortV1,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttestedMoveEdgeV1 {
    PublicHttp,
    IdentityToOwnerService,
    OwnerServiceToAccount,
    AccountToDirectory,
    AccountToFenceService,
    FenceServiceToActor,
    AccountToIdentityService,
    IdentityServiceToManifest,
    SidReadBefore,
    SidReadAfter,
    CsrfReadBefore,
    CsrfReadAfter,
    IdentityToNative,
    NativeToWorldView,
    NativeToWorldOperation,
    IdentityToContextActor,
    AccountToRoleFactory,
    RoleFactoryToVault,
    RoleToOwnerService,
    RoleToFenceService,
}
/// One row is exactly one paid physical request, never a batch of hidden calls.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttestedMovePaidPortV1 {
    pub kind: AttestedMovePortKindV1,
    pub purpose: SessionPurposeV1,
    pub context_operation_id: String,
    pub action_request_id: String,
    pub stage: AttestedMoveStageV1,
    pub edge: AttestedMoveEdgeV1,
    pub source_operation_id: String,
    pub claimed_at: UnixMillis,
}
fn required_nullable<'de, D, T>(d: D) -> core::result::Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::deserialize(d)
}
/// Original public bytes are immutable. A lost result leaves this exact slot
/// claimed; only separately paid authenticated lookup may resolve the unknown.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttestedMoveOriginalSlotV1 {
    pub canonical_cbor_hex: String,
    pub source_operation_id: String,
    pub allocated_at: UnixMillis,
    #[serde(deserialize_with = "required_nullable")]
    pub send_claim: Option<String>,
    #[serde(deserialize_with = "required_nullable")]
    pub archived_response_cbor_hex: Option<String>,
    #[serde(deserialize_with = "required_nullable")]
    pub admitted_at: Option<UnixMillis>,
}
/// Required tagged ledger; never shares legacy ContextRef/body-use authority.
/// Root/session binding, typed originals, World proofs and each paid row must be
/// revalidated against the actual Account state before any live stage is used.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttestedMoveContextLedgerV1 {
    pub profile: AttestedMoveProfileV1,
    pub session_id: String,
    pub action_request_id: String,
    pub action_request_digest: String,
    pub context_operation_id: String,
    pub approved_position_cm: [i64; 3],
    pub original_fence: oasis7_agent_api::game::GameTaskFenceV1,
    pub configured_world_public_key: String,
    pub registered_at: UnixMillis,
    pub write_deadline: UnixMillis,
    pub read_until: UnixMillis,
    pub paid_rows: BTreeMap<String, AttestedMovePaidPortV1>,
    pub originals: BTreeMap<AttestedMoveStageV1, AttestedMoveOriginalSlotV1>,
    pub public_facts: BTreeMap<AttestedMoveFactStageV1, AttestedMovePublicFactV1>,
}
