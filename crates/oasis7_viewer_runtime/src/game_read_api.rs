//! Opt-in signed-read bootstrap for Game API. No pairing, token issuance or
//! writes are exposed until the corresponding Runtime contracts exist.
use oasis7_world_service::{
    SignedReadRequest, VIEW_PATH, authority,
    client::{RemoteWorldServiceClient, WorldServiceClientError},
};
use oasis7_agent_api::game::{
    API_REVISION, MAX_ACTIVE_READS, MAX_READ_HTTP_HEADER_BYTES, MAX_READ_PROOF_HEADER_BYTES,
    MAX_READ_RESPONSE_BYTES, READ_OPENAPI_PATH, READ_SCHEMA_REVISION, parse_request,
    read_bootstrap_limits, read_bootstrap_openapi,
};
use oasis7_client_api::world_service::ReadWorldViewRequest;
use serde::de::IntoDeserializer;
use serde_json::{Value, json};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::{
    io::{self, Read, Write},
    net::TcpStream,
    time::{Duration, Instant},
};
use tungstenite::handshake::machine::TryParse;
use tungstenite::handshake::server::{ErrorResponse, Request};
static ACTIVE_READS: AtomicUsize = AtomicUsize::new(0);
struct ReadPermit;
impl ReadPermit {
    fn acquire() -> Result<Self, (u16, &'static str)> {
        ACTIVE_READS
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| {
                (n < MAX_ACTIVE_READS).then_some(n + 1)
            })
            .map(|_| Self)
            .map_err(|_| (429, "read_limit_exceeded"))
    }
}
impl Drop for ReadPermit {
    fn drop(&mut self) {
        ACTIVE_READS.fetch_sub(1, Ordering::AcqRel);
    }
}

pub(super) fn serve_if_game_request(
    stream: &mut TcpStream,
    client: Option<&RemoteWorldServiceClient>,
) -> io::Result<bool> {
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut buffer = vec![0; MAX_READ_HTTP_HEADER_BYTES];
    let (header_len, request) = loop {
        let read = stream.peek(&mut buffer)?;
        if read == 0 {
            return Err(io::Error::from(io::ErrorKind::UnexpectedEof));
        }
        if let Some(parsed) = Request::try_parse(&buffer[..read])
            .map_err(|_| io::Error::from(io::ErrorKind::InvalidData))?
        {
            break parsed;
        }
        if read == buffer.len() || Instant::now() >= deadline {
            return Err(io::Error::from(io::ErrorKind::InvalidData));
        }
        std::thread::sleep(Duration::from_millis(5));
    };
    if request.uri().path() != "/v1/game" && !request.uri().path().starts_with("/v1/game/") {
        return Ok(false);
    }
    stream.read_exact(&mut buffer[..header_len])?;
    let response = handle(&request, client);
    let body = response.body().as_deref().unwrap_or("");
    write!(
        stream,
        "HTTP/1.1 {}\r\nContent-Type: application/json\r\nCache-Control: no-store\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{}",
        response.status(),
        body.len(),
        body
    )?;
    stream.flush()?;
    Ok(true)
}

pub(super) fn handle(
    request: &Request,
    client: Option<&RemoteWorldServiceClient>,
) -> ErrorResponse {
    render_response(route(request, client))
}

fn render_response(result: Result<Value, (u16, &'static str)>) -> ErrorResponse {
    let (mut status, mut body) = match result {
        Ok(value) => (200, value),
        Err((status, code)) => (
            status,
            json!({"code":code, "retryable":matches!(status, 429 | 503)}),
        ),
    };
    if body.to_string().len() > MAX_READ_RESPONSE_BYTES {
        status = 503;
        body = json!({"code":"response_limit_exceeded", "retryable":false});
    }
    tungstenite::http::Response::builder()
        .status(status)
        .header("Content-Type", "application/json")
        .header("Cache-Control", "no-store")
        .header("Connection", "close")
        .body(Some(body.to_string()))
        .expect("fixed HTTP response headers")
}

fn route(
    request: &Request,
    client: Option<&RemoteWorldServiceClient>,
) -> Result<Value, (u16, &'static str)> {
    if request.method() != "GET"
        || request.uri().query().is_some()
        || request.headers().contains_key("Transfer-Encoding")
        || request
            .headers()
            .get_all("Content-Length")
            .iter()
            .any(|value| value != "0")
    {
        return Err((400, "invalid_request"));
    }
    if request.uri().path() == READ_OPENAPI_PATH {
        return Ok(read_bootstrap_openapi());
    }
    if request.uri().path() == "/v1/game/info" {
        return Ok(
            json!({"api_revision":API_REVISION, "mode":"signed_read_bootstrap",
            "supported_routes": if client.is_some() { vec!["agent:observation", "agent:capabilities", "agent:task-control"] } else {vec![]},
            "availability":"checked_against_each_authorized_world_read",
            "actor_proof_header":"X-Oasis7-World-Read-Proof", "actor_proof_encoding":"hex_json",
            "operation_domain":VIEW_PATH, "max_proof_header_bytes":MAX_READ_PROOF_HEADER_BYTES,
            "schema_revision":READ_SCHEMA_REVISION, "openapi_url":READ_OPENAPI_PATH,
            "limits":read_bootstrap_limits(),
            "writes_available":false}),
        );
    }
    let client = client.ok_or((503, "capability_unavailable"))?;
    if request.headers().contains_key("Origin") {
        return Err((403, "access_denied"));
    }
    let scope_agent = client
        .config()
        .scope_id
        .strip_prefix("agent:")
        .ok_or((503, "capability_unavailable"))?;
    let prefix = format!(
        "/v1/game/worlds/{}/agents/{}/",
        client.config().expected_world.world_id,
        scope_agent
    );
    let resource = request
        .uri()
        .path()
        .strip_prefix(&prefix)
        .ok_or((403, "access_denied"))?;
    if !matches!(resource, "observation" | "capabilities" | "task-control") {
        return Err((404, "capability_unavailable"));
    }
    let mut headers = request
        .headers()
        .get_all("X-Oasis7-World-Read-Proof")
        .iter();
    let proof = headers.next().ok_or((428, "actor_proof_required"))?;
    if headers.next().is_some() || proof.as_bytes().len() > MAX_READ_PROOF_HEADER_BYTES {
        return Err((400, "invalid_actor_proof"));
    }
    let bytes = hex::decode(proof.as_bytes()).map_err(|_| (400, "invalid_actor_proof"))?;
    let value: Value = parse_request(&bytes).map_err(|_| (400, "invalid_actor_proof"))?;
    let mut unknown = false;
    let signed: SignedReadRequest<ReadWorldViewRequest> =
        serde_ignored::deserialize(value.into_deserializer(), |_| unknown = true)
            .map_err(|_| (400, "invalid_actor_proof"))?;
    if unknown {
        return Err((400, "invalid_actor_proof"));
    }
    authority::verify_read_request(VIEW_PATH, &signed).map_err(|_| (403, "access_denied"))?;
    if signed.request.world != client.config().expected_world
        || signed.request.scope_id != client.config().scope_id
    {
        return Err((403, "access_denied"));
    }
    if signed.request.fixed_commit.is_some() {
        return Err((400, "historical_read_unavailable"));
    }
    signed
        .request
        .validate()
        .map_err(|_| (400, "invalid_actor_proof"))?;
    let caller_public_key = signed.subject_public_key.clone();
    let _permit = ReadPermit::acquire()?;
    let view = client
        .read_actor_view(signed)
        .map_err(|error| match error {
            WorldServiceClientError::Rejected {
                status: 401 | 403 | 404,
                ..
            } => (403, "access_denied"),
            _ => (503, "world_read_unavailable"),
        })?;
    let observation = if resource == "task-control" {
        let control = view.projection().game_task_control.as_ref().ok_or_else(|| {
            if view.projection().canonical_agent_owner.as_ref().is_some_and(|owner| {
                owner.agent_id == scope_agent && owner.public_key != caller_public_key
            }) {
                (403, "access_denied")
            } else {
                (404, "task_control_unavailable")
            }
        })?;
        let commit = &view.version().commit;
        if control.agent_id != scope_agent
            || (control.owner_public_key != caller_public_key
                && !control.executor_binding.as_ref().is_some_and(|b| {
                    b.executor_public_key == caller_public_key
                        && b.executor_epoch == control.executor_epoch
                        && control.task_status == oasis7_agent_api::game::GameTaskStatusV1::Active
                }))
            || control.world_id != commit.world.world_id
            || control.genesis_digest != commit.world.genesis_digest
            || control.branch_id != commit.binding.branch_id
            || control.reorg_epoch.0 != commit.binding.reorg_generation
        {
            return Err((503, "world_read_unavailable"));
        }
        serde_json::to_value(control)
    } else if resource == "capabilities" {
        let catalog = &view
                .projection()
                .agent_context
                .as_ref()
                .filter(|context| context.agent_id == scope_agent)
                .ok_or((503, "capability_unavailable"))?
                .capability_catalog;
        catalog.validate().map_err(|_| (503,"world_read_unavailable"))?;
        if catalog.world_id != view.version().commit.binding.provider_world_id
            || catalog.branch_id != view.version().commit.binding.branch_id
            || catalog.logical_tick != view.logical_tick()
            || !matches!(&catalog.subject, oasis7_wasm_abi::CapabilitySubject::Agent {agent_id,..} if agent_id == scope_agent) {
            return Err((503,"world_read_unavailable"));
        }
        serde_json::to_value(catalog)
    } else {
        serde_json::to_value(
            view.projection()
                .state
                .agents
                .get(scope_agent)
                .ok_or((403, "access_denied"))?,
        )
    }
    .map_err(|_| (503, "world_read_unavailable"))?;
    Ok(json!({"api_revision":API_REVISION, "agent_id":scope_agent,
        "logical_tick":view.logical_tick().to_string(),
        "world_binding":decimal_integers(serde_json::to_value(view.version()).map_err(|_| (503,"world_read_unavailable"))?),
        "resource":resource,
        "data":decimal_integers(observation)}))
}

// Bootstrap projection integers are lossless decimal strings. This is a new
// Game API projection, not the World Service DTO's numeric wire representation.
fn decimal_integers(value: Value) -> Value {
    match value {
        Value::Number(n) if n.is_u64() || n.is_i64() => Value::String(n.to_string()),
        Value::Array(values) => Value::Array(values.into_iter().map(decimal_integers).collect()),
        Value::Object(values) => Value::Object(
            values
                .into_iter()
                .map(|(k, v)| (k, decimal_integers(v)))
                .collect(),
        ),
        value => value,
    }
}

#[cfg(test)]
#[path = "game_read_api_tests.rs"]
mod tests;
