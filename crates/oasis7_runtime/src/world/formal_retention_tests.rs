use super::*;
use crate::AgentIntentV2;
use oasis7_distfs::controlled_authority::formal::business_index::{
    BusinessIndexNodeReader, BusinessIndexRootV1,
};
use oasis7_wasm_abi::{CapabilityAudience, CapabilityPresenter, CapabilitySubject};
use std::sync::{Arc, Mutex};

#[derive(Default)]
struct EmptyNodes;

impl BusinessIndexNodeReader for EmptyNodes {
    fn read_node(&self, _digest: &str) -> Result<Option<Vec<u8>>, String> {
        Ok(None)
    }
}

#[derive(Clone, Default)]
struct SharedMemoryNodes(Arc<Mutex<BTreeMap<String, Vec<u8>>>>);

impl BusinessIndexNodeReader for SharedMemoryNodes {
    fn read_node(&self, digest: &str) -> Result<Option<Vec<u8>>, String> {
        Ok(self
            .0
            .lock()
            .map_err(|_| "test node lock poisoned".to_string())?
            .get(digest)
            .cloned())
    }
}

#[derive(Default)]
struct CorruptNodes;

impl BusinessIndexNodeReader for CorruptNodes {
    fn read_node(&self, _digest: &str) -> Result<Option<Vec<u8>>, String> {
        Ok(Some(vec![0xff]))
    }
}

fn persist_retention_delta(nodes: &SharedMemoryNodes, plan: &FormalWorldRetentionPlanV1) {
    let Some(delta) = &plan.delta else {
        return;
    };
    let mut stored = nodes.0.lock().unwrap();
    for write in &delta.node_writes {
        stored.insert(write.digest.clone(), write.bytes.clone());
    }
}

fn attach_test_parent(world: &mut World, nodes: Arc<dyn BusinessIndexNodeReader + Send + Sync>) {
    let snapshot_cbor = serde_cbor::to_vec(&world.snapshot()).unwrap();
    let root = world.state.retained_history_root.clone().unwrap();
    let parent = VerifiedFormalRetainedHistoryParentV1::for_test_snapshot(root, &snapshot_cbor);
    world.attach_verified_formal_history(parent, nodes).unwrap();
}

fn synthetic_journal_event(id: u64) -> crate::WorldEvent {
    crate::WorldEvent {
        id,
        time: id,
        caused_by: None,
        body: crate::WorldEventBody::SnapshotCreated(crate::SnapshotMeta {
            journal_len: id as usize,
        }),
    }
}

fn synthetic_large_journal_event(id: u64, payload_bytes: usize) -> crate::WorldEvent {
    crate::WorldEvent {
        id,
        time: id,
        caused_by: None,
        body: crate::WorldEventBody::ModuleEmitted(oasis7_wasm_abi::ModuleEmitEvent {
            module_id: "module.retention-test".into(),
            trace_id: format!("journal-{id}"),
            kind: "retention-test".into(),
            payload: serde_json::Value::String("x".repeat(payload_bytes)),
        }),
    }
}

fn empty_parent() -> VerifiedFormalRetainedHistoryParentV1 {
    let root = FormalRetainedHistoryRootV1::from_index_root(BusinessIndexRootV1::empty().unwrap())
        .unwrap();
    VerifiedFormalRetainedHistoryParentV1::for_test(root)
}

#[test]
fn fresh_formal_history_initialization_is_explicit_and_idempotent() {
    let mut world = World::new();
    assert!(world.state.retained_history_root.is_none());
    world.capability_authorization_root = "stale-before-init".into();
    world.initialize_empty_formal_history().unwrap();
    assert_eq!(
        world.capability_authorization_root,
        world.compute_capability_authorization_root().unwrap()
    );
    let first = world.state.retained_history_root.clone().unwrap();
    assert_eq!(first.index_root, BusinessIndexRootV1::empty().unwrap());
    world.capability_authorization_root = "stale-before-repeat".into();
    world.initialize_empty_formal_history().unwrap();
    assert_eq!(
        world.capability_authorization_root,
        world.compute_capability_authorization_root().unwrap()
    );
    assert_eq!(world.state.retained_history_root.as_ref(), Some(&first));
}

#[test]
fn formal_history_initialization_rejects_existing_archive_obligations() {
    let mut world = World::new();
    world
        .state
        .agent_intent_ledger
        .insert("intent".into(), intent("intent", "completed", None));
    assert!(world.initialize_empty_formal_history().is_err());
    assert!(world.state.retained_history_root.is_none());
}

#[test]
fn formal_history_initialization_preserves_an_already_published_tick() {
    let mut world = World::new();
    world.step().unwrap();
    assert!(world.latest_tick_consensus_record().is_some());
    let before = serde_cbor::to_vec(&world.snapshot()).unwrap();
    assert!(
        world
            .initialize_empty_formal_history()
            .unwrap_err()
            .contains("explicit migration")
    );
    assert_eq!(serde_cbor::to_vec(&world.snapshot()).unwrap(), before);
    World::from_snapshot(world.snapshot(), world.journal().clone()).unwrap();
}

#[test]
fn staged_empty_history_proves_absence_but_cannot_plan_or_restore_qualified_lineage() {
    let mut world = World::new();
    world.initialize_empty_formal_history().unwrap();
    let key = format!("blake3:{}", "a".repeat(64));
    assert!(
        world
            .read_retained_world_service_result(&key)
            .unwrap()
            .is_none()
    );
    assert!(
        world
            .prepare_formal_retention_candidate()
            .unwrap_err()
            .contains("not a qualified")
    );
    let mut restored = World::from_snapshot(world.snapshot(), world.journal().clone()).unwrap();
    assert!(restored.read_retained_world_service_result(&key).is_err());
    assert!(restored.inherit_formal_history_reader(&world).is_err());
}

#[test]
fn retained_reads_fail_closed_without_cas_and_inherit_only_matching_view() {
    let correlation_key = format!("blake3:{}", "a".repeat(64));
    let legacy = World::new();
    assert_eq!(
        legacy
            .read_retained_world_service_result(&correlation_key)
            .unwrap(),
        None
    );

    let mut source = World::new();
    source.initialize_empty_formal_history().unwrap();
    let detached = World::from_snapshot(source.snapshot(), source.journal().clone()).unwrap();
    assert!(matches!(
        detached.read_retained_world_service_result(&correlation_key),
        Err(RetainedHistoryReadError::Unavailable(_))
    ));

    let snapshot_cbor = serde_cbor::to_vec(&source.snapshot()).unwrap();
    let root = source.state.retained_history_root.clone().unwrap();
    let parent = VerifiedFormalRetainedHistoryParentV1::for_test_snapshot(root, &snapshot_cbor);
    source
        .attach_verified_formal_history(parent, Arc::new(EmptyNodes))
        .unwrap();

    let mut matching_target = World::new();
    matching_target.initialize_empty_formal_history().unwrap();
    matching_target
        .inherit_formal_history_reader(&source)
        .unwrap();
    assert_eq!(
        matching_target
            .read_retained_world_service_result(&correlation_key)
            .unwrap(),
        None
    );

    let mut mismatched_target = World::new();
    assert!(
        mismatched_target
            .inherit_formal_history_reader(&source)
            .is_err()
    );
}

#[test]
fn immutable_archive_keys_cannot_be_rewritten_with_different_data() {
    let parent = empty_parent();
    let mut staged = BTreeMap::new();
    let first =
        RetainedHistoryLeafV1::AgentIntentReorgEpochs(DistinctReorgEpochSummaryV1::Single(4));
    let changed =
        RetainedHistoryLeafV1::AgentIntentReorgEpochs(DistinctReorgEpochSummaryV1::Single(5));

    stage_immutable_leaf(
        &parent,
        &EmptyNodes,
        &mut staged,
        "agent-intent",
        "intent-1",
        first.clone(),
    )
    .unwrap();
    stage_immutable_leaf(
        &parent,
        &EmptyNodes,
        &mut staged,
        "agent-intent",
        "intent-1",
        first,
    )
    .unwrap();
    assert!(
        stage_immutable_leaf(
            &parent,
            &EmptyNodes,
            &mut staged,
            "agent-intent",
            "intent-1",
            changed,
        )
        .is_err()
    );
}

#[test]
fn secondary_indexes_keep_two_distinct_refs_to_preserve_ambiguity() {
    let mut refs = Vec::new();
    push_bounded_ref(
        &mut refs,
        format!("blake3:{}", "b".repeat(64)),
        MAX_INDEXED_RESULT_REFS,
    );
    push_bounded_ref(
        &mut refs,
        format!("blake3:{}", "c".repeat(64)),
        MAX_INDEXED_RESULT_REFS,
    );
    push_bounded_ref(
        &mut refs,
        format!("blake3:{}", "a".repeat(64)),
        MAX_INDEXED_RESULT_REFS,
    );
    push_bounded_ref(
        &mut refs,
        format!("blake3:{}", "a".repeat(64)),
        MAX_INDEXED_RESULT_REFS,
    );

    assert_eq!(
        refs,
        vec![
            format!("blake3:{}", "a".repeat(64)),
            format!("blake3:{}", "b".repeat(64))
        ]
    );
}

fn intent(intent_id: &str, status: &str, effect_intent_id: Option<&str>) -> AgentIntentV2 {
    AgentIntentV2 {
        schema_version: 2,
        agent_id: "agent".into(),
        intent_id: intent_id.into(),
        kind: "chat".into(),
        summary: "summary".into(),
        target_id: None,
        effect_intent_id: effect_intent_id.map(str::to_owned),
        intent_tick: Some(1),
        world_id: Some("world".into()),
        reorg_epoch: Some(2),
        authority_scope: Some("scope".into()),
        status: status.into(),
        source: "test".into(),
        logical_time: 1,
        event_seq: 1,
        updated_at: 2,
        receipt_ref: None,
        reason_code: None,
        reason_summary: None,
        replaced_by: None,
        actor_id: "actor".into(),
        request_digest: "request-digest".into(),
    }
}

#[test]
fn only_unreferenced_terminal_intents_are_candidates() {
    let mut world = World::new();
    world
        .state
        .agent_intent_ledger
        .insert("done".into(), intent("done", "completed", None));
    world
        .state
        .agent_intent_ledger
        .insert("blocked".into(), intent("blocked", "blocked", None));
    world.state.agent_intent_ledger.insert(
        "open-effect".into(),
        intent("open-effect", "completed", Some("effect")),
    );
    world
        .pending_effects
        .push_back(crate::EffectIntent {
            birth_era: None,
            intent_id: "effect".into(),
            kind: "test".into(),
            params: serde_json::Value::Null,
            cap_ref: "cap".into(),
            origin: crate::EffectOrigin::System,
        });

    let candidates = agent_intent_candidates(&world).unwrap();
    assert_eq!(
        candidates
            .into_iter()
            .map(|candidate| candidate.intent_id)
            .collect::<Vec<_>>(),
        vec!["done"]
    );
}

#[test]
fn terminal_status_allowlist_excludes_blocked_and_unknown_states() {
    assert!(is_terminal_agent_intent("completed"));
    assert!(is_terminal_agent_intent("superseded"));
    assert!(!is_terminal_agent_intent("blocked"));
    assert!(!is_terminal_agent_intent("inflight"));
}

fn nonce_record() -> CapabilityAuthorizationNonceRecord {
    CapabilityAuthorizationNonceRecord {
        request_hash: "request-hash".into(),
        outcome_hash: "outcome-hash".into(),
        committed_receipt_id: Some("receipt".into()),
        state: "committed".into(),
    }
}

fn authorization_receipt(nonce_key: &str) -> CapabilityAuthorizationAuditReceipt {
    let context = invocation_context();
    CapabilityAuthorizationAuditReceipt {
        receipt_id: "receipt".into(),
        root_receipt_id: None,
        grant_id: Some("grant".into()),
        subject: serde_json::to_value(&context.subject).unwrap(),
        presenter: Some(serde_json::to_value(&context.presenter).unwrap()),
        audience: serde_json::to_value(&context.audience).unwrap(),
        scope_hash: "scope".into(),
        module_id: Some(context.module_id.clone()),
        module_version: Some(context.module_version.clone()),
        manifest_hash: None,
        catalog_snapshot_id: Some(context.catalog_snapshot_id.clone()),
        response_nonce: Some("nonce".into()),
        authorization_nonce_key_hash: Some(nonce_key.into()),
        decision: "accepted".into(),
        denial_code: None,
        budget_before: 1,
        budget_after: Some(0),
        world_head_before: 1,
        world_head_after: Some(2),
        branch_id: "branch".into(),
        finality_epoch: 1,
        finality_block_hash: None,
        finality_status: "finalized".into(),
        state_hash_before: "before".into(),
        state_hash_after: Some("after".into()),
        committed_effect_receipt_id: None,
        committed_effect_receipt_ids: BTreeSet::new(),
        canonical_request_hash: "request-hash".into(),
        canonical_result_hash: "outcome-hash".into(),
    }
}

fn invocation_context() -> crate::CapabilityInvocationContext {
    crate::CapabilityInvocationContext {
        grant_id: "grant".into(),
        subject: CapabilitySubject::System {
            system_id: "system.retention".into(),
            epoch: 1,
        },
        presenter: CapabilityPresenter {
            presenter_id: "provider.retention".into(),
            presenter_kind: "provider".into(),
            session_id: Some("session.retention".into()),
            attestation_ref: None,
        },
        audience: CapabilityAudience {
            world_id: "world.retention".into(),
            branch_id: "branch.retention".into(),
            finality_epoch: 1,
            target_kind: "world".into(),
            target_id: None,
        },
        catalog_snapshot_id: "catalog.retention".into(),
        module_id: "module.retention".into(),
        module_version: "1.0.0".into(),
        response_nonce: "nonce".into(),
    }
}

fn retained_invocation_context() -> Vec<RetainedCapabilityInvocationContextV1> {
    let context = invocation_context();
    let map_key =
        capability_invocation_context_key(&context.grant_id, &context.response_nonce).unwrap();
    vec![RetainedCapabilityInvocationContextV1 { map_key, context }]
}

#[test]
fn retention_pruning_refreshes_authorization_root_and_restores_snapshot() {
    let mut world = World::new();
    world.initialize_empty_formal_history().unwrap();

    for index in 0..33 {
        let nonce_key = format!("{index:064x}");
        let receipt_id = format!("receipt-{index}");
        let response_nonce = format!("nonce-{index}");
        let mut record = nonce_record();
        record.committed_receipt_id = Some(receipt_id.clone());
        record.request_hash = format!("request-{index}");
        record.outcome_hash = format!("outcome-{index}");

        let mut receipt = authorization_receipt(&nonce_key);
        receipt.receipt_id = receipt_id;
        receipt.response_nonce = Some(response_nonce.clone());
        receipt.canonical_request_hash = record.request_hash.clone();
        receipt.canonical_result_hash = record.outcome_hash.clone();

        let mut context = invocation_context();
        context.response_nonce = response_nonce;
        let context_key =
            capability_invocation_context_key(&context.grant_id, &context.response_nonce).unwrap();

        world.capability_nonce_records.insert(nonce_key, record);
        world
            .capability_authorization_receipts
            .insert(receipt.receipt_id.clone(), receipt);
        world
            .capability_invocation_contexts
            .insert(context_key, context);
    }
    world.refresh_capability_authorization_root().unwrap();

    let snapshot_before = world.snapshot();
    let snapshot_bytes = serde_cbor::to_vec(&snapshot_before).unwrap();
    let root = world.state.retained_history_root.clone().unwrap();
    let parent = VerifiedFormalRetainedHistoryParentV1::for_test_snapshot(root, &snapshot_bytes);
    world
        .attach_verified_formal_history(parent, Arc::new(EmptyNodes))
        .unwrap();

    let prior_authorization_root = world.capability_authorization_root.clone();
    let plan = world.prepare_formal_retention_candidate().unwrap();
    assert_eq!(plan.capability_nonce_keys_to_prune.len(), 1);
    assert_eq!(plan.capability_receipt_ids_to_prune.len(), 1);
    assert_eq!(plan.capability_invocation_context_keys_to_prune.len(), 1);
    let pruned_nonce_key = plan.capability_nonce_keys_to_prune[0].clone();
    world.apply_formal_retention_plan(&plan).unwrap();

    let archived_nonce = world
        .read_retained_capability_nonce_record(&pruned_nonce_key)
        .unwrap()
        .unwrap();
    assert_eq!(archived_nonce.nonce_key, pruned_nonce_key);

    assert_eq!(world.capability_nonce_records.len(), 32);
    assert_eq!(world.capability_authorization_receipts.len(), 32);
    assert_eq!(world.capability_invocation_contexts.len(), 32);
    assert_ne!(
        world.capability_authorization_root,
        prior_authorization_root
    );
    assert_eq!(
        world.capability_authorization_root,
        world.compute_capability_authorization_root().unwrap()
    );

    let restored = World::from_snapshot(world.snapshot(), world.journal().clone()).unwrap();
    assert_eq!(restored.capability_nonce_records.len(), 32);
    assert_eq!(restored.capability_authorization_receipts.len(), 32);
    assert_eq!(restored.capability_invocation_contexts.len(), 32);
    assert_eq!(
        restored.capability_authorization_root,
        restored.compute_capability_authorization_root().unwrap()
    );
}

#[test]
fn capability_nonce_candidates_require_receipt_and_effect_closure() {
    let nonce_key = "a".repeat(64);
    let record = nonce_record();
    let receipt = authorization_receipt(&nonce_key);
    RetainedCapabilityNonceV1 {
        nonce_key: nonce_key.clone(),
        record: record.clone(),
        receipt: receipt.clone(),
        invocation_contexts: retained_invocation_context(),
    }
    .validate()
    .unwrap();
    let mut aliased_receipt = receipt.clone();
    aliased_receipt.authorization_nonce_key_hash = Some("b".repeat(64));
    assert!(
        RetainedCapabilityNonceV1 {
            nonce_key: nonce_key.clone(),
            record: record.clone(),
            receipt: aliased_receipt,
            invocation_contexts: retained_invocation_context(),
        }
        .validate()
        .is_err()
    );
    let mut world = World::new();
    assert!(capability_nonce_is_closed(
        &world, &nonce_key, &record, &receipt
    ));

    world.capability_effect_receipt_links.insert(
        "effect".into(),
        crate::CapabilityEffectReceiptLink {
            authorization_receipt_id: "receipt".into(),
        },
    );
    assert!(!capability_nonce_is_closed(
        &world, &nonce_key, &record, &receipt
    ));

    world.capability_effect_receipt_links.clear();
    let mut with_effect = receipt.clone();
    with_effect
        .committed_effect_receipt_ids
        .insert("effect".into());
    let effect = crate::EffectIntent {
        birth_era: None,
        intent_id: "effect".into(),
        kind: "test".into(),
        params: serde_json::Value::Null,
        cap_ref: "cap".into(),
        origin: crate::EffectOrigin::System,
    };
    world.pending_effects.push_back(effect.clone());
    assert!(!capability_nonce_is_closed(
        &world,
        &nonce_key,
        &record,
        &with_effect
    ));

    world.pending_effects.clear();
    world.inflight_effects.insert("effect".into(), effect);
    assert!(!capability_nonce_is_closed(
        &world,
        &nonce_key,
        &record,
        &with_effect
    ));
}

#[test]
fn archived_nonce_preserves_exact_and_legacy_invocation_context_keys() {
    let nonce_key = "a".repeat(64);
    let record = nonce_record();
    let receipt = authorization_receipt(&nonce_key);
    let context = invocation_context();
    let canonical_key =
        capability_invocation_context_key(&context.grant_id, &context.response_nonce).unwrap();
    let mut world = World::new();
    world
        .capability_nonce_records
        .insert(nonce_key.clone(), record);
    world
        .capability_authorization_receipts
        .insert("receipt".into(), receipt);
    world
        .capability_invocation_contexts
        .insert(canonical_key.clone(), context.clone());
    world
        .capability_invocation_contexts
        .insert(context.grant_id.clone(), context.clone());

    let candidate = capability_nonce_candidates(&world).unwrap().remove(0);
    assert_eq!(candidate.invocation_context_keys.len(), 2);
    assert_eq!(candidate.value.invocation_contexts.len(), 2);
    candidate.value.validate().unwrap();
    assert_eq!(
        candidate
            .value
            .invocation_contexts
            .iter()
            .find(|stored| stored.map_key == canonical_key)
            .unwrap()
            .context,
        context
    );
}

#[test]
fn world_state_serializes_retained_root_only_when_present() {
    let mut state = crate::WorldState::default();
    let legacy = serde_json::to_value(&state).unwrap();
    assert!(legacy.get("retained_history_root").is_none());

    state.retained_history_root = Some(
        FormalRetainedHistoryRootV1::from_index_root(BusinessIndexRootV1::empty().unwrap())
            .unwrap(),
    );
    let encoded = serde_json::to_value(&state).unwrap();
    assert!(encoded.get("retained_history_root").is_some());
}

#[test]
fn journal_archive_is_exact_repeatable_and_fails_closed_on_missing_or_corrupt_cas() {
    let mut world = World::new();
    world.initialize_empty_formal_history().unwrap();
    world.journal.events = (1..=258).map(synthetic_journal_event).collect();
    world.next_event_id = 259;

    let nodes = SharedMemoryNodes::default();
    attach_test_parent(&mut world, Arc::new(nodes.clone()));
    let first_plan = world.prepare_formal_retention_candidate().unwrap();
    assert_eq!(first_plan.journal_events_to_prune, vec![(0, 1), (0, 2)]);
    persist_retention_delta(&nodes, &first_plan);
    world.apply_formal_retention_plan(&first_plan).unwrap();
    assert_eq!(world.journal.events.len(), HOT_FORMAL_JOURNAL_EVENTS);
    assert_eq!(world.journal.events.first().unwrap().id, 3);
    assert_eq!(
        world.read_retained_journal_event(0, 1).unwrap(),
        Some(synthetic_journal_event(1))
    );
    assert_eq!(
        world.read_retained_journal_event(0, 2).unwrap(),
        Some(synthetic_journal_event(2))
    );
    world
        .verify_formal_archived_journal_prefix(&[
            ((0, 1), synthetic_journal_event(1)),
            ((0, 2), synthetic_journal_event(2)),
        ])
        .unwrap();

    // A later qualified step archives the next exact prefix while preserving
    // prior archive leaves in the independent sparse index.
    world.journal.events.push(synthetic_journal_event(259));
    world.next_event_id = 260;
    attach_test_parent(&mut world, Arc::new(nodes.clone()));
    let second_plan = world.prepare_formal_retention_candidate().unwrap();
    assert_eq!(second_plan.journal_events_to_prune, vec![(0, 3)]);
    persist_retention_delta(&nodes, &second_plan);
    world.apply_formal_retention_plan(&second_plan).unwrap();
    for id in 1..=3 {
        assert_eq!(
            world.read_retained_journal_event(0, id).unwrap(),
            Some(synthetic_journal_event(id))
        );
    }
    assert_eq!(world.read_retained_journal_event(0, 999).unwrap(), None);
    world
        .verify_formal_archived_journal_prefix(&[
            ((0, 1), synthetic_journal_event(1)),
            ((0, 2), synthetic_journal_event(2)),
            ((0, 3), synthetic_journal_event(3)),
        ])
        .unwrap();

    let snapshot = world.snapshot();
    let hot_journal = world.journal().clone();
    let mut restored_missing = World::from_snapshot(snapshot.clone(), hot_journal.clone()).unwrap();
    attach_test_parent(&mut restored_missing, Arc::new(EmptyNodes));
    assert!(matches!(
        restored_missing.read_retained_journal_event(0, 1),
        Err(RetainedHistoryReadError::Unavailable(_))
    ));

    let mut restored_corrupt = World::from_snapshot(snapshot, hot_journal).unwrap();
    attach_test_parent(&mut restored_corrupt, Arc::new(CorruptNodes));
    assert!(restored_corrupt.read_retained_journal_event(0, 1).is_err());
}

#[test]
fn formal_journal_cursor_derives_rollover_era_and_rejects_saturation() {
    let mut world = World::new();
    world.next_event_id_era = 5;
    world.journal.events = [u64::MAX - 1, u64::MAX, 1, 2]
        .into_iter()
        .map(synthetic_journal_event)
        .collect();
    world.next_event_id = 3;
    assert_eq!(
        world.formal_journal_event_cursors().unwrap(),
        vec![(4, u64::MAX - 1), (4, u64::MAX), (5, 1), (5, 2)]
    );

    world.journal.events = [1, 2].into_iter().map(synthetic_journal_event).collect();
    world.next_event_id = 3;
    world.next_event_id_era = u64::MAX;
    assert!(world.formal_journal_event_cursors().is_err());
}

#[test]
fn formal_journal_planner_enforces_canonical_hot_byte_budget() {
    let mut world = World::new();
    world.initialize_empty_formal_history().unwrap();
    world.journal.events = (1..=5)
        .map(|id| synthetic_large_journal_event(id, 150 * 1024))
        .collect();
    world.next_event_id = 6;
    assert!(
        oasis7_wasm_abi::encode_canonical_cbor(&&world.journal.events[2..])
            .unwrap()
            .len()
            <= HOT_FORMAL_JOURNAL_CBOR_BYTES
    );
    assert!(
        oasis7_wasm_abi::encode_canonical_cbor(&&world.journal.events[1..])
            .unwrap()
            .len()
            > HOT_FORMAL_JOURNAL_CBOR_BYTES
    );

    let nodes = SharedMemoryNodes::default();
    attach_test_parent(&mut world, Arc::new(nodes.clone()));
    let plan = world.prepare_formal_retention_candidate().unwrap();
    assert_eq!(plan.journal_events_to_prune, vec![(0, 1), (0, 2)]);
    persist_retention_delta(&nodes, &plan);
    world.apply_formal_retention_plan(&plan).unwrap();
    assert_eq!(world.journal.events.len(), 3);
    assert!(
        oasis7_wasm_abi::encode_canonical_cbor(&world.journal.events)
            .unwrap()
            .len()
            <= HOT_FORMAL_JOURNAL_CBOR_BYTES
    );
    assert_eq!(
        world.read_retained_journal_event(0, 1).unwrap(),
        Some(synthetic_large_journal_event(1, 150 * 1024))
    );
}

#[path = "formal_retention_cold_tick_tests.rs"]
mod cold_tick_tests;

#[path = "formal_tick_consensus_tests.rs"]
mod bounded_tick_tests;

#[path = "formal_execution_tick_tests.rs"]
mod successor_tick_tests;
