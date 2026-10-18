use super::*;
use oasis7_client_api::world_service as authority;
use oasis7_client_api::world_service::{ProjectionVersion, ReadWorldViewRequest, VIEW_PATH};
use oasis7_proto::viewer::{CanonicalAgentChatViewV1, CanonicalAgentOwnerFenceV1};
use serde::Deserialize;

pub(super) struct ChatInput<'a> {
    pub agent_id: &'a str,
    pub player_id: &'a str,
    pub message: &'a str,
    pub private_key_hex: &'a str,
    pub public_key_hex: Option<&'a str>,
    pub intent_tick: Option<u64>,
    pub intent_seq: Option<u64>,
}

#[derive(Deserialize)]
struct OwnerContext {
    request: ReadWorldViewRequest,
    fence: Option<CanonicalAgentOwnerFenceV1>,
    expected: ExpectedOwner,
    signing_domain: String,
    signing_bytes_hex: String,
}
#[derive(Deserialize)]
struct ExpectedOwner {
    agent_id: String,
    player_id: String,
    public_key: String,
    world_id: String,
    branch_id: String,
    reorg_epoch: u64,
}

pub(super) struct ChatOutcome {
    pub responses: Vec<CollectedResponse>,
    pub artifact: Option<std::path::PathBuf>,
}

pub(super) fn finish_report(
    conn: &mut ViewerConnection,
    mut outcome: ChatOutcome,
    with_snapshot: bool,
    timeout: Duration,
    output: &mut impl Write,
) -> Result<(), String> {
    let result = (|| {
        maybe_request_snapshot(conn, with_snapshot, &mut outcome.responses, timeout)?;
        let bytes = serde_json::to_vec_pretty(&command_output(&conn.hello_ack, &outcome.responses))
            .map_err(|e| format!("serialize output failed: {e}"))?;
        output
            .write_all(&bytes)
            .map_err(|e| format!("write output failed: {e}"))?;
        output
            .write_all(b"\n")
            .map_err(|e| format!("write output failed: {e}"))?;
        output
            .flush()
            .map_err(|e| format!("flush output failed: {e}"))?;
        Ok(())
    })();
    if let Err(error) = result {
        return Err(match outcome.artifact {
            Some(path) => retained_error(&path, &format!("canonical chat committed; {error}")),
            None => error,
        });
    }
    if let Some(path) = outcome.artifact {
        pending::complete(&path).map_err(|error| {
            if path.exists() {
                retained_error(&path, &format!("canonical chat committed; {error}"))
            } else {
                format!("canonical chat committed; {error}; pending artifact already removed")
            }
        })?;
    }
    Ok(())
}

fn retained_error(path: &std::path::Path, error: &str) -> String {
    format!(
        "{error}; original signed request retained at {}. Retry with chat-retry --signed-request-file <that path>; do not create a new chat nonce",
        path.display()
    )
}

pub(super) fn exchange(
    conn: &mut ViewerConnection,
    timeout: Duration,
    input: ChatInput<'_>,
) -> Result<ChatOutcome, String> {
    let canonical = conn.hello_ack["capabilities"]
        .as_array()
        .is_some_and(|values| values.iter().any(|v| v == "canonical_agent_chat_v1"));
    if !canonical {
        let request = build_signed_agent_chat_request(
            input.agent_id,
            input.player_id,
            input.message,
            input.private_key_hex,
            input.public_key_hex,
            input.intent_tick,
            input.intent_seq,
        )?;
        conn.send(&ViewerRequest::AgentChat { request })?;
        return conn
            .collect_until(
                timeout,
                terminal_agent_chat,
                "waiting for agent_chat reply/error",
            )
            .map(|responses| ChatOutcome {
                responses,
                artifact: None,
            });
    }
    let deadline = Instant::now() + timeout;
    let (chat, responses, artifact) = prepare_canonical(conn, deadline, input)?;
    submit_original(conn, deadline, chat, responses, &artifact)
}

fn prepare_canonical(
    conn: &mut ViewerConnection,
    deadline: Instant,
    input: ChatInput<'_>,
) -> Result<(AgentChatRequest, Vec<CollectedResponse>, std::path::PathBuf), String> {
    let key = owner_public_key(input.private_key_hex, input.public_key_hex)?;
    conn.send(&ViewerRequest::AuthoritativeRecovery {
        command: AuthoritativeRecoveryCommand::ReconnectSync {
            request: AuthoritativeReconnectSyncRequest {
                player_id: input.player_id.into(),
                session_pubkey: Some(key.clone()),
                last_known_log_cursor: None,
                expected_reorg_epoch: None,
            },
        },
    })?;
    let mut responses = conn.collect_until(
        remaining(deadline)?,
        terminal_recovery,
        "restoring existing owner session (register-session must already be authorized)",
    )?;
    let epoch = match &responses
        .last()
        .ok_or("missing reconnect response")?
        .response
    {
        ViewerResponse::AuthoritativeRecoveryAck { ack }
            if ack.status == oasis7_proto::viewer::AuthoritativeRecoveryStatus::CatchUpReady
                && ack.player_id.as_deref() == Some(input.player_id)
                && ack.session_pubkey.as_deref() == Some(key.as_str())
                && ack
                    .agent_id
                    .as_deref()
                    .is_none_or(|agent| agent == input.agent_id) =>
        {
            ack.reorg_epoch
        }
        _ => return Err("existing owner session recovery rejected or mismatched".into()),
    };
    conn.send(&ViewerRequest::RequestSnapshot)?;
    let snapshot = conn.collect_until(
        remaining(deadline)?,
        terminal_snapshot,
        "priming verified canonical view",
    )?;
    let tick = match &snapshot.last().ok_or("missing snapshot")?.response {
        ViewerResponse::Snapshot { snapshot } => snapshot.time,
        _ => return Err("canonical snapshot unavailable".into()),
    };
    responses.extend(snapshot);
    conn.send(&ViewerRequest::RequestCanonicalAgentOwnerReadContext {
        agent_id: input.agent_id.into(),
    })?;
    let contexts = conn.collect_until(
        remaining(deadline)?,
        |v| matches!(v, ViewerResponse::CanonicalAgentOwnerReadContext { .. }),
        "waiting for owner read context",
    )?;
    let context = match &contexts.last().ok_or("missing owner context")?.response {
        ViewerResponse::CanonicalAgentOwnerReadContext { context } => {
            serde_json::from_value::<OwnerContext>(context.clone()).map_err(|e| e.to_string())?
        }
        _ => return Err("canonical owner context rejected".into()),
    };
    validate_context(&context, &input, &key, epoch, &conn.hello_ack)?;
    responses.extend(contexts);
    let signed =
        authority::sign_read_request(VIEW_PATH, context.request.clone(), input.private_key_hex)?;
    let original = serde_json::to_value(&signed).map_err(|e| e.to_string())?;
    let digest = authority::request_digest(VIEW_PATH, &signed)?;
    conn.send(&ViewerRequest::CanonicalAgentOwnerRead {
        request: original.clone(),
    })?;
    let reads = conn.collect_until(
        remaining(deadline)?,
        |v| matches!(v, ViewerResponse::CanonicalAgentOwnerView { .. }),
        "waiting for authenticated owner view",
    )?;
    let view = match &reads.last().ok_or("missing owner view")?.response {
        ViewerResponse::CanonicalAgentOwnerView {
            version,
            request_digest,
            request,
            view,
        } => {
            if request != &original || request_digest != &digest {
                return Err("owner response does not match original signed read".into());
            }
            let version: ProjectionVersion =
                serde_json::from_value(version.clone()).map_err(|e| e.to_string())?;
            validate_view(&context, &version, view)?;
            view.clone()
        }
        _ => return Err("authenticated owner read rejected".into()),
    };
    responses.extend(reads);
    let nonce = next_u64_id();
    let mut chat = AgentChatRequest {
        agent_id: input.agent_id.into(),
        message: input.message.into(),
        player_id: Some(input.player_id.into()),
        public_key: Some(key.clone()),
        auth: None,
        intent_tick: Some(input.intent_tick.unwrap_or(tick)),
        intent_seq: Some(input.intent_seq.unwrap_or(nonce)),
        world_id: Some(view.world_id),
        reorg_epoch: Some(view.reorg_epoch),
        authority_scope: Some(view.authority_scope),
        replaces_intent_id: view.current_intent_id,
        canonical_authority: Some(view.canonical_authority),
    };
    if chat.intent_seq == Some(0) {
        return Err("canonical intent sequence must be positive".into());
    }
    chat.auth = Some(sign_agent_chat_auth_proof(
        &chat,
        nonce,
        &key,
        input.private_key_hex,
    )?);
    let artifact = pending::save(&chat, &context.request.world, &conn.endpoint)?;
    Ok((chat, responses, artifact))
}

fn owner_public_key(private_key_hex: &str, public_key_hex: Option<&str>) -> Result<String, String> {
    let supplied =
        oasis7_pure_api_client_support::resolve_public_key_hex(private_key_hex, public_key_hex)?;
    let bytes = hex::decode(supplied.trim()).map_err(|_| "owner public key must be valid hex")?;
    if bytes.len() != 32 {
        return Err("owner public key must be exactly 32 bytes".into());
    }
    let key = hex::encode(bytes);
    if key != oasis7_pure_api_client_support::derive_public_key_hex(private_key_hex)? {
        return Err("owner public key does not match local private key".into());
    }
    Ok(key)
}

fn submit_original(
    conn: &mut ViewerConnection,
    deadline: Instant,
    chat: AgentChatRequest,
    mut responses: Vec<CollectedResponse>,
    artifact: &std::path::Path,
) -> Result<ChatOutcome, String> {
    let result = wait_committed(conn, deadline, chat, &mut responses);
    match result {
        Ok(()) => Ok(ChatOutcome {
            responses,
            artifact: Some(artifact.to_path_buf()),
        }),
        Err(error) => Err(retained_error(artifact, &error)),
    }
}
fn wait_committed(
    conn: &mut ViewerConnection,
    deadline: Instant,
    chat: AgentChatRequest,
    responses: &mut Vec<CollectedResponse>,
) -> Result<(), String> {
    let nonce = chat
        .auth
        .as_ref()
        .ok_or("original owner proof missing")?
        .nonce;
    let request = ViewerRequest::AgentChat {
        request: chat.clone(),
    };
    conn.send(&request)?;
    let mut retry_at = Instant::now() + Duration::from_millis(1500);
    loop {
        let wait = remaining(deadline)?.min(retry_at.saturating_duration_since(Instant::now()));
        if wait.is_zero() {
            // Retry the exact original signed payload. The service resolves its original correlation.
            conn.send(&request)?;
            retry_at = Instant::now() + Duration::from_millis(1500);
            continue;
        }
        match conn.read_response(wait)? {
            ReadResponseOutcome::Response(response) => {
                match &response.response {
                    ViewerResponse::AgentChatAck { ack } if correlated(ack, &chat, nonce) => {
                        if matches!(ack.status.as_deref(), Some("accepted" | "blocked"))
                            && ack.intent_id.as_ref().is_some_and(|id| !id.is_empty())
                            && ack.accepted_event_seq.is_some() {
                            responses.push(response);
                            return Ok(());
                        }
                        if ack.status.as_deref() != Some("pending") {
                            return Err("canonical ACK lacks a committed receipt".into());
                        }
                    },
                    ViewerResponse::AgentChatError { error }
                        if matching_error(error, &chat)
                            && error.code == "world_service_outcome_unknown" => {},
                    ViewerResponse::AgentChatError { error }
                        if matching_error(error, &chat) => return Err(format!("canonical chat rejected: {}: {}", error.code, error.message)),
                    ViewerResponse::Error { .. } => return Err("canonical chat transport rejected".into()),
                    _ => {},
                }
                responses.push(response);
            },
            ReadResponseOutcome::TimedOut => {},
            ReadResponseOutcome::Closed => return Err("canonical chat outcome unknown: connection closed; retain original request identity".into()),
        }
    }
}
fn remaining(deadline: Instant) -> Result<Duration, String> {
    let duration = deadline.saturating_duration_since(Instant::now());
    if duration.is_zero() {
        Err("canonical chat outcome not committed before deadline".into())
    } else {
        Ok(duration)
    }
}
fn validate_context(
    context: &OwnerContext,
    input: &ChatInput<'_>,
    key: &str,
    epoch: u64,
    hello: &Value,
) -> Result<(), String> {
    context.request.validate().map_err(|e| e.to_string())?;
    let expected = &context.expected;
    let minimum = context
        .request
        .min_commit
        .as_ref()
        .ok_or("owner context needs verified minimum commit")?;
    if context.signing_domain != VIEW_PATH
        || context.signing_bytes_hex
            != hex::encode(authority::signing_bytes(VIEW_PATH, &context.request)?)
        || expected.agent_id != input.agent_id
        || expected.player_id != input.player_id
        || expected.public_key != key
        || expected.reorg_epoch != epoch
        || hello["world_id"].as_str() != Some(expected.world_id.as_str())
        || context.request.world.world_id != expected.world_id
        || context.request.scope_id != format!("agent:{}", input.agent_id)
        || context.request.fixed_commit.is_some()
        || context.request.deadline_unix_ms.is_some()
        || minimum.binding.branch_id != expected.branch_id
        || minimum.binding.reorg_generation != epoch
    {
        return Err("owner signing context identity/domain mismatch".into());
    }
    Ok(())
}
fn validate_view(
    context: &OwnerContext,
    version: &ProjectionVersion,
    view: &CanonicalAgentChatViewV1,
) -> Result<(), String> {
    version.commit.validate().map_err(|e| e.to_string())?;
    let minimum = context
        .request
        .min_commit
        .as_ref()
        .ok_or("missing minimum commit")?;
    let expected = &context.expected;
    if version.commit.world != context.request.world
        || version.commit.binding != minimum.binding
        || !version
            .commit
            .satisfies_minimum(minimum)
            .map_err(|e| e.to_string())?
        || version.visibility_scope != context.request.scope_id
        || version.projection_revision.is_empty()
        || view.agent_id != expected.agent_id
        || view.player_id != expected.player_id
        || view.public_key != expected.public_key
        || view.world_id != expected.world_id
        || view.reorg_epoch != expected.reorg_epoch
        || view.authority_scope != "player_agent_chat"
        || view.canonical_authority.branch_id != expected.branch_id
        || view.canonical_authority.agent_identity_generation == 0
        || context
            .fence
            .as_ref()
            .is_some_and(|fence| fence != &CanonicalAgentOwnerFenceV1::from(view))
    {
        return Err("owner response commit/authority fence mismatch".into());
    }
    Ok(())
}
fn correlated(
    ack: &oasis7_proto::viewer::AgentChatAck<u64>,
    chat: &AgentChatRequest,
    nonce: u64,
) -> bool {
    ack.agent_id == chat.agent_id
        && ack.player_id == chat.player_id
        && ack.intent_tick == chat.intent_tick
        && ack.intent_seq == chat.intent_seq
        && ack.auth_nonce == Some(nonce)
}

#[cfg(test)]
#[path = "canonical_chat_tests.rs"]
mod tests;

fn matching_error(error: &oasis7_proto::viewer::AgentChatError, chat: &AgentChatRequest) -> bool {
    // One outstanding chat on this connection; canonical handler errors carry the request agent.
    error.agent_id.as_deref() == Some(chat.agent_id.as_str())
}

pub(super) fn retry(
    conn: &mut ViewerConnection,
    timeout: Duration,
    path: &std::path::Path,
) -> Result<ChatOutcome, String> {
    if !conn.hello_ack["capabilities"]
        .as_array()
        .is_some_and(|values| values.iter().any(|v| v == "canonical_agent_chat_v1"))
    {
        return Err("chat-retry requires canonical Agent chat capability".into());
    }
    let saved = pending::load(path)?;
    saved.world.validate().map_err(|error| error.to_string())?;
    if saved.version != 1 || saved.endpoint != conn.endpoint {
        return Err("original request is pinned to a different Viewer endpoint/version".into());
    }
    let chat = saved.request;
    let proof = chat
        .auth
        .as_ref()
        .ok_or("original signed owner proof missing")?;
    let verified = oasis7_proto::player_auth::verify_agent_chat_auth_proof(&chat, proof)?;
    if chat.agent_id.is_empty()
        || chat.world_id.is_none()
        || verified.nonce == 0
        || chat.intent_seq.is_none_or(|v| v == 0)
        || chat.intent_tick.is_none()
        || chat.authority_scope.as_deref() != Some("player_agent_chat")
        || chat
            .canonical_authority
            .as_ref()
            .is_none_or(|v| v.branch_id.is_empty() || v.agent_identity_generation == 0)
        || chat.world_id.as_deref() != conn.hello_ack["world_id"].as_str()
        || chat.reorg_epoch.is_none()
    {
        return Err("original canonical chat identity is incomplete or foreign".into());
    }
    let deadline = Instant::now() + timeout;
    conn.send(&ViewerRequest::AuthoritativeRecovery {
        command: AuthoritativeRecoveryCommand::ReconnectSync {
            request: AuthoritativeReconnectSyncRequest {
                player_id: verified.player_id.clone(),
                session_pubkey: Some(verified.public_key.clone()),
                last_known_log_cursor: None,
                expected_reorg_epoch: None,
            },
        },
    })?;
    let mut responses = conn.collect_until(
        remaining(deadline)?,
        terminal_recovery,
        "restoring original owner session",
    )?;
    match &responses.last().ok_or("missing reconnect response")?.response {
        ViewerResponse::AuthoritativeRecoveryAck{ack} if ack.status == oasis7_proto::viewer::AuthoritativeRecoveryStatus::CatchUpReady
            && ack.player_id.as_deref() == Some(verified.player_id.as_str())
            && ack.session_pubkey.as_deref() == Some(verified.public_key.as_str())
            && ack.reorg_epoch == chat.reorg_epoch.unwrap()
            && ack.agent_id.as_deref().is_none_or(|agent| agent == chat.agent_id) => {},
        _ => return Err("original owner session/fence rejected; retained artifact must not be re-signed automatically".into()),
    }
    // Verify the original full world pin without signing a new read or changing authority.
    conn.send(&ViewerRequest::RequestSnapshot)?;
    responses.extend(conn.collect_until(
        remaining(deadline)?,
        terminal_snapshot,
        "verifying original world",
    )?);
    conn.send(&ViewerRequest::RequestCanonicalAgentOwnerReadContext {
        agent_id: chat.agent_id.clone(),
    })?;
    let contexts = conn.collect_until(
        remaining(deadline)?,
        |response| {
            matches!(
                response,
                ViewerResponse::CanonicalAgentOwnerReadContext { .. }
            )
        },
        "checking original world identity",
    )?;
    let context = match &contexts.last().ok_or("owner context missing")?.response {
        ViewerResponse::CanonicalAgentOwnerReadContext { context } => {
            serde_json::from_value::<OwnerContext>(context.clone()).map_err(|e| e.to_string())?
        }
        _ => return Err("original world identity context rejected".into()),
    };
    let input = ChatInput {
        agent_id: &chat.agent_id,
        player_id: &verified.player_id,
        message: &chat.message,
        private_key_hex: "",
        public_key_hex: None,
        intent_tick: chat.intent_tick,
        intent_seq: chat.intent_seq,
    };
    validate_context(
        &context,
        &input,
        &verified.public_key,
        chat.reorg_epoch.unwrap(),
        &conn.hello_ack,
    )?;
    if context.request.world != saved.world {
        return Err("original genesis/world identity changed; request retained".into());
    }
    responses.extend(contexts);
    submit_original(conn, deadline, chat, responses, path)
}

#[path = "canonical_chat_pending.rs"]
mod pending;
