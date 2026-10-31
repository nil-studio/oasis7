use super::*;
use oasis7_agent_api::world_action::{Action, MaterialLedgerId, MaterialTransitPriority};
impl From<&MaterialLedgerId> for ResourceLedgerV1 {
    fn from(v: &MaterialLedgerId) -> Self {
        match v {
            MaterialLedgerId::World => Self::World,
            MaterialLedgerId::Agent(v) => Self::Agent {
                agent_id: v.clone(),
            },
            MaterialLedgerId::Site(v) => Self::Site { site_id: v.clone() },
            MaterialLedgerId::Factory(v) => Self::Factory {
                factory_id: v.clone(),
            },
        }
    }
}
impl From<ResourceLedgerV1> for MaterialLedgerId {
    fn from(v: ResourceLedgerV1) -> Self {
        match v {
            ResourceLedgerV1::World => Self::World,
            ResourceLedgerV1::Agent { agent_id } => Self::Agent(agent_id),
            ResourceLedgerV1::Site { site_id } => Self::Site(site_id),
            ResourceLedgerV1::Factory { factory_id } => Self::Factory(factory_id),
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "data", deny_unknown_fields)]
pub enum ResourceQuoteActionV1 {
    TransferMaterial {
        requester_agent_id: String,
        from_ledger: ResourceLedgerV1,
        to_ledger: ResourceLedgerV1,
        kind: String,
        amount: ResourceDecimalI64V1,
        distance_km: ResourceDecimalI64V1,
        #[serde(deserialize_with = "required_option")]
        priority: Option<MaterialTransitPriority>,
        #[serde(deserialize_with = "required_option")]
        route_id: Option<String>,
        route_ids: Vec<String>,
        auto_reroute: bool,
    },
    BuildFactory {
        builder_agent_id: String,
        site_id: String,
        spec: ResourceFactorySpecV1,
    },
    ScheduleRecipe {
        requester_agent_id: String,
        factory_id: String,
        recipe_id: String,
        plan: ResourceRecipePlanV1,
        logistics_route_ids: Vec<String>,
        logistics_path_ids: Vec<String>,
    },
}
impl TryFrom<&Action> for ResourceQuoteActionV1 {
    type Error = String;
    fn try_from(v: &Action) -> Result<Self, String> {
        Ok(match v {
            Action::TransferMaterial {
                requester_agent_id,
                from_ledger,
                to_ledger,
                kind,
                amount,
                distance_km,
                priority,
                route_id,
                route_ids,
                auto_reroute,
            } => Self::TransferMaterial {
                requester_agent_id: requester_agent_id.clone(),
                from_ledger: from_ledger.into(),
                to_ledger: to_ledger.into(),
                kind: kind.clone(),
                amount: ResourceDecimalI64V1(*amount),
                distance_km: ResourceDecimalI64V1(*distance_km),
                priority: *priority,
                route_id: route_id.clone(),
                route_ids: route_ids.clone(),
                auto_reroute: *auto_reroute,
            },
            Action::BuildFactory {
                builder_agent_id,
                site_id,
                spec,
            } => Self::BuildFactory {
                builder_agent_id: builder_agent_id.clone(),
                site_id: site_id.clone(),
                spec: spec.into(),
            },
            Action::ScheduleRecipe {
                requester_agent_id,
                factory_id,
                recipe_id,
                plan,
                logistics_route_ids,
                logistics_path_ids,
            } => Self::ScheduleRecipe {
                requester_agent_id: requester_agent_id.clone(),
                factory_id: factory_id.clone(),
                recipe_id: recipe_id.clone(),
                plan: plan.into(),
                logistics_route_ids: logistics_route_ids.clone(),
                logistics_path_ids: logistics_path_ids.clone(),
            },
            _ => return Err(no()),
        })
    }
}
impl From<ResourceQuoteActionV1> for Action {
    fn from(v: ResourceQuoteActionV1) -> Self {
        match v {
            ResourceQuoteActionV1::TransferMaterial {
                requester_agent_id,
                from_ledger,
                to_ledger,
                kind,
                amount,
                distance_km,
                priority,
                route_id,
                route_ids,
                auto_reroute,
            } => Self::TransferMaterial {
                requester_agent_id,
                from_ledger: from_ledger.into(),
                to_ledger: to_ledger.into(),
                kind,
                amount: amount.0,
                distance_km: distance_km.0,
                priority,
                route_id,
                route_ids,
                auto_reroute,
            },
            ResourceQuoteActionV1::BuildFactory {
                builder_agent_id,
                site_id,
                spec,
            } => Self::BuildFactory {
                builder_agent_id,
                site_id,
                spec: spec.into(),
            },
            ResourceQuoteActionV1::ScheduleRecipe {
                requester_agent_id,
                factory_id,
                recipe_id,
                plan,
                logistics_route_ids,
                logistics_path_ids,
            } => Self::ScheduleRecipe {
                requester_agent_id,
                factory_id,
                recipe_id,
                plan: plan.into(),
                logistics_route_ids,
                logistics_path_ids,
            },
        }
    }
}
