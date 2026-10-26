use super::*;

#[test]
#[cfg(unix)]
fn formal_read_ledger_refuses_engineering_and_legacy_entrypoints() {
    std::thread::Builder::new()
        .name("formal-bft-profile-isolation".into())
        .stack_size(32 * 1024 * 1024)
        .spawn(formal_read_profile_isolation_case)
        .expect("spawn large-stack profile isolation fixture")
        .join()
        .expect("formal BFT profile isolation case");
}

#[cfg(unix)]
fn formal_read_profile_isolation_case() {
    let h = qualified_handoff();

    let v1_scratch = Scratch::new();
    seed_formal_ledger(&h, &v1_scratch);
    let v1_scope = oasis7_node::BftScopeV1 {
        world_id: h.target.scope().world_id.clone(),
        chain_id: h.target.scope().chain_id.clone(),
        genesis_hash: h.target.scope().genesis_hash.clone(),
    };
    let mut v1_driver = driver(v1_scratch.path(), empty_bft_genesis_world(&v1_scope));
    let v1_validators = h
        .target
        .initial_validators()
        .iter()
        .map(|validator| oasis7_node::BftValidatorV1 {
            validator_id: validator.validator_id.clone(),
            public_key_hex: validator.public_key_hex.clone(),
            voting_power: validator.voting_power,
        })
        .collect();
    let v1_set = oasis7_consensus::tendermint_v1::BftValidatorSetV1::new(v1_scope, v1_validators)
        .expect("build valid engineering V1 set");
    let v1_config = oasis7_node::NodeBftExecutionConfigV1::from_validator_set(
        &v1_set,
        h.target.authority_epoch(),
    );
    let v1_result = v1_driver.configure_bft(&v1_config);
    let v1_error = v1_result.expect_err("engineering V1 must refuse formal-read material");
    assert!(
        v1_error.contains("formal BFT read"),
        "engineering V1 must identify the conflicting durable profile: {v1_error}"
    );
    drop(v1_driver);
    assert!(
        !v1_scratch.path().join("records/bft-execution-v1").exists(),
        "refusal must not create an engineering V1 ledger"
    );

    let v2_scratch = Scratch::new();
    seed_formal_ledger(&h, &v2_scratch);
    let v2_scope = h.target.scope();
    let mut v2_driver = driver(
        v2_scratch.path(),
        empty_bft_genesis_world(&oasis7_node::BftScopeV1 {
            world_id: v2_scope.world_id.clone(),
            chain_id: v2_scope.chain_id.clone(),
            genesis_hash: v2_scope.genesis_hash.clone(),
        }),
    );
    let v2_genesis_state_root = v2_driver
        .execution_world
        .execution_state_identity()
        .expect("read BFT V2 test genesis state root")
        .state_root_hash;
    let v2_genesis_manifest_hash = v2_driver
        .execution_world
        .current_manifest_hash()
        .expect("read BFT V2 test genesis manifest");
    let v2_config = oasis7_node::NodeBftExecutionConfigV2 {
        scope: v2_scope.clone(),
        authority_epoch: h.target.authority_epoch(),
        initial_validators: h.target.initial_validators().to_vec(),
        schedule_policy: h.target.schedule_policy().clone(),
        genesis_state_root: v2_genesis_state_root,
        genesis_manifest_hash: v2_genesis_manifest_hash,
    };
    let v2_result = v2_driver.configure_bft_v2(&v2_config);
    assert!(
        v2_result
            .expect_err("engineering V2 must refuse formal-read material")
            .contains("formal BFT read"),
        "engineering V2 must identify the conflicting durable profile"
    );
    drop(v2_driver);
    assert!(
        !v2_scratch.path().join("records/bft-execution-v2").exists(),
        "refusal must not create an engineering V2 ledger"
    );

    let legacy_scratch = Scratch::new();
    seed_formal_ledger(&h, &legacy_scratch);
    let legacy_result = NodeRuntimeExecutionDriver::new(
        legacy_scratch.path().join("state.json"),
        legacy_scratch.path().join("world"),
        legacy_scratch.path().join("records"),
        legacy_scratch.path().join("cas"),
    );
    let legacy_error = match legacy_result {
        Ok(_) => panic!("legacy World startup must refuse formal-read material"),
        Err(error) => error,
    };
    assert!(
        legacy_error.contains("formal BFT read"),
        "legacy startup must identify the conflicting durable profile"
    );
    assert!(
        !legacy_scratch.path().join("world").exists(),
        "refusal must precede legacy World bootstrap"
    );
}

#[cfg(unix)]
fn seed_formal_ledger(h: &QualifiedH, scratch: &Scratch) {
    let reader: Arc<dyn BusinessIndexNodeReader + Send + Sync> = h.reader.clone();
    let mut runtime = driver(scratch.path(), h.world.clone());
    let execution = runtime
        .open_formal_bft_read_v1(
            h.anchor.clone(),
            h.target.clone(),
            h.material.clone(),
            reader,
            true,
        )
        .expect("create exact formal-read ledger for profile fence");
    drop(execution);
}

#[cfg(unix)]
fn empty_bft_genesis_world(scope: &oasis7_node::BftScopeV1) -> RuntimeWorld {
    let world = oasis7::runtime_host::world::new_production_hardened();
    let manifest = world.chain_resource_manifest();
    let snapshot = world.snapshot_with_chain_resource_context(
        oasis7_runtime::ChainResourceDerivationContext {
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
        .expect("bind empty BFT test World to the requested genesis identity")
}
