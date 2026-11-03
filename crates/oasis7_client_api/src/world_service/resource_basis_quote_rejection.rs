use super::decimal::strict_map;
use super::*;
use oasis7_agent_api::world_action::ResourceKind;
use oasis7_agent_api::world_quote::RejectReason;
use std::collections::BTreeMap;
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "data", deny_unknown_fields)]
pub enum ResourceQuoteRejectionV1 {
    AgentAlreadyExists {
        agent_id: String,
    },
    AgentNotFound {
        agent_id: String,
    },
    AgentsNotCoLocated {
        agent_id: String,
        other_agent_id: String,
    },
    InvalidAmount {
        amount: ResourceDecimalI64V1,
    },
    InsufficientResource {
        agent_id: String,
        kind: ResourceKind,
        requested: ResourceDecimalI64V1,
        available: ResourceDecimalI64V1,
    },
    InsufficientResources {
        #[serde(deserialize_with = "strict_map")]
        deficits: BTreeMap<ResourceKind, ResourceDecimalI64V1>,
    },
    InsufficientMaterial {
        material_kind: String,
        requested: ResourceDecimalI64V1,
        available: ResourceDecimalI64V1,
    },
    MaterialTransferDistanceExceeded {
        distance_km: ResourceDecimalI64V1,
        max_distance_km: ResourceDecimalI64V1,
    },
    MaterialTransitCapacityExceeded {
        in_flight: DecimalU64,
        max_in_flight: DecimalU64,
    },
    FactoryNotFound {
        factory_id: String,
    },
    FactoryBusy {
        factory_id: String,
        active_jobs: DecimalU64,
        recipe_slots: u16,
    },
    RuleDenied {
        notes: Vec<String>,
    },
}
impl TryFrom<&RejectReason> for ResourceQuoteRejectionV1 {
    type Error = String;
    fn try_from(v: &RejectReason) -> Result<Self, String> {
        Ok(match v {
            RejectReason::AgentAlreadyExists { agent_id } => Self::AgentAlreadyExists {
                agent_id: agent_id.clone(),
            },
            RejectReason::AgentNotFound { agent_id } => Self::AgentNotFound {
                agent_id: agent_id.clone(),
            },
            RejectReason::AgentsNotCoLocated {
                agent_id,
                other_agent_id,
            } => Self::AgentsNotCoLocated {
                agent_id: agent_id.clone(),
                other_agent_id: other_agent_id.clone(),
            },
            RejectReason::InvalidAmount { amount } => Self::InvalidAmount {
                amount: ResourceDecimalI64V1(*amount),
            },
            RejectReason::InsufficientResource {
                agent_id,
                kind,
                requested,
                available,
            } => Self::InsufficientResource {
                agent_id: agent_id.clone(),
                kind: *kind,
                requested: ResourceDecimalI64V1(*requested),
                available: ResourceDecimalI64V1(*available),
            },
            RejectReason::InsufficientResources { deficits } => Self::InsufficientResources {
                deficits: deficits
                    .iter()
                    .map(|(k, v)| (*k, ResourceDecimalI64V1(*v)))
                    .collect(),
            },
            RejectReason::InsufficientMaterial {
                material_kind,
                requested,
                available,
            } => Self::InsufficientMaterial {
                material_kind: material_kind.clone(),
                requested: ResourceDecimalI64V1(*requested),
                available: ResourceDecimalI64V1(*available),
            },
            RejectReason::MaterialTransferDistanceExceeded {
                distance_km,
                max_distance_km,
            } => Self::MaterialTransferDistanceExceeded {
                distance_km: ResourceDecimalI64V1(*distance_km),
                max_distance_km: ResourceDecimalI64V1(*max_distance_km),
            },
            RejectReason::MaterialTransitCapacityExceeded {
                in_flight,
                max_in_flight,
            } => Self::MaterialTransitCapacityExceeded {
                in_flight: DecimalU64(u64::try_from(*in_flight).map_err(|_| no())?),
                max_in_flight: DecimalU64(u64::try_from(*max_in_flight).map_err(|_| no())?),
            },
            RejectReason::FactoryNotFound { factory_id } => Self::FactoryNotFound {
                factory_id: factory_id.clone(),
            },
            RejectReason::FactoryBusy {
                factory_id,
                active_jobs,
                recipe_slots,
            } => Self::FactoryBusy {
                factory_id: factory_id.clone(),
                active_jobs: DecimalU64(u64::try_from(*active_jobs).map_err(|_| no())?),
                recipe_slots: *recipe_slots,
            },
            RejectReason::RuleDenied { notes } => Self::RuleDenied {
                notes: notes.clone(),
            },
        })
    }
}
impl TryFrom<ResourceQuoteRejectionV1> for RejectReason {
    type Error = String;
    fn try_from(v: ResourceQuoteRejectionV1) -> Result<Self, String> {
        Ok(match v {
            ResourceQuoteRejectionV1::AgentAlreadyExists { agent_id } => {
                Self::AgentAlreadyExists { agent_id }
            }
            ResourceQuoteRejectionV1::AgentNotFound { agent_id } => {
                Self::AgentNotFound { agent_id }
            }
            ResourceQuoteRejectionV1::AgentsNotCoLocated {
                agent_id,
                other_agent_id,
            } => Self::AgentsNotCoLocated {
                agent_id,
                other_agent_id,
            },
            ResourceQuoteRejectionV1::InvalidAmount { amount } => {
                Self::InvalidAmount { amount: amount.0 }
            }
            ResourceQuoteRejectionV1::InsufficientResource {
                agent_id,
                kind,
                requested,
                available,
            } => Self::InsufficientResource {
                agent_id,
                kind,
                requested: requested.0,
                available: available.0,
            },
            ResourceQuoteRejectionV1::InsufficientResources { deficits } => {
                Self::InsufficientResources {
                    deficits: deficits.into_iter().map(|(k, v)| (k, v.0)).collect(),
                }
            }
            ResourceQuoteRejectionV1::InsufficientMaterial {
                material_kind,
                requested,
                available,
            } => Self::InsufficientMaterial {
                material_kind,
                requested: requested.0,
                available: available.0,
            },
            ResourceQuoteRejectionV1::MaterialTransferDistanceExceeded {
                distance_km,
                max_distance_km,
            } => Self::MaterialTransferDistanceExceeded {
                distance_km: distance_km.0,
                max_distance_km: max_distance_km.0,
            },
            ResourceQuoteRejectionV1::MaterialTransitCapacityExceeded {
                in_flight,
                max_in_flight,
            } => Self::MaterialTransitCapacityExceeded {
                in_flight: usize::try_from(in_flight.0).map_err(|_| no())?,
                max_in_flight: usize::try_from(max_in_flight.0).map_err(|_| no())?,
            },
            ResourceQuoteRejectionV1::FactoryNotFound { factory_id } => {
                Self::FactoryNotFound { factory_id }
            }
            ResourceQuoteRejectionV1::FactoryBusy {
                factory_id,
                active_jobs,
                recipe_slots,
            } => Self::FactoryBusy {
                factory_id,
                active_jobs: usize::try_from(active_jobs.0).map_err(|_| no())?,
                recipe_slots,
            },
            ResourceQuoteRejectionV1::RuleDenied { notes } => Self::RuleDenied { notes },
        })
    }
}
