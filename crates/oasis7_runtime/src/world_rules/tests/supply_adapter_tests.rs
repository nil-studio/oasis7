use super::*;
use ed25519_dalek::{Signer, SigningKey};
use oasis7_agent_api::world_action::{
    GovernanceFinalityCertificate, MinimumServiceCommandV1, ResourceKind,
    minimum_service_authority_manifest,
};
fn install_finality_registry(state: &mut WorldState) -> BTreeMap<String, SigningKey> {
    let keys = BTreeMap::from([
        ("finality-a".to_owned(), SigningKey::from_bytes(&[11; 32])),
        ("finality-b".to_owned(), SigningKey::from_bytes(&[22; 32])),
    ]);
    let bindings = keys
        .iter()
        .map(|(node, key)| (node.clone(), hex::encode(key.verifying_key().to_bytes())))
        .collect::<BTreeMap<_, _>>();
    state.node_identity_bindings.extend(bindings.clone());
    state.governance_finality_signer_registry = Some(crate::GovernanceFinalitySignerRegistry {
        slot_id: "org-continuity-test".into(),
        threshold: 2,
        threshold_bps: 10_000,
        signer_bindings: bindings,
        validator_stakes: BTreeMap::from([("finality-a".into(), 1), ("finality-b".into(), 1)]),
    });
    keys
}

fn certificate(
    state: &WorldState,
    keys: &BTreeMap<String, SigningKey>,
    manifest: String,
    height: u64,
) -> GovernanceFinalityCertificate {
    let registry = state.governance_finality_signer_registry.as_ref().unwrap();
    let mut c = GovernanceFinalityCertificate {
        proposal_id: u64::from_str_radix(&manifest[..16], 16).unwrap(),
        manifest_hash: manifest,
        consensus_height: height,
        epoch_id: 7,
        validator_set_hash: crate::util::sha256_hex(
            keys.keys()
                .cloned()
                .collect::<Vec<_>>()
                .join("|")
                .as_bytes(),
        ),
        stake_root: crate::util::sha256_hex(b"finality-a:1|finality-b:1"),
        threshold_bps: registry.threshold_bps,
        min_unique_signers: registry.threshold,
        threshold: registry.threshold,
        signatures: BTreeMap::new(),
    };
    for (node, key) in keys {
        let payload = GovernanceFinalityCertificate::signing_payload_v1(
            c.proposal_id,
            &c.manifest_hash,
            c.consensus_height,
            c.epoch_id,
            &c.validator_set_hash,
            &c.stake_root,
            c.threshold_bps,
            c.effective_min_unique_signers(),
            node,
        );
        c.signatures.insert(
            node.clone(),
            format!(
                "{}{}",
                GovernanceFinalityCertificate::SIGNATURE_PREFIX_ED25519_V1,
                hex::encode(key.sign(&payload).to_bytes())
            ),
        );
    }
    c
}
fn bound_event(actor: &str, request: &str, c: WorldRuleCommandV1) -> DomainEvent {
    let mut event = event(actor, request, c);
    if let DomainEvent::WorldRuleCommitted { world_binding, .. } = &mut event {
        *world_binding = Some(supply_test_binding());
    }
    event
}
pub(super) fn registered_fixture() -> WorldState {
    let mut state = fixture();
    let keys = install_finality_registry(&mut state);
    state.agent_claims.clear();
    let mut registrations = Vec::new();
    for (actor, location) in [
        ("owner", "north"),
        ("primary", "north"),
        ("recipient", "south"),
    ] {
        state.starter_oc_claims.insert(
            actor.into(),
            crate::gameplay_state::StarterOcClaimState {
                agent_id: actor.into(),
                player_id: format!("player-{actor}"),
                public_key: Some(format!("key-{actor}")),
                claimed_at: 1,
                ..Default::default()
            },
        );
        state.location_anchors.insert(
            location.into(),
            crate::LocationAnchorV1 {
                location_id: location.into(),
                active: true,
                authority_revision: 1,
                effective_at: 1,
            },
        );
        state.agent_location_authorities.insert(
            actor.into(),
            crate::AgentLocationAuthorityV1 {
                agent_id: actor.into(),
                location_id: location.into(),
                active: true,
                authority_revision: 1,
                effective_at: 1,
            },
        );
        let current = state.agents[actor]
            .state
            .resources
            .get(ResourceKind::Electricity);
        state
            .agents
            .get_mut(actor)
            .unwrap()
            .state
            .resources
            .remove(ResourceKind::Electricity, current)
            .unwrap();
        if actor == "owner" {
            state
                .agents
                .get_mut(actor)
                .unwrap()
                .state
                .resources
                .add(ResourceKind::Electricity, 50)
                .unwrap();
        }
        let registration = Service {
            obligation_id: format!("minimum-{actor}"),
            beneficiary_id: actor.into(),
            controller_id: format!("player-{actor}"),
            region_id: "region".into(),
            location_id: location.into(),
            registered_at: 10,
            electricity_per_interval: 100,
            public_critical: false,
            recipe_ids: ["recipe.assembler.gear".into()].into(),
            source_ref: state.resolved_agent_owner(actor).unwrap().source_id,
        };
        let manifest = minimum_service_authority_manifest(
            actor,
            &registration,
            "region-root",
            &supply_test_binding(),
            10,
        )
        .unwrap();
        let c = certificate(&state, &keys, manifest, 10);
        let source = WorldRuleCommandV1::MinimumService {
            command: MinimumServiceCommandV1::Register {
                registration: registration.clone(),
                region_budget_lineage: "region-root".into(),
                certificate: c,
            },
        };
        let mut wrong = bound_event(actor, &format!("bad-{actor}"), source.clone());
        if let DomainEvent::WorldRuleCommitted { world_binding, .. } = &mut wrong {
            *world_binding = Some("wrong-world".into());
        }
        assert!(PreparedWorldRule::prepare(&state, &wrong, 10).is_err());
        apply(
            &mut state,
            &bound_event(actor, &format!("source-{actor}"), source),
            10,
        );
        registrations.push((actor, registration));
    }
    for (actor, registration) in registrations {
        let consume = command(C::RegisterService {
            proposal_key: format!("consume-{actor}"),
            registration,
        });
        approve(&mut state, actor, &format!("consume-{actor}"), &consume);
        assert!(
            PreparedWorldRule::prepare(
                &state,
                &event(actor, &format!("consume-{actor}"), consume.clone()),
                609
            )
            .is_err()
        );
        apply(
            &mut state,
            &event(actor, &format!("consume-{actor}"), consume),
            610,
        );
    }
    state
}
#[test]
fn authority_minimum_service_producer_canonical_observation_and_local_restricted_delivery() {
    let mut state = registered_fixture();
    for tick in [720, 780, 840] {
        let c = command(C::Observe {
            region_id: "region".into(),
        });
        assert!(
            PreparedWorldRule::prepare(&state, &event("owner", "client-observe", c.clone()), tick)
                .is_err()
        );
        let mut observation = event("owner", &format!("observation-{tick}"), c);
        if let DomainEvent::WorldRuleCommitted { action_id, .. } = &mut observation {
            *action_id = 0;
        }
        apply(&mut state, &observation, tick);
    }
    let observations = state
        .world_rules
        .as_ref()
        .unwrap()
        .emergency_supply
        .observations
        .values()
        .cloned()
        .collect::<Vec<_>>();
    let mut observations = observations;
    observations.sort_by_key(|w| w.window_start);
    let proof = Evidence {
        region_id: "region".into(),
        region_budget_lineage: "region-root".into(),
        source_ref: format!(
            "observations:{}",
            crate::util::hash_json(&observations).unwrap()
        ),
        observations,
    };
    assert!(
        !state
            .world_rules
            .as_ref()
            .unwrap()
            .emergency_supply
            .evidence
            .is_empty()
    );
    let mut forged = proof.clone();
    *forged.observations[0]
        .normal_coverage
        .get_mut("minimum-owner")
        .unwrap() += 1;
    forged.source_ref = format!(
        "observations:{}",
        crate::util::hash_json(&forged.observations).unwrap()
    );
    let counterfeit = command(C::PublishEvidence {
        proposal_key: "forged-proof".into(),
        evidence: forged,
    });
    approve(&mut state, "owner", "forged-proof", &counterfeit);
    let unchanged = state.clone();
    assert!(
        PreparedWorldRule::prepare(&state, &event("owner", "forged-proof", counterfeit), 840)
            .is_err()
    );
    assert_eq!(state, unchanged);
    let c = command(C::PublishEvidence {
        proposal_key: "proof".into(),
        evidence: proof,
    });
    approve(&mut state, "owner", "proof", &c);
    apply(&mut state, &event("owner", "proof", c), 840);
    let c = command(C::Authorize {
        proposal_key: "auth".into(),
        evidence_id: request_key("owner", "proof"),
        reviewer_id: "recipient".into(),
        starts_at: 840,
        expires_at: 1440,
        quantity: 9,
    });
    approve(&mut state, "owner", "auth", &c);
    apply(&mut state, &event("owner", "auth", c), 840);
    let auth = request_key("owner", "auth");
    apply(
        &mut state,
        &event(
            "primary",
            "need",
            command(C::Request {
                authorization_id: auth.clone(),
                needs: vec![oasis7_agent_api::world_action::EmergencyNeedV1 {
                    obligation_id: "minimum-primary".into(),
                    interval_start: 960,
                }],
            }),
        ),
        850,
    );
    apply(
        &mut state,
        &event(
            "owner",
            "freeze",
            command(C::Freeze {
                authorization_id: auth.clone(),
            }),
        ),
        900,
    );
    let batch = request_key("owner", "freeze");
    apply(
        &mut state,
        &event(
            "owner",
            "allocate",
            command(C::Allocate {
                batch_id: batch.clone(),
            }),
        ),
        900,
    );
    let need = state.world_rules.as_ref().unwrap().emergency_supply.batches[&batch]
        .allocation
        .keys()
        .next()
        .unwrap()
        .clone();
    let delivery = event(
        "owner",
        "deliver",
        command(C::Deliver {
            batch_id: batch,
            need_id: need.clone(),
            quantity: 9,
        }),
    );
    let mut drifted = state.clone();
    drifted
        .agent_location_authorities
        .get_mut("primary")
        .unwrap()
        .location_id = "south".into();
    assert!(PreparedWorldRule::prepare(&drifted, &delivery, 900).is_err());
    apply(&mut state, &delivery, 900);
    apply(&mut state, &delivery, 901);
    assert_eq!(
        state.agents["primary"]
            .state
            .resources
            .get(ResourceKind::Electricity),
        0
    );
    let rules = &state.world_rules.as_ref().unwrap().emergency_supply;
    assert_eq!(rules.needs[&need].fulfilled, 9);
    assert_eq!(rules.restricted_power[&need].remaining, 9);
    let consumed = rules
        .prepare_consumption(
            "primary",
            "minimum-primary",
            "recipe.assembler.gear",
            960,
            9,
        )
        .unwrap();
    assert_eq!(consumed.restricted_power[&need].remaining, 0);
    let factory = state.factories.get_mut("assembly").unwrap();
    factory.builder_agent_id = "primary".into();
    factory.input_ledger = MaterialLedgerId::agent("primary");
    factory.output_ledger = MaterialLedgerId::agent("primary");
    factory.site_location_id = Some("north".into());
    factory.site_authority_revision = Some(1);
    factory.location_anchor_revision = Some(1);
    state.material_ledgers.insert(
        MaterialLedgerId::agent("primary"),
        [
            ("iron_ingot".into(), 8),
            ("copper_wire".into(), 8),
            ("polymer_resin".into(), 4),
        ]
        .into(),
    );
    state.time = 959;
    let essential = |recipe: &str, obligation: &str| Action::ScheduleEssentialRecipe {
        requester_agent_id: "primary".into(),
        factory_id: "assembly".into(),
        recipe_id: recipe.into(),
        batches: 1,
        obligation_id: obligation.into(),
    };
    for (recipe, obligation, time, move_location) in [
        (
            "recipe.assembler.control_chip",
            "minimum-primary",
            959,
            false,
        ),
        ("recipe.assembler.gear", "unknown-obligation", 959, false),
        ("recipe.assembler.gear", "minimum-primary", 1019, false),
        ("recipe.assembler.gear", "minimum-primary", 959, true),
    ] {
        let mut candidate = state.clone();
        candidate.time = time;
        if move_location {
            candidate
                .agent_location_authorities
                .get_mut("primary")
                .unwrap()
                .location_id = "south".into();
        }
        let mut world = supply_test_world(candidate, "supply-test-world");
        world.submit_action(essential(recipe, obligation));
        world.step().unwrap();
        assert!(world.state().pending_recipe_jobs.is_empty());
        assert_eq!(
            world
                .state()
                .world_rules
                .as_ref()
                .unwrap()
                .emergency_supply
                .restricted_power[&need]
                .remaining,
            9
        );
        assert_eq!(
            world.state().material_ledgers[&MaterialLedgerId::agent("primary")]["iron_ingot"],
            8
        );
    }
    for revoke_anchor in [true, false] {
        let mut candidate = state.clone();
        if revoke_anchor {
            candidate.location_anchors.get_mut("north").unwrap().active = false;
        } else {
            candidate
                .starter_oc_claims
                .get_mut("primary")
                .unwrap()
                .player_id = "player-owner".into();
        }
        let mut world = supply_test_world(candidate, "supply-test-world");
        world.submit_action(essential("recipe.assembler.gear", "minimum-primary"));
        world.step().unwrap();
        assert!(world.state().pending_recipe_jobs.is_empty());
        assert_eq!(
            world
                .state()
                .world_rules
                .as_ref()
                .unwrap()
                .emergency_supply
                .restricted_power[&need]
                .remaining,
            9
        );
    }
    let mut moved = supply_test_world(state.clone(), "another-world");
    moved.submit_action(essential("recipe.assembler.gear", "minimum-primary"));
    moved.step().unwrap();
    assert!(moved.state().pending_recipe_jobs.is_empty());
    assert_eq!(
        moved
            .state()
            .world_rules
            .as_ref()
            .unwrap()
            .emergency_supply
            .restricted_power[&need]
            .remaining,
        9
    );
    let mut world = supply_test_world(state.clone(), "supply-test-world");
    world.submit_action(essential("recipe.assembler.gear", "minimum-primary"));
    world.step().unwrap();
    assert_eq!(
        world.state().pending_recipe_jobs.len(),
        1,
        "{:?}",
        world.journal().events
    );
    assert_eq!(
        world.state().agents["primary"]
            .state
            .resources
            .get(ResourceKind::Electricity),
        0
    );
    assert_eq!(
        world
            .state()
            .world_rules
            .as_ref()
            .unwrap()
            .emergency_supply
            .restricted_power[&need]
            .remaining,
        5
    );
    let start = world
        .journal()
        .events
        .iter()
        .find_map(|e| match &e.body {
            crate::WorldEventBody::Domain(d @ DomainEvent::RecipeStarted { .. }) => Some(d.clone()),
            _ => None,
        })
        .unwrap();
    let mut moved_raw = supply_test_world(state.clone(), "another-world");
    let before_moved_raw = moved_raw.snapshot();
    assert!(
        moved_raw
            .append_event_for_test(crate::WorldEventBody::Domain(start.clone()), None)
            .is_err()
    );
    assert_eq!(moved_raw.snapshot(), before_moved_raw);
    let projected = {
        let prepared =
            crate::state::industry_transition::PreparedIndustryEvent::prepare(&state, &start, 960)
                .unwrap();
        let mut projection = WorldStateProjection::borrowed(&state);
        projection.industry_overlay = Some(&prepared);
        serde_json::to_value(projection).unwrap()
    };
    let mut direct = state.clone();
    direct.apply_domain_event(&start, 960).unwrap();
    direct.route_domain_event(&start);
    assert_eq!(projected, serde_json::to_value(&direct).unwrap());
    let before_duplicate = direct.clone();
    assert!(direct.apply_domain_event(&start, 960).is_err());
    assert_eq!(direct, before_duplicate);
    let job = world
        .state()
        .pending_recipe_jobs
        .values()
        .next()
        .unwrap()
        .clone();
    assert_eq!(
        job.emergency_power_world_binding.as_ref(),
        Some(&supply_test_binding())
    );
    let mut moved_wip = supply_test_world(world.state().clone(), "another-world");
    while world.state().time < job.ready_at {
        world.step().unwrap();
        moved_wip.step().unwrap();
    }
    assert!(world.state().settled_recipe_job_ids.contains(&job.job_id));
    assert!(
        moved_wip
            .state()
            .pending_recipe_jobs
            .contains_key(&job.job_id)
    );
    assert!(
        !moved_wip
            .state()
            .settled_recipe_job_ids
            .contains(&job.job_id)
    );
    let completion =
        world
            .journal()
            .events
            .iter()
            .find_map(|event| match &event.body {
                crate::WorldEventBody::Domain(
                    event @ DomainEvent::RecipeCompleted { job_id, .. },
                ) if *job_id == job.job_id => Some(event.clone()),
                _ => None,
            })
            .unwrap();
    let before_moved_completion = moved_wip.snapshot();
    assert!(
        moved_wip
            .append_event_for_test(crate::WorldEventBody::Domain(completion), None)
            .is_err()
    );
    assert_eq!(moved_wip.snapshot(), before_moved_completion);
    assert_eq!(
        world.state().material_ledgers[&MaterialLedgerId::agent("primary")]["gear"],
        1
    );
    assert_eq!(
        world
            .state()
            .world_rules
            .as_ref()
            .unwrap()
            .emergency_supply
            .restricted_power[&need]
            .remaining,
        5
    );
    assert_eq!(
        state.agents["owner"]
            .state
            .resources
            .get(ResourceKind::Electricity),
        41
    );
}

pub(super) fn supply_test_binding() -> String {
    crate::util::hash_json(&(
        "oasis7-world-rule-world-v1",
        "supply-test-world",
        "supply-test-chain",
        "supply-test-genesis",
    ))
    .unwrap()
}
fn supply_test_world(state: WorldState, world_id: &str) -> World {
    let world = World::new_with_state(state);
    let snapshot = world.snapshot_with_chain_resource_context(
        crate::ChainResourceDerivationContext {
            world_id,
            chain_id: "supply-test-chain",
            genesis_ref: Some("supply-test-genesis"),
            created_at_height: 0,
            manifest_height: 0,
            commit_block_hash: None,
            tick: world.state().time,
        },
        "a".repeat(64),
        "b".repeat(64),
    );
    World::from_snapshot(snapshot, world.journal().clone()).unwrap()
}

#[test]
fn emergency_commands_cannot_inherit_another_world_registration() {
    let state = registered_fixture();
    let mut event = bound_event(
        "owner",
        "cross-world-observe",
        WorldRuleCommandV1::EmergencySupply {
            command: oasis7_agent_api::world_action::EmergencySupplyCommandV1::Observe {
                region_id: "region".into(),
            },
        },
    );
    if let DomainEvent::WorldRuleCommitted {
        world_binding,
        action_id,
        ..
    } = &mut event
    {
        *world_binding = Some("another-world-binding".into());
        *action_id = 0;
    }
    let error = PreparedWorldRule::prepare(&state, &event, 60).unwrap_err();
    assert!(format!("{error:?}").contains("emergency_supply_world_binding_mismatch"));
}

fn observed_authorization_fixture(starts_at: u64) -> (WorldState, WorldRuleCommandV1) {
    let mut state = registered_fixture();
    for tick in [720, 780, 840] {
        let mut e = event(
            "owner",
            &format!("fixture-observe-{tick}"),
            command(C::Observe {
                region_id: "region".into(),
            }),
        );
        if let DomainEvent::WorldRuleCommitted { action_id, .. } = &mut e {
            *action_id = 0;
        }
        apply(&mut state, &e, tick);
    }
    let evidence_id = state
        .world_rules
        .as_ref()
        .unwrap()
        .emergency_supply
        .evidence
        .keys()
        .next()
        .unwrap()
        .clone();
    let authorization = command(C::Authorize {
        proposal_key: "fixture-auth".into(),
        evidence_id,
        reviewer_id: "recipient".into(),
        starts_at,
        expires_at: starts_at + 600,
        quantity: 9,
    });
    approve(&mut state, "owner", "fixture-auth", &authorization);
    state.time = 840;
    (state, authorization)
}
pub(super) fn authorized_preparation_fixture() -> (World, Action) {
    let (state, command) = observed_authorization_fixture(900);
    (
        supply_test_world(state, "supply-test-world"),
        Action::WorldRule {
            actor_id: "owner".into(),
            request_id: "fixture-auth".into(),
            command,
        },
    )
}
#[cfg(feature = "wasmtime")]
pub(super) fn essential_preparation_fixture() -> (World, Action) {
    let (mut state, authorization) = observed_authorization_fixture(840);
    apply(
        &mut state,
        &event("owner", "fixture-auth", authorization),
        840,
    );
    let auth = request_key("owner", "fixture-auth");
    apply(
        &mut state,
        &event(
            "primary",
            "fixture-need",
            command(C::Request {
                authorization_id: auth.clone(),
                needs: vec![oasis7_agent_api::world_action::EmergencyNeedV1 {
                    obligation_id: "minimum-primary".into(),
                    interval_start: 960,
                }],
            }),
        ),
        850,
    );
    apply(
        &mut state,
        &event(
            "owner",
            "fixture-freeze",
            command(C::Freeze {
                authorization_id: auth,
            }),
        ),
        900,
    );
    let batch = request_key("owner", "fixture-freeze");
    apply(
        &mut state,
        &event(
            "owner",
            "fixture-allocate",
            command(C::Allocate {
                batch_id: batch.clone(),
            }),
        ),
        900,
    );
    let need = state.world_rules.as_ref().unwrap().emergency_supply.batches[&batch]
        .allocation
        .keys()
        .next()
        .unwrap()
        .clone();
    apply(
        &mut state,
        &event(
            "owner",
            "fixture-deliver",
            command(C::Deliver {
                batch_id: batch,
                need_id: need,
                quantity: 9,
            }),
        ),
        900,
    );
    let factory = state.factories.get_mut("assembly").unwrap();
    factory.builder_agent_id = "primary".into();
    factory.input_ledger = MaterialLedgerId::agent("primary");
    factory.output_ledger = MaterialLedgerId::agent("primary");
    factory.site_location_id = Some("north".into());
    factory.site_authority_revision = Some(1);
    factory.location_anchor_revision = Some(1);
    state.material_ledgers.insert(
        MaterialLedgerId::agent("primary"),
        [
            ("iron_ingot".into(), 8),
            ("copper_wire".into(), 8),
            ("polymer_resin".into(), 4),
        ]
        .into(),
    );
    state.time = 959;
    (
        supply_test_world(state, "supply-test-world"),
        Action::ScheduleEssentialRecipe {
            requester_agent_id: "primary".into(),
            factory_id: "assembly".into(),
            recipe_id: "recipe.assembler.gear".into(),
            batches: 1,
            obligation_id: "minimum-primary".into(),
        },
    )
}
