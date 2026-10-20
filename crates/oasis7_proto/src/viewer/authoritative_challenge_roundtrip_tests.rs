use super::*;

#[test]
fn viewer_response_round_trip_authoritative_challenge_ack() {
    let response = ViewerResponse::<
        serde_json::Value,
        serde_json::Value,
        serde_json::Value,
        serde_json::Value,
        u64,
    >::AuthoritativeChallengeAck {
        ack: AuthoritativeChallengeAck {
            challenge_id: "challenge-1".to_string(),
            batch_id: "batch-1".to_string(),
            watcher_id: "watcher-1".to_string(),
            status: AuthoritativeChallengeStatus::ResolvedFraudSlashed,
            submitted_at_tick: 40,
            resolved_at_tick: Some(42),
            slash_applied: true,
            slash_reason: Some("state_root_mismatch".to_string()),
        },
    };
    let json = serde_json::to_string(&response).expect("serialize response");
    let parsed: ViewerResponse<
        serde_json::Value,
        serde_json::Value,
        serde_json::Value,
        serde_json::Value,
        u64,
    > = serde_json::from_str(&json).expect("deserialize response");
    assert_eq!(parsed, response);
}

#[test]
fn authoritative_challenge_ack_round_trips_without_a_slash() {
    for status in [
        AuthoritativeChallengeStatus::Challenged,
        AuthoritativeChallengeStatus::ResolvedNoFraud,
    ] {
        let ack = AuthoritativeChallengeAck {
            challenge_id: "challenge-1".to_string(),
            batch_id: "batch-1".to_string(),
            watcher_id: "watcher-1".to_string(),
            status,
            submitted_at_tick: 40_u64,
            resolved_at_tick: None,
            slash_applied: false,
            slash_reason: None,
        };
        let json = serde_json::to_string(&ack).unwrap();
        assert!(!json.contains("slash_applied"));
        let parsed: AuthoritativeChallengeAck<u64> = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, ack);
    }
}
