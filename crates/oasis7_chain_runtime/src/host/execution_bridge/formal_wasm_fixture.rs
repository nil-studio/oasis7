//! Actual module fixture for formal runtime replay; test authorities only.
use super::*;
use crate::formal_live_runtime::tests::protected_process_tests::common;
use oasis7_runtime::{
    Manifest, ModuleAbiContract, ModuleActivation, ModuleChangeSet, ModuleKind, ModuleLimits,
    ModuleManifest, ModuleRole, ModuleSubscription, ModuleSubscriptionStage, PolicySet,
    ProposalDecision,
};
use sha2::{Digest, Sha256};
const WASM_EXECUTOR_TEST_MODULE: &[u8] = &[
    0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00, 0x01, 0x0d, 0x02, 0x60, 0x01, 0x7f, 0x01, 0x7f,
    0x60, 0x02, 0x7f, 0x7f, 0x02, 0x7f, 0x7f, 0x03, 0x03, 0x02, 0x00, 0x01, 0x05, 0x03, 0x01, 0x00,
    0x01, 0x07, 0x19, 0x03, 0x06, 0x6d, 0x65, 0x6d, 0x6f, 0x72, 0x79, 0x02, 0x00, 0x05, 0x61, 0x6c,
    0x6c, 0x6f, 0x63, 0x00, 0x00, 0x04, 0x63, 0x61, 0x6c, 0x6c, 0x00, 0x01, 0x0a, 0x0f, 0x02, 0x05,
    0x00, 0x41, 0x80, 0x01, 0x0b, 0x07, 0x00, 0x41, 0x00, 0x41, 0xc7, 0x00, 0x0b, 0x0b, 0x4d, 0x01,
    0x00, 0x41, 0x00, 0x0b, 0x47, 0xa4, 0x65, 0x65, 0x6d, 0x69, 0x74, 0x73, 0x81, 0xa2, 0x64, 0x6b,
    0x69, 0x6e, 0x64, 0x69, 0x77, 0x61, 0x73, 0x6d, 0x2e, 0x74, 0x65, 0x73, 0x74, 0x67, 0x70, 0x61,
    0x79, 0x6c, 0x6f, 0x61, 0x64, 0xa1, 0x62, 0x6f, 0x6b, 0xf5, 0x67, 0x65, 0x66, 0x66, 0x65, 0x63,
    0x74, 0x73, 0x80, 0x69, 0x6e, 0x65, 0x77, 0x5f, 0x73, 0x74, 0x61, 0x74, 0x65, 0xf6, 0x6c, 0x6f,
    0x75, 0x74, 0x70, 0x75, 0x74, 0x5f, 0x62, 0x79, 0x74, 0x65, 0x73, 0x00,
];

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

fn wasm_executor_module_manifest(wasm_hash: String) -> ModuleManifest {
    ModuleManifest {
        module_id: "m.wasm".to_string(),
        name: "Wasm".to_string(),
        version: "0.1.0".to_string(),
        kind: ModuleKind::Pure,
        role: ModuleRole::Domain,
        wasm_hash: wasm_hash.clone(),
        interface_version: "wasm-1".to_string(),
        abi_contract: ModuleAbiContract::default(),
        exports: vec!["call".to_string()],
        subscriptions: vec![ModuleSubscription {
            event_kinds: vec!["domain.agent_registered".to_string()],
            action_kinds: Vec::new(),
            stage: Some(ModuleSubscriptionStage::PostEvent),
            filters: None,
        }],
        required_caps: Vec::new(),
        artifact_identity: Some(common::signed_test_artifact_identity(wasm_hash.as_str())),
        limits: ModuleLimits {
            max_mem_bytes: 64 * 1024,
            max_gas: 10_000_000,
            max_call_rate: 1,
            max_output_bytes: 1024,
            max_effects: 0,
            max_emits: 2,
        },
    }
}

fn apply_module_manifest(world: &mut World, module_manifest: ModuleManifest) {
    let changes = ModuleChangeSet {
        register: vec![module_manifest.clone()],
        activate: vec![ModuleActivation {
            module_id: module_manifest.module_id.clone(),
            version: module_manifest.version.clone(),
        }],
        ..ModuleChangeSet::default()
    };

    let mut content = serde_json::Map::new();
    content.insert(
        "module_changes".to_string(),
        serde_json::to_value(&changes).unwrap(),
    );
    let manifest = Manifest {
        version: 2,
        content: serde_json::Value::Object(content),
    };
    let proposal_id = world.propose_manifest_update(manifest, "alice").unwrap();
    world.shadow_proposal(proposal_id).unwrap();
    world
        .approve_proposal(proposal_id, "bob", ProposalDecision::Approve)
        .unwrap();
    world.apply_proposal(proposal_id).unwrap();
}

pub(super) fn world_and_anchor() -> (World, VerifiedFormalReadAnchorV1) {
    let (_, mut trust, old, _) = fixtures::actual_world_initial_fixture();
    let identity = &old.authority().definition().world;
    let world = prepared_wasm_world(identity, false);
    let draft = super::super::super::formal_initial::InitialWorldClosure::from_existing_world(
        &world,
        identity,
        &ReleaseSecurityPolicy::default(),
    )
    .unwrap();
    let body = draft
        .activation_body(
            old.authority().definition().clone(),
            old.authority().policy().clone(),
        )
        .unwrap();
    trust.initial_state_root = body.initial_state_root.clone();
    trust.governing_manifest_digest = body.governing_manifest_digest.clone();
    let certificate = SignedInitialActivationV1 {
        signature_hex: fixtures::sign(initial_activation_signing_bytes(&body).unwrap(), 9),
        body,
    };
    let (authority, candidate) = draft
        .authorize(&certificate, &trust, &fixtures::key(4))
        .unwrap();
    let proof = fixtures::qualified_proof(
        candidate.candidate.record,
        None,
        &authority,
        None,
        None,
        4,
        5,
        6,
    );
    let anchor = VerifiedFormalReadAnchorV1::from_initial(&certificate, &trust, &proof).unwrap();
    (world, anchor)
}

/// A profile-2 activation fixture stages the empty retained-history index
/// before publishing any World ticks, as required for a fresh approved World.
pub(super) fn world_with_formal_history() -> World {
    let (_, _, old, _) = fixtures::actual_world_initial_fixture();
    prepared_wasm_world(&old.authority().definition().world, true)
}

fn prepared_wasm_world(identity: &FormalWorldIdentityV1, initialize_formal_history: bool) -> World {
    // Module admission precedes external initial activation; this is a test setup.
    let mut world = oasis7::runtime_host::world::new();
    world.set_policy(PolicySet::allow_all());
    if initialize_formal_history {
        world.initialize_empty_formal_history().unwrap();
    }
    let hash = sha256_hex(WASM_EXECUTOR_TEST_MODULE);
    world
        .register_module_artifact(hash.clone(), WASM_EXECUTOR_TEST_MODULE)
        .unwrap();
    apply_module_manifest(&mut world, wasm_executor_module_manifest(hash));
    world
        .bind_cognition_runtime(&identity.world_id, "main", 0, None, "pending", 0)
        .unwrap();
    let snapshot = world.snapshot_with_chain_resource_context(
        oasis7_runtime::ChainResourceDerivationContext {
            world_id: &identity.world_id,
            chain_id: &identity.chain_id,
            genesis_ref: Some(&identity.genesis_digest),
            created_at_height: 0,
            manifest_height: 0,
            commit_block_hash: None,
            tick: world.state().time,
        },
        fixtures::h(10),
        fixtures::h(11),
    );
    oasis7::runtime_host::world::from_snapshot(snapshot, world.journal().clone()).unwrap()
}
