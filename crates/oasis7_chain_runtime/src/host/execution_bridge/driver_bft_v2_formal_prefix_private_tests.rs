//! V3 private reads use current World authorization, separately from the
//! historical World whose projection is published.
use super::prefix_ledger_tests::{actual_prefix, open_prefix};
use super::*;
use oasis7_client_api::{
    world_service::{FormalBftReadTargetV3, SignedReadRequest},
    world_service_wire::AgentSignerDelegationChangeV1,
};
use oasis7_game_client::{
    FormalBftReadClientConfigV3, FormalBftReadClientV3, WorldServiceClientError,
};
use std::{
    net::{SocketAddr, TcpListener},
    sync::{Arc, Mutex, mpsc},
    thread,
    time::Duration,
};

const AGENT_ID: &str = "agent-private";
const OWNER_BINDING: &str = "owner-private";
const PRIVATE_SCOPE: &str = "agent:agent-private";

#[test]
#[cfg(unix)]
fn v3_private_read_holds_and_rechecks_the_current_delegation() {
    thread::Builder::new()
        .name("formal-bft-private-auth".into())
        .stack_size(32 * 1024 * 1024)
        .spawn(private_auth_case)
        .unwrap()
        .join()
        .unwrap();
}

fn private_auth_case() {
    let owner_private = fixtures::key(120);
    let delegate_private = fixtures::key(121);
    let owner_private_hex = hex::encode(owner_private.to_bytes());
    let delegate_private_hex = hex::encode(delegate_private.to_bytes());
    let owner_public = hex::encode(owner_private.verifying_key().to_bytes());
    let delegate_public = hex::encode(delegate_private.verifying_key().to_bytes());

    let setup_owner_private = owner_private_hex.clone();
    let setup_owner_public = owner_public.clone();
    let setup_delegate_public = delegate_public.clone();
    let h = qualified_handoff_with_setup(102, move |world| {
        world.submit_action(Action::RegisterAgent {
            agent_id: AGENT_ID.into(),
            pos: oasis7_agent_api::world_action::GeoPos::new(0, 0, 0),
        });
        world.submit_action(Action::ClaimStarterOc {
            agent_id: AGENT_ID.into(),
            player_id: OWNER_BINDING.into(),
            public_key: Some(setup_owner_public.clone()),
        });
        world
            .step()
            .expect("execute real agent registration and owner claim");
        assert!(world.state().agents.contains_key(AGENT_ID));
        assert_eq!(
            world
                .current_agent_owner(AGENT_ID)
                .and_then(|owner| owner.public_key),
            Some(setup_owner_public)
        );
        world
            .install_capability_agent_identity(AGENT_ID, OWNER_BINDING, 1)
            .expect("journal the canonical agent owner identity");
        let delegation = signed_delegation(
            world,
            &setup_owner_private,
            &setup_delegate_public,
            1,
            1,
            false,
        );
        world
            .apply_agent_signer_delegation(&delegation)
            .expect("journal the owner-authorized delegate");
    });
    assert_eq!(h.world.state().time, 1);

    // The published data is a real durable, three-height cut with empty BFT
    // action lists. Authorization is read from the distinct current World.
    let build = Scratch::new();
    let prefix = actual_prefix(&h, &build);
    assert_eq!(prefix.entries.len(), 3);
    assert!(
        prefix
            .entries
            .iter()
            .all(|entry| { entry.certificate.proposal.block.actions_cbor == [0x80] })
    );
    let tip = prefix.entries.last().unwrap();
    let height = tip.certificate.proposal.block.height;
    let exact_target = FormalBftReadTargetV3 {
        height,
        block_id: tip.certificate.proposal.block.block_id().unwrap(),
    };
    let durable = Scratch::new();
    let cut = open_prefix(&h, &durable, h.reader.clone(), true)
        .unwrap()
        .install_prefix(&prefix, exact_target.height, &exact_target.block_id)
        .unwrap();
    assert_eq!(cut.world().state().time, exact_target.height);
    let publication = oasis7::world_service::formal_bft_prefix_read_service::FormalBftReadPublicationV3::new(
        oasis7::world_service::formal_bft_prefix_read_service::FormalBftReadPublicationInputV3 {
            handoff_anchor: h.anchor.clone(),
            target_config: h.target.clone(),
            exact_target: exact_target.clone(),
            certified_prefix: cut.certified_prefix(),
            committed_world: cut.world(),
            result_tree: cut.result_tree(),
        },
    )
    .expect("publish the independently replayed durable cut");

    let service_key = fixtures::key(122);
    let service_public = hex::encode(service_key.verifying_key().to_bytes());
    let current_authorization = Arc::new(Mutex::new(cut.world().clone()));
    let (entered_tx, entered_rx) = mpsc::sync_channel(1);
    let (resume_tx, resume_rx) = mpsc::channel();
    let service = Arc::new(
        crate::formal_bft_prefix_http::FormalBftReadHttpServiceV3::new(
            publication,
            service_key,
            Some(Arc::clone(&current_authorization)),
        )
        .with_private_release_pause(entered_tx, resume_rx),
    );
    let reservation = TcpListener::bind("127.0.0.1:0").unwrap();
    let bind = reservation.local_addr().unwrap();
    drop(reservation);
    let mut server = service.serve_loopback(bind).unwrap();

    let paused_client = private_client(
        &h,
        bind,
        &service_public,
        &delegate_private_hex,
        exact_target.clone(),
        PRIVATE_SCOPE,
    );
    let first_read = thread::spawn(move || paused_client.read::<serde_json::Value>());
    entered_rx
        .recv_timeout(Duration::from_secs(5))
        .expect("private response was signed before the authorization lock was released");
    assert!(
        current_authorization.try_lock().is_err(),
        "the current authorization World must remain locked through response release"
    );
    assert!(
        !first_read.is_finished(),
        "the private response must not reach the client before the auth lock is released"
    );
    resume_tx.send(()).unwrap();
    let page = first_read
        .join()
        .expect("private read thread must return")
        .expect("the currently delegated reader receives the fixed cut");
    assert_eq!(page.response().scope_id, PRIVATE_SCOPE);
    assert_eq!(page.response().head.height, exact_target.height);

    // Contention is fail-closed and visible as an authenticated 503.
    let busy_client = private_client(
        &h,
        bind,
        &service_public,
        &delegate_private_hex,
        exact_target.clone(),
        PRIVATE_SCOPE,
    );
    let held = current_authorization.lock().unwrap();
    assert_http_status(busy_client.read::<serde_json::Value>(), 503);
    drop(held);

    // Revoke the delegation in the actual current World. The historical
    // publication remains readable by the owner, but cannot revive the
    // delegate that was authorized when that history was produced.
    let revoked = signed_delegation(
        &current_authorization.lock().unwrap(),
        &owner_private_hex,
        &delegate_public,
        2,
        2,
        true,
    );
    current_authorization
        .lock()
        .unwrap()
        .apply_agent_signer_delegation(&revoked)
        .expect("journal current delegation revocation");
    let revoked_client = private_client(
        &h,
        bind,
        &service_public,
        &delegate_private_hex,
        exact_target.clone(),
        PRIVATE_SCOPE,
    );
    assert_http_status(revoked_client.read::<serde_json::Value>(), 403);

    let owner_client = private_client(
        &h,
        bind,
        &service_public,
        &owner_private_hex,
        exact_target,
        PRIVATE_SCOPE,
    );
    assert_eq!(
        owner_client
            .read::<serde_json::Value>()
            .unwrap()
            .response()
            .scope_id,
        PRIVATE_SCOPE
    );
    server.stop().unwrap();
}

fn private_client(
    h: &QualifiedH,
    bind: SocketAddr,
    service_public_key: &str,
    reader_private_key: &str,
    exact_target: FormalBftReadTargetV3,
    scope_id: &str,
) -> FormalBftReadClientV3 {
    FormalBftReadClientV3::new(
        FormalBftReadClientConfigV3 {
            endpoint: format!("http://{bind}"),
            trusted_service_public_key: service_public_key.into(),
            read_private_key_hex: reader_private_key.into(),
            timeout: Duration::from_secs(3),
            scope_id: scope_id.into(),
            exact_target,
        },
        h.anchor.clone(),
        h.target.clone(),
    )
    .unwrap()
}

fn signed_delegation(
    world: &RuntimeWorld,
    owner_private_key: &str,
    delegate_public_key: &str,
    generation: u64,
    nonce: u64,
    revoked: bool,
) -> SignedReadRequest<AgentSignerDelegationChangeV1> {
    let binding = world.current_cognition_runtime_binding().unwrap();
    oasis7::world_service::authority::sign_read_request(
        "delegation",
        AgentSignerDelegationChangeV1 {
            world_id: binding.world_id,
            branch_id: binding.branch_id,
            agent_id: AGENT_ID.into(),
            owner_binding: OWNER_BINDING.into(),
            agent_identity_generation: 1,
            generation,
            delegate_public_key: delegate_public_key.into(),
            revoked,
            nonce,
        },
        owner_private_key,
    )
    .unwrap()
}

fn assert_http_status<T>(result: Result<T, WorldServiceClientError>, expected: u16) {
    match result {
        Err(WorldServiceClientError::Rejected { status, .. }) => assert_eq!(status, expected),
        Err(error) => panic!("expected HTTP {expected}, got {error}"),
        Ok(_) => panic!("unexpected successful private read; expected HTTP {expected}"),
    }
}
