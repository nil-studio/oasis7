use super::*;
use crate::runtime::ReleaseSecurityPolicy;
use crate::viewer::runtime_live::chain_link::{
    chain_link_http_response_is_complete, load_chain_execution_world,
};

#[test]
fn chain_status_cannot_choose_the_observers_local_root() {
    let local_root = runtime_live_temp_dir("observer_trusted_local_root");
    let mut local_world = crate::runtime::World::new();
    local_world.submit_action(RuntimeAction::RegisterAgent {
        agent_id: "trusted-local-agent".into(),
        pos: crate::geometry::GeoPos::new(1, 2, 0),
    });
    local_world.step().unwrap();
    local_world.save_to_dir(&local_root).unwrap();
    let remote_path = runtime_live_temp_dir("observer_remote_path_must_not_be_opened");
    std::fs::remove_dir_all(&remote_path).unwrap();
    let chain_status = TestChainStatusServer::start(remote_path.clone());
    chain_status.committed_height.store(1, Ordering::SeqCst);
    let mut server = ViewerRuntimeLiveServer::new(
        ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal)
            .with_chain_status_bind(chain_status.addr.clone())
            .with_chain_execution_world_dir(local_root.clone()),
    )
    .unwrap();
    server.prime_chain_linked_runtime_for_snapshot().unwrap();
    assert!(
        server
            .world
            .state()
            .agents
            .contains_key("trusted-local-agent")
    );
    assert!(!remote_path.exists());
    std::fs::remove_dir_all(local_root).unwrap();
}

#[test]
fn chain_linked_runtime_missing_persistence_keeps_world_and_height() {
    let execution_world_dir = runtime_live_temp_dir("chain_sync_missing_persistence");
    let chain_status = TestChainStatusServer::start(execution_world_dir.clone());
    chain_status.committed_height.store(1, Ordering::SeqCst);

    let mut server = ViewerRuntimeLiveServer::new(
        ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal)
            .with_chain_status_bind(chain_status.addr.clone())
            .with_chain_execution_world_dir(execution_world_dir.clone())
            .with_chain_poll_interval(Duration::from_millis(50)),
    )
    .expect("runtime server");
    let mut session = RuntimeLiveSession::new();
    session.playing = false;
    session.subscribed.insert(ViewerStream::Events);
    session.subscribed.insert(ViewerStream::Snapshot);
    let initial_time = server.world.state().time;
    let (mut writer, peer) = test_writer_pair();

    let err = server
        .sync_chain_linked_runtime(&mut session, &mut writer)
        .expect_err("chain sync should retry when persistence files are missing");

    match err {
        ViewerRuntimeLiveServerError::Init(message) => {
            assert!(message.contains("NotReady"));
        }
        other => panic!("unexpected chain sync error: {other:?}"),
    }
    assert_eq!(server.world.state().time, initial_time);
    assert_eq!(server.last_chain_committed_height, 0);
    assert!(read_response_line(&peer, Duration::from_millis(100)).is_none());
    let feedback = server
        .latest_player_gameplay_feedback
        .as_ref()
        .expect("chain sync failure should be reflected in gameplay feedback");
    assert_eq!(feedback.action, "chain_sync");
    assert_eq!(feedback.stage, "blocked");
    assert!(
        feedback
            .reason
            .as_deref()
            .is_some_and(|reason| reason.contains("NotReady"))
    );
}

#[test]
fn chain_linked_runtime_shadow_policy_keeps_chain_failure_diagnostic_only() {
    let execution_world_dir = runtime_live_temp_dir("chain_sync_shadow_preserve_feedback");
    let chain_status = TestChainStatusServer::start(execution_world_dir.clone());
    chain_status.committed_height.store(1, Ordering::SeqCst);

    let mut server = ViewerRuntimeLiveServer::new(
        ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal)
            .with_chain_status_bind(chain_status.addr.clone())
            .with_chain_execution_world_dir(execution_world_dir.clone())
            .with_chain_link_policy(ChainLinkPolicy::Shadow)
            .with_chain_poll_interval(Duration::from_millis(50)),
    )
    .expect("runtime server");
    let prior_feedback = ViewerRuntimeLiveServer::make_player_gameplay_feedback(
        "gameplay_action",
        "accepted",
        "runtime preview accepted the local action",
        Some("continue local preview".to_string()),
        Some("agent-1".to_string()),
        None,
        None,
        1,
        1,
    );
    server.set_latest_player_gameplay_feedback(prior_feedback.clone());
    let mut session = RuntimeLiveSession::new();
    session.playing = false;
    session.subscribed.insert(ViewerStream::Events);
    session.subscribed.insert(ViewerStream::Snapshot);
    let initial_time = server.world.state().time;
    let (mut writer, peer) = test_writer_pair();

    let err = server
        .sync_chain_linked_runtime(&mut session, &mut writer)
        .expect_err("shadow chain sync should still report diagnostics to the caller");

    match err {
        ViewerRuntimeLiveServerError::Init(message) => {
            assert!(message.contains("NotReady"));
        }
        other => panic!("unexpected chain sync error: {other:?}"),
    }
    assert_eq!(server.world.state().time, initial_time);
    assert_eq!(server.last_chain_committed_height, 0);
    assert!(read_response_line(&peer, Duration::from_millis(100)).is_none());
    let feedback = server
        .latest_player_gameplay_feedback
        .as_ref()
        .expect("shadow sync should preserve prior local gameplay feedback");
    assert_eq!(feedback.action, prior_feedback.action);
    assert_eq!(feedback.stage, prior_feedback.stage);
    assert_ne!(feedback.action, "chain_sync");
}

#[test]
fn chain_linked_runtime_missing_persistence_without_subscription_does_not_poison_feedback() {
    let execution_world_dir = runtime_live_temp_dir("chain_sync_missing_persistence_unsubscribed");
    let chain_status = TestChainStatusServer::start(execution_world_dir.clone());
    chain_status.committed_height.store(1, Ordering::SeqCst);

    let mut server = ViewerRuntimeLiveServer::new(
        ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal)
            .with_chain_status_bind(chain_status.addr.clone())
            .with_chain_execution_world_dir(execution_world_dir.clone())
            .with_chain_poll_interval(Duration::from_millis(50)),
    )
    .expect("runtime server");
    let mut session = RuntimeLiveSession::new();
    session.playing = false;
    let initial_time = server.world.state().time;
    let (mut writer, peer) = test_writer_pair();

    let err = server
        .sync_chain_linked_runtime(&mut session, &mut writer)
        .expect_err("chain sync should still fail when persistence files are missing");

    match err {
        ViewerRuntimeLiveServerError::Init(message) => {
            assert!(message.contains("NotReady"));
        }
        other => panic!("unexpected chain sync error: {other:?}"),
    }
    assert_eq!(server.world.state().time, initial_time);
    assert!(server.latest_player_gameplay_feedback.is_none());
    assert!(read_response_line(&peer, Duration::from_millis(100)).is_none());
}

#[test]
fn chain_linked_runtime_shadow_policy_keeps_chain_failures_out_of_gameplay_feedback() {
    let execution_world_dir = runtime_live_temp_dir("chain_sync_shadow_missing_persistence");
    let chain_status = TestChainStatusServer::start(execution_world_dir.clone());
    chain_status.committed_height.store(1, Ordering::SeqCst);

    let mut server = ViewerRuntimeLiveServer::new(
        ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal)
            .with_chain_status_bind(chain_status.addr.clone())
            .with_chain_execution_world_dir(execution_world_dir.clone())
            .with_chain_link_policy(ChainLinkPolicy::Shadow)
            .with_chain_poll_interval(Duration::from_millis(50)),
    )
    .expect("runtime server");
    let mut session = RuntimeLiveSession::new();
    session.playing = false;
    session.subscribed.insert(ViewerStream::Events);
    session.subscribed.insert(ViewerStream::Snapshot);
    let initial_time = server.world.state().time;
    let (mut writer, peer) = test_writer_pair();

    let err = server
        .sync_chain_linked_runtime(&mut session, &mut writer)
        .expect_err("shadow chain sync should still report the sync error to the caller");

    match err {
        ViewerRuntimeLiveServerError::Init(message) => {
            assert!(message.contains("NotReady"));
        }
        other => panic!("unexpected chain sync error: {other:?}"),
    }
    assert_eq!(server.world.state().time, initial_time);
    assert_eq!(server.last_chain_committed_height, 0);
    assert!(server.latest_player_gameplay_feedback.is_none());
    assert!(read_response_line(&peer, Duration::from_millis(100)).is_none());
}

#[test]
fn chain_link_http_response_waits_for_full_content_length_body() {
    let partial = b"HTTP/1.1 200 OK\r\nContent-Length: 11\r\n\r\n{\"ok\":";
    assert!(
        !chain_link_http_response_is_complete(partial)
            .expect("partial response should parse headers")
    );

    let complete = b"HTTP/1.1 200 OK\r\nContent-Length: 11\r\n\r\n{\"ok\":true}";
    assert!(
        chain_link_http_response_is_complete(complete).expect("complete response should parse")
    );
}

#[test]
fn chain_link_http_response_without_content_length_waits_for_eof() {
    let response = b"HTTP/1.1 200 OK\r\nConnection: close\r\n\r\n{}";
    assert!(
        !chain_link_http_response_is_complete(response)
            .expect("response without content length should parse")
    );
}

#[test]
fn chain_linked_runtime_dev_local_policy_preserves_economic_config_before_verifying_tick_consensus()
{
    let execution_world_dir = runtime_live_temp_dir("chain_sync_dev_local_main_token_normalize");
    let mut execution_world = crate::runtime::World::new_production_hardened();
    execution_world.submit_action(RuntimeAction::RegisterAgent {
        agent_id: "chain-agent".to_string(),
        pos: crate::geometry::GeoPos::new(1, 2, 0),
    });
    execution_world.step().expect("advance execution world");
    execution_world
        .save_to_dir(execution_world_dir.as_path())
        .expect("persist execution world");

    let world = load_chain_execution_world(
        execution_world_dir.as_path(),
        ReleaseSecurityPolicy::default(),
    )
    .expect("dev-local viewer load should normalize persisted execution world");

    assert_eq!(
        world.main_token_config(),
        execution_world.main_token_config()
    );
    world
        .verify_tick_consensus_chain()
        .expect("viewer world should retain a valid tick consensus chain");
}
