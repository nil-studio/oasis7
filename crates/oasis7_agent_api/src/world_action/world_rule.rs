//! Versioned native world-rule intents. Authority is revalidated at commit.
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "data", deny_unknown_fields)]
pub enum WorldRuleCommandV1 {
    MinimumService {
        command: super::MinimumServiceCommandV1,
    },
    OrganizationContinuity {
        command: super::OrganizationContinuityCommandV1,
    },
    EmergencySupply {
        command: super::EmergencySupplyCommandV1,
    },
    DepositOrganizationEstate {
        organization_id: String,
        material: String,
        amount: i64,
        held_in_trust: bool,
    },
    OpenOrganizationProtection {
        organization_id: String,
    },
    BeginOrganizationDissolution {
        organization_id: String,
    },
    AdvanceOrganization {
        case_id: String,
        expected_revision: u64,
    },
    GrantOrganizationScope {
        case_id: String,
        expected_revision: u64,
        proposal_key: String,
        delegate_id: String,
        scopes: BTreeSet<OrganizationEstateScopeV1>,
        expires_at: u64,
        replace_grant_id: Option<String>,
    },
    RevokeOrganizationScope {
        case_id: String,
        expected_revision: u64,
        grant_id: String,
        proposal_key: String,
    },
    RequestOrganizationRecovery {
        case_id: String,
        expected_revision: u64,
        appeal: bool,
    },
    ResolveOrganizationRecovery {
        case_id: String,
        expected_revision: u64,
        recovery_request_id: String,
        approved: bool,
        proposal_key: String,
    },
    DisposeOrganizationEstate {
        case_id: String,
        expected_revision: u64,
        grant_id: String,
        grant_revision: u64,
        deposit_id: String,
    },
    AuthorizeSupply {
        proposal_key: String,
        crisis_id: String,
        material: String,
        beneficiaries: BTreeSet<String>,
        budget: i64,
        expires_at: u64,
    },
    RequestSupply {
        authorization_id: String,
        cumulative_demand: i64,
    },
    FreezeSupply {
        authorization_id: String,
    },
    AllocateSupply {
        batch_id: String,
    },
    DispatchSupply {
        batch_id: String,
        beneficiary_id: String,
        amount: i64,
        route_id: String,
    },
    SettleSupply {
        job_id: u64,
    },
    CloseSupply {
        authorization_id: String,
    },
    ConfirmAgentOrder {
        factory_id: String,
        quoted_at: u64,
        quote_digest: String,
    },
    AdvanceAgentOrder {
        order_id: String,
    },
    CancelAgentOrder {
        order_id: String,
    },
    DisposeAgentOrder {
        order_id: String,
    },
    DeliverAgentOrder {
        order_id: String,
    },
}

/// Governance proposals bind the entire intent, excluding their own reference.
pub fn world_rule_proposal_description(
    command: &WorldRuleCommandV1,
) -> Result<String, serde_json::Error> {
    let mut bound = command.clone();
    match &mut bound {
        WorldRuleCommandV1::EmergencySupply { command } => command.clear_governance_proposal(),
        WorldRuleCommandV1::OrganizationContinuity { command } => {
            command.clear_governance_proposal()
        }
        WorldRuleCommandV1::AuthorizeSupply { proposal_key, .. }
        | WorldRuleCommandV1::GrantOrganizationScope { proposal_key, .. }
        | WorldRuleCommandV1::RevokeOrganizationScope { proposal_key, .. }
        | WorldRuleCommandV1::ResolveOrganizationRecovery { proposal_key, .. } => {
            proposal_key.clear()
        }
        _ => {}
    }
    serde_json::to_string(&bound).map(|json| format!("world-rule-v1:{json}"))
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct OrganizationEstateScopeV1 {
    pub object_id: String,
    pub right: OrganizationEstateRightV1,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrganizationEstateRightV1 {
    Preserve,
    ReturnTrust,
    DistributeResidual,
    FinalizeCase,
    AdvanceCase,
}

/// Stable business receipt/effect identity. This is a correlation key only;
/// deriving it grants no authority and does not commit an operation.
pub fn world_rule_request_key_v1(actor: &str, request: &str) -> String {
    let mut hash = blake3::Hasher::new();
    hash.update(b"oasis7.world-rule.request.v1");
    hash.update(&(actor.len() as u64).to_le_bytes());
    hash.update(actor.as_bytes());
    hash.update(&(request.len() as u64).to_le_bytes());
    hash.update(request.as_bytes());
    format!("rule:{}", hash.finalize().to_hex())
}

#[cfg(test)]
mod request_key_tests {
    use super::world_rule_request_key_v1;

    #[test]
    fn business_effect_identity_preserves_v1_receipts_and_component_boundaries() {
        assert_eq!(
            world_rule_request_key_v1("agent-a", "effect-preview-only"),
            "rule:12c04decf162923ca4aff8cb6f02ae53e3121b2b3d857a993336b7cb6125b9e9"
        );
        assert_ne!(
            world_rule_request_key_v1("ab", "c"),
            world_rule_request_key_v1("a", "bc")
        );
    }
}

impl WorldRuleCommandV1 {
    /// Exact object used by existing native delegation grants; no wildcard scope.
    pub fn authority_scope(&self) -> String {
        match self {
            Self::EmergencySupply { command } => command.authority_scope(),
            Self::MinimumService { command } => command.authority_scope(),
            Self::OrganizationContinuity { command } => command.authority_scope(),
            Self::ConfirmAgentOrder { factory_id, .. } => factory_id.clone(),
            Self::DeliverAgentOrder { order_id }
            | Self::AdvanceAgentOrder { order_id }
            | Self::CancelAgentOrder { order_id }
            | Self::DisposeAgentOrder { order_id } => order_id.clone(),
            Self::AuthorizeSupply { crisis_id, .. } => crisis_id.clone(),
            Self::RequestSupply {
                authorization_id, ..
            }
            | Self::FreezeSupply { authorization_id }
            | Self::CloseSupply { authorization_id } => authorization_id.clone(),
            Self::AllocateSupply { batch_id } | Self::DispatchSupply { batch_id, .. } => {
                batch_id.clone()
            }
            Self::SettleSupply { job_id } => job_id.to_string(),
            Self::DepositOrganizationEstate {
                organization_id, ..
            }
            | Self::BeginOrganizationDissolution { organization_id }
            | Self::OpenOrganizationProtection { organization_id } => organization_id.clone(),
            Self::AdvanceOrganization { case_id, .. }
            | Self::GrantOrganizationScope { case_id, .. }
            | Self::RevokeOrganizationScope { case_id, .. }
            | Self::RequestOrganizationRecovery { case_id, .. }
            | Self::ResolveOrganizationRecovery { case_id, .. }
            | Self::DisposeOrganizationEstate { case_id, .. } => case_id.clone(),
        }
    }
}

impl WorldRuleCommandV1 {
    pub fn governance_proposal(&self) -> Option<&str> {
        match self {
            Self::EmergencySupply { command } => command.governance_proposal(),
            Self::OrganizationContinuity { command } => command.governance_proposal(),
            Self::AuthorizeSupply { proposal_key, .. }
            | Self::GrantOrganizationScope { proposal_key, .. }
            | Self::RevokeOrganizationScope { proposal_key, .. }
            | Self::ResolveOrganizationRecovery { proposal_key, .. } => Some(proposal_key),
            _ => None,
        }
    }

    /// Commands that only persist runtime-observed watermarks. The journal
    /// remains their durable proof, so they need neither per-tick user
    /// receipts nor agent mailbox delivery.
    pub fn is_runtime_observation(&self) -> bool {
        match self {
            Self::OrganizationContinuity { command } => command.is_runtime_observation(),
            Self::EmergencySupply { command } => matches!(
                command,
                super::EmergencySupplyCommandV1::Observe { .. }
                    | super::EmergencySupplyCommandV1::Close { .. }
            ),
            _ => false,
        }
    }
}
