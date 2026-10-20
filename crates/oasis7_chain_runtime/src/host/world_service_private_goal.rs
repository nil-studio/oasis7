//! Opt-in confidential owner goal subset; never a public CAS or full TaskSnapshot.
use super::*;
use oasis7::world_service::{
    private_goal_prepare::SignedGoalPrepareV1, private_goal_store::PrivateGoalStore,
};
use oasis7_agent_api::game::GameTaskControlV1;
use std::{io::Write, path::PathBuf};
use zeroize::Zeroizing;
#[path = "world_service_private_task_context.rs"]
mod task_context;
use oasis7::world_service::private_task_context::{
    TASK_CONTEXT_PREPARE_PATH, TASK_CONTEXT_READ_PATH, TASK_SNAPSHOT_PATH,
};
use oasis7::world_service::task_factory_quote::TASK_FACTORY_QUOTE_PATH;
use oasis7::world_service::task_recipe_quote::TASK_RECIPE_QUOTE_PATH;

use oasis7::world_service::private_goal_read::{
    CurrentGoalRequest, CurrentGoalResponse, GOAL_CURRENT_PATH as CURRENT_PATH,
    GOAL_PREPARE_PATH as PREPARE_PATH, verify_current_request,
};
/// Load once; never place this secret or private namespace in public config/CAS.
pub(crate) fn configured_store(
    address: std::net::SocketAddr,
    public_roots: &[&Path],
) -> Result<Option<Arc<PrivateGoalStore>>, String> {
    let directory = std::env::var_os("OASIS7_PRIVATE_GOAL_DIR");
    let keyfile = std::env::var_os("OASIS7_PRIVATE_GOAL_KEY_FILE");
    open_configured_store(address, public_roots, directory, keyfile)
}
fn open_configured_store(
    address: std::net::SocketAddr,
    public_roots: &[&Path],
    directory: Option<std::ffi::OsString>,
    keyfile: Option<std::ffi::OsString>,
) -> Result<Option<Arc<PrivateGoalStore>>, String> {
    match (directory, keyfile) {
        (None, None) => Ok(None),
        (Some(directory), Some(keyfile)) => {
            if !address.ip().is_loopback() {
                return Err("private goal routes require a loopback listener".into());
            }
            let directory = PathBuf::from(directory);
            let keyfile = PathBuf::from(keyfile);
            for candidate in [&directory, &keyfile] {
                if !candidate.is_absolute()
                    || candidate
                        .components()
                        .skip(1)
                        .any(|c| !matches!(c, std::path::Component::Normal(_)))
                {
                    return Err("private goal paths must be absolute and normalized".into());
                }
                // Resolve existing ancestor before creation, catching public-root aliases.
                let resolved = if candidate.exists() {
                    std::fs::canonicalize(candidate)
                } else {
                    candidate
                        .parent()
                        .ok_or("missing private path parent")?
                        .canonicalize()
                        .map(|parent| parent.join(candidate.file_name().unwrap()))
                }
                .map_err(|_| "private path unavailable")?;
                for public in public_roots {
                    let public = public
                        .canonicalize()
                        .map_err(|_| "public root unavailable")?;
                    if resolved.starts_with(&public) || public.starts_with(&resolved) {
                        return Err("private goal paths must be outside public storage".into());
                    }
                }
            }
            let key = read_key(&keyfile)?;
            Ok(Some(Arc::new(PrivateGoalStore::open(&directory, *key)?)))
        }
        _ => Err("both private goal directory and key file must be configured".into()),
    }
}
fn read_key(path: &Path) -> Result<Zeroizing<[u8; 32]>, String> {
    oasis7::world_service::private_key_file::read_private_key_file(path)
}

#[derive(Clone, Copy)]
enum PrivateError {
    Invalid,
    Forbidden,
    Conflict,
    TaskContextReadConflict(task_context::ReadConflictStage),
    QuoteBaseDrift,
    QuoteDeliveryDrift,
    ContextReadDeliveryDrift(Option<task_context::ReadConflictStage>),
    Recovery,
    Unavailable,
}
impl From<String> for PrivateError {
    fn from(_: String) -> Self {
        Self::Unavailable
    }
}
impl From<&str> for PrivateError {
    fn from(_: &str) -> Self {
        Self::Unavailable
    }
}
impl From<oasis7::world_service::private_goal_store::PreparedGoalError> for PrivateError {
    fn from(error: oasis7::world_service::private_goal_store::PreparedGoalError) -> Self {
        use oasis7::world_service::private_goal_store::PreparedGoalError;
        match error {
            PreparedGoalError::Invalid => Self::Invalid,
            PreparedGoalError::Conflict => Self::Conflict,
            PreparedGoalError::Unavailable => Self::Recovery,
        }
    }
}
fn current(
    service: &Service<'_>,
    agent: &str,
    task: &str,
    key: &str,
) -> Result<(PinnedWorld, GameTaskControlV1), PrivateError> {
    let pinned = service.pin(None)?;
    let control =
        game_task::current_control(&pinned.world, agent)?.ok_or(PrivateError::Forbidden)?;
    if control.task_id != task
        || control.owner_public_key != key
        || control.world_id != service.identity.world_id
        || control.genesis_digest != service.identity.genesis_digest
    {
        return Err(PrivateError::Forbidden);
    }
    Ok((pinned, control))
}
fn same_authority(proof: &SignedGoalPrepareV1, control: &GameTaskControlV1) -> Result<(), String> {
    let mut predecessor = control.clone();
    predecessor.task_revision = proof.request.expected_task_revision;
    predecessor.latest_goal_revision = proof.request.expected_latest_goal_revision;
    proof.validate_against(&predecessor)
}
fn prepare(
    service: &Service<'_>,
    store: &PrivateGoalStore,
    proof: SignedGoalPrepareV1,
    before_delivery: impl FnOnce(),
) -> Result<Value, PrivateError> {
    proof.verify().map_err(|_| PrivateError::Invalid)?;
    let (_, before) = current(
        service,
        &proof.request.agent_id,
        &proof.request.task_id,
        &proof.subject_public_key,
    )?;
    same_authority(&proof, &before).map_err(|_| PrivateError::Forbidden)?;
    let receipt = match store.lookup_prepared(&proof).map_err(PrivateError::from)? {
        Some(receipt) => receipt,
        None => {
            proof
                .validate_against(&before)
                .map_err(|_| PrivateError::Conflict)?;
            store.prepare(&proof).map_err(PrivateError::from)?
        }
    };
    before_delivery();
    let (_, after) = current(
        service,
        &proof.request.agent_id,
        &proof.request.task_id,
        &proof.subject_public_key,
    )?;
    if before != after {
        return Err(PrivateError::Conflict);
    }
    service
        .signed(PREPARE_PATH, &proof, receipt)
        .map_err(PrivateError::from)
}
fn read_current(
    service: &Service<'_>,
    store: &PrivateGoalStore,
    request: SignedReadRequest<CurrentGoalRequest>,
    before_delivery: impl FnOnce(),
) -> Result<Value, PrivateError> {
    verify_current_request(&request).map_err(|_| PrivateError::Invalid)?;
    let r = &request.request;
    if r.schema_revision != 1
        || r.world != service.identity
        || [&r.agent_id, &r.task_id]
            .iter()
            .any(|s| s.is_empty() || s.len() > 256)
    {
        return Err("invalid current goal request".into());
    }
    let (pinned, control) = current(
        service,
        &r.agent_id,
        &r.task_id,
        &request.subject_public_key,
    )?;
    let (activation_commit, goal) = if let Some(result) =
        game_task::active_publish(&pinned.world, &r.agent_id)?
    {
        let WorldServicePayloadV1::GameGoalPublish(public) = &result.request.signed_payload else {
            return Err("invalid publication".into());
        };
        let historical = service
            .pin_at_height(result.committed_height)
            .map_err(|_| PrivateError::Recovery)?;
        let digest = correlation::key_digest(&result.request.correlation.key)?;
        let encoded = serde_json::to_value(&result).map_err(|_| "invalid publication")?;
        if historical
            .world
            .capability_revocation_state()
            .world_service_results
            .get(&digest)
            != Some(&encoded)
        {
            return Err("activation history unavailable".into());
        }
        let receipt = store
            .get_prepared(&public.request)
            .map_err(|_| PrivateError::Recovery)?;
        (Some(historical.commit), Some(receipt))
    } else {
        (None, None)
    };
    before_delivery();
    let (fresh, after) = current(
        service,
        &r.agent_id,
        &r.task_id,
        &request.subject_public_key,
    )?;
    if control != after || pinned.commit.binding != fresh.commit.binding {
        return Err(PrivateError::Conflict);
    }
    service
        .signed(
            CURRENT_PATH,
            &request,
            CurrentGoalResponse {
                schema_revision: 1,
                control,
                read_commit: pinned.commit,
                activation_commit,
                active_goal: goal,
            },
        )
        .map_err(PrivateError::from)
}

fn validate_private_headers(bytes: &[u8]) -> Result<(), PrivateError> {
    let end = bytes
        .windows(4)
        .position(|v| v == b"\r\n\r\n")
        .ok_or(PrivateError::Invalid)?;
    let headers = std::str::from_utf8(&bytes[..end]).map_err(|_| PrivateError::Invalid)?;
    if !headers.is_ascii() {
        return Err(PrivateError::Invalid);
    }
    if headers
        .split("\r\n")
        .next()
        .unwrap_or_default()
        .bytes()
        .any(|b| b < 32 || b == 127)
    {
        return Err(PrivateError::Invalid);
    }
    let mut length = None;
    let mut host = false;
    let mut content_type = false;
    for line in headers.split("\r\n").skip(1) {
        let (name, value) = line.split_once(':').ok_or(PrivateError::Invalid)?;
        if name.is_empty()
            || !name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&b))
            || value.bytes().any(|b| b < 32 && b != b'\t' || b == 127)
        {
            return Err(PrivateError::Invalid);
        }
        if [
            "origin",
            "authorization",
            "forwarded",
            "x-forwarded-for",
            "x-forwarded-host",
            "x-forwarded-proto",
        ]
        .iter()
        .any(|blocked| name.eq_ignore_ascii_case(blocked))
        {
            return Err(PrivateError::Forbidden);
        }
        if name.eq_ignore_ascii_case("transfer-encoding") {
            return Err(PrivateError::Invalid);
        }
        if name.eq_ignore_ascii_case("content-length") {
            let value = value.trim();
            if length.is_some() || value.is_empty() || !value.bytes().all(|b| b.is_ascii_digit()) {
                return Err(PrivateError::Invalid);
            }
            length = Some(value.parse::<usize>().map_err(|_| PrivateError::Invalid)?);
        }
        if name.eq_ignore_ascii_case("host") {
            if host || value.trim().is_empty() {
                return Err(PrivateError::Invalid);
            }
            host = true;
        }
        if name.eq_ignore_ascii_case("content-type") {
            if content_type
                || !value
                    .trim()
                    .split(';')
                    .next()
                    .unwrap_or_default()
                    .eq_ignore_ascii_case("application/json")
            {
                return Err(PrivateError::Invalid);
            }
            content_type = true;
        }
    }
    if !host || !content_type || length != Some(bytes.len() - end - 4) {
        return Err(PrivateError::Invalid);
    }
    Ok(())
}

#[expect(
    clippy::too_many_arguments,
    reason = "Explicit authority and storage roots"
)]
pub(crate) fn maybe_handle(
    stream: &mut TcpStream,
    bytes: &[u8],
    runtime: &Arc<Mutex<NodeRuntime>>,
    method: &str,
    target: &str,
    world_id: &str,
    world_dir: &Path,
    records: &Path,
    storage: &Path,
    signer: &FeedbackSubmitSigner,
    store: Option<&PrivateGoalStore>,
) -> Result<bool, String> {
    maybe_handle_with_delivery_hook(
        stream,
        bytes,
        runtime,
        method,
        target,
        world_id,
        world_dir,
        records,
        storage,
        signer,
        store,
        || {},
        |stage| {
            tracing::warn!(stage, "private task context operation rejected");
            #[cfg(test)]
            eprintln!(
                "TASK_PRIVATE_CONTEXT_REJECT route={} stage={stage}",
                private_route_tag(target)
            );
        },
    )
}

#[cfg(test)]
fn private_route_tag(target: &str) -> &'static str {
    match target.split('?').next().unwrap_or(target) {
        PREPARE_PATH => "goal_prepare",
        CURRENT_PATH => "goal_current",
        TASK_SNAPSHOT_PATH => "task_snapshot",
        TASK_CONTEXT_PREPARE_PATH => "task_context_prepare",
        TASK_CONTEXT_READ_PATH => "task_context_read",
        TASK_RECIPE_QUOTE_PATH => "task_recipe_quote",
        TASK_FACTORY_QUOTE_PATH => "task_factory_quote",
        _ => "other",
    }
}
/// Returns bounded errors only; never prints body, signatures, keys or paths.
#[expect(
    clippy::too_many_arguments,
    reason = "Explicit service authority and private store"
)]
pub(crate) fn maybe_handle_with_delivery_hook(
    stream: &mut TcpStream,
    bytes: &[u8],
    runtime: &Arc<Mutex<NodeRuntime>>,
    method: &str,
    target: &str,
    world_id: &str,
    world_dir: &Path,
    records: &Path,
    storage: &Path,
    signer: &FeedbackSubmitSigner,
    store: Option<&PrivateGoalStore>,
    before_delivery: impl FnOnce(),
    mut on_read_conflict: impl FnMut(&'static str),
) -> Result<bool, String> {
    let path = target.split('?').next().unwrap_or(target);
    if ![
        PREPARE_PATH,
        CURRENT_PATH,
        TASK_SNAPSHOT_PATH,
        TASK_CONTEXT_PREPARE_PATH,
        TASK_CONTEXT_READ_PATH,
        TASK_RECIPE_QUOTE_PATH,
        TASK_FACTORY_QUOTE_PATH,
    ]
    .contains(&path)
    {
        return Ok(false);
    }
    #[cfg(test)]
    let route = private_route_tag(path);
    #[cfg(test)]
    let started = std::time::Instant::now();
    let result: Result<Value, PrivateError> = (|| {
        if method != "POST"
            || target.contains('?')
            || !stream
                .local_addr()
                .map_err(|_| "local address unavailable")?
                .ip()
                .is_loopback()
            || !stream
                .peer_addr()
                .map_err(|_| "peer address unavailable")?
                .ip()
                .is_loopback()
        {
            return Err(PrivateError::Forbidden);
        }
        validate_private_headers(bytes)?;
        let store = store.ok_or("private goals disabled")?;
        let body = super::super::feedback_submit_api::extract_http_json_body(bytes)?;
        let service = Service {
            records,
            storage,
            identity: world_service_read::identity(world_dir, world_id)?,
            signer,
            runtime,
            guarded: None,
        };
        if [
            TASK_SNAPSHOT_PATH,
            TASK_CONTEXT_PREPARE_PATH,
            TASK_CONTEXT_READ_PATH,
            TASK_RECIPE_QUOTE_PATH,
            TASK_FACTORY_QUOTE_PATH,
        ]
        .contains(&path)
        {
            return task_context::process(&service, store, path, body, before_delivery);
        }
        if path == PREPARE_PATH {
            prepare(
                &service,
                store,
                parse(body).map_err(|_| PrivateError::Invalid)?,
                before_delivery,
            )
        } else {
            read_current(
                &service,
                store,
                parse(body).map_err(|_| PrivateError::Invalid)?,
                before_delivery,
            )
        }
    })();
    let (status, value, read_conflict_stage) = match result {
        Ok(value) => (200, value, None),
        Err(error) => {
            let (status, code, stage) = match error {
                PrivateError::Invalid => (400, "invalid_private_goal_request", None),
                PrivateError::Forbidden => (403, "private_goal_forbidden", None),
                PrivateError::Conflict => (409, "private_goal_conflict", None),
                PrivateError::TaskContextReadConflict(stage) => {
                    (409, "private_goal_conflict", Some(stage.as_str()))
                }
                PrivateError::QuoteBaseDrift => (
                    409,
                    oasis7::world_service::task_factory_quote::TASK_QUOTE_BASE_DRIFT,
                    None,
                ),
                PrivateError::QuoteDeliveryDrift => (
                    409,
                    oasis7::world_service::task_factory_quote::TASK_QUOTE_DELIVERY_DRIFT,
                    None,
                ),
                PrivateError::ContextReadDeliveryDrift(stage) => (
                    409,
                    oasis7_client_api::world_service::task_context::TASK_CONTEXT_READ_DELIVERY_DRIFT,
                    stage.map(task_context::ReadConflictStage::as_str),
                ),
                PrivateError::Recovery => (503, "private_goal_recovery_required", None),
                PrivateError::Unavailable => (503, "private_goal_unavailable", None),
            };
            (status, json!({"error":code}), stage)
        }
    };
    if let Some(stage) = read_conflict_stage {
        on_read_conflict(stage);
    }
    let body = serde_json::to_vec(&value).map_err(|_| "private response encoding failed")?;
    let response = format!(
        "HTTP/1.1 {status} {}\r\nContent-Type: application/json\r\nCache-Control: no-store\r\nPragma: no-cache\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        match status {
            200 => "OK",
            400 => "Bad Request",
            403 => "Forbidden",
            409 => "Conflict",
            _ => "Service Unavailable",
        },
        body.len()
    );
    let write_result = stream
        .write_all(response.as_bytes())
        .and_then(|_| stream.write_all(&body));
    #[cfg(test)]
    if route != "other" {
        let outcome = match &write_result {
            Ok(()) => "response_written",
            Err(error) if is_peer_disconnect(error) => "peer_disconnected",
            Err(_) => "write_failed",
        };
        eprintln!(
            "TASK_PRIVATE_HTTP route={route} status={status} outcome={outcome} elapsed_ms={}",
            started.elapsed().as_millis()
        );
    }
    write_result.map_err(|_| "private response write failed")?;
    Ok(true)
}

#[cfg(test)]
fn is_peer_disconnect(error: &std::io::Error) -> bool {
    matches!(
        error.kind(),
        std::io::ErrorKind::BrokenPipe
            | std::io::ErrorKind::ConnectionAborted
            | std::io::ErrorKind::ConnectionReset
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn private_native_quote_routes_reject_query_origin_and_disabled_store() {
        use std::io::{Read, Write};
        let runtime = Arc::new(Mutex::new(NodeRuntime::new(
            oasis7_node::NodeConfig::new("quote-node", "world", oasis7_node::NodeRole::Sequencer)
                .unwrap(),
        )));
        let signer = FeedbackSubmitSigner {
            private_key_hex: hex::encode([9; 32]),
            public_key_hex: hex::encode([10; 32]),
        };
        for quote_path in [TASK_RECIPE_QUOTE_PATH, TASK_FACTORY_QUOTE_PATH] {
            for (suffix, headers, expected) in [
                ("?token=forbidden", "", 403),
                ("", "Origin: https://example.invalid\r\n", 403),
                ("", "", 503),
            ] {
                let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
                let mut client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
                let (mut server, _) = listener.accept().unwrap();
                let target = format!("{quote_path}{suffix}");
                let bytes=format!("POST {target} HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\n{headers}Content-Length: 2\r\n\r\n{{}}").into_bytes();
                assert!(
                    maybe_handle(
                        &mut server,
                        &bytes,
                        &runtime,
                        "POST",
                        &target,
                        "world",
                        Path::new("unused"),
                        Path::new("unused"),
                        Path::new("unused"),
                        &signer,
                        None
                    )
                    .unwrap()
                );
                server.flush().unwrap();
                drop(server);
                let mut reply = String::new();
                client.read_to_string(&mut reply).unwrap();
                assert_eq!(
                    reply
                        .split_whitespace()
                        .nth(1)
                        .unwrap()
                        .parse::<u16>()
                        .unwrap(),
                    expected
                );
                assert!(reply.contains("Cache-Control: no-store"));
                assert!(!reply.contains("Access-Control-Allow-Origin"));
            }
        }
    }
    #[test]
    fn private_header_guard_rejects_bare_line_breaks_and_controls() {
        let suffix =
            "\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: 0\r\n\r\n";
        assert!(
            validate_private_headers(
                format!("POST /v1/world/private-goal/current HTTP/1.1{suffix}").as_bytes()
            )
            .is_ok()
        );
        for bad in [
            "POST /v1/world/private-goal/current HTTP/1.1\nOrigin: evil",
            "POST /v1/world/private-goal/current HTTP/1.1\rOrigin: evil",
            "POST /v1/world/private-goal/current HTTP/1.1\0",
            "POST /v1/world/private-goal/current HTTP/1.1\r\nX-Test: value\nOrigin: evil",
        ] {
            assert!(validate_private_headers(format!("{bad}{suffix}").as_bytes()).is_err());
        }
    }
    #[cfg(unix)]
    #[test]
    fn private_configuration_is_opt_in_loopback_and_outside_public_roots() {
        use std::os::unix::fs::PermissionsExt;
        let mut nonce = [0; 16];
        getrandom::fill(&mut nonce).unwrap();
        let base = std::env::temp_dir()
            .canonicalize()
            .unwrap()
            .join(format!("oasis7-private-config-{}", hex::encode(nonce)));
        std::fs::create_dir(&base).unwrap();
        let public = base.join("public");
        std::fs::create_dir(&public).unwrap();
        let private = base.join("private");
        let key = base.join("key");
        std::fs::write(&key, [7u8; 32]).unwrap();
        std::fs::set_permissions(&key, std::fs::Permissions::from_mode(0o600)).unwrap();
        let local = "127.0.0.1:12345".parse().unwrap();
        let remote = "0.0.0.0:12345".parse().unwrap();
        let config = |address, directory: &Path, key: &Path| {
            open_configured_store(
                address,
                &[&public],
                Some(directory.as_os_str().to_owned()),
                Some(key.as_os_str().to_owned()),
            )
        };
        assert!(
            open_configured_store(remote, &[&public], None, None)
                .unwrap()
                .is_none()
        );
        assert!(
            open_configured_store(
                local,
                &[&public],
                Some(private.as_os_str().to_owned()),
                None
            )
            .is_err()
        );
        assert!(config(remote, &private, &key).is_err());
        assert!(!private.exists());
        assert!(config(local, &public.join("secret"), &key).is_err());
        assert!(config(local, &base, &key).is_err());
        assert!(config(local, &private, &public.join("key")).is_err());
        assert!(config(local, &private, &key).unwrap().is_some());
        std::fs::remove_dir_all(base).unwrap();
    }
    #[cfg(unix)]
    #[test]
    fn private_key_loader_rejects_links_permissions_and_lengths() {
        use std::os::unix::fs::{PermissionsExt, symlink};
        let directory = std::env::temp_dir().join(format!(
            "oasis7-private-key-{}-{}",
            std::process::id(),
            super::super::super::now_unix_ms()
        ));
        std::fs::create_dir(&directory).unwrap();
        let directory = directory.canonicalize().unwrap();
        let file = directory.join("key");
        std::fs::write(&file, [3u8; 32]).unwrap();
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600)).unwrap();
        assert_eq!(*read_key(&file).unwrap(), [3; 32]);
        let link = directory.join("link");
        symlink(&file, &link).unwrap();
        assert!(read_key(&link).is_err());
        let hard = directory.join("hard");
        std::fs::hard_link(&file, &hard).unwrap();
        assert!(read_key(&file).is_err());
        std::fs::remove_file(hard).unwrap();
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o640)).unwrap();
        assert!(read_key(&file).is_err());
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600)).unwrap();
        for size in [0, 31, 33] {
            std::fs::write(&file, vec![3u8; size]).unwrap();
            assert!(read_key(&file).is_err());
        }
        let directory_link = directory.with_extension("alias");
        symlink(&directory, &directory_link).unwrap();
        assert!(read_key(&directory_link.join("key")).is_err());
        std::fs::remove_file(directory_link).unwrap();
        std::fs::remove_dir_all(directory).unwrap();
    }
}
