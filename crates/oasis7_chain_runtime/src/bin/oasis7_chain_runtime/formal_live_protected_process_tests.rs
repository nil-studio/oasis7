//! Unix composition test for the formal Node, protected endpoint processes,
//! Wasmtime execution, and the ordinary externally pinned read client.
//!
//! This is intentionally ignored by the normal bin suite because it needs the
//! separately built `oasis7_durability_endpoint` binary. Its loopback monotonic
//! oracle is a test simulation and does not prove HSM custody or fault domains.
use super::*;
use ed25519_dalek::{Signature, SigningKey, Verifier, VerifyingKey};
use oasis7::{
    runtime::{
        Action, Manifest, ModuleAbiContract, ModuleActivation, ModuleChangeSet, ModuleKind,
        ModuleLimits, ModuleManifest, ModuleRole, ModuleSubscription, ModuleSubscriptionStage,
        PolicySet, ProposalDecision, SchedulerPolicyV1, WorldState, blake3_hex,
    },
    world_service::{
        authority::sign_read_request,
        client::{
            FormalReadTrustConfigV1, RemoteWorldServiceClient, WorldServiceClientConfig,
            WorldServicePort,
        },
        correlation, game_task,
        wire::WorldServicePayloadV1,
    },
};
use oasis7_agent_api::game::{DecimalU64, GAME_TASK_CREATE_DOMAIN, GameTaskCreateV1};
use oasis7_client_api::world_service::{
    ReadWorldViewRequest, SubmitIntentRequest, SubmitObservation, WORLD_SERVICE_CONTRACT_VERSION,
    WorldIdentity,
};
use oasis7_distfs::controlled_authority::{
    formal::{
        coordinator::FormalCoordinator, fencing::directory_fingerprint,
        transport::RemoteFormalEndpoint, *,
    },
    replicated_protocol::EndpointRole,
};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    net::TcpListener,
    path::Path,
    thread,
    time::{Duration, Instant},
};

#[path = "formal_live_protected_availability_tests.rs"]
mod availability_tests;
#[path = "../../../../oasis7/tests/common/mod.rs"]
pub(crate) mod common;
#[path = "formal_live_protected_crash_tests.rs"]
mod crash_tests;
#[path = "formal_live_protected_durable_set_tests.rs"]
mod durable_set_tests;
#[path = "formal_live_protected_effect_tests.rs"]
mod effect_tests;
#[path = "formal_live_first_agent_test_effect.rs"]
mod first_agent_test_effect;
use crate::formal_read_fixtures as fixtures;
#[path = "formal_live_protected_healthy_fault_process_tests.rs"]
mod healthy_fault_tests;
#[path = "formal_live_healthy_operation_process_tests.rs"]
mod healthy_operation_tests;
#[path = "formal_live_protected_long_history_tests.rs"]
mod long_history_tests;
#[path = "formal_live_protected_process_oracle.rs"]
mod oracle_support;
#[path = "formal_live_protected_proc_sampler.rs"]
mod proc_sampler;
#[path = "world_initialization_process_tests.rs"]
mod world_initialization_tests;

mod crypto {
    use super::*;

    pub(super) fn bytes<T: serde::Serialize>(domain: &str, value: &T) -> Result<Vec<u8>, ()> {
        let mut bytes = (domain.len() as u64).to_be_bytes().to_vec();
        bytes.extend(domain.as_bytes());
        bytes.extend(serde_cbor::to_vec(value).map_err(|_| ())?);
        Ok(bytes)
    }

    pub(super) fn verify<T: serde::Serialize>(
        domain: &str,
        value: &T,
        signature: &str,
        key: &str,
    ) -> Result<(), ()> {
        let key: [u8; 32] = hex::decode(key)
            .map_err(|_| ())?
            .try_into()
            .map_err(|_| ())?;
        let signature: [u8; 64] = hex::decode(signature)
            .map_err(|_| ())?
            .try_into()
            .map_err(|_| ())?;
        VerifyingKey::from_bytes(&key)
            .map_err(|_| ())?
            .verify(&bytes(domain, value)?, &Signature::from_bytes(&signature))
            .map_err(|_| ())
    }
}

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

#[test]
#[ignore = "requires OASIS7_FORMAL_ENDPOINT_TEST_BIN; oracle is a test simulation"]
fn protected_endpoint_children_submit_qualify_restart_and_read_from_external_pin() {
    run_on_fixture_stack(false).unwrap();
}

#[test]
#[ignore = "requires OASIS7_FORMAL_ENDPOINT_TEST_BIN; oracle is a test simulation"]
fn protected_endpoint_children_start_from_qualified_zero_height_genesis() {
    run_on_fixture_stack(true).unwrap();
}

fn run_on_fixture_stack(genesis_mode: bool) -> Result<(), String> {
    std::thread::Builder::new()
        .name("formal-protected-fixture".into())
        .stack_size(32 * 1024 * 1024)
        .spawn(move || run_protected_runtime_round_trip(genesis_mode, None))
        .map_err(|error| error.to_string())?
        .join()
        .map_err(|_| "formal protected fixture thread panicked".to_string())?
}

fn run_protected_runtime_round_trip(
    genesis_mode: bool,
    crash_phase: Option<&str>,
) -> Result<(), String> {
    run_protected_runtime_case(genesis_mode, crash_phase, false)
}

#[derive(Clone, Copy)]
struct ScenarioDeadline {
    started_at: Instant,
    expires_at: Instant,
}

impl ScenarioDeadline {
    fn report_stage(self, stage: &str) -> Result<(), String> {
        let elapsed = self.started_at.elapsed();
        eprintln!(
            "C03 healthy_set stage={stage} elapsed_ms={}",
            elapsed.as_millis()
        );
        if Instant::now() >= self.expires_at {
            return Err(format!(
                "C03 healthy-set total scenario deadline exceeded at {stage} after {}ms",
                elapsed.as_millis()
            ));
        }
        Ok(())
    }
}

fn run_protected_runtime_case(
    genesis_mode: bool,
    crash_phase: Option<&str>,
    long_history: bool,
) -> Result<(), String> {
    run_protected_runtime_case_with_deadline(genesis_mode, crash_phase, long_history, None)
}

fn run_protected_runtime_case_with_deadline(
    genesis_mode: bool,
    crash_phase: Option<&str>,
    long_history: bool,
    scenario_deadline: Option<ScenarioDeadline>,
) -> Result<(), String> {
    run_protected_runtime_fixture(
        genesis_mode,
        crash_phase,
        long_history,
        None,
        false,
        scenario_deadline,
    )
}

fn run_protected_runtime_effect_case(
    scenario: &effect_tests::EffectScenario,
) -> Result<(), String> {
    run_protected_runtime_fixture(true, None, false, Some(scenario), false, None)
}

fn run_protected_runtime_availability_case() -> Result<(), String> {
    run_protected_runtime_fixture(false, None, false, None, true, None)
}

fn run_protected_runtime_fixture(
    genesis_mode: bool,
    crash_phase: Option<&str>,
    long_history: bool,
    effect_scenario: Option<&effect_tests::EffectScenario>,
    unavailable_replica: bool,
    scenario_deadline: Option<ScenarioDeadline>,
) -> Result<(), String> {
    let endpoint_binary = std::env::var_os("OASIS7_FORMAL_ENDPOINT_TEST_BIN")
        .ok_or("set OASIS7_FORMAL_ENDPOINT_TEST_BIN to oasis7_durability_endpoint")?;
    let root_guard = oracle_support::PrivateRoot::new()?;
    let root = root_guard.path().to_path_buf();
    let pdir = root.join("primary-data");
    let rdir = root.join("replica-data");
    oracle_support::private_directory(&pdir)?;
    oracle_support::private_directory(&rdir)?;
    if let Some(deadline) = scenario_deadline {
        deadline.report_stage("private P/R fixture directories ready")?;
    }

    let shipping_mode = matches!(
        crash_phase,
        Some("healthy_operation" | "healthy_fault_writer_lock" | "healthy_fault_old_owner_sigkill")
    );
    let shipping_executable = if shipping_mode {
        Some(std::path::PathBuf::from(
            std::env::var_os("OASIS7_CHAIN_RUNTIME_BIN")
                .ok_or("shipping fixture requires OASIS7_CHAIN_RUNTIME_BIN")?,
        ))
    } else {
        None
    };
    let release = oasis7_runtime::ReleaseSecurityPolicy::production_hardened();
    let (_policy_material, executable_digest, execution_policy_digest) = match &shipping_executable {
        Some(path) => crate::execution_bridge_real_tests::real_execution_bridge::formal_initial::test_execution_policy_material(path, &release)?,
        None => formal_package::execution_policy_material(&release)?,
    };
    if executable_digest.trim().is_empty() {
        return Err("loaded executable image digest must be nonempty".into());
    }

    let (mut world, owner_key) = world_with_owner_seed();
    world.set_policy(PolicySet::allow_all());
    world.initialize_empty_formal_history()?;
    if let Some(scenario) = effect_scenario {
        scenario.prepare_world(&mut world)?;
    } else {
        install_wasmtime_action_observer(&mut world)?;
    }
    let (_seed_world, mut initial_trust, seed_anchor, _) =
        fixtures::actual_world_initial_fixture_with_execution_policy(execution_policy_digest);
    let identity = seed_anchor.authority().definition().world.clone();
    if !genesis_mode {
        world.submit_action(Action::RegisterAgent {
            agent_id: "agent".into(),
            pos: oasis7_agent_api::world_action::GeoPos::new(0, 0, 0),
        });
        world
            .step()
            .map_err(|error| format!("execute Wasmtime observer on actual action: {error:?}"))?;
        world.submit_action(Action::ClaimStarterOc {
            agent_id: "agent".into(),
            player_id: "owner".into(),
            public_key: Some(hex::encode(owner_key.verifying_key().to_bytes())),
        });
        world
            .step()
            .map_err(|error| format!("claim canonical fixture owner: {error:?}"))?;
        world
            .install_capability_agent_identity("agent", "owner", 1)
            .map_err(|error| format!("install canonical fixture owner: {error:?}"))?;
    }
    world
        .bind_cognition_runtime(&identity.world_id, "main", 0, None, "pending", 0)
        .map_err(|error| format!("bind existing formal World: {error:?}"))?;
    if !genesis_mode {
        world
            .install_test_provider_capability_fixture("agent")
            .map_err(|error| {
                format!("install Runtime-owned provider authority fixture: {error:?}")
            })?;
    }
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
    let world = oasis7_runtime::World::from_snapshot(snapshot, world.journal().clone())
        .map_err(|error| format!("restore fixture World identity: {error:?}"))?;
    if (genesis_mode && world.state().time != 0) || (!genesis_mode && world.state().time == 0) {
        return Err(format!(
            "fixture World time {} does not match genesis mode {genesis_mode}",
            world.state().time
        ));
    }
    let module_emits_initial = wasm_emit_count(&world);

    let mut definition = seed_anchor.authority().definition().clone();
    let recovery = seed_anchor.authority().policy().clone();
    definition.profile_version = 2;
    definition.primary.instance.storage_fingerprint =
        directory_fingerprint(&pdir).map_err(|error| error.to_string())?;
    definition.replica.instance.storage_fingerprint =
        directory_fingerprint(&rdir).map_err(|error| error.to_string())?;
    let state_root = blake3_hex(&serde_cbor::to_vec(&world.snapshot()).map_err(|e| e.to_string())?);
    let bootstrap = crate::execution_bridge_real_tests::real_execution_bridge::formal_initial::InitialNodeBootstrapV1 {
        height: world.state().time,
        consensus_block_hash: fixtures::h(81),
        execution_block_hash: fixtures::h(82),
        execution_state_root: state_root,
    };
    let closure = if let Some(executable) = &shipping_executable {
        crate::execution_bridge_real_tests::real_execution_bridge::formal_initial::InitialWorldClosure::from_existing_world_for_test_executable(&world, &identity, &release, bootstrap, executable)?
    } else if genesis_mode {
        crate::execution_bridge_real_tests::real_execution_bridge::formal_initial::InitialWorldClosure::
            from_genesis_world_with_node_bootstrap(&world, &identity, &release, bootstrap)?
    } else {
        crate::execution_bridge_real_tests::real_execution_bridge::formal_initial::InitialWorldClosure::
            from_existing_world_with_node_bootstrap(&world, &identity, &release, bootstrap)?
    };
    let body = closure.activation_body(definition, recovery.clone())?;
    initial_trust.world = identity.clone();
    initial_trust.initial_state_root = body.initial_state_root.clone();
    initial_trust.governing_manifest_digest = body.governing_manifest_digest.clone();
    initial_trust.recovery_policy_digest = recovery.digest().map_err(|e| e.to_string())?;
    let certificate = SignedInitialActivationV1 {
        signature_hex: fixtures::sign(
            initial_activation_signing_bytes(&body).map_err(|e| e.to_string())?,
            9,
        ),
        body,
    };
    let writer_key = fixtures::key(4);
    let (authority, initial_candidate) =
        closure.authorize(&certificate, &initial_trust, &writer_key)?;

    let oracle_fixture =
        oracle_support::Fixture::new(root.clone(), pdir.clone(), rdir.clone(), authority.clone());
    let healthy_set_mode = matches!(
        crash_phase,
        Some(
            "healthy_set"
                | "healthy_set_long"
                | "healthy_operation"
                | "healthy_fault_writer_lock"
                | "healthy_fault_old_owner_sigkill"
        )
    );
    let (primary_oracle, primary_provision) = if healthy_set_mode {
        let (oracle, provision) =
            oracle_support::durable_set_oracle::start_healthy_primary_oracle(&oracle_fixture);
        (oracle, Some(provision))
    } else {
        (
            oracle_support::start_oracle(&oracle_fixture, EndpointRole::Primary),
            None,
        )
    };
    let replica_oracle = oracle_support::start_oracle(&oracle_fixture, EndpointRole::Replica);
    let oracle_auth = root.join("oracle-authentication-key");
    oracle_support::write_private(&oracle_auth, hex::encode([17u8; 32]).as_bytes())?;

    let primary_config = root.join("primary-endpoint.json");
    let replica_config = root.join("replica-endpoint.json");
    write_endpoint_config(
        &primary_config,
        &pdir,
        &oracle_auth,
        primary_oracle.address(),
        EndpointRole::Primary,
        &initial_trust,
        &certificate,
    )?;
    write_endpoint_config(
        &replica_config,
        &rdir,
        &oracle_auth,
        replica_oracle.address(),
        EndpointRole::Replica,
        &initial_trust,
        &certificate,
    )?;
    if unavailable_replica {
        let listener =
            std::net::TcpListener::bind("127.0.0.1:0").map_err(|error| error.to_string())?;
        let address = listener.local_addr().map_err(|error| error.to_string())?;
        let mut config: Value = serde_json::from_slice(
            &std::fs::read(&replica_config).map_err(|error| error.to_string())?,
        )
        .map_err(|error| error.to_string())?;
        config["listen"] = Value::String(address.to_string());
        // This fixture-owned private config is not yet consumed by a child.
        // Keep its existing permissions while setting the fixed restart port.
        std::fs::write(
            &replica_config,
            &serde_json::to_vec(&config).map_err(|error| error.to_string())?,
        )
        .map_err(|error| error.to_string())?;
        drop(listener);
    }
    let (primary_child, primary_address) =
        oracle_support::start_endpoint(&endpoint_binary, &primary_config)?;
    let (replica_child, replica_address) =
        oracle_support::start_endpoint(&endpoint_binary, &replica_config)?;

    let genesis = authority
        .definition()
        .world
        .genesis_head()
        .map_err(|error| error.to_string())?;
    let endpoint = |address, role| {
        RemoteFormalEndpoint::new(
            address,
            Duration::from_secs(5),
            authority.clone(),
            role,
            writer_key.clone(),
            genesis.clone(),
        )
        .map_err(|error| error.to_string())
    };
    let mut coordinator = FormalCoordinator::new(
        endpoint(primary_address, EndpointRole::Primary)?,
        endpoint(replica_address, EndpointRole::Replica)?,
        authority.clone(),
        authority.clone(),
        None,
        None,
    )
    .map_err(|error| error.to_string())?;
    let initial_proof = coordinator
        .submit(initial_candidate.clone())
        .map_err(|error| format!("qualify signed profile-2 initial closure: {error}"))?;
    verify_formal_qualified(&initial_proof, &authority, None, None)
        .map_err(|error| error.to_string())?;
    let anchor =
        VerifiedFormalReadAnchorV1::from_initial(&certificate, &initial_trust, &initial_proof)
            .map_err(|error| error.to_string())?;
    if let Some(deadline) = scenario_deadline {
        deadline.report_stage("initial profile-2 closure qualified on protected P/R")?;
    }
    let checkpoint = anchor.checkpoint();
    let trust = FormalReadTrustConfigV1 {
        initial_trust: initial_trust.clone(),
        checkpoint_digest: checkpoint.digest().map_err(|error| error.to_string())?,
        checkpoint,
    };
    let approved_initial = super::super::super::formal_live_config::ApprovedInitialPublicationV1 {
        schema_version: 1,
        candidate: initial_candidate.clone(),
        proof: initial_proof.clone(),
    };
    let writer_dir = root.join("formal-writer");
    let node_key_dir = root.join("node-keypair");
    oracle_support::private_directory(&writer_dir)?;
    oracle_support::private_directory(&node_key_dir)?;
    let writer_key_file = root.join("writer-key");
    oracle_support::write_private(
        &writer_key_file,
        hex::encode(writer_key.to_bytes()).as_bytes(),
    )?;
    let node_key = fixtures::key(90);
    let node_key_file = node_key_dir.join("node-keypair.toml");
    oracle_support::write_private(
        &node_key_file,
        format!(
            "[node]\nprivate_key = \"{}\"\npublic_key = \"{}\"\n",
            hex::encode(node_key.to_bytes()),
            hex::encode(node_key.verifying_key().to_bytes()),
        )
        .as_bytes(),
    )?;
    let trust_file = root.join("formal-read-trust.json");
    oracle_support::write_private(
        &trust_file,
        &serde_json::to_vec(&trust).map_err(|error| error.to_string())?,
    )?;
    let approved_file = root.join("approved-initial.cbor");
    oracle_support::write_private(
        &approved_file,
        &serde_cbor::to_vec(&approved_initial).map_err(|error| error.to_string())?,
    )?;
    let status_bind = free_loopback_address()?;
    let runtime_config_path = root.join("formal-live-runtime.json");
    let mut runtime_config = serde_json::json!({
        "schema_version": 1,
        "node_id": "formal-live-protected-test-node",
        "node_keypair_directory": node_key_dir,
        "writer_key_file": writer_key_file,
        "formal_read_trust_file": trust_file,
        "writer_directory": writer_dir,
        "primary": primary_address,
        "replica": replica_address,
        "timeout_ms": 30000,
        "status_bind": status_bind,
        "approved_initial_file": approved_file,
    });
    if let Some(scenario) = effect_scenario {
        scenario.decorate_config(&mut runtime_config)?;
    }
    oracle_support::write_private(
        &runtime_config_path,
        &serde_json::to_vec(&runtime_config).map_err(|error| error.to_string())?,
    )?;
    if let Some(deadline) = scenario_deadline {
        deadline.report_stage("runtime config and trust files written")?;
    }

    let world_identity = WorldIdentity {
        world_id: identity.world_id.clone(),
        genesis_digest: identity.genesis_digest.clone(),
    };
    let owner_request = if genesis_mode {
        request_for_first_agent(&owner_key, &world_identity)?
    } else {
        request_for_owner(&owner_key, &world_identity)
    };
    let node_signer_public = hex::encode(node_key.verifying_key().to_bytes());
    let reader_key = fixtures::key(92);
    let client_config = || WorldServiceClientConfig {
        formal_read_trust: Some(trust.clone()),
        endpoint: format!("http://{status_bind}"),
        trusted_service_public_key: node_signer_public.clone(),
        expected_world: world_identity.clone(),
        scope_id: "public".into(),
        read_private_key_hex: hex::encode(reader_key.to_bytes()),
        timeout: Duration::from_secs(30),
        max_response_bytes: 16 * 1024 * 1024,
    };
    let owner_client_config = || {
        let mut config = client_config();
        config.scope_id = if genesis_mode {
            // Bootstrap receipt is public-scope but strictly original-sender
            // bound; RegisterAgent does not mint an AgentOwner grant.
            "public".into()
        } else {
            "agent:agent".into()
        };
        config.read_private_key_hex = hex::encode(owner_key.to_bytes());
        config
    };

    if let Some(scenario) = effect_scenario {
        return scenario.exercise(effect_tests::EffectCaseContext {
            root: &root,
            config_path: &runtime_config_path,
            writer_directory: &writer_dir,
            owner_request: &owner_request,
            public_config: client_config(),
            owner_config: owner_client_config(),
            initial_anchor: &anchor,
        });
    }
    if let Some(mode @ ("survivor_frozen" | "survivor_prepared")) = crash_phase {
        return oracle_support::survivor_tests::run(
            mode,
            &runtime_config_path,
            client_config(),
            owner_client_config(),
            &world_identity,
            &owner_request,
            module_emits_initial,
            oracle_support::survivor_tests::SurvivorResources {
                primary_child,
                pdir,
                rdir,
                replica_address,
                authority: authority.clone(),
                trust: initial_trust.clone(),
                certificate: certificate.clone(),
                anchor: anchor.clone(),
                endpoint_binary,
                oracle_auth,
            },
        );
    }
    if let Some(phase) = crash_phase
        && !matches!(
            phase,
            "healthy_set"
                | "healthy_set_long"
                | "healthy_operation"
                | "healthy_fault_writer_lock"
                | "healthy_fault_old_owner_sigkill"
        )
    {
        return crash_tests::run_crash_recovery(
            phase,
            &runtime_config_path,
            client_config(),
            owner_client_config(),
            &world_identity,
            &owner_request,
            module_emits_initial,
        );
    }
    if matches!(
        crash_phase,
        Some(
            "healthy_set"
                | "healthy_set_long"
                | "healthy_operation"
                | "healthy_fault_writer_lock"
                | "healthy_fault_old_owner_sigkill"
        )
    ) {
        scenario_deadline
            .ok_or("healthy durable-set fixture lacks its scenario deadline")?
            .report_stage("opening original C0 runtime")?;
    }
    let runtime = if shipping_mode {
        None
    } else {
        let mut runtime = FormalLiveRuntime::open_with_native_history(&runtime_config_path)?;
        runtime.start()?;
        Some(runtime)
    };
    if matches!(
        crash_phase,
        Some(
            "healthy_set"
                | "healthy_set_long"
                | "healthy_operation"
                | "healthy_fault_writer_lock"
                | "healthy_fault_old_owner_sigkill"
        )
    ) && !shipping_mode
    {
        scenario_deadline
            .ok_or("healthy durable-set fixture lacks its scenario deadline")?
            .report_stage("original C0 runtime started")?;
    }
    let initial_client =
        RemoteWorldServiceClient::new(client_config()).map_err(|error| error.to_string())?;
    let initial_view = if shipping_mode {
        None
    } else {
        let view = read_current(&initial_client, &world_identity)
            .map_err(|error| format!("initial externally pinned read: {error}"))?;
        assert_eq!(view.version().commit.position, 1);
        assert!(view.formal_commit().is_some());
        Some(view)
    };
    if let Some(
        mode @ ("healthy_set"
        | "healthy_set_long"
        | "healthy_operation"
        | "healthy_fault_writer_lock"
        | "healthy_fault_old_owner_sigkill"),
    ) = crash_phase
    {
        let scenario_deadline =
            scenario_deadline.ok_or("healthy durable-set fixture lacks its scenario deadline")?;
        scenario_deadline.report_stage(if shipping_mode {
            "shipping C0 qualified; awaiting shipping owner startup"
        } else {
            "initial qualified C0 read complete"
        })?;
        let case = durable_set_tests::HealthySetCase {
            long_history: mode == "healthy_set_long",
            runtime,
            runtime_config_path,
            writer_directory: writer_dir,
            endpoint_binary,
            root,
            primary_directory: pdir,
            primary_address,
            replica_address,
            primary_child,
            replica_child,
            primary_oracle,
            replica_oracle,
            primary_provision: primary_provision
                .ok_or("healthy set test lacks original primary provision handle")?,
            oracle_authentication_file: oracle_auth,
            authority: authority.clone(),
            initial_trust,
            certificate,
            initial_proof,
            initial_anchor: anchor,
            writer_key,
            owner_key: owner_key.clone(),
            owner_request,
            world: world_identity.clone(),
            scenario_deadline,
            public_client_config: client_config(),
            owner_client_config: owner_client_config(),
        };
        return match mode {
            "healthy_operation" => healthy_operation_tests::exercise(case),
            "healthy_fault_writer_lock" => healthy_fault_tests::exercise_writer_lock(case),
            "healthy_fault_old_owner_sigkill" => {
                healthy_fault_tests::exercise_old_owner_sigkill(case)
            }
            _ => durable_set_tests::exercise(case),
        };
    }
    let mut runtime = runtime.ok_or("non-shipping fixture runtime missing")?;
    let initial_view = initial_view.ok_or("non-shipping fixture initial view missing")?;
    if unavailable_replica {
        return availability_tests::exercise(availability_tests::AvailabilityCase {
            runtime,
            runtime_config: runtime_config_path,
            endpoint_binary,
            replica_config,
            replica_address,
            replica_child,
            public_client_config: client_config(),
            owner_client_config: owner_client_config(),
            world: world_identity,
            owner_request,
            initial_view,
        });
    }
    let changes_client =
        RemoteWorldServiceClient::new(client_config()).map_err(|error| error.to_string())?;
    let changes_initial_view = read_current(&changes_client, &world_identity)
        .map_err(|error| format!("Changes origin read: {error}"))?;
    let changes_cursor = changes_client
        .formal_changes_cursor(&changes_initial_view)
        .map_err(|error| error.to_string())?;

    let admission = initial_client
        .submit(owner_request.clone())
        .map_err(|error| format!("ordinary signed HTTP submit: {error}"))?;
    let admission = match admission {
        SubmitObservation::Response(response) => *response,
        SubmitObservation::OutcomeUnknown(_) => {
            return Err("ordinary client could not verify formal-profile admission".into());
        }
    };
    if admission.correlation != owner_request.correlation
        || admission.formal_profile.as_ref()
            != Some(&oasis7_client_api::world_service::FormalReadProfileV1::controlled())
        || admission.execution_evidence_scope.is_some()
        || !matches!(
            admission.outcome,
            oasis7_client_api::world_service::IntentOutcome::Received {
                durability: oasis7_client_api::world_service::AdmissionDurability::Volatile
            }
        )
    {
        return Err("ordinary client did not verify a volatile formal-profile admission".into());
    }

    wait_for_published_position(&runtime, 2)?;
    let first_view = read_until_position(&initial_client, &world_identity, 2)
        .map_err(|error| format!("first qualified publication read: {error}"))?;
    let changes = changes_client
        .read_formal_changes(&changes_cursor, 1024, 4 * 1024 * 1024)
        .map_err(|error| format!("ordinary formal Changes: {error}"))?;
    if changes.next_cursor().cursor().commit != first_view.version().commit {
        if changes.next_cursor().cursor().commit.position < first_view.version().commit.position {
            return Err("formal Changes regressed behind the prior verified read".into());
        }
        let exact_changes = runtime
            .history
            .fixed_publication(&changes.next_cursor().cursor().commit)?;
        runtime.history.history_page(
            first_view
                .formal_read_anchor()
                .ok_or("prior verified read anchor missing")?,
            exact_changes.anchor(),
            oasis7::world_service::formal_read::MAX_FORMAL_HISTORY_STEPS,
        )?;
    }
    let (first_cut, original_result) = wait_for_canonical_request_result(&runtime, &owner_request)?;
    if genesis_mode && wasm_emit_count(&first_cut.world) <= module_emits_initial {
        return Err("formal execution did not invoke the actual Wasmtime tick observer".into());
    }
    if first_cut.anchor.head().head().position != first_view.version().commit.position
        || first_cut.anchor.head().head().record_id
            != first_view.version().commit.execution_block_hash
    {
        assert_live_descends_from_view(&runtime, &first_cut, &first_view)?;
    }
    let result_key = correlation::key_digest(&owner_request.correlation.key)
        .map_err(|error| error.to_string())?;
    let game_task_before = if genesis_mode {
        if !first_cut
            .world
            .state()
            .agents
            .contains_key(oasis7::viewer::FIRST_AGENT_CLAIM_TARGET_AGENT_ID)
        {
            return Err("canonical genesis gameplay action did not create the first agent".into());
        }
        None
    } else {
        Some(
            game_task::current_control(&first_cut.world, "agent")
                .map_err(|error| error.to_string())?
                .ok_or("canonical game task effect was not published")?,
        )
    };
    let first_result_commit = if genesis_mode {
        None
    } else {
        let owner_client = RemoteWorldServiceClient::new(owner_client_config())
            .map_err(|error| error.to_string())?;
        Some(lookup_original_commit(&owner_client, &owner_request)?)
    };
    if let Some(commit) = &first_result_commit {
        assert_admin_commit_did_not_emit_wasm(&writer_dir, commit)?;
    }
    if long_history {
        return long_history_tests::exercise_long_history(
            runtime,
            &runtime_config_path,
            &writer_dir,
            client_config(),
            owner_client_config(),
            &world_identity,
            &owner_key,
            &owner_request,
            [primary_child.pid(), replica_child.pid()],
            first_result_commit
                .as_ref()
                .ok_or("long history needs owner request")?,
        );
    }
    let module_emits_before = wasm_emit_count(&first_cut.world);
    if genesis_mode && module_emits_before == 0 {
        return Err("the actual Wasmtime action observer emitted no wasm.test event".into());
    }

    let (_before_restart_view, _before_restart_cut) =
        if let Some(first_result_commit) = first_result_commit.as_ref() {
            let owner_client = RemoteWorldServiceClient::new(owner_client_config())
                .map_err(|error| error.to_string())?;
            let position_before_duplicate =
                runtime.published.current()?.anchor.head().record().position;
            let duplicate = owner_client
                .submit(owner_request.clone())
                .map_err(|error| format!("resubmit original Game task request: {error}"))?;
            assert_volatile_admission(duplicate, &owner_request.correlation)?;

            let next_position = position_before_duplicate
                .checked_add(1)
                .ok_or("qualified Node position overflow after duplicate submit")?;
            wait_for_published_position(&runtime, next_position)?;
            let next_view = read_until_position(&initial_client, &world_identity, next_position)
                .map_err(|error| format!("duplicate publication read: {error}"))?;
            let next_cut = runtime.published.current()?;
            if next_cut.anchor.head().head().position != next_view.version().commit.position
                || next_cut.anchor.head().head().record_id
                    != next_view.version().commit.execution_block_hash
            {
                assert_live_descends_from_view(&runtime, &next_cut, &next_view)?;
            }
            let game_task_after_duplicate = game_task::current_control(&next_cut.world, "agent")
                .map_err(|error| error.to_string())?
                .ok_or("Game task effect missing after duplicate submit")?;
            if Some(game_task_after_duplicate) != game_task_before {
                return Err("duplicate submit changed canonical Game task identity".into());
            }
            if &lookup_original_commit(&owner_client, &owner_request)? != first_result_commit {
                return Err(
                    "lookup after duplicate submit changed the original result CommitRef".into(),
                );
            }
            (next_view, next_cut)
        } else {
            (first_view, first_cut)
        };

    runtime.stop(Duration::from_secs(10))?;
    let stopped_cut = runtime.published.current()?;
    let stopped_snapshot = stopped_cut.world.snapshot();
    drop(runtime);

    let mut restarted = FormalLiveRuntime::open_with_native_history(&runtime_config_path)?;
    let restored_before_start = restarted.published.current()?;
    if restored_before_start.world.snapshot() != stopped_snapshot
        || restored_before_start.anchor.head().record() != stopped_cut.anchor.head().record()
        || wasm_emit_count(&restored_before_start.world) != wasm_emit_count(&stopped_cut.world)
    {
        return Err("opening Node replayed or changed an already-qualified World".into());
    }
    restarted.start()?;
    let restarted_client =
        RemoteWorldServiceClient::new(client_config()).map_err(|error| error.to_string())?;
    let after_restart_view = read_current(&restarted_client, &world_identity)
        .map_err(|error| format!("reopened qualified publication read: {error}"))?;
    let reopened_cut = restarted.published.current()?;
    // A running Node can legitimately qualify a successor while HTTP is in
    // flight. Compare exact restored bytes before start, then verify ancestry
    // of the independently checked client cut rather than requiring simultaneity.
    assert_live_descends_from_view(&restarted, &reopened_cut, &after_restart_view)?;
    if after_restart_view.version().commit.position < stopped_cut.anchor.head().record().position {
        return Err("restart HTTP view regressed behind the qualified stopped cut".into());
    }
    let result_after_restart = retained_result_from_hot_or_cold(&reopened_cut.world, &result_key)?;
    if result_after_restart != original_result {
        return Err("restart changed the original canonical request result".into());
    }
    let game_task_after = if genesis_mode {
        if !reopened_cut
            .world
            .state()
            .agents
            .contains_key(oasis7::viewer::FIRST_AGENT_CLAIM_TARGET_AGENT_ID)
        {
            return Err("genesis gameplay effect missing after restart".into());
        }
        None
    } else {
        Some(
            game_task::current_control(&reopened_cut.world, "agent")
                .map_err(|error| error.to_string())?
                .ok_or("canonical game task effect missing after restart")?,
        )
    };
    if game_task_after != game_task_before {
        return Err("restart replayed the canonical effect or Wasmtime action observer".into());
    }
    if let Some(first_result_commit) = first_result_commit.as_ref() {
        let restarted_owner_client = RemoteWorldServiceClient::new(owner_client_config())
            .map_err(|error| error.to_string())?;
        if &lookup_original_commit(&restarted_owner_client, &owner_request)? != first_result_commit
        {
            return Err("lookup after restart changed the original result CommitRef".into());
        }
    }
    restarted.stop(Duration::from_secs(10))?;
    drop((primary_oracle, replica_oracle, coordinator));
    Ok(())
}

#[path = "formal_live_protected_process_helpers.rs"]
mod helpers;
use helpers::*;
