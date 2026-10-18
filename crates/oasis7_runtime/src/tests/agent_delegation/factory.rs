use super::*;
use oasis7_agent_api::world_action::ResourceKind;
const KIND: &str = "factory.smelter.mk1";
const FACTORY: &str = "registered.factory";

fn build_fixture() -> World {
    let mut world = fixture();
    super::super::economy_factory_lifecycle::install_factory_authority(
        &mut world, AGENT_ID, "site", FACTORY, 7,
    );
    let spec = builtin_factory_spec(KIND, FACTORY).unwrap();
    world
        .upsert_factory_profile(FactoryProfileV1 {
            factory_id: FACTORY.into(),
            tier: spec.tier,
            recipe_slots: spec.recipe_slots,
            tags: spec.tags.clone(),
        })
        .unwrap();
    let mut profile = world.state().factory_construction_power_profiles[FACTORY].clone();
    profile.factory_kind = KIND.into();
    profile.authority_revision = 2;
    world
        .set_factory_construction_power_profile(profile)
        .unwrap();
    world
        .set_agent_resource_balance(AGENT_ID, ResourceKind::Electricity, 100)
        .unwrap();
    for material in &spec.build_cost {
        world
            .set_ledger_material_balance(
                MaterialLedgerId::agent(AGENT_ID),
                &material.kind,
                material.amount,
            )
            .unwrap();
    }
    world
}
fn allowance() -> AgentDelegationGrantV1 {
    let mut g = grant();
    g.object_id = FACTORY.into();
    g.action_kinds = vec!["BuildFactory".into()];
    g.limit_units = 14;
    g
}
fn quote(world: &World) -> Result<NativeBuiltinFactoryQuote, WorldError> {
    world.builtin_factory_quote(AGENT_ID, "site", KIND, FACTORY)
}
#[test]
fn builtin_factory_quote_matches_real_build_and_debits_once() {
    let mut world = build_fixture();
    world.install_agent_delegation_grant(allowance()).unwrap();
    let before = serde_json::to_value(world.snapshot()).unwrap();
    let journal = world.journal().clone();
    let q = quote(&world).unwrap();
    assert_eq!(quote(&world).unwrap(), q);
    assert_eq!(serde_json::to_value(world.snapshot()).unwrap(), before);
    assert_eq!(world.journal(), &journal);
    assert_eq!(q.authorization.cost_units, 7); // construction profile, not base draw 20
    assert_eq!(q.electricity_after, Some(93));
    let p = q.preview.as_ref().unwrap();
    assert_eq!(p.duration_ticks, 1);
    let prepared = prepare(&mut world, "factory-build", q.action, 7, Default::default());
    world
        .finalize_cognition_commit(&prepared.commit_id)
        .unwrap();
    world
        .finalize_cognition_commit(&prepared.commit_id)
        .unwrap();
    assert_eq!(
        world.agent_delegation_authorizations(AGENT_ID).unwrap()[0].spent_units,
        7
    );
    let receipts = world.agent_causal_receipts(AGENT_ID).unwrap();
    assert_eq!(receipts.len(), 1);
    assert_eq!(receipts[0].disposition, "applied");
    assert_eq!(receipts[0].domain_event_refs.len(), 1);
}
#[test]
fn builtin_factory_quote_denies_missing_revoked_ambiguous_grants() {
    let mut world = build_fixture();
    assert!(quote(&world).is_err());
    let mut g = allowance();
    g.revoked = true;
    world.install_agent_delegation_grant(g).unwrap();
    assert!(quote(&world).is_err());
    let mut world = build_fixture();
    world.install_agent_delegation_grant(allowance()).unwrap();
    let mut g = allowance();
    g.grant_id = "second".into();
    world.install_agent_delegation_grant(g).unwrap();
    assert!(quote(&world).is_err());
}
#[test]
fn builtin_factory_quote_denies_each_mutated_spec_field() {
    for field in [
        "factory_id",
        "display_name",
        "tier",
        "tags",
        "build_cost",
        "build_time_ticks",
        "base_power_draw",
        "recipe_slots",
        "throughput_bps",
        "maintenance_per_tick",
    ] {
        let mut world = build_fixture();
        world.install_agent_delegation_grant(allowance()).unwrap();
        let mut value = serde_json::to_value(builtin_factory_spec(KIND, FACTORY).unwrap()).unwrap();
        match field {
            "factory_id" | "display_name" => value[field] = json!("forged"),
            "tags" => value[field] = json!([]),
            "build_cost" => value[field] = json!([]),
            "tier" | "recipe_slots" => value[field] = json!(3),
            "build_time_ticks" | "maintenance_per_tick" => value[field] = json!(2),
            "base_power_draw" => value[field] = json!(21),
            "throughput_bps" => value[field] = json!(9999),
            _ => unreachable!(),
        }
        let action = Action::BuildFactory {
            builder_agent_id: AGENT_ID.into(),
            site_id: "site".into(),
            spec: serde_json::from_value(value).unwrap(),
        };
        assert!(
            world
                .bind_agent_causal_decision("forged", AGENT_ID, &action, Default::default())
                .is_err(),
            "{field}"
        );
    }
}
#[test]
fn builtin_factory_quote_reports_real_domain_denials_without_mutation() {
    for failure in [
        "materials",
        "electricity",
        "site",
        "anchor",
        "power",
        "kind",
    ] {
        let mut world = build_fixture();
        match failure {
            "materials" => world
                .set_ledger_material_balance(
                    MaterialLedgerId::agent(AGENT_ID),
                    "structural_frame",
                    0,
                )
                .unwrap(),
            "electricity" => world
                .set_agent_resource_balance(AGENT_ID, ResourceKind::Electricity, 0)
                .unwrap(),
            _ => {
                let mut state = world.state().clone();
                match failure {
                    "site" => {
                        state
                            .factory_site_authorities
                            .get_mut("site")
                            .unwrap()
                            .active = false
                    }
                    "anchor" => {
                        state
                            .location_anchors
                            .get_mut("location-site")
                            .unwrap()
                            .active = false
                    }
                    "power" => {
                        state
                            .factory_construction_power_profiles
                            .get_mut(FACTORY)
                            .unwrap()
                            .active = false
                    }
                    "kind" => {
                        state
                            .factory_construction_power_profiles
                            .get_mut(FACTORY)
                            .unwrap()
                            .factory_kind = "custom".into()
                    }
                    _ => unreachable!(),
                }
                world = World::new_with_state(state);
            }
        }
        world.install_agent_delegation_grant(allowance()).unwrap();
        let before = serde_json::to_value(world.snapshot()).unwrap();
        let result = quote(&world);
        if matches!(failure, "power" | "kind") {
            assert!(result.is_err());
        } else {
            assert!(result.unwrap().rejection.is_some(), "{failure}");
        }
        assert_eq!(
            serde_json::to_value(world.snapshot()).unwrap(),
            before,
            "{failure}"
        );
    }
}

#[test]
fn builtin_factory_quote_rejects_tampered_admission_facts() {
    let mut world = build_fixture();
    world.install_agent_delegation_grant(allowance()).unwrap();
    let original = quote(&world).unwrap();
    for field in [
        "electricity",
        "materials",
        "ledger",
        "duration",
        "site",
        "profile",
    ] {
        let mut q = original.clone();
        let p = q.preview.as_mut().unwrap();
        match field {
            "electricity" => q.electricity_after = Some(100),
            "materials" => {
                p.construction_power_obligation
                    .material_balances_after
                    .as_mut()
                    .unwrap()
                    .insert("structural_frame".into(), 99);
            }
            "ledger" => p.consume_ledger = MaterialLedgerId::agent("other"),
            "duration" => p.duration_ticks += 1,
            "site" => p.site_authority_revision = 0,
            "profile" => p.construction_power_obligation.profile_key = "other".into(),
            _ => unreachable!(),
        }
        assert!(
            q.validate_selection(AGENT_ID, "site", KIND, FACTORY)
                .is_err(),
            "{field}"
        );
    }
}
#[test]
fn trusted_custom_factory_keeps_legacy_admission_without_native_grant() {
    let mut world = fixture();
    super::super::economy_factory_lifecycle::install_factory_authority(
        &mut world, AGENT_ID, "site", "custom", 0,
    );
    let spec = super::super::economy_factory_lifecycle::factory_spec("custom", 1, 1, 0);
    for material in &spec.build_cost {
        world
            .set_ledger_material_balance(
                MaterialLedgerId::agent(AGENT_ID),
                &material.kind,
                material.amount,
            )
            .unwrap();
    }
    world.submit_action(Action::BuildFactory {
        builder_agent_id: AGENT_ID.into(),
        site_id: "site".into(),
        spec,
    });
    world.step().unwrap();
    world.step().unwrap();
    assert!(world.state().factories.contains_key("custom"));
}
