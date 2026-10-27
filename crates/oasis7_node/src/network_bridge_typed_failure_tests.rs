use super::*;
use crate::network_error_classification::{persisted_network_reason, replication_network_failure};
use oasis7_proto::distributed::DistributedErrorCode as Code;
use oasis7_proto::network_failure::{
    FailureCause as Cause, FailureEvidence as Evidence, FailureOrigin as Origin, NetworkFailure,
};

#[test]
fn six_origins_cross_three_protocols_without_diagnostic_policy_coupling() {
    for protocol in [
        REPLICATION_GET_HEAD_PROTOCOL,
        REPLICATION_FETCH_COMMIT_PROTOCOL,
        crate::replication::REPLICATION_FETCH_BLOB_PROTOCOL,
    ] {
        for (origin, cause) in [
            (Origin::LocalTransport, Cause::Timeout),
            (Origin::RemoteResponse, Cause::MissingHandler),
            (Origin::Dht, Cause::Timeout),
            (Origin::ProviderRouting, Cause::ProviderExhausted),
            (Origin::Publish, Cause::Unknown),
            (Origin::DecodeValidation, Cause::Validation),
        ] {
            let facts = NetworkFailure::observed(origin, cause, Some(protocol));
            for diagnostic in ["diagnostic wording one", "completely different diagnostic"] {
                let source = WorldError::NetworkProtocolUnavailable {
                    protocol: diagnostic.into(),
                }
                .with_network_failure(facts.clone());
                let err = network_err_for_protocol(protocol, source);
                assert_eq!(replication_network_failure(&err), facts);
                assert_eq!(
                    replication_network_error_is_availability_gap(&err),
                    facts.is_connection_gap()
                );
                assert_eq!(
                    replication_network_error_is_unsupported_protocol(&err, protocol),
                    cause == Cause::MissingHandler
                );
                assert!(!replication_network_error_is_timeout_protocol(
                    &err,
                    &format!("{protocol}/other")
                ));
                let persisted = persisted_network_reason(format!("blocked: {err}"), &err);
                // Same snapshot string field, restored as legacy evidence, preserving decisions.
                let restored = NodeError::Replication { reason: persisted };
                let restored_facts = replication_network_failure(&restored);
                assert_eq!(restored_facts.cause, cause);
                assert_eq!(restored_facts.evidence, Evidence::LegacyInterpretation);
                assert_eq!(restored_facts.requested_protocol.as_deref(), Some(protocol));
                assert_eq!(
                    replication_network_error_is_availability_gap(&restored),
                    replication_network_error_is_availability_gap(&err)
                );
            }
        }
    }
}

#[test]
fn legal_legacy_inputs_match_typed_bridge_and_keep_business_unsupported_separate() {
    for protocol in [
        REPLICATION_GET_HEAD_PROTOCOL,
        REPLICATION_FETCH_COMMIT_PROTOCOL,
        crate::replication::REPLICATION_FETCH_BLOB_PROTOCOL,
    ] {
        for (code, message, cause) in [
            (
                Code::ErrUnsupported,
                protocol.to_owned(),
                Cause::MissingHandler,
            ),
            (
                Code::ErrUnsupported,
                "remote peer declined request".into(),
                Cause::Rejected,
            ),
            (
                Code::ErrNotAvailable,
                format!("request failed: Timeout protocol={protocol}"),
                Cause::Timeout,
            ),
            (
                Code::ErrNotAvailable,
                "remote temporarily unavailable".into(),
                Cause::Unavailable,
            ),
            (Code::ErrNotFound, "absent".into(), Cause::NotFound),
            (
                Code::ErrRateLimited,
                "window exhausted".into(),
                Cause::RateLimited,
            ),
        ] {
            let old = WorldError::NetworkRequestFailed {
                code,
                message,
                retryable: true,
            };
            let expected = old.network_failure(Some(protocol));
            assert_eq!(expected.cause, cause);
            let legacy = network_err_for_protocol(protocol, old.clone());
            let typed = network_err_for_protocol(protocol, old.with_network_failure(expected));
            assert_eq!(
                replication_network_failure(&legacy),
                replication_network_failure(&typed)
            );
            assert_eq!(
                world_head_lookup_can_fallback(&legacy),
                world_head_lookup_can_fallback(&typed)
            );
        }
    }
}

#[test]
fn denials_unknown_and_publish_never_gain_availability_from_claims() {
    for code in [
        Code::ErrUnauthorized,
        Code::ErrBadRequest,
        Code::ErrStateMismatch,
        Code::ErrInvalidHash,
    ] {
        let err = network_err_for_protocol(
            REPLICATION_GET_HEAD_PROTOCOL,
            WorldError::NetworkRequestFailed {
                code,
                message: format!(
                    "request failed: Timeout protocol={REPLICATION_GET_HEAD_PROTOCOL}"
                ),
                retryable: true,
            },
        );
        assert!(!world_head_lookup_can_fallback(&err));
        let restored = NodeError::Replication {
            reason: persisted_network_reason(format!("{err}"), &err),
        };
        assert!(!world_head_lookup_can_fallback(&restored));
    }
}

#[test]
fn timeout_priority_survives_provider_gap_and_snapshot_restore() {
    let protocol = crate::replication::REPLICATION_FETCH_BLOB_PROTOCOL;
    let err = network_err_for_protocol(
        protocol,
        WorldError::Io("changed".into()).with_network_failure(NetworkFailure::observed(
            Origin::LocalTransport,
            Cause::Timeout,
            Some(protocol),
        )),
    );
    let restored = NodeError::Replication {
        reason: persisted_network_reason("saved".into(), &err),
    };
    let gap = network_err_for_protocol(
        protocol,
        WorldError::Io("changed".into()).with_network_failure(NetworkFailure::observed(
            Origin::ProviderRouting,
            Cause::ConnectionGap,
            Some(protocol),
        )),
    );
    assert!(crate::network_error_classification::replication_network_error_should_keep_timeout_over_provider_gap(Some(&restored), &gap, protocol));
}

#[test]
fn malformed_response_and_unknown_wire_code_remain_rejections() {
    for payload in [
        br#"{"code":"ERR_FUTURE","message":"request failed: Timeout","retryable":true}"#.as_slice(),
        b"malformed",
    ] {
        assert!(
            serde_json::from_slice::<oasis7_proto::distributed::ErrorResponse>(payload).is_err()
        );
        let err = network_err_for_protocol(
            REPLICATION_GET_HEAD_PROTOCOL,
            WorldError::Serde("malformed".into()),
        );
        assert!(!world_head_lookup_can_fallback(&err));
    }
}
#[test]
fn broad_protocol_matching_and_contradictory_persisted_facts_are_not_eligibility() {
    let protocol = crate::replication::REPLICATION_FETCH_BLOB_PROTOCOL;
    let business = WorldError::NetworkRequestFailed {
        code: Code::ErrUnsupported,
        message: format!("business operation {protocol} is unsupported"),
        retryable: true,
    };
    assert!(
        !crate::replication_probe_gate::should_fallback_provider_aware_replication_request(
            &network_err_for_protocol(protocol, business)
        )
    );
    let collision = NodeError::Replication {
        reason: format!(
            "replication network request failed: kind=timeout protocol={protocol}/evil detail=timed out"
        ),
    };
    assert!(!replication_network_error_is_timeout_protocol(
        &collision, protocol
    ));
    let mut facts =
        NetworkFailure::observed(Origin::RemoteResponse, Cause::Rejected, Some(protocol));
    facts.connection_gap = true;
    let err = NodeError::ReplicationNetwork {
        reason: "denial".into(),
        failure: Box::new(facts),
    };
    let saved = NodeError::Replication {
        reason: persisted_network_reason("blocked".into(), &err),
    };
    assert!(!replication_network_error_is_availability_gap(&saved));
}

#[test]
fn persisted_facts_win_over_diagnostic_denial_words_and_bad_suffix_is_unknown() {
    let protocol = REPLICATION_FETCH_COMMIT_PROTOCOL;
    let err = network_err_for_protocol(
        protocol,
        WorldError::Io("ErrUnauthorized kind=invalid_hash diagnostic only".into())
            .with_network_failure(NetworkFailure::observed(
                Origin::LocalTransport,
                Cause::Timeout,
                Some(protocol),
            )),
    );
    let restored = NodeError::Replication {
        reason: persisted_network_reason(format!("blocked: {err}"), &err),
    };
    assert_eq!(replication_network_failure(&restored).cause, Cause::Timeout);
    assert!(replication_network_error_is_timeout_protocol(
        &restored, protocol
    ));
    for suffix in [
        " [network-failure-v1={bad}]",
        " [network-failure-v2={}]",
        " [network-failure-v1={\"cause\":\"Future\"}]",
    ] {
        let malformed = NodeError::Replication {
            reason: format!(
                "replication network availability gap: request failed: Timeout protocol={protocol}{suffix}"
            ),
        };
        assert_eq!(
            replication_network_failure(&malformed).cause,
            Cause::Unknown
        );
        assert!(!replication_network_error_is_availability_gap(&malformed));
    }
}

#[test]
fn checkpoint_missing_policy_uses_typed_facts_and_preserves_legacy_hashes() {
    use crate::PosNodeEngine;
    let protocol = crate::replication::REPLICATION_FETCH_BLOB_PROTOCOL;
    for (origin, cause) in [
        (Origin::RemoteResponse, Cause::Rejected),
        (Origin::DecodeValidation, Cause::Validation),
        (Origin::LocalTransport, Cause::Unknown),
    ] {
        for diagnostic in [
            "execution checkpoint blob not found hash=forged-peer-hash",
            "gap sync height 1 blob not found for hash forged-peer-hash",
        ] {
            let err = NodeError::ReplicationNetwork {
                reason: diagnostic.into(),
                failure: Box::new(NetworkFailure::observed(origin, cause, Some(protocol))),
            };
            assert!(
                !PosNodeEngine::high_replication_checkpoint_probe_can_continue(&err),
                "{cause:?} must not continue from diagnostic"
            );
            assert_eq!(
                PosNodeEngine::high_replication_checkpoint_closure_missing_reason(&err),
                None
            );
        }
    }
    let original = "renamed diagnostic; peer claimed hash=forged-peer-hash";
    let missing = NodeError::ReplicationNetwork {
        reason: original.into(),
        failure: Box::new(NetworkFailure::observed(
            Origin::RemoteResponse,
            Cause::NotFound,
            Some(protocol),
        )),
    };
    assert!(PosNodeEngine::high_replication_checkpoint_probe_can_continue(&missing));
    let summary = PosNodeEngine::high_replication_checkpoint_closure_missing_reason(&missing)
        .expect("typed blob not found qualifies");
    assert!(
        summary.starts_with("execution checkpoint blob not found hash=unknown; original_error=")
    );
    assert!(summary.contains(original));
    let wrong_protocol = NodeError::ReplicationNetwork {
        reason: "execution checkpoint blob not found hash=forged".into(),
        failure: Box::new(NetworkFailure::observed(
            Origin::RemoteResponse,
            Cause::NotFound,
            Some(REPLICATION_FETCH_COMMIT_PROTOCOL),
        )),
    };
    assert!(!PosNodeEngine::high_replication_checkpoint_probe_can_continue(&wrong_protocol));
    assert_eq!(
        PosNodeEngine::high_replication_checkpoint_closure_missing_reason(&wrong_protocol),
        None
    );
    for (diagnostic, expected) in [
        (
            "execution checkpoint blob not found hash=real-local-hash",
            "execution checkpoint blob not found hash=real-local-hash",
        ),
        (
            "gap sync height 1 blob not found for hash real-local-hash",
            "execution checkpoint blob not found hash=real-local-hash",
        ),
    ] {
        let legacy = NodeError::Replication {
            reason: diagnostic.into(),
        };
        assert!(PosNodeEngine::high_replication_checkpoint_probe_can_continue(&legacy));
        assert_eq!(
            PosNodeEngine::high_replication_checkpoint_closure_missing_reason(&legacy).as_deref(),
            Some(expected)
        );
    }
}

#[test]
fn review_local_missing_blob_cooldown_survives_persisted_facts() {
    use crate::node_engine_replication_provider_route::{
        replication_gap_sync_provider_blob_route_blocked,
        replication_gap_sync_provider_blob_route_blocked_in_cooldown,
    };
    let err = NodeError::Replication {
        reason: "gap sync height 7 blob not found for hash abc".into(),
    };
    assert!(replication_gap_sync_provider_blob_route_blocked(&err));
    let summary = persisted_network_reason(format!("attempt 1/3 failed: {err}"), &err);
    let blocked = format!(
        "replication gap sync provider route blocked: missing commit height 7 repair_attempt={summary}"
    );
    let restored: String = serde_json::from_str(&serde_json::to_string(&blocked).unwrap()).unwrap();
    assert!(
        replication_gap_sync_provider_blob_route_blocked_in_cooldown(
            Some(7),
            Some(&restored),
            Some(1000),
            7,
            30999
        )
    );
    assert!(
        !replication_gap_sync_provider_blob_route_blocked_in_cooldown(
            Some(7),
            Some(&restored),
            Some(1000),
            7,
            31000
        )
    );
    let facts = replication_network_failure(&NodeError::Replication { reason: restored });
    assert_eq!(facts.cause, Cause::NotFound);
    assert_eq!(
        facts.requested_protocol.as_deref(),
        Some(crate::replication::REPLICATION_FETCH_BLOB_PROTOCOL)
    );
    assert_eq!(facts.evidence, Evidence::LegacyInterpretation);
    assert!(!facts.is_connection_gap());
}

#[test]
fn review_missing_handler_claim_is_bound_to_requested_protocol() {
    let requested = crate::replication::REPLICATION_FETCH_BLOB_PROTOCOL;
    for named in [
        requested.to_owned(),
        REPLICATION_FETCH_COMMIT_PROTOCOL.to_owned(),
        format!("{requested}/other"),
    ] {
        for detail in [
            named.clone(),
            format!("{named} unsupported by remote"),
            format!("handler missing: {named}"),
            format!("libp2p-replication handler missing: {named}"),
        ] {
            let err = network_err_for_protocol(
                requested,
                WorldError::NetworkRequestFailed {
                    code: Code::ErrUnsupported,
                    message: detail.clone(),
                    retryable: false,
                },
            );
            let expected = named == requested;
            assert_eq!(
                replication_network_error_is_unsupported_protocol(&err, requested),
                expected,
                "{detail}"
            );
            assert_eq!(
                crate::replication_probe_gate::should_fallback_provider_aware_replication_request(
                    &err
                ),
                expected,
                "{detail}"
            );
            assert!(
                !crate::PosNodeEngine::high_replication_checkpoint_probe_can_continue(&err),
                "missing handler is not a checkpoint missing-blob or commit-timeout fact"
            );
        }
    }
}
