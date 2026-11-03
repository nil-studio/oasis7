use super::*;

fn messages(bytes: &[u8]) -> Vec<serde_json::Value> {
    std::str::from_utf8(bytes)
        .unwrap()
        .lines()
        .filter(|line| !line.is_empty())
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}
fn hello(
    server: &mut ViewerRuntimeLiveServer,
    session: &mut RuntimeLiveSession,
    offered: bool,
) -> Vec<serde_json::Value> {
    let mut bytes = Vec::new();
    server
        .handle_request(
            ViewerRequest::HelloV2 {
                client: "control-state-test".into(),
                version: 2,
                capabilities: if offered {
                    vec!["live_control_state_v1".into()]
                } else {
                    vec![]
                },
            },
            session,
            &mut bytes,
        )
        .unwrap();
    messages(&bytes)
}

#[test]
fn live_control_state_confirms_pause_without_world_progress() {
    let mut server = ViewerRuntimeLiveServer::new(
        ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal).with_auto_play_on_connect(true),
    )
    .unwrap();
    let mut session = RuntimeLiveSession::new();
    let initial = hello(&mut server, &mut session, true);
    assert!(
        initial[0]["capabilities"]
            .as_array()
            .unwrap()
            .iter()
            .any(|cap| cap == "live_control_state_v1")
    );
    let original = initial
        .iter()
        .find(|message| message["type"] == "live_control_state")
        .expect("negotiated Hello must publish actual current control policy")["state"]
        .clone();
    assert_eq!(original["paused"], false);
    let before_time = server.world.state().time;
    let before_events = server.world.journal().len();
    let mut bytes = Vec::new();
    server
        .handle_request(
            ViewerRequest::LiveControl {
                mode: crate::viewer::LiveControl::Pause,
                request_id: Some(71),
            },
            &mut session,
            &mut bytes,
        )
        .unwrap();
    let responses = messages(&bytes);
    let paused = responses
        .iter()
        .find(|message| message["type"] == "live_control_state")
        .expect("successful Pause must confirm the original request")["state"]
        .clone();
    assert_eq!(paused["request_id"], 71);
    assert_eq!(paused["paused"], true);
    assert_eq!(paused["scope"], "viewer_new_decisions");
    assert_eq!(paused["epoch"], original["epoch"]);
    assert!(paused["revision"].as_u64().unwrap() > original["revision"].as_u64().unwrap());
    assert_eq!(server.world.state().time, before_time);
    assert_eq!(server.world.journal().len(), before_events);
    assert!(
        !responses
            .iter()
            .any(|message| message["type"] == "control_completion_ack")
    );
    let mut repeated = Vec::new();
    server
        .handle_request(
            ViewerRequest::LiveControl {
                mode: crate::viewer::LiveControl::Pause,
                request_id: Some(72),
            },
            &mut session,
            &mut repeated,
        )
        .unwrap();
    assert_eq!(
        messages(&repeated)[0]["state"]["revision"],
        paused["revision"]
    );
    assert_eq!(messages(&repeated)[0]["state"]["request_id"], 72);
    let mut another = RuntimeLiveSession::new();
    let reconnected = hello(&mut server, &mut another, true);
    assert_eq!(
        reconnected
            .iter()
            .find(|message| message["type"] == "live_control_state")
            .unwrap()["state"]["paused"],
        true
    );
    let mut other_view = RuntimeLiveSession::new();
    let _ = hello(&mut server, &mut other_view, true);
    server.resume_auto_play(&mut session).unwrap();
    let mut changed = Vec::new();
    server
        .publish_live_control_state(&mut other_view, &mut changed, None, false)
        .unwrap();
    let changed = messages(&changed);
    assert_eq!(changed[0]["state"]["paused"], false);
    assert!(changed[0]["state"].get("request_id").is_none());
    let mut duplicate = Vec::new();
    server
        .publish_live_control_state(&mut other_view, &mut duplicate, None, false)
        .unwrap();
    assert!(
        duplicate.is_empty(),
        "unchanged periodic state is not flooded"
    );
    server
        .publish_live_control_state(&mut other_view, &mut duplicate, None, true)
        .unwrap();
    assert_eq!(messages(&duplicate)[0]["state"], changed[0]["state"]);
}

#[test]
fn live_control_state_is_not_sent_to_unnegotiated_clients() {
    let mut server =
        ViewerRuntimeLiveServer::new(ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal))
            .unwrap();
    let mut session = RuntimeLiveSession::new();
    let responses = hello(&mut server, &mut session, false);
    assert!(
        !responses
            .iter()
            .any(|message| message["type"] == "live_control_state")
    );
    let mut bytes = Vec::new();
    server
        .handle_request(
            ViewerRequest::LiveControl {
                mode: crate::viewer::LiveControl::Pause,
                request_id: Some(73),
            },
            &mut session,
            &mut bytes,
        )
        .unwrap();
    assert!(bytes.is_empty());
}
