use super::*;
use crate::world_rules::industrial_work_admission::{self as admission, tests as producer};
use oasis7_wasm_abi::*;
fn reference(id: &str) -> SubstitutionProfileRefV1 {
    SubstitutionProfileRefV1 {
        id: id.into(),
        version: "1".into(),
        digest: crate::util::sha256_hex(id.as_bytes()),
    }
}
fn sign_accept(
    state: &WorldState,
    key: &ed25519_dalek::SigningKey,
    request: &str,
    command: &mut IndustrialWorkCommandV1,
) {
    if let IndustrialWorkCommandV1::AcceptRoot {
        authority,
        recipe_id,
        ..
    } = command
    {
        authority.state_digest = crate::util::hash_json(state).unwrap();
        authority.profile_digest =
            crate::util::hash_json(state.recipe_profiles.get(recipe_id.as_str()).unwrap()).unwrap();
        authority.plan_digest =
            crate::util::hash_json(&crate::builtin_recipe_plan(recipe_id, 1).unwrap()).unwrap();
        authority.expires_at = 100;
    }
    let manifest = admission::industrial_root_manifest(
        "world",
        "owner",
        &crate::world_rules::request_key("owner", request),
        command,
        state,
    )
    .unwrap();
    if let IndustrialWorkCommandV1::AcceptRoot { certificate, .. } = command {
        *certificate = producer::certificate(manifest, key);
    }
}
fn publish(
    state: &mut WorldState,
    key: &ed25519_dalek::SigningKey,
    request: &str,
    a: SubstitutionExecutionAuthorityV1,
) {
    let manifest =
        crate::util::hash_json(&("recipe-substitution-authority/v1", "world", &a)).unwrap();
    commit(
        state,
        sub_event(
            request,
            RecipeSubstitutionCommandV1::PublishAuthority {
                authority: a,
                consensus_height: 1,
                finality_certificate: producer::certificate(manifest, key),
            },
        ),
        1,
    );
}
fn decision(state: &WorldState, key: &str) -> IndustrialRefV1 {
    state
        .world_rules
        .as_ref()
        .unwrap()
        .substitution
        .decisions
        .values()
        .find(|d| d.confirmation_key == crate::world_rules::request_key("owner", key))
        .unwrap()
        .decision_ref
        .clone()
}
fn authority(
    state: &WorldState,
    root: &RootOperationV1,
    slot: &str,
    selection: SubstitutionSelectionV1,
    baseline: SubstitutionImpactV1,
    candidate: SubstitutionImpactV1,
    target_recipe: &str,
    id: &str,
) -> SubstitutionExecutionAuthorityV1 {
    let input = &root.slots[slot];
    SubstitutionExecutionAuthorityV1 {
        authority_ref: work::industrial_ref(
            "world",
            "industrial",
            "recipe-substitution/v1",
            id,
            1,
            &id,
        )
        .unwrap(),
        owner_id: "owner".into(),
        owner_binding_digest: admission::industrial_owner_binding(state, "owner").unwrap(),
        factory_digest: crate::util::hash_json(&state.factories[&root.factory_id]).unwrap(),
        root_ref: root.root_ref.clone(),
        slot_id: slot.into(),
        expected_binding_revision: input.binding_revision,
        profile_digest: root.pinned_profile_digest.clone(),
        selection,
        target_recipe: target_recipe.into(),
        baseline_impacts: baseline,
        candidate_impacts: candidate,
        output_material: input.material.clone(),
        output_unit: input.unit.clone(),
        output_spec: input.spec.clone(),
        output_quality: input.quality.clone(),
        output_custody: input.custody.clone(),
        owner_ledger_path_policy: reference("signed-owner-path"),
        input_state: "unused".into(),
        costs: BTreeMap::new(),
        power_cost: 0,
        duration_ticks: 0,
        terminal_capacity: 100,
        valid_until: 100,
        risks: vec![
            "Revalidate source custody, capacity and available power before execution.".into(),
        ],
        next_recheck: 100,
        valuation: SubstitutionValuationV1 {
            unit: "fixture-value-unit".into(),
            held: 4,
            consumed: 0,
            expected_output: 3,
        },
    }
}
#[test]
fn native_rule_free_source_swap_and_explicit_new_recipe_cutover_preserve_identities() {
    let (mut state, key, mut old_accept) = producer::root_fixture();
    let baseline = match &old_accept {
        IndustrialWorkCommandV1::AcceptRoot { authority, .. } => authority.baseline_impacts.clone(),
        _ => unreachable!(),
    };
    sign_accept(&state, &key, "old-root", &mut old_accept);
    commit(
        &mut state,
        producer::event("old-root", old_accept.clone()),
        1,
    );
    let old = state
        .world_rules
        .as_ref()
        .unwrap()
        .industrial_work
        .roots
        .values()
        .next()
        .unwrap()
        .clone();
    let recipe = "recipe.smelter.copper_wire";
    state.recipe_profiles.insert(
        recipe.into(),
        RecipeProfileV1 {
            recipe_id: recipe.into(),
            substitution_rules: vec![],
            bottleneck_tags: vec![],
            stage_gate: "bootstrap".into(),
            preferred_factory_tags: vec![],
        },
    );
    state
        .material_ledgers
        .get_mut(&MaterialLedgerId::agent("owner"))
        .unwrap()
        .insert("copper_ore".into(), 20);
    let mut facts = producer::facts(&state);
    facts.material = "copper_ore".into();
    facts.material_profile = reference("copper_ore");
    facts.quantity = 3;
    facts.escrow_capacity = 3;
    facts.expires_at = 100;
    let import = producer::import(&state, &key, "copper-import", facts.clone());
    commit(&mut state, import, 1);
    let mut target_impacts = baseline.clone();
    target_impacts.outputs = reference("copper-outputs");
    let mut target_accept = old_accept;
    if let IndustrialWorkCommandV1::AcceptRoot {
        recipe_id,
        input_baselines,
        authority,
        ..
    } = &mut target_accept
    {
        *recipe_id = recipe.into();
        *input_baselines = BTreeMap::from([(
            "copper_ore".into(),
            IndustrialInputBaselineV1 {
                material_profile: reference("copper_ore"),
                unit: facts.unit.clone(),
                spec: facts.spec.clone(),
                quality: facts.quality.clone(),
                custody: facts.custody.clone(),
            },
        )]);
        authority.baseline_impacts = target_impacts.clone();
        authority.terminal_capacities = BTreeMap::from([("copper_wire".into(), 100)]);
    }
    sign_accept(&state, &key, "new-root", &mut target_accept);
    commit(&mut state, producer::event("new-root", target_accept), 1);
    let target = state
        .world_rules
        .as_ref()
        .unwrap()
        .industrial_work
        .roots
        .values()
        .find(|r| r.pinned_recipe_id == recipe)
        .unwrap()
        .clone();
    let lot = state
        .world_rules
        .as_ref()
        .unwrap()
        .industrial_work
        .lots
        .values()
        .find(|l| l.material == "copper_ore")
        .unwrap()
        .clone();
    let a = authority(
        &state,
        &target,
        "copper_ore",
        SubstitutionSelectionV1::SameMaterialSource {
            source_lot: lot.lot_ref,
            source_material: reference("copper_ore"),
        },
        target_impacts.clone(),
        target_impacts.clone(),
        recipe,
        "source-authority",
    );
    publish(&mut state, &key, "source-authority", a);
    let request = SubstitutionPreviewRequestV1 {
        root_ref: target.root_ref.clone(),
        slot_id: "copper_ore".into(),
        authority_refs: vec![],
    };
    let preview = preview_substitution(&state, "owner", &request, "world", 1).unwrap();
    assert_eq!(preview.status, SubstitutionPreviewStatusV1::Available);
    let c = &preview.candidates[0];
    assert!(c.rule.is_none());
    assert!(c.plan.is_none());
    assert_eq!(c.native_source_quantity, Some(3));
    commit(
        &mut state,
        sub_event(
            "source-choice",
            RecipeSubstitutionCommandV1::Confirm {
                confirmation: ConfirmSubstitutionV1 {
                    candidate_digest: c.candidate_digest.clone(),
                    preview,
                    scope: SubstitutionScopeV1::DecisionOnly,
                    maximum_source: 3,
                    maximum_costs: BTreeMap::new(),
                    maximum_power: 0,
                    cutover_plan_digest: None,
                },
            },
        ),
        2,
    );
    let source_decision = decision(&state, "source-choice");
    commit(
        &mut state,
        sub_event(
            "source-bind",
            RecipeSubstitutionCommandV1::ApplyBinding {
                decision_ref: source_decision,
            },
        ),
        2,
    );
    let target =
        state.world_rules.as_ref().unwrap().industrial_work.roots[&target.root_ref.id].clone();
    let a = authority(
        &state,
        &old,
        "iron_ore",
        SubstitutionSelectionV1::NewRecipe {
            candidate_root: target.root_ref.clone(),
        },
        baseline,
        target_impacts,
        recipe,
        "recipe-choice",
    );
    let mut unknown = state.clone();
    let mut unsupported = a.clone();
    unsupported.authority_ref = work::industrial_ref(
        "world",
        "industrial",
        "recipe-substitution/v1",
        "unsupported-authority",
        1,
        &"unsupported",
    )
    .unwrap();
    unsupported.selection = SubstitutionSelectionV1::NewRecipe {
        candidate_root: work::industrial_ref(
            "world",
            "industrial",
            "industrial-root/v1",
            "missing-target",
            1,
            &"missing",
        )
        .unwrap(),
    };
    publish(&mut unknown, &key, "unsupported-authority", unsupported);
    let unknown_request = SubstitutionPreviewRequestV1 {
        root_ref: old.root_ref.clone(),
        slot_id: "iron_ore".into(),
        authority_refs: vec![],
    };
    let before_unknown = serde_json::to_vec(&unknown).unwrap();
    let unknown_preview =
        preview_substitution(&unknown, "owner", &unknown_request, "world", 2).unwrap();
    assert_eq!(unknown_preview.status, SubstitutionPreviewStatusV1::Unknown);
    assert!(unknown_preview.candidates.is_empty());
    assert_eq!(before_unknown, serde_json::to_vec(&unknown).unwrap());
    publish(&mut state, &key, "recipe-authority", a);
    // Historical job identities must not make an unrelated new recipe look
    // unavailable, even at the maximum synthetic rehearsal identity.
    state.settled_recipe_job_ids.insert(1);
    state.settled_recipe_job_ids.insert(u64::MAX);
    let before_recipe_preview = serde_json::to_vec(&state).unwrap();
    let request = SubstitutionPreviewRequestV1 {
        root_ref: old.root_ref.clone(),
        slot_id: "iron_ore".into(),
        authority_refs: vec![],
    };
    let preview = preview_substitution(&state, "owner", &request, "world", 2).unwrap();
    assert_eq!(
        preview.status,
        SubstitutionPreviewStatusV1::Available,
        "{preview:?}"
    );
    let c = &preview.candidates[0];
    assert_eq!(before_recipe_preview, serde_json::to_vec(&state).unwrap());
    assert_eq!(c.kind, SubstitutionCandidateKindV1::NewRecipe);
    assert!(c.plan.is_none());
    assert_eq!(
        c.recipe_plan.as_ref().unwrap().produce[0].kind,
        "copper_wire"
    );
    let disposition = c
        .cutover_plan
        .clone()
        .expect("server prepares full explicit disposition");
    let plan_digest = c
        .cutover_plan_digest
        .clone()
        .expect("server publishes copyable exact digest");
    let before_ledgers = state.material_ledgers.clone();
    let before_power = state.agents["owner"]
        .state
        .resources
        .get(ResourceKind::Electricity);
    commit(
        &mut state,
        sub_event(
            "recipe-confirm",
            RecipeSubstitutionCommandV1::Confirm {
                confirmation: ConfirmSubstitutionV1 {
                    candidate_digest: c.candidate_digest.clone(),
                    preview,
                    scope: SubstitutionScopeV1::DecisionAndExactConversion,
                    maximum_source: 0,
                    maximum_costs: BTreeMap::new(),
                    maximum_power: 0,
                    cutover_plan_digest: Some(plan_digest),
                },
            },
        ),
        2,
    );
    let chosen = decision(&state, "recipe-confirm");
    reject_unchanged(
        &state,
        sub_event(
            "recipe-no-convert",
            RecipeSubstitutionCommandV1::Convert {
                decision_ref: chosen.clone(),
            },
        ),
        2,
    );
    reject_unchanged(
        &state,
        sub_event(
            "recipe-no-bind",
            RecipeSubstitutionCommandV1::ApplyBinding {
                decision_ref: chosen.clone(),
            },
        ),
        2,
    );
    commit(
        &mut state,
        sub_event(
            "cutover",
            RecipeSubstitutionCommandV1::Cutover {
                decision_ref: chosen.clone(),
                disposition,
                candidate_root: target.clone(),
            },
        ),
        2,
    );
    assert_eq!(state.material_ledgers, before_ledgers);
    assert_eq!(
        state.agents["owner"]
            .state
            .resources
            .get(ResourceKind::Electricity),
        before_power
    );
    let graph = &state.world_rules.as_ref().unwrap().industrial_work;
    assert_eq!(graph.roots[&target.root_ref.id].work, 0);
    assert_eq!(graph.roots[&old.root_ref.id].work, 0);
    assert_eq!(graph.roots[&old.root_ref.id].slots, old.slots);
    let receipt = graph
        .receipts
        .values()
        .find(|r| r.effect_role == "substitution_cutover")
        .unwrap();
    assert_eq!(receipt.root_ref, old.root_ref);
    assert_eq!(receipt.child_ref, chosen);
    assert_eq!(receipt.output, vec![target.root_ref.clone()]);
    let mut missing = state.clone();
    missing
        .world_rules
        .as_mut()
        .unwrap()
        .industrial_work
        .imported_lot_authorities
        .clear();
    reject_unchanged(
        &missing,
        producer::event(
            "missing-target-start",
            IndustrialWorkCommandV1::StartRoot {
                root_ref: target.root_ref.clone(),
                expected_profile_digest: target.pinned_profile_digest.clone(),
                logistics_route_ids: vec![],
                logistics_path_ids: vec![],
            },
        ),
        3,
    );
    commit(
        &mut state,
        producer::event(
            "target-start",
            IndustrialWorkCommandV1::StartRoot {
                root_ref: target.root_ref.clone(),
                expected_profile_digest: target.pinned_profile_digest,
                logistics_route_ids: vec![],
                logistics_path_ids: vec![],
            },
        ),
        3,
    );
    assert!(
        !state.world_rules.as_ref().unwrap().industrial_work.roots[&old.root_ref.id].first_sink
    );
    assert!(
        state.world_rules.as_ref().unwrap().industrial_work.roots[&target.root_ref.id].first_sink
    );
}
