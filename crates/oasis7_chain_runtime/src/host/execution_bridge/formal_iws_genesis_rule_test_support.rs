//! One real governed Rule seed shared by GEN and both later Host transfers.
//! Initial fixture construction is a separate resource domain: it executes the
//! real Wasm and original World APIs, never a fixed sandbox or accepted token.
use ed25519_dalek::{Signer, SigningKey};
use oasis7_agent_api::world_action::{GeoPos, ResourceKind};
use oasis7_formal_protocol::incremental_history::GenesisConfigV1;
use oasis7_runtime::{
    Action, Manifest, ModuleAbiContract, ModuleActivation, ModuleArtifactIdentity, ModuleChangeSet,
    ModuleKind, ModuleLimits, ModuleManifest, ModuleRole, ModuleSubscription,
    ModuleSubscriptionStage, PolicySet, ProposalDecision, World,
};
use oasis7_wasm_executor::{WasmExecutor, WasmExecutorConfig};
use oasis7_world_state::{AgentCell, AgentState, DomainEvent, WorldEventBody, WorldState};
use sha2::{Digest, Sha256};

pub(crate) const MODULE_ID: &str = "m1.rule.transfer";
pub(crate) const MODULE_VERSION: &str = "0.1.0";
pub(crate) const SENDER: &str = "sender";
pub(crate) const RECEIVER: &str = "receiver";
pub(crate) const INITIAL_SENDER_ELECTRICITY: i64 = 8;
pub(crate) const INITIAL_RECEIVER_ELECTRICITY: i64 = 10_000;
pub(crate) const INITIAL_RECEIVER_DATA: i64 = 10_000;
const SIGNER: &str = "iws.genesis.candidate.fixture.signer";
const WASM: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/iws_rule_transfer_candidate/m1.rule.transfer.wasm"
));
const METADATA: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/iws_rule_transfer_candidate/m1.rule.transfer.metadata.json"
));
const SHA256: &str = "2d3a20151d9b8f7766ecc89a8353d0de10bb65c877e10bb898fbbec616cda316";

pub(crate) struct RuleGenesisSeedV1 {
    pub world: World,
    /// Actual initialization clock, original fee events and finite balances.
    /// This is a test receipt, not a formal commit or worker attestation.
    pub receipt: serde_json::Value,
}

fn manifest(key: &SigningKey) -> ModuleManifest {
    assert_eq!(WASM.len(), 349764);
    assert_eq!(hex::encode(Sha256::digest(WASM)), SHA256);
    let metadata: serde_json::Value = serde_json::from_str(METADATA).unwrap();
    assert_eq!(metadata["module_id"], MODULE_ID);
    assert_eq!(metadata["purpose"], "candidate-build");
    let source_hash = metadata["source_hash"].as_str().unwrap().to_owned();
    let build_manifest_hash = metadata["build_manifest_hash"].as_str().unwrap().to_owned();
    let signature = key.sign(&ModuleArtifactIdentity::signing_payload_v1(
        SHA256,
        &source_hash,
        &build_manifest_hash,
        SIGNER,
    ));
    ModuleManifest {
        module_id: MODULE_ID.into(),
        name: "IWS actual transfer Rule".into(),
        version: MODULE_VERSION.into(),
        kind: ModuleKind::Reducer,
        role: ModuleRole::Rule,
        wasm_hash: SHA256.into(),
        interface_version: "wasm-1".into(),
        abi_contract: ModuleAbiContract::default(),
        exports: vec!["reduce".into(), "call".into()],
        subscriptions: vec![
            ModuleSubscription {
                event_kinds: vec![
                    "domain.agent_registered".into(),
                    "domain.agent_moved".into(),
                ],
                action_kinds: vec![],
                stage: Some(ModuleSubscriptionStage::PostEvent),
                filters: None,
            },
            ModuleSubscription {
                event_kinds: vec![],
                action_kinds: vec!["action.transfer_resource".into()],
                stage: Some(ModuleSubscriptionStage::PreAction),
                filters: None,
            },
        ],
        required_caps: vec![],
        // This dedicated test World authorizes the actual candidate's original
        // source/build hashes. It does not change its purpose to builtin release.
        artifact_identity: Some(ModuleArtifactIdentity {
            source_hash,
            build_manifest_hash,
            signer_node_id: SIGNER.into(),
            signature_scheme: ModuleArtifactIdentity::SIGNATURE_SCHEME_ED25519.into(),
            artifact_signature: format!(
                "{}{}",
                ModuleArtifactIdentity::SIGNATURE_PREFIX_ED25519_V1,
                hex::encode(signature.to_bytes())
            ),
        }),
        limits: ModuleLimits {
            max_mem_bytes: 64 << 20,
            max_gas: 10_000_000,
            max_call_rate: 64,
            max_output_bytes: 4096,
            max_effects: 0,
            max_emits: 1,
        },
    }
}

pub(crate) fn governed_transfer_seed(cfg: &GenesisConfigV1) -> RuleGenesisSeedV1 {
    let key = SigningKey::from_bytes(&[87; 32]);
    let module = manifest(&key);
    let mut state = WorldState::default();
    state
        .node_identity_bindings
        .insert(SIGNER.into(), hex::encode(key.verifying_key().as_bytes()));
    state.resources.insert(ResourceKind::Electricity, 100_000);
    state.resources.insert(ResourceKind::Data, 100_000);
    for id in [SENDER, RECEIVER] {
        let mut agent = AgentState::new(id, GeoPos::new(0, 0, 0));
        let electricity = if id == SENDER {
            INITIAL_SENDER_ELECTRICITY
        } else {
            INITIAL_RECEIVER_ELECTRICITY
        };
        agent
            .resources
            .set(ResourceKind::Electricity, electricity)
            .unwrap();
        agent
            .resources
            .set(ResourceKind::Data, INITIAL_RECEIVER_DATA)
            .unwrap();
        state.agents.insert(id.into(), AgentCell::new(agent, 0));
    }
    let mut initial = World::new_with_state(state);
    initial.set_policy(PolicySet::allow_all());
    initial
        .bind_cognition_runtime(&cfg.world_id, "main", 0, None, "pending", 0)
        .unwrap();
    initial.initialize_empty_formal_history().unwrap();
    let genesis = hex::encode(cfg.digest().unwrap());
    // Initial input construction, before any tick, governance or Wasm call.
    // This snapshot is never formal material or an accepted-parent substitute.
    let snapshot = initial.snapshot_with_chain_resource_context(
        oasis7_runtime::ChainResourceDerivationContext {
            world_id: &cfg.world_id,
            chain_id: "projected-genesis-chain",
            genesis_ref: Some(&genesis),
            created_at_height: 0,
            manifest_height: 0,
            commit_block_hash: None,
            tick: 0,
        },
        hex::encode(cfg.initial_content),
        hex::encode(cfg.format_profile),
    );
    let mut world = World::from_snapshot(snapshot, initial.journal().clone()).unwrap();
    world.initialize_empty_formal_history().unwrap();

    // Original deployment charges receiver and records actual artifact owner.
    world.submit_action(Action::DeployModuleArtifact {
        publisher_agent_id: RECEIVER.into(),
        wasm_hash: SHA256.into(),
        wasm_bytes: WASM.to_vec(),
    });
    world.step().unwrap();
    assert_eq!(
        world
            .state()
            .module_artifact_owners
            .get(SHA256)
            .map(String::as_str),
        Some(RECEIVER)
    );

    // Governance admission is the original proposal -> shadow -> approve ->
    // apply chain, with the unchanged real candidate bytes and signed identity.
    let changes = ModuleChangeSet {
        register: vec![module.clone()],
        activate: vec![ModuleActivation {
            module_id: MODULE_ID.into(),
            version: MODULE_VERSION.into(),
        }],
        ..ModuleChangeSet::default()
    };
    let proposal = world
        .propose_manifest_update(
            Manifest {
                version: world.manifest().version + 1,
                content: serde_json::json!({"module_changes":changes}),
            },
            RECEIVER,
        )
        .unwrap();
    world.shadow_proposal(proposal).unwrap();
    world
        .approve_proposal(proposal, SENDER, ProposalDecision::Approve)
        .unwrap();
    world.apply_proposal(proposal).unwrap();
    world.submit_action(Action::InstallModuleFromArtifact {
        installer_agent_id: RECEIVER.into(),
        manifest: module,
        activate: true,
    });
    world.step().unwrap();
    assert_eq!(world.state().module_instances.len(), 1);
    let instance = world.state().module_instances.values().next().unwrap();
    assert_eq!(instance.owner_agent_id, RECEIVER);
    assert_eq!(instance.module_id, MODULE_ID);
    assert_eq!(instance.wasm_hash, SHA256);
    assert!(instance.active);
    let instance_id = instance.instance_id.clone();

    // Actual zero-distance moves emit original AgentMoved events. The original
    // Wasmtime sandbox executes the Rule's reducer and persists its position
    // state. No transfer executes during warmup; World clock advances to 3,
    // independently of the later GEN CommitBody height zero.
    let start = world.journal().events.len();
    let action_ids = [SENDER, RECEIVER].map(|id| {
        world.submit_action(Action::MoveAgent {
            agent_id: id.into(),
            to: GeoPos::new(0, 0, 0),
        })
    });
    let mut sandbox = WasmExecutor::new(WasmExecutorConfig::default()).unwrap();
    world.step_with_modules(&mut sandbox).unwrap();
    let warm = &world.journal().events[start..];
    let moves = warm.iter().filter(|event|matches!(&event.body,WorldEventBody::Domain(DomainEvent::AgentMoved{agent_id,..}) if agent_id==SENDER || agent_id==RECEIVER)).count();
    assert_eq!(moves, 2);
    let charges: Vec<_> = warm
        .iter()
        .filter_map(|event| match &event.body {
            WorldEventBody::ModuleRuntimeCharged(charge) => Some(charge),
            _ => None,
        })
        .collect();
    assert_eq!(charges.len(), 2);
    assert!(
        charges
            .iter()
            .all(|charge| charge.payer_agent_id == RECEIVER
                && charge.electricity_fee_amount > 0
                && charge.compute_fee_amount > 0)
    );
    let actual_state: &[u8] = world
        .state()
        .module_states
        .get(&instance_id)
        .expect("real instance position state");
    let decoded: serde_cbor::Value = serde_cbor::from_slice(actual_state).unwrap();
    let serde_cbor::Value::Map(fields) = decoded else {
        panic!("real Rule state map")
    };
    let Some(serde_cbor::Value::Map(agents)) =
        fields.get(&serde_cbor::Value::Text("agents".into()))
    else {
        panic!("real Rule position map")
    };
    assert_eq!(agents.len(), 2);
    for id in [SENDER, RECEIVER] {
        assert!(agents.contains_key(&serde_cbor::Value::Text(id.into())));
    }
    assert_eq!(world.pending_actions_len(), 0);
    assert_eq!(world.state().agents.len(), 2);
    assert_eq!(
        world
            .agent_resource_balance(SENDER, ResourceKind::Electricity)
            .unwrap(),
        8
    );
    assert_eq!(world.state().time, 3);
    let receipt = serde_json::json!({
        "fixture_wasm_sha256":SHA256,"governance_proposal":proposal,"instance_id":instance_id,
        "initial_sender_electricity":INITIAL_SENDER_ELECTRICITY,
        "initial_receiver_electricity":INITIAL_RECEIVER_ELECTRICITY,"initial_receiver_data":INITIAL_RECEIVER_DATA,
        "warmup_action_ids":action_ids,"warmup_moves":moves,"warmup_runtime_charges":charges,
        "pre_gen_world_clock":world.state().time,
        "pre_gen_sender_electricity":world.agent_resource_balance(SENDER,ResourceKind::Electricity).unwrap(),
        "pre_gen_receiver_electricity":world.agent_resource_balance(RECEIVER,ResourceKind::Electricity).unwrap(),
        "pre_gen_receiver_data":world.agent_resource_balance(RECEIVER,ResourceKind::Data).unwrap(),
        "genesis_commit_body_height":0,
    });
    RuleGenesisSeedV1 { world, receipt }
}
