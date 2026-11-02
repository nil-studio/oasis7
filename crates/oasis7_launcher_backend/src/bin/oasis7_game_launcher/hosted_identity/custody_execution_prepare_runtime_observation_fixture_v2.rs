//! Actual Runtime actor workload inside the explicitly synthetic matching
//! World authority. No external snapshot transplant, paid Account, or CF claim.
use super::*;
pub(super) fn build(data: &identity::participant_entry::resource_fixture::Data) -> (u64, Value) {
    let control = &data.state.control;
    assert_eq!(control.world_id, "world");
    assert_eq!(control.genesis_digest, "genesis");
    assert_eq!(control.branch_id, "main");
    assert_eq!(control.reorg_epoch.0, 3);
    assert_eq!(control.agent_id, "agent-a");
    assert_eq!(control.owner_binding, "owner-binding");
    assert_eq!(data.owner, hex::encode([11; 32]));
    let public = ws::sign_read_request("key", (), &data.owner)
        .unwrap()
        .subject_public_key;
    assert_eq!(control.owner_public_key, public);
    assert_eq!(data.state.world.world_id, control.world_id);
    assert_eq!(data.state.world.genesis_digest, control.genesis_digest);
    assert_eq!(data.state.owner.world_id, control.world_id);
    assert_eq!(data.state.owner.branch_id, control.branch_id);
    assert_eq!(data.state.owner.reorg_epoch, control.reorg_epoch);
    assert_eq!(data.state.owner.agent_id, control.agent_id);
    assert_eq!(data.state.owner.player_id, control.owner_binding);
    assert_eq!(data.state.owner.public_key, public);
    assert_eq!(
        data.prepared.owner_proof.request.genesis_digest,
        control.genesis_digest
    );
    assert_eq!(data.prepared.owner_proof.request.task_id, control.task_id);
    assert_eq!(data.prepared.owner_proof.request.agent_id, control.agent_id);
    assert_eq!(
        data.prepared.owner_proof.request.owner_binding,
        control.owner_binding
    );
    assert_eq!(
        data.prepared.owner_proof.request.owner_generation,
        control.owner_generation
    );
    assert_eq!(
        data.prepared.owner_proof.request.goal_revision,
        control.active_goal_revision
    );
    assert_eq!(
        data.prepared.goal_ref,
        control.active_goal_reference.as_ref().unwrap().goal_ref
    );
    let signer = data.state.signer_delegation.as_ref().unwrap();
    let executor = control.executor_binding.as_ref().unwrap();
    assert_eq!(signer.world_id, control.world_id);
    assert_eq!(signer.branch_id, control.branch_id);
    assert_eq!(signer.agent_id, control.agent_id);
    assert_eq!(signer.owner_binding, control.owner_binding);
    assert_eq!(signer.delegate_public_key, executor.executor_public_key);
    assert_eq!(signer.generation, executor.delegation_generation);

    assert_eq!(data.prepared.owner_proof.request.owner_public_key, public);
    assert_eq!(data.prepared.owner_proof.request.world_id, control.world_id);
    assert_eq!(
        data.prepared.owner_proof.request.branch_id,
        control.branch_id
    );
    assert_eq!(
        data.prepared.owner_proof.request.reorg_epoch,
        control.reorg_epoch
    );
    // Native grant/dispatch windows start only after ready setup. Cache the
    // exact already-generated observation so the World response does not spend
    // that short window running the same local workload a second time.
    static GENERATED: std::sync::OnceLock<(String, u64, Value)> = std::sync::OnceLock::new();
    let key = oasis7_custody_core::approval::bytes_digest(
        &serde_json::to_vec(&(
            data.state.schema_revision,
            &data.state.world,
            &data.state.read_commit,
            &data.state.owner,
            control,
            &data.prepared,
            &data.state.signer_delegation,
        ))
        .unwrap(),
    );
    let generated = GENERATED.get_or_init(|| {
        let (tick, observation) = generate(data);
        (key.clone(), tick, observation)
    });
    assert_eq!(
        generated.0, key,
        "do not reuse actor facts for a changed complete fixture scope"
    );
    (generated.1, generated.2.clone())
}
fn generate(data: &identity::participant_entry::resource_fixture::Data) -> (u64, Value) {
    use oasis7_agent_api::world_action::GeoPos;
    use oasis7_runtime::{Action, DomainEvent, World, WorldState};
    let control = &data.state.control;
    let public = ws::sign_read_request("key", (), &data.owner)
        .unwrap()
        .subject_public_key;
    let mut world = World::new_with_state(WorldState::default());
    world
        .bind_cognition_runtime(
            &control.world_id,
            &control.branch_id,
            0,
            None,
            "pending",
            control.reorg_epoch.0,
        )
        .unwrap();
    world.submit_action(Action::RegisterAgent {
        agent_id: control.agent_id.clone(),
        pos: GeoPos::new(0, 0, 0),
    });
    world.step().unwrap();
    world.submit_action(Action::ClaimStarterOc {
        agent_id: control.agent_id.clone(),
        player_id: control.owner_binding.clone(),
        public_key: Some(public.clone()),
    });
    world.step().unwrap();
    let claim = &world.state().starter_oc_claims[&control.agent_id];
    assert_eq!(claim.player_id, control.owner_binding);
    assert_eq!(claim.public_key.as_deref(), Some(public.as_str()));
    world
        .install_capability_agent_identity(&control.agent_id, &control.owner_binding, 1)
        .unwrap();
    let identity = world.current_agent_identity(&control.agent_id).unwrap();
    assert_eq!(identity.owner_binding, control.owner_binding);
    assert_eq!(
        identity.generation,
        data.state.owner.agent_identity_generation.0
    );
    let target = 48usize * 1024;
    // Derive a bounded work count from the actual supported event encoding.
    // A Move is small; the former literal256 cannot fill this legal body.
    let event_bytes = serde_json::to_vec(&DomainEvent::AgentMoved {
        agent_id: control.agent_id.clone(),
        from: GeoPos::new(0, 0, 0),
        to: GeoPos::new(10, 0, 0),
    })
    .unwrap()
    .len();
    assert!(event_bytes > 0);
    let maximum_moves = target
        .checked_add(event_bytes - 1)
        .unwrap()
        .checked_div(event_bytes)
        .unwrap()
        .checked_add(1)
        .unwrap();
    // Independent fixture work ceiling; never a product/HTTP/body allowance.
    const HARD_MAXIMUM_MOVES: usize = 512;
    assert!(maximum_moves <= HARD_MAXIMUM_MOVES);
    let mut moves = 0usize;
    loop {
        let cell = &world.state().agents[&control.agent_id];
        if serde_json::to_vec(cell).unwrap().len() >= target {
            break;
        }
        assert!(
            moves < maximum_moves,
            "bounded real Move workload must produce large observation"
        );
        let before = cell.state.pos;
        let mailbox_before = cell.mailbox.len();
        let to = GeoPos::new(if moves % 2 == 0 { 10 } else { 20 }, 0, 0);
        world.submit_action(Action::MoveAgent {
            agent_id: control.agent_id.clone(),
            to,
        });
        world.step().unwrap();
        assert!(
            world.state().agents[&control.agent_id].state.pos != before,
            "each retained business event must result from successful real Move"
        );
        assert!(
            world.state().agents[&control.agent_id]
                .mailbox
                .iter()
                .skip(mailbox_before)
                .any(|event| matches!(event,
                DomainEvent::AgentMoved { agent_id, from, to: actual }
                    if agent_id == &control.agent_id && *from == before && *actual == to)),
            "the retained event must be this successful actual Move"
        );
        moves += 1;
    }
    let binding = world.current_cognition_runtime_binding().unwrap();
    assert_eq!(binding.world_id, control.world_id);
    assert_eq!(binding.branch_id, control.branch_id);
    assert_eq!(binding.reorg_epoch, control.reorg_epoch.0);
    let cell = &world.state().agents[&control.agent_id];
    assert!(cell.mailbox.len() > 2);
    assert!(
        cell.intent.is_none(),
        "no foreign goal/intent may be transplanted"
    );
    let observation = serde_json::to_value(cell).unwrap();
    assert_eq!(
        observation["state"]["agent_id"].as_str(),
        Some(control.agent_id.as_str())
    );
    eprintln!(
        "matching_runtime_moves={moves} actor_json_bytes={} tick={}",
        serde_json::to_vec(&observation).unwrap().len(),
        world.state().time
    );
    (world.state().time, observation)
}

#[test]
fn matching_runtime_actor_observation_generator_uses_real_registered_owner_moves() {
    let mut data = identity::participant_entry::resource_fixture::Data::new();
    data.owner = hex::encode([11; 32]);
    let public = ws::sign_read_request("key", (), &data.owner)
        .unwrap()
        .subject_public_key;
    data.state.owner.public_key = public.clone();
    data.state.control.owner_public_key = public.clone();
    let mut goal = data.prepared.owner_proof.request.clone();
    goal.owner_public_key = public;
    let proof = ws::private_goal::sign_goal_prepare(goal, &data.owner).unwrap();
    data.prepared = ws::private_goal::PreparedGoalReceiptV1 {
        schema_revision: 1,
        goal_ref: ws::private_goal::prepared_goal_commitment(&[1; 32], &proof).unwrap(),
        reference_salt: hex::encode([1; 32]),
        owner_proof: proof,
    };
    data.state
        .control
        .active_goal_reference
        .as_mut()
        .unwrap()
        .goal_ref = data.prepared.goal_ref.clone();
    let (tick, observation) = build(&data);
    assert!(serde_json::to_vec(&observation).unwrap().len() >= 48 * 1024);
    if let Some(path) = std::env::var_os("OASIS7_MATCHING_RUNTIME_OBSERVATION_EXPORT") {
        std::fs::write(path,serde_json::to_vec(&json!({
            "qualification":"actual Runtime actor only; enclosing Native World/Account remain fixtures",
            "world":data.state.world,"control":data.state.control,
            "goal":data.prepared,"logical_tick":tick,"actor_observation":observation,
        })).unwrap()).unwrap();
    }
}
