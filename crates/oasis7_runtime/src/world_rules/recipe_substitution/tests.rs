use super::*;
fn fixture() -> (WorldState, SubstitutionPreviewRequestV1) {
    let (mut state, _) = crate::world_rules::industrial_work_admission::tests::fixture();
    let profile = oasis7_wasm_abi::RecipeProfileV1 {
        recipe_id: "recipe".into(),
        substitution_rules: vec![],
        bottleneck_tags: vec![],
        stage_gate: String::new(),
        preferred_factory_tags: vec![],
    };
    let digest = crate::util::hash_json(&profile).unwrap();
    state.recipe_profiles.insert("recipe".into(), profile);
    let reference = work::industrial_ref(
        "world",
        "industrial",
        "industrial-root/v1",
        "root",
        1,
        &"root",
    )
    .unwrap();
    let slot = IndustrialInputSlotV1 {
        material: "iron".into(),
        material_profile: oasis7_wasm_abi::SubstitutionProfileRefV1 {
            id: "iron".into(),
            version: "1".into(),
            digest: crate::util::sha256_hex(b"iron"),
        },
        unit: "piece".into(),
        spec: "v1".into(),
        quality: "qualified".into(),
        custody: "owner".into(),
        quantity: 4,
        binding_revision: 1,
        parents: vec![],
        consumed: false,
    };
    let root = RootOperationV1 {
        root_ref: reference.clone(),
        owner_id: "owner".into(),
        owner_binding_digest:
            crate::world_rules::industrial_work_admission::industrial_owner_binding(&state, "owner")
                .unwrap(),
        factory_id: "factory".into(),
        output_ledger: MaterialLedgerId::agent("owner"),
        terminal_capacities: BTreeMap::from([("iron".into(), 100)]),
        pinned_recipe_id: "recipe".into(),
        pinned_profile_digest: digest,
        causal_digest: "a".repeat(64),
        accepted_batches: 1,
        allowed_dispositions: vec![IndustrialDispositionV1::Terminate],
        started_job_id: None,
        work: 0,
        first_sink: false,
        slots: BTreeMap::from([("input".into(), slot)]),
    };
    let mut rules = super::super::WorldRulesStateV1::default();
    rules.industrial_work.roots.insert("root".into(), root);
    state.world_rules = Some(rules);
    (
        state,
        SubstitutionPreviewRequestV1 {
            root_ref: reference,
            slot_id: "input".into(),
            authority_refs: vec![],
        },
    )
}
#[test]
fn empty_authenticated_rule_set_is_distinct_from_unknown_and_preview_is_read_only() {
    let (mut state, request) = fixture();
    let before = serde_json::to_vec(&state.world_rules).unwrap();
    let preview = preview_substitution(&state, "owner", &request, "world", 1).unwrap();
    assert_eq!(
        preview.status,
        SubstitutionPreviewStatusV1::NoLegalSubstitute
    );
    assert!(preview.candidates.is_empty());
    assert_eq!(before, serde_json::to_vec(&state.world_rules).unwrap());
    state.recipe_profiles.clear();
    let preview = preview_substitution(&state, "owner", &request, "world", 1).unwrap();
    assert_eq!(preview.status, SubstitutionPreviewStatusV1::Unknown);
    assert_eq!(preview.blocker.as_deref(), Some("missing_authority"));
    assert!(preview_substitution(&state, "stranger", &request, "world", 1).is_err());
}
#[test]
fn restore_preserves_preview_digest_and_rejects_forged_qualified_identity() {
    let (mut state, mut request) = fixture();
    let before = preview_substitution(&state, "owner", &request, "world", 9).unwrap();
    let restored =
        serde_json::from_slice(&serde_json::to_vec(&state.world_rules).unwrap()).unwrap();
    state.world_rules = restored;
    assert_eq!(
        before,
        preview_substitution(&state, "owner", &request, "world", 9).unwrap()
    );
    request.root_ref.revision = 2;
    assert!(preview_substitution(&state, "owner", &request, "world", 9).is_err());
    request.root_ref.revision = 1;
    request.root_ref.world_binding = "other-world".into();
    assert!(preview_substitution(&state, "owner", &request, "world", 9).is_err());
}

fn sub_event(request: &str, command: RecipeSubstitutionCommandV1) -> crate::DomainEvent {
    crate::DomainEvent::WorldRuleCommitted {
        action_id: 1,
        actor_id: "owner".into(),
        request_id: request.into(),
        command: WorldRuleCommandV1::RecipeSubstitution {
            command: Box::new(command),
        },
        governance_epoch: 0,
        world_binding: Some("world".into()),
    }
}
fn commit(state: &mut WorldState, event: crate::DomainEvent, now: u64) {
    let prepared = PreparedWorldRule::prepare(state, &event, now).unwrap();
    let projected = serde_json::to_value(
        crate::state::WorldStateProjection::borrowed(state).with_world_rule_overlay(&prepared),
    )
    .unwrap();
    prepared.install(state);
    state.route_domain_event(&event);
    assert_eq!(
        projected,
        serde_json::to_value(&*state).unwrap(),
        "prepared projection must equal installed canonical state"
    );
}
fn reject_unchanged(state: &WorldState, event: crate::DomainEvent, now: u64) {
    let before = serde_json::to_vec(state).unwrap();
    assert!(PreparedWorldRule::prepare(state, &event, now).is_err());
    assert_eq!(before, serde_json::to_vec(state).unwrap());
}
fn drift(state: &mut WorldState, kind: usize, lot: &IndustrialLotV1) {
    match kind {
        0 => {
            state
                .agents
                .get_mut("owner")
                .unwrap()
                .state
                .resources
                .remove(ResourceKind::Electricity, 1)
                .unwrap();
        }
        1 => state
            .recipe_profiles
            .get_mut("recipe.smelter.iron_ingot")
            .unwrap()
            .bottleneck_tags
            .push("changed".into()),
        2 => state.starter_oc_claims.get_mut("owner").unwrap().public_key = Some("revoked".into()),
        3 => state.factories.get_mut("smelter").unwrap().durability_ppm = 0,
        4 => {
            state
                .world_rules
                .as_mut()
                .unwrap()
                .industrial_work
                .lots
                .get_mut(&lot.lot_ref.id)
                .unwrap()
                .quality = "revoked".into()
        }
        _ => unreachable!(),
    }
}
#[path = "tests/cutover.rs"]
mod cutover;
#[path = "tests/lifecycle.rs"]
mod lifecycle;
