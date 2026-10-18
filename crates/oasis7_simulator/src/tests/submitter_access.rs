use super::*;

#[test]
fn player_submitter_cannot_submit_world_actions_directly() {
    let mut kernel = WorldKernel::new();
    kernel.submit_action_from_player(
        "player-a",
        Action::RegisterLocation {
            location_id: "loc-1".to_string(),
            name: "base".to_string(),
            pos: pos(0, 0),
            profile: LocationProfile::default(),
        },
    );
    let event = kernel.step().expect("event");
    match event.kind {
        WorldEventKind::ActionRejected { reason } => match reason {
            RejectReason::RuleDenied { notes } => {
                assert!(
                    notes
                        .iter()
                        .any(|note| note.contains("cannot submit world actions directly")),
                    "missing rejection note: {notes:?}"
                );
            }
            other => panic!("unexpected reject reason: {other:?}"),
        },
        other => panic!("unexpected event: {other:?}"),
    }
    assert!(!kernel.model().locations.contains_key("loc-1"));
}

#[test]
fn agent_submitter_rejects_mismatched_action_agent_id() {
    let mut kernel = WorldKernel::new();
    kernel.submit_action_from_agent(
        "agent-1",
        Action::MoveAgent {
            agent_id: "agent-2".to_string(),
            to: "loc-1".to_string(),
        },
    );
    let event = kernel.step().expect("event");
    match event.kind {
        WorldEventKind::ActionRejected {
            reason: RejectReason::RuleDenied { notes },
        } => {
            assert!(
                notes
                    .iter()
                    .any(|note| note.contains("action agent_id mismatch")),
                "missing mismatch rejection note: {notes:?}"
            );
        }
        other => panic!("unexpected event: {other:?}"),
    }
}

#[test]
fn agent_submitter_rejects_non_agent_owner_actions() {
    let mut kernel = WorldKernel::new();
    kernel.submit_action_from_agent(
        "agent-1",
        Action::TransferResource {
            from: ResourceOwner::Location {
                location_id: "loc-1".to_string(),
            },
            to: ResourceOwner::Agent {
                agent_id: "agent-1".to_string(),
            },
            kind: ResourceKind::Electricity,
            amount: 1,
        },
    );
    let event = kernel.step().expect("event");
    match event.kind {
        WorldEventKind::ActionRejected {
            reason: RejectReason::RuleDenied { notes },
        } => {
            assert!(
                notes
                    .iter()
                    .any(|note| note.contains("from owner must be the submitter agent")),
                "missing owner rejection note: {notes:?}"
            );
        }
        other => panic!("unexpected event: {other:?}"),
    }
}

#[test]
fn agent_submitter_rejects_mismatched_gameplay_actor() {
    let mut kernel = WorldKernel::new();
    kernel.submit_action_from_agent(
        "agent-1",
        Action::CastGovernanceVote {
            voter_agent_id: "agent-2".to_string(),
            proposal_key: "proposal.alpha".to_string(),
            option: "approve".to_string(),
            weight: 1,
        },
    );
    let event = kernel.step().expect("event");
    match event.kind {
        WorldEventKind::ActionRejected {
            reason: RejectReason::RuleDenied { notes },
        } => {
            assert!(
                notes
                    .iter()
                    .any(|note| note.contains("voter_agent_id must be the submitter agent")),
                "missing gameplay actor rejection note: {notes:?}"
            );
        }
        other => panic!("unexpected event: {other:?}"),
    }
}

#[test]
fn agent_submitter_rejects_mismatched_economic_contract_actor() {
    let mut kernel = WorldKernel::new();
    kernel.submit_action_from_agent(
        "agent-1",
        Action::OpenEconomicContract {
            creator_agent_id: "agent-2".to_string(),
            contract_id: "contract.alpha".to_string(),
            counterparty_agent_id: "agent-3".to_string(),
            settlement_kind: ResourceKind::Data,
            settlement_amount: 5,
            reputation_stake: 2,
            expires_at: 20,
            description: "contract".to_string(),
        },
    );
    let event = kernel.step().expect("event");
    match event.kind {
        WorldEventKind::ActionRejected {
            reason: RejectReason::RuleDenied { notes },
        } => {
            assert!(
                notes
                    .iter()
                    .any(|note| note.contains("creator_agent_id must be the submitter agent")),
                "missing economic actor rejection note: {notes:?}"
            );
        }
        other => panic!("unexpected event: {other:?}"),
    }
}
