use super::DomainEvent;
use crate::{EpochSettlementReport, MaterialLedgerId, NodeSettlement};
use oasis7_agent_api::world_action::{GeoPos, MaterialTransitPriority, ResourceKind};
use oasis7_wasm_abi::MaterialStack;

pub(super) fn settlement_event(compute_score: f64) -> DomainEvent {
    DomainEvent::NodePointsSettlementApplied {
        report: EpochSettlementReport {
            epoch_index: 23,
            pool_points: 29,
            storage_pool_points: 31,
            distributed_points: 37,
            storage_distributed_points: 41,
            total_distributed_points: 43,
            settlements: vec![NodeSettlement {
                node_id: "node-a".to_owned(),
                obligation_met: true,
                compute_score,
                storage_score: 0.25,
                uptime_score: 0.5,
                reliability_score: 0.75,
                storage_reward_score: 1.0,
                rewardable_storage_bytes: 47,
                penalty_score: 0.0,
                total_score: 0.5,
                main_awarded_points: 53,
                storage_awarded_points: 59,
                awarded_points: 61,
                cumulative_points: 67,
            }],
        },
        signer_node_id: "signer".to_owned(),
        settlement_hash: "settlement-hash".to_owned(),
        minted_records: Vec::new(),
        main_token_bridge_total_amount: 71,
        main_token_bridge_distributions: Vec::new(),
    }
}

pub(super) fn debug_samples() -> Vec<(&'static str, DomainEvent)> {
    vec![
        (
            "world_agent.agent_moved",
            DomainEvent::AgentMoved {
                agent_id: "agent-a".to_owned(),
                from: GeoPos::new(1, -2, 3),
                to: GeoPos::new(4, 5, -6),
            },
        ),
        (
            "module_lifecycle.module_artifact_deployed",
            DomainEvent::ModuleArtifactDeployed {
                publisher_agent_id: "publisher".to_owned(),
                wasm_hash: "sha256:abc".to_owned(),
                bytes_len: 7,
                fee_kind: ResourceKind::Electricity,
                fee_amount: 11,
            },
        ),
        (
            "data_power.data_collected_authenticated",
            DomainEvent::DataCollectedAuthenticated {
                collector_agent_id: "collector".to_owned(),
                electricity_cost: 13,
                data_amount: 17,
                player_id: "player".to_owned(),
                public_key: "public-key".to_owned(),
                nonce: 19,
                signature: "signature".to_owned(),
            },
        ),
        (
            "token_claims.node_points_settlement_applied",
            settlement_event(0.375),
        ),
        (
            "logistics.material_transferred",
            DomainEvent::MaterialTransferred {
                transfer_id: Some(73),
                requester_agent_id: "requester".to_owned(),
                from_ledger: MaterialLedgerId::world(),
                to_ledger: MaterialLedgerId::world(),
                kind: "ore".to_owned(),
                amount: 79,
                distance_km: 83,
                priority: MaterialTransitPriority::Standard,
                route_id: Some("route-a".to_owned()),
            },
        ),
        (
            "production.factory_durability_changed",
            DomainEvent::FactoryDurabilityChanged {
                factory_id: "factory-a".to_owned(),
                previous_durability_ppm: 89,
                durability_ppm: 97,
                reason: "wear".to_owned(),
            },
        ),
        (
            "governance.crisis_spawned",
            DomainEvent::CrisisSpawned {
                crisis_id: "crisis-a".to_owned(),
                kind: "supply".to_owned(),
                severity: 2,
                expires_at: 101,
            },
        ),
        (
            "validation.product_validated",
            DomainEvent::ProductValidated {
                requester_agent_id: "requester".to_owned(),
                module_id: "product-module".to_owned(),
                stack: MaterialStack::new("ore", 103),
                stack_limit: 107,
                tradable: true,
                quality_levels: vec!["common".to_owned(), "rare".to_owned()],
                notes: vec!["validated".to_owned()],
            },
        ),
    ]
}
