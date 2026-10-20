use super::*;

#[test]
fn start_root_consumes_escrow_atomically_without_aggregate_credit_or_duplicate() {
    let (mut state, root) = bound_root_fixture();
    let plan = crate::builtin_recipe_plan(&root.pinned_recipe_id, root.accepted_batches).unwrap();
    let mut exact_capacity = root.clone();
    exact_capacity.terminal_capacities =
        BTreeMap::from([("iron_ingot".into(), 3), ("slag".into(), 1)]);
    crate::world_rules::industrial_work_start::validate_industrial_output_capacity(
        &state,
        &exact_capacity,
        &plan,
        None,
    )
    .unwrap();
    let mut occupied_terminal = state.clone();
    occupied_terminal
        .material_ledgers
        .get_mut(&root.output_ledger)
        .unwrap()
        .insert("iron_ingot".into(), 1);
    assert!(
        crate::world_rules::industrial_work_start::validate_industrial_output_capacity(
            &occupied_terminal,
            &exact_capacity,
            &plan,
            None
        )
        .is_err()
    );
    let before_ledgers = state.material_ledgers.clone();
    let before_power = state.agents["owner"]
        .state
        .resources
        .get(ResourceKind::Electricity);
    let start = event(
        "start",
        IndustrialWorkCommandV1::StartRoot {
            root_ref: root.root_ref.clone(),
            expected_profile_digest: root.pinned_profile_digest.clone(),
            logistics_route_ids: vec![],
            logistics_path_ids: vec![],
        },
    );
    let mut recovered_without_authority = state.clone();
    recovered_without_authority
        .world_rules
        .as_mut()
        .unwrap()
        .industrial_work
        .imported_lot_authorities
        .clear();
    assert!(PreparedWorldRule::prepare(&recovered_without_authority, &start, 1).is_err());
    let mut profile_drift = state.clone();
    profile_drift
        .world_rules
        .as_mut()
        .unwrap()
        .industrial_work
        .imported_lot_authorities
        .values_mut()
        .next()
        .unwrap()
        .material_profile
        .digest = crate::util::sha256_hex(b"other-profile");
    assert!(PreparedWorldRule::prepare(&profile_drift, &start, 1).is_err());
    let mut owner_drift = state.clone();
    owner_drift
        .starter_oc_claims
        .get_mut("owner")
        .unwrap()
        .public_key = Some("new-owner-key".into());
    assert!(PreparedWorldRule::prepare(&owner_drift, &start, 1).is_err());
    let mut locked = state.clone();
    locked.product_profiles.insert(
        "iron_ingot".into(),
        oasis7_wasm_abi::ProductProfileV1 {
            product_id: "iron_ingot".into(),
            role_tag: "material".into(),
            maintenance_sink: vec![],
            tradable: true,
            unlock_stage: "governance".into(),
        },
    );
    let locked_before = serde_json::to_vec(&locked).unwrap();
    assert!(PreparedWorldRule::prepare(&locked, &start, 1).is_err());
    assert_eq!(serde_json::to_vec(&locked).unwrap(), locked_before);
    assert!(
        locked.world_rules.as_ref().unwrap().industrial_work.roots[&root.root_ref.id]
            .slots
            .values()
            .all(|slot| !slot.consumed)
    );
    let prepared = PreparedWorldRule::prepare(&state, &start, 1).unwrap();
    assert_eq!(state.material_ledgers, before_ledgers);
    prepared.install(&mut state);
    assert_eq!(state.material_ledgers, before_ledgers);
    assert_eq!(state.pending_recipe_jobs.len(), 1);
    let mut competing_capacity = root.clone();
    competing_capacity.terminal_capacities =
        BTreeMap::from([("iron_ingot".into(), 5), ("slag".into(), 2)]);
    assert!(
        crate::world_rules::industrial_work_start::validate_industrial_output_capacity(
            &state,
            &competing_capacity,
            &plan,
            None
        )
        .is_err()
    );
    let running_job = state.pending_recipe_jobs.values().next().unwrap().job_id;
    crate::world_rules::industrial_work_start::validate_industrial_output_capacity(
        &state,
        &competing_capacity,
        &plan,
        Some(running_job),
    )
    .unwrap();
    assert!(
        state
            .world_rules
            .as_ref()
            .unwrap()
            .industrial_work
            .lots
            .values()
            .all(|lot| lot.remaining == 0)
    );
    assert!(
        state.world_rules.as_ref().unwrap().industrial_work.roots[&root.root_ref.id]
            .slots
            .values()
            .all(|slot| slot.consumed)
    );
    assert!(
        state.agents["owner"]
            .state
            .resources
            .get(ResourceKind::Electricity)
            < before_power
    );
    let after = serde_json::to_vec(&state).unwrap();
    PreparedWorldRule::prepare(&state, &start, 1)
        .unwrap()
        .install(&mut state);
    assert_eq!(serde_json::to_vec(&state).unwrap(), after);
    let repeated = event(
        "new-start-key",
        IndustrialWorkCommandV1::StartRoot {
            root_ref: root.root_ref.clone(),
            expected_profile_digest: root.pinned_profile_digest.clone(),
            logistics_route_ids: vec![],
            logistics_path_ids: vec![],
        },
    );
    assert!(PreparedWorldRule::prepare(&state, &repeated, 1).is_err());
    let terminate = event(
        "terminate",
        IndustrialWorkCommandV1::TerminateRoot {
            root_ref: root.root_ref.clone(),
        },
    );
    let job_id = state.world_rules.as_ref().unwrap().industrial_work.roots[&root.root_ref.id]
        .started_job_id
        .unwrap();
    let pending = state.pending_recipe_jobs[&job_id].clone();
    let mut completed = state.clone();
    completed.pending_recipe_jobs.remove(&job_id);
    assert!(PreparedWorldRule::prepare(&completed, &terminate, 2).is_err());
    let mut unsupported = state.clone();
    unsupported
        .world_rules
        .as_mut()
        .unwrap()
        .industrial_work
        .roots
        .get_mut(&root.root_ref.id)
        .unwrap()
        .allowed_dispositions
        .clear();
    assert!(PreparedWorldRule::prepare(&unsupported, &terminate, 2).is_err());
    let power_before_termination = state.agents["owner"]
        .state
        .resources
        .get(ResourceKind::Electricity);
    PreparedWorldRule::prepare(&state, &terminate, 2)
        .unwrap()
        .install(&mut state);
    assert!(!state.pending_recipe_jobs.contains_key(&job_id));
    let loss = &state.factory_production_failure_dispositions[&job_id];
    assert_eq!(loss.blocker_kind, "industrial_termination");
    assert_eq!(loss.lost_inputs, pending.consume);
    assert_eq!(loss.lost_power, pending.power_required);
    assert_eq!(state.material_ledgers, before_ledgers);
    assert_eq!(
        state.agents["owner"]
            .state
            .resources
            .get(ResourceKind::Electricity),
        power_before_termination
    );
    let terminated = serde_json::to_vec(&state).unwrap();
    PreparedWorldRule::prepare(&state, &terminate, 3)
        .unwrap()
        .install(&mut state);
    assert_eq!(serde_json::to_vec(&state).unwrap(), terminated);
    assert!(
        PreparedWorldRule::prepare(
            &state,
            &event(
                "terminate-new-key",
                IndustrialWorkCommandV1::TerminateRoot {
                    root_ref: root.root_ref
                }
            ),
            3
        )
        .is_err()
    );
}

fn completion(job: &crate::RecipeJobState) -> DomainEvent {
    DomainEvent::RecipeCompleted {
        committed_recipe_origin: job.committed_recipe_origin.clone(),
        job_id: job.job_id,
        requester_agent_id: job.requester_agent_id.clone(),
        factory_id: job.factory_id.clone(),
        recipe_id: job.recipe_id.clone(),
        accepted_batches: job.accepted_batches,
        produce: job.produce.clone(),
        byproducts: job.byproducts.clone(),
        output_ledger: job.output_ledger.clone(),
        bottleneck_tags: job.bottleneck_tags.clone(),
        logistics_route_ids: job.logistics_route_ids.clone(),
        logistics_path_ids: job.logistics_path_ids.clone(),
    }
}

#[test]
fn finite_terminal_blocks_completion_without_refund_then_settles_exactly_once() {
    let (mut state, root_ref, job_id) = started_root_fixture();
    let job = state.pending_recipe_jobs[&job_id].clone();
    let root = state.world_rules.as_ref().unwrap().industrial_work.roots[&root_ref.id].clone();
    state
        .material_ledgers
        .get_mut(&root.output_ledger)
        .unwrap()
        .insert("iron_ingot".into(), 100);
    let ev = completion(&job);
    let before = serde_json::to_vec(&state).unwrap();
    assert!(crate::world_rules::validate_industrial_completion(&state, &job).is_err());
    assert!(state.apply_domain_event(&ev, job.ready_at).is_err());
    assert_eq!(serde_json::to_vec(&state).unwrap(), before);
    assert!(state.pending_recipe_jobs.contains_key(&job_id));
    assert!(
        state
            .world_rules
            .as_ref()
            .unwrap()
            .industrial_work
            .lots
            .values()
            .all(|lot| lot.remaining == 0)
    );
    state
        .material_ledgers
        .get_mut(&root.output_ledger)
        .unwrap()
        .insert("iron_ingot".into(), 0);
    crate::world_rules::validate_industrial_completion(&state, &job).unwrap();
    state.time = job.ready_at;
    state.apply_domain_event(&ev, job.ready_at).unwrap();
    assert!(!state.pending_recipe_jobs.contains_key(&job_id));
    assert_eq!(state.material_ledgers[&root.output_ledger]["iron_ingot"], 3);
    assert_eq!(state.material_ledgers[&root.output_ledger]["slag"], 1);
    let completed = serde_json::to_vec(&state).unwrap();
    state.apply_domain_event(&ev, job.ready_at + 1).unwrap();
    assert_eq!(serde_json::to_vec(&state).unwrap(), completed);
    assert!(
        PreparedWorldRule::prepare(
            &state,
            &event(
                "terminate-completed",
                IndustrialWorkCommandV1::TerminateRoot { root_ref }
            ),
            job.ready_at + 1
        )
        .is_err()
    );
}

#[test]
fn actual_world_collector_emits_terminal_blocker_once_and_recovers_after_capacity_release() {
    let (mut state, root_ref, job_id) = started_root_fixture();
    let job = state.pending_recipe_jobs[&job_id].clone();
    let root = state.world_rules.as_ref().unwrap().industrial_work.roots[&root_ref.id].clone();
    state
        .material_ledgers
        .get_mut(&root.output_ledger)
        .unwrap()
        .insert("iron_ingot".into(), 100);
    state.time = job.ready_at.saturating_sub(1);
    let mut world = crate::World::new_with_state(state);
    let blockers = |world: &crate::World| {
        world.journal().events.iter().filter(|event|
        matches!(&event.body, crate::WorldEventBody::Domain(DomainEvent::FactoryProductionBlocked {
            blocker_kind, action_id, .. }) if blocker_kind == "industrial_terminal" && *action_id == job_id)).count()
    };
    world.step().unwrap();
    assert!(world.state().pending_recipe_jobs.contains_key(&job_id));
    assert_eq!(blockers(&world), 1);
    world.step().unwrap();
    assert!(world.state().pending_recipe_jobs.contains_key(&job_id));
    assert_eq!(blockers(&world), 1);
    world
        .set_ledger_material_balance(root.output_ledger.clone(), "iron_ingot", 0)
        .unwrap();
    world.step().unwrap();
    assert!(!world.state().pending_recipe_jobs.contains_key(&job_id));
    assert_eq!(
        world.state().material_ledgers[&root.output_ledger]["iron_ingot"],
        3
    );
    assert_eq!(blockers(&world), 1);
    world.step().unwrap();
    assert_eq!(
        world.state().material_ledgers[&root.output_ledger]["iron_ingot"],
        3
    );
}

#[test]
fn terminated_unsunk_binding_releases_escrow_once_but_live_successor_still_blocks() {
    let (mut state, root) = bound_root_fixture();
    let lot_ref = root.slots["iron_ore"].parents[0].lot_ref.clone();
    let release = event(
        "release-disposed",
        IndustrialWorkCommandV1::ReleaseLot {
            lot_ref: lot_ref.clone(),
        },
    );
    assert!(PreparedWorldRule::prepare(&state, &release, 2).is_err());
    let plan = |root: &RootOperationV1, disposition| DispositionPlanV1 {
        root_ref: root.root_ref.clone(),
        plan_ref: industrial_ref(
            "world",
            "industrial",
            "industrial-disposition/v1",
            &format!("dispose:{}", root.root_ref.id),
            1,
            &root.root_ref,
        )
        .unwrap(),
        expected_causal_digest: root.causal_digest.clone(),
        disposition,
    };
    let mut held = state.clone();
    crate::world_rules::industrial_work::apply_disposition(
        &mut held.world_rules.as_mut().unwrap().industrial_work,
        plan(&root, IndustrialDispositionV1::Hold),
        "world",
    )
    .unwrap();
    assert!(PreparedWorldRule::prepare(&held, &release, 2).is_err());
    crate::world_rules::industrial_work::apply_disposition(
        &mut state.world_rules.as_mut().unwrap().industrial_work,
        plan(&root, IndustrialDispositionV1::Terminate),
        "world",
    )
    .unwrap();
    let mut successor = root.clone();
    for slot in successor.slots.values_mut() {
        slot.binding_revision = 1;
    }
    successor.root_ref = industrial_ref(
        "world",
        "industrial",
        "industrial-root/v1",
        "successor",
        1,
        &root.root_ref,
    )
    .unwrap();
    crate::world_rules::industrial_work::accept_root(
        &mut state.world_rules.as_mut().unwrap().industrial_work,
        successor.clone(),
        "world",
    )
    .unwrap();
    assert!(PreparedWorldRule::prepare(&state, &release, 2).is_err());
    crate::world_rules::industrial_work::apply_disposition(
        &mut state.world_rules.as_mut().unwrap().industrial_work,
        plan(&successor, IndustrialDispositionV1::Terminate),
        "world",
    )
    .unwrap();
    PreparedWorldRule::prepare(&state, &release, 2)
        .unwrap()
        .install(&mut state);
    assert_eq!(
        state.material_ledgers[&MaterialLedgerId::agent("owner")]["iron_ore"],
        20
    );
    assert_eq!(
        state.world_rules.as_ref().unwrap().industrial_work.lots[&lot_ref.id].remaining,
        0
    );
    let released = serde_json::to_vec(&state).unwrap();
    PreparedWorldRule::prepare(&state, &release, 3)
        .unwrap()
        .install(&mut state);
    assert_eq!(serde_json::to_vec(&state).unwrap(), released);
    assert!(
        PreparedWorldRule::prepare(
            &state,
            &event(
                "release-new-key",
                IndustrialWorkCommandV1::ReleaseLot { lot_ref }
            ),
            3
        )
        .is_err()
    );
}

#[test]
fn signed_site_terminal_revocation_preserves_wip_until_same_destination_recovers() {
    let (mut state, root) = bound_root_fixture_with_site(true);
    let start = event(
        "site-start",
        IndustrialWorkCommandV1::StartRoot {
            root_ref: root.root_ref.clone(),
            expected_profile_digest: root.pinned_profile_digest.clone(),
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
    let job = state.pending_recipe_jobs[&job_id].clone();
    state.time = job.ready_at;
    let ev = completion(&job);
    let electricity = state.agents["owner"]
        .state
        .resources
        .get(ResourceKind::Electricity);
    state
        .factory_site_authorities
        .get_mut("terminal-site")
        .unwrap()
        .active = false;
    let revoked = serde_json::to_vec(&state).unwrap();
    assert!(crate::world_rules::validate_industrial_completion(&state, &job).is_err());
    assert!(state.apply_domain_event(&ev, job.ready_at).is_err());
    assert_eq!(serde_json::to_vec(&state).unwrap(), revoked);
    assert!(state.pending_recipe_jobs.contains_key(&job_id));
    assert!(
        state
            .world_rules
            .as_ref()
            .unwrap()
            .industrial_work
            .lots
            .values()
            .all(|lot| lot.remaining == 0)
    );
    assert_eq!(
        state.agents["owner"]
            .state
            .resources
            .get(ResourceKind::Electricity),
        electricity
    );
    state
        .factory_site_authorities
        .get_mut("terminal-site")
        .unwrap()
        .active = true;
    let mut replaced = state.clone();
    replaced
        .factories
        .get_mut(&root.factory_id)
        .unwrap()
        .builder_agent_id = "replacement".into();
    assert!(crate::world_rules::validate_industrial_completion(&replaced, &job).is_err());
    let mut redirected = state.clone();
    redirected
        .factories
        .get_mut(&root.factory_id)
        .unwrap()
        .output_ledger = MaterialLedgerId::agent("owner");
    assert!(crate::world_rules::validate_industrial_completion(&redirected, &job).is_err());
    crate::world_rules::validate_industrial_completion(&state, &job).unwrap();
    state.apply_domain_event(&ev, job.ready_at).unwrap();
    assert!(!state.pending_recipe_jobs.contains_key(&job_id));
    assert_eq!(state.material_ledgers[&root.output_ledger]["iron_ingot"], 3);
    assert_eq!(state.material_ledgers[&root.output_ledger]["slag"], 1);
    assert_eq!(
        state.material_ledgers[&MaterialLedgerId::agent("owner")].get("iron_ingot"),
        None
    );
    let completed = serde_json::to_vec(&state).unwrap();
    state.apply_domain_event(&ev, job.ready_at).unwrap();
    assert_eq!(serde_json::to_vec(&state).unwrap(), completed);
}

#[test]
fn restored_industrial_job_cannot_settle_without_its_exact_graph_link() {
    let (state, root_ref, job_id) = started_root_fixture();
    let job = state.pending_recipe_jobs[&job_id].clone();
    assert_eq!(job.industrial_root_ref.as_ref(), Some(&root_ref));
    let restored: WorldState =
        serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
    assert_eq!(
        restored.pending_recipe_jobs[&job_id]
            .industrial_root_ref
            .as_ref(),
        Some(&root_ref)
    );
    let ev = completion(&job);
    for mutation in 0..4 {
        let mut damaged = restored.clone();
        match mutation {
            0 => damaged.world_rules = None,
            1 => damaged
                .world_rules
                .as_mut()
                .unwrap()
                .industrial_work
                .roots
                .clear(),
            2 => {
                damaged
                    .pending_recipe_jobs
                    .get_mut(&job_id)
                    .unwrap()
                    .industrial_root_ref
                    .as_mut()
                    .unwrap()
                    .digest = crate::util::sha256_hex(b"foreign-root")
            }
            _ => {
                damaged
                    .pending_recipe_jobs
                    .get_mut(&job_id)
                    .unwrap()
                    .industrial_root_ref = None
            }
        }
        let before = serde_json::to_vec(&damaged).unwrap();
        assert!(
            crate::world_rules::validate_industrial_completion(
                &damaged,
                &damaged.pending_recipe_jobs[&job_id]
            )
            .is_err()
        );
        assert!(damaged.apply_domain_event(&ev, job.ready_at).is_err());
        assert_eq!(serde_json::to_vec(&damaged).unwrap(), before);
        assert!(damaged.pending_recipe_jobs.contains_key(&job_id));
        assert!(
            PreparedWorldRule::prepare(
                &damaged,
                &event(
                    "terminate-missing-link",
                    IndustrialWorkCommandV1::TerminateRoot {
                        root_ref: root_ref.clone()
                    }
                ),
                job.ready_at
            )
            .is_err()
        );
    }
    // Existing ordinary commitments retain their exact absent-field encoding.
    let mut legacy = serde_json::to_value(&job).unwrap();
    legacy
        .as_object_mut()
        .unwrap()
        .remove("industrial_root_ref");
    let decoded: crate::RecipeJobState = serde_json::from_value(legacy.clone()).unwrap();
    assert_eq!(decoded.industrial_root_ref, None);
    assert_eq!(serde_json::to_value(decoded).unwrap(), legacy);
}
