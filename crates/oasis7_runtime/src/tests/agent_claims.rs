use super::super::*;
use super::pos;
use std::collections::{BTreeMap, BTreeSet};

#[path = "agent_claims/basic_claim_regressions.rs"]
mod basic_claim_regressions;
#[path = "agent_claims/raw_claim_monetary_publication_transaction_regressions.rs"]
mod raw_claim_monetary_publication_transaction_regressions;
#[path = "agent_claims/raw_claim_terminal_publication_transaction_regressions.rs"]
mod raw_claim_terminal_publication_transaction_regressions;
#[path = "agent_claims/raw_light_lifecycle_publication_transaction_regressions.rs"]
mod raw_light_lifecycle_publication_transaction_regressions;

fn register_agent(world: &mut World, agent_id: &str) {
    world.submit_action(Action::RegisterAgent {
        agent_id: agent_id.to_string(),
        pos: pos(0, 0),
    });
    world.step().expect("register agent");
}

fn setup_claim_world(balance: u64, reputation_score: i64) -> World {
    setup_claim_world_with_balances(balance, 0, reputation_score)
}

fn setup_claim_world_with_balances(
    liquid_balance: u64,
    restricted_balance: u64,
    reputation_score: i64,
) -> World {
    let mut world = World::new();
    world
        .set_governance_execution_policy(GovernanceExecutionPolicy {
            epoch_length_ticks: 1,
            ..GovernanceExecutionPolicy::default()
        })
        .expect("set governance epoch length");
    register_agent(&mut world, "alice");
    register_agent(&mut world, "bob");
    register_agent(&mut world, "carol");
    world
        .set_agent_reputation_score("alice", reputation_score)
        .expect("set alice reputation");
    world.set_main_token_supply(MainTokenSupplyState {
        total_supply: liquid_balance.saturating_add(restricted_balance),
        circulating_supply: liquid_balance.saturating_add(restricted_balance),
        ..MainTokenSupplyState::default()
    });
    world
        .set_main_token_account_balance_with_restricted(
            "alice",
            liquid_balance,
            0,
            restricted_balance,
        )
        .expect("seed alice main token balance");
    world
        .set_main_token_account_balance_with_restricted(
            "carol",
            liquid_balance,
            0,
            restricted_balance,
        )
        .expect("seed carol main token balance");
    world
}

fn setup_claim_world_with_treasury(
    treasury_balance: u64,
    alice_liquid_balance: u64,
    reputation_score: i64,
) -> World {
    let mut world = World::new();
    world
        .set_governance_execution_policy(GovernanceExecutionPolicy {
            epoch_length_ticks: 1,
            ..GovernanceExecutionPolicy::default()
        })
        .expect("set governance epoch length");
    register_agent(&mut world, "alice");
    register_agent(&mut world, "bob");
    register_agent(&mut world, "carol");
    world
        .set_agent_reputation_score("alice", reputation_score)
        .expect("set alice reputation");
    world.set_main_token_supply(MainTokenSupplyState {
        total_supply: treasury_balance.saturating_add(alice_liquid_balance),
        circulating_supply: alice_liquid_balance,
        ..MainTokenSupplyState::default()
    });
    world
        .set_main_token_treasury_balance(
            MAIN_TOKEN_TREASURY_BUCKET_RESTRICTED_STARTER_CLAIM_LIVEOPS_POOL,
            treasury_balance,
        )
        .expect("seed restricted liveops treasury");
    if alice_liquid_balance > 0 {
        world
            .set_main_token_account_balance("alice", alice_liquid_balance, 0)
            .expect("seed alice liquid balance");
    }
    world
}

fn allowlist_restricted_grant_admins(world: &mut World, admin_account_ids: &[&str]) {
    configure_restricted_grant_registry(
        world,
        &["msig.ecosystem_governance.v1"],
        admin_account_ids,
    );
}

fn configure_restricted_grant_registry(
    world: &mut World,
    policy_account_ids: &[&str],
    admin_account_ids: &[&str],
) {
    let mut controller_signer_policies = BTreeMap::from([(
        "msig.genesis.v1".to_string(),
        GovernanceThresholdSignerPolicy {
            threshold: 1,
            allowed_public_keys: BTreeSet::from([
                "6249e5a58278dbc4e629a16b5d33f6b84c39e3ceeb10e963bb9ef64ea4daac30".to_string(),
            ]),
        },
    )]);
    let mut all_policy_account_ids = policy_account_ids
        .iter()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .collect::<BTreeSet<_>>();
    for account_id in admin_account_ids {
        let account_id = account_id.trim();
        if !account_id.is_empty() {
            all_policy_account_ids.insert(account_id.to_string());
        }
    }
    for (index, account_id) in all_policy_account_ids.into_iter().enumerate() {
        controller_signer_policies.insert(
            account_id,
            GovernanceThresholdSignerPolicy {
                threshold: 1,
                allowed_public_keys: BTreeSet::from([format!("{:064x}", index + 3)]),
            },
        );
    }
    world
        .set_governance_main_token_controller_registry(GovernanceMainTokenControllerRegistry {
            genesis_controller_account_id: "msig.genesis.v1".to_string(),
            treasury_bucket_controller_slots: BTreeMap::from([(
                MAIN_TOKEN_TREASURY_BUCKET_ECOSYSTEM_POOL.to_string(),
                policy_account_ids
                    .first()
                    .copied()
                    .unwrap_or("msig.ecosystem_governance.v1")
                    .to_string(),
            )]),
            restricted_starter_claim_admin_account_ids: admin_account_ids
                .iter()
                .map(|value| (*value).to_string())
                .collect(),
            controller_signer_policies,
        })
        .expect("set restricted grant admin registry");
}

fn claim_upfront_amount(claim: &AgentClaimState) -> u64 {
    claim.activation_fee_amount + claim.claim_bond_amount + claim.upkeep_per_epoch
}

fn upkeep_settlement_total(world: &World, target_agent_id: &str) -> u64 {
    world
        .journal()
        .events
        .iter()
        .filter_map(|event| match &event.body {
            WorldEventBody::Domain(DomainEvent::AgentClaimUpkeepSettled {
                target_agent_id: event_target_agent_id,
                amount,
                ..
            }) if event_target_agent_id == target_agent_id => Some(*amount),
            _ => None,
        })
        .sum()
}

#[test]
fn concurrent_claim_conflict_charges_only_winner() {
    let mut world = setup_claim_world(1_000, 0);
    let journal_len_before = world.journal().events.len();

    world.submit_action(Action::ClaimAgent {
        claimer_agent_id: "alice".to_string(),
        target_agent_id: "bob".to_string(),
    });
    world.submit_action(Action::ClaimAgent {
        claimer_agent_id: "carol".to_string(),
        target_agent_id: "bob".to_string(),
    });
    world.step().expect("process competing claims");

    let claim = world.agent_claim("bob").expect("winning claim persisted");
    assert_eq!(claim.claim_owner_id, "alice");
    assert_eq!(
        world.main_token_liquid_balance("alice"),
        1_000 - claim_upfront_amount(claim)
    );
    assert_eq!(world.main_token_liquid_balance("carol"), 1_000);
    assert_eq!(
        world.main_token_treasury_balance(MAIN_TOKEN_TREASURY_BUCKET_ECOSYSTEM_POOL),
        claim.activation_fee_treasury_amount + claim.upkeep_per_epoch
    );
    assert_eq!(
        world.main_token_treasury_balance(MAIN_TOKEN_TREASURY_BUCKET_SLASH),
        0
    );

    let mut claim_events = 0;
    let mut rejection_notes = Vec::new();
    for event in &world.journal().events[journal_len_before..] {
        match &event.body {
            WorldEventBody::Domain(DomainEvent::AgentClaimed { .. }) => {
                claim_events += 1;
            }
            WorldEventBody::Domain(DomainEvent::ActionRejected {
                reason: RejectReason::RuleDenied { notes },
                ..
            }) => {
                rejection_notes.extend(notes.iter().cloned());
            }
            _ => {}
        }
    }
    assert_eq!(claim_events, 1);
    assert!(
        rejection_notes
            .iter()
            .any(|note| note.contains("already claimed"))
    );
}

#[test]
fn claimed_agent_rejects_second_owner() {
    let mut world = setup_claim_world(1_000, 0);

    world.submit_action(Action::ClaimAgent {
        claimer_agent_id: "alice".to_string(),
        target_agent_id: "bob".to_string(),
    });
    world.step().expect("initial claim");

    world.submit_action(Action::ClaimAgent {
        claimer_agent_id: "carol".to_string(),
        target_agent_id: "bob".to_string(),
    });
    let journal_len_before = world.journal().events.len();
    world.step().expect("reject duplicate claim");

    let rejection = world.journal().events[journal_len_before..]
        .iter()
        .find_map(|event| match &event.body {
            WorldEventBody::Domain(DomainEvent::ActionRejected { reason, .. }) => Some(reason),
            _ => None,
        })
        .expect("duplicate claim rejection event");
    match rejection {
        RejectReason::RuleDenied { notes } => {
            assert!(notes.iter().any(|note| note.contains("already claimed")));
        }
        other => panic!("expected rule denied, got {other:?}"),
    }
    assert_eq!(world.claimed_agent_count("alice"), 1);
    assert_eq!(world.claimed_agent_count("carol"), 0);
}

#[test]
fn reputation_tier_scales_second_slot_and_enforces_claim_cap() {
    let mut world = setup_claim_world(4_000, 10);
    register_agent(&mut world, "dave");
    register_agent(&mut world, "erin");

    world.submit_action(Action::ClaimAgent {
        claimer_agent_id: "alice".to_string(),
        target_agent_id: "bob".to_string(),
    });
    world.step().expect("claim slot 1");
    let first_claim = world.agent_claim("bob").expect("slot 1 claim").clone();

    world.submit_action(Action::ClaimAgent {
        claimer_agent_id: "alice".to_string(),
        target_agent_id: "carol".to_string(),
    });
    world.step().expect("claim slot 2");
    let second_claim = world.agent_claim("carol").expect("slot 2 claim").clone();

    assert_eq!(first_claim.reputation_tier, 1);
    assert_eq!(first_claim.slot_index, 1);
    assert_eq!(second_claim.reputation_tier, 1);
    assert_eq!(second_claim.slot_index, 2);
    assert!(claim_upfront_amount(&second_claim) > claim_upfront_amount(&first_claim));
    assert_eq!(world.claimed_agent_count("alice"), 2);

    let journal_len_before = world.journal().events.len();
    world.submit_action(Action::ClaimAgent {
        claimer_agent_id: "alice".to_string(),
        target_agent_id: "dave".to_string(),
    });
    world.step().expect("reject slot 3 over cap");

    let rejection = world.journal().events[journal_len_before..]
        .iter()
        .find_map(|event| match &event.body {
            WorldEventBody::Domain(DomainEvent::ActionRejected { reason, .. }) => Some(reason),
            _ => None,
        })
        .expect("cap rejection event");
    match rejection {
        RejectReason::RuleDenied { notes } => {
            assert!(notes.iter().any(|note| note.contains("cap exceeded")));
        }
        other => panic!("expected rule denied, got {other:?}"),
    }
    assert!(world.agent_claim("dave").is_none());
    assert_eq!(world.claimed_agent_count("alice"), 2);
}

#[test]
fn release_refunds_remaining_bond_after_cooldown() {
    let mut world = setup_claim_world(2_000, 0);

    world.submit_action(Action::ClaimAgent {
        claimer_agent_id: "alice".to_string(),
        target_agent_id: "bob".to_string(),
    });
    world.step().expect("claim agent");
    let claim = world.agent_claim("bob").expect("claim").clone();
    let cooldown_epochs = claim.release_cooldown_epochs;

    world.submit_action(Action::ReleaseAgentClaim {
        claimer_agent_id: "alice".to_string(),
        target_agent_id: "bob".to_string(),
    });
    world.step().expect("request release");
    assert!(
        world
            .agent_claim("bob")
            .expect("claim still held")
            .release_requested_at_epoch
            .is_some()
    );

    for _ in 0..cooldown_epochs.saturating_sub(1) {
        world.step().expect("advance cooldown");
    }
    let balance_before_final_step = world.main_token_liquid_balance("alice");
    world.step().expect("finalize release");

    assert!(world.agent_claim("bob").is_none());
    let settled_upkeep = upkeep_settlement_total(&world, "bob");
    assert!(world.main_token_liquid_balance("alice") > balance_before_final_step);
    assert_eq!(
        world.main_token_liquid_balance("alice"),
        2_000 - claim_upfront_amount(&claim) - settled_upkeep + claim.locked_bond_amount
    );
    assert_eq!(
        world.main_token_treasury_balance(MAIN_TOKEN_TREASURY_BUCKET_SLASH),
        0
    );
    match &world.journal().events.last().expect("event").body {
        WorldEventBody::Domain(DomainEvent::AgentClaimReleased {
            target_agent_id,
            refunded_bond_amount,
            ..
        }) => {
            assert_eq!(target_agent_id, "bob");
            assert_eq!(*refunded_bond_amount, claim.locked_bond_amount);
        }
        other => panic!("expected AgentClaimReleased, got {other:?}"),
    }
}

#[test]
fn grace_claim_recovers_when_owner_refills_before_deadline() {
    let mut world = setup_claim_world(325, 0);

    world.submit_action(Action::ClaimAgent {
        claimer_agent_id: "alice".to_string(),
        target_agent_id: "bob".to_string(),
    });
    world.step().expect("claim agent");
    world.step().expect("enter grace");

    let grace_claim = world.agent_claim("bob").expect("claim in grace").clone();
    assert!(grace_claim.grace_deadline_epoch.is_some());

    let topup = grace_claim.upkeep_per_epoch * 2;
    let mut supply = world.main_token_supply().clone();
    supply.total_supply += topup;
    supply.circulating_supply += topup;
    world.set_main_token_supply(supply);
    world
        .set_main_token_account_balance("alice", topup, 0)
        .expect("top up alice");

    world.step().expect("recover from grace");

    let recovered = world.agent_claim("bob").expect("claim recovered");
    assert_eq!(
        recovered.upkeep_paid_through_epoch,
        grace_claim.upkeep_paid_through_epoch + 2
    );
    assert!(recovered.grace_deadline_epoch.is_none());
    assert!(recovered.delinquent_since_epoch.is_none());
    assert_eq!(world.main_token_liquid_balance("alice"), 0);
    assert_eq!(
        world.main_token_treasury_balance(MAIN_TOKEN_TREASURY_BUCKET_ECOSYSTEM_POOL),
        grace_claim.activation_fee_treasury_amount + grace_claim.upkeep_per_epoch + topup
    );
}

#[test]
fn missed_upkeep_enters_grace_then_forced_reclaim() {
    let mut world = setup_claim_world(325, 0);

    world.submit_action(Action::ClaimAgent {
        claimer_agent_id: "alice".to_string(),
        target_agent_id: "bob".to_string(),
    });
    world.step().expect("claim agent");
    assert_eq!(world.main_token_liquid_balance("alice"), 0);
    let initial_claim = world.agent_claim("bob").expect("claim exists").clone();

    world.step().expect("enter grace");
    let claim = world.agent_claim("bob").expect("claim still held");
    assert!(claim.grace_deadline_epoch.is_some());
    match &world.journal().events.last().expect("event").body {
        WorldEventBody::Domain(DomainEvent::AgentClaimEnteredGrace {
            target_agent_id,
            upkeep_arrears_amount,
            ..
        }) => {
            assert_eq!(target_agent_id, "bob");
            assert!(*upkeep_arrears_amount > 0);
        }
        other => panic!("expected AgentClaimEnteredGrace, got {other:?}"),
    }

    while world.agent_claim("bob").is_some() {
        world.step().expect("advance toward forced reclaim");
    }
    match &world.journal().events.last().expect("event").body {
        WorldEventBody::Domain(DomainEvent::AgentClaimReclaimed {
            target_agent_id,
            reason,
            upkeep_arrears_amount,
            collected_upkeep_amount,
            penalty_amount,
            refunded_bond_amount,
            ..
        }) => {
            assert_eq!(target_agent_id, "bob");
            assert_eq!(reason, "upkeep_delinquent");
            assert_eq!(*upkeep_arrears_amount, 75);
            assert_eq!(*collected_upkeep_amount, 75);
            assert_eq!(*penalty_amount, 25);
            assert_eq!(*refunded_bond_amount, 100);
        }
        other => panic!("expected AgentClaimReclaimed, got {other:?}"),
    }
    assert_eq!(
        world.main_token_treasury_balance(MAIN_TOKEN_TREASURY_BUCKET_ECOSYSTEM_POOL),
        initial_claim.activation_fee_treasury_amount + initial_claim.upkeep_per_epoch + 75
    );
    assert_eq!(
        world.main_token_treasury_balance(MAIN_TOKEN_TREASURY_BUCKET_SLASH),
        25
    );
    assert_eq!(world.main_token_liquid_balance("alice"), 100);
}

#[test]
fn idle_claim_emits_warning_then_reclaims() {
    let mut world = setup_claim_world(2_000, 0);

    world.submit_action(Action::ClaimAgent {
        claimer_agent_id: "alice".to_string(),
        target_agent_id: "bob".to_string(),
    });
    world.step().expect("claim agent");
    let claim = world.agent_claim("bob").expect("claim").clone();
    let warning_epochs = claim.idle_warning_epochs;
    let reclaim_epochs = claim.forced_idle_reclaim_epochs;

    let mut saw_warning = false;
    for _ in 0..=reclaim_epochs + 1 {
        world.step().expect("advance idle clock");
        if matches!(
            &world.journal().events.last().expect("event").body,
            WorldEventBody::Domain(DomainEvent::AgentClaimIdleWarning { .. })
        ) {
            saw_warning = true;
            break;
        }
    }
    assert!(
        saw_warning,
        "expected idle warning by epoch {warning_epochs}"
    );

    while world.agent_claim("bob").is_some() {
        world.step().expect("advance to idle reclaim");
    }
    match &world.journal().events.last().expect("event").body {
        WorldEventBody::Domain(DomainEvent::AgentClaimReclaimed {
            target_agent_id,
            reason,
            upkeep_arrears_amount,
            collected_upkeep_amount,
            penalty_amount,
            refunded_bond_amount,
            ..
        }) => {
            assert_eq!(target_agent_id, "bob");
            assert_eq!(reason, "idle_timeout");
            assert_eq!(*upkeep_arrears_amount, 0);
            assert_eq!(*collected_upkeep_amount, 0);
            assert_eq!(*penalty_amount, 40);
            assert_eq!(*refunded_bond_amount, 160);
        }
        other => panic!("expected AgentClaimReclaimed, got {other:?}"),
    }
    let settled_upkeep = upkeep_settlement_total(&world, "bob");
    assert_eq!(
        world.main_token_treasury_balance(MAIN_TOKEN_TREASURY_BUCKET_ECOSYSTEM_POOL),
        claim.activation_fee_treasury_amount + claim.upkeep_per_epoch + settled_upkeep
    );
    assert_eq!(
        world.main_token_treasury_balance(MAIN_TOKEN_TREASURY_BUCKET_SLASH),
        40
    );
    assert_eq!(
        world.main_token_liquid_balance("alice"),
        2_000 - claim_upfront_amount(&claim) - settled_upkeep + 160
    );
}

#[test]
fn slot_1_claim_can_spend_restricted_balance() {
    let mut world = setup_claim_world_with_balances(50, 325, 0);

    world.submit_action(Action::ClaimAgent {
        claimer_agent_id: "alice".to_string(),
        target_agent_id: "bob".to_string(),
    });
    world
        .step()
        .expect("claim first agent with restricted balance");

    let claim = world.agent_claim("bob").expect("claim persisted");
    assert_eq!(claim.upfront_restricted_spent_amount, 325);
    assert_eq!(claim.upfront_liquid_spent_amount, 0);
    assert_eq!(claim.claim_bond_locked_restricted_amount, 200);
    assert_eq!(claim.claim_bond_locked_liquid_amount, 0);
    assert_eq!(world.main_token_liquid_balance("alice"), 50);
    assert_eq!(
        world.main_token_restricted_starter_claim_balance("alice"),
        0
    );
}

#[test]
fn mixed_slot_1_claim_tracks_bond_provenance_and_refunds_back_to_source_buckets() {
    let mut world = setup_claim_world_with_balances(500, 150, 0);

    world.submit_action(Action::ClaimAgent {
        claimer_agent_id: "alice".to_string(),
        target_agent_id: "bob".to_string(),
    });
    world.step().expect("claim first agent with mixed funding");
    let claim = world.agent_claim("bob").expect("claim exists").clone();
    assert_eq!(claim.upfront_restricted_spent_amount, 150);
    assert_eq!(claim.upfront_liquid_spent_amount, 175);
    assert_eq!(claim.claim_bond_locked_restricted_amount, 50);
    assert_eq!(claim.claim_bond_locked_liquid_amount, 150);

    world.submit_action(Action::ReleaseAgentClaim {
        claimer_agent_id: "alice".to_string(),
        target_agent_id: "bob".to_string(),
    });
    world.step().expect("request release");
    for _ in 0..claim.release_cooldown_epochs.saturating_sub(1) {
        world.step().expect("advance release cooldown");
    }
    world.step().expect("finalize release");

    assert!(world.agent_claim("bob").is_none());
    assert_eq!(
        world.main_token_restricted_starter_claim_balance("alice"),
        50
    );
    assert_eq!(
        world.main_token_liquid_balance("alice"),
        500 - 175 - upkeep_settlement_total(&world, "bob") + 150
    );
    match &world.journal().events.last().expect("event").body {
        WorldEventBody::Domain(DomainEvent::AgentClaimReleased {
            refunded_bond_restricted_amount,
            refunded_bond_liquid_amount,
            ..
        }) => {
            assert_eq!(*refunded_bond_restricted_amount, 50);
            assert_eq!(*refunded_bond_liquid_amount, 150);
        }
        other => panic!("expected AgentClaimReleased, got {other:?}"),
    }
}

#[test]
fn slot_2_claim_cannot_spend_restricted_balance() {
    let mut world = setup_claim_world_with_balances(200, 500, 10);

    world.submit_action(Action::ClaimAgent {
        claimer_agent_id: "alice".to_string(),
        target_agent_id: "bob".to_string(),
    });
    world.step().expect("claim slot 1");

    let journal_len_before = world.journal().events.len();
    world.submit_action(Action::ClaimAgent {
        claimer_agent_id: "alice".to_string(),
        target_agent_id: "carol".to_string(),
    });
    world.step().expect("reject slot 2 without enough liquid");

    assert!(world.agent_claim("carol").is_none());
    let rejection = world.journal().events[journal_len_before..]
        .iter()
        .find_map(|event| match &event.body {
            WorldEventBody::Domain(DomainEvent::ActionRejected { reason, .. }) => Some(reason),
            _ => None,
        })
        .expect("slot 2 rejection");
    match rejection {
        RejectReason::RuleDenied { notes } => {
            assert!(
                notes
                    .iter()
                    .any(|note| note.contains("restricted/liquid funding unavailable"))
            );
        }
        other => panic!("expected rule denied, got {other:?}"),
    }
}

#[test]
fn slot_1_upkeep_uses_restricted_balance_before_liquid() {
    let mut world = setup_claim_world_with_balances(100, 350, 0);

    world.submit_action(Action::ClaimAgent {
        claimer_agent_id: "alice".to_string(),
        target_agent_id: "bob".to_string(),
    });
    world.step().expect("claim first agent");
    let claimed_at_epoch = world
        .agent_claim("bob")
        .expect("claim exists after activation")
        .claimed_at_epoch;

    world.step().expect("settle slot 1 upkeep from restricted");

    let claim = world.agent_claim("bob").expect("claim still active");
    assert_eq!(claim.upkeep_paid_through_epoch, claimed_at_epoch + 1);
    assert_eq!(
        world.main_token_restricted_starter_claim_balance("alice"),
        0
    );
    assert_eq!(world.main_token_liquid_balance("alice"), 100);
    match &world.journal().events.last().expect("event").body {
        WorldEventBody::Domain(DomainEvent::AgentClaimUpkeepSettled {
            restricted_spent_amount,
            liquid_spent_amount,
            ..
        }) => {
            assert_eq!(*restricted_spent_amount, 25);
            assert_eq!(*liquid_spent_amount, 0);
        }
        other => panic!("expected AgentClaimUpkeepSettled, got {other:?}"),
    }
}

#[test]
fn forced_reclaim_preserves_refund_provenance_after_mixed_funding() {
    let mut world = setup_claim_world_with_balances(1_000, 150, 0);

    world.submit_action(Action::ClaimAgent {
        claimer_agent_id: "alice".to_string(),
        target_agent_id: "bob".to_string(),
    });
    world.step().expect("claim first agent");
    let claim = world.agent_claim("bob").expect("claim exists").clone();

    while world.agent_claim("bob").is_some() {
        world.step().expect("advance to idle reclaim");
    }

    let settled_upkeep = upkeep_settlement_total(&world, "bob");
    assert_eq!(
        world.main_token_restricted_starter_claim_balance("alice"),
        10
    );
    assert_eq!(
        world.main_token_liquid_balance("alice"),
        1_000 - claim.upfront_liquid_spent_amount - settled_upkeep + 150
    );
    match &world.journal().events.last().expect("event").body {
        WorldEventBody::Domain(DomainEvent::AgentClaimReclaimed {
            refunded_bond_restricted_amount,
            refunded_bond_liquid_amount,
            ..
        }) => {
            assert_eq!(*refunded_bond_restricted_amount, 10);
            assert_eq!(*refunded_bond_liquid_amount, 150);
        }
        other => panic!("expected AgentClaimReclaimed, got {other:?}"),
    }
}

#[test]
fn restricted_grant_issue_records_metadata_and_moves_treasury_to_restricted_balance() {
    let mut world = setup_claim_world_with_treasury(1_000, 0, 0);
    allowlist_restricted_grant_admins(&mut world, &["liveops"]);

    world.submit_action(Action::IssueRestrictedStarterClaimGrant {
        issuer_account_id: "liveops".to_string(),
        beneficiary_account_id: "alice".to_string(),
        amount: 325,
        issuance_reason: "qa_seed".to_string(),
        expires_at_epoch: 5,
    });
    world.step().expect("issue restricted grant");

    let grant = world
        .restricted_starter_claim_grant("alice")
        .expect("grant state persisted");
    assert_eq!(grant.issuer_id, "liveops");
    assert_eq!(grant.issuance_reason, "qa_seed");
    assert_eq!(
        grant.spend_scope,
        RESTRICTED_STARTER_CLAIM_GRANT_SPEND_SCOPE_SLOT_1_ONLY
    );
    assert_eq!(grant.issued_amount, 325);
    assert_eq!(grant.expires_at_epoch, 5);
    assert_eq!(grant.status, RestrictedStarterClaimGrantStatus::Issued);
    assert_eq!(
        world.main_token_restricted_starter_claim_balance("alice"),
        325
    );
    assert_eq!(
        world.main_token_treasury_balance(
            MAIN_TOKEN_TREASURY_BUCKET_RESTRICTED_STARTER_CLAIM_LIVEOPS_POOL
        ),
        675
    );
    assert_eq!(world.main_token_supply().circulating_supply, 325);

    match &world.journal().events.last().expect("event").body {
        WorldEventBody::Domain(DomainEvent::RestrictedStarterClaimGrantIssued {
            issuer_id,
            beneficiary_account_id,
            source_treasury_bucket_id,
            amount,
            issuance_reason,
            expires_at_epoch,
            ..
        }) => {
            assert_eq!(issuer_id, "liveops");
            assert_eq!(beneficiary_account_id, "alice");
            assert_eq!(
                source_treasury_bucket_id,
                MAIN_TOKEN_TREASURY_BUCKET_RESTRICTED_STARTER_CLAIM_LIVEOPS_POOL
            );
            assert_eq!(*amount, 325);
            assert_eq!(issuance_reason, "qa_seed");
            assert_eq!(*expires_at_epoch, 5);
        }
        other => panic!("expected RestrictedStarterClaimGrantIssued, got {other:?}"),
    }
}

#[test]
fn expired_restricted_grant_returns_remaining_balance_and_redirects_release_refund_to_treasury() {
    let mut world = setup_claim_world_with_treasury(1_000, 150, 0);
    allowlist_restricted_grant_admins(&mut world, &["liveops"]);

    world.submit_action(Action::IssueRestrictedStarterClaimGrant {
        issuer_account_id: "liveops".to_string(),
        beneficiary_account_id: "alice".to_string(),
        amount: 500,
        issuance_reason: "preview_allowlist".to_string(),
        expires_at_epoch: 6,
    });
    world.step().expect("issue restricted grant");

    world.submit_action(Action::ClaimAgent {
        claimer_agent_id: "alice".to_string(),
        target_agent_id: "bob".to_string(),
    });
    world.step().expect("claim using restricted grant");
    let claim = world.agent_claim("bob").expect("claim exists").clone();
    assert_eq!(claim.claim_bond_locked_restricted_amount, 200);
    assert_eq!(
        world.main_token_restricted_starter_claim_balance("alice"),
        175
    );

    world.step().expect("expire grant and settle upkeep");
    let grant = world
        .restricted_starter_claim_grant("alice")
        .expect("grant still tracked after expiry");
    assert_eq!(grant.status, RestrictedStarterClaimGrantStatus::Expired);
    assert_eq!(
        world.main_token_restricted_starter_claim_balance("alice"),
        0
    );

    world.submit_action(Action::ReleaseAgentClaim {
        claimer_agent_id: "alice".to_string(),
        target_agent_id: "bob".to_string(),
    });
    world.step().expect("request release after grant expiry");
    for _ in 0..claim.release_cooldown_epochs.saturating_sub(1) {
        world.step().expect("advance release cooldown after expiry");
    }
    world.step().expect("finalize release after grant expiry");

    let released_event = world
        .journal()
        .events
        .iter()
        .rev()
        .find(|event| {
            matches!(
                event.body,
                WorldEventBody::Domain(DomainEvent::AgentClaimReleased { .. })
            )
        })
        .expect("missing AgentClaimReleased event");

    match &released_event.body {
        WorldEventBody::Domain(DomainEvent::AgentClaimReleased {
            refunded_bond_restricted_amount,
            refunded_bond_restricted_sink,
            refunded_bond_restricted_sink_bucket_id,
            ..
        }) => {
            assert_eq!(*refunded_bond_restricted_amount, 200);
            assert_eq!(
                *refunded_bond_restricted_sink,
                RestrictedStarterClaimRefundSink::SourceTreasuryBucket
            );
            assert_eq!(
                refunded_bond_restricted_sink_bucket_id,
                MAIN_TOKEN_TREASURY_BUCKET_RESTRICTED_STARTER_CLAIM_LIVEOPS_POOL
            );
        }
        other => panic!("expected AgentClaimReleased, got {other:?}"),
    }
    assert_eq!(
        world.main_token_restricted_starter_claim_balance("alice"),
        0
    );
}

#[test]
fn expired_restricted_grants_follow_btreemap_account_order() {
    let mut world = setup_claim_world_with_treasury(1_000, 0, 0);
    allowlist_restricted_grant_admins(&mut world, &["liveops"]);

    for beneficiary_account_id in ["zara", "alice", "mike"] {
        world.submit_action(Action::IssueRestrictedStarterClaimGrant {
            issuer_account_id: "liveops".to_string(),
            beneficiary_account_id: beneficiary_account_id.to_string(),
            amount: 100,
            issuance_reason: "same_epoch_seed".to_string(),
            expires_at_epoch: 12,
        });
        world.step().expect("issue restricted grant");
    }

    let snapshot_before_prepare = world.snapshot();
    let journal_before_prepare = world.journal().clone();
    let prepared = world.prepared_restricted_starter_claim_grant_expiry_accounts_for_test(12);
    assert_eq!(world.snapshot(), snapshot_before_prepare);
    assert_eq!(world.journal(), &journal_before_prepare);
    assert_eq!(prepared, ["alice", "mike", "zara"]);

    let journal_len_before_expiry = world.journal().events.len();
    for _ in 0..12 {
        world.step().expect("advance to restricted grant expiry");
    }

    let expired_accounts = world.journal().events[journal_len_before_expiry..]
        .iter()
        .filter_map(|event| match &event.body {
            WorldEventBody::Domain(DomainEvent::RestrictedStarterClaimGrantExpired {
                beneficiary_account_id,
                ..
            }) => Some(beneficiary_account_id.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(expired_accounts, ["alice", "mike", "zara"]);
}

#[test]
fn revoked_restricted_grant_returns_spendable_balance_and_redirects_release_refund_to_treasury() {
    let mut world = setup_claim_world_with_treasury(1_000, 150, 0);
    allowlist_restricted_grant_admins(&mut world, &["liveops"]);

    world.submit_action(Action::IssueRestrictedStarterClaimGrant {
        issuer_account_id: "liveops".to_string(),
        beneficiary_account_id: "alice".to_string(),
        amount: 400,
        issuance_reason: "qa_seed".to_string(),
        expires_at_epoch: 10,
    });
    world.step().expect("issue restricted grant");

    world.submit_action(Action::ClaimAgent {
        claimer_agent_id: "alice".to_string(),
        target_agent_id: "bob".to_string(),
    });
    world.step().expect("claim with restricted grant");
    let claim = world.agent_claim("bob").expect("claim exists").clone();
    assert_eq!(
        world.main_token_restricted_starter_claim_balance("alice"),
        75
    );

    world.submit_action(Action::RevokeRestrictedStarterClaimGrant {
        issuer_account_id: "liveops".to_string(),
        beneficiary_account_id: "alice".to_string(),
        revoke_reason: "campaign_closed".to_string(),
    });
    world.step().expect("revoke restricted grant");

    let grant = world
        .restricted_starter_claim_grant("alice")
        .expect("revoked grant remains tracked");
    assert_eq!(grant.status, RestrictedStarterClaimGrantStatus::Revoked);
    assert_eq!(grant.status_reason.as_deref(), Some("campaign_closed"));
    assert_eq!(
        world.main_token_restricted_starter_claim_balance("alice"),
        0
    );

    world.submit_action(Action::ReleaseAgentClaim {
        claimer_agent_id: "alice".to_string(),
        target_agent_id: "bob".to_string(),
    });
    world.step().expect("request release after revoke");
    for _ in 0..claim.release_cooldown_epochs.saturating_sub(1) {
        world.step().expect("advance release cooldown after revoke");
    }
    world.step().expect("finalize release after revoke");

    let released_event = world
        .journal()
        .events
        .iter()
        .rev()
        .find(|event| {
            matches!(
                event.body,
                WorldEventBody::Domain(DomainEvent::AgentClaimReleased { .. })
            )
        })
        .expect("missing AgentClaimReleased event");

    match &released_event.body {
        WorldEventBody::Domain(DomainEvent::AgentClaimReleased {
            refunded_bond_restricted_amount,
            refunded_bond_restricted_sink,
            refunded_bond_restricted_sink_bucket_id,
            ..
        }) => {
            assert_eq!(*refunded_bond_restricted_amount, 200);
            assert_eq!(
                *refunded_bond_restricted_sink,
                RestrictedStarterClaimRefundSink::SourceTreasuryBucket
            );
            assert_eq!(
                refunded_bond_restricted_sink_bucket_id,
                MAIN_TOKEN_TREASURY_BUCKET_RESTRICTED_STARTER_CLAIM_LIVEOPS_POOL
            );
        }
        other => panic!("expected AgentClaimReleased, got {other:?}"),
    }
    assert_eq!(
        world.main_token_restricted_starter_claim_balance("alice"),
        0
    );
}

#[test]
fn restricted_grant_issue_rejects_when_admin_registry_is_missing() {
    let mut world = setup_claim_world_with_treasury(1_000, 0, 0);
    let journal_len_before = world.journal().events.len();

    world.submit_action(Action::IssueRestrictedStarterClaimGrant {
        issuer_account_id: "liveops".to_string(),
        beneficiary_account_id: "alice".to_string(),
        amount: 325,
        issuance_reason: "qa_seed".to_string(),
        expires_at_epoch: 10,
    });
    world.step().expect("reject issue without admin registry");

    let rejection = world.journal().events[journal_len_before..]
        .iter()
        .find_map(|event| match &event.body {
            WorldEventBody::Domain(DomainEvent::ActionRejected { reason, .. }) => Some(reason),
            _ => None,
        })
        .expect("missing admin registry rejection event");
    match rejection {
        RejectReason::RuleDenied { notes } => {
            assert!(
                notes
                    .iter()
                    .any(|note| note.contains("admin registry is not configured"))
            );
        }
        other => panic!("expected rule denied, got {other:?}"),
    }
    assert!(world.restricted_starter_claim_grant("alice").is_none());
}

#[test]
fn restricted_grant_issue_rejects_non_admin_issuer_before_grant_checks() {
    let mut world = setup_claim_world_with_treasury(1_000, 0, 0);
    allowlist_restricted_grant_admins(&mut world, &["liveops"]);
    let journal_len_before = world.journal().events.len();

    world.submit_action(Action::IssueRestrictedStarterClaimGrant {
        issuer_account_id: "qa".to_string(),
        beneficiary_account_id: "alice".to_string(),
        amount: 325,
        issuance_reason: "qa_seed".to_string(),
        expires_at_epoch: 5,
    });
    world.step().expect("reject non-admin issue");

    let rejection = world.journal().events[journal_len_before..]
        .iter()
        .find_map(|event| match &event.body {
            WorldEventBody::Domain(DomainEvent::ActionRejected { reason, .. }) => Some(reason),
            _ => None,
        })
        .expect("non-admin rejection event");
    match rejection {
        RejectReason::RuleDenied { notes } => {
            assert!(
                notes
                    .iter()
                    .any(|note| note.contains("not allowlisted admin"))
            );
        }
        other => panic!("expected rule denied, got {other:?}"),
    }
    assert!(world.restricted_starter_claim_grant("alice").is_none());
}

#[test]
fn restricted_grant_revoke_rejects_non_admin_before_issuer_match_checks() {
    let mut world = setup_claim_world_with_treasury(1_000, 0, 0);
    allowlist_restricted_grant_admins(&mut world, &["liveops"]);

    world.submit_action(Action::IssueRestrictedStarterClaimGrant {
        issuer_account_id: "liveops".to_string(),
        beneficiary_account_id: "alice".to_string(),
        amount: 325,
        issuance_reason: "qa_seed".to_string(),
        expires_at_epoch: 10,
    });
    world.step().expect("issue restricted grant");

    let journal_len_before = world.journal().events.len();
    world.submit_action(Action::RevokeRestrictedStarterClaimGrant {
        issuer_account_id: "qa".to_string(),
        beneficiary_account_id: "alice".to_string(),
        revoke_reason: "qa_window_closed".to_string(),
    });
    world.step().expect("reject non-admin revoke");

    let rejection = world.journal().events[journal_len_before..]
        .iter()
        .find_map(|event| match &event.body {
            WorldEventBody::Domain(DomainEvent::ActionRejected { reason, .. }) => Some(reason),
            _ => None,
        })
        .expect("non-admin revoke rejection event");
    match rejection {
        RejectReason::RuleDenied { notes } => {
            assert!(
                notes
                    .iter()
                    .any(|note| note.contains("not allowlisted admin"))
            );
            assert!(!notes.iter().any(|note| note.contains("issuer mismatch")));
        }
        other => panic!("expected rule denied, got {other:?}"),
    }
    assert_eq!(
        world
            .restricted_starter_claim_grant("alice")
            .expect("grant still tracked")
            .status,
        RestrictedStarterClaimGrantStatus::Issued
    );
}

#[test]
fn controller_registry_update_can_enable_restricted_grant_admin_before_issue() {
    let mut world = setup_claim_world_with_treasury(1_000, 0, 0);
    configure_restricted_grant_registry(&mut world, &["liveops"], &[]);

    world.submit_action(Action::UpdateRestrictedStarterClaimAdminRegistry {
        controller_account_id: "liveops".to_string(),
        next_admin_account_ids: vec!["liveops".to_string()],
    });
    world
        .step()
        .expect("enable liveops admin via controller registry update");

    world.submit_action(Action::IssueRestrictedStarterClaimGrant {
        issuer_account_id: "liveops".to_string(),
        beneficiary_account_id: "alice".to_string(),
        amount: 325,
        issuance_reason: "qa_seed".to_string(),
        expires_at_epoch: 10,
    });
    world.step().expect("issue grant after governance update");

    assert!(world.journal().events.iter().any(|event| {
        matches!(
            &event.body,
            WorldEventBody::Domain(DomainEvent::RestrictedStarterClaimGrantIssued {
                beneficiary_account_id,
                issuer_id,
                amount,
                ..
            }) if beneficiary_account_id == "alice"
                && issuer_id == "liveops"
                && *amount == 325
        )
    }));

    assert_eq!(
        world
            .governance_main_token_controller_registry()
            .expect("registry")
            .restricted_starter_claim_admin_account_ids
            .iter()
            .cloned()
            .collect::<Vec<_>>(),
        vec!["liveops".to_string()]
    );
    assert_eq!(
        world.main_token_restricted_starter_claim_balance("alice"),
        325
    );
}
