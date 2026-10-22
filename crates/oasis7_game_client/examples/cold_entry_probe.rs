//! Fixture-only actual SDK probe. Never starts a model Runtime or creates roles.
use oasis7_agent_api::{
    game::GameTaskStatusV1,
    world_action::{Action, GeoPos},
};
use oasis7_client_api::world_service::IntentOutcome;
use oasis7_client_storage::identity_store::OwnerControlStore;
use oasis7_game_client::{
    device_client::Client,
    identity_client::{ParticipantClient, ParticipantClientConfig},
    operation_store::ClientStateKind,
};
use serde_json::{Value, json};
use std::{io::Read, path::Path};

fn check(value: bool, message: &str) -> Result<(), String> {
    if value { Ok(()) } else { Err(message.into()) }
}
fn run() -> Result<Value, String> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.len() != 2 {
        return Err("cold_entry_probe <orchestration-directory> <executor.json>".into());
    }
    let mut password = zeroize::Zeroizing::new(Vec::new());
    std::io::stdin()
        .take(1025)
        .read_to_end(&mut password)
        .map_err(|_| "private unlock unavailable")?;
    check(
        (12..=1024).contains(&password.len()),
        "private unlock required",
    )?;
    let store = OwnerControlStore::open(Path::new(&args[0]), &password)?;
    let immutable: Value = store
        .read_original("trusted-cold-entry/v1", "immutable-plan", "plan")?
        .ok_or("original plan missing")?;
    let ops = immutable["operations"]
        .as_array()
        .ok_or("original operations missing")?;
    check(ops.len() == 10, "unexpected operation manifest")?;
    let op = |index: usize| ops[index].as_str().ok_or("invalid operation identity");
    let config: ParticipantClientConfig =
        serde_json::from_value(immutable["plan"]["entry"].clone())
            .map_err(|_| "invalid original participant config")?;
    let participant = ParticipantClient::new(config, &password)?;
    let current = participant.current_owner(op(2)?, op(1)?)?;
    let task = current.task.ok_or("canonical task unavailable")?;
    check(
        current.receipt.client_operation_id == op(2)?,
        "participant original mismatch",
    )?;
    check(
        task.created_operation_id == op(4)?,
        "task original mismatch",
    )?;
    check(
        task.task_status == GameTaskStatusV1::Active,
        "task not active",
    )?;
    let goal = participant.read_current_goal(op(2)?, op(1)?)?;
    let active = goal.active_goal.ok_or("canonical private goal missing")?;
    check(
        active.owner_proof.request.client_operation_id == op(5)?,
        "goal original mismatch",
    )?;
    check(
        serde_json::to_value(&active.owner_proof.request.content)
            .map_err(|_| "goal encoding failed")?
            == immutable["plan"]["goal"],
        "goal content mismatch",
    )?;
    let bound = participant.bind_executor(
        op(2)?,
        op(1)?,
        op(8)?,
        immutable["plan"]["executor_id"]
            .as_str()
            .ok_or("executor id missing")?,
        &task
            .executor_binding
            .as_ref()
            .ok_or("binding missing")?
            .executor_public_key,
        task.executor_binding
            .as_ref()
            .ok_or("binding missing")?
            .delegation_generation
            .0,
    )?;
    check(
        bound.state == "confirmed",
        "bind original is not canonically confirmed",
    )?;
    let client = Client::open_authenticated(Path::new(&args[1]), &password)?;
    let fence = client.task_fence()?;
    check(
        fence.task_id == task.task_id && fence.agent_id == current.receipt.agent_id,
        "SDK task binding mismatch",
    )?;
    let operation = "cold-entry-live-smoke-one-move-v1";
    let original = client.cached_intent(ClientStateKind::Action, operation)?;
    let result = if let Some(original) = original {
        client.lookup(&original)?
    } else {
        let prefix = client.prefix(operation)?;
        let prefix_result = client.submit_and_poll(&prefix)?;
        check(
            matches!(prefix_result.outcome, IntentOutcome::Committed { .. }),
            "context prefix not committed",
        )?;
        let context = client.captured(operation)?;
        check(
            context.prepared.prepare_proof.request.fence.task_id == task.task_id,
            "private context resource mismatch",
        )?;
        client.submit_action(
            operation,
            Action::MoveAgent {
                agent_id: task.agent_id.clone(),
                to: GeoPos::new(1, 0, 0),
            },
        )?
    };
    check(
        matches!(result.outcome, IntentOutcome::Committed { .. }),
        "actual action not canonically committed",
    )?;
    let after = client.public_view()?;
    check(
        after
            .projection()
            .game_task_control
            .as_ref()
            .is_some_and(|value| value.task_id == task.task_id),
        "post-action task not authenticated",
    )?;
    let position = &after.projection().state["agents"][&task.agent_id]["state"]["pos"];
    check(
        *position == json!({"x_cm":1,"y_cm":0,"z_cm":0}),
        "committed move did not update authenticated world position",
    )?;
    Ok(
        json!({"canonical_verified":true,"agents":1,"tasks":1,"goals":1,"active_executors":1,
        "sdk_task_context_verified":true,"action_committed":true,"resource_scope":"new_participant",
        "participant_operation_id":op(2)?,"executor_operation_id":op(8)?,"task_id":task.task_id,"agent_id":task.agent_id,
        "action_correlation":result.correlation,"world_commit":after.version().commit}),
    )
}
fn main() {
    match run() {
        Ok(value) => println!("{value}"),
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
}
