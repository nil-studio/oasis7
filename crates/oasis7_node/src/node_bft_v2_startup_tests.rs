use super::*;
use ed25519_dalek::SigningKey;
use oasis7_consensus::tendermint_v1::BftTimeoutsV1;
use oasis7_consensus::tendermint_v2::BftSchedulePolicyV2;
use oasis7_distfs::protected_bft_v2::{
    BftSignerBindingV2, BftSignerKeyIdentityV2, BftSignerTrustV2, RemoteBftSignerProviderV2,
};
use oasis7_proto::distributed_bft::v2::{BftScopeV2, BftValidatorV2};
use std::net::TcpListener;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

struct LockedDriver(Arc<AtomicUsize>);

impl NodeExecutionHook for LockedDriver {
    fn on_commit(
        &mut self,
        _: crate::NodeExecutionCommitContext,
    ) -> Result<crate::NodeExecutionCommitResult, String> {
        Err("legacy commit is forbidden in this V2 fixture".into())
    }

    fn configure_bft_v2_execution(
        &mut self,
        _: &crate::NodeBftExecutionConfigV2,
    ) -> Result<Option<crate::NodeBftExecutionHeadV2>, String> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Err("execution lifecycle lock is already held".into())
    }
}

#[test]
fn local_driver_lock_failure_never_contacts_or_fences_the_remote_provider() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let validator = SigningKey::from_bytes(&[91; 32]);
    let requester = SigningKey::from_bytes(&[92; 32]);
    let provider_key = SigningKey::from_bytes(&[93; 32]);
    let public = hex::encode(validator.verifying_key().to_bytes());
    let binding = BftSignerBindingV2 {
        schema_version: 2,
        scope: BftScopeV2 {
            world_id: "startup-lock-world".into(),
            chain_id: "startup-lock-chain".into(),
            genesis_hash: "aa".repeat(32),
        },
        genesis_authority_epoch: 1,
        genesis_validators: vec![BftValidatorV2 {
            validator_id: "validator".into(),
            public_key_hex: public.clone(),
            voting_power: 1,
        }],
        genesis_state_root: "bb".repeat(32),
        genesis_manifest_hash: "cc".repeat(32),
        schedule_policy: BftSchedulePolicyV2::new(4, "engineering_tendermint_v2").unwrap(),
        local_validator_id: "validator".into(),
        provisioned_signing_keys: vec![BftSignerKeyIdentityV2 {
            validator_id: "validator".into(),
            public_key_hex: public,
        }],
        timeouts: BftTimeoutsV1 {
            propose_ms: 100,
            prevote_ms: 100,
            precommit_ms: 100,
            round_delta_ms: 10,
        },
        operator_instance_id: "startup-lock-instance".into(),
        requester_auth_public_key: hex::encode(requester.verifying_key().to_bytes()),
    };
    let trust = BftSignerTrustV2 {
        schema_version: 2,
        provider_public_key: hex::encode(provider_key.verifying_key().to_bytes()),
        binding_digest: binding.digest().unwrap(),
    };
    let provider = Arc::new(
        RemoteBftSignerProviderV2::new(
            listener.local_addr().unwrap(),
            Duration::from_millis(100),
            binding.clone(),
            requester,
            trust,
        )
        .unwrap(),
    );
    // This path is configuration only: the locked hook must fail before any
    // storage initialization or TCP exchange can occur.
    let config = crate::NodeConfig::new(
        "validator",
        "startup-lock-world",
        crate::NodeRole::Sequencer,
    )
    .unwrap()
    .with_replication(
        crate::NodeReplicationConfig::new(
            std::env::temp_dir().join("oasis7-v2-lock-test-unused-storage"),
        )
        .unwrap(),
    );
    let calls = Arc::new(AtomicUsize::new(0));
    let mut runtime = NodeRuntime::new(config)
        .with_replication_network_consensus_enabled(false)
        .with_execution_hook(LockedDriver(calls.clone()))
        .with_tendermint_v2(binding, provider)
        .unwrap();
    let failure = start(&mut runtime).unwrap_err();
    assert!(
        failure
            .to_string()
            .contains("lifecycle lock is already held")
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert!(!runtime.running.load(Ordering::SeqCst));
    assert!(runtime.worker.is_none());
    assert_eq!(
        listener.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
}

#[test]
fn remote_v2_candidate_cannot_bypass_main_token_action_authentication() {
    let scope = BftScopeV2 {
        world_id: "asset-auth-world".into(),
        chain_id: "asset-auth-chain".into(),
        genesis_hash: "aa".repeat(32),
    };
    let key = SigningKey::from_bytes(&[94; 32]);
    let set = BftValidatorSetV2::new(
        scope.clone(),
        vec![BftValidatorV2 {
            validator_id: "validator".into(),
            public_key_hex: hex::encode(key.verifying_key().to_bytes()),
            voting_power: 1,
        }],
    )
    .unwrap();
    let schedule = BftHeightProposerScheduleV2::genesis(
        set,
        BftSchedulePolicyV2::new(4, "engineering_tendermint_v2").unwrap(),
        1,
        "bb".repeat(32),
        "cc".repeat(32),
    )
    .unwrap();
    let payload = serde_cbor::to_vec(&serde_json::json!({
        "version": 1,
        "auth": null,
        "body": {"type": "runtime_action", "data": {"action": {
            "type": "TransferMainToken",
            "data": {
                "from_account_id": "account:sender",
                "to_account_id": "protocol:receiver",
                "amount": 7,
                "nonce": 1
            }
        }}}
    }))
    .unwrap();
    let actions = vec![crate::NodeConsensusAction::from_payload(1, "sender", payload).unwrap()];
    let block = oasis7_proto::distributed_bft::v2::BftBlockV2 {
        protocol_version: 2,
        scope,
        height: 1,
        parent_commit_ref: schedule.parent_commit_ref().into(),
        authority_epoch: 1,
        active_validator_set_hash: schedule.active_set().set_hash().into(),
        schedule_policy_hash: schedule.policy_hash().into(),
        manifest_hash: schedule.parent_manifest_hash().into(),
        post_manifest_hash: schedule.parent_manifest_hash().into(),
        actions_cbor: serde_cbor::to_vec(&actions).unwrap(),
        action_root: "dd".repeat(32),
        event_root: "ee".repeat(32),
        execution_block_hash: "ff".repeat(32),
        execution_state_root: "11".repeat(32),
        state_root: "22".repeat(32),
        transition: None,
        transition_ref: None,
    };
    let config =
        crate::NodeConfig::new("validator", "asset-auth-world", crate::NodeRole::Sequencer)
            .unwrap();
    let hook: Hook = Arc::new(Mutex::new(Box::new(LockedDriver(Arc::new(
        AtomicUsize::new(0),
    )))));
    let failure = validate_candidate_with_hook(&config, &hook, &schedule, &block).unwrap_err();
    assert!(failure.to_string().contains("missing_main_token_auth"));
}
