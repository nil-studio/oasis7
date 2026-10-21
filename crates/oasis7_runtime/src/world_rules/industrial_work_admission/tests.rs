use super::*;
use crate::DomainEvent;
use ed25519_dalek::{Signer, SigningKey};
use oasis7_wasm_abi::SubstitutionProfileRefV1;

pub fn fixture() -> (WorldState, SigningKey) {
    let mut world = crate::World::new();
    world.submit_action(crate::Action::RegisterAgent {
        agent_id: "owner".into(),
        pos: GeoPos::new(0, 0, 0),
    });
    world.step().unwrap();
    let mut state = world.state().clone();
    state.starter_oc_claims.insert(
        "owner".into(),
        crate::gameplay_state::StarterOcClaimState {
            agent_id: "owner".into(),
            player_id: "player".into(),
            public_key: Some("credential".into()),
            claimed_at: 1,
            ..Default::default()
        },
    );
    state.material_ledgers.insert(
        MaterialLedgerId::agent("owner"),
        BTreeMap::from([("iron_ore".into(), 20), ("carbon_fuel".into(), 20)]),
    );
    state
        .agents
        .get_mut("owner")
        .unwrap()
        .state
        .resources
        .set(ResourceKind::Electricity, 100)
        .unwrap();
    let key = SigningKey::from_bytes(&[31; 32]);
    let public = hex::encode(key.verifying_key().to_bytes());
    state
        .node_identity_bindings
        .insert("validator".into(), public.clone());
    state.governance_finality_signer_registry = Some(crate::GovernanceFinalitySignerRegistry {
        slot_id: "industrial".into(),
        threshold: 1,
        threshold_bps: 10_000,
        signer_bindings: BTreeMap::from([("validator".into(), public)]),
        validator_stakes: BTreeMap::from([("validator".into(), 1)]),
    });
    (state, key)
}
pub fn certificate(manifest: String, key: &SigningKey) -> GovernanceFinalityCertificate {
    let mut c = GovernanceFinalityCertificate {
        proposal_id: 1,
        manifest_hash: manifest,
        consensus_height: 1,
        epoch_id: 1,
        validator_set_hash: crate::util::sha256_hex(b"validator"),
        stake_root: crate::util::sha256_hex(b"validator:1"),
        threshold_bps: 10_000,
        min_unique_signers: 1,
        threshold: 1,
        signatures: BTreeMap::new(),
    };
    let payload = GovernanceFinalityCertificate::signing_payload_v1(
        c.proposal_id,
        &c.manifest_hash,
        c.consensus_height,
        c.epoch_id,
        &c.validator_set_hash,
        &c.stake_root,
        c.threshold_bps,
        1,
        "validator",
    );
    c.signatures.insert(
        "validator".into(),
        format!(
            "{}{}",
            GovernanceFinalityCertificate::SIGNATURE_PREFIX_ED25519_V1,
            hex::encode(key.sign(&payload).to_bytes())
        ),
    );
    c
}
pub fn facts(state: &WorldState) -> QualifiedIndustrialLotFactsV1 {
    QualifiedIndustrialLotFactsV1 {
        world_binding: "world".into(),
        owner_id: "owner".into(),
        owner_binding_digest: industrial_owner_binding(state, "owner").unwrap(),
        source_ledger: MaterialLedgerId::agent("owner"),
        material: "iron_ore".into(),
        material_profile: SubstitutionProfileRefV1 {
            id: "ore-profile".into(),
            version: "1".into(),
            digest: crate::util::sha256_hex(b"profile"),
        },
        unit: "mass".into(),
        spec: "ore-spec".into(),
        quality: "qualified".into(),
        custody: "sealed".into(),
        provenance_ref: industrial_ref(
            "world",
            "material",
            "qualification/v1",
            "source-lot",
            1,
            &"qualified",
        )
        .unwrap(),
        quantity: 10,
        escrow_capacity: 20,
        expires_at: 10,
        consensus_height: 1,
    }
}
pub fn import(
    _state: &WorldState,
    key: &SigningKey,
    request: &str,
    facts: QualifiedIndustrialLotFactsV1,
) -> DomainEvent {
    let manifest = industrial_import_manifest(
        "world",
        "owner",
        &super::super::request_key("owner", request),
        &facts,
    )
    .unwrap();
    event(
        request,
        IndustrialWorkCommandV1::ImportQualifiedLot {
            facts: Box::new(facts),
            certificate: certificate(manifest, key),
        },
    )
}
pub fn event(request: &str, command: IndustrialWorkCommandV1) -> DomainEvent {
    DomainEvent::WorldRuleCommitted {
        action_id: 1,
        actor_id: "owner".into(),
        request_id: request.into(),
        command: WorldRuleCommandV1::IndustrialWork {
            command: Box::new(command),
        },
        governance_epoch: 0,
        world_binding: Some("world".into()),
    }
}
#[test]
fn certified_import_escrows_existing_balance_and_survives_restore() {
    let (mut state, key) = fixture();
    let ev = import(&state, &key, "import", facts(&state));
    let prepared = PreparedWorldRule::prepare(&state, &ev, 1).unwrap();
    assert_eq!(
        state.material_ledgers[&MaterialLedgerId::agent("owner")]["iron_ore"],
        20
    );
    prepared.install(&mut state);
    assert_eq!(
        state.material_ledgers[&MaterialLedgerId::agent("owner")]["iron_ore"],
        10
    );
    let encoded = serde_json::to_vec(&state).unwrap();
    let mut restored: WorldState = serde_json::from_slice(&encoded).unwrap();
    PreparedWorldRule::prepare(&restored, &ev, 2)
        .unwrap()
        .install(&mut restored);
    assert_eq!(
        restored.material_ledgers[&MaterialLedgerId::agent("owner")]["iron_ore"],
        10
    );
    assert_eq!(
        restored
            .world_rules
            .as_ref()
            .unwrap()
            .industrial_work
            .lots
            .len(),
        1
    );
    let repeated = import(&restored, &key, "new-key", facts(&state));
    assert!(PreparedWorldRule::prepare(&restored, &repeated, 2).is_err());
}
#[test]
fn import_rejects_bad_authority_without_minting() {
    let (state, key) = fixture();
    for mutation in 0..6 {
        let mut f = facts(&state);
        match mutation {
            0 => f.world_binding = "foreign".into(),
            1 => f.owner_id = "other".into(),
            2 => f.source_ledger = MaterialLedgerId::world(),
            3 => f.quality.clear(),
            4 => f.quantity = 21,
            _ => f.expires_at = 0,
        }
        let ev = import(&state, &key, "bad", f);
        assert!(PreparedWorldRule::prepare(&state, &ev, 1).is_err());
    }
    let mut wrong_signer = import(&state, &key, "bad-signature", facts(&state));
    if let DomainEvent::WorldRuleCommitted {
        command: WorldRuleCommandV1::IndustrialWork { command },
        ..
    } = &mut wrong_signer
        && let IndustrialWorkCommandV1::ImportQualifiedLot { certificate, .. } = command.as_mut()
    {
        certificate.signatures.clear();
    }
    assert!(PreparedWorldRule::prepare(&state, &wrong_signer, 1).is_err());
    assert_eq!(
        state.material_ledgers[&MaterialLedgerId::agent("owner")]["iron_ore"],
        20
    );
    assert!(
        state
            .world_rules
            .as_ref()
            .is_none_or(|r| r.industrial_work.lots.is_empty())
    );
}
#[test]
fn release_credits_only_remaining_once_and_rejects_owner_drift() {
    let (mut state, key) = fixture();
    let ev = import(&state, &key, "import", facts(&state));
    PreparedWorldRule::prepare(&state, &ev, 1)
        .unwrap()
        .install(&mut state);
    let lot_ref = state
        .world_rules
        .as_ref()
        .unwrap()
        .industrial_work
        .lots
        .values()
        .next()
        .unwrap()
        .lot_ref
        .clone();
    let release = event(
        "release",
        IndustrialWorkCommandV1::ReleaseLot {
            lot_ref: lot_ref.clone(),
        },
    );
    let mut drifted = state.clone();
    drifted
        .starter_oc_claims
        .get_mut("owner")
        .unwrap()
        .public_key = Some("new-credential".into());
    assert!(PreparedWorldRule::prepare(&drifted, &release, 2).is_err());
    PreparedWorldRule::prepare(&state, &release, 2)
        .unwrap()
        .install(&mut state);
    assert_eq!(
        state.material_ledgers[&MaterialLedgerId::agent("owner")]["iron_ore"],
        20
    );
    PreparedWorldRule::prepare(&state, &release, 3)
        .unwrap()
        .install(&mut state);
    assert!(
        PreparedWorldRule::prepare(
            &state,
            &event(
                "release-again",
                IndustrialWorkCommandV1::ReleaseLot { lot_ref }
            ),
            3
        )
        .is_err()
    );
    assert_eq!(
        state.material_ledgers[&MaterialLedgerId::agent("owner")]["iron_ore"],
        20
    );
}
#[test]
fn certified_root_uses_canonical_inputs_without_sink_and_rejects_stale_baseline() {
    let (state, signing_key, mut command) = root_fixture();
    let _ = signing_key;
    let ev = event("root", command.clone());
    let prepared = PreparedWorldRule::prepare(&state, &ev, 1).unwrap();
    assert_eq!(prepared.rules.industrial_work.roots.len(), 1);
    let root = prepared
        .rules
        .industrial_work
        .roots
        .values()
        .next()
        .unwrap();
    assert_eq!(root.work, 0);
    assert!(!root.first_sink);
    assert_eq!(root.slots["iron_ore"].quantity, 4);
    assert_eq!(root.slots["carbon_fuel"].quantity, 1);
    assert!(
        root.slots
            .values()
            .all(|slot| slot.parents.is_empty() && !slot.consumed)
    );
    assert!(prepared.ledgers.is_empty());
    assert!(PreparedWorldRule::prepare(&state, &ev, 2).is_err());
    if let IndustrialWorkCommandV1::AcceptRoot {
        input_baselines, ..
    } = &mut command
    {
        input_baselines.remove("iron_ore");
    }
    assert!(PreparedWorldRule::prepare(&state, &event("root", command), 1).is_err());
}

pub fn root_fixture() -> (WorldState, SigningKey, IndustrialWorkCommandV1) {
    let (mut state, signing_key) = fixture();
    let recipe = "recipe.smelter.iron_ingot";
    state.recipe_profiles.insert(
        recipe.into(),
        oasis7_wasm_abi::RecipeProfileV1 {
            recipe_id: recipe.into(),
            substitution_rules: vec![],
            bottleneck_tags: vec![],
            stage_gate: "bootstrap".into(),
            preferred_factory_tags: vec![],
        },
    );
    state.factories.insert(
        "smelter".into(),
        crate::FactoryState {
            factory_id: "smelter".into(),
            site_id: "site".into(),
            builder_agent_id: "owner".into(),
            spec: crate::builtin_factory_spec("factory.smelter.mk1", "smelter").unwrap(),
            input_ledger: MaterialLedgerId::agent("owner"),
            output_ledger: MaterialLedgerId::agent("owner"),
            durability_ppm: 1_000_000,
            production: Default::default(),
            site_authority_revision: None,
            site_location_id: None,
            location_anchor_revision: None,
            construction_power_profile_key: None,
            construction_power_profile_revision: None,
            built_at: 0,
        },
    );
    let p = SubstitutionProfileRefV1 {
        id: "authority-profile".into(),
        version: "1".into(),
        digest: crate::util::sha256_hex(b"authority-profile"),
    };
    let impacts = oasis7_wasm_abi::SubstitutionImpactV1 {
        outputs: p.clone(),
        quality: p.clone(),
        power: p.clone(),
        time: p.clone(),
        byproducts: p.clone(),
        terminal: p.clone(),
        value_class: p.clone(),
        logistics: p.clone(),
        buffer: p.clone(),
    };
    let plan = crate::builtin_recipe_plan(recipe, 1).unwrap();
    let input_baselines = plan
        .consume
        .iter()
        .map(|stack| {
            (
                stack.kind.clone(),
                IndustrialInputBaselineV1 {
                    material_profile: p.clone(),
                    unit: "mass".into(),
                    spec: "qualified-spec".into(),
                    quality: "qualified-quality".into(),
                    custody: "qualified-custody".into(),
                },
            )
        })
        .collect();
    let authority = IndustrialBaselineAuthorityV1 {
        world_binding: "world".into(),
        owner_binding_digest: industrial_owner_binding(&state, "owner").unwrap(),
        state_digest: crate::util::hash_json(&state).unwrap(),
        profile_digest: crate::util::hash_json(&state.recipe_profiles[recipe]).unwrap(),
        plan_digest: crate::util::hash_json(&plan).unwrap(),
        baseline_impacts: impacts,
        allowed_dispositions: vec![IndustrialDispositionV1::Terminate],
        terminal_capacities: BTreeMap::from([("iron_ingot".into(), 100), ("slag".into(), 100)]),
        quoted_at: 1,
        expires_at: 3,
        consensus_height: 1,
    };
    let mut command = IndustrialWorkCommandV1::AcceptRoot {
        factory_id: "smelter".into(),
        recipe_id: recipe.into(),
        batches: 1,
        input_baselines,
        authority: Box::new(authority),
        certificate: certificate("pending".into(), &signing_key),
    };
    let key = super::super::request_key("owner", "root");
    let manifest = industrial_root_manifest("world", "owner", &key, &command, &state).unwrap();
    if let IndustrialWorkCommandV1::AcceptRoot { certificate: c, .. } = &mut command {
        *c = certificate(manifest, &signing_key);
    }
    (state, signing_key, command)
}

#[test]
fn certified_site_escrow_requires_current_explicit_site_access() {
    let (mut state, key) = fixture();
    let ledger = MaterialLedgerId::site("site");
    state.factory_site_authorities.insert(
        "site".into(),
        crate::FactorySiteAuthorityV1 {
            site_id: "site".into(),
            location_id: "location".into(),
            owner_agent_id: "owner".into(),
            authorized_agent_ids: vec![],
            chunk_ready: true,
            active: true,
            authority_revision: 1,
            registered_at: 1,
        },
    );
    state
        .material_ledgers
        .insert(ledger.clone(), BTreeMap::from([("iron_ore".into(), 20)]));
    let mut f = facts(&state);
    f.source_ledger = ledger.clone();
    let ev = import(&state, &key, "site-import", f);
    let mut inactive = state.clone();
    inactive
        .factory_site_authorities
        .get_mut("site")
        .unwrap()
        .active = false;
    assert!(PreparedWorldRule::prepare(&inactive, &ev, 1).is_err());
    PreparedWorldRule::prepare(&state, &ev, 1)
        .unwrap()
        .install(&mut state);
    assert_eq!(state.material_ledgers[&ledger]["iron_ore"], 10);
    let lot_ref = state
        .world_rules
        .as_ref()
        .unwrap()
        .industrial_work
        .lots
        .values()
        .next()
        .unwrap()
        .lot_ref
        .clone();
    state
        .factory_site_authorities
        .get_mut("site")
        .unwrap()
        .active = false;
    assert!(
        PreparedWorldRule::prepare(
            &state,
            &event(
                "release-site",
                IndustrialWorkCommandV1::ReleaseLot { lot_ref }
            ),
            2
        )
        .is_err()
    );
    assert_eq!(state.material_ledgers[&ledger]["iron_ore"], 10);
}

fn bound_root_fixture() -> (WorldState, RootOperationV1) {
    bound_root_fixture_with_site(false)
}

fn bound_root_fixture_with_site(site_output: bool) -> (WorldState, RootOperationV1) {
    let (mut state, key, mut command) = root_fixture();
    if site_output {
        let ledger = MaterialLedgerId::site("terminal-site");
        state.factory_site_authorities.insert(
            "terminal-site".into(),
            crate::FactorySiteAuthorityV1 {
                site_id: "terminal-site".into(),
                location_id: "terminal-location".into(),
                owner_agent_id: "owner".into(),
                authorized_agent_ids: vec![],
                chunk_ready: true,
                active: true,
                authority_revision: 1,
                registered_at: 1,
            },
        );
        state
            .material_ledgers
            .insert(ledger.clone(), BTreeMap::new());
        state.factories.get_mut("smelter").unwrap().output_ledger = ledger;
        if let IndustrialWorkCommandV1::AcceptRoot { authority, .. } = &mut command {
            authority.state_digest = crate::util::hash_json(&state).unwrap();
        }
        let request_key = crate::world_rules::request_key("owner", "root");
        let manifest =
            industrial_root_manifest("world", "owner", &request_key, &command, &state).unwrap();
        if let IndustrialWorkCommandV1::AcceptRoot {
            certificate: value, ..
        } = &mut command
        {
            *value = certificate(manifest, &key);
        }
    }
    let root_event = event("root", command);
    PreparedWorldRule::prepare(&state, &root_event, 1)
        .unwrap()
        .install(&mut state);
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
    for (material, slot) in &root.slots {
        let mut f = facts(&state);
        f.material = material.clone();
        f.material_profile = slot.material_profile.clone();
        f.quantity = slot.quantity;
        f.unit = slot.unit.clone();
        f.spec = slot.spec.clone();
        f.quality = slot.quality.clone();
        f.custody = slot.custody.clone();
        f.provenance_ref = industrial_ref(
            "world",
            "material",
            "qualification/v1",
            material,
            1,
            material,
        )
        .unwrap();
        let request = format!("import-{material}");
        let import_event = import(&state, &key, &request, f);
        PreparedWorldRule::prepare(&state, &import_event, 1)
            .unwrap()
            .install(&mut state);
        let lot = state
            .world_rules
            .as_ref()
            .unwrap()
            .industrial_work
            .lots
            .values()
            .find(|lot| lot.material == *material)
            .unwrap()
            .clone();
        // The core binding helper is internal: real substitution routes resolve
        // a persisted, explicitly scoped decision before reaching this boundary.
        crate::world_rules::industrial_work::apply_parent_binding(
            &mut state.world_rules.as_mut().unwrap().industrial_work,
            ParentBindingRevisionV1 {
                root_ref: root.root_ref.clone(),
                slot_id: material.clone(),
                expected_revision: 1,
                decision_ref: industrial_ref(
                    "world",
                    "industrial",
                    "test-decision/v1",
                    material,
                    1,
                    material,
                )
                .unwrap(),
                parents: vec![IndustrialParentV1 {
                    lot_ref: lot.lot_ref,
                    settlement_ref: lot.provenance_ref,
                    quantity: slot.quantity,
                }],
            },
            "world",
        )
        .unwrap();
    }
    let bound_root =
        state.world_rules.as_ref().unwrap().industrial_work.roots[&root.root_ref.id].clone();
    assert!(
        bound_root
            .slots
            .values()
            .all(|slot| !slot.parents.is_empty() && slot.binding_revision == 2)
    );
    (state, bound_root)
}

pub fn started_root_fixture() -> (WorldState, IndustrialRefV1, u64) {
    let (mut state, root) = bound_root_fixture();
    let start = event(
        "start",
        IndustrialWorkCommandV1::StartRoot {
            root_ref: root.root_ref.clone(),
            expected_profile_digest: root.pinned_profile_digest,
            logistics_route_ids: vec![],
            logistics_path_ids: vec![],
        },
    );
    PreparedWorldRule::prepare(&state, &start, 1)
        .unwrap()
        .install(&mut state);
    let job_id = state.world_rules.as_ref().unwrap().industrial_work.roots[&root.root_ref.id]
        .started_job_id
        .unwrap();
    (state, root.root_ref, job_id)
}

mod lifecycle;
