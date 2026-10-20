//! Owner-confirmed alternatives for unsupported waypoint/timed movement requests.
//! Models and delegated executors cannot manufacture owner confirmation.
use oasis7_agent_api::world_action::GeoPos;
use serde::{Deserialize, Serialize};

pub const SAFE_ALTERNATIVE_API_REV: u32 = 1;
pub const MAX_ALTERNATIVE_WAYPOINTS: usize = 8;
pub const MAX_ALTERNATIVE_GOAL_BYTES: usize = 4096;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FineGrainedMoveRequestV1 {
    pub goal: String,
    pub waypoints: Vec<GeoPos>,
    pub arrival_tick: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "operation",
    content = "data",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum SafeAlternativeOperationV1 {
    Propose {
        request_id: String,
        original: FineGrainedMoveRequestV1,
        valid_until_tick: u64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        predecessor_request_id: Option<String>,
    },
    Reevaluate {
        request_id: String,
        valid_until_tick: u64,
    },
    Confirm {
        request_id: String,
        generation: u64,
        candidate_id: String,
        candidate_digest: String,
        confirmation_id: String,
        valid_until_tick: u64,
    },
    Execute {
        request_id: String,
        authorization_id: String,
        execution_id: String,
    },
    Cancel {
        request_id: String,
    },
    Read {
        request_id: String,
    },
}
impl SafeAlternativeOperationV1 {
    pub fn request_id(&self) -> &str {
        match self {
            Self::Propose { request_id, .. }
            | Self::Confirm { request_id, .. }
            | Self::Execute { request_id, .. }
            | Self::Cancel { request_id }
            | Self::Read { request_id }
            | Self::Reevaluate { request_id, .. } => request_id,
        }
    }
    pub fn signing_domain(&self) -> &'static str {
        match self {
            Self::Propose { .. } => "safe_alternative.propose.v1",
            Self::Reevaluate { .. } => "safe_alternative.reevaluate.v1",
            Self::Confirm { .. } => "safe_alternative.confirm.v1",
            Self::Execute { .. } => "safe_alternative.execute.v1",
            Self::Cancel { .. } => "safe_alternative.cancel.v1",
            Self::Read { .. } => "safe_alternative.read.v1",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SafeAlternativeCommandV1 {
    pub api_rev: u32,
    pub client_operation_id: String,
    pub world_id: String,
    pub genesis_digest: String,
    pub branch_id: String,
    pub reorg_epoch: u64,
    pub agent_id: String,
    pub owner_binding: String,
    pub owner_generation: u64,
    pub operation: SafeAlternativeOperationV1,
}
impl SafeAlternativeCommandV1 {
    pub fn signing_domain(&self) -> &'static str {
        self.operation.signing_domain()
    }
    pub fn validate(&self) -> Result<(), String> {
        let id =
            |s: &str| !s.trim().is_empty() && s.len() <= 256 && !s.chars().any(char::is_control);
        if self.api_rev != SAFE_ALTERNATIVE_API_REV {
            return Err("version_unsupported".into());
        }
        if ![
            &self.client_operation_id,
            &self.world_id,
            &self.genesis_digest,
            &self.branch_id,
            &self.agent_id,
            &self.owner_binding,
            self.operation.request_id(),
        ]
        .into_iter()
        .all(|s| id(s))
        {
            return Err("identity_conflict".into());
        }
        match &self.operation {
            SafeAlternativeOperationV1::Propose {
                original,
                predecessor_request_id,
                ..
            } => {
                if predecessor_request_id
                    .as_deref()
                    .is_some_and(|v| !id(v) || v == self.operation.request_id())
                {
                    return Err("identity_conflict".into());
                }
                if original.goal.trim().is_empty()
                    || original.goal.len() > MAX_ALTERNATIVE_GOAL_BYTES
                    || original.waypoints.is_empty()
                    || original.waypoints.len() > MAX_ALTERNATIVE_WAYPOINTS
                {
                    return Err("evaluation_unavailable".into());
                }
                if original.waypoints.len() == 1 && original.arrival_tick.is_none() {
                    return Err("original_not_eligible".into());
                }
            }
            SafeAlternativeOperationV1::Confirm {
                candidate_id,
                candidate_digest,
                confirmation_id,
                generation,
                ..
            } => {
                if *generation == 0
                    || !id(candidate_id)
                    || !id(candidate_digest)
                    || !id(confirmation_id)
                {
                    return Err("identity_conflict".into());
                }
            }
            SafeAlternativeOperationV1::Execute {
                authorization_id,
                execution_id,
                ..
            } => {
                if !id(authorization_id) || !id(execution_id) {
                    return Err("identity_conflict".into());
                }
            }
            _ => {}
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{world_service::*, world_service_wire::WorldServicePayloadV1};
    fn command() -> SafeAlternativeCommandV1 {
        SafeAlternativeCommandV1 {
            api_rev: 1,
            client_operation_id: "proposal-1".into(),
            world_id: "world".into(),
            genesis_digest: "genesis".into(),
            branch_id: "main".into(),
            reorg_epoch: 0,
            agent_id: "agent".into(),
            owner_binding: "owner".into(),
            owner_generation: 1,
            operation: SafeAlternativeOperationV1::Propose {
                request_id: "original".into(),
                original: FineGrainedMoveRequestV1 {
                    goal: "Arrive through the waypoints".into(),
                    waypoints: vec![GeoPos::new(1, 2, 3), GeoPos::new(4, 5, 6)],
                    arrival_tick: None,
                },
                valid_until_tick: 100,
                predecessor_request_id: None,
            },
        }
    }
    #[test]
    fn unsupported_original_is_bounded_and_supported_move_is_not_reinterpreted() {
        let mut c = command();
        c.validate().unwrap();
        let SafeAlternativeOperationV1::Propose { original, .. } = &mut c.operation else {
            unreachable!()
        };
        original.waypoints.truncate(1);
        assert_eq!(c.validate().unwrap_err(), "original_not_eligible");
        let SafeAlternativeOperationV1::Propose { original, .. } = &mut c.operation else {
            unreachable!()
        };
        original.arrival_tick = Some(50);
        original.goal = "x".repeat(MAX_ALTERNATIVE_GOAL_BYTES + 1);
        assert!(c.validate().is_err());
        let mut c = command();
        let SafeAlternativeOperationV1::Propose { original, .. } = &mut c.operation else {
            unreachable!()
        };
        original.waypoints = vec![GeoPos::new(0, 0, 0); MAX_ALTERNATIVE_WAYPOINTS + 1];
        assert!(c.validate().is_err());
    }
    #[test]
    fn owner_signature_cannot_cross_operation_domains_or_worlds() {
        let c = command();
        let signed =
            sign_read_request(c.signing_domain(), c.clone(), &hex::encode([7; 32])).unwrap();
        let payload: WorldServicePayloadV1 = WorldServicePayloadV1::SafeAlternative(signed.clone());
        let world = WorldIdentity {
            world_id: "world".into(),
            genesis_digest: "genesis".into(),
        };
        let correlation =
            crate::world_service_correlation::derive_correlation(world.clone(), &payload).unwrap();
        assert_eq!(correlation.key.request_id_or_nonce, "proposal-1");
        let mut tampered = signed;
        tampered.request.operation = SafeAlternativeOperationV1::Cancel {
            request_id: "original".into(),
        };
        assert!(
            crate::world_service_correlation::derive_correlation(
                world.clone(),
                &WorldServicePayloadV1::SafeAlternative(tampered)
            )
            .is_err()
        );
        assert!(
            crate::world_service_correlation::derive_correlation(
                WorldIdentity {
                    world_id: "other".into(),
                    ..world
                },
                &payload
            )
            .is_err()
        );
        let wrong = sign_read_request("agent_chat", c, &hex::encode([7; 32])).unwrap();
        assert!(
            crate::world_service_correlation::derive_correlation(
                WorldIdentity {
                    world_id: "world".into(),
                    genesis_digest: "genesis".into()
                },
                &WorldServicePayloadV1::SafeAlternative(wrong)
            )
            .is_err()
        );
    }
}
