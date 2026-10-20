use super::*;
#[test]
fn certified_producer_to_decision_conversion_binding_and_normal_start_survives_restore() {
    use crate::world_rules::industrial_work_admission::{self as admission, tests as producer};
    use oasis7_wasm_abi::*;
    let (mut state, key, mut accept) = producer::root_fixture();
    let impacts = match &accept {
        IndustrialWorkCommandV1::AcceptRoot { authority, .. } => authority.baseline_impacts.clone(),
        _ => unreachable!(),
    };
    let reference = |id: &str| SubstitutionProfileRefV1 {
        id: id.into(),
        version: "1".into(),
        digest: crate::util::sha256_hex(id.as_bytes()),
    };
    let mut rules = vec![];
    {
        let material = "iron_ore";
        let r = reference(material);
        rules.push(SubstitutionRuleV1 {
            rule: reference(&format!("rule-{material}")),
            recipe: reference("recipe.smelter.iron_ingot"),
            input_edge: material.into(),
            required_material: r.clone(),
            allowed_materials: vec![reference("reclaimed_ore")],
            accepted_specs: vec!["qualified-spec".into()],
            accepted_quality: vec!["qualified-quality".into()],
            accepted_custody: vec!["qualified-custody".into()],
            owner_ledger_path_policy: reference("owner-path"),
            source_unit: "mass".into(),
            target_unit: "mass".into(),
            ratio_numerator: 1,
            ratio_denominator: 1,
            executable_target_quantum: 1,
            rounding: SubstitutionRoundingV1::Exact,
            residual: SubstitutionResidualV1::Reject,
            aggregation_domain: reference("batch"),
            source_loss_numerator: 0,
            source_loss_denominator: 1,
            impacts: impacts.clone(),
            conversion: if material == "iron_ore" {
                SubstitutionConversionV1::Required {
                    converter: reference("converter"),
                }
            } else {
                SubstitutionConversionV1::None
            },
            applicable_input_states: vec!["unused".into()],
            disposition_policy: reference("hold"),
            recheck_policy: reference("fresh"),
        });
    }
    if let IndustrialWorkCommandV1::AcceptRoot {
        input_baselines, ..
    } = &mut accept
    {
        for (material, baseline) in input_baselines {
            baseline.material_profile = reference(material);
        }
    }
    state
        .recipe_profiles
        .get_mut("recipe.smelter.iron_ingot")
        .unwrap()
        .substitution_rules = rules;
    state
        .material_ledgers
        .get_mut(&MaterialLedgerId::agent("owner"))
        .unwrap()
        .insert("reclaimed_ore".into(), 20);
    for material in ["iron_ore", "carbon_fuel"] {
        let mut facts = producer::facts(&state);
        facts.material = if material == "iron_ore" {
            "reclaimed_ore"
        } else {
            material
        }
        .into();
        facts.material_profile = reference(&facts.material);
        facts.spec = "qualified-spec".into();
        facts.quality = "qualified-quality".into();
        facts.custody = "qualified-custody".into();
        facts.quantity = if material == "iron_ore" { 4 } else { 1 };
        facts.escrow_capacity = facts.quantity;
        facts.expires_at = 100;
        facts.provenance_ref =
            work::industrial_ref("world", "industrial", "source/v1", material, 1, &material)
                .unwrap();
        let event = producer::import(&state, &key, &format!("import-{material}"), facts);
        commit(&mut state, event, 1);
    }
    if let IndustrialWorkCommandV1::AcceptRoot { authority, .. } = &mut accept {
        authority.state_digest = crate::util::hash_json(&state).unwrap();
        authority.profile_digest =
            crate::util::hash_json(&state.recipe_profiles["recipe.smelter.iron_ingot"]).unwrap();
        authority.expires_at = 100;
    }
    let manifest = admission::industrial_root_manifest(
        "world",
        "owner",
        &crate::world_rules::request_key("owner", "root"),
        &accept,
        &state,
    )
    .unwrap();
    if let IndustrialWorkCommandV1::AcceptRoot { certificate, .. } = &mut accept {
        *certificate = producer::certificate(manifest, &key);
    }
    commit(&mut state, producer::event("root", accept), 1);
    let root = state
        .world_rules
        .as_ref()
        .unwrap()
        .industrial_work
        .roots
        .values()
        .next()
        .unwrap()
        .clone();
    for material in ["iron_ore", "carbon_fuel"] {
        let lot = state
            .world_rules
            .as_ref()
            .unwrap()
            .industrial_work
            .lots
            .values()
            .find(|l| {
                l.material
                    == if material == "iron_ore" {
                        "reclaimed_ore"
                    } else {
                        material
                    }
            })
            .unwrap()
            .clone();
        let rule = state.recipe_profiles["recipe.smelter.iron_ingot"]
            .substitution_rules
            .iter()
            .find(|r| r.input_edge == material)
            .cloned();
        let a = SubstitutionExecutionAuthorityV1 {
            authority_ref: work::industrial_ref(
                "world",
                "industrial",
                "recipe-substitution/v1",
                &format!("authority-{material}"),
                1,
                &material,
            )
            .unwrap(),
            owner_id: "owner".into(),
            owner_binding_digest: admission::industrial_owner_binding(&state, "owner").unwrap(),
            factory_digest: crate::util::hash_json(&state.factories["smelter"]).unwrap(),
            root_ref: root.root_ref.clone(),
            slot_id: material.into(),
            expected_binding_revision: 1,
            profile_digest: root.pinned_profile_digest.clone(),
            selection: if let Some(rule) = &rule {
                SubstitutionSelectionV1::ProfileRule {
                    rule: rule.rule.clone(),
                    source_lot: lot.lot_ref.clone(),
                    source_material: reference(&lot.material),
                }
            } else {
                SubstitutionSelectionV1::SameMaterialSource {
                    source_lot: lot.lot_ref.clone(),
                    source_material: reference(&lot.material),
                }
            },
            target_recipe: root.pinned_recipe_id.clone(),
            baseline_impacts: impacts.clone(),
            candidate_impacts: impacts.clone(),
            output_material: material.into(),
            output_unit: "mass".into(),
            output_spec: "qualified-spec".into(),
            output_quality: "qualified-quality".into(),
            output_custody: "qualified-custody".into(),
            owner_ledger_path_policy: reference("owner-path"),
            input_state: "unused".into(),
            costs: BTreeMap::new(),
            power_cost: 0,
            duration_ticks: 0,
            terminal_capacity: root.slots[material].quantity,
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
        };
        if material == "iron_ore" {
            let mut unknown = serde_json::to_value(&a).unwrap();
            unknown
                .as_object_mut()
                .unwrap()
                .insert("allow_implicit_conversion".into(), serde_json::json!(true));
            assert!(serde_json::from_value::<SubstitutionExecutionAuthorityV1>(unknown).is_err());
            for required in ["risks", "next_recheck", "valuation"] {
                let mut missing = serde_json::to_value(&a).unwrap();
                missing.as_object_mut().unwrap().remove(required);
                assert!(
                    serde_json::from_value::<SubstitutionExecutionAuthorityV1>(missing).is_err()
                );
            }
            let mut oversized = state.clone();
            for index in 0..8 {
                let mut large = a.clone();
                let id = format!("large-{index}");
                large.authority_ref = work::industrial_ref(
                    "world",
                    "industrial",
                    "recipe-substitution/v1",
                    &id,
                    1,
                    &id,
                )
                .unwrap();
                large.risks = vec!["r".repeat(256); 32];
                let manifest =
                    crate::util::hash_json(&("recipe-substitution-authority/v1", "world", &large))
                        .unwrap();
                commit(
                    &mut oversized,
                    sub_event(
                        &id,
                        RecipeSubstitutionCommandV1::PublishAuthority {
                            authority: Box::new(large),
                            consensus_height: 1,
                            finality_certificate: producer::certificate(manifest, &key),
                        },
                    ),
                    1,
                );
            }
            let cap_request = SubstitutionPreviewRequestV1 {
                root_ref: root.root_ref.clone(),
                slot_id: material.into(),
                authority_refs: vec![],
            };
            let before = serde_json::to_vec(&oversized).unwrap();
            assert!(preview_substitution(&oversized, "owner", &cap_request, "world", 1).is_err());
            assert_eq!(before, serde_json::to_vec(&oversized).unwrap());
        }
        let manifest =
            crate::util::hash_json(&("recipe-substitution-authority/v1", "world", &a)).unwrap();
        commit(
            &mut state,
            sub_event(
                &format!("publish-{material}"),
                RecipeSubstitutionCommandV1::PublishAuthority {
                    authority: Box::new(a),
                    consensus_height: 1,
                    finality_certificate: producer::certificate(manifest, &key),
                },
            ),
            1,
        );
        let request = SubstitutionPreviewRequestV1 {
            root_ref: root.root_ref.clone(),
            slot_id: material.into(),
            authority_refs: vec![],
        };
        let preview = preview_substitution(&state, "owner", &request, "world", 1).unwrap();
        assert_eq!(
            preview.status,
            SubstitutionPreviewStatusV1::Available,
            "{preview:?}"
        );
        let c = preview.candidates[0].clone();
        if material == "carbon_fuel" {
            assert!(c.rule.is_none());
            assert!(c.plan.is_none());
            assert_eq!(c.native_source_quantity, Some(1));
        }
        let before = state.material_ledgers.clone();
        let power = state.agents["owner"]
            .state
            .resources
            .get(ResourceKind::Electricity);
        commit(
            &mut state,
            sub_event(
                &format!("confirm-{material}"),
                RecipeSubstitutionCommandV1::Confirm {
                    confirmation: ConfirmSubstitutionV1 {
                        candidate_digest: c.candidate_digest.clone(),
                        preview,
                        scope: SubstitutionScopeV1::DecisionAndExactConversion,
                        maximum_source: c.source_required(),
                        maximum_costs: BTreeMap::new(),
                        maximum_power: 0,
                        cutover_plan_digest: None,
                    },
                },
            ),
            2,
        );
        assert_eq!(state.material_ledgers, before);
        assert_eq!(
            state.agents["owner"]
                .state
                .resources
                .get(ResourceKind::Electricity),
            power
        );
        let decision = state
            .world_rules
            .as_ref()
            .unwrap()
            .substitution
            .decisions
            .values()
            .find(|d| d.candidate.authority.slot_id == material)
            .unwrap()
            .decision_ref
            .clone();
        if material == "iron_ore" {
            let intent = state.world_rules.as_ref().unwrap().substitution.decisions[&decision.id]
                .intent
                .clone();
            for kind in 0..5 {
                let mut changed = state.clone();
                drift(&mut changed, kind, &lot);
                let preview = preview_substitution(&changed, "owner", &request, "world", 2);
                if kind == 0 {
                    assert_ne!(
                        preview.unwrap().candidates[0].candidate_digest,
                        intent.candidate_digest
                    );
                } else {
                    assert!(
                        preview.is_err()
                            || preview.unwrap().status != SubstitutionPreviewStatusV1::Available
                    );
                }
                reject_unchanged(
                    &changed,
                    sub_event(
                        "stale-confirm",
                        RecipeSubstitutionCommandV1::Confirm {
                            confirmation: intent.clone(),
                        },
                    ),
                    2,
                );
                reject_unchanged(
                    &changed,
                    sub_event(
                        "stale-convert",
                        RecipeSubstitutionCommandV1::Convert {
                            decision_ref: decision.clone(),
                        },
                    ),
                    2,
                );
            }
            let mut contender = state.clone();
            commit(
                &mut contender,
                sub_event(
                    "contender",
                    RecipeSubstitutionCommandV1::Confirm {
                        confirmation: intent,
                    },
                ),
                2,
            );
            let contender_ref = contender
                .world_rules
                .as_ref()
                .unwrap()
                .substitution
                .decisions
                .values()
                .find(|d| {
                    d.confirmation_key == crate::world_rules::request_key("owner", "contender")
                })
                .unwrap()
                .decision_ref
                .clone();
            commit(
                &mut contender,
                sub_event(
                    "winner",
                    RecipeSubstitutionCommandV1::Convert {
                        decision_ref: decision.clone(),
                    },
                ),
                2,
            );
            reject_unchanged(
                &contender,
                sub_event(
                    "loser",
                    RecipeSubstitutionCommandV1::Convert {
                        decision_ref: contender_ref,
                    },
                ),
                2,
            );
            assert_eq!(
                contender
                    .world_rules
                    .as_ref()
                    .unwrap()
                    .substitution
                    .settlements
                    .len(),
                1
            );
            assert!(contender
                .world_rules
                .as_ref()
                .unwrap()
                .substitution
                .retained_residuals
                .is_empty());
            let mut expired = state.clone();
            expired
                .world_rules
                .as_mut()
                .unwrap()
                .industrial_work
                .imported_lot_authorities
                .get_mut(&lot.lot_ref.id)
                .unwrap()
                .expires_at = 2;
            commit(
                &mut expired,
                sub_event(
                    "expires-convert",
                    RecipeSubstitutionCommandV1::Convert {
                        decision_ref: decision.clone(),
                    },
                ),
                2,
            );
            reject_unchanged(
                &expired,
                sub_event(
                    "expires-bind",
                    RecipeSubstitutionCommandV1::ApplyBinding {
                        decision_ref: decision.clone(),
                    },
                ),
                3,
            );
        }
        if material == "iron_ore" {
            let mut fork = state.clone();
            let mut only = fork.world_rules.as_ref().unwrap().substitution.decisions[&decision.id]
                .intent
                .clone();
            only.scope = SubstitutionScopeV1::DecisionOnly;
            commit(
                &mut fork,
                sub_event(
                    "only",
                    RecipeSubstitutionCommandV1::Confirm { confirmation: only },
                ),
                2,
            );
            let only_ref = fork
                .world_rules
                .as_ref()
                .unwrap()
                .substitution
                .decisions
                .values()
                .find(|d| d.confirmation_key == crate::world_rules::request_key("owner", "only"))
                .unwrap()
                .decision_ref
                .clone();
            let before = serde_json::to_vec(&fork).unwrap();
            assert!(PreparedWorldRule::prepare(
                &fork,
                &sub_event(
                    "unauthorized-conversion",
                    RecipeSubstitutionCommandV1::Convert {
                        decision_ref: only_ref
                    }
                ),
                2
            )
            .is_err());
            assert_eq!(before, serde_json::to_vec(&fork).unwrap());
        }
        if material == "iron_ore" {
            let event = sub_event(
                "convert",
                RecipeSubstitutionCommandV1::Convert {
                    decision_ref: decision.clone(),
                },
            );
            commit(&mut state, event.clone(), 2);
            let conversion = state.world_rules.as_ref().unwrap().substitution.settlements
                [&decision.id]
                .conversion_ref
                .clone();
            let graph = &state.world_rules.as_ref().unwrap().industrial_work;
            let decision_receipt = graph
                .receipts
                .values()
                .find(|r| r.child_ref == decision && r.effect_role == "substitution_decision")
                .unwrap();
            let conversion_receipt = graph
                .receipts
                .values()
                .find(|r| r.child_ref == conversion && r.effect_role == "conversion_settlement")
                .unwrap();
            assert_ne!(decision_receipt.child_ref, conversion_receipt.child_ref);
            let encoded = serde_json::to_vec(&state).unwrap();
            state = serde_json::from_slice(&encoded).unwrap();
            commit(&mut state, event, 3);
            assert_eq!(
                state.world_rules.as_ref().unwrap().substitution.settlements[&decision.id]
                    .conversion_ref,
                conversion
            );
            assert_eq!(
                state
                    .world_rules
                    .as_ref()
                    .unwrap()
                    .substitution
                    .settlements
                    .len(),
                1
            );
            assert_eq!(
                state.world_rules.as_ref().unwrap().industrial_work.lots[&lot.lot_ref.id].remaining,
                0
            );
        }
        commit(
            &mut state,
            sub_event(
                &format!("bind-{material}"),
                RecipeSubstitutionCommandV1::ApplyBinding {
                    decision_ref: decision,
                },
            ),
            3,
        );
    }
    assert_eq!(
        state.world_rules.as_ref().unwrap().industrial_work.roots[&root.root_ref.id].work,
        0
    );
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
            "start-missing-evidence",
            IndustrialWorkCommandV1::StartRoot {
                root_ref: root.root_ref.clone(),
                expected_profile_digest: root.pinned_profile_digest.clone(),
                logistics_route_ids: vec![],
                logistics_path_ids: vec![],
            },
        ),
        3,
    );
    commit(
        &mut state,
        producer::event(
            "start",
            IndustrialWorkCommandV1::StartRoot {
                root_ref: root.root_ref.clone(),
                expected_profile_digest: root.pinned_profile_digest,
                logistics_route_ids: vec![],
                logistics_path_ids: vec![],
            },
        ),
        3,
    );
    assert!(
        state.world_rules.as_ref().unwrap().industrial_work.roots[&root.root_ref.id]
            .slots
            .values()
            .all(|s| s.consumed)
    );
}
