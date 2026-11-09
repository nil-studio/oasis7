use super::*;
use oasis7_agent_api::world_quote as native;
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResourceGrantV1 {
    pub grant_id: String,
    pub source_id: String,
    pub issuer_id: String,
    pub owner_id: String,
    #[serde(deserialize_with = "required_option")]
    pub organization_id: Option<String>,
    pub agent_id: String,
    pub object_id: String,
    pub action_kinds: Vec<String>,
    pub revision: DecimalU64,
    pub period_id: String,
    pub valid_from_tick: DecimalU64,
    pub valid_until_tick: DecimalU64,
    pub limit_units: DecimalU64,
    pub resource_kind: String,
    pub revoked: bool,
}
impl From<&native::AgentDelegationGrantV1> for ResourceGrantV1 {
    fn from(v: &native::AgentDelegationGrantV1) -> Self {
        Self {
            grant_id: v.grant_id.clone(),
            source_id: v.source_id.clone(),
            issuer_id: v.issuer_id.clone(),
            owner_id: v.owner_id.clone(),
            organization_id: v.organization_id.clone(),
            agent_id: v.agent_id.clone(),
            object_id: v.object_id.clone(),
            action_kinds: v.action_kinds.clone(),
            revision: DecimalU64(v.revision),
            period_id: v.period_id.clone(),
            valid_from_tick: DecimalU64(v.valid_from_tick),
            valid_until_tick: DecimalU64(v.valid_until_tick),
            limit_units: DecimalU64(v.limit_units),
            resource_kind: v.resource_kind.clone(),
            revoked: v.revoked,
        }
    }
}
impl From<ResourceGrantV1> for native::AgentDelegationGrantV1 {
    fn from(v: ResourceGrantV1) -> Self {
        Self {
            grant_id: v.grant_id,
            source_id: v.source_id,
            issuer_id: v.issuer_id,
            owner_id: v.owner_id,
            organization_id: v.organization_id,
            agent_id: v.agent_id,
            object_id: v.object_id,
            action_kinds: v.action_kinds,
            revision: v.revision.0,
            period_id: v.period_id,
            valid_from_tick: v.valid_from_tick.0,
            valid_until_tick: v.valid_until_tick.0,
            limit_units: v.limit_units.0,
            resource_kind: v.resource_kind,
            revoked: v.revoked,
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResourceAuthorizationV1 {
    pub grant: ResourceGrantV1,
    pub cost_units: DecimalU64,
    pub spent_units: DecimalU64,
    pub remaining_units: DecimalU64,
}
impl From<&native::AgentDelegationAuthorizationV1> for ResourceAuthorizationV1 {
    fn from(v: &native::AgentDelegationAuthorizationV1) -> Self {
        Self {
            grant: (&v.grant).into(),
            cost_units: DecimalU64(v.cost_units),
            spent_units: DecimalU64(v.spent_units),
            remaining_units: DecimalU64(v.remaining_units),
        }
    }
}
impl From<ResourceAuthorizationV1> for native::AgentDelegationAuthorizationV1 {
    fn from(v: ResourceAuthorizationV1) -> Self {
        Self {
            grant: v.grant.into(),
            cost_units: v.cost_units.0,
            spent_units: v.spent_units.0,
            remaining_units: v.remaining_units.0,
        }
    }
}
