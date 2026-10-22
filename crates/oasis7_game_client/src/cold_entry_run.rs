use super::*;
use oasis7_agent_api::game::{GameTaskControlV1, GameTaskFenceV1, GameTaskStatusV1};
use oasis7_client_storage::identity_store::IdentityStore;

pub(super) fn verify_keys(
    store: &OwnerControlStore,
    plan: &ColdEntryPlan,
    password: &[u8],
) -> Result<Keys, String> {
    let expected=store.read_original::<Keys>(SCOPE,PLAN,"initialized-keys")?
        .ok_or("action_required: recover original partial initialization; never create replacement keys")?;
    let identity = IdentityStore::open(&plan.entry.identity.state_directory, password)?;
    let owner = OwnerControlStore::open(&plan.entry.owner_state_directory, password)?;
    let executor = ExecutorControlStore::open(&plan.executor_vault_directory, password)?;
    let actual = Keys {
        identity: hex::encode(
            SigningKey::from_bytes(identity.login_seed())
                .verifying_key()
                .to_bytes(),
        ),
        owner: hex::encode(
            SigningKey::from_bytes(owner.owner_seed())
                .verifying_key()
                .to_bytes(),
        ),
        executor: hex::encode(
            SigningKey::from_bytes(executor.executor_seed())
                .verifying_key()
                .to_bytes(),
        ),
    };
    if actual != expected
        || actual.owner == actual.identity
        || actual.executor == actual.owner
        || actual.executor == actual.identity
    {
        return Err("cold entry original role vault binding mismatch".into());
    }
    // Missing operation/config directories are not silently regenerated on resume.
    for path in [
        &plan.executor_state_directory,
        &plan.executor_config_directory,
    ] {
        if !path
            .try_exists()
            .map_err(|_| "executor directory unavailable")?
        {
            return Err("action_required: original executor directory missing".into());
        }
        private_storage::open_root(path)?;
    }
    Ok(actual)
}
fn confirmed(store: &OwnerControlStore, operation: &str) -> Result<bool, String> {
    Ok(store.read_original::<bool>(SCOPE, operation, "confirmed")? == Some(true))
}
fn start(store: &OwnerControlStore, operation: &str) -> Result<(), String> {
    store
        .put_original(SCOPE, operation, "started", &true)
        .map(|_| ())
}
fn finish(store: &OwnerControlStore, operation: &str) -> Result<(), String> {
    store
        .put_original(SCOPE, operation, "confirmed", &true)
        .map(|_| ())
}
fn stopped(stage: &str, operation: &str, state: &str, next: &str) -> Value {
    json!({"stage":stage,"operation_id":operation,"state":state,"next_action":next})
}
fn ensure_current_task(
    plan: &ColdEntryPlan,
    task: &GameTaskControlV1,
    keys: &Keys,
    operation: &str,
) -> Result<(), String> {
    if task.created_operation_id != operation
        || task.world_id != plan.entry.world.world_id
        || task.genesis_digest != plan.entry.world.genesis_digest
        || task.owner_public_key != keys.owner
    {
        return Err("action_required: canonical task differs from original cold entry task; no takeover or replacement permitted".into());
    }
    Ok(())
}
pub(super) fn execute(
    store: &OwnerControlStore,
    immutable: &ImmutablePlan,
    password: &[u8],
) -> Result<Value, String> {
    let p = &immutable.plan;
    let mut selected_operations = immutable.operations.clone();
    let selection = login::selection(store, immutable)?;
    if let Some(pending) = selection.pending {
        return Ok(stopped(
            "login_refresh",
            &pending,
            "action_required",
            "explicit_fresh_login_same_operation",
        ));
    }
    selected_operations[1] = selection.active;
    let ops = &selected_operations;
    preflight(p)?;
    let keys = verify_keys(store, p, password)?;
    let identity = IdentityClient::new(p.entry.identity.clone(), password)?;
    for i in 0..2 {
        // Login must always revalidate the original authenticated session. An
        // absent cookie never causes creation of another login operation.
        if i == 0 && confirmed(store, &ops[i])? {
            continue;
        }
        start(store, &ops[i])?;
        let result = if i == 0 {
            let historical = identity.status(&ops[i])?;
            if historical.state == "confirmed" {
                Ok(historical)
            } else {
                identity.register(&ops[i], &p.display_name)
            }
        } else {
            identity.login(&ops[i])
        };
        let progress = match result {
            Ok(progress) => progress,
            Err(_) => {
                return Ok(stopped(
                    STAGES[i],
                    &ops[i],
                    "unknown",
                    if i == 1 {
                        "reconcile_original_login_or_explicit_fresh_login"
                    } else {
                        "reconcile_original_registration"
                    },
                ));
            }
        };
        if progress.state != "confirmed" {
            return Ok(stopped(
                STAGES[i],
                &ops[i],
                &progress.state,
                &progress.next_action,
            ));
        }
        let account = progress
            .account_id
            .ok_or("confirmed identity account unavailable")?;
        let expected: String = if i == 0 {
            store.put_original(SCOPE, PLAN, "registered-account", &account)?
        } else {
            store
                .read_original(SCOPE, PLAN, "registered-account")?
                .ok_or("original registered account binding missing")?
        };
        if account != expected {
            return Err(
                "action_required: authenticated account differs from original registration".into(),
            );
        }
        finish(store, &ops[i])?;
    }
    drop(identity);
    let client = ParticipantClient::new(p.entry.clone(), password)?;
    if !confirmed(store, &ops[2])? {
        start(store, &ops[2])?;
        // Existing SDK looks up its exact signed original before resubmission.
        let progress = match client.register(&ops[2], &ops[1]) {
            Ok(v) => v,
            Err(_) => {
                return Ok(stopped(
                    STAGES[2],
                    &ops[2],
                    "unknown",
                    "query_original_participant",
                ));
            }
        };
        if let Some(stop) = participant_result(&progress, &ops[2], || {
            client.current_owner(&ops[2], &ops[1]).map(|_| ())
        })? {
            return Ok(stop);
        }
        finish(store, &ops[2])?;
    }
    let current = client.current_owner(&ops[2], &ops[1])?;
    if let Some(task) = &current.task {
        ensure_current_task(p, task, &keys, &ops[4])?;
    }
    if !confirmed(store, &ops[3])? {
        start(store, &ops[3])?;
        if matches!(p.starter, Starter::Claim) {
            let eligibility = client.starter_eligibility(&ops[2], &ops[1])?;
            match eligibility["eligibility"]["status"].as_str() {
                Some("claimed") => {
                    // If a previous attempt signed a claim, reconcile that exact
                    // original even when the canonical eligibility now says claimed.
                    if client.owner_operation_status(&ops[2], &ops[3])?.state != "not_started"
                        && let Some(stop) = owner_result(
                            client.claim_starter(&ops[2], &ops[1], &ops[3]),
                            STAGES[3],
                            &ops[3],
                        )
                    {
                        return Ok(stop);
                    }
                }
                Some("eligible") => {
                    let result = client.claim_starter(&ops[2], &ops[1], &ops[3]);
                    if let Some(stop) = owner_result(result, STAGES[3], &ops[3]) {
                        return Ok(stop);
                    }
                    if client.starter_eligibility(&ops[2], &ops[1])?["eligibility"]["status"]
                        != "claimed"
                    {
                        return Ok(stopped(
                            STAGES[3],
                            &ops[3],
                            "pending",
                            "read_canonical_starter_eligibility",
                        ));
                    }
                }
                _ => {
                    return Ok(stopped(
                        STAGES[3],
                        &ops[3],
                        "blocked",
                        "inspect_canonical_starter_eligibility",
                    ));
                }
            }
        }
        finish(store, &ops[3])?;
    }
    if !confirmed(store, &ops[4])? {
        start(store, &ops[4])?;
        let result = client.create_task(&ops[2], &ops[1], &ops[4]);
        if let Some(stop) = owner_result(result, STAGES[4], &ops[4]) {
            return Ok(stop);
        }
        let current = client.current_owner(&ops[2], &ops[1])?;
        let task = current
            .task
            .ok_or("canonical task not yet visible; retain original operation")?;
        ensure_current_task(p, &task, &keys, &ops[4])?;
        if task.task_revision.0 != 1
            || task.task_status != GameTaskStatusV1::Suspended
            || task.latest_goal_revision.0 != 0
            || task.active_goal_revision.0 != 0
            || task.active_goal_reference.is_some()
            || task.executor_epoch.0 != 0
            || task.executor_id.is_some()
            || task.executor_binding.is_some()
        {
            return Err(
                "action_required: new canonical task was modified before initial confirmation"
                    .into(),
            );
        }
        store.put_original(SCOPE, PLAN, "task-baseline", &task)?;
        finish(store, &ops[4])?;
    }
    let baseline = store
        .read_original::<GameTaskControlV1>(SCOPE, PLAN, "task-baseline")?
        .ok_or("original task baseline missing")?;
    let current = client.current_owner(&ops[2], &ops[1])?;
    let task = current.task.ok_or("canonical task missing")?;
    ensure_current_task(p, &task, &keys, &ops[4])?;
    if task.task_id != baseline.task_id
        || task.branch_id != baseline.branch_id
        || task.reorg_epoch != baseline.reorg_epoch
        || task.owner_binding != baseline.owner_binding
        || task.owner_generation != baseline.owner_generation
    {
        return Err(
            "action_required: cold entry task authority changed; retain original plan".into(),
        );
    }
    if !confirmed(store, &ops[5])? {
        if task != baseline {
            return Err(
                "action_required: canonical task changed before original goal preparation".into(),
            );
        }
        start(store, &ops[5])?;
        match client.prepare_goal(&ops[2], &ops[1], &ops[5], p.goal.clone()) {
            Ok(_) => finish(store, &ops[5])?,
            Err(_) => {
                return Ok(stopped(
                    STAGES[5],
                    &ops[5],
                    "unknown",
                    "reconcile_original_goal_prepare",
                ));
            }
        }
    }
    for i in 6..9 {
        if confirmed(store, &ops[i])? {
            continue;
        }
        start(store, &ops[i])?;
        let result = match i {
            6 => client.publish_prepared_goal(&ops[2], &ops[1], &ops[6], &ops[5]),
            7 => client.authorize_executor(
                &ops[2],
                &ops[1],
                &ops[7],
                &keys.executor,
                p.delegation_generation.0,
                p.delegation_nonce.0,
            ),
            8 => client.bind_executor(
                &ops[2],
                &ops[1],
                &ops[8],
                &p.executor_id,
                &keys.executor,
                p.delegation_generation.0,
            ),
            _ => unreachable!(),
        };
        if let Some(stop) = owner_result(result, STAGES[i], &ops[i]) {
            return Ok(stop);
        }
        // Reads after writes must resolve canonical state before the next stage.
        let current = client.current_owner(&ops[2], &ops[1])?;
        let task = current.task.ok_or("canonical task unavailable")?;
        ensure_current_task(p, &task, &keys, &ops[4])?;
        if i == 6 {
            validate_goal(&client, p, ops, &keys, false)?;
        }
        finish(store, &ops[i])?;
    }
    start(store, &ops[9])?;
    let task = validate_goal(&client, p, ops, &keys, true)?;
    let config = json!({"authority_mode":"native_executor_v1","identity":p.entry.identity,"login_operation":ops[1],"executor_vault_directory":p.executor_vault_directory,
        "service_public_key":p.entry.trusted_world_service_key,"fence":GameTaskFenceV1::from_control(&task),"state_directory":p.executor_state_directory});
    let export_name = if ops[1] == immutable.operations[1] {
        "executor.json".to_owned()
    } else {
        format!("executor-{}.json", ops[1])
    };
    publish_executor_config(&p.executor_config_directory, &export_name, &config)?;
    finish(store, &ops[9])?;
    Ok(
        json!({"stage":"complete","state":"confirmed","executor_config":p.executor_config_directory.join(export_name),"next_action":"explicit_validated_runtime_profile_required","runtime_started":false}),
    )
}
// A completed legacy handoff keeps its exact immutable provider bytes. This
// compatibility read never rewrites a broker artifact or upgrades its mode.
pub(super) fn publish_executor_config(
    path: &std::path::Path,
    name: &str,
    config: &Value,
) -> Result<(), String> {
    let directory = private_storage::open_root(path)?;
    let bytes = serde_json::to_vec_pretty(config).map_err(|_| "executor config encoding failed")?;
    if private_storage::publish_immutable(&directory, name, &bytes)? {
        return Ok(());
    }
    let original = private_storage::read_private(&directory, name, 65536)?;
    if original == bytes {
        return Ok(());
    }
    let mut legacy = config.clone();
    let object = legacy
        .as_object_mut()
        .ok_or("invalid executor configuration")?;
    if object.remove("authority_mode") != Some(json!("native_executor_v1")) {
        return Err("invalid executor authority mode".into());
    }
    if original
        == serde_json::to_vec_pretty(&legacy).map_err(|_| "executor config encoding failed")?
    {
        return Ok(());
    }
    Err("action_required: original exported executor fence or authority mode changed; no automatic replacement".into())
}
// Participant settlement deliberately remains pending until a fresh canonical
// owner observation succeeds. Other pending/unknown results must retain the
// exact original and must never advance by consulting an unrelated owner view.
pub(crate) fn participant_result(
    progress: &crate::identity_client::ParticipantProgress,
    operation: &str,
    canonical_owner: impl FnOnce() -> Result<(), String>,
) -> Result<Option<Value>, String> {
    if progress.state != "confirmed"
        && !(progress.state == "pending" && progress.next_action == "verify_canonical_owner_view")
    {
        return Ok(Some(stopped(
            STAGES[2],
            operation,
            &progress.state,
            &progress.next_action,
        )));
    }
    canonical_owner()?;
    Ok(None)
}
fn owner_result(
    result: Result<crate::identity_client::EntryOwnerProgress, String>,
    stage: &str,
    operation: &str,
) -> Option<Value> {
    match result {
        Ok(p) if p.state == "confirmed" => None,
        Ok(p) => Some(stopped(stage, operation, &p.state, &p.next_action)),
        Err(_) => Some(stopped(
            stage,
            operation,
            "unknown",
            "query_original_owner_operation",
        )),
    }
}
fn validate_goal(
    client: &ParticipantClient,
    p: &ColdEntryPlan,
    ops: &[String],
    keys: &Keys,
    require_binding: bool,
) -> Result<GameTaskControlV1, String> {
    let current = client.current_owner(&ops[2], &ops[1])?;
    let task = current.task.ok_or("canonical task unavailable")?;
    ensure_current_task(p, &task, keys, &ops[4])?;
    let goal = client.read_current_goal(&ops[2], &ops[1])?;
    let active = goal
        .active_goal
        .ok_or("canonical private goal unavailable")?;
    let original = &active.owner_proof.request;
    if goal.control != task
        || original.client_operation_id != ops[5]
        || original.content != p.goal
        || original.task_id != task.task_id
        || original.owner_public_key != keys.owner
        || task.active_goal_revision != original.goal_revision
    {
        return Err("cold entry canonical goal differs from immutable private original".into());
    }
    if require_binding
        && (task.task_status != GameTaskStatusV1::Active
            || !task.executor_binding.as_ref().is_some_and(|b| {
                b.executor_id == p.executor_id
                    && b.executor_public_key == keys.executor
                    && b.delegation_generation == p.delegation_generation
                    && b.executor_epoch == task.executor_epoch
            })
            || task.executor_id.as_deref() != Some(&p.executor_id))
    {
        return Err("cold entry canonical executor differs from fixed independent delegate".into());
    }
    Ok(task)
}
