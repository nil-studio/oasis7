//! The production formal wrapper verifies governance against its committed cut.
use super::*;
use ed25519_dalek::{Signer, SigningKey};
use oasis7::consensus_action_payload::{
    ConsensusActionAuthEnvelope, ConsensusActionPayloadBody, ConsensusActionPayloadEnvelope,
};
use oasis7_runtime::Action;

#[test]
fn governance_authorization_formal_node_hook_uses_qualified_committed_cut() {
    let (world, anchor) =
        super::super::super::governance_authorization_tests::qualified_world_and_anchor();
    let command = Action::SubmitGovernanceValidatorAdmission {
        controller_account_id: "controller".into(),
        candidate_id: "formal-ingress".into(),
        node_id: "node.formal-ingress".into(),
        finality_signer_public_key: hex::encode(
            SigningKey::from_bytes(&[72; 32]).verifying_key().as_bytes(),
        ),
        stake: 25,
        operator_owner: "ops.fixture".into(),
        public_manifest_hash: "a".repeat(64),
    };
    let key = SigningKey::from_bytes(&[71; 32]);
    let mut proof = world.governance_authorization_for_next_commit(1).unwrap();
    let signature = key.sign(&proof.signing_bytes(&command).unwrap());
    proof.signatures.insert(
        hex::encode(key.verifying_key().as_bytes()),
        hex::encode(signature.to_bytes()),
    );
    let world_id = proof.world_id.clone();
    // The mutable writer view deliberately differs. Only committed_read_cut
    // carries the actual externally signed anchor and its matching World.
    let mut mutable_world = World::new();
    mutable_world.submit_action(Action::RegisterAgent {
        agent_id: "staged".into(),
        pos: oasis7_agent_api::world_action::GeoPos::new(0, 0, 0),
    });
    let mut backing = writer(
        mutable_world,
        CurrentHeadEvidence::InitialActivation(NodeExecutionBootstrap {
            height: 0,
            consensus_block_hash: String::new(),
            execution_block_hash: String::new(),
            execution_state_root: String::new(),
        }),
        None,
    );
    backing.committed_cut = Some((world, anchor));
    let hook = FormalNodeHook {
        writer: backing,
        consumed_terminal: None,
        publication_sink: None,
        durable_set_arm: None,
        admission_gate: None,
    };
    let node = oasis7_node::NodeRuntime::new(
        oasis7_node::NodeConfig::new(
            "formal-governance-ingress",
            &world_id,
            oasis7_node::NodeRole::Observer,
        )
        .unwrap(),
    )
    .with_execution_hook(hook);
    let payload = |proof| {
        serde_cbor::to_vec(&ConsensusActionPayloadEnvelope {
            version: 1,
            gameplay_submission_origin: None,
            auth: Some(ConsensusActionAuthEnvelope::GovernanceAction(proof)),
            body: ConsensusActionPayloadBody::RuntimeAction {
                action: command.clone(),
            },
        })
        .unwrap()
    };
    let mut forged = proof.clone();
    forged
        .signatures
        .values_mut()
        .for_each(|signature| *signature = "0".repeat(128));
    assert!(
        node.submit_consensus_action_payload(1, payload(forged))
            .is_err()
    );
    let mut wrong_world = proof.clone();
    wrong_world.world_id = "other-world".into();
    let signature = key.sign(&wrong_world.signing_bytes(&command).unwrap());
    wrong_world.signatures.insert(
        hex::encode(key.verifying_key().as_bytes()),
        hex::encode(signature.to_bytes()),
    );
    assert!(
        node.submit_consensus_action_payload(1, payload(wrong_world))
            .is_err()
    );
    node.submit_consensus_action_payload(1, payload(proof))
        .unwrap();
}
