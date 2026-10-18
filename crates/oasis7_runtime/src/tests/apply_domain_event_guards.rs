use super::super::*;
use super::pos;
use crate::agent_state::AgentState;
use oasis7_agent_api::world_action::ResourceKind;

fn test_agent_cell(agent_id: &str) -> AgentCell {
    AgentCell::new(AgentState::new(agent_id, pos(0, 0)), 0)
}

fn direct_material_transfer(transfer_id: Option<ActionId>, amount: i64) -> DomainEvent {
    DomainEvent::MaterialTransferred {
        transfer_id,
        requester_agent_id: "operator-a".to_string(),
        from_ledger: MaterialLedgerId::site("site-a"),
        to_ledger: MaterialLedgerId::site("site-b"),
        kind: "iron_ingot".to_string(),
        amount,
        distance_km: 0,
        priority: MaterialTransitPriority::Standard,
        route_id: None,
    }
}

fn direct_material_transfer_state() -> WorldState {
    let mut state = WorldState::default();
    state
        .material_ledgers
        .entry(MaterialLedgerId::site("site-a"))
        .or_default()
        .insert("iron_ingot".to_string(), 40);
    state
}

fn direct_material_balance(state: &WorldState, ledger: &MaterialLedgerId) -> i64 {
    state
        .material_ledgers
        .get(ledger)
        .and_then(|balances| balances.get("iron_ingot"))
        .copied()
        .unwrap_or_default()
}

#[test]
fn direct_material_transfer_duplicate_is_byte_stable() {
    let mut state = direct_material_transfer_state();
    let event = direct_material_transfer(Some(41), 8);
    state
        .apply_domain_event(&event, 1)
        .expect("first direct transfer settles");
    let settled = state.clone();

    state
        .apply_domain_event(&event, 2)
        .expect("exact duplicate is an idempotent no-op");

    assert_eq!(state, settled);
}

#[test]
fn direct_material_transfer_tampered_duplicate_fails_before_mutation() {
    let mut state = direct_material_transfer_state();
    state
        .apply_domain_event(&direct_material_transfer(Some(42), 8), 1)
        .expect("first direct transfer settles");
    let settled = state.clone();

    let result = state.apply_domain_event(&direct_material_transfer(Some(42), 7), 2);

    assert!(matches!(
        result,
        Err(WorldError::ResourceBalanceInvalid { .. })
    ));
    assert_eq!(state, settled);
}

#[test]
fn distinct_direct_material_transfer_ids_may_share_the_same_payload() {
    let mut state = direct_material_transfer_state();
    state
        .apply_domain_event(&direct_material_transfer(Some(43), 8), 1)
        .expect("first direct transfer settles");
    state
        .apply_domain_event(&direct_material_transfer(Some(44), 8), 2)
        .expect("distinct identity settles independently");

    assert_eq!(
        direct_material_balance(&state, &MaterialLedgerId::site("site-a")),
        24
    );
    assert_eq!(
        direct_material_balance(&state, &MaterialLedgerId::site("site-b")),
        16
    );
}

#[test]
fn legacy_direct_material_transfer_without_identity_still_deserializes() {
    let event = direct_material_transfer(Some(45), 8);
    let mut legacy = serde_json::to_value(event).expect("serialize direct transfer");
    legacy["data"]
        .as_object_mut()
        .expect("domain event variant data serializes as an object")
        .remove("transfer_id");

    let decoded: DomainEvent =
        serde_json::from_value(legacy).expect("legacy direct transfer remains compatible");
    assert!(matches!(
        decoded,
        DomainEvent::MaterialTransferred {
            transfer_id: None,
            ..
        }
    ));
}

fn sample_contract(creator_agent_id: &str, counterparty_agent_id: &str) -> EconomicContractState {
    EconomicContractState {
        contract_id: "contract.guard".to_string(),
        creator_agent_id: creator_agent_id.to_string(),
        counterparty_agent_id: counterparty_agent_id.to_string(),
        fulfillment_kind: EconomicContractFulfillmentKind::AtomicExchange,
        settlement_kind: ResourceKind::Data,
        settlement_amount: 10,
        reputation_stake: 1,
        expires_at: 50,
        description: "guard contract".to_string(),
        status: EconomicContractStatus::Accepted,
        accepted_at: Some(5),
        settled_at: None,
        settlement_success: None,
        transfer_amount: 0,
        tax_amount: 0,
        settlement_notes: None,
    }
}

#[test]
fn economic_contract_legacy_persistence_defaults_to_atomic_exchange() {
    let mut legacy = serde_json::to_value(sample_contract("creator", "counterparty"))
        .expect("serialize legacy economic contract");
    legacy
        .as_object_mut()
        .expect("legacy economic contract serializes as an object")
        .remove("fulfillment_kind");
    let decoded: EconomicContractState =
        serde_json::from_value(legacy).expect("deserialize legacy economic contract");
    let persisted = serde_json::to_value(decoded).expect("serialize migrated economic contract");

    assert_eq!(
        persisted["fulfillment_kind"],
        serde_json::json!("atomic_exchange")
    );
}

#[test]
fn economic_contract_persisted_service_cannot_be_settled_by_atomic_event() {
    let mut service = serde_json::to_value(sample_contract("creator", "counterparty"))
        .expect("serialize service fixture");
    service["fulfillment_kind"] = serde_json::json!("service");
    let service: EconomicContractState =
        serde_json::from_value(service).expect("deserialize persisted service fixture");

    let mut state = WorldState::default();
    state
        .agents
        .insert("creator".to_string(), test_agent_cell("creator"));
    state
        .agents
        .insert("counterparty".to_string(), test_agent_cell("counterparty"));
    state
        .agents
        .get_mut("creator")
        .expect("creator agent")
        .state
        .resources
        .set(ResourceKind::Data, 17)
        .expect("seed creator data");
    state
        .agents
        .get_mut("counterparty")
        .expect("counterparty agent")
        .state
        .resources
        .set(ResourceKind::Data, 23)
        .expect("seed counterparty data");
    state.reputation_scores.insert("creator".to_string(), 11);
    state
        .reputation_scores
        .insert("counterparty".to_string(), -4);
    state
        .economic_contracts
        .insert("contract.guard".to_string(), service);

    let err = state
        .apply_domain_event(
            &DomainEvent::EconomicContractSettled {
                operator_agent_id: "creator".to_string(),
                contract_id: "contract.guard".to_string(),
                success: false,
                transfer_amount: 0,
                tax_amount: 0,
                notes: "unsupported service breach".to_string(),
                creator_reputation_delta: 0,
                counterparty_reputation_delta: 0,
            },
            8,
        )
        .expect_err("persisted Service contracts must be rejected by atomic settlement");

    assert_eq!(
        err,
        WorldError::ResourceBalanceInvalid {
            reason: "service contracts unavailable: collateral/evidence/remedy not implemented"
                .to_string(),
        }
    );
    let contract = state
        .economic_contracts
        .get("contract.guard")
        .expect("service contract retained");
    assert_eq!(contract.status, EconomicContractStatus::Accepted);
    assert_eq!(contract.settlement_success, None);
    assert_eq!(contract.settled_at, None);
    assert_eq!(
        state
            .agents
            .get("creator")
            .expect("creator agent")
            .state
            .resources
            .get(ResourceKind::Data),
        17
    );
    assert_eq!(
        state
            .agents
            .get("counterparty")
            .expect("counterparty agent")
            .state
            .resources
            .get(ResourceKind::Data),
        23
    );
    assert_eq!(state.reputation_scores.get("creator"), Some(&11));
    assert_eq!(state.reputation_scores.get("counterparty"), Some(&-4));
}

#[test]
fn economic_contract_persisted_service_cannot_be_expired_by_atomic_lifecycle() {
    let mut contract = sample_contract("creator", "counterparty");
    contract.fulfillment_kind = EconomicContractFulfillmentKind::Service;
    let mut state = WorldState::default();
    state.reputation_scores.insert("creator".to_string(), 11);
    state
        .reputation_scores
        .insert("counterparty".to_string(), -4);
    state
        .economic_contracts
        .insert("contract.guard".to_string(), contract);

    let err = state
        .apply_domain_event(
            &DomainEvent::EconomicContractExpired {
                contract_id: "contract.guard".to_string(),
                creator_agent_id: "creator".to_string(),
                counterparty_agent_id: "counterparty".to_string(),
                creator_reputation_delta: -6,
                counterparty_reputation_delta: -3,
            },
            51,
        )
        .expect_err("persisted Service contracts must be quarantined from expiry");

    assert_eq!(
        err,
        WorldError::ResourceBalanceInvalid {
            reason: "service contracts unavailable: collateral/evidence/remedy not implemented"
                .to_string(),
        }
    );
    let contract = state
        .economic_contracts
        .get("contract.guard")
        .expect("service contract retained");
    assert_eq!(contract.status, EconomicContractStatus::Accepted);
    assert_eq!(contract.settled_at, None);
    assert_eq!(contract.settlement_success, None);
    assert_eq!(state.reputation_scores.get("creator"), Some(&11));
    assert_eq!(state.reputation_scores.get("counterparty"), Some(&-4));
}

fn sample_claim(target_agent_id: &str, claimer_agent_id: &str) -> AgentClaimState {
    AgentClaimState {
        target_agent_id: target_agent_id.to_string(),
        claim_owner_id: claimer_agent_id.to_string(),
        reputation_tier: 0,
        slot_index: 1,
        activation_fee_amount: 10,
        activation_fee_burn_amount: 1,
        activation_fee_treasury_amount: 4,
        claim_bond_amount: 6,
        locked_bond_amount: 6,
        upfront_restricted_spent_amount: 0,
        upfront_liquid_spent_amount: 16,
        claim_bond_locked_restricted_amount: 0,
        claim_bond_locked_liquid_amount: 6,
        claim_bond_restricted_source_treasury_bucket_id: None,
        upkeep_per_epoch: 2,
        release_cooldown_epochs: 1,
        grace_epochs: 1,
        idle_warning_epochs: 1,
        forced_idle_reclaim_epochs: 2,
        forced_reclaim_penalty_bps: 500,
        claimed_at_epoch: 3,
        upkeep_paid_through_epoch: 3,
        delinquent_since_epoch: None,
        grace_deadline_epoch: None,
        release_requested_at_epoch: None,
        release_ready_at_epoch: None,
        idle_warning_emitted_at_epoch: None,
    }
}

#[test]
fn economic_contract_settlement_missing_operator_returns_agent_not_found() {
    let mut state = WorldState::default();
    state
        .agents
        .insert("creator".to_string(), test_agent_cell("creator"));
    state
        .agents
        .insert("counterparty".to_string(), test_agent_cell("counterparty"));
    state.economic_contracts.insert(
        "contract.guard".to_string(),
        sample_contract("creator", "counterparty"),
    );

    let err = state
        .apply_domain_event(
            &DomainEvent::EconomicContractSettled {
                operator_agent_id: "missing".to_string(),
                contract_id: "contract.guard".to_string(),
                success: false,
                transfer_amount: 0,
                tax_amount: 0,
                notes: "missing operator".to_string(),
                creator_reputation_delta: 0,
                counterparty_reputation_delta: 0,
            },
            8,
        )
        .expect_err("missing operator must be rejected");

    assert_eq!(
        err,
        WorldError::AgentNotFound {
            agent_id: "missing".to_string(),
        }
    );
    assert_eq!(
        state
            .economic_contracts
            .get("contract.guard")
            .expect("contract kept")
            .status,
        EconomicContractStatus::Accepted
    );
}

#[test]
fn claim_release_request_missing_claimer_returns_error_without_mutating_claim() {
    let mut state = WorldState::default();
    state
        .agents
        .insert("target".to_string(), test_agent_cell("target"));
    state
        .agent_claims
        .insert("target".to_string(), sample_claim("target", "claimer"));

    let err = state
        .apply_domain_event(
            &DomainEvent::AgentClaimReleaseRequested {
                claimer_agent_id: "claimer".to_string(),
                target_agent_id: "target".to_string(),
                requested_at_epoch: 4,
                ready_at_epoch: 5,
            },
            9,
        )
        .expect_err("missing claimer must be rejected");

    assert_eq!(
        err,
        WorldError::AgentNotFound {
            agent_id: "claimer".to_string(),
        }
    );
    let claim = state.agent_claims.get("target").expect("claim retained");
    assert_eq!(claim.release_requested_at_epoch, None);
    assert_eq!(claim.release_ready_at_epoch, None);
}

#[test]
fn apply_domain_event_agent_move_keeps_integer_centimeter_positions() {
    let mut state = WorldState::default();
    state
        .agents
        .insert("agent-1".to_string(), test_agent_cell("agent-1"));

    state
        .apply_domain_event(
            &DomainEvent::AgentMoved {
                agent_id: "agent-1".to_string(),
                from: pos(0, 0),
                to: pos(10, 20),
            },
            1,
        )
        .expect("move should apply");

    let agent = state.agents.get("agent-1").expect("agent exists");
    assert_eq!(agent.state.pos.x_cm, 10);
    assert_eq!(agent.state.pos.y_cm, 20);
}

#[test]
fn claim_reclaim_missing_claimer_returns_error_without_removing_claim() {
    let mut state = WorldState::default();
    state
        .agents
        .insert("target".to_string(), test_agent_cell("target"));
    state
        .agent_claims
        .insert("target".to_string(), sample_claim("target", "claimer"));

    let err = state
        .apply_domain_event(
            &DomainEvent::AgentClaimReclaimed {
                claimer_agent_id: "claimer".to_string(),
                target_agent_id: "target".to_string(),
                reclaimed_at_epoch: 8,
                reason: "missing claimer".to_string(),
                upkeep_arrears_amount: 0,
                collected_upkeep_amount: 0,
                penalty_amount: 0,
                refunded_bond_amount: 6,
                refunded_bond_restricted_amount: 0,
                refunded_bond_liquid_amount: 6,
                refunded_bond_restricted_sink:
                    RestrictedStarterClaimRefundSink::BeneficiaryRestrictedBalance,
                refunded_bond_restricted_sink_bucket_id: String::new(),
            },
            10,
        )
        .expect_err("missing claimer must be rejected");

    assert_eq!(
        err,
        WorldError::AgentNotFound {
            agent_id: "claimer".to_string(),
        }
    );
    assert!(
        state.agent_claims.contains_key("target"),
        "claim should remain present after early error"
    );
}
