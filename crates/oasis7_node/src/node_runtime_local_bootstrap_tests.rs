use super::*;

fn restored_continuation(
    engine: &mut PosNodeEngine,
    slot: u64,
    restore_clock: bool,
) -> NodeLocalExecutionContinuation {
    let node_id = "node-clock";
    let world_id = "world-clock";
    let predecessor_height = 19;
    let predecessor_node_hash = "node-parent-19";
    let predecessor_execution_hash = "execution-parent-19";
    let predecessor_state_root = "state-parent-19";
    let bootstrap = NodeExecutionBootstrap {
        height: predecessor_height,
        consensus_block_hash: predecessor_node_hash.into(),
        execution_block_hash: predecessor_execution_hash.into(),
        execution_state_root: predecessor_state_root.into(),
    };
    engine
        .apply_local_execution_bootstrap(&bootstrap)
        .expect("apply qualified predecessor boundary");

    let height = predecessor_height + 1;
    let action_root = compute_consensus_action_root(&[]).expect("empty action root");
    let epoch = engine.slot_epoch(slot);
    let node_block_hash = engine
        .compute_block_hash(
            world_id,
            height,
            slot,
            epoch,
            node_id,
            predecessor_node_hash,
            &action_root,
        )
        .expect("compute qualified proposal hash");
    let saved = NodeLocalExecutionContinuation {
        schema_version: 1,
        context: NodeExecutionCommitContext {
            world_id: world_id.into(),
            node_id: node_id.into(),
            proposer_id: node_id.into(),
            height,
            slot,
            epoch,
            node_block_hash,
            action_root,
            committed_actions: Vec::new(),
            committed_at_unix_ms: 1_000_000,
        },
        predecessor_committed_height: predecessor_height,
        predecessor_node_block_hash: Some(predecessor_node_hash.into()),
        predecessor_execution_height: predecessor_height,
        predecessor_execution_block_hash: Some(predecessor_execution_hash.into()),
        predecessor_execution_state_root: Some(predecessor_state_root.into()),
        next_height: height,
        next_slot: slot + 1,
        reserved_action_bytes: 0,
        approved_stake: engine.required_stake,
        rejected_stake: 0,
        required_stake: engine.required_stake,
        total_stake: engine.total_stake,
    };
    engine.next_slot = saved.next_slot;
    engine
        .restore_guarded_local_decision(&saved, node_id, world_id)
        .expect("restore exact guarded local decision");
    if restore_clock {
        engine
            .restore_guarded_local_schedule_clock(&saved)
            .expect("restore clock from validated continuation");
    }
    saved
}

fn clock_test_config() -> NodeConfig {
    let mut config =
        NodeConfig::new("node-clock", "world-clock", NodeRole::Sequencer).expect("config");
    config.pos_config.slot_duration_ms = 8;
    config.pos_config.ticks_per_slot = 4;
    config.pos_config.proposal_tick_phase = 2;
    // This test exercises the schedule gate without requiring an execution
    // hook after the single-validator proposal commits.
    config.require_execution_on_commit = false;
    config
}

#[test]
fn guarded_continuation_restores_clock_before_first_tick_and_proposes_successor() {
    let config = clock_test_config();
    let committed_slot = 128_084;
    let mut unseeded = PosNodeEngine::new(&config).expect("unseeded engine");
    let unseeded_saved = restored_continuation(&mut unseeded, committed_slot, false);
    assert_eq!(unseeded.next_slot, unseeded_saved.next_slot);

    unseeded
        .tick(
            "node-clock",
            "world-clock",
            1_000_000,
            None,
            None,
            None,
            None,
            Vec::new(),
            None,
        )
        .expect("unseeded first tick");
    unseeded
        .tick(
            "node-clock",
            "world-clock",
            1_000_008,
            None,
            None,
            None,
            None,
            Vec::new(),
            None,
        )
        .expect("unseeded successor remains below restored next slot");
    assert_eq!(unseeded.committed_height, 19);
    assert!(unseeded.pending.is_none());

    let mut engine = PosNodeEngine::new(&config).expect("seeded engine");
    let saved = restored_continuation(&mut engine, committed_slot, true);

    assert_eq!(engine.next_slot, saved.next_slot);
    assert_eq!(engine.last_observed_slot, committed_slot);
    assert_eq!(
        engine.last_observed_tick,
        committed_slot * config.pos_config.ticks_per_slot + config.pos_config.proposal_tick_phase
    );

    let first = engine
        .tick(
            "node-clock",
            "world-clock",
            1_000_000,
            None,
            None,
            None,
            None,
            Vec::new(),
            None,
        )
        .expect("first tick observes the restored committed slot");
    assert_eq!(first.consensus_snapshot.committed_height, 19);
    assert!(engine.pending.is_none());

    let successor = engine
        .tick(
            "node-clock",
            "world-clock",
            1_000_008,
            None,
            None,
            None,
            None,
            Vec::new(),
            None,
        )
        .expect("successor slot becomes eligible");
    assert_eq!(successor.consensus_snapshot.committed_height, 20);
    assert_eq!(engine.committed_height, 20);
}

#[test]
fn guarded_continuation_clock_keeps_configured_absolute_origin() {
    let mut config = clock_test_config();
    config.pos_config.slot_clock_genesis_unix_ms = Some(700_000);
    let mut engine = PosNodeEngine::new(&config).expect("engine");
    restored_continuation(&mut engine, 128_084, true);

    assert_eq!(engine.slot_clock_genesis_unix_ms, Some(700_000));
    assert_eq!(engine.last_observed_slot, 128_084);
}

#[test]
fn guarded_continuation_clock_rejects_unbound_successor_slot() {
    let config = clock_test_config();
    let mut engine = PosNodeEngine::new(&config).expect("engine");
    let mut saved = restored_continuation(&mut engine, 128_084, false);
    saved.next_slot += 1;

    assert!(engine.restore_guarded_local_schedule_clock(&saved).is_err());
    assert_eq!(engine.last_observed_tick, 0);
    assert_eq!(engine.last_observed_slot, 0);
}

struct CurrentBootstrapState {
    current: NodeLocalExecutionBootstrapV1,
    qualified_genesis: Option<NodeExecutionGenesisBootstrap>,
    pending: Option<NodeLocalExecutionContinuation>,
    reads: usize,
    restored: usize,
}

struct CurrentBootstrapHook {
    state: Arc<Mutex<CurrentBootstrapState>>,
    guarded: bool,
    dynamic: bool,
}

impl NodeExecutionHook for CurrentBootstrapHook {
    fn on_commit(
        &mut self,
        context: NodeExecutionCommitContext,
    ) -> Result<NodeExecutionCommitResult, String> {
        Ok(NodeExecutionCommitResult {
            execution_height: context.height,
            execution_block_hash: format!("execution-{}", context.height),
            execution_state_root: format!("root-{}", context.height),
        })
    }

    fn guarded_local_execution(&self) -> bool {
        self.guarded
    }

    fn current_guarded_local_execution_bootstrap(
        &mut self,
    ) -> Result<Option<NodeLocalExecutionBootstrapV1>, String> {
        let mut state = self.state.lock().unwrap();
        state.reads += 1;
        Ok(self.dynamic.then(|| state.current.clone()))
    }

    fn qualified_local_execution_genesis_bootstrap(
        &mut self,
    ) -> Result<Option<NodeExecutionGenesisBootstrap>, String> {
        Ok(self.state.lock().unwrap().qualified_genesis.clone())
    }

    fn prepare_verified_local_continuation(
        &mut self,
        source: &crate::execution_hook::VerifiedNodeLocalContinuationV1<'_>,
    ) -> Result<(), String> {
        self.state.lock().unwrap().pending = Some(source.original().clone());
        Ok(())
    }

    fn pending_local_continuation(
        &mut self,
    ) -> Result<Option<NodeLocalExecutionContinuation>, String> {
        Ok(self.state.lock().unwrap().pending.clone())
    }

    fn restored_verified_local_continuation(
        &mut self,
        source: &crate::execution_hook::VerifiedNodeRestoredContinuationV1<'_>,
    ) -> Result<(), String> {
        let mut state = self.state.lock().unwrap();
        assert_eq!(state.pending.as_ref(), Some(source.original()));
        state.restored += 1;
        Ok(())
    }

    fn complete_local_continuation(
        &mut self,
        context: &NodeExecutionCommitContext,
    ) -> Result<(), String> {
        let mut state = self.state.lock().unwrap();
        assert_eq!(state.pending.as_ref().map(|v| &v.context), Some(context));
        state.current = NodeLocalExecutionBootstrapV1::Normal(NodeExecutionBootstrap {
            height: context.height,
            consensus_block_hash: context.node_block_hash.clone(),
            execution_block_hash: format!("execution-{}", context.height),
            execution_state_root: format!("root-{}", context.height),
        });
        state.qualified_genesis = None;
        state.pending = None;
        Ok(())
    }
}

fn current_bootstrap_state(
    current: NodeLocalExecutionBootstrapV1,
) -> Arc<Mutex<CurrentBootstrapState>> {
    let qualified_genesis = match &current {
        NodeLocalExecutionBootstrapV1::Genesis(value) => Some(value.clone()),
        NodeLocalExecutionBootstrapV1::Normal(_) => None,
    };
    Arc::new(Mutex::new(CurrentBootstrapState {
        current,
        qualified_genesis,
        pending: None,
        reads: 0,
        restored: 0,
    }))
}

fn current_bootstrap_hook(state: &Arc<Mutex<CurrentBootstrapState>>) -> CurrentBootstrapHook {
    CurrentBootstrapHook {
        state: Arc::clone(state),
        guarded: true,
        dynamic: true,
    }
}

fn local_bootstrap(height: u64) -> NodeExecutionBootstrap {
    NodeExecutionBootstrap {
        height,
        consensus_block_hash: format!("node-parent-{height}"),
        execution_block_hash: format!("execution-parent-{height}"),
        execution_state_root: format!("state-parent-{height}"),
    }
}

fn local_genesis() -> NodeExecutionGenesisBootstrap {
    NodeExecutionGenesisBootstrap::from_qualified_initial_activation(
        "world-clock",
        "qualified-initial-record",
        local_bootstrap(0),
    )
    .unwrap()
}

#[test]
fn guarded_current_bootstrap_restarts_after_genesis_completion() {
    let mut config = clock_test_config();
    config.pos_config.ticks_per_slot = 1;
    config.pos_config.proposal_tick_phase = 0;
    let genesis = local_genesis();
    let state = current_bootstrap_state(NodeLocalExecutionBootstrapV1::Genesis(genesis.clone()));
    let runtime = NodeRuntime::new(config.clone())
        .with_local_execution_genesis_bootstrap(genesis)
        .with_execution_hook(current_bootstrap_hook(&state));
    let mut engine = PosNodeEngine::new(&config).unwrap();
    runtime
        .initialize_local_execution_bootstrap(&mut engine)
        .unwrap();
    {
        let mut hook = runtime.execution_hook.as_ref().unwrap().lock().unwrap();
        engine
            .tick(
                "node-clock",
                "world-clock",
                1_000_000,
                None,
                None,
                None,
                None,
                Vec::new(),
                Some(hook.as_mut()),
            )
            .unwrap();
    }
    assert_eq!(engine.committed_height, 1);
    assert!(state.lock().unwrap().pending.is_none());

    let mut restarted = PosNodeEngine::new(&config).unwrap();
    runtime
        .initialize_local_execution_bootstrap(&mut restarted)
        .unwrap();
    assert_eq!(restarted.committed_height, 1);
    assert_eq!(restarted.next_height, 2);
    assert_eq!(
        restarted.last_committed_block_hash,
        engine.last_committed_block_hash
    );
    assert_eq!(
        restarted.last_execution_block_hash,
        engine.last_execution_block_hash
    );
    assert_eq!(
        restarted.last_execution_state_root,
        engine.last_execution_state_root
    );
    assert_eq!(state.lock().unwrap().reads, 2);
}

#[test]
fn guarded_current_bootstrap_restores_pending_against_current_predecessor() {
    let config = clock_test_config();
    let mut original_engine = PosNodeEngine::new(&config).unwrap();
    let saved = restored_continuation(&mut original_engine, 128_084, false);
    let state = current_bootstrap_state(NodeLocalExecutionBootstrapV1::Normal(local_bootstrap(19)));
    state.lock().unwrap().pending = Some(saved.clone());
    let runtime = NodeRuntime::new(config.clone())
        .with_local_execution_bootstrap(local_bootstrap(3))
        .with_execution_hook(current_bootstrap_hook(&state));
    let mut restarted = PosNodeEngine::new(&config).unwrap();
    runtime
        .initialize_local_execution_bootstrap(&mut restarted)
        .unwrap();
    assert_eq!(restarted.committed_height, 19);
    assert_eq!(restarted.next_height, saved.next_height);
    assert_eq!(restarted.next_slot, saved.next_slot);
    assert_eq!(restarted.last_observed_slot, saved.context.slot);
    assert_eq!(state.lock().unwrap().restored, 1);
}

#[test]
fn guarded_current_bootstrap_keeps_pending_identity_and_counter_checks() {
    let config = clock_test_config();
    let mut original_engine = PosNodeEngine::new(&config).unwrap();
    let original = restored_continuation(&mut original_engine, 128_084, false);
    for case in 0..6 {
        let mut saved = original.clone();
        match case {
            0 => saved.predecessor_committed_height -= 1,
            1 => saved.predecessor_execution_block_hash = Some("changed".into()),
            2 => saved.context.node_block_hash.push('x'),
            3 => saved.next_height += 1,
            4 => saved.next_slot += 1,
            5 => saved.context.world_id = "another-world".into(),
            _ => unreachable!(),
        }
        let state =
            current_bootstrap_state(NodeLocalExecutionBootstrapV1::Normal(local_bootstrap(19)));
        state.lock().unwrap().pending = Some(saved);
        let runtime =
            NodeRuntime::new(config.clone()).with_execution_hook(current_bootstrap_hook(&state));
        let mut engine = PosNodeEngine::new(&config).unwrap();
        assert!(
            runtime
                .initialize_local_execution_bootstrap(&mut engine)
                .is_err(),
            "case {case}"
        );
        assert_eq!(state.lock().unwrap().restored, 0, "case {case}");
    }
}

#[test]
fn guarded_current_genesis_still_requires_qualified_confirmation() {
    let config = clock_test_config();
    for confirmation in [
        None,
        Some(
            NodeExecutionGenesisBootstrap::from_qualified_initial_activation(
                "world-clock",
                "another-initial-record",
                local_bootstrap(0),
            )
            .unwrap(),
        ),
    ] {
        let state =
            current_bootstrap_state(NodeLocalExecutionBootstrapV1::Genesis(local_genesis()));
        state.lock().unwrap().qualified_genesis = confirmation;
        let runtime =
            NodeRuntime::new(config.clone()).with_execution_hook(current_bootstrap_hook(&state));
        let mut engine = PosNodeEngine::new(&config).unwrap();
        assert!(
            runtime
                .initialize_local_execution_bootstrap(&mut engine)
                .is_err()
        );
        assert!(engine.last_committed_block_hash.is_none());
    }
}

#[test]
fn current_bootstrap_default_none_and_unguarded_hooks_keep_static_configuration() {
    let config = clock_test_config();
    for (guarded, dynamic, reads) in [(true, false, 1), (false, true, 0)] {
        let state =
            current_bootstrap_state(NodeLocalExecutionBootstrapV1::Normal(local_bootstrap(19)));
        let runtime = NodeRuntime::new(config.clone())
            .with_local_execution_bootstrap(local_bootstrap(3))
            .with_execution_hook(CurrentBootstrapHook {
                state: Arc::clone(&state),
                guarded,
                dynamic,
            });
        let mut engine = PosNodeEngine::new(&config).unwrap();
        runtime
            .initialize_local_execution_bootstrap(&mut engine)
            .unwrap();
        assert_eq!(engine.committed_height, 3);
        assert_eq!(state.lock().unwrap().reads, reads);
    }
}

#[test]
fn guarded_current_bootstrap_rejects_network_and_conflicting_static_configuration() {
    let state = current_bootstrap_state(NodeLocalExecutionBootstrapV1::Normal(local_bootstrap(19)));
    let mut config = clock_test_config();
    config.gossip = Some(NodeGossipConfig {
        bind_addr: "127.0.0.1:0".parse().unwrap(),
        peers: Vec::new(),
        max_dynamic_peers: 1,
        dynamic_peer_ttl_ms: 1_000,
    });
    let mut engine = PosNodeEngine::new(&config).unwrap();
    let runtime = NodeRuntime::new(config).with_execution_hook(current_bootstrap_hook(&state));
    assert!(
        runtime
            .initialize_local_execution_bootstrap(&mut engine)
            .is_err()
    );
    assert_eq!(engine.committed_height, 0);

    let config = clock_test_config();
    let mut engine = PosNodeEngine::new(&config).unwrap();
    let runtime = NodeRuntime::new(config)
        .with_local_execution_bootstrap(local_bootstrap(3))
        .with_local_execution_genesis_bootstrap(local_genesis())
        .with_execution_hook(current_bootstrap_hook(&state));
    assert!(
        runtime
            .initialize_local_execution_bootstrap(&mut engine)
            .is_err()
    );
    assert_eq!(engine.committed_height, 0);
    assert_eq!(state.lock().unwrap().reads, 1);
}
