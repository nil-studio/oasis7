//! Trusted local cold entry orchestration, excluded from executor tool routing.
use crate::identity_client::{IdentityClient, ParticipantClient, ParticipantClientConfig};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use ed25519_dalek::SigningKey;
use oasis7_agent_api::game::{DecimalU64, GoalContentV1};
use oasis7_client_storage::{
    identity_store::{ExecutorControlStore, OwnerControlStore},
    private_storage,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    io::{IsTerminal, Read},
    path::{Path, PathBuf},
};
use zeroize::Zeroizing;
#[path = "cold_entry_run.rs"]
mod execution;
#[path = "cold_entry_login.rs"]
mod login;
#[cfg(test)]
#[path = "cold_entry_tests.rs"]
mod tests;
const SCOPE: &str = "trusted-cold-entry/v1";
const PLAN: &str = "immutable-plan";
const STAGES: [&str; 10] = [
    "registration",
    "login",
    "participant",
    "starter",
    "task",
    "goal_prepare",
    "goal_publish",
    "delegation",
    "executor_bind",
    "complete",
];
pub(crate) const HELP: &str = "oasis7_game_client cold-entry init --plan <private-owner-approved.json> | cold-entry run|status --directory <orchestration-vault> | cold-entry fresh-login --directory <vault> --operation <explicit-stable-id>\nTrusted local control only; password via private stdin. The private plan binds complete goal, trust pins, starter choice, independent vaults and executor identity. Never pass it or the owner unlock to a Runtime. Partial initialization requires explicit recovery of original vaults; init never generates replacement keys. status is local historical progress, not canonical authority. No Runtime is launched.";

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ColdEntryPlan {
    owner_approved: bool,
    entry: ParticipantClientConfig,
    orchestration_directory: PathBuf,
    executor_vault_directory: PathBuf,
    executor_state_directory: PathBuf,
    executor_config_directory: PathBuf,
    display_name: String,
    starter: Starter,
    executor_id: String,
    delegation_generation: DecimalU64,
    delegation_nonce: DecimalU64,
    goal: GoalContentV1,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Starter {
    Claim,
    Skip,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ImmutablePlan {
    plan: ColdEntryPlan,
    operations: Vec<String>,
}
#[derive(Serialize, Deserialize, PartialEq, Eq)]
struct Keys {
    identity: String,
    owner: String,
    executor: String,
}
impl ColdEntryPlan {
    fn validate(&self) -> Result<(), String> {
        ParticipantClient::validate(&self.entry)?;
        if !self.owner_approved
            || self.display_name.trim().is_empty()
            || self.display_name.len() > 128
            || self.display_name.chars().any(char::is_control)
            || self.executor_id.trim().is_empty()
            || self.executor_id.len() > 256
            || self.delegation_generation.0 == 0
        {
            return Err("explicit valid owner-approved cold entry plan required".into());
        }
        self.goal.validate().map_err(str::to_owned)?;
        let paths = [
            &self.entry.identity.state_directory,
            &self.entry.owner_state_directory,
            &self.orchestration_directory,
            &self.executor_vault_directory,
            &self.executor_state_directory,
            &self.executor_config_directory,
        ];
        for (i, path) in paths.iter().enumerate() {
            if !path.is_absolute()
                || path.components().any(|v| {
                    !matches!(
                        v,
                        std::path::Component::RootDir | std::path::Component::Normal(_)
                    )
                })
                || paths.iter().enumerate().any(|(j, other)| {
                    i != j && (path.starts_with(other) || other.starts_with(path))
                })
            {
                return Err("cold entry directories must be absolute independent nonoverlapping private paths".into());
            }
        }
        Ok(())
    }
}
fn require_enabled(value: &Value) -> Result<(), String> {
    for phase in [
        "machine_registration",
        "participant_registration",
        "owner_control",
        "executor_control",
    ] {
        if value["phase_capabilities"][phase] != true {
            return Err(
                "unsupported: cold entry phase unavailable; no identity or vault was created"
                    .into(),
            );
        }
    }
    Ok(())
}
fn preflight(plan: &ColdEntryPlan) -> Result<(), String> {
    plan.validate()?;
    let discovery = IdentityClient::discover_entry(&plan.entry.identity)?;
    validate_discovery(plan, &discovery)
}
fn validate_discovery(plan: &ColdEntryPlan, value: &Value) -> Result<(), String> {
    require_enabled(value)?;
    let expected_epoch = plan.entry.identity_instance_epoch.0.to_string();
    if value["identity"]["service_id"] != plan.entry.identity.identity_service_id
        || value["identity"]["environment_id"] != plan.entry.identity.environment_id
        || value["world"]
            != serde_json::to_value(&plan.entry.world).map_err(|_| "world trust encoding failed")?
        || value["world_service_public_key"] != plan.entry.trusted_world_service_key
        || value["identity"]["instance_epoch"].as_str() != Some(expected_epoch.as_str())
    {
        return Err("cold entry world, service pin or identity epoch mismatch".into());
    }
    Ok(())
}
fn private_json<T: serde::de::DeserializeOwned>(path: &Path, bound: usize) -> Result<T, String> {
    let parent = path.parent().ok_or("private plan requires absolute path")?;
    let directory = private_storage::open_root(parent)?;
    let name = path
        .file_name()
        .and_then(|v| v.to_str())
        .ok_or("invalid private input path")?;
    let bytes = Zeroizing::new(private_storage::read_private(&directory, name, bound)?);
    oasis7_agent_api::game::parse_request(&bytes)
        .map_err(|_| "invalid bounded private input".into())
}
fn random_operation() -> Result<String, String> {
    let mut bytes = [0; 32];
    getrandom::fill(&mut bytes).map_err(|_| "cold entry randomness unavailable")?;
    Ok(URL_SAFE_NO_PAD.encode(bytes))
}
fn initialize(plan: ColdEntryPlan, password: &[u8]) -> Result<Value, String> {
    // Discovery is deliberately before creating even the orchestration namespace.
    preflight(&plan)?;
    initialize_vaults(plan, password)
}
fn initialize_vaults(plan: ColdEntryPlan, password: &[u8]) -> Result<Value, String> {
    plan.validate()?;
    for path in [
        &plan.orchestration_directory,
        &plan.entry.identity.state_directory,
        &plan.entry.owner_state_directory,
        &plan.executor_vault_directory,
        &plan.executor_state_directory,
        &plan.executor_config_directory,
    ] {
        if path
            .try_exists()
            .map_err(|_| "cold entry path unavailable")?
        {
            return Err("action_required: initialization requires fresh directories; preserve and recover original partial initialization, never replace keys".into());
        }
    }
    let store = OwnerControlStore::create(&plan.orchestration_directory, password)?;
    let _lock = store.lock_workflow()?;
    let immutable = ImmutablePlan {
        plan: plan.clone(),
        operations: (0..STAGES.len())
            .map(|_| random_operation())
            .collect::<Result<_, _>>()?,
    };
    store.put_original(SCOPE, PLAN, "plan", &immutable)?;
    // Any interruption after this durable plan is fail closed: run requires all
    // original roles and the immutable key manifest. init refuses this namespace.
    let identity = IdentityClient::initialize(&plan.entry.identity, password)?;
    let owner = ParticipantClient::initialize_owner(&plan.entry, password)?;
    let executor = ExecutorControlStore::create(&plan.executor_vault_directory, password)?;
    let executor = hex::encode(
        SigningKey::from_bytes(executor.executor_seed())
            .verifying_key()
            .to_bytes(),
    );
    if identity == owner || owner == executor || identity == executor {
        return Err("independent role keys required".into());
    }
    private_storage::open_root(&plan.executor_state_directory)?;
    private_storage::open_root(&plan.executor_config_directory)?;
    store.put_original(
        SCOPE,
        PLAN,
        "initialized-keys",
        &Keys {
            identity,
            owner,
            executor,
        },
    )?;
    Ok(
        json!({"stage":"initialized","state":"confirmed","next_action":"explicit_cold_entry_run","network_effects":"discovery_only"}),
    )
}
fn open(directory: &Path, password: &[u8]) -> Result<(OwnerControlStore, ImmutablePlan), String> {
    let store = OwnerControlStore::open(directory, password)?;
    let immutable = store
        .read_original::<ImmutablePlan>(SCOPE, PLAN, "plan")?
        .ok_or("action_required: original immutable cold entry plan missing")?;
    immutable.plan.validate()?;
    if immutable.plan.orchestration_directory != directory
        || immutable.operations.len() != STAGES.len()
    {
        return Err("cold entry original plan binding mismatch".into());
    }
    let unique = immutable
        .operations
        .iter()
        .collect::<std::collections::BTreeSet<_>>();
    if unique.len() != STAGES.len()
        || immutable.operations.iter().any(|v| {
            URL_SAFE_NO_PAD
                .decode(v)
                .map_or(true, |b| b.len() != 32 || URL_SAFE_NO_PAD.encode(b) != *v)
        })
    {
        return Err("cold entry original operation identities invalid".into());
    }
    Ok((store, immutable))
}
fn status(store: &OwnerControlStore, immutable: &ImmutablePlan) -> Result<Value, String> {
    if store
        .read_original::<Keys>(SCOPE, PLAN, "initialized-keys")?
        .is_none()
    {
        return Ok(
            json!({"stage":"initialization","state":"action_required","next_action":"recover_original_partial_initialization_no_automatic_key_generation"}),
        );
    }
    if let Some(pending) = login::selection(store, immutable)?.pending {
        return Ok(
            json!({"stage":"login_refresh","state":"action_required","operation_id":pending,"next_action":"explicit_fresh_login_same_operation"}),
        );
    }
    for (i, stage) in STAGES.iter().enumerate() {
        if store.read_original::<bool>(SCOPE, &immutable.operations[i], "confirmed")? != Some(true)
        {
            let started =
                store.read_original::<bool>(SCOPE, &immutable.operations[i], "started")?
                    == Some(true);
            return Ok(
                json!({"stage":stage,"state":if started{"unknown"}else{"not_started"},"operation_id":immutable.operations[i],"next_action":if started{"query_original_with_same_plan"}else{"explicit_cold_entry_run"}}),
            );
        }
    }
    Ok(
        json!({"stage":"complete","state":"historically_confirmed","next_action":"fresh_canonical_revalidation_required_before_executor_use"}),
    )
}
pub(crate) fn run(args: &[String]) -> Result<(), String> {
    let ordinary = args.len() == 4
        && args[0] == "cold-entry"
        && ["init", "run", "status"].contains(&args[1].as_str())
        && args[2]
            == if args[1] == "init" {
                "--plan"
            } else {
                "--directory"
            };
    let fresh = args.len() == 6
        && args[0] == "cold-entry"
        && args[1] == "fresh-login"
        && args[2] == "--directory"
        && args[4] == "--operation";
    if !ordinary && !fresh {
        return Err(HELP.into());
    }
    if std::io::stdin().is_terminal() {
        return Err("action_required: trusted local private stdin required; never expose owner unlock to Runtime".into());
    }
    let mut password = Zeroizing::new(Vec::new());
    std::io::stdin()
        .take(1025)
        .read_to_end(&mut password)
        .map_err(|_| "private unlock unavailable")?;
    if !(12..=1024).contains(&password.len()) {
        return Err("private unlock requires 12 to 1024 bytes".into());
    }
    let result = if args[1] == "init" {
        initialize(private_json(Path::new(&args[3]), 16384)?, &password)?
    } else {
        let (store, immutable) = open(Path::new(&args[3]), &password)?;
        let _lock = store.lock_workflow()?;
        if fresh {
            login::fresh_login(&store, &immutable, &password, &args[5])?
        } else if args[1] == "status" {
            status(&store, &immutable)?
        } else {
            execution::execute(&store, &immutable, &password)?
        }
    };
    crate::device_client::print(&result)
}
