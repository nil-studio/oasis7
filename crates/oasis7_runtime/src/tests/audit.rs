use super::super::*;
use super::pos;
use oasis7_agent_api::world_action::ResourceKind;
use serde_json::json;
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn audit_filter_by_kind_and_cause() {
    let mut world = World::new();
    world.add_capability(CapabilityGrant::allow_all("cap_all"));
    world.set_policy(PolicySet::allow_all());

    let intent_id = world
        .emit_effect(
            "http.request",
            json!({ "url": "https://example.com" }),
            "cap_all",
            EffectOrigin::System,
        )
        .unwrap();

    let intent = world.take_next_effect().unwrap();
    assert_eq!(intent.intent_id, intent_id);

    let receipt = EffectReceipt {
        birth_era: None,
        world: None,
        intent_payload_digest: None,
        intent_id: intent_id.clone(),
        status: "ok".to_string(),
        payload: json!({ "status": 200 }),
        cost_cents: None,
        signature: None,
    };
    world.ingest_receipt(receipt).unwrap();

    let filter = AuditFilter {
        kinds: Some(vec![AuditEventKind::ReceiptAppended]),
        caused_by: Some(AuditCausedBy::Effect),
        ..AuditFilter::default()
    };
    let events = world.audit_events(&filter);
    assert_eq!(events.len(), 1);
    assert!(matches!(events[0].caused_by, Some(CausedBy::Effect { .. })));
}

#[test]
fn audit_filter_rule_decision_events() {
    let mut world = World::new();
    let action_id = 42;
    let original_action = Action::RegisterAgent {
        agent_id: "agent-1".to_string(),
        pos: pos(0, 0),
    };
    let override_action = Action::RegisterAgent {
        agent_id: "agent-1".to_string(),
        pos: pos(1, 1),
    };
    let mut cost = ResourceDelta::default();
    cost.entries.insert(ResourceKind::Electricity, -3);

    world
        .record_rule_decision(
            RuleDecisionRecord {
                action_id,
                module_id: "rule.module".to_string(),
                stage: ModuleSubscriptionStage::PreAction,
                verdict: RuleVerdict::Modify,
                override_action: Some(override_action.clone()),
                cost,
                notes: vec!["override".to_string()],
            },
            Some(CausedBy::Action(action_id)),
        )
        .unwrap();
    world
        .record_action_override(
            ActionOverrideRecord {
                action_id,
                original_action,
                override_action,
            },
            Some(CausedBy::Action(action_id)),
        )
        .unwrap();

    let rule_events = world.audit_events(&AuditFilter {
        kinds: Some(vec![AuditEventKind::RuleDecision]),
        ..AuditFilter::default()
    });
    assert_eq!(rule_events.len(), 1);
    assert!(matches!(
        rule_events[0].body,
        WorldEventBody::RuleDecisionRecorded(_)
    ));

    let override_events = world.audit_events(&AuditFilter {
        kinds: Some(vec![AuditEventKind::ActionOverridden]),
        ..AuditFilter::default()
    });
    assert_eq!(override_events.len(), 1);
    assert!(matches!(
        override_events[0].body,
        WorldEventBody::ActionOverridden(_)
    ));
}

#[test]
fn audit_log_export_writes_file() {
    let mut world = World::new();
    world.submit_action(Action::RegisterAgent {
        agent_id: "agent-1".to_string(),
        pos: pos(0, 0),
    });
    world.step().unwrap();

    let dir = std::env::temp_dir().join(format!(
        "oasis7-audit-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("audit.json");

    world
        .save_audit_log(&path, &AuditFilter::default())
        .unwrap();
    let events: Vec<WorldEvent> = util::read_json_from_path(&path).unwrap();
    assert!(!events.is_empty());

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn audit_filter_governance_events() {
    let mut world = World::new();
    let manifest = Manifest {
        version: 2,
        content: json!({ "name": "audit" }),
    };

    let proposal_id = world.propose_manifest_update(manifest, "alice").unwrap();
    world.shadow_proposal(proposal_id).unwrap();
    world
        .approve_proposal(proposal_id, "bob", ProposalDecision::Approve)
        .unwrap();
    world.apply_proposal(proposal_id).unwrap();

    let governance_events = world.audit_events(&AuditFilter {
        kinds: Some(vec![AuditEventKind::Governance]),
        ..AuditFilter::default()
    });
    assert_eq!(governance_events.len(), 5);
    assert!(matches!(
        governance_events[0].body,
        WorldEventBody::Governance(GovernanceEvent::Proposed { .. })
    ));
    assert!(matches!(
        governance_events[1].body,
        WorldEventBody::Governance(GovernanceEvent::ShadowReport { .. })
    ));
    assert!(matches!(
        governance_events[2].body,
        WorldEventBody::Governance(GovernanceEvent::Approved { .. })
    ));
    assert!(matches!(
        governance_events[3].body,
        WorldEventBody::Governance(GovernanceEvent::Queued { .. })
    ));
    assert!(matches!(
        governance_events[4].body,
        WorldEventBody::Governance(GovernanceEvent::Applied { .. })
    ));

    let manifest_events = world.audit_events(&AuditFilter {
        kinds: Some(vec![AuditEventKind::ManifestUpdated]),
        ..AuditFilter::default()
    });
    assert_eq!(manifest_events.len(), 1);
    assert!(matches!(
        manifest_events[0].body,
        WorldEventBody::ManifestUpdated(_)
    ));
}
