//! Trusted read-only original-cache diagnostics. Never prepares or submits.
#[cfg(unix)]
use std::io::Read;
#[cfg(unix)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use oasis7_game_client::operation_store::{ClientStateKind, ClientStateV1};
    let mut args = std::env::args().skip(1);
    let path = args.next().ok_or("executor config required")?;
    let operation = args.next().ok_or("original context required")?;
    let mut unlock = zeroize::Zeroizing::new(Vec::new());
    std::io::stdin().take(1025).read_to_end(&mut unlock)?;
    if unlock.len() > 1024 {
        return Err("unlock too large".into());
    }
    let client = oasis7_game_client::device_client::Client::open_authenticated(
        std::path::Path::new(&path),
        &unlock,
    )?;
    let prepare = client
        .state
        .read_client_state(ClientStateKind::Prepare, &operation)?;
    let (present, signature_valid, operation_matches, signer_matches, digest) = match &prepare {
        Some(ClientStateV1::ContextPrepare(proof)) => (
            true,
            oasis7_client_api::world_service::task_context::verify_prepare(proof).is_ok(),
            proof.request.client_operation_id == operation,
            proof.subject_public_key == client.public_key,
            Some(oasis7_client_api::world_service::request_digest(
                oasis7_client_api::world_service::task_context::TASK_CONTEXT_PREPARE_PATH,
                &proof,
            )?),
        ),
        _ => (false, false, false, false, None),
    };
    let snapshot = if let Some(ClientStateV1::ContextPrepare(original)) = &prepare {
        use oasis7_client_api::world_service::task_context::{
            TASK_SNAPSHOT_PATH, TaskGoalSnapshotRequestV1,
        };
        let config: oasis7_game_client::device_client::ExecutorDeviceConfig =
            serde_json::from_slice(&std::fs::read(&path)?)?;
        let vault = oasis7_client_storage::identity_store::ExecutorControlStore::open(
            &config.executor_vault_directory,
            &unlock,
        )?;
        let private = zeroize::Zeroizing::new(hex::encode(vault.executor_seed()));
        match oasis7_client_api::world_service::sign_read_request(
            TASK_SNAPSHOT_PATH,
            TaskGoalSnapshotRequestV1 {
                schema_revision:
                    oasis7_agent_api::game::TaskContextAuthorityModeV1::schema_revision(
                        original.request.authority_mode,
                    ),
                authority_mode: original.request.authority_mode,
                fence: original.request.fence.clone(),
            },
            &private,
        ) {
            Ok(proof) => match client.remote.read_task_goal_snapshot(&proof) {
                Ok(snapshot) => {
                    serde_json::json!({"category":"verified_snapshot","read_commit":snapshot.read_commit,
                    "goal_ref":snapshot.active_goal.goal_ref,"content_digest":blake3::hash(&serde_json::to_vec(&snapshot.active_goal.owner_proof.request.content)?).to_hex().to_string()})
                }
                Err(error) => {
                    use oasis7_game_client::WorldServiceClientError as E;
                    let (category, status) = match error {
                        E::Configuration(_) => ("configuration", None),
                        E::Transport(_) => ("transport", None),
                        E::Assurance(_) => ("assurance", None),
                        E::Rejected { status, .. } => ("rejected", Some(status)),
                        E::Cooldown { status, .. } => ("cooldown", Some(status)),
                    };
                    serde_json::json!({"category":category,"status":status})
                }
            },
            Err(_) => serde_json::json!({"category":"snapshot_signing_rejected"}),
        }
    } else {
        serde_json::json!({"category":"original_prepare_missing"})
    };
    let current_fence = match client.task_fence() {
        Ok(_) => "verified_current",
        Err(message) if message.contains("executor resource fence changed") => {
            "resource_fence_changed"
        }
        Err(_) => "signed_task_read_unavailable",
    };
    println!(
        "{}",
        serde_json::json!({
            "readonly_goal_snapshot":snapshot,"prepare_present":present,"prepare_signature_valid":signature_valid,
            "operation_matches":operation_matches,"signer_matches":signer_matches,
            "prepare_request_digest":digest,"current_fence":current_fence,
            "prefix_present":client.cached_intent(ClientStateKind::Prefix,&operation)?.is_some(),
            "action_present":client.cached_intent(ClientStateKind::Action,&operation)?.is_some(),
            "new_prepare_or_submit":false,
        })
    );
    Ok(())
}

#[cfg(not(unix))]
fn main() {
    eprintln!("unix_required");
    std::process::exit(1);
}
