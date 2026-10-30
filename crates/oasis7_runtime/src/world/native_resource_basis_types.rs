//! Unsigned, as-of resource facts. These Rust-only values are never a use grant
//! or an HTTP codec. A future transport must bind its own trusted World pin.
use crate::{
    Action, AgentDelegationAuthorizationV1, LogisticsTransferQuote, MaterialLedgerId,
    MaterialTransitPriority, NativeBuiltinFactoryQuote, NativeBuiltinRecipeQuote,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResourceBasisRoutingV1 {
    Direct,
    Routes { route_ids: Vec<String> },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResourceBasisSelectionV1 {
    Transfer {
        from: MaterialLedgerId,
        to: MaterialLedgerId,
        material: String,
        amount: i64,
        distance_km: i64,
        priority: MaterialTransitPriority,
        routing: ResourceBasisRoutingV1,
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

fn id(value: &str) -> bool {
    !value.trim().is_empty() && value.len() <= 256 && !value.chars().any(char::is_control)
}

fn ledger(value: &MaterialLedgerId) -> bool {
    match value {
        MaterialLedgerId::World => true,
        MaterialLedgerId::Agent(value)
        | MaterialLedgerId::Site(value)
        | MaterialLedgerId::Factory(value) => id(value),
    }
}

impl ResourceBasisSelectionV1 {
    pub fn validate(&self) -> bool {
        match self {
            Self::Transfer {
                from,
                to,
                material,
                amount,
                distance_km,
                routing,
                ..
            } => {
                ledger(from)
                    && ledger(to)
                    && from != to
                    && from != &MaterialLedgerId::World
                    && id(material)
                    && *amount > 0
                    && *distance_km >= 0
                    && match routing {
                        ResourceBasisRoutingV1::Direct => true,
                        ResourceBasisRoutingV1::Routes { route_ids } => {
                            !route_ids.is_empty()
                                && route_ids.len() <= 16
                                && route_ids.iter().enumerate().all(|(index, route)| {
                                    id(route) && !route_ids[..index].contains(route)
                                })
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
                    && crate::builtin_factory_spec(factory_kind, factory).is_some()
            }
            Self::Schedule {
                factory,
                recipe,
                batches,
            } => {
                id(factory)
                    && id(recipe)
                    && *batches > 0
                    && crate::builtin_recipe_plan(recipe, *batches).is_some()
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceBasisBlockerV1 {
    InvalidSelection,
    AgentOwnerUnavailable,
    AdmissionUnavailable,
    LedgerOrObjectUnavailable,
    DomainUnavailable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourceActorOwnerV1 {
    pub player_id: String,
    pub source_id: String,
    pub public_key: Option<String>,
    pub generation: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResourceLedgerOwnerV1 {
    World,
    Agent {
        agent_id: String,
        owner: ResourceActorOwnerV1,
    },
    Site {
        site_id: String,
        owner_agent_id: String,
        authority_revision: u64,
    },
    Factory {
        factory_id: String,
        builder_agent_id: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourceMaterialBalanceV1 {
    pub material: String,
    /// None is actual absence, not an invented zero balance.
    pub amount: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourceLedgerFactV1 {
    pub ledger: MaterialLedgerId,
    pub owner: ResourceLedgerOwnerV1,
    pub exists: bool,
    pub materials: Vec<ResourceMaterialBalanceV1>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResourceObjectFactV1 {
    Site {
        site_id: String,
        owner_agent_id: String,
        authorized_agent_ids: Vec<String>,
        authority_revision: u64,
        location_id: String,
    },
    Factory {
        factory_id: String,
        builder_agent_id: String,
        input_ledger: MaterialLedgerId,
        output_ledger: MaterialLedgerId,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct ResourceTransferQuoteV1 {
    pub action: Action,
    pub authorization: AgentDelegationAuthorizationV1,
    pub quote: LogisticsTransferQuote,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ResourceBasisQuoteV1 {
    Transfer(Box<ResourceTransferQuoteV1>),
    Build(Box<NativeBuiltinFactoryQuote>),
    Schedule(Box<NativeBuiltinRecipeQuote>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct NativeResourceBasisFactV1 {
    pub agent_id: String,
    pub actor_owner: ResourceActorOwnerV1,
    pub quoted_at_tick: u64,
    pub actor_electricity_before: i64,
    pub selection: ResourceBasisSelectionV1,
    pub quote: ResourceBasisQuoteV1,
    pub ledgers: Vec<ResourceLedgerFactV1>,
    pub objects: Vec<ResourceObjectFactV1>,
    /// Display/allowlist references only; never authority without typed facts.
    pub derived_resource_refs: Vec<String>,
}
