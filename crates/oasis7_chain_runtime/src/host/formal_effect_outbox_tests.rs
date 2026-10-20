use super::*;
use oasis7_runtime::EffectOrigin;

fn world(name: &str) -> FormalWorldIdentityV1 {
    FormalWorldIdentityV1 {
        world_id: name.to_owned(),
        chain_id: "chain-c13".into(),
        genesis_digest: "a".repeat(64),
    }
}

fn commit() -> QualifiedEffectCommitRefV1 {
    QualifiedEffectCommitRefV1 {
        position: 7,
        record_id: "b".repeat(64),
        active_authority_id: "authority-c13".into(),
        finalizing_authority_id: "authority-c13".into(),
    }
}

fn source(
    world: &FormalWorldIdentityV1,
    commit: &QualifiedEffectCommitRefV1,
) -> QualifiedEffectSourceV1 {
    QualifiedEffectSourceV1 {
        world: world.clone(),
        position: commit.position,
        record_id: commit.record_id.clone(),
        active_authority_id: commit.active_authority_id.clone(),
        effect_outbox_root: "f".repeat(64),
    }
}

fn intent(birth_era: Option<u64>, params: JsonValue) -> EffectIntent {
    EffectIntent {
        intent_id: "effect-7".into(),
        birth_era,
        kind: "send_message".into(),
        params,
        cap_ref: "cap-7".into(),
        origin: EffectOrigin::System,
    }
}

fn parameters(extra: Option<(&str, JsonValue)>) -> JsonValue {
    let mut object = serde_json::Map::from_iter([
        ("schema_version".into(), JsonValue::from(1)),
        ("target_id".into(), JsonValue::from("test.receiver")),
        (
            "receiver_contract_digest".into(),
            JsonValue::from("c".repeat(64)),
        ),
        ("payload".into(), serde_json::json!({"text":"hello"})),
    ]);
    if let Some((key, value)) = extra {
        object.insert(key.to_owned(), value);
    }
    JsonValue::Object(object)
}

#[test]
fn formal_effect_outbox_accepts_only_birth_bound_delivery_schema() {
    let world = world("world-c13");
    let commit = commit();
    let source = source(&world, &commit);
    let mut seen = BTreeMap::new();
    let mut obligations = Vec::new();
    let mut blocked = Vec::new();
    append_intent(
        &world,
        &commit,
        &source,
        intent(Some(3), parameters(None)),
        &mut seen,
        &mut obligations,
        &mut blocked,
    )
    .unwrap();
    assert_eq!(obligations.len(), 1);
    assert!(blocked.is_empty());
    assert_eq!(obligations[0].identity.birth_era, 3);
    assert_eq!(obligations[0].target_id, "test.receiver");

    append_intent(
        &world,
        &commit,
        &source,
        intent(None, parameters(None)),
        &mut seen,
        &mut obligations,
        &mut blocked,
    )
    .unwrap();
    assert_eq!(blocked.len(), 1);
    assert_eq!(blocked[0].birth_era, None);
    assert!(blocked[0].reason.contains("birth_era"));
}

#[test]
fn formal_effect_outbox_blocks_uncommitted_routes_and_unknown_selector_fields() {
    for params in [
        serde_json::json!({"payload":{"text":"hello"}}),
        parameters(Some(("url", JsonValue::from("http://127.0.0.1:1/")))),
        serde_json::json!({
            "schema_version": 2,
            "target_id": "test.receiver",
            "receiver_contract_digest": "c".repeat(64),
            "payload": {}
        }),
    ] {
        let world = world("world-c13");
        let commit = commit();
        let source = source(&world, &commit);
        let mut seen = BTreeMap::new();
        let mut obligations = Vec::new();
        let mut blocked = Vec::new();
        append_intent(
            &world,
            &commit,
            &source,
            intent(Some(4), params),
            &mut seen,
            &mut obligations,
            &mut blocked,
        )
        .unwrap();
        assert!(obligations.is_empty());
        assert_eq!(blocked.len(), 1);
    }
}

#[test]
fn formal_effect_outbox_deduplicates_exact_and_rejects_conflicting_identity_in_one_cut() {
    let world = world("world-c13");
    let commit = commit();
    let source = source(&world, &commit);
    let mut seen = BTreeMap::new();
    let mut obligations = Vec::new();
    let mut blocked = Vec::new();
    let first = intent(Some(5), parameters(None));
    append_intent(
        &world,
        &commit,
        &source,
        first.clone(),
        &mut seen,
        &mut obligations,
        &mut blocked,
    )
    .unwrap();
    append_intent(
        &world,
        &commit,
        &source,
        first.clone(),
        &mut seen,
        &mut obligations,
        &mut blocked,
    )
    .unwrap();
    assert_eq!(obligations.len(), 1, "exact duplicate is observed once");
    let mut conflicting = first;
    conflicting.params["payload"]["text"] = JsonValue::from("changed");
    assert!(
        append_intent(
            &world,
            &commit,
            &source,
            conflicting,
            &mut seen,
            &mut obligations,
            &mut blocked,
        )
        .is_err()
    );
}

#[test]
fn formal_effect_outbox_external_identity_binds_world_and_birth_era() {
    let first = ExternalEffectIdentityV1 {
        world: world("world-a"),
        birth_era: 2,
        intent_id: "effect-7".into(),
    };
    let other_world = ExternalEffectIdentityV1 {
        world: world("world-b"),
        ..first.clone()
    };
    let other_era = ExternalEffectIdentityV1 {
        birth_era: 3,
        ..first.clone()
    };
    assert_ne!(
        delivery_identity_key(&first).unwrap(),
        delivery_identity_key(&other_world).unwrap()
    );
    assert_ne!(
        delivery_identity_key(&first).unwrap(),
        delivery_identity_key(&other_era).unwrap()
    );
}

#[test]
fn formal_effect_qualified_obligation_source_marker_binds_all_dispatch_fields() {
    let value = intent(Some(6), parameters(None));
    let mut qualified = test_only_qualified_obligation(world("world-seal"), commit(), value)
        .expect("explicit unit-layer source fixture");
    validate_qualified_effect_obligation(&qualified).unwrap();
    qualified.target_id = "changed.target".into();
    assert!(validate_qualified_effect_obligation(&qualified).is_err());
}

#[test]
fn formal_effect_qualified_private_seal_rejects_rewritten_intent_and_recomputed_public_digest() {
    let value = intent(Some(6), parameters(None));
    let mut qualified = test_only_qualified_obligation(world("world-seal"), commit(), value)
        .expect("explicit unit-layer source fixture");
    validate_qualified_effect_obligation(&qualified).unwrap();

    qualified.intent.params["payload"]["text"] = JsonValue::from("rewritten");
    // A caller can recompute mutable public fields, but cannot replace the
    // private commitment captured from the qualified source record.
    qualified.payload_digest = qualified.intent.payload_digest().unwrap();
    assert!(
        validate_effect_obligation_binding(&EffectObligationBindingV1::from_qualified(&qualified))
            .is_ok()
    );
    assert!(validate_qualified_effect_obligation(&qualified).is_err());
}

#[test]
fn formal_effect_whole_outbox_seal_binds_empty_cut_finalizer_and_obligation_list() {
    let world = world("world-empty-seal");
    let mut source_commit = commit();
    let qualification_source = source(&world, &source_commit);
    let mut outbox = QualifiedEffectOutboxV1 {
        commit: source_commit.clone(),
        obligations: Vec::new(),
        blocked: Vec::new(),
        qualification: qualification_source.clone(),
        qualification_seal: qualified_outbox_digest(
            &qualification_source,
            &source_commit,
            &[],
            &[],
        )
        .unwrap(),
    };
    validate_qualified_effect_outbox(&outbox).unwrap();

    source_commit.finalizing_authority_id = "rewritten-finalizer".into();
    outbox.commit = source_commit;
    assert!(validate_qualified_effect_outbox(&outbox).is_err());

    let source_commit = commit();
    let qualification_source = source(&world, &source_commit);
    let obligation = test_only_qualified_obligation(
        world.clone(),
        source_commit.clone(),
        intent(Some(7), parameters(None)),
    )
    .unwrap();
    let mut sealed = QualifiedEffectOutboxV1 {
        commit: source_commit.clone(),
        obligations: vec![obligation],
        blocked: Vec::new(),
        qualification: qualification_source.clone(),
        qualification_seal: String::new(),
    };
    sealed.qualification_seal = qualified_outbox_digest(
        &qualification_source,
        &source_commit,
        &sealed.obligations,
        &sealed.blocked,
    )
    .unwrap();
    validate_qualified_effect_outbox(&sealed).unwrap();
    sealed.obligations.clear();
    assert!(validate_qualified_effect_outbox(&sealed).is_err());
}
