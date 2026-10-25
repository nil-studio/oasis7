use super::*;
use ed25519_dalek::SigningKey;
use oasis7::consensus_action_payload::{
    ConsensusActionPayloadEnvelope, encode_consensus_action_payload,
};
use oasis7_node::{
    BftBlockV1, BftCommitCertificateV1, BftHeightProposerScheduleV1, BftProposalHeaderV1,
    BftProposalV1, BftScopeV1, BftValidatorSetV1, BftValidatorV1, BftVotePhaseV1, BftVoteV1,
    BftVoteValueV1, NodeBftBuildContextV1, NodeBftExecutionConfigV1, NodeConsensusAction,
    NodeExecutionCommitContext, NodeExecutionHook, compute_consensus_action_root,
    verify_commit_certificate_token_v1,
};
use oasis7_runtime::{
    Action as RuntimeAction, ChainResourceDerivationContext, World as RuntimeWorld,
};
use oasis7_wasm_executor::{WasmExecutor, WasmExecutorConfig};

pub(super) fn keys() -> Vec<SigningKey> {
    (1..=4)
        .map(|seed| SigningKey::from_bytes(&[seed; 32]))
        .collect()
}

pub(super) fn scope() -> BftScopeV1 {
    BftScopeV1 {
        world_id: "driver-bft-world".into(),
        chain_id: "driver-bft-chain".into(),
        genesis_hash: "11".repeat(32),
    }
}

pub(super) fn validator_set(scope: &BftScopeV1, keys: &[SigningKey]) -> BftValidatorSetV1 {
    BftValidatorSetV1::new(
        scope.clone(),
        keys.iter()
            .enumerate()
            .map(|(index, key)| BftValidatorV1 {
                validator_id: format!("v{index}"),
                public_key_hex: hex::encode(key.verifying_key().to_bytes()),
                voting_power: 1,
            })
            .collect(),
    )
    .expect("valid fixed BFT set")
}

pub(super) fn bft_genesis_world(scope: &BftScopeV1) -> RuntimeWorld {
    let world = oasis7::runtime_host::world::new_production_hardened();
    let manifest = world.chain_resource_manifest();
    let snapshot = world.snapshot_with_chain_resource_context(
        ChainResourceDerivationContext {
            world_id: &scope.world_id,
            chain_id: &scope.chain_id,
            genesis_ref: Some(&scope.genesis_hash),
            created_at_height: 0,
            manifest_height: 0,
            commit_block_hash: Some(&scope.genesis_hash),
            tick: world.state().time,
        },
        manifest.world_config_hash.clone(),
        manifest.generation_algorithm_hash.clone(),
    );
    world
        .restore_candidate_snapshot_with_attached_formal_history(snapshot, world.journal().clone())
        .expect("bind the existing empty World genesis identity")
}

fn runtime_action() -> NodeConsensusAction {
    let envelope =
        ConsensusActionPayloadEnvelope::from_runtime_action(RuntimeAction::RegisterAgent {
            agent_id: "bft-agent".into(),
            pos: oasis7::geometry::GeoPos::new(1, 2, 3),
        });
    let payload = encode_consensus_action_payload(&envelope).expect("encode runtime action");
    NodeConsensusAction::from_payload(1, "v0", payload).expect("build consensus action")
}

fn signed_certificate(
    set: &BftValidatorSetV1,
    keys: &[SigningKey],
    schedule: &BftHeightProposerScheduleV1,
    block: BftBlockV1,
) -> BftCommitCertificateV1 {
    let mut priority = schedule.starting_priority().clone();
    let proposer = priority
        .next_proposer(set)
        .expect("deterministic height proposer");
    let proposer_index = proposer.validator_id[1..]
        .parse::<usize>()
        .expect("validator index");
    let block_id = block.block_id().expect("block ID");
    let proposal = BftProposalV1 {
        header: BftProposalHeaderV1::sign(
            set.scope().clone(),
            schedule.height(),
            0,
            set.set_hash().to_string(),
            proposer.validator_id,
            block_id.clone(),
            None,
            &keys[proposer_index],
        )
        .expect("sign proposal"),
        block,
        valid_round_evidence: None,
    };
    let precommits = keys
        .iter()
        .take(3)
        .enumerate()
        .map(|(index, key)| {
            BftVoteV1::sign(
                set.scope().clone(),
                schedule.height(),
                0,
                BftVotePhaseV1::Precommit,
                set.set_hash().to_string(),
                BftVoteValueV1::Block {
                    block_id: block_id.clone(),
                },
                format!("v{index}"),
                key,
            )
            .expect("sign precommit")
        })
        .collect();
    BftCommitCertificateV1 {
        proposal: Box::new(proposal),
        precommits,
    }
}

pub(super) fn driver_for_world(
    root: &std::path::Path,
    world: RuntimeWorld,
) -> NodeRuntimeExecutionDriver {
    NodeRuntimeExecutionDriver::new_with_sandbox(
        root.join("state.json"),
        root.join("world"),
        root.join("records"),
        root.join("cas"),
        ExecutionBridgeState::default(),
        world,
        Box::new(WasmExecutor::new(WasmExecutorConfig::default()).expect("executor")),
        EXECUTION_BRIDGE_DEFAULT_HOT_WINDOW_HEIGHTS,
        EXECUTION_BRIDGE_DEFAULT_CHECKPOINT_INTERVAL_HEIGHTS,
        EXECUTION_BRIDGE_DEFAULT_CHECKPOINT_KEEP_LATEST,
    )
}

#[test]
fn fixed_set_bft_driver_previews_without_mutation_commits_and_replays_exact_qc_history() {
    let root = temp_dir("bft-driver-replay");
    let scope = scope();
    let keys = keys();
    let set = validator_set(&scope, &keys);
    let config = NodeBftExecutionConfigV1::from_validator_set(&set, 9);
    let genesis_schedule =
        BftHeightProposerScheduleV1::genesis(&set, config.authority_epoch).expect("schedule");
    let mut driver = driver_for_world(&root, bft_genesis_world(&scope));
    driver
        .configure_bft_execution(&config)
        .expect("pin BFT config");

    let legacy_commit = driver
        .on_commit(NodeExecutionCommitContext {
            world_id: scope.world_id.clone(),
            node_id: "v0".into(),
            proposer_id: "v0".into(),
            height: 1,
            slot: 0,
            epoch: 9,
            node_block_hash: "22".repeat(32),
            action_root: compute_consensus_action_root(&[]).expect("empty root"),
            committed_actions: Vec::new(),
            committed_at_unix_ms: 1,
        })
        .expect_err("the Pos commit path must remain fenced");
    assert!(legacy_commit.contains("BFT mode fence"));

    let action = runtime_action();
    let context = driver
        .current_bft_execution_context(&scope, 1, &scope.genesis_hash, config.authority_epoch)
        .expect("current authenticated execution context");
    let before_snapshot = driver.execution_world.snapshot();
    let before_journal = driver.execution_world.journal().clone();
    let candidate = driver
        .build_bft_candidate(NodeBftBuildContextV1 {
            execution: context.clone(),
            actions: vec![action.clone()],
        })
        .expect("build deterministic candidate");
    driver
        .preview_bft_candidate(&context, &candidate)
        .expect("independent candidate replay");
    assert_eq!(driver.execution_world.snapshot(), before_snapshot);
    assert_eq!(driver.execution_world.journal(), &before_journal);
    assert!(
        !root.join("world").exists(),
        "preview must not persist World files"
    );

    let certificate = signed_certificate(&set, &keys, &genesis_schedule, candidate.clone());
    let token = verify_commit_certificate_token_v1(&set, &genesis_schedule, &certificate)
        .expect("independently verified fixed-set QC");
    let committed = driver
        .commit_bft_certificate(token)
        .expect("apply certified BFT execution");
    assert_eq!(committed.height, 1);
    assert_eq!(
        committed.bft_commit_ref,
        candidate.block_id().expect("block ID")
    );
    assert_eq!(driver.execution_world.state().time, 1);
    assert!(
        driver
            .execution_world
            .state()
            .agents
            .contains_key("bft-agent")
    );
    assert!(
        root.join("records/bft-execution-v1/height-00000000000000000001.cbor")
            .is_file()
    );
    assert!(!root.join("records/00000000000000000001.json").exists());
    assert!(!root.join("records/latest.json").exists());

    let repeated = verify_commit_certificate_token_v1(&set, &genesis_schedule, &certificate)
        .expect("retry token is reverified from the same fixed schedule");
    let idempotent = driver
        .commit_bft_certificate(repeated)
        .expect("exact QC retry returns original result");
    assert_eq!(idempotent, committed);
    assert_eq!(driver.execution_world.state().time, 1);

    let mut forked_first = candidate.clone();
    forked_first.event_root = "aa".repeat(32);
    let fork_certificate = signed_certificate(&set, &keys, &genesis_schedule, forked_first);
    let fork_token = verify_commit_certificate_token_v1(&set, &genesis_schedule, &fork_certificate)
        .expect("a separately valid QC chain is cryptographically valid");
    let fork_next_schedule = fork_token.next_height_schedule().clone();
    assert!(driver.commit_bft_certificate(fork_token).is_err());
    let mut forked_second = candidate;
    forked_second.height = 2;
    forked_second.parent_commit_ref = fork_next_schedule.parent_commit_ref().to_string();
    let fork_second_certificate =
        signed_certificate(&set, &keys, &fork_next_schedule, forked_second);
    let fork_second_token =
        verify_commit_certificate_token_v1(&set, &fork_next_schedule, &fork_second_certificate)
            .expect("fork continuation has its own valid schedule and QC");
    assert!(driver.commit_bft_certificate(fork_second_token).is_err());
    assert_eq!(driver.execution_world.state().time, 1);

    let mut changed_config = config.clone();
    changed_config.authority_epoch += 1;
    assert!(driver.configure_bft_execution(&changed_config).is_err());
    assert_eq!(driver.execution_world.state().time, 1);

    let stopped_restart_head = driver
        .configure_bft_execution(&config)
        .expect("same-instance stop/start keeps the open ledger and pinned config");
    assert_eq!(
        stopped_restart_head.as_ref().map(|head| head.height),
        Some(1)
    );
    assert_eq!(
        stopped_restart_head
            .as_ref()
            .map(|head| head.block_id.as_str()),
        Some(committed.bft_commit_ref.as_str())
    );
    assert_eq!(driver.execution_world.state().time, 0);
    let replay_page = driver
        .load_bft_commit_history_page(&config, 0, 64)
        .expect("same-instance restart replays raw history from genesis");
    let replay_token = verify_commit_certificate_token_v1(&set, &genesis_schedule, &replay_page[0])
        .expect("reverify original fixed-set QC");
    driver
        .commit_bft_certificate(replay_token)
        .expect("same-instance restart installs the exact original execution");
    assert_eq!(driver.execution_world.state().time, 1);

    drop(driver);
    let mut restarted = NodeRuntimeExecutionDriver::new(
        root.join("state.json"),
        root.join("world"),
        root.join("records"),
        root.join("cas"),
    )
    .expect("BFT marker must select the BFT-aware inert startup path");
    let head = restarted
        .configure_bft_execution(&config)
        .expect("reopen exact fixed BFT config");
    assert_eq!(head.as_ref().map(|value| value.height), Some(1));
    let page = restarted
        .load_bft_commit_history_page(&config, 0, 64)
        .expect("load raw BFT evidence");
    assert_eq!(page, vec![certificate.clone()]);
    let cold_token = verify_commit_certificate_token_v1(&set, &genesis_schedule, &page[0])
        .expect("cold recovery independently re-verifies the raw QC");
    restarted
        .commit_bft_certificate(cold_token)
        .expect("replay exact committed execution from genesis");
    assert_eq!(restarted.execution_world.state().time, 1);
    assert!(
        restarted
            .execution_world
            .state()
            .agents
            .contains_key("bft-agent")
    );

    drop(restarted);
    let _ = fs::remove_dir_all(root);
}
