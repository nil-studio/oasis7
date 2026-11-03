//! Real Runtime fixtures; wire roundtrips are not current-use permission.
use super::*;
use adapter::{fact, resolution, selection, transfer_native};
use oasis7_runtime as runtime;
use wire::{DecimalU64, ResourceDecimalI64V1 as I};
const AGENT: &str = "resource-agent";
fn site(world: &mut runtime::World) {
    world
        .set_location_anchor(runtime::LocationAnchorV1 {
            location_id: "resource-location".into(),
            active: true,
            authority_revision: 1,
            effective_at: 0,
        })
        .unwrap();
    world
        .set_agent_location_authority(runtime::AgentLocationAuthorityV1 {
            agent_id: AGENT.into(),
            location_id: "resource-location".into(),
            active: true,
            authority_revision: 1,
            effective_at: 0,
        })
        .unwrap();
    world
        .set_factory_site_authority(runtime::FactorySiteAuthorityV1 {
            site_id: "resource-site".into(),
            location_id: "resource-location".into(),
            owner_agent_id: AGENT.into(),
            authorized_agent_ids: vec![],
            chunk_ready: true,
            active: true,
            authority_revision: 1,
            registered_at: 0,
        })
        .unwrap();
}
fn grant(
    world: &runtime::World,
    id: &str,
    object: String,
    action: &str,
) -> runtime::AgentDelegationGrantV1 {
    let owner = world.current_agent_owner(AGENT).unwrap();
    runtime::AgentDelegationGrantV1 {
        grant_id: id.into(),
        source_id: owner.source_id,
        issuer_id: owner.player_id.clone(),
        owner_id: owner.player_id,
        organization_id: None,
        agent_id: AGENT.into(),
        object_id: object,
        action_kinds: vec![action.into()],
        revision: 1,
        period_id: "resource-period".into(),
        valid_from_tick: 0,
        valid_until_tick: 10000,
        limit_units: 1000,
        resource_kind: "electricity".into(),
        revoked: false,
    }
}
fn fixture() -> runtime::World {
    let mut world = runtime::World::new();
    world.submit_action(runtime::Action::RegisterAgent {
        agent_id: AGENT.into(),
        pos: oasis7_agent_api::world_action::GeoPos::new(0, 0, 0),
    });
    world.step().unwrap();
    let public =
        oasis7_client_api::world_service::sign_read_request("key", (), &hex::encode([9; 32]))
            .unwrap()
            .subject_public_key;
    world.submit_action(runtime::Action::ClaimStarterOc {
        agent_id: AGENT.into(),
        player_id: "resource-owner".into(),
        public_key: Some(public),
    });
    world.step().unwrap();
    world
        .bind_cognition_runtime("resource-world", "main", 0, None, "pending", 0)
        .unwrap();
    site(&mut world);
    world
        .set_agent_resource_balance(
            AGENT,
            oasis7_agent_api::world_action::ResourceKind::Electricity,
            1000,
        )
        .unwrap();
    let mut existing =
        runtime::builtin_factory_spec("factory.smelter.mk1", "existing-factory").unwrap();
    existing.display_name = "actual legacy fixture factory".into();
    world
        .set_factory_construction_power_profile(runtime::FactoryConstructionPowerProfileV1 {
            factory_id: "existing-factory".into(),
            factory_kind: "fixture-custom".into(),
            source_module_id: None,
            electricity_amount: 0,
            mode: runtime::FactoryConstructionPowerMode::StartOnlySink,
            authority_revision: 1,
            active: true,
        })
        .unwrap();
    world
        .upsert_factory_profile(runtime::FactoryProfileV1 {
            factory_id: existing.factory_id.clone(),
            tier: existing.tier,
            recipe_slots: existing.recipe_slots,
            tags: existing.tags.clone(),
        })
        .unwrap();
    for m in &existing.build_cost {
        world
            .set_ledger_material_balance(runtime::MaterialLedgerId::agent(AGENT), &m.kind, m.amount)
            .unwrap();
    }
    world.submit_action(runtime::Action::BuildFactory {
        builder_agent_id: AGENT.into(),
        site_id: "resource-site".into(),
        spec: existing,
    });
    world.step().unwrap();
    world.step().unwrap();
    assert!(world.state().factories.contains_key("existing-factory"));
    let spec = runtime::builtin_factory_spec("factory.smelter.mk1", "new-factory").unwrap();
    world
        .set_factory_construction_power_profile(runtime::FactoryConstructionPowerProfileV1 {
            factory_id: "new-factory".into(),
            factory_kind: "factory.smelter.mk1".into(),
            source_module_id: None,
            electricity_amount: 7,
            mode: runtime::FactoryConstructionPowerMode::StartOnlySink,
            authority_revision: 1,
            active: true,
        })
        .unwrap();
    world
        .upsert_factory_profile(runtime::FactoryProfileV1 {
            factory_id: spec.factory_id.clone(),
            tier: spec.tier,
            recipe_slots: spec.recipe_slots,
            tags: spec.tags.clone(),
        })
        .unwrap();
    for m in &spec.build_cost {
        world
            .set_ledger_material_balance(runtime::MaterialLedgerId::agent(AGENT), &m.kind, m.amount)
            .unwrap();
    }
    world
        .set_ledger_material_balance(runtime::MaterialLedgerId::agent(AGENT), "iron_ore", 20)
        .unwrap();
    let input = world.state().factories["existing-factory"]
        .input_ledger
        .clone();
    for m in runtime::builtin_recipe_plan("recipe.smelter.iron_ingot", 1)
        .unwrap()
        .consume
    {
        world
            .set_ledger_material_balance(input.clone(), &m.kind, m.amount * 3)
            .unwrap();
    }
    world
        .set_agent_resource_balance(
            AGENT,
            oasis7_agent_api::world_action::ResourceKind::Electricity,
            1000,
        )
        .unwrap();
    for (id, object, action) in [
        (
            "transfer",
            format!("agent:{AGENT}->site:resource-site:iron_ore"),
            "TransferMaterial",
        ),
        ("build", "new-factory".into(), "BuildFactory"),
        ("schedule", "existing-factory".into(), "ScheduleRecipe"),
    ] {
        let g = grant(&world, id, object, action);
        world.install_test_native_delegation_grant(g).unwrap();
    }
    world
}
fn choices() -> Vec<wire::ResourceBasisSelectionV1> {
    vec![
        wire::ResourceBasisSelectionV1::Transfer {
            from: wire::ResourceLedgerV1::Agent {
                agent_id: AGENT.into(),
            },
            to: wire::ResourceLedgerV1::Site {
                site_id: "resource-site".into(),
            },
            material: "iron_ore".into(),
            amount: I(5),
            distance_km: I(0),
            priority: runtime::MaterialTransitPriority::Standard,
            routing: wire::ResourceRoutingV1::Direct,
        },
        wire::ResourceBasisSelectionV1::Build {
            site: "resource-site".into(),
            factory: "new-factory".into(),
            factory_kind: "factory.smelter.mk1".into(),
        },
        wire::ResourceBasisSelectionV1::Schedule {
            factory: "existing-factory".into(),
            recipe: "recipe.smelter.iron_ingot".into(),
            batches: 1,
        },
    ]
}
#[test]
fn resource_basis_full_quotes_roundtrip_actual_world_without_mutation() {
    let world = fixture();
    let before = serde_json::to_vec(&world.snapshot()).unwrap();
    let journal = world.journal().clone();
    for choice in choices() {
        let original = world
            .native_resource_basis(AGENT, &selection(&choice).unwrap())
            .unwrap();
        let wire = fact(&original, &choice).unwrap();
        let bytes = serde_json::to_vec(&wire).unwrap();
        let decoded: wire::ResourceBasisFactV1 = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(decoded, wire);
        match (&original.quote, decoded.quote) {
            (
                runtime::ResourceBasisQuoteV1::Transfer(a),
                wire::ResourceBasisQuoteV1::Transfer(b),
            ) => {
                assert_eq!(transfer_native(b.quote).unwrap(), a.quote);
                assert_eq!(
                    oasis7_agent_api::world_action::Action::from(b.action),
                    a.action
                );
                assert_eq!(
                    runtime::AgentDelegationAuthorizationV1::from(b.authorization),
                    a.authorization
                );
            }
            (runtime::ResourceBasisQuoteV1::Build(a), wire::ResourceBasisQuoteV1::Build(b)) => {
                let native: runtime::NativeBuiltinFactoryQuote = (*b).try_into().unwrap();
                assert_eq!(native, **a);
            }
            (
                runtime::ResourceBasisQuoteV1::Schedule(a),
                wire::ResourceBasisQuoteV1::Schedule(b),
            ) => {
                let native: runtime::NativeBuiltinRecipeQuote = (*b).try_into().unwrap();
                assert_eq!(native, **a);
            }
            _ => panic!("resource quote kind changed"),
        }
    }
    assert_eq!(serde_json::to_vec(&world.snapshot()).unwrap(), before);
    assert_eq!(world.journal(), &journal);
}
#[test]
fn resource_basis_old_wire_quote_cannot_replace_current_material_admission() {
    let mut world = fixture();
    let choice = choices().remove(0);
    let wire::ResourceBasisResolutionV1::Ready { fact: old } =
        resolution(&world, AGENT, &choice).unwrap()
    else {
        panic!("real positive grant")
    };
    world
        .set_ledger_material_balance(runtime::MaterialLedgerId::agent(AGENT), "iron_ore", 0)
        .unwrap();
    let before = serde_json::to_vec(&world.snapshot()).unwrap();
    let journal = world.journal().clone();
    assert!(matches!(
        resolution(&world, AGENT, &choice).unwrap(),
        wire::ResourceBasisResolutionV1::Blocked { .. }
    ));
    assert_eq!(old.selection, choice);
    assert_eq!(serde_json::to_vec(&world.snapshot()).unwrap(), before);
    assert_eq!(world.journal(), &journal);
    let mut foreign = choice;
    if let wire::ResourceBasisSelectionV1::Transfer { from, .. } = &mut foreign {
        *from = wire::ResourceLedgerV1::Agent {
            agent_id: "foreign-agent".into(),
        };
    }
    assert!(matches!(
        resolution(&world, AGENT, &foreign).unwrap(),
        wire::ResourceBasisResolutionV1::Blocked { .. }
    ));
    assert!(DecimalU64(u64::MAX).0 > 9007199254740993);
}
