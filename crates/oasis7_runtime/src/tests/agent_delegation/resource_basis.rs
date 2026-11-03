use super::*;
use crate::{ResourceBasisBlockerV1 as Blocked, ResourceBasisQuoteV1, ResourceBasisRoutingV1};
use crate::{ResourceBasisSelectionV1 as Selection, ResourceLedgerOwnerV1};

fn build() -> Selection {
    Selection::Build {
        site: "site".into(),
        factory: FACTORY.into(),
        factory_kind: KIND.into(),
    }
}

fn schedule() -> Selection {
    Selection::Schedule {
        factory: "factory".into(),
        recipe: "recipe.smelter.iron_ingot".into(),
        batches: 1,
    }
}

fn recipe_allowance() -> AgentDelegationGrantV1 {
    let mut value = grant();
    value.object_id = "factory".into();
    value.action_kinds = vec!["ScheduleRecipe".into()];
    value.limit_units = 12;
    value
}

fn assert_readonly(world: &World, selection: &Selection) -> crate::NativeResourceBasisFactV1 {
    let before = serde_json::to_value(world.snapshot()).unwrap();
    let journal = world.journal().clone();
    let fact = world.native_resource_basis(AGENT_ID, selection).unwrap();
    assert_eq!(fact.selection, *selection);
    assert_eq!(fact.quoted_at_tick, world.state().time);
    assert_eq!(fact.actor_owner.player_id, "player");
    assert_eq!(
        world.native_resource_basis(AGENT_ID, selection).unwrap(),
        fact
    );
    assert_eq!(serde_json::to_value(world.snapshot()).unwrap(), before);
    assert_eq!(world.journal(), &journal);
    fact
}

fn assert_blocked(world: &World, selection: &Selection) {
    let before = serde_json::to_value(world.snapshot()).unwrap();
    let journal = world.journal().clone();
    assert!(world.native_resource_basis(AGENT_ID, selection).is_err());
    assert_eq!(serde_json::to_value(world.snapshot()).unwrap(), before);
    assert_eq!(world.journal(), &journal);
}

#[test]
fn native_resource_basis_build_is_real_grant_backed_and_readonly() {
    let mut world = build_fixture();
    world.install_agent_delegation_grant(allowance()).unwrap();
    let fact = assert_readonly(&world, &build());
    let ResourceBasisQuoteV1::Build(value) = &fact.quote else {
        panic!("build fact")
    };
    assert_eq!(**value, quote(&world).unwrap());
    assert_eq!(value.authorization.cost_units, 7);
    assert_eq!(fact.ledgers[0].ledger, MaterialLedgerId::agent(AGENT_ID));
    assert!(fact.ledgers[0].exists);
    assert!(
        fact.ledgers[0]
            .materials
            .iter()
            .all(|value| value.amount.is_some())
    );
    assert!(
        fact.derived_resource_refs
            .iter()
            .any(|value| value == FACTORY)
    );
    assert_eq!(fact.objects.len(), 1);
}

#[test]
fn native_resource_basis_build_missing_revoked_ambiguous_and_poor_grants_block() {
    for boundary in [
        "missing",
        "revoked",
        "ambiguous",
        "budget",
        "expired",
        "source",
    ] {
        let mut world = build_fixture();
        // Every mutated case starts with a real admissible positive control.
        world.install_agent_delegation_grant(allowance()).unwrap();
        // The fixture's official material/site setters run after its first
        // tick. Publish that actual state before exercising snapshot restore.
        world.step().unwrap();
        assert_readonly(&world, &build());
        let mut snapshot = world.snapshot();
        let values = &mut snapshot.cognition["agent_delegation"]["grants"];
        match boundary {
            "missing" => {
                values.as_object_mut().unwrap().clear();
            }
            "revoked" => {
                values["grant"]["revoked"] = json!(true);
            }
            "budget" => {
                values["grant"]["limit_units"] = json!(1);
            }
            "expired" => {
                values["grant"]["valid_until_tick"] = json!(world.state().time - 1);
            }
            "source" => {
                values["grant"]["source_id"] = json!("other-owner-source");
            }
            "ambiguous" => {
                let mut other = values["grant"].clone();
                other["grant_id"] = json!("other");
                values["other"] = other;
            }
            _ => unreachable!(),
        }
        let world = World::from_snapshot(snapshot, world.journal().clone()).unwrap();
        assert_blocked(&world, &build());
    }
}

#[test]
fn native_resource_basis_recipe_preserves_actual_quote_and_ledger_owners() {
    let mut world = super::super::recipe_fixture();
    world
        .install_agent_delegation_grant(recipe_allowance())
        .unwrap();
    let fact = assert_readonly(&world, &schedule());
    let ResourceBasisQuoteV1::Schedule(value) = &fact.quote else {
        panic!("recipe fact")
    };
    assert_eq!(
        **value,
        world
            .builtin_recipe_quote(AGENT_ID, "factory", "recipe.smelter.iron_ingot", 1)
            .unwrap()
    );
    assert_eq!(value.authorization.cost_units, 8);
    assert_eq!(fact.ledgers.len(), 1);
    assert_eq!(fact.ledgers[0].ledger, MaterialLedgerId::site("site"));
    let amounts: std::collections::BTreeMap<_, _> = fact.ledgers[0]
        .materials
        .iter()
        .map(|value| (value.material.as_str(), value.amount))
        .collect();
    assert_eq!(amounts.get("iron_ore"), Some(&Some(8)));
    assert_eq!(amounts.get("carbon_fuel"), Some(&Some(2)));
    assert_eq!(amounts.get("iron_ingot"), Some(&None));
    assert_eq!(amounts.get("slag"), Some(&None));
    assert_eq!(fact.objects.len(), 1);
    let before = fact.clone();
    let input = world.state().factories["factory"].input_ledger.clone();
    world
        .set_ledger_material_balance(input, "iron_ore", 0)
        .unwrap();
    assert_blocked(&world, &schedule());
    assert_eq!(fact, before); // The archived as-of fact is never rewritten.
}

fn transfer(from: MaterialLedgerId) -> Selection {
    Selection::Transfer {
        from,
        to: MaterialLedgerId::site("site"),
        material: "iron_ore".into(),
        amount: 5,
        distance_km: 0,
        priority: MaterialTransitPriority::Standard,
        routing: ResourceBasisRoutingV1::Direct,
    }
}

fn transfer_world(from: &MaterialLedgerId) -> World {
    let mut world = build_fixture();
    if let MaterialLedgerId::Agent(agent) = from
        && agent != AGENT_ID
    {
        world.submit_action(Action::RegisterAgent {
            agent_id: agent.clone(),
            pos: super::super::super::pos(0, 0),
        });
        world.step().unwrap();
    }
    world
        .set_ledger_material_balance(from.clone(), "iron_ore", 10)
        .unwrap();
    let mut value = grant();
    value.object_id = format!("{from}->site:site:iron_ore");
    value.action_kinds = vec!["TransferMaterial".into()];
    world.install_agent_delegation_grant(value).unwrap();
    world
}

#[test]
fn native_resource_basis_site_and_factory_sources_cannot_borrow_an_agent_grant() {
    for source in [
        MaterialLedgerId::site("site"),
        MaterialLedgerId::factory("factory"),
    ] {
        let mut world = super::super::recipe_fixture();
        let mut target = world.state().factory_site_authorities["site"].clone();
        target.site_id = "target".into();
        world.set_factory_site_authority(target).unwrap();
        world
            .set_ledger_material_balance(source.clone(), "iron_ore", 10)
            .unwrap();
        let destination = MaterialLedgerId::site("target");
        let mut value = grant();
        value.object_id = format!("{source}->{destination}:iron_ore");
        value.action_kinds = vec!["TransferMaterial".into()];
        world.install_agent_delegation_grant(value).unwrap();
        assert!(
            world
                .logistics_transfer_quote(
                    AGENT_ID,
                    &source,
                    &destination,
                    "iron_ore",
                    5,
                    0,
                    Some(MaterialTransitPriority::Standard),
                )
                .unwrap()
                .submission_feasible
        );
        let selection = Selection::Transfer {
            from: source,
            to: destination,
            material: "iron_ore".into(),
            amount: 5,
            distance_km: 0,
            priority: MaterialTransitPriority::Standard,
            routing: ResourceBasisRoutingV1::Direct,
        };
        assert_eq!(
            world.native_resource_basis(AGENT_ID, &selection),
            Err(Blocked::AdmissionUnavailable)
        );
        assert_blocked(&world, &selection);
    }
}

#[test]
fn native_resource_basis_transfer_checks_real_ownership_before_logistics_quote() {
    let source = MaterialLedgerId::agent(AGENT_ID);
    let world = transfer_world(&source);
    let fact = assert_readonly(&world, &transfer(source));
    let ResourceBasisQuoteV1::Transfer(value) = &fact.quote else {
        panic!("transfer fact")
    };
    assert!(value.quote.submission_feasible);
    assert_eq!(value.quote.requested_amount, 5);
    assert_eq!(value.authorization.cost_units, 0);
    assert!(
        matches!(&fact.ledgers[1].owner, ResourceLedgerOwnerV1::Site { owner_agent_id, .. } if owner_agent_id == AGENT_ID)
    );
    assert!(!fact.ledgers[1].exists);
    assert_eq!(fact.ledgers[1].materials[0].amount, None);
    for source in [
        MaterialLedgerId::agent("foreign"),
        MaterialLedgerId::world(),
    ] {
        let world = transfer_world(&source);
        // The naked logistics projection alone is positive; it is not authority.
        assert!(
            world
                .logistics_transfer_quote(
                    AGENT_ID,
                    &source,
                    &MaterialLedgerId::site("site"),
                    "iron_ore",
                    5,
                    0,
                    Some(MaterialTransitPriority::Standard),
                )
                .unwrap()
                .submission_feasible
        );
        assert_blocked(&world, &transfer(source));
    }
}

#[test]
fn native_resource_basis_transfer_missing_or_mismatched_grant_blocks() {
    let source = MaterialLedgerId::agent(AGENT_ID);
    let mut world = transfer_world(&source);
    // Commit the fixture's actual ledger state, without editing root hashes.
    world.step().unwrap();
    assert_readonly(&world, &transfer(source.clone()));
    for boundary in ["missing", "revoked", "from", "material"] {
        let mut snapshot = world.snapshot();
        let grant = &mut snapshot.cognition["agent_delegation"]["grants"];
        match boundary {
            "missing" => grant.as_object_mut().unwrap().clear(),
            "revoked" => {
                grant["grant"]["revoked"] = json!(true);
            }
            "from" => {
                grant["grant"]["object_id"] = json!("agent:other->site:site:iron_ore");
            }
            "material" => {
                grant["grant"]["object_id"] = json!(format!("{source}->site:site:carbon_fuel"));
            }
            _ => unreachable!(),
        }
        let denied = World::from_snapshot(snapshot, world.journal().clone()).unwrap();
        assert_eq!(
            denied.native_resource_basis(AGENT_ID, &transfer(source.clone())),
            Err(Blocked::AdmissionUnavailable)
        );
    }
}

#[test]
fn native_resource_basis_route_quote_cannot_exceed_real_grant_allowance() {
    let source = MaterialLedgerId::agent(AGENT_ID);
    let mut world = build_fixture();
    world
        .set_ledger_material_balance(source.clone(), "iron_ore", 10)
        .unwrap();
    let start = world.journal().events.len();
    world.submit_action(Action::RegisterLogisticsRoute {
        requester_agent_id: AGENT_ID.into(),
        from_ledger: source.clone(),
        to_ledger: MaterialLedgerId::site("site"),
        kind: "iron_ore".into(),
        distance_km: 1,
        priority: MaterialTransitPriority::Standard,
        capacity_units: 100,
        tariff_electricity_per_unit: 2,
    });
    world.step().unwrap();
    let route = world.journal().events[start..]
        .iter()
        .find_map(|event| match &event.body {
            WorldEventBody::Domain(DomainEvent::LogisticsRouteRegistered {
                route_id: Some(id),
                ..
            }) => Some(id.clone()),
            _ => None,
        })
        .unwrap();
    let mut value = grant();
    value.object_id = format!("{source}->site:site:iron_ore");
    value.action_kinds = vec!["TransferMaterial".into()];
    value.limit_units = 10;
    world.install_agent_delegation_grant(value).unwrap();
    let selection = Selection::Transfer {
        from: source,
        to: MaterialLedgerId::site("site"),
        material: "iron_ore".into(),
        amount: 5,
        distance_km: 1,
        priority: MaterialTransitPriority::Standard,
        routing: ResourceBasisRoutingV1::Routes {
            route_ids: vec![route],
        },
    };
    let fact = assert_readonly(&world, &selection);
    let ResourceBasisQuoteV1::Transfer(quote) = fact.quote else {
        panic!("transfer")
    };
    assert_eq!(quote.authorization.cost_units, 10);
    assert_eq!(quote.quote.tariff_electricity_total, 10);
    let mut snapshot = world.snapshot();
    snapshot.cognition["agent_delegation"]["grants"]["grant"]["limit_units"] = json!(9);
    let denied = World::from_snapshot(snapshot, world.journal().clone()).unwrap();
    assert_eq!(
        denied.native_resource_basis(AGENT_ID, &selection),
        Err(Blocked::AdmissionUnavailable)
    );
}
