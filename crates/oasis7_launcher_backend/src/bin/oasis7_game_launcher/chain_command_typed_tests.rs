use super::*;
use oasis7::development_node_config::*;
use std::sync::atomic::{AtomicU64, Ordering};
static SEQUENCE: AtomicU64 = AtomicU64::new(0);
fn options() -> CliOptions {
    CliOptions {
        chain_node_id: format!(
            "typed-launcher-test-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ),
        chain_enabled: true,
        chain_network_tier_manifest: Default::default(),
        chain_node_auto_attest_all_validators: false,
        ..CliOptions::default()
    }
}
fn read_config(args: &[String]) -> DevelopmentNodeConfigV1 {
    assert_eq!(args.len(), 4);
    assert_eq!(args[0], "dev");
    assert_eq!(args[2], "--config");
    serde_json::from_slice(&std::fs::read(&args[3]).unwrap()).unwrap()
}
fn cleanup(args: &[String]) {
    std::fs::remove_dir_all(Path::new(&args[3]).parent().unwrap()).unwrap();
}
#[test]
fn launcher_typed_adapter_emits_dev_node_and_typed_runtime() {
    let mut options = options();
    options.chain_node_role = "observer".into();
    options.chain_node_validators = vec!["remote:25".into()];
    let args = prepare_oasis7_chain_runtime_args(&options).unwrap();
    assert_eq!(args[1], "node");
    let config = read_config(&args);
    assert_eq!(config.api_version, "oasis7.development-node/v1");
    assert_eq!(config.working_directory, std::env::current_dir().unwrap());
    assert_eq!(config.runtime.node_id, options.chain_node_id);
    assert_eq!(
        config.runtime.node_role,
        Some(DevelopmentNodeRoleV1::Observer)
    );
    assert_eq!(
        config.runtime.node_tick_ms,
        Some(options.chain_node_tick_ms)
    );
    assert_eq!(
        config.runtime.node_validators,
        vec![DevelopmentValidatorV1 {
            validator_id: "remote".into(),
            stake: 25
        }]
    );
    assert!(config.local_provider.is_none());
    cleanup(&args);
}
#[test]
fn launcher_typed_adapter_emits_local_provider_exact_funding_lane() {
    let mut options = options();
    options.local_test_provider_authority_path = Some("authority.json".into());
    options.local_test_provider_wasm_path = Some("provider.wasm".into());
    options.local_test_provider_metadata_path = Some("provider.metadata.json".into());
    options.local_test_provider_finality_block_hash = Some("11".repeat(32));
    options.local_test_provider_session_mode = "hosted_public_join".into();
    options.chain_local_standalone_test = true;
    options.agent_decision_source = "provider_backed".into();
    options.agent_provider_backend = "provider_local_mock".into();
    options.agent_provider_contract = "worldsim_provider_v1".into();
    options.agent_provider_transport = "loopback_http".into();
    options.agent_execution_lane = oasis7::simulator::ProviderExecutionMode::PlayerParity;
    let args = prepare_oasis7_chain_runtime_args(&options).unwrap();
    assert_eq!(args[1], "local-provider");
    let config = read_config(&args);
    let provider = config.local_provider.unwrap();
    assert_eq!(provider.authority_path, PathBuf::from("authority.json"));
    assert_eq!(provider.wasm_path, PathBuf::from("provider.wasm"));
    assert_eq!(provider.finality_block_hash, "11".repeat(32));
    assert_eq!(
        provider.session_mode,
        DevelopmentProviderSessionV1::HostedPublicJoin
    );
    assert_eq!(
        provider.funding_lane,
        Some(DevelopmentFundingLaneV1::HostedLocalMock)
    );
    cleanup(&args);
    options.agent_provider_transport = "remote_http".into();
    assert!(prepare_oasis7_chain_runtime_args(&options).is_err());
    assert!(
        !std::env::current_dir()
            .unwrap()
            .join("output/chain-runtime")
            .join(&options.chain_node_id)
            .exists()
    );
}
#[test]
fn launcher_typed_adapter_rejects_formal_synthetic_before_config_effect() {
    let mut options = options();
    options.chain_network_tier_manifest = "formal.json".into();
    options.chain_node_auto_attest_all_validators = true;
    assert!(prepare_oasis7_chain_runtime_args(&options).is_err());
    assert!(
        !std::env::current_dir()
            .unwrap()
            .join("output/chain-runtime")
            .join(&options.chain_node_id)
            .exists()
    );
}

#[test]
fn launcher_typed_adapter_rejects_path_node_ids_before_effects() {
    for id in [
        "../..",
        "/absolute",
        "nested/name",
        "nested\\name",
        ".",
        "..",
        "C:\\absolute",
        "",
        " spaced ",
    ] {
        let mut options = options();
        options.chain_node_id = id.into();
        assert!(
            super::super::cli::validate_chain_node_id(id).is_err(),
            "id={id}"
        );
        assert!(
            prepare_oasis7_chain_runtime_args(&options).is_err(),
            "id={id}"
        );
        assert!(
            super::super::cli::parse_options(["--chain-node-id", id].into_iter()).is_err(),
            "id={id}"
        );
    }
    assert!(super::super::cli::parse_options(["--chain-node-id"].into_iter()).is_err());
    assert!(
        super::super::cli::parse_options(["--chain-node-id", "--chain-disable"].into_iter())
            .is_err()
    );
    for id in ["node01", "node-1", "node_1", "node.1"] {
        super::super::cli::validate_chain_node_id(id).unwrap();
    }
}
