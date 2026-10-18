use super::*;

const WORLD_ID: &str = "formal-node-hook-fixture";

#[derive(Clone)]
struct FakeWriter {
    world: World,
    head: CurrentHeadEvidence,
    head_error: Option<String>,
    next_head: Option<CurrentHeadEvidence>,
    pending: Option<NodeLocalExecutionContinuation>,
    prepared: usize,
    resumed: usize,
    successor_predecessors: Vec<String>,
    proof_record_id: Option<String>,
}

impl FormalNodeWriter for FakeWriter {
    fn world(&self) -> &World {
        &self.world
    }

    fn world_id(&self) -> &str {
        WORLD_ID
    }

    fn current_head_evidence(&self) -> Result<CurrentHeadEvidence, String> {
        if let Some(error) = &self.head_error {
            return Err(error.clone());
        }
        Ok(self.head.clone())
    }

    fn pending_continuation(&self) -> Result<Option<NodeLocalExecutionContinuation>, String> {
        Ok(self.pending.clone())
    }

    fn prepare(&mut self, continuation: &NodeLocalExecutionContinuation) -> Result<(), String> {
        if self
            .pending
            .as_ref()
            .is_some_and(|pending| pending != continuation)
        {
            return Err("fake writer refuses a successor".into());
        }
        self.prepared += 1;
        self.pending = Some(continuation.clone());
        Ok(())
    }

    fn prepare_successor(
        &mut self,
        continuation: &NodeLocalExecutionContinuation,
        exact_terminal_record_id: &str,
    ) -> Result<(), String> {
        let original = self
            .pending
            .as_ref()
            .ok_or("fake writer has no terminal to replace")?;
        let evidence = match &self.head {
            CurrentHeadEvidence::Execution(evidence) => evidence,
            CurrentHeadEvidence::InitialActivation(_) => {
                return Err("fake writer has no qualified terminal".into());
            }
        };
        if original.context.height >= continuation.context.height
            || evidence.formal_record_id != exact_terminal_record_id
            || result_for_continuation(original, evidence).is_err()
        {
            return Err("fake writer refuses unrelated terminal replacement".into());
        }
        self.successor_predecessors
            .push(exact_terminal_record_id.to_string());
        self.prepared += 1;
        self.pending = Some(continuation.clone());
        Ok(())
    }

    fn resume_original(&mut self) -> Result<String, String> {
        let pending = self
            .pending
            .as_ref()
            .ok_or("fake writer has no original continuation")?;
        self.resumed += 1;
        let default_record_id = match &self.next_head {
            Some(CurrentHeadEvidence::Execution(evidence)) => evidence.formal_record_id.clone(),
            _ => match &self.head {
                CurrentHeadEvidence::Execution(evidence) => evidence.formal_record_id.clone(),
                CurrentHeadEvidence::InitialActivation(_) => {
                    return Err("fake writer has no qualified execution result".into());
                }
            },
        };
        if pending.context.height == 0 {
            return Err("fake writer received invalid height".into());
        }
        if let Some(next_head) = self.next_head.take() {
            self.head = next_head;
        }
        Ok(self.proof_record_id.take().unwrap_or(default_record_id))
    }

    fn committed_execution(&self) -> Result<CommittedExecutionEvidence, String> {
        match &self.head {
            CurrentHeadEvidence::Execution(evidence) => Ok(evidence.as_ref().clone()),
            CurrentHeadEvidence::InitialActivation(_) => {
                Err("fake writer has no committed execution".into())
            }
        }
    }
}

fn snapshot_root(world: &World) -> String {
    blake3_hex(&to_cbor(world.snapshot()).expect("encode world snapshot"))
}

#[expect(
    clippy::too_many_arguments,
    reason = "the test builds one exact execution receipt from independently varied fields"
)]
fn execution_evidence(
    world: &World,
    height: u64,
    node_hash: &str,
    execution_hash: &str,
    formal_record_id: &str,
    previous_node_hash: Option<&str>,
    action_root: &str,
    schema_version: u32,
) -> CurrentHeadEvidence {
    let root = snapshot_root(world);
    let mut result = ExecutionBridgeRecord::new_v3(
        WORLD_ID.to_string(),
        height,
        Some(node_hash.to_string()),
        previous_node_hash.map(ToOwned::to_owned),
        "node-a".to_string(),
        action_root.to_string(),
        execution_hash.to_string(),
        root.clone(),
        0,
        "cas:snapshot".to_string(),
        "cas:journal".to_string(),
        None,
        None,
        1,
    );
    result.schema_version = schema_version;
    CurrentHeadEvidence::Execution(Box::new(CommittedExecutionEvidence {
        world_id: WORLD_ID.to_string(),
        canonical_after_state_root: root.clone(),
        world_snapshot_root: root,
        formal_record_id: formal_record_id.to_string(),
        result,
    }))
}

fn initial_activation_evidence(bootstrap: NodeExecutionBootstrap) -> CurrentHeadEvidence {
    CurrentHeadEvidence::InitialActivation(bootstrap)
}

fn world_at_tick(tick: u64) -> World {
    let mut world = oasis7::runtime_host::world::new();
    for _ in 0..tick {
        world.step().expect("advance fixture World time");
    }
    world
}

fn initial_bootstrap(world: &World) -> NodeExecutionBootstrap {
    NodeExecutionBootstrap {
        height: world.state().time,
        consensus_block_hash: "signed-initial-node-hash".to_string(),
        execution_block_hash: "signed-initial-execution-hash".to_string(),
        execution_state_root: snapshot_root(world),
    }
}

fn context(height: u64, node_hash: &str) -> NodeExecutionCommitContext {
    NodeExecutionCommitContext {
        world_id: WORLD_ID.to_string(),
        node_id: "node-a".to_string(),
        proposer_id: "node-a".to_string(),
        height,
        slot: height + 10,
        epoch: 1,
        node_block_hash: node_hash.to_string(),
        action_root: oasis7_node::compute_consensus_action_root(&[])
            .expect("compute empty action root"),
        committed_actions: Vec::new(),
        committed_at_unix_ms: 1,
    }
}

fn continuation(
    world: &World,
    predecessor_height: u64,
    predecessor_node_hash: &str,
    predecessor_execution_hash: &str,
) -> NodeLocalExecutionContinuation {
    let height = predecessor_height + 1;
    NodeLocalExecutionContinuation {
        schema_version: 1,
        context: context(height, &format!("node-h{height}")),
        predecessor_committed_height: predecessor_height,
        predecessor_node_block_hash: Some(predecessor_node_hash.to_string()),
        predecessor_execution_height: predecessor_height,
        predecessor_execution_block_hash: Some(predecessor_execution_hash.to_string()),
        predecessor_execution_state_root: Some(snapshot_root(world)),
        next_height: height,
        next_slot: height + 10,
        reserved_action_bytes: 0,
        approved_stake: 1,
        rejected_stake: 0,
        required_stake: 1,
        total_stake: 1,
    }
}

fn writer(
    world: World,
    head: CurrentHeadEvidence,
    pending: Option<NodeLocalExecutionContinuation>,
) -> FakeWriter {
    FakeWriter {
        world,
        head,
        head_error: None,
        next_head: None,
        pending,
        prepared: 0,
        resumed: 0,
        successor_predecessors: Vec::new(),
        proof_record_id: None,
    }
}

#[test]
fn bootstrap_uses_node_result_hashes_instead_of_formal_record_id() {
    let world = oasis7::runtime_host::world::new();
    let head = execution_evidence(
        &world,
        5,
        "node-h5",
        "execution-h5",
        "formal-canonical-record-9001",
        Some("node-h4"),
        &context(5, "node-h5").action_root,
        EXECUTION_BRIDGE_RECORD_SCHEMA_V3,
    );
    let hook = FormalNodeHook::new(writer(world, head, None)).expect("valid execution head");
    let bootstrap = hook.bootstrap().expect("bootstrap from result artifact");
    assert_eq!(bootstrap.height, 5);
    assert_eq!(bootstrap.consensus_block_hash, "node-h5");
    assert_eq!(bootstrap.execution_block_hash, "execution-h5");
    assert_ne!(
        bootstrap.execution_block_hash,
        "formal-canonical-record-9001"
    );
}

#[test]
fn initial_activation_uses_typed_bootstrap_and_rejects_missing_or_tampered_evidence() {
    let world = world_at_tick(1);
    let bootstrap = initial_bootstrap(&world);
    let hook = FormalNodeHook::new(writer(
        world.clone(),
        initial_activation_evidence(bootstrap.clone()),
        None,
    ))
    .expect("typed bootstrap returned by verified writer closure");
    assert_eq!(hook.bootstrap().unwrap(), bootstrap);

    let mut missing = writer(
        world.clone(),
        initial_activation_evidence(bootstrap.clone()),
        None,
    );
    missing.head_error = Some("signed InitialInput bootstrap closure unavailable".into());
    assert!(FormalNodeHook::new(missing).is_err());

    let mut wrong_height = initial_bootstrap(&world);
    wrong_height.height += 1;
    assert!(
        FormalNodeHook::new(writer(
            world.clone(),
            initial_activation_evidence(wrong_height),
            None,
        ))
        .is_err()
    );

    let mut wrong_root = initial_bootstrap(&world);
    wrong_root.execution_state_root = "not-the-world-root".to_string();
    assert!(
        FormalNodeHook::new(writer(
            world.clone(),
            initial_activation_evidence(wrong_root),
            None,
        ))
        .is_err()
    );
}

#[test]
fn pending_zero_predecessor_requires_verified_genesis_read_cut() {
    let world = oasis7::runtime_host::world::new();
    let zero_height = continuation(&world, 0, "node-genesis", "execution-genesis");
    let error = match FormalNodeHook::new(writer(
        world.clone(),
        initial_activation_evidence(initial_bootstrap(&world)),
        Some(zero_height),
    )) {
        Err(error) => error,
        Ok(_) => panic!("raw InitialActivation bootstrap cannot authorize predecessor zero"),
    };
    assert!(error.contains("does not expose a verified committed read cut"));
}

#[test]
fn terminal_pending_bootstraps_from_original_predecessor_not_published_result() {
    let world = oasis7::runtime_host::world::new();
    let action_root = context(8, "node-h8").action_root;
    let current_head = execution_evidence(
        &world,
        8,
        "node-h8",
        "execution-h8",
        "formal-record-8",
        Some("node-h7"),
        &action_root,
        EXECUTION_BRIDGE_RECORD_SCHEMA_V3,
    );
    let pending = continuation(&world, 7, "node-h7", "execution-h7");
    let hook = FormalNodeHook::new(writer(world, current_head, Some(pending)))
        .expect("restore from exact original predecessor");
    let bootstrap = hook.bootstrap().expect("pending predecessor boundary");
    assert_eq!(bootstrap.height, 7);
    assert_eq!(bootstrap.consensus_block_hash, "node-h7");
    assert_eq!(bootstrap.execution_block_hash, "execution-h7");
}

#[test]
fn prepare_is_durable_before_execution_and_successor_cannot_replace_pending() {
    let world = oasis7::runtime_host::world::new();
    let action_root = context(5, "node-h5").action_root;
    let head = execution_evidence(
        &world,
        5,
        "node-h5",
        "execution-h5",
        "formal-record-5",
        Some("node-h4"),
        &action_root,
        EXECUTION_BRIDGE_RECORD_SCHEMA_V3,
    );
    let mut hook = FormalNodeHook::new(writer(world.clone(), head, None)).expect("valid head");
    let original = continuation(&world, 5, "node-h5", "execution-h5");
    hook.prepare_local_continuation(&original)
        .expect("persist original Frozen request");
    assert_eq!(hook.writer.prepared, 1);
    assert_eq!(hook.writer.resumed, 0, "prepare must not execute the World");
    assert_eq!(
        hook.pending_local_continuation().unwrap(),
        Some(original.clone())
    );

    let successor = continuation(&world, 5, "node-h5", "execution-h5");
    let mut successor = successor;
    successor.context.node_block_hash = "node-successor".to_string();
    assert!(hook.prepare_local_continuation(&successor).is_err());
    assert_eq!(hook.pending_local_continuation().unwrap(), Some(original));
    assert_eq!(hook.writer.prepared, 1);
    assert_eq!(hook.writer.resumed, 0);
}

#[test]
fn completion_keeps_terminal_until_exact_successor_replaces_it() {
    let world = oasis7::runtime_host::world::new();
    let action_root = context(5, "node-h5").action_root;
    let predecessor = execution_evidence(
        &world,
        5,
        "node-h5",
        "execution-h5",
        "formal-record-5",
        Some("node-h4"),
        &action_root,
        EXECUTION_BRIDGE_RECORD_SCHEMA_V3,
    );
    let committed = execution_evidence(
        &world,
        6,
        "node-h6",
        "execution-h6-actual",
        "formal-record-6",
        Some("node-h5"),
        &context(6, "node-h6").action_root,
        EXECUTION_BRIDGE_RECORD_SCHEMA_V3,
    );
    let mut fake = writer(world.clone(), predecessor, None);
    fake.next_head = Some(committed);
    let mut hook = FormalNodeHook::new(fake).expect("valid predecessor");
    let original = continuation(&world, 5, "node-h5", "execution-h5");
    hook.prepare_local_continuation(&original).unwrap();

    assert!(hook.on_commit(original.context.clone()).is_err());
    assert!(
        hook.on_commit_with_expected(original.context.clone(), Some("peer-hash"), None)
            .is_err()
    );
    assert!(
        hook.on_commit_with_expected(original.context.clone(), None, Some("peer-root"))
            .is_err()
    );
    assert_eq!(hook.writer.resumed, 0);

    let outcome = hook
        .on_commit_outcome_with_expected(original.context.clone(), None, None)
        .expect("resume exact decision to dual-qualified result");
    let NodeExecutionCommitOutcome::Applied(result) = outcome else {
        panic!("a qualified original result must be Applied");
    };
    assert_eq!(result.execution_height, 6);
    assert_eq!(result.execution_block_hash, "execution-h6-actual");
    assert_eq!(hook.writer.resumed, 1);

    let mut successor_context = original.context.clone();
    successor_context.height += 1;
    assert!(
        hook.complete_local_continuation(&successor_context)
            .is_err()
    );
    assert!(hook.writer.successor_predecessors.is_empty());
    hook.complete_local_continuation(&original.context)
        .expect("mark only exact terminal context consumed in memory");
    assert_eq!(
        hook.writer.pending.as_ref(),
        Some(&original),
        "completion must keep the durable terminal for restart replay"
    );
    assert!(hook.pending_local_continuation().unwrap().is_none());
    let mut restarted = FormalNodeHook::new(hook.writer.clone())
        .expect("a restart must initialize from the persisted terminal receipt");
    assert_eq!(
        restarted.pending_local_continuation().unwrap(),
        Some(original.clone())
    );
    assert_eq!(restarted.bootstrap().unwrap().height, 5);

    let next = continuation(&world, 6, "node-h6", "execution-h6-actual");
    hook.prepare_local_continuation(&next)
        .expect("atomically replace consumed terminal with the exact successor");
    assert_eq!(hook.writer.successor_predecessors, ["formal-record-6"]);
    assert_eq!(hook.pending_local_continuation().unwrap(), Some(next));
}

#[test]
fn proof_head_record_mismatch_never_returns_applied_or_clears_pending() {
    let world = oasis7::runtime_host::world::new();
    let action_root = context(5, "node-h5").action_root;
    let predecessor = execution_evidence(
        &world,
        5,
        "node-h5",
        "execution-h5",
        "formal-record-5",
        Some("node-h4"),
        &action_root,
        EXECUTION_BRIDGE_RECORD_SCHEMA_V3,
    );
    let committed = execution_evidence(
        &world,
        6,
        "node-h6",
        "execution-h6",
        "formal-record-6",
        Some("node-h5"),
        &context(6, "node-h6").action_root,
        EXECUTION_BRIDGE_RECORD_SCHEMA_V3,
    );
    let mut fake = writer(world.clone(), predecessor, None);
    fake.next_head = Some(committed);
    fake.proof_record_id = Some("different-formal-record".to_string());
    let mut hook = FormalNodeHook::new(fake).expect("valid predecessor");
    let original = continuation(&world, 5, "node-h5", "execution-h5");
    hook.prepare_local_continuation(&original).unwrap();

    let error = hook
        .on_commit_outcome_with_expected(original.context.clone(), None, None)
        .expect_err("proof must identify the exact qualified publication");
    assert!(error.contains("proof and qualified publication head differ"));
    assert_eq!(hook.writer.resumed, 1);
    assert_eq!(hook.pending_local_continuation().unwrap(), Some(original));
    assert!(hook.writer.successor_predecessors.is_empty());
}

#[test]
fn execution_bootstrap_rejects_non_v3_or_world_root_mismatch() {
    let world = oasis7::runtime_host::world::new();
    let action_root = context(5, "node-h5").action_root;
    let old_schema = execution_evidence(
        &world,
        5,
        "node-h5",
        "execution-h5",
        "formal-record-5",
        Some("node-h4"),
        &action_root,
        super::super::EXECUTION_BRIDGE_RECORD_SCHEMA_V2,
    );
    assert!(FormalNodeHook::new(writer(world.clone(), old_schema, None)).is_err());

    let mut wrong_root = execution_evidence(
        &world,
        5,
        "node-h5",
        "execution-h5",
        "formal-record-5",
        Some("node-h4"),
        &action_root,
        EXECUTION_BRIDGE_RECORD_SCHEMA_V3,
    );
    if let CurrentHeadEvidence::Execution(evidence) = &mut wrong_root {
        evidence.result.execution_state_root = "another-world-root".into();
    }
    assert!(FormalNodeHook::new(writer(world, wrong_root, None)).is_err());
}
