use super::super::*;
use super::pos;
use crate::agent_state::BodyKernelView;
use oasis7_agent_api::world_action::ResourceKind;
use oasis7_wasm_abi::{
    ModuleCallErrorCode, ModuleCallFailure, ModuleCallRequest, ModuleEffectIntent, ModuleOutput,
    ModuleSandbox,
};

#[derive(Default)]
struct NoopSandbox;

impl ModuleSandbox for NoopSandbox {
    fn call(&mut self, _request: &ModuleCallRequest) -> Result<ModuleOutput, ModuleCallFailure> {
        Ok(ModuleOutput {
            new_state: None,
            effects: Vec::new(),
            emits: Vec::new(),
            tick_lifecycle: None,
            output_bytes: 0,
        })
    }
}

#[derive(Default)]
struct StateWritingThenFailingSandbox {
    calls: usize,
}

impl ModuleSandbox for StateWritingThenFailingSandbox {
    fn call(&mut self, request: &ModuleCallRequest) -> Result<ModuleOutput, ModuleCallFailure> {
        self.calls = self.calls.saturating_add(1);
        if self.calls > 1 {
            return Err(ModuleCallFailure {
                module_id: request.module_id.clone(),
                trace_id: request.trace_id.clone(),
                code: ModuleCallErrorCode::Trap,
                detail: "failure injected after staged module state write".to_string(),
            });
        }
        Ok(ModuleOutput {
            new_state: Some(b"state-written-before-later-failure".to_vec()),
            effects: Vec::new(),
            emits: Vec::new(),
            tick_lifecycle: None,
            output_bytes: 34,
        })
    }
}

fn world_ready_for_overflowing_transfer() -> World {
    let mut world = World::new();
    world.submit_action(Action::RegisterAgent {
        agent_id: "from".to_string(),
        pos: pos(0, 0),
    });
    world.submit_action(Action::RegisterAgent {
        agent_id: "to".to_string(),
        pos: pos(0, 0),
    });
    world.step().expect("register agents");
    world
        .set_agent_resource_balance("from", ResourceKind::Data, 10)
        .expect("seed source balance");
    world
        .set_agent_resource_balance("to", ResourceKind::Data, i64::MAX)
        .expect("seed target overflow boundary");
    world.submit_action(Action::GrantDataAccess {
        owner_agent_id: "from".to_string(),
        grantee_agent_id: "to".to_string(),
    });
    world.step().expect("grant transfer access");
    world
}

fn submit_overflowing_transfer(world: &mut World) {
    world.submit_action(Action::EmitResourceTransfer {
        from_agent_id: "from".to_string(),
        to_agent_id: "to".to_string(),
        kind: ResourceKind::Data,
        amount: 1,
    });
}

fn world_with_overflowing_transfer_pending() -> World {
    let mut world = world_ready_for_overflowing_transfer();
    submit_overflowing_transfer(&mut world);
    world
}

fn assert_failed_transition_is_unpublished(
    world: &World,
    snapshot_before: &Snapshot,
    journal_before: &Journal,
) {
    assert_eq!(
        world.snapshot(),
        *snapshot_before,
        "failed transition changed snapshot state"
    );
    assert_eq!(
        world.journal(),
        journal_before,
        "failed transition changed journal"
    );
    assert_eq!(
        world.pending_actions_len(),
        1,
        "failed transition consumed its pending action"
    );
}

fn activate_post_move_state_writer(world: &mut World) {
    world.set_policy(PolicySet::allow_all());
    let wasm_bytes = b"transaction-regression-state-writer";
    let wasm_hash = util::sha256_hex(wasm_bytes);
    world
        .register_module_artifact(wasm_hash.clone(), wasm_bytes)
        .expect("register state-writer artifact");
    super::modules::activate_module_manifest(
        world,
        ModuleManifest {
            module_id: "m.transaction-regression.state-writer".to_string(),
            name: "TransactionRegressionStateWriter".to_string(),
            version: "0.1.0".to_string(),
            kind: ModuleKind::Reducer,
            role: ModuleRole::Domain,
            wasm_hash: wasm_hash.clone(),
            interface_version: "wasm-1".to_string(),
            abi_contract: ModuleAbiContract::default(),
            exports: vec!["reduce".to_string()],
            subscriptions: vec![ModuleSubscription {
                event_kinds: Vec::new(),
                action_kinds: vec!["action.move_agent".to_string()],
                stage: Some(ModuleSubscriptionStage::PostAction),
                filters: None,
            }],
            required_caps: Vec::new(),
            artifact_identity: Some(super::signed_test_artifact_identity(wasm_hash.as_str())),
            limits: ModuleLimits {
                max_mem_bytes: 1024,
                max_gas: 10_000,
                max_call_rate: 1,
                max_output_bytes: 1024,
                max_effects: 0,
                max_emits: 0,
            },
        },
    );
}

#[test]
fn failed_step_does_not_publish_partial_world_mutations() {
    let mut world = world_with_overflowing_transfer_pending();
    let snapshot_before = world.snapshot();
    let journal_before = world.journal().clone();

    let error = world.step().expect_err("overflowing transfer must fail");
    assert!(matches!(error, WorldError::ResourceBalanceInvalid { .. }));

    assert_failed_transition_is_unpublished(&world, &snapshot_before, &journal_before);
}

#[test]
fn failed_step_with_modules_does_not_publish_partial_world_mutations() {
    let mut world = world_with_overflowing_transfer_pending();
    let snapshot_before = world.snapshot();
    let journal_before = world.journal().clone();
    let mut sandbox = NoopSandbox;

    let error = world
        .step_with_modules(&mut sandbox)
        .expect_err("overflowing transfer must fail");
    assert!(matches!(error, WorldError::ResourceBalanceInvalid { .. }));

    assert_failed_transition_is_unpublished(&world, &snapshot_before, &journal_before);
}

#[test]
fn failed_step_with_modules_rolls_back_prior_module_state_write() {
    let mut world = world_ready_for_overflowing_transfer();
    activate_post_move_state_writer(&mut world);
    world.submit_action(Action::MoveAgent {
        agent_id: "from".to_string(),
        to: pos(1, 0),
    });
    world.submit_action(Action::MoveAgent {
        agent_id: "from".to_string(),
        to: pos(2, 0),
    });
    let snapshot_before = world.snapshot();
    let journal_before = world.journal().clone();
    let pending_before = world.pending_actions_len();
    let mut sandbox = StateWritingThenFailingSandbox::default();

    world
        .step_with_modules(&mut sandbox)
        .expect_err("second module call must fail after first state write");
    assert_eq!(
        sandbox.calls, 2,
        "one state-writing call must precede the injected failure"
    );
    assert_eq!(world.snapshot(), snapshot_before);
    assert_eq!(world.journal(), &journal_before);
    assert_eq!(world.pending_actions_len(), pending_before);
}

#[test]
fn failed_committed_context_step_does_not_publish_partial_world_mutations() {
    let mut world = world_with_overflowing_transfer_pending();
    let snapshot_before = world.snapshot();
    let journal_before = world.journal().clone();
    let mut sandbox = NoopSandbox;
    let committed_height = world.state().time.saturating_add(1);
    let context = RuntimeCommittedTickContext {
        height: committed_height,
        slot: committed_height.saturating_sub(1),
        epoch: 0,
        node_block_hash: "block.transaction-regression".to_string(),
        action_root: "action-root.transaction-regression".to_string(),
        authority_node_id: "builtin.module.release.signer".to_string(),
        committed_at_unix_ms: 0,
    };

    let error = world
        .step_with_modules_for_committed_context(&mut sandbox, &context)
        .expect_err("overflowing transfer must fail");
    assert!(matches!(error, WorldError::ResourceBalanceInvalid { .. }));

    assert_failed_transition_is_unpublished(&world, &snapshot_before, &journal_before);
}

#[test]
fn append_event_failure_after_reducer_does_not_publish_any_observable_delta() {
    let mut world = World::new();
    world.submit_action(Action::RegisterAgent {
        agent_id: "append-event-regression-agent".to_string(),
        pos: pos(0, 0),
    });
    world
        .step()
        .expect("register append-event regression agent");

    let snapshot_before = world.snapshot();
    let journal_before = world.journal().clone();
    let consensus_before = world.tick_consensus_records().to_vec();
    let view = BodyKernelView {
        mass_kg: 120,
        radius_cm: 80,
        thrust_limit: 200,
        cross_section_cm2: 4_000,
    };

    // This test-only hook must fire after apply_event_body_at has mutated the
    // body reducer, but before append_event can publish its id/journal/
    // consensus delta.  The hook is intentionally absent until the runtime
    // transaction migration implements this RED contract.
    world.fail_next_append_after_reducer_for_test();
    world
        .record_body_attributes_update(
            "append-event-regression-agent",
            view,
            "late-failure-regression",
            None,
        )
        .expect_err("injected append_event late failure must abort the transition");

    let snapshot_after = world.snapshot();
    assert_eq!(
        snapshot_after, snapshot_before,
        "late append_event failure published reducer or sequence state"
    );
    assert_eq!(
        snapshot_after.last_event_id, snapshot_before.last_event_id,
        "late append_event failure consumed an event id"
    );
    assert_eq!(
        snapshot_after.event_id_era, snapshot_before.event_id_era,
        "late append_event failure changed event id era"
    );
    assert_eq!(
        world.journal(),
        &journal_before,
        "late append_event failure published a journal event"
    );
    assert_eq!(
        world.tick_consensus_records(),
        consensus_before.as_slice(),
        "late append_event failure changed consensus records"
    );
}

#[test]
fn append_event_failure_after_publication_prepare_does_not_publish_any_observable_delta() {
    let mut world = World::new();
    world.submit_action(Action::RegisterAgent {
        agent_id: "append-publication-regression-agent".to_string(),
        pos: pos(0, 0),
    });
    world
        .step()
        .expect("register append-publication regression agent");

    let snapshot_before = world.snapshot();
    let journal_before = world.journal().clone();
    let consensus_before = world.tick_consensus_records().to_vec();
    let rejection_audit_before = world.tick_consensus_rejection_audit_events().to_vec();
    let view = BodyKernelView {
        mass_kg: 120,
        radius_cm: 80,
        thrust_limit: 200,
        cross_section_cm2: 4_000,
    };

    // This test-only hook is deliberately placed after reducer delta,
    // event envelope/ID+era, journal/limit/backpressure, and consensus
    // candidate preparation, but before any canonical installation.  The
    // hook is intentionally absent until the publication seam is migrated.
    world.fail_next_append_after_publication_prepare_for_test();
    world
        .record_body_attributes_update(
            "append-publication-regression-agent",
            view,
            "late-publication-failure-regression",
            None,
        )
        .expect_err("injected publication-preparation failure must abort the transition");

    assert_eq!(
        world.snapshot(),
        snapshot_before,
        "publication-preparation failure changed the full snapshot"
    );
    assert_eq!(
        world.snapshot().last_event_id,
        snapshot_before.last_event_id,
        "publication-preparation failure consumed an event id"
    );
    assert_eq!(
        world.snapshot().event_id_era,
        snapshot_before.event_id_era,
        "publication-preparation failure changed event id era"
    );
    assert_eq!(
        world.journal(),
        &journal_before,
        "publication-preparation failure published a journal event"
    );
    assert_eq!(
        world.tick_consensus_records(),
        consensus_before.as_slice(),
        "publication-preparation failure changed consensus records"
    );
    assert_eq!(
        world.tick_consensus_rejection_audit_events(),
        rejection_audit_before.as_slice(),
        "publication-preparation failure changed consensus rejection audit"
    );
}

#[test]
fn rule_decision_publication_failure_after_prepare_is_unpublished() {
    let mut world = World::new();
    let snapshot_before = world.snapshot();
    let journal_before = world.journal().clone();
    let consensus_before = world.tick_consensus_records().to_vec();
    let rejection_audit_before = world.tick_consensus_rejection_audit_events().to_vec();

    // RuleDecisionRecorded is a no-state audit event that still publishes an
    // event id, journal entry, and tick-consensus record. The public recorder
    // must use the explicit no-state prepared path so the publication-prepare
    // failpoint aborts before any of those observable deltas install.
    world.fail_next_append_after_publication_prepare_for_test();
    let error = world
        .record_rule_decision(
            RuleDecisionRecord {
                action_id: 7,
                module_id: "rule.transaction-regression".to_string(),
                stage: ModuleSubscriptionStage::PreAction,
                verdict: RuleVerdict::Allow,
                override_action: None,
                cost: ResourceDelta::default(),
                notes: vec!["publication-failure-regression".to_string()],
            },
            None,
        )
        .expect_err("publication-preparation failure must abort rule decision publication");
    assert!(matches!(
        error,
        WorldError::ResourceBalanceInvalid { ref reason }
            if reason.contains("publication preparation")
    ));

    assert_eq!(world.snapshot(), snapshot_before);
    assert_eq!(world.journal(), &journal_before);
    assert_eq!(world.tick_consensus_records(), consensus_before.as_slice());
    assert_eq!(
        world.tick_consensus_rejection_audit_events(),
        rejection_audit_before.as_slice()
    );
}

#[test]
fn effect_publication_failure_after_prepare_is_fully_unpublished_and_retry_reuses_ids() {
    let mut world = World::new();
    let mut initial_snapshot = world.snapshot();
    initial_snapshot.next_intent_id = u64::MAX;
    initial_snapshot.intent_id_era = 7;
    world = World::from_snapshot(initial_snapshot, world.journal().clone())
        .expect("restore effect sequence boundary");
    world = world.with_runtime_memory_limits(WorldRuntimeMemoryLimits {
        max_pending_effects: 1,
        ..WorldRuntimeMemoryLimits::default()
    });
    world.add_capability(CapabilityGrant::allow_all("cap_all"));
    world.set_policy(PolicySet::allow_all());

    let seed_intent_id = world
        .emit_effect(
            "http.request",
            serde_json::json!({"url": "https://example.com/seed"}),
            "cap_all",
            EffectOrigin::System,
        )
        .expect("seed effect");
    assert_eq!(seed_intent_id, "intent-18446744073709551615");
    let snapshot_before = world.snapshot();
    let journal_before = world.journal().clone();
    let consensus_before = world.tick_consensus_records().to_vec();
    let rejection_audit_before = world.tick_consensus_rejection_audit_events().to_vec();
    let backpressure_before = world.runtime_backpressure_stats().clone();

    // This is deliberately a public effect call.  The legacy path allocates
    // the intent sequence and appends the policy/EffectQueued events directly;
    // it therefore bypasses the existing post-publication-prepare failpoint
    // and/or leaves a sequence gap.  The migrated path must stage the complete
    // public operation, including its policy audit and queue admission.
    world.fail_next_append_after_publication_prepare_for_test();
    let error = world
        .emit_effect(
            "http.request",
            serde_json::json!({"url": "https://example.com/retry"}),
            "cap_all",
            EffectOrigin::System,
        )
        .expect_err("publication-preparation failure must abort effect publication");
    assert!(matches!(
        error,
        WorldError::ResourceBalanceInvalid { ref reason }
            if reason.contains("publication preparation")
    ));

    assert_eq!(world.snapshot(), snapshot_before);
    assert_eq!(
        world.snapshot().next_intent_id,
        snapshot_before.next_intent_id
    );
    assert_eq!(
        world.snapshot().intent_id_era,
        snapshot_before.intent_id_era
    );
    assert_eq!(
        world.snapshot().last_event_id,
        snapshot_before.last_event_id
    );
    assert_eq!(world.snapshot().event_id_era, snapshot_before.event_id_era);
    assert_eq!(
        world.snapshot().pending_effects,
        snapshot_before.pending_effects
    );
    assert_eq!(world.journal(), &journal_before);
    assert_eq!(world.tick_consensus_records(), consensus_before.as_slice());
    assert_eq!(
        world.tick_consensus_rejection_audit_events(),
        rejection_audit_before.as_slice()
    );
    assert_eq!(world.runtime_backpressure_stats(), &backpressure_before);

    // A failed attempt must not consume the intent sequence or event id.  A
    // control world makes the expected successful IDs independent of how many
    // audit events the public API emits internally.
    let mut expected_world = World::from_snapshot(snapshot_before.clone(), journal_before.clone())
        .expect("restore effect retry control");
    let expected_intent_id = expected_world
        .emit_effect(
            "http.request",
            serde_json::json!({"url": "https://example.com/retry"}),
            "cap_all",
            EffectOrigin::System,
        )
        .expect("control effect");

    let retry_intent_id = world
        .emit_effect(
            "http.request",
            serde_json::json!({"url": "https://example.com/retry"}),
            "cap_all",
            EffectOrigin::System,
        )
        .expect("retry effect after one-shot failpoint");
    assert_eq!(retry_intent_id, expected_intent_id);
    assert_eq!(world.snapshot(), expected_world.snapshot());
    assert_eq!(world.journal(), expected_world.journal());
    assert_eq!(
        world.tick_consensus_records(),
        expected_world.tick_consensus_records()
    );
    assert_eq!(
        world.tick_consensus_rejection_audit_events(),
        expected_world.tick_consensus_rejection_audit_events()
    );
    assert_eq!(
        world.runtime_backpressure_stats(),
        expected_world.runtime_backpressure_stats()
    );

    let effect_event = world
        .journal()
        .events
        .iter()
        .rev()
        .find(|event| matches!(event.body, WorldEventBody::EffectQueued(_)))
        .expect("retry must publish EffectQueued");
    let expected_effect_event = expected_world
        .journal()
        .events
        .iter()
        .rev()
        .find(|event| matches!(event.body, WorldEventBody::EffectQueued(_)))
        .expect("control must publish EffectQueued");
    assert_eq!(effect_event.id, expected_effect_event.id);
    assert_eq!(effect_event.id, world.snapshot().last_event_id);
    match &effect_event.body {
        WorldEventBody::EffectQueued(intent) => assert_eq!(intent.intent_id, retry_intent_id),
        other => panic!("unexpected retry event: {other:?}"),
    }
    assert_eq!(world.pending_effects_len(), 1);

    let replayed = World::from_snapshot(snapshot_before, world.journal().clone())
        .expect("replay successful effect publication");
    assert_eq!(replayed.state(), world.state());
    assert_eq!(replayed.journal(), world.journal());
    assert_eq!(
        replayed.tick_consensus_records(),
        world.tick_consensus_records(),
        "effect replay must rebuild the same consensus records"
    );
    assert_eq!(
        replayed.runtime_backpressure_stats(),
        world.runtime_backpressure_stats(),
        "effect replay must rebuild deterministic queue pressure"
    );
}

#[test]
fn public_ingest_receipt_post_prepare_failure_is_fully_unpublished_and_retry_reuses_ids() {
    let mut world = World::new().with_runtime_memory_limits(WorldRuntimeMemoryLimits {
        max_pending_effects: 1,
        max_inflight_effects: 1,
        max_journal_events: 4,
        ..WorldRuntimeMemoryLimits::default()
    });
    world.add_capability(CapabilityGrant::allow_all("cap_all"));
    world.set_policy(PolicySet::allow_all());

    let intent_id = world
        .emit_effect(
            "http.request",
            serde_json::json!({"url": "https://example.com/receipt"}),
            "cap_all",
            EffectOrigin::System,
        )
        .expect("seed effect");
    let intent = world
        .take_next_effect()
        .expect("dispatch effect to inflight");
    assert_eq!(intent.intent_id, intent_id);

    let snapshot_before = world.snapshot();
    let journal_before = world.journal().clone();
    let consensus_before = world.tick_consensus_records().to_vec();
    let rejection_audit_before = world.tick_consensus_rejection_audit_events().to_vec();
    let root_before = world.capability_authorization_root().to_string();
    let backpressure_before = world.runtime_backpressure_stats().clone();
    let receipt = EffectReceipt {
        birth_era: None,
        world: None,
        intent_payload_digest: None,
        intent_id: intent_id.clone(),
        status: "ok".to_string(),
        payload: serde_json::json!({"status": 200}),
        cost_cents: Some(5),
        signature: None,
    };
    // A test-only clone is the exact live control. Restoring the bounded
    // journal through `from_snapshot` also performs recovery normalization,
    // which is outside this receipt retry assertion.
    let mut expected_world = world.clone();

    // The public receipt path must stage both the authorization closure (when
    // present) and ReceiptAppended, then fail before installing any delta.
    // This currently exposes the unmigrated legacy append path: the failpoint
    // is ignored and ingest_receipt succeeds instead of returning the injected
    // publication-preparation error.
    world.fail_next_append_after_publication_prepare_for_test();
    let error = world
        .ingest_receipt(receipt.clone())
        .expect_err("post-prepare failure must abort public receipt ingestion");
    assert!(matches!(
        error,
        WorldError::ResourceBalanceInvalid { ref reason }
            if reason.contains("publication preparation")
    ));

    assert_eq!(world.snapshot(), snapshot_before);
    assert_eq!(
        world.snapshot().last_event_id,
        snapshot_before.last_event_id
    );
    assert_eq!(world.snapshot().event_id_era, snapshot_before.event_id_era);
    assert_eq!(
        world.snapshot().pending_effects,
        snapshot_before.pending_effects
    );
    assert_eq!(
        world.snapshot().inflight_effects,
        snapshot_before.inflight_effects
    );
    assert_eq!(world.journal(), &journal_before);
    assert_eq!(world.capability_authorization_root(), root_before);
    assert_eq!(world.tick_consensus_records(), consensus_before.as_slice());
    assert_eq!(
        world.tick_consensus_rejection_audit_events(),
        rejection_audit_before.as_slice()
    );
    assert_eq!(world.runtime_backpressure_stats(), &backpressure_before);

    expected_world
        .ingest_receipt(receipt.clone())
        .expect("control receipt ingestion");
    world
        .ingest_receipt(receipt)
        .expect("retry receipt after one-shot failpoint");
    assert_eq!(world.snapshot(), expected_world.snapshot());
    assert_eq!(world.journal(), expected_world.journal());
    assert_eq!(
        world.tick_consensus_records(),
        expected_world.tick_consensus_records()
    );
    assert_eq!(
        world.tick_consensus_rejection_audit_events(),
        expected_world.tick_consensus_rejection_audit_events()
    );
    assert_eq!(
        world.runtime_backpressure_stats(),
        expected_world.runtime_backpressure_stats()
    );

    let replayed = World::from_snapshot(snapshot_before, world.journal().clone())
        .expect("replay receipt ingestion");
    assert_eq!(replayed.state(), world.state());
    assert_eq!(replayed.pending_effects_len(), world.pending_effects_len());
    assert_eq!(
        replayed.snapshot().inflight_effects,
        world.snapshot().inflight_effects
    );
    assert_eq!(
        replayed.capability_authorization_root(),
        world.capability_authorization_root()
    );
    assert_eq!(
        replayed.snapshot().last_event_id,
        world.snapshot().last_event_id
    );
    assert_eq!(
        replayed.snapshot().event_id_era,
        world.snapshot().event_id_era
    );
    assert_eq!(replayed.journal(), world.journal());
    assert_eq!(
        replayed.tick_consensus_records(),
        world.tick_consensus_records()
    );
}

#[test]
fn effect_capability_preflight_failures_are_unpublished_and_precede_policy_or_queue() {
    for (case, cap_ref) in [
        ("missing", "cap.missing"),
        ("expired", "cap.expired"),
        ("not_allowed", "cap.narrow"),
    ] {
        let mut world = World::new().with_runtime_memory_limits(WorldRuntimeMemoryLimits {
            max_pending_effects: 1,
            ..WorldRuntimeMemoryLimits::default()
        });
        world.add_capability(CapabilityGrant::allow_all("cap.seed"));
        world.set_policy(PolicySet::allow_all());
        world
            .emit_effect(
                "http.request",
                serde_json::json!({"url": "https://example.com/seed"}),
                "cap.seed",
                EffectOrigin::System,
            )
            .expect("seed effect fills the bounded queue");

        match case {
            "expired" => {
                world.add_capability(CapabilityGrant {
                    name: cap_ref.to_string(),
                    effect_kinds: vec!["*".to_string()],
                    expiry: Some(0),
                });
                world
                    .step()
                    .expect("advance logical time for expired-capability fixture");
            }
            "not_allowed" => world.add_capability(CapabilityGrant::new(
                cap_ref,
                vec!["other.effect".to_string()],
            )),
            "missing" => {}
            _ => unreachable!("unknown capability preflight case"),
        }

        // A policy deny and a full queue must not mask capability admission
        // errors.  Neither policy evaluation nor queue preparation may run
        // before this preflight completes.
        world.set_policy(PolicySet {
            rules: vec![PolicyRule {
                when: PolicyWhen {
                    effect_kind: None,
                    origin_kind: None,
                    cap_name: None,
                },
                decision: PolicyDecision::Deny {
                    reason: "policy-must-not-win".to_string(),
                },
            }],
        });
        let snapshot_before = world.snapshot();
        let journal_before = world.journal().clone();
        let consensus_before = world.tick_consensus_records().to_vec();
        let rejection_audit_before = world.tick_consensus_rejection_audit_events().to_vec();
        let backpressure_before = world.runtime_backpressure_stats().clone();

        let error = world
            .emit_effect(
                "http.request",
                serde_json::json!({"url": "https://example.com/rejected"}),
                cap_ref,
                EffectOrigin::System,
            )
            .expect_err("capability preflight must reject the effect");
        match case {
            "missing" => assert!(matches!(
                error,
                WorldError::CapabilityMissing { cap_ref: ref actual }
                    if actual == cap_ref
            )),
            "expired" => assert!(matches!(
                error,
                WorldError::CapabilityExpired { cap_ref: ref actual }
                    if actual == cap_ref
            )),
            "not_allowed" => assert!(matches!(
                error,
                WorldError::CapabilityNotAllowed {
                    cap_ref: ref actual_cap,
                    ref kind,
                } if actual_cap == cap_ref && kind == "http.request"
            )),
            _ => unreachable!("unknown capability preflight case"),
        }
        assert_eq!(world.snapshot(), snapshot_before);
        assert_eq!(
            world.snapshot().next_intent_id,
            snapshot_before.next_intent_id
        );
        assert_eq!(
            world.snapshot().intent_id_era,
            snapshot_before.intent_id_era
        );
        assert_eq!(
            world.snapshot().last_event_id,
            snapshot_before.last_event_id
        );
        assert_eq!(world.snapshot().event_id_era, snapshot_before.event_id_era);
        assert_eq!(world.journal(), &journal_before);
        assert_eq!(world.tick_consensus_records(), consensus_before.as_slice());
        assert_eq!(
            world.tick_consensus_rejection_audit_events(),
            rejection_audit_before.as_slice()
        );
        assert_eq!(world.runtime_backpressure_stats(), &backpressure_before);
    }
}

#[test]
fn deterministic_effect_policy_deny_commits_one_audit_event_and_replays_sequences() {
    let mut world = World::new();
    world.add_capability(CapabilityGrant::allow_all("cap_all"));
    world.set_policy(PolicySet {
        rules: vec![PolicyRule {
            when: PolicyWhen {
                effect_kind: None,
                origin_kind: None,
                cap_name: None,
            },
            decision: PolicyDecision::Deny {
                reason: "deterministic-effect-deny".to_string(),
            },
        }],
    });
    let snapshot_before = world.snapshot();
    let journal_before = world.journal().clone();

    let error = world
        .emit_effect(
            "http.request",
            serde_json::json!({"url": "https://example.com/denied"}),
            "cap_all",
            EffectOrigin::System,
        )
        .expect_err("policy deny is a deterministic rejection");
    assert!(matches!(
        error,
        WorldError::PolicyDenied { ref intent_id, ref reason }
            if intent_id == "intent-1" && reason == "deterministic-effect-deny"
    ));

    let tail = &world.journal().events[journal_before.events.len()..];
    assert_eq!(tail.len(), 1, "policy deny has exactly one committed event");
    match &tail[0].body {
        WorldEventBody::PolicyDecisionRecorded(record) => {
            assert_eq!(record.intent_id, "intent-1");
            assert_eq!(record.effect_kind, "http.request");
            assert_eq!(record.cap_ref, "cap_all");
            assert!(matches!(
                record.decision,
                PolicyDecision::Deny { ref reason }
                    if reason == "deterministic-effect-deny"
            ));
        }
        other => panic!("unexpected policy-deny event: {other:?}"),
    }
    assert!(
        !tail
            .iter()
            .any(|event| matches!(event.body, WorldEventBody::EffectQueued(_)))
    );
    assert_eq!(world.pending_effects_len(), 0);
    assert_eq!(world.snapshot().next_intent_id, 2);
    assert_eq!(
        world.snapshot().intent_id_era,
        snapshot_before.intent_id_era
    );
    assert_eq!(world.snapshot().last_event_id, 1);
    assert_eq!(world.snapshot().event_id_era, snapshot_before.event_id_era);

    let replayed = World::from_snapshot(snapshot_before, world.journal().clone())
        .expect("replay deterministic policy disposition");
    assert_eq!(replayed.state(), world.state());
    assert_eq!(replayed.journal(), world.journal());
    assert_eq!(
        replayed.snapshot().next_intent_id,
        world.snapshot().next_intent_id,
        "replay must preserve the post-deny intent allocator"
    );
    assert_eq!(
        replayed.snapshot().intent_id_era,
        world.snapshot().intent_id_era
    );
    assert_eq!(
        replayed.snapshot().last_event_id,
        world.snapshot().last_event_id
    );
    assert_eq!(
        replayed.tick_consensus_records(),
        world.tick_consensus_records(),
        "replay must rebuild the policy audit consensus record"
    );
}

#[test]
fn authorization_linked_effect_queue_full_rolls_back_allowed_public_effect() {
    let effect_grant = super::capability_grant_v2::signed_effect_grant();
    let mut world =
        super::capability_grant_v2::fixture_world_with_revocations_and_budget_and_effect_grant(
            std::collections::BTreeSet::new(),
            128,
            effect_grant.clone(),
        );
    world = world.with_runtime_memory_limits(WorldRuntimeMemoryLimits {
        max_pending_effects: 1,
        ..WorldRuntimeMemoryLimits::default()
    });
    let command_grant = super::capability_grant_v2::signed_grant(
        super::capability_grant_v2::grant_json(serde_json::json!({})),
    );
    let (catalog, response) = super::capability_grant_v2::prepared_invocation(
        &world,
        &command_grant,
        super::capability_grant_v2::catalog_json(serde_json::json!({})),
        super::capability_grant_v2::response_json(serde_json::json!({})),
    );
    super::capability_grant_v2::install_invocation_context(
        &mut world,
        &command_grant,
        &catalog,
        &response,
    );
    let mut sandbox = super::capability_grant_v2::ConfiguredSandbox {
        calls: 0,
        output: ModuleOutput {
            new_state: None,
            effects: vec![ModuleEffectIntent {
                kind: "weather.publish".to_string(),
                params: serde_json::json!({"station": "station-linked"}),
                cap_ref: effect_grant.grant_id,
                cap_slot: None,
            }],
            emits: Vec::new(),
            tick_lifecycle: None,
            output_bytes: 16,
        },
    };
    super::capability_grant_v2::execute_without_invocation_context(
        &mut world,
        command_grant,
        catalog,
        response,
        &mut sandbox,
    )
    .expect("trusted command creates an authorization-linked pending effect");
    assert_eq!(world.pending_effects_len(), 1);
    let linked_intent = world
        .snapshot()
        .pending_effects
        .first()
        .expect("linked pending effect")
        .intent_id
        .clone();
    assert!(
        world
            .capability_effect_receipt_links()
            .contains_key(&linked_intent)
    );

    world.add_capability(CapabilityGrant::allow_all("cap_all"));
    world.set_policy(PolicySet::allow_all());
    let snapshot_before = world.snapshot();
    let journal_before = world.journal().clone();
    let consensus_before = world.tick_consensus_records().to_vec();
    let rejection_audit_before = world.tick_consensus_rejection_audit_events().to_vec();
    let backpressure_before = world.runtime_backpressure_stats().clone();

    let error = world
        .emit_effect(
            "http.request",
            serde_json::json!({"url": "https://example.com/blocked-by-linked-queue"}),
            "cap_all",
            EffectOrigin::System,
        )
        .expect_err("authorization-linked queue must reject a new allowed effect");
    assert!(matches!(
        error,
        WorldError::CapabilityAuthorizationDenied { ref reason }
            if reason.contains("authorization-linked")
    ));
    assert_eq!(world.snapshot(), snapshot_before);
    assert_eq!(world.journal(), &journal_before);
    assert_eq!(world.tick_consensus_records(), consensus_before.as_slice());
    assert_eq!(
        world.tick_consensus_rejection_audit_events(),
        rejection_audit_before.as_slice()
    );
    assert_eq!(world.runtime_backpressure_stats(), &backpressure_before);
    assert_eq!(world.pending_effects_len(), 1);
    assert!(
        world
            .capability_effect_receipt_links()
            .contains_key(&linked_intent)
    );
}

#[test]
fn successful_rule_decision_publication_replays_consensus_equivalently() {
    let mut world = World::new();
    let stable_snapshot = world.snapshot();

    world
        .record_rule_decision(
            RuleDecisionRecord {
                action_id: 7,
                module_id: "rule.transaction-regression".to_string(),
                stage: ModuleSubscriptionStage::PreAction,
                verdict: RuleVerdict::Allow,
                override_action: None,
                cost: ResourceDelta::default(),
                notes: vec!["publication-failure-regression".to_string()],
            },
            None,
        )
        .expect("successful rule decision publication");

    let record = world
        .latest_tick_consensus_record()
        .expect("rule decision must publish a tick consensus record");
    assert_eq!(
        record.block.header.state_root,
        world
            .current_state_root_hash()
            .expect("compute post-publication state root")
    );

    let restored = World::from_snapshot(stable_snapshot, world.journal().clone())
        .expect("replay rule decision publication");
    assert_eq!(restored.state(), world.state());
    assert_eq!(restored.journal(), world.journal());
    assert_eq!(
        restored.latest_tick_consensus_record(),
        world.latest_tick_consensus_record()
    );
}

#[test]
fn prepared_body_publication_commits_consensus_root_after_domain_routing() {
    let mut world = World::new();
    world.submit_action(Action::RegisterAgent {
        agent_id: "body-root-regression-agent".to_string(),
        pos: pos(0, 0),
    });
    world.step().expect("register body root regression agent");

    let view = BodyKernelView {
        mass_kg: 120,
        radius_cm: 80,
        thrust_limit: 200,
        cross_section_cm2: 4_000,
    };
    world
        .record_body_attributes_update(
            "body-root-regression-agent",
            view,
            "state-root-regression",
            None,
        )
        .expect("publish body attributes update");

    let record = world
        .latest_tick_consensus_record()
        .expect("body update must publish a tick consensus record");
    assert_eq!(
        record.block.header.state_root,
        world
            .current_state_root_hash()
            .expect("compute post-publication state root"),
        "prepared consensus root must include all canonical state changes from domain routing"
    );
}

#[test]
fn prepared_body_publication_replays_to_equivalent_state_and_consensus() {
    let mut world = World::new();
    world.submit_action(Action::RegisterAgent {
        agent_id: "body-replay-regression-agent".to_string(),
        pos: pos(0, 0),
    });
    world.step().expect("register body replay regression agent");
    let stable_snapshot = world.snapshot();

    let view = BodyKernelView {
        mass_kg: 120,
        radius_cm: 80,
        thrust_limit: 200,
        cross_section_cm2: 4_000,
    };
    world
        .record_body_attributes_update(
            "body-replay-regression-agent",
            view,
            "replay-regression",
            None,
        )
        .expect("publish body attributes update");

    let restored = World::from_snapshot(stable_snapshot, world.journal().clone())
        .expect("replay body attributes update");
    assert_eq!(restored.state(), world.state());
    assert_eq!(restored.journal(), world.journal());
    assert_eq!(
        restored
            .current_state_root_hash()
            .expect("compute replayed state root"),
        world
            .current_state_root_hash()
            .expect("compute live state root")
    );
    assert_eq!(
        restored.latest_tick_consensus_record(),
        world.latest_tick_consensus_record(),
        "replay must rebuild the same consensus record as live publication"
    );
}

#[test]
fn rejected_body_publication_failure_after_prepare_is_unpublished() {
    let mut world = World::new();
    world.submit_action(Action::RegisterAgent {
        agent_id: "body-rejected-publication-regression-agent".to_string(),
        pos: pos(0, 0),
    });
    world
        .step()
        .expect("register body rejection regression agent");

    let snapshot_before = world.snapshot();
    let journal_before = world.journal().clone();
    let consensus_before = world.tick_consensus_records().to_vec();
    let rejection_audit_before = world.tick_consensus_rejection_audit_events().to_vec();
    let mailbox_before = world
        .state()
        .agents
        .get("body-rejected-publication-regression-agent")
        .expect("body rejection regression agent")
        .mailbox
        .clone();

    // This failpoint is after event id/era, journal/backpressure and
    // consensus candidate preparation, before canonical installation. The
    // rejection path must use the same atomic append seam as body updates.
    world.fail_next_append_after_publication_prepare_for_test();
    world
        .record_body_attributes_reject(
            "body-rejected-publication-regression-agent",
            "invalid body attributes",
            None,
        )
        .expect_err("injected rejection publication failure must abort the transition");

    assert_eq!(world.snapshot(), snapshot_before);
    assert_eq!(
        world.snapshot().last_event_id,
        snapshot_before.last_event_id
    );
    assert_eq!(world.snapshot().event_id_era, snapshot_before.event_id_era);
    assert_eq!(world.journal(), &journal_before);
    assert_eq!(world.tick_consensus_records(), consensus_before.as_slice());
    assert_eq!(
        world.tick_consensus_rejection_audit_events(),
        rejection_audit_before.as_slice()
    );
    assert_eq!(
        world
            .state()
            .agents
            .get("body-rejected-publication-regression-agent")
            .expect("body rejection regression agent")
            .mailbox,
        mailbox_before
    );
}

#[test]
fn successful_staged_step_preserves_snapshot_journal_replay_equivalence() {
    let mut world = World::new();
    world.submit_action(Action::RegisterAgent {
        agent_id: "replay-agent".to_string(),
        pos: pos(0, 0),
    });
    world.step().expect("register replay agent");
    let stable_snapshot = world.snapshot();

    world.submit_action(Action::MoveAgent {
        agent_id: "replay-agent".to_string(),
        to: pos(4, 7),
    });
    world.step().expect("commit staged move");

    let restored = World::from_snapshot(stable_snapshot, world.journal().clone())
        .expect("replay staged transition");
    assert_eq!(restored.state(), world.state());
    assert_eq!(restored.journal(), world.journal());
    assert_eq!(
        restored.current_state_root_hash().expect("restored root"),
        world.current_state_root_hash().expect("live root")
    );
}
