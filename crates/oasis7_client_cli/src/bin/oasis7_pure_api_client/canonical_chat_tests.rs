use super::*;
use oasis7::world_service::*;
use oasis7_client_api::world_service as authority;
#[cfg(unix)]
use std::io::Read;
use std::net::TcpListener;
#[path = "canonical_chat_fixture.rs"]
mod fixture;
use fixture::world_fixture;
#[cfg(unix)]
use fixture::{canonical_fixture, private_state_dir};

#[test]
fn owner_context_and_response_reject_foreign_stale_and_changed_authority() {
    let (world, private, identity, commit) = world_fixture();
    let view = agent_chat::owner_view(&world, "agent-0").unwrap().unwrap();
    let key = view.public_key.clone();
    let request = ReadWorldViewRequest {
        observation_nonce: None,
        formal_anchor: None,
        contract_version: 1,
        world: identity,
        scope_id: "agent:agent-0".into(),
        min_commit: Some(commit.clone()),
        fixed_commit: None,
        deadline_unix_ms: None,
    };
    let mut context = OwnerContext {
        signing_domain: VIEW_PATH.into(),
        signing_bytes_hex: hex::encode(authority::signing_bytes(VIEW_PATH, &request).unwrap()),
        request,
        fence: Some(CanonicalAgentOwnerFenceV1::from(&view)),
        expected: ExpectedOwner {
            agent_id: "agent-0".into(),
            player_id: "owner".into(),
            public_key: key.clone(),
            world_id: "canonical-cli-fixture".into(),
            branch_id: "main".into(),
            reorg_epoch: 0,
        },
    };
    let input = ChatInput {
        agent_id: "agent-0",
        player_id: "owner",
        message: "goal",
        private_key_hex: &private,
        public_key_hex: None,
        intent_tick: None,
        intent_seq: None,
    };
    let hello = json!({"world_id":"canonical-cli-fixture"});
    validate_context(&context, &input, &key, 0, &hello).unwrap();
    context.signing_domain = "foreign".into();
    assert!(validate_context(&context, &input, &key, 0, &hello).is_err());
    context.signing_domain = VIEW_PATH.into();
    context.signing_bytes_hex.push('0');
    assert!(validate_context(&context, &input, &key, 0, &hello).is_err());
    context.signing_bytes_hex.pop();
    assert!(validate_context(&context, &input, &key, 1, &hello).is_err());
    assert!(validate_context(&context, &input, "foreign", 0, &hello).is_err());
    let version = ProjectionVersion {
        commit,
        projection_revision: "fixture".into(),
        visibility_scope: "agent:agent-0".into(),
    };
    validate_view(&context, &version, &view).unwrap();
    let mut stale = version.clone();
    stale.commit.position = 1;
    assert!(validate_view(&context, &stale, &view).is_err());
    let mut foreign = version.clone();
    foreign.commit.world.world_id = "foreign".into();
    assert!(validate_view(&context, &foreign, &view).is_err());
    let mut wrong = view.clone();
    wrong.player_id = "foreign".into();
    assert!(validate_view(&context, &version, &wrong).is_err());
    let mut wrong = view.clone();
    wrong.canonical_authority.agent_identity_generation += 1;
    assert!(validate_view(&context, &version, &wrong).is_err());
    let mut wrong = view.clone();
    wrong.current_intent_id = Some("replacement".into());
    assert!(validate_view(&context, &version, &wrong).is_err());
}

#[test]
fn partial_response_survives_timeout_without_losing_signed_frame() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    let (mut peer, _) = listener.accept().unwrap();
    let mut conn = ViewerConnection {
        reader: BufReader::new(client.try_clone().unwrap()),
        writer: BufWriter::new(client),
        hello_ack: Value::Null,
        pending_line: String::new(),
        endpoint: "fixture".into(),
    };
    peer.write_all(b"{\"type\":\"error\",\"message\":\"preserved")
        .unwrap();
    assert!(matches!(
        conn.read_response(Duration::from_millis(20)).unwrap(),
        ReadResponseOutcome::TimedOut
    ));
    peer.write_all(b"\"}\n").unwrap();
    let ReadResponseOutcome::Response(response) =
        conn.read_response(Duration::from_secs(1)).unwrap()
    else {
        panic!("missing frame")
    };
    assert_eq!(response.raw["message"], "preserved");
}

#[test]
fn canonical_outcomes_require_original_nonce_and_request_agent() {
    let private = hex::encode([73; 32]);
    let chat = build_signed_agent_chat_request(
        "agent-0",
        "owner",
        "goal",
        &private,
        None,
        Some(2),
        Some(3),
    )
    .unwrap();
    let nonce = chat.auth.as_ref().unwrap().nonce;
    let mut ack:oasis7_proto::viewer::AgentChatAck<u64>=serde_json::from_value(json!({"agent_id":"agent-0","accepted_at_tick":2,"message_len":4,"player_id":"owner","intent_tick":2,"intent_seq":3,"auth_nonce":nonce,"intent_id":"committed","accepted_event_seq":1,"status":"accepted"})).unwrap();
    assert!(correlated(&ack, &chat, nonce));
    ack.auth_nonce = Some(nonce - 1);
    assert!(!correlated(&ack, &chat, nonce));
    ack.auth_nonce = Some(nonce);
    ack.intent_seq = Some(4);
    assert!(!correlated(&ack, &chat, nonce));
    let mut error = oasis7_proto::viewer::AgentChatError {
        code: "invalid_scope".into(),
        message: "denied".into(),
        agent_id: Some("agent-0".into()),
    };
    assert!(matching_error(&error, &chat));
    error.agent_id = Some("foreign".into());
    assert!(!matching_error(&error, &chat));
    error.agent_id = None;
    assert!(!matching_error(&error, &chat));
}

#[cfg(unix)]
#[test]
fn canonical_chat_uses_existing_session_and_actual_signed_world_service() {
    canonical_fixture(false, None);
}
#[cfg(unix)]
#[test]
fn canonical_chat_accepts_uppercase_owner_key_in_signed_workflow() {
    canonical_fixture(false, Some("uppercase"));
}
#[cfg(unix)]
#[test]
fn canonical_chat_accepts_whitespace_owner_key_in_signed_workflow() {
    canonical_fixture(false, Some("whitespace"));
}

#[cfg(unix)]
#[test]
fn canonical_timeout_recovers_original_request_on_new_connection() {
    canonical_fixture(true, None);
}

#[cfg(unix)]
#[test]
fn pending_artifact_is_private_bounded_and_rejects_symlink_or_unknown_fields() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let (_, private, world, _) = world_fixture();
    let chat = build_signed_agent_chat_request(
        "agent-0",
        "owner",
        "private goal",
        &private,
        None,
        Some(2),
        Some(next_u64_id()),
    )
    .unwrap();
    let state = private_state_dir();
    let path = pending::save_at(&chat, &world, "fixture", &state).unwrap();
    assert_eq!(
        std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );
    let before = std::fs::read(&path).unwrap();
    pending::load(&path).unwrap();
    let link = path.parent().unwrap().join("link");
    symlink(&path, &link).unwrap();
    assert!(pending::load(&link).is_err());
    std::fs::remove_file(link).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
    assert!(pending::load(&path).is_err());
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    let mut raw: Value = serde_json::from_slice(&before).unwrap();
    raw["request"]["private_key_hex"] = json!("accidental secret");
    std::fs::write(&path, serde_json::to_vec(&raw).unwrap()).unwrap();
    assert!(pending::load(&path).is_err());
    std::fs::write(&path, b"\"ACCIDENTAL_PRIVATE_KEY_VALUE\"").unwrap();
    let error = pending::load(&path).err().unwrap();
    assert!(!error.contains("ACCIDENTAL_PRIVATE_KEY_VALUE"));
    std::fs::write(&path, b"{} {}").unwrap();
    assert!(pending::load(&path).is_err());
    std::fs::write(&path, vec![b' '; 1024 * 1024 + 1]).unwrap();
    assert!(pending::load(&path).is_err());
    std::fs::write(&path, before).unwrap();
    pending::complete(&path).unwrap();
    std::fs::remove_dir_all(state).unwrap();
}

#[cfg(unix)]
#[test]
fn retry_rejects_changed_endpoint_or_foreign_world_before_transport() {
    let (_, private, world, _) = world_fixture();
    let mut chat = build_signed_agent_chat_request(
        "agent-0",
        "owner",
        "goal",
        &private,
        None,
        Some(2),
        Some(next_u64_id()),
    )
    .unwrap();
    chat.world_id = Some(world.world_id.clone());
    chat.reorg_epoch = Some(0);
    chat.authority_scope = Some("player_agent_chat".into());
    chat.canonical_authority = Some(oasis7_proto::viewer::CanonicalAgentChatAuthorityV1 {
        branch_id: "main".into(),
        agent_identity_generation: 1,
    });
    let proof = chat.auth.as_ref().unwrap();
    chat.auth =
        Some(sign_agent_chat_auth_proof(&chat, proof.nonce, &proof.public_key, &private).unwrap());
    let state = private_state_dir();
    let path = pending::save_at(&chat, &world, "original-endpoint", &state).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    let (mut peer, _) = listener.accept().unwrap();
    let mut conn = ViewerConnection {
        reader: BufReader::new(client.try_clone().unwrap()),
        writer: BufWriter::new(client),
        hello_ack: json!({"capabilities":["canonical_agent_chat_v1"],"world_id":world.world_id}),
        pending_line: String::new(),
        endpoint: "changed-endpoint".into(),
    };
    assert!(
        retry(&mut conn, Duration::from_millis(100), &path)
            .err()
            .unwrap()
            .contains("different Viewer endpoint")
    );
    conn.endpoint = "original-endpoint".into();
    conn.hello_ack["world_id"] = json!("foreign");
    assert!(
        retry(&mut conn, Duration::from_millis(100), &path)
            .err()
            .unwrap()
            .contains("foreign")
    );
    peer.set_read_timeout(Some(Duration::from_millis(20)))
        .unwrap();
    assert!(peer.read(&mut [0; 1]).is_err());
    assert!(path.exists());
    pending::complete(&path).unwrap();
    std::fs::remove_dir_all(state).unwrap();
}

#[test]
fn canonical_owner_key_still_rejects_malformed_and_foreign_keys() {
    let private = hex::encode([73; 32]);
    for key in [
        "zz".to_string(),
        "00".repeat(31),
        "00".repeat(33),
        hex::encode([74; 32]),
    ] {
        assert!(owner_public_key(&private, Some(&key)).is_err());
    }
}

#[cfg(unix)]
#[test]
fn canonical_committed_request_survives_followup_failure() {
    canonical_fixture(false, Some("post_commit_failure"));
}

#[cfg(unix)]
#[test]
fn canonical_committed_request_survives_output_write_failure() {
    canonical_fixture(false, Some("write_failure"));
}
#[cfg(unix)]
#[test]
fn canonical_committed_request_survives_output_flush_failure() {
    canonical_fixture(false, Some("flush_failure"));
}

#[cfg(unix)]
#[test]
fn durable_state_selection_and_private_root_fail_closed() {
    use std::ffi::OsStr;
    use std::os::unix::fs::PermissionsExt;
    assert_eq!(
        pending::state_base(
            Some(OsStr::new("/isolated/state")),
            Some(OsStr::new("/home/owner"))
        )
        .unwrap(),
        std::path::PathBuf::from("/isolated/state")
    );
    assert_eq!(
        pending::state_base(None, Some(OsStr::new("/home/owner"))).unwrap(),
        std::path::PathBuf::from("/home/owner/.local/state")
    );
    assert!(pending::state_base(None, None).is_err());
    assert!(
        pending::state_base(
            Some(OsStr::new("relative")),
            Some(OsStr::new("/home/owner"))
        )
        .is_err()
    );
    assert!(pending::state_base(None, Some(OsStr::new("relative"))).is_err());
    let state = private_state_dir();
    std::fs::set_permissions(&state, std::fs::Permissions::from_mode(0o755)).unwrap();
    let (_, private, world, _) = world_fixture();
    let chat = build_signed_agent_chat_request(
        "agent-0",
        "owner",
        "privategoal",
        &private,
        None,
        Some(2),
        Some(next_u64_id()),
    )
    .unwrap();
    let path = pending::save_at(&chat, &world, "fixture", &state).unwrap();
    assert!(path.starts_with(state.join("oasis7/canonical-chat")));
    assert_eq!(
        std::fs::metadata(&state).unwrap().permissions().mode() & 0o777,
        0o755
    );
    assert_eq!(
        std::fs::metadata(state.join("oasis7"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o700
    );
    assert!(
        pending::save_at(&chat, &world, "fixture", &state).is_err(),
        "exclusive original path must not overwrite"
    );
    pending::load(&path).unwrap();
    pending::complete(&path).unwrap();
    std::fs::set_permissions(state.join("oasis7"), std::fs::Permissions::from_mode(0o755)).unwrap();
    assert!(
        pending::save_at(&chat, &world, "fixture", &state).is_err(),
        "unsafe private app root must be rejected, not chmodded"
    );
    std::fs::remove_dir_all(state.join("oasis7")).unwrap();
    std::os::unix::fs::symlink("/tmp", state.join("oasis7")).unwrap();
    assert!(pending::save_at(&chat, &world, "fixture", &state).is_err());
    std::fs::remove_dir_all(state).unwrap();
}

#[cfg(unix)]
#[test]
fn durable_state_rejects_existing_ancestor_symlink_and_writable_parent() {
    use std::os::unix::fs::{DirBuilderExt, PermissionsExt, symlink};
    let root = private_state_dir();
    let actual = root.join("actual");
    std::fs::DirBuilder::new()
        .mode(0o700)
        .create(&actual)
        .unwrap();
    std::fs::DirBuilder::new()
        .mode(0o700)
        .create(actual.join("state"))
        .unwrap();
    symlink(&actual, root.join("alias")).unwrap();
    let writable = root.join("writable");
    std::fs::DirBuilder::new()
        .mode(0o700)
        .create(&writable)
        .unwrap();
    std::fs::DirBuilder::new()
        .mode(0o700)
        .create(writable.join("state"))
        .unwrap();
    std::fs::set_permissions(&writable, std::fs::Permissions::from_mode(0o777)).unwrap();
    let (_, private, world, _) = world_fixture();
    let chat = build_signed_agent_chat_request(
        "agent-0",
        "owner",
        "fixturegoal",
        &private,
        None,
        Some(2),
        Some(next_u64_id()),
    )
    .unwrap();
    let linked = pending::save_at(&chat, &world, "fixture", &root.join("alias/state"));
    let unsafe_parent = pending::save_at(&chat, &world, "fixture", &writable.join("state"));
    for path in [&linked, &unsafe_parent].into_iter().flatten() {
        pending::complete(path).unwrap();
    }
    std::fs::remove_dir_all(root).unwrap();
    assert!(
        linked.is_err(),
        "existing owned base must not bypass ancestor symlink rejection"
    );
    assert!(
        unsafe_parent.is_err(),
        "existing owned base must not bypass writable ancestor rejection"
    );
}
