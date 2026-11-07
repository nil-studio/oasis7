//! Bounded trusted fixture setup for existing identities only. No model or context.
use oasis7_agent_api::game::GameTaskFenceV1;
use oasis7_client_storage::identity_store::ExecutorControlStore;
use oasis7_game_client::{
    device_client::Client,
    identity_client::{EntryOwnerProgress, ParticipantClient, ParticipantClientConfig},
};
use serde_json::{Value, json};
use std::{
    io::Write,
    os::unix::fs::PermissionsExt,
    path::Path,
    time::{Duration, Instant},
};
fn retry_rate<T>(
    mut call: impl FnMut() -> Result<T, String>,
    deadline: Instant,
) -> Result<T, String> {
    while Instant::now() < deadline {
        match call() {
            Ok(value) => return Ok(value),
            Err(error) if error.starts_with("rate_limited") => {
                std::thread::sleep(Duration::from_secs(2))
            }
            Err(error) => return Err(error),
        }
    }
    Err("fixture_deadline".into())
}
fn owner_step(
    mut call: impl FnMut() -> Result<EntryOwnerProgress, String>,
    deadline: Instant,
) -> Result<(), String> {
    while Instant::now() < deadline {
        let progress = match call() {
            Ok(progress) => progress,
            Err(error) if error.starts_with("rate_limited") => {
                std::thread::sleep(Duration::from_secs(2));
                continue;
            }
            Err(error) => return Err(error),
        };
        if progress.state == "confirmed" {
            return Ok(());
        }
        std::thread::sleep(Duration::from_secs(2));
    }
    Err("fixture_deadline".into())
}
fn private_json(path: &Path, value: &Value) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(value).map_err(|_| "fixture_encode")?;
    if path.exists() {
        if std::fs::read(path).map_err(|_| "fixture_read")? == bytes {
            return Ok(());
        }
        return Err("immutable_fixture_conflict".into());
    }
    let mut file = std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)
        .map_err(|_| "fixture_write")?;
    file.set_permissions(std::fs::Permissions::from_mode(0o600))
        .map_err(|_| "fixture_permissions")?;
    file.write_all(&bytes)
        .and_then(|_| file.sync_all())
        .map_err(|_| "fixture_write".into())
}
fn run() -> Result<(), String> {
    let root = std::env::args().nth(1).ok_or("fixture_root_required")?;
    let root = Path::new(&root);
    let runtime: Value = serde_json::from_slice(
        &std::fs::read(root.join("runtime-private.json"))
            .map_err(|_| "private_runtime_unavailable")?,
    )
    .map_err(|_| "runtime_invalid")?;
    let catalog: Vec<Value> = serde_json::from_slice(
        &std::fs::read(root.join("catalog/catalog.json")).map_err(|_| "catalog_unavailable")?,
    )
    .map_err(|_| "catalog_invalid")?;
    let unlock = zeroize::Zeroizing::new(
        std::fs::read(root.join("unlock.bin")).map_err(|_| "private_unlock_unavailable")?,
    );
    let deadline = Instant::now() + Duration::from_secs(180);
    let mut result = Vec::new();
    for label in ["A", "B"] {
        if Instant::now() >= deadline {
            return Err("fixture_deadline".into());
        }
        let account = catalog
            .iter()
            .find(|a| a["alias"] == label)
            .ok_or("existing_account_unavailable")?;
        let login = account["login_operation"]
            .as_str()
            .ok_or("existing_login_unavailable")?;
        let dir = root.join(format!("read-5535-{label}"));
        std::fs::create_dir_all(&dir).map_err(|_| "fixture_directory")?;
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700))
            .map_err(|_| "fixture_permissions")?;
        let mut entry = runtime["plan_template"]["entry"].clone();
        entry["identity"] = serde_json::from_slice(
            &std::fs::read(root.join(format!("account-{label}.json")))
                .map_err(|_| "identity_configuration_unavailable")?,
        )
        .map_err(|_| "identity_configuration_invalid")?;
        entry["owner_state_directory"] = json!(dir.join("owner"));
        let config: ParticipantClientConfig = serde_json::from_value(entry.clone())
            .map_err(|_| "participant_configuration_invalid")?;
        if !dir.join("owner").exists() {
            ParticipantClient::initialize_owner(&config, &unlock)?;
        }
        let client = ParticipantClient::new(config, &unlock)?;
        let op = |stage: &str| format!("dev-test-5535-{label}-{stage}-v1");
        let participant = op("participant");
        eprintln!("fixture_stage {label} participant");
        loop {
            if Instant::now() >= deadline {
                return Err("fixture_deadline".into());
            }
            let progress = match client.register(&participant, login) {
                Ok(progress) => progress,
                Err(error) if error.starts_with("rate_limited") => {
                    std::thread::sleep(Duration::from_secs(2));
                    continue;
                }
                Err(error) => return Err(error),
            };
            if progress.state == "confirmed" || client.current_owner(&participant, login).is_ok() {
                break;
            }
            std::thread::sleep(Duration::from_secs(2));
        }
        let vault = dir.join("executor-vault");
        let executor = if !vault.exists() {
            client.initialize_executor(&vault, &unlock)?
        } else {
            let store = ExecutorControlStore::open(&vault, &unlock)?;
            hex::encode(
                ed25519_dalek::SigningKey::from_bytes(store.executor_seed())
                    .verifying_key()
                    .to_bytes(),
            )
        };
        eprintln!("fixture_stage {label} task");
        owner_step(
            || client.create_task(&participant, login, &op("task")),
            deadline,
        )?;
        eprintln!("fixture_stage {label} goal");
        let goal: oasis7_agent_api::game::GoalContentV1 =
            serde_json::from_value(runtime["plan_template"]["goal"].clone())
                .map_err(|_| "goal_invalid")?;
        retry_rate(
            || client.prepare_goal(&participant, login, &op("goal-prepare"), goal.clone()),
            deadline,
        )?;
        owner_step(
            || {
                client.publish_prepared_goal(
                    &participant,
                    login,
                    &op("goal-publish"),
                    &op("goal-prepare"),
                )
            },
            deadline,
        )?;
        eprintln!("fixture_stage {label} executor");
        owner_step(
            || client.authorize_executor(&participant, login, &op("authorize"), &executor, 1, 1),
            deadline,
        )?;
        owner_step(
            || {
                client.bind_executor(
                    &participant,
                    login,
                    &op("bind"),
                    &format!("dev-test-5535-{label}"),
                    &executor,
                    1,
                )
            },
            deadline,
        )?;
        let current = retry_rate(|| client.current_owner(&participant, login), deadline)?;
        let task = current.task.ok_or("task_unavailable")?;
        let fence = GameTaskFenceV1::from_control(&task);
        let device = json!({"authority_mode":"native_executor_v1","identity":entry["identity"],"login_operation":login,"executor_vault_directory":vault,"service_public_key":entry["trusted_world_service_key"],"fence":fence,"state_directory":dir.join("executor-state")});
        let path = dir.join("executor.json");
        private_json(&path, &device)?;
        let reader = Client::open_authenticated(&path, &unlock)?;
        let actual = reader.task_fence()?;
        if actual != fence {
            return Err("authoritative_fence_mismatch".into());
        }
        result.push(json!({"account_ref":account["account_ref"],"account_id":account["account_id"],"environment":account["environment"],"executor_config":path,"executor_config_digest":blake3::hash(&std::fs::read(&path).map_err(|_|"fixture_read")?).to_hex().to_string(),"fence":actual,"scope":"actual_existing_account_readonly_resource"}));
    }
    let result = json!({"accounts":result,"context_created":false,"world_action_sent":false,"real_external_agent":false});
    private_json(&root.join("read-5535-result.json"), &result)?;
    println!("{}", result);
    Ok(())
}
fn main() {
    if let Err(error) = run() {
        // SDK errors contain bounded protocol codes, never original HTTP bodies.
        eprintln!(
            "fixture_preparation_failed_retain_originals: {}",
            error.chars().take(160).collect::<String>()
        );
        std::process::exit(1)
    }
}
