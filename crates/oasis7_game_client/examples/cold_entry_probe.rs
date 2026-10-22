//! Fixture-only actual SDK probe. Never starts a model Runtime or creates roles.
use oasis7_agent_api::{
    game::GameTaskStatusV1,
    world_action::{Action, GeoPos},
};
use oasis7_client_api::world_service::{
    IntentOutcome, IntentResponse, SubmitIntentRequest, WorldServiceErrorKind,
};
use oasis7_client_api::world_service_wire::WorldServicePayloadV1;
use oasis7_client_storage::identity_store::OwnerControlStore;
use oasis7_game_client::{
    device_client::Client,
    identity_client::{ParticipantClient, ParticipantClientConfig},
    operation_store::ClientStateKind,
};
use serde_json::{Value, json};
use std::{
    io::Read,
    path::Path,
    time::{Duration, Instant},
};

fn check(value: bool, message: &str) -> Result<(), String> {
    if value { Ok(()) } else { Err(message.into()) }
}
fn run() -> Result<Value, String> {
    // Shared new-call budget includes owner prework and both original intents.
    // Existing SDK polling reserves eight seconds; a bounded I/O already in
    // flight may add about five seconds, staying below the 60-second watchdog.
    let deadline = Instant::now() + Duration::from_secs(45);
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
    require_time(deadline, Duration::ZERO)?;
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
    require_time(deadline, Duration::ZERO)?;
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
    require_time(deadline, Duration::ZERO)?;
    let client = Client::open_authenticated(Path::new(&args[1]), &password)?;
    let fence = client.task_fence()?;
    check(
        fence.task_id == task.task_id && fence.agent_id == current.receipt.agent_id,
        "SDK task binding mismatch",
    )?;
    let operation = "cold-entry-live-smoke-one-move-v1";
    let original = client.cached_intent(ClientStateKind::Action, operation)?;
    let result = if let Some(original) = original {
        recover_original(&client, &original, None, deadline, "action")?
    } else {
        let cached_prefix = client.cached_intent(ClientStateKind::Prefix, operation)?;
        let (prefix, first) = if let Some(original) = cached_prefix {
            (original, None)
        } else {
            require_time(deadline, Duration::from_secs(8))?;
            let original = client.prefix(operation)?;
            require_time(deadline, Duration::from_secs(8))?;
            let response = client.submit_and_poll(&original).ok();
            (original, response)
        };
        recover_original(&client, &prefix, first, deadline, "context_prefix")?;
        let context = client.captured(operation)?;
        // submit_action performs one more bounded context read before its
        // single send and existing eight-second polling window.
        require_time(deadline, Duration::from_secs(13))?;
        check(
            context.prepared.prepare_proof.request.fence.task_id == task.task_id,
            "private context resource mismatch",
        )?;
        let submitted = client.submit_action(
            operation,
            Action::MoveAgent {
                agent_id: task.agent_id.clone(),
                to: GeoPos::new(1, 0, 0),
            },
        );
        let original = client
            .cached_intent(ClientStateKind::Action, operation)?
            .ok_or("original action unavailable; retain original operation")?;
        recover_original(&client, &original, submitted.ok(), deadline, "action")?
    };
    check(
        matches!(result.outcome, IntentOutcome::Committed { .. }),
        "actual action not canonically committed",
    )?;
    require_time(deadline, Duration::ZERO)?;
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

#[derive(Debug, PartialEq, Eq)]
enum ProbeOutcome {
    Committed,
    Recover(&'static str),
    Terminal(&'static str),
}
fn classify_outcome<T>(outcome: &IntentOutcome<T>) -> ProbeOutcome {
    match outcome {
        IntentOutcome::Committed { .. } => ProbeOutcome::Committed,
        IntentOutcome::Received { .. } => ProbeOutcome::Recover("received"),
        IntentOutcome::Pending => ProbeOutcome::Recover("pending"),
        IntentOutcome::Unknown => ProbeOutcome::Recover("unknown"),
        IntentOutcome::Expired => ProbeOutcome::Terminal("expired"),
        IntentOutcome::HistoryUnavailable => ProbeOutcome::Terminal("history_unavailable"),
        IntentOutcome::Rejected { reason } => ProbeOutcome::Terminal(match reason {
            WorldServiceErrorKind::UnsupportedContract => "rejected:unsupported_contract",
            WorldServiceErrorKind::UnsupportedCapability => "rejected:unsupported_capability",
            WorldServiceErrorKind::Unauthorized => "rejected:unauthorized",
            WorldServiceErrorKind::Conflict => "rejected:conflict",
            WorldServiceErrorKind::Unavailable => "rejected:unavailable",
            WorldServiceErrorKind::AssuranceUnavailable => "rejected:assurance_unavailable",
            WorldServiceErrorKind::ReadNotCaughtUp => "rejected:read_not_caught_up",
            WorldServiceErrorKind::HistoryUnavailable => "rejected:history_unavailable",
            WorldServiceErrorKind::CursorExpired => "rejected:cursor_expired",
            WorldServiceErrorKind::ResyncRequired => "rejected:resync_required",
        }),
    }
}
fn require_time(deadline: Instant, reserve: Duration) -> Result<(), String> {
    if deadline.saturating_duration_since(Instant::now()) <= reserve {
        Err("probe recovery deadline exhausted; retain original operation".into())
    } else {
        Ok(())
    }
}
fn recover_original(
    client: &Client,
    original: &SubmitIntentRequest<WorldServicePayloadV1>,
    mut response: Option<IntentResponse<Value>>,
    deadline: Instant,
    phase: &'static str,
) -> Result<IntentResponse<Value>, String> {
    let mut last = "unobserved";
    loop {
        if let Some(observed) = response.take() {
            match classify_outcome(&observed.outcome) {
                ProbeOutcome::Committed => return Ok(observed),
                ProbeOutcome::Terminal(category) => {
                    return Err(format!("{phase} {category}; retain original operation"));
                }
                ProbeOutcome::Recover(category) => last = category,
            }
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "{phase} recovery deadline exhausted outcome={last}; retain original operation"
            ));
        }
        // Only fixed categories escape this probe; response bodies/proofs and
        // transport diagnostics remain private. Never resubmit or re-sign here.
        response = Some(client.lookup_typed(original).map_err(|error| {
            let category = match error {
                oasis7_game_client::WorldServiceClientError::Configuration(_) => "configuration",
                oasis7_game_client::WorldServiceClientError::Transport(_) => "transport",
                oasis7_game_client::WorldServiceClientError::Assurance(_) => "assurance",
                oasis7_game_client::WorldServiceClientError::Rejected { .. } => "http_rejected",
                oasis7_game_client::WorldServiceClientError::Cooldown { .. } => "cooldown",
            };
            format!("{phase} query {category}; retain original operation")
        })?);
        if response.as_ref().is_some_and(|value| {
            matches!(classify_outcome(&value.outcome), ProbeOutcome::Recover(_))
        }) {
            std::thread::sleep(
                Duration::from_millis(500).min(deadline.saturating_duration_since(Instant::now())),
            );
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pending_received_unknown_require_original_read_recovery() {
        for outcome in [
            IntentOutcome::<()>::Pending,
            IntentOutcome::Unknown,
            IntentOutcome::Received {
                durability: oasis7_client_api::world_service::AdmissionDurability::Volatile,
            },
            IntentOutcome::Received {
                durability: oasis7_client_api::world_service::AdmissionDurability::Durable,
            },
        ] {
            assert!(matches!(
                classify_outcome(&outcome),
                ProbeOutcome::Recover(_)
            ));
        }
    }
    #[test]
    fn shared_deadline_is_not_renewed_for_later_phase() {
        let deadline = Instant::now();
        assert!(require_time(deadline, Duration::ZERO).is_err());
        assert!(require_time(deadline + Duration::from_secs(9), Duration::from_secs(13)).is_err());
    }
    #[test]
    fn terminal_outcomes_keep_safe_distinct_categories() {
        assert_eq!(
            classify_outcome(&IntentOutcome::<()>::Expired),
            ProbeOutcome::Terminal("expired")
        );
        assert_eq!(
            classify_outcome(&IntentOutcome::<()>::HistoryUnavailable),
            ProbeOutcome::Terminal("history_unavailable")
        );
        assert_eq!(
            classify_outcome(&IntentOutcome::<()>::Rejected {
                reason: oasis7_client_api::world_service::WorldServiceErrorKind::Conflict
            }),
            ProbeOutcome::Terminal("rejected:conflict")
        );
    }
}
