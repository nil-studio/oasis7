use super::*;
use crate::node_bft_v2_test_provider::ProtectedBftV2TestProvider;
use crate::{NodeConsensusAction, NodeExecutionCommitContext, NodeExecutionCommitResult, NodeRole};
use ed25519_dalek::SigningKey;
use oasis7_consensus::tendermint_v1::BftTimeoutsV1;
use oasis7_consensus::tendermint_v2::BftSchedulePolicyV2;
use oasis7_distfs::protected_bft_v2::{BftSignerBindingV2, BftSignerKeyIdentityV2};
use oasis7_proto::distributed_bft::v2::{BftScopeV2, BftValidatorV2};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

#[derive(Clone)]
struct AdmissionHook {
    legacy_preflight_calls: Arc<AtomicUsize>,
    validated_batches: Arc<Mutex<Vec<Vec<NodeConsensusAction>>>>,
}

impl crate::NodeExecutionHook for AdmissionHook {
    fn on_commit(
        &mut self,
        _: NodeExecutionCommitContext,
    ) -> Result<NodeExecutionCommitResult, String> {
        Err("admission-only hook cannot commit".into())
    }

    fn validate_governance_action_payload(&self, _: &[u8]) -> Result<(), String> {
        self.legacy_preflight_calls.fetch_add(1, Ordering::SeqCst);
        Err("legacy governance preflight does not use the V2 parent".into())
    }

    fn validate_bft_v2_action_payload(&mut self, payload: &[u8]) -> Result<(), String> {
        let envelope: serde_json::Value = serde_cbor::from_slice(payload)
            .map_err(|error| format!("invalid V2 action envelope: {error}"))?;
        if envelope.get("body").is_none() || envelope.get("auth").is_none() {
            return Err("V2 action envelope is missing body/auth fields".into());
        }
        Ok(())
    }

    fn validate_bft_v2_action_batch(
        &mut self,
        _: &BftHeightProposerScheduleV2,
        actions: &[NodeConsensusAction],
    ) -> Result<(), String> {
        self.validated_batches
            .lock()
            .map_err(|_| "V2 admission batch log poisoned".to_string())?
            .push(actions.to_vec());
        Ok(())
    }
}

fn binding_and_fixture() -> (BftSignerBindingV2, ProtectedBftV2TestProvider) {
    let validator = SigningKey::from_bytes(&[91; 32]);
    let requester = SigningKey::from_bytes(&[92; 32]);
    let provider_key = SigningKey::from_bytes(&[93; 32]);
    let public_key_hex = hex::encode(validator.verifying_key().to_bytes());
    let binding = BftSignerBindingV2 {
        schema_version: 2,
        scope: BftScopeV2 {
            world_id: "v2-governance-admission-world".into(),
            chain_id: "v2-governance-admission-chain".into(),
            genesis_hash: "aa".repeat(32),
        },
        genesis_authority_epoch: 1,
        genesis_validators: vec![BftValidatorV2 {
            validator_id: "validator".into(),
            public_key_hex: public_key_hex.clone(),
            voting_power: 1,
        }],
        genesis_state_root: "bb".repeat(32),
        genesis_manifest_hash: "cc".repeat(32),
        schedule_policy: BftSchedulePolicyV2::new(4, "engineering_tendermint_v2")
            .expect("valid V2 schedule policy"),
        local_validator_id: "validator".into(),
        provisioned_signing_keys: vec![BftSignerKeyIdentityV2 {
            validator_id: "validator".into(),
            public_key_hex,
        }],
        timeouts: BftTimeoutsV1 {
            propose_ms: 100,
            prevote_ms: 100,
            precommit_ms: 100,
            round_delta_ms: 10,
        },
        operator_instance_id: "v2-governance-admission-instance".into(),
        requester_auth_public_key: hex::encode(requester.verifying_key().to_bytes()),
    };
    let fixture = ProtectedBftV2TestProvider::start(
        binding.clone(),
        requester,
        provider_key,
        vec![validator],
    )
    .expect("start actual loopback signer service");
    (binding, fixture)
}

fn signed_governance_payload() -> Vec<u8> {
    serde_cbor::to_vec(&serde_json::json!({
        "version": 1,
        "auth": {
            "type": "governance_action",
            "data": {
                "schema_version": 1,
                "signatures": {"parent-policy-key": "signed-by-parent-policy"}
            }
        },
        "body": {
            "type": "runtime_action",
            "data": {"action": {
                "type": "SubmitGovernanceValidatorAdmission",
                "data": {
                    "controller_account_id": "controller",
                    "candidate_id": "candidate-v2",
                    "node_id": "candidate-node-v2",
                    "finality_signer_public_key": "11".repeat(32),
                    "operator_owner": "operator",
                    "public_manifest_hash": "22".repeat(32),
                    "requested_at_epoch": 2
                }
            }}
        }
    }))
    .expect("encode signed governance action")
}

#[test]
fn v2_governance_action_uses_parent_admission_and_survives_proposal_selection() {
    let (binding, mut signer) = binding_and_fixture();
    let legacy_preflight_calls = Arc::new(AtomicUsize::new(0));
    let validated_batches = Arc::new(Mutex::new(Vec::new()));
    let hook = AdmissionHook {
        legacy_preflight_calls: legacy_preflight_calls.clone(),
        validated_batches: validated_batches.clone(),
    };
    let config = crate::NodeConfig::new(
        "validator",
        binding.scope.world_id.clone(),
        NodeRole::Sequencer,
    )
    .expect("valid node config");
    let runtime = NodeRuntime::new(config)
        .with_execution_hook(hook)
        .with_tendermint_v2(binding.clone(), signer.provider.clone())
        .expect("select protected V2 mode");

    let payload = signed_governance_payload();
    runtime
        .submit_consensus_action_payload(7, payload.clone())
        .expect("V2 parent admission should authorize queueing");
    assert_eq!(legacy_preflight_calls.load(Ordering::SeqCst), 0);

    let mut hook = runtime
        .execution_hook
        .as_ref()
        .expect("configured hook")
        .lock()
        .expect("hook lock");
    let mut pending = runtime
        .pending_consensus_actions
        .lock()
        .expect("pending queue lock");
    let schedule = genesis_schedule(&binding).expect("genesis schedule");
    let (proposal_actions, rejected) = NodeRuntime::canonicalize_bft_v2_local_actions(
        &mut pending,
        &runtime.pending_consensus_action_queue_bytes,
        &mut **hook,
        &schedule,
    )
    .expect("canonicalize V2 proposal actions");

    assert!(rejected.is_empty());
    assert_eq!(proposal_actions.len(), 1);
    assert_eq!(proposal_actions[0].action_id, 7);
    assert_eq!(proposal_actions[0].payload_cbor, payload);
    let batches = validated_batches.lock().expect("batch log lock");
    assert_eq!(batches.last().expect("candidate batch"), &proposal_actions);
    assert_eq!(legacy_preflight_calls.load(Ordering::SeqCst), 0);
    drop(hook);
    signer.server.shutdown();
}
