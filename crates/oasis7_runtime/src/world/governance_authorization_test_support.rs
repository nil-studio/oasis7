//! Synthetic parent tokens exercise execution and state transitions only.
//! Host tests separately verify admission through cryptographically qualified parents.
use super::*;
use crate::{Action, RuntimeCommittedTickContext};
use ed25519_dalek::{Signer, SigningKey};
use oasis7_formal_protocol::controlled_authority::formal::{FormalHeadRefV1, FormalWorldIdentityV1};
use oasis7_formal_protocol::controlled_authority::formal::business_index::BusinessIndexNodeReader;
use oasis7_wasm_abi::{ModuleCallFailure, ModuleCallRequest, ModuleOutput, ModuleSandbox};
use std::sync::Arc;

struct EmptyNodes;
impl BusinessIndexNodeReader for EmptyNodes {
    fn read_node(&self, _: &str) -> Result<Option<Vec<u8>>, String> { Ok(None) }
}
struct Sandbox;
impl ModuleSandbox for Sandbox {
    fn call(&mut self, _: &ModuleCallRequest) -> Result<ModuleOutput, ModuleCallFailure> {
        Ok(ModuleOutput { new_state: None, effects: Vec::new(), emits: Vec::new(), tick_lifecycle: None, output_bytes: 0 })
    }
}

pub(crate) fn fixture_key() -> SigningKey { SigningKey::from_bytes(&[7; 32]) }

impl World {
    pub(crate) fn submit_governance_fixture_action(&mut self, action: Action) -> ActionId {
        if self.state.retained_history_root.is_none() { self.initialize_empty_formal_history().unwrap(); }
        let bytes = serde_cbor::to_vec(&self.snapshot()).unwrap();
        let parent = crate::formal_retained_history::VerifiedFormalRetainedHistoryParentV1::for_test_governance_snapshot(
            self.state.retained_history_root.clone().unwrap(), &bytes,
            FormalWorldIdentityV1 { world_id: "governance.fixture".into(), chain_id: "fixture.chain".into(), genesis_digest: "a".repeat(64) },
            FormalHeadRefV1 { position: self.state.time + 1000, record_id: "b".repeat(64) },
        );
        self.attach_verified_formal_history(parent, Arc::new(EmptyNodes)).unwrap();
        let context = RuntimeCommittedTickContext {
            height: self.state.time + 1, slot: self.state.time, epoch: 0,
            node_block_hash: "c".repeat(64), action_root: "d".repeat(64),
            authority_node_id: self.tick_consensus_authority_source().into(), committed_at_unix_ms: 0,
        };
        self.begin_formal_execution_tick(&context).unwrap();
        let controller = self.state.governance_main_token_controller_registry.as_ref().unwrap().genesis_controller_account_id.clone();
        let nonce = self.state.governance_authorization_nonces.get(&controller).copied().unwrap_or(0) + 1;
        let mut proof = self.governance_authorization_for_next_commit(nonce).unwrap();
        let key = fixture_key();
        proof.signatures.insert(hex::encode(key.verifying_key().to_bytes()), hex::encode(key.sign(&proof.signing_bytes(&action).unwrap()).to_bytes()));
        self.submit_governance_action(action, proof)
    }

    pub(crate) fn step_governance_fixture(&mut self) -> Result<(), WorldError> {
        let Some((context, _)) = self.formal_execution_tick.clone() else { return self.step(); };
        self.step_with_modules_for_committed_context(&mut Sandbox, &context)?;
        self.finish_formal_execution_tick(&context)
    }
}
