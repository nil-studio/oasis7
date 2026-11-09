//! Resource V2 wire components. Historical evidence, never a use grant.
pub use oasis7_agent_api::game::DecimalU64;
use serde::{Deserialize, Serialize};
#[path = "resource_basis_decimal.rs"]
mod decimal;
pub use decimal::*;
#[path = "resource_basis_quote_authority.rs"]
mod quote_authority;
pub use quote_authority::*;
#[path = "resource_basis_quote_action.rs"]
mod quote_action;
pub use quote_action::*;
#[path = "resource_basis_quote_economy.rs"]
mod quote_economy;
pub use quote_economy::*;
#[path = "resource_basis_quote_rejection.rs"]
mod quote_rejection;
pub use quote_rejection::*;
#[path = "resource_basis_quote.rs"]
mod quote;
pub use quote::*;
#[path = "resource_basis_observation.rs"]
mod observation;
pub use observation::*;
pub const RESOURCE_GOAL_PATH_V2: &str = "/v1/world/private-goal/current-resource-basis-v2";
pub const OWNER_RESOURCE_VIEW_PATH_V1: &str = "/v1/world/participant-owner-resource-view-v1";
pub const NATIVE_OWNER_RESOURCE_VIEW_PATH_V1: &str =
    "/api/public/hosted-account/v2/participant/owner-resource-view";
pub const MAX_RESOURCE_ENVELOPE_BYTES: usize = 64 * 1024;
pub(crate) fn id(v: &str) -> bool {
    !v.trim().is_empty() && v.len() <= 256 && !v.chars().any(char::is_control)
}
pub(crate) fn hex64(v: &str) -> bool {
    v.len() == 64
        && v.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
pub(crate) fn no() -> String {
    "invalid resource observation".into()
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ResourceLedgerV1 {
    World,
    Agent { agent_id: String },
    Site { site_id: String },
    Factory { factory_id: String },
}
impl ResourceLedgerV1 {
    pub fn validate(&self) -> Result<(), String> {
        if match self {
            Self::World => true,
            Self::Agent { agent_id } => id(agent_id),
            Self::Site { site_id } => id(site_id),
            Self::Factory { factory_id } => id(factory_id),
        } {
            Ok(())
        } else {
            Err(no())
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ResourceRoutingV1 {
    Direct,
    Routes { route_ids: Vec<String> },
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ResourceBasisSelectionV1 {
    Transfer {
        from: ResourceLedgerV1,
        to: ResourceLedgerV1,
        material: String,
        amount: ResourceDecimalI64V1,
        distance_km: ResourceDecimalI64V1,
        priority: oasis7_agent_api::world_action::MaterialTransitPriority,
        routing: ResourceRoutingV1,
    },
    Build {
        site: String,
        factory: String,
        factory_kind: String,
    },
    Schedule {
        factory: String,
        recipe: String,
        batches: u32,
    },
}
impl ResourceBasisSelectionV1 {
    pub fn validate(&self) -> Result<(), String> {
        let valid = match self {
            Self::Transfer {
                from,
                to,
                material,
                amount,
                distance_km,
                routing,
                ..
            } => {
                from.validate()?;
                to.validate()?;
                from != to
                    && *from != ResourceLedgerV1::World
                    && id(material)
                    && amount.0 > 0
                    && distance_km.0 >= 0
                    && match routing {
                        ResourceRoutingV1::Direct => true,
                        ResourceRoutingV1::Routes { route_ids } => {
                            !route_ids.is_empty()
                                && route_ids.len() <= 16
                                && route_ids
                                    .iter()
                                    .enumerate()
                                    .all(|(i, r)| id(r) && !route_ids[..i].contains(r))
                        }
                    }
            }
            Self::Build {
                site,
                factory,
                factory_kind,
            } => {
                id(site)
                    && id(factory)
                    && id(factory_kind)
                    && oasis7_agent_api::world_quote::builtin_factory_spec(factory_kind, factory)
                        .is_some()
            }
            Self::Schedule {
                factory,
                recipe,
                batches,
            } => {
                id(factory)
                    && id(recipe)
                    && *batches > 0
                    && oasis7_agent_api::world_quote::builtin_recipe_plan(recipe, *batches)
                        .is_some()
            }
        };
        if valid { Ok(()) } else { Err(no()) }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResourceActorOwnerV1 {
    pub player_id: String,
    pub source_id: String,
    #[serde(deserialize_with = "required_option")]
    pub public_key: Option<String>,
    pub generation: DecimalU64,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ResourceLedgerOwnerV1 {
    World,
    Agent {
        agent_id: String,
        owner: ResourceActorOwnerV1,
    },
    Site {
        site_id: String,
        owner_agent_id: String,
        authority_revision: DecimalU64,
    },
    Factory {
        factory_id: String,
        builder_agent_id: String,
    },
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResourceMaterialBalanceV1 {
    pub material: String,
    #[serde(deserialize_with = "required_option")]
    pub amount: Option<ResourceDecimalI64V1>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResourceLedgerFactV1 {
    pub ledger: ResourceLedgerV1,
    pub owner: ResourceLedgerOwnerV1,
    pub exists: bool,
    pub materials: Vec<ResourceMaterialBalanceV1>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ResourceObjectFactV1 {
    Site {
        site_id: String,
        owner_agent_id: String,
        authorized_agent_ids: Vec<String>,
        authority_revision: DecimalU64,
        location_id: String,
    },
    Factory {
        factory_id: String,
        builder_agent_id: String,
        input_ledger: ResourceLedgerV1,
        output_ledger: ResourceLedgerV1,
    },
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResourceBasisFactV1 {
    pub agent_id: String,
    pub actor_owner: ResourceActorOwnerV1,
    pub quoted_at_tick: DecimalU64,
    pub actor_electricity_before: ResourceDecimalI64V1,
    pub selection: ResourceBasisSelectionV1,
    pub quote: ResourceBasisQuoteV1,
    pub ledgers: Vec<ResourceLedgerFactV1>,
    pub objects: Vec<ResourceObjectFactV1>,
    pub derived_resource_refs: Vec<String>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceBasisBlockerV1 {
    InvalidSelection,
    AgentOwnerUnavailable,
    AdmissionUnavailable,
    LedgerOrObjectUnavailable,
    DomainUnavailable,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub enum ResourceBasisResolutionV1 {
    Ready { fact: Box<ResourceBasisFactV1> },
    Blocked { reason: ResourceBasisBlockerV1 },
}
pub(crate) fn required_option<'de, D, T>(d: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::deserialize(d)
}

#[path = "resource_basis_verify.rs"]
mod verify;
pub use verify::*;

#[path = "resource_basis_fact_verify.rs"]
mod fact_verify;

#[cfg(test)]
#[path = "resource_basis_tests.rs"]
mod tests;
