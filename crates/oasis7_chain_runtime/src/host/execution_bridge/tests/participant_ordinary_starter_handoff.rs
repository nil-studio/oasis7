//! Explicit fixture handoff after genuine public claim/build/transport.
use super::*;
use std::io::Write;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

pub(super) struct Inputs<'a> {
    pub ready: &'a Path,
    pub root: &'a Path,
    pub endpoint: &'a str,
    pub identity: &'a WorldIdentity,
    pub service_public_key: &'a str,
    pub participant: &'a GameParticipantReceiptV1,
    pub executor_private_key: &'a str,
}

fn private_file(path: &Path, bytes: &[u8]) {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .unwrap();
    file.write_all(bytes).unwrap();
    file.sync_all().unwrap();
}

pub(super) fn wait(
    input: Inputs<'_>,
) -> Option<oasis7_agent_api::world_action::OrdinaryStarterFirstProductionV1> {
    assert!(input.ready.is_absolute());
    assert!(
        !input.ready.exists(),
        "handoff metadata cannot overwrite an existing fixture"
    );
    let device = input.root.join("ordinary-client-device");
    fs::create_dir(&device).unwrap();
    fs::set_permissions(&device, fs::Permissions::from_mode(0o700)).unwrap();
    let state = device.join("state");
    fs::create_dir(&state).unwrap();
    fs::set_permissions(&state, fs::Permissions::from_mode(0o700)).unwrap();
    let key = device.join("executor.key");
    private_file(&key, &hex::decode(input.executor_private_key).unwrap());
    let config = device.join("client.json");
    private_file(
        &config,
        &serde_json::to_vec(&serde_json::json!({
            "endpoint": input.endpoint, "world_id": input.identity.world_id,
            "genesis_digest": input.identity.genesis_digest, "agent_id": input.participant.agent_id,
            "service_public_key": input.service_public_key, "signer_file": key,
            "state_directory": state,
        "authority_mode": "native_executor_v1",
        }))
        .unwrap(),
    );
    let stop = input.ready.with_extension("stop");
    let result = input.ready.with_extension("result.json");
    assert!(
        !stop.exists() && !result.exists(),
        "fresh handoff paths required"
    );
    let current = pin(input.root, input.identity);
    assert!(
        starter(&current.world, &input.participant.agent_id)
            .claim
            .unwrap()
            .first_production
            .is_none()
    );
    let metadata = serde_json::json!({
        "fixture": "ordinary-native-first-production-v1", "root": input.root,
        "endpoint": input.endpoint, "config": config, "agent_id": input.participant.agent_id,
        "world": input.identity, "stop_file": stop, "result_file": result,
        "goal": "In your own legally authorized factory, complete exactly one batch of iron ingots and verify its actual outputs.",
        "scenario_profile": "prepared_world", "preparation_profile": "ordinary_first_session",
        "cold_entry_model_validated": false,
        "timeout_seconds": 600, "max_tool_calls": 40,
        "preparation": "real participant registration, public starter claim, owner grants, factory build and input transport",
    });
    private_file(input.ready, &serde_json::to_vec(&metadata).unwrap());
    eprintln!("ORDINARY_STARTER_PLAYTEST_READY {}", input.ready.display());
    let deadline = Instant::now() + Duration::from_secs(600);
    while !stop.exists() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(100));
    }
    // This fixture never finishes the player's recipe. An unsuccessful tester
    // remains an unsuccessful playtest, with its actual canonical state retained.
    let current = pin(input.root, input.identity);
    let status = starter(&current.world, &input.participant.agent_id);
    let first = status
        .claim
        .as_ref()
        .and_then(|claim| claim.first_production.clone());
    private_file(&result, &serde_json::to_vec(&serde_json::json!({
        "observed_at_tick": current.world.state().time,
        "stopped_by_operator": stop.exists(), "first_production": first,
        "ordinary_starter": status,
        "authorizations": current.world.agent_delegation_authorizations(&input.participant.agent_id).unwrap(),
    })).unwrap());
    first
}
