use super::*;
use crate::codec::*;

fn projection() -> WorldServiceProjection {
    WorldServiceProjection::from(WorldServiceProjectionData {
        world_rule_status: None,
        formal_commit_header: None,
        canonical_agent_owner: None,
        canonical_agent_chat: None,
        game_task_control: None,
        native_delegation_authorizations: Vec::new(),
        feedback_history: None,
        state: WorldState::default(),
        events: Vec::new(),
        runtime_binding: None,
        agent_context: None,
        scheduler_wakes: Vec::new(),
        continuations: Vec::new(),
        cognition_leases: Vec::new(),
        continuation_contexts: Default::default(),
    })
}

#[test]
fn concrete_codecs_preserve_legacy_bytes_and_optional_field_defaults() {
    let projection = projection();
    let json = projection_to_json(&projection).unwrap();
    assert_eq!(json, serde_json::to_vec(&projection).unwrap());
    let cbor = projection_to_cbor(&projection).unwrap();
    assert_eq!(cbor, serde_cbor::to_vec(&projection).unwrap());
    for decoded in [
        projection_from_json(&json).unwrap(),
        projection_from_cbor(&cbor).unwrap(),
    ] {
        assert_eq!(projection_to_json(&decoded).unwrap(), json);
    }
    let value = projection_to_json_value(&projection).unwrap();
    for absent in [
        "world_rule_status",
        "formal_commit_header",
        "canonical_agent_owner",
        "canonical_agent_chat",
        "game_task_control",
        "native_delegation_authorizations",
        "feedback_history",
    ] {
        assert!(value.get(absent).is_none(), "unexpected field {absent}");
    }
    let mut future = value;
    future
        .as_object_mut()
        .unwrap()
        .insert("future_metadata".into(), serde_json::json!(true));
    assert_eq!(
        projection_to_json(&projection_from_json_value(future).unwrap()).unwrap(),
        json
    );
}

#[test]
fn concrete_codecs_preserve_duplicate_missing_and_malformed_errors() {
    use serde_json::error::Category;

    // Use the pre-split fixture, not the current encoder, as the wire input.
    let historical = include_str!("../fixtures/public.json");
    assert!(projection_from_json(historical.as_bytes()).is_ok());
    let body = historical.trim_end().strip_suffix('}').unwrap();
    let duplicate = format!(r#"{body},"state":null}}"#);

    for (case, invalid, expected, reason) in [
        (
            "wrong shape",
            b"null".as_slice(),
            Category::Data,
            "invalid type",
        ),
        ("missing field", b"{}", Category::Data, "missing field"),
        ("truncated object", b"{", Category::Eof, "EOF"),
        (
            "duplicate state",
            duplicate.as_bytes(),
            Category::Data,
            "duplicate field `state`",
        ),
    ] {
        let actual = projection_from_json(invalid).unwrap_err();
        assert_eq!(actual.classify(), expected, "{case}: {actual}");
        assert!(actual.to_string().contains(reason), "{case}: {actual}");
        let legacy = serde_json::from_slice::<WorldServiceProjection>(invalid).unwrap_err();
        assert_eq!(actual.to_string(), legacy.to_string(), "{case}");
    }
    for (case, invalid, truncated) in [
        ("empty input", b"".as_slice(), true),
        ("unexpected break", &[0xff], false),
        ("truncated map key", &[0xa1, 0x65, b's'], true),
    ] {
        let actual = projection_from_cbor(invalid).unwrap_err();
        assert_eq!(actual.is_eof(), truncated, "{case}: {actual}");
        let legacy = serde_cbor::from_slice::<WorldServiceProjection>(invalid).unwrap_err();
        assert_eq!(actual.to_string(), legacy.to_string(), "{case}");
    }
}

#[test]
fn pre_split_projection_and_response_fixtures_reencode_without_wire_drift() {
    for (json, cbor, signing) in [
        (
            include_bytes!("../fixtures/public.json").as_slice(),
            include_bytes!("../fixtures/public.cbor").as_slice(),
            include_bytes!("../fixtures/public.signing").as_slice(),
        ),
        (
            include_bytes!("../fixtures/owner.json").as_slice(),
            include_bytes!("../fixtures/owner.cbor").as_slice(),
            include_bytes!("../fixtures/owner.signing").as_slice(),
        ),
        (
            include_bytes!("../fixtures/delegate.json").as_slice(),
            include_bytes!("../fixtures/delegate.cbor").as_slice(),
            include_bytes!("../fixtures/delegate.signing").as_slice(),
        ),
    ] {
        for value in [
            projection_from_json(json).unwrap(),
            projection_from_cbor(cbor).unwrap(),
        ] {
            assert_eq!(projection_to_json(&value).unwrap(), json);
            assert_eq!(projection_to_cbor(&value).unwrap(), cbor);
            assert_eq!(
                oasis7_client_api::world_service::signing_bytes("view", &value).unwrap(),
                signing
            );
        }
    }
    let json = include_bytes!("../fixtures/response.json");
    let cbor = include_bytes!("../fixtures/response.cbor");
    for value in [
        response_from_json(json).unwrap(),
        response_from_cbor(cbor).unwrap(),
    ] {
        assert_eq!(response_to_json(&value).unwrap(), json);
        assert_eq!(response_to_cbor(&value).unwrap(), cbor);
        assert_eq!(
            projection_revision(&value.view).unwrap(),
            value.version.projection_revision
        );
        assert_eq!(
            oasis7_client_api::world_service::signing_bytes("view", &value).unwrap(),
            include_bytes!("../fixtures/response.signing")
        );
    }
}

#[test]
fn concrete_changes_codec_preserves_page_bytes_and_parser_errors() {
    use oasis7_client_api::world_service::{
        FormalBusinessChangesCursorV1, FormalReadAnchorRefV1, ReadWorldViewRequest,
    };
    let target_view = response_from_json(include_bytes!("../fixtures/response.json")).unwrap();
    let from_anchor = FormalReadAnchorRefV1 {
        position: target_view.version.commit.position,
        record_id: target_view.version.commit.execution_block_hash.clone(),
        active_authority_id: "a".repeat(64),
    };
    let cursor = FormalBusinessChangesCursorV1 {
        stream_id: target_view.continuation.stream_id.clone(),
        scope_id: target_view.continuation.scope_id.clone(),
        event_id_era: target_view.continuation.era,
        sequence: target_view.continuation.sequence,
        commit: target_view.version.commit.clone(),
    };
    let page = ChangesPage {
        schema_version: 1,
        from_anchor: from_anchor.clone(),
        from_exclusive: cursor.clone(),
        to_inclusive: cursor.clone(),
        next_cursor: cursor.clone(),
        target_request: ReadWorldViewRequest {
            observation_nonce: None,
            formal_anchor: Some(from_anchor),
            contract_version: target_view.contract_version,
            world: target_view.version.commit.world.clone(),
            scope_id: cursor.scope_id,
            min_commit: None,
            fixed_commit: None,
            deadline_unix_ms: None,
        },
        target_view,
        changes: Vec::new(),
    };
    let json = changes_page_json(&page).unwrap();
    assert_eq!(json, serde_json::to_vec(&page).unwrap());
    assert_eq!(
        changes_page_json(&changes_page_from_json(&json).unwrap()).unwrap(),
        json
    );
    let source = std::str::from_utf8(&json).unwrap();
    let duplicate = format!("{{\"schema_version\":1,{rest}", rest = &source[1..]);
    for invalid in [b"null".as_slice(), b"{}", b"{", duplicate.as_bytes()] {
        assert_eq!(
            changes_page_from_json(invalid).unwrap_err().to_string(),
            serde_json::from_slice::<ChangesPage>(invalid)
                .unwrap_err()
                .to_string()
        );
    }
}

#[test]
fn transparent_adapter_preserves_shared_data_layout_and_commitments() {
    use oasis7_client_api::formal_projection::ProjectionAudienceV1;
    assert_eq!(
        std::mem::size_of::<WorldServiceProjection>(),
        std::mem::size_of::<WorldServiceProjectionData>()
    );
    assert_eq!(
        std::mem::align_of::<WorldServiceProjection>(),
        std::mem::align_of::<WorldServiceProjectionData>()
    );
    type Column = serde_json::Value;
    assert_eq!(
        std::mem::size_of::<WorldServiceProjection<Column>>(),
        std::mem::size_of::<WorldServiceProjectionData<Column>>()
    );
    for (json, cbor) in [
        (
            include_bytes!("../fixtures/public.json").as_slice(),
            include_bytes!("../fixtures/public.cbor").as_slice(),
        ),
        (
            include_bytes!("../fixtures/owner.json").as_slice(),
            include_bytes!("../fixtures/owner.cbor").as_slice(),
        ),
        (
            include_bytes!("../fixtures/delegate.json").as_slice(),
            include_bytes!("../fixtures/delegate.cbor").as_slice(),
        ),
    ] {
        for data in [
            serde_json::from_slice::<WorldServiceProjectionData>(json).unwrap(),
            serde_cbor::from_slice::<WorldServiceProjectionData>(cbor).unwrap(),
        ] {
            let projection = WorldServiceProjection::from(data);
            assert_eq!(serde_json::to_vec(projection.data()).unwrap(), json);
            assert_eq!(serde_cbor::to_vec(projection.data()).unwrap(), cbor);
            assert_eq!(projection_to_json(&projection).unwrap(), json);
            assert_eq!(projection_to_cbor(&projection).unwrap(), cbor);
            assert_eq!(
                format!("{projection:?}"),
                format!("{:?}", projection.data())
            );
            assert_eq!(
                projection_revision(&projection).unwrap(),
                oasis7_client_api::world_service::request_digest("projection", projection.data())
                    .unwrap()
            );
            for audience in [
                ProjectionAudienceV1::public(),
                ProjectionAudienceV1::agent("agent-a", "a".repeat(64)).unwrap(),
            ] {
                assert_eq!(
                    projection_leaf(&audience, &projection).unwrap(),
                    oasis7_client_api::formal_projection::projection_leaf(
                        &audience,
                        projection.data()
                    )
                    .unwrap()
                );
            }
        }
    }
}

#[test]
fn transparent_adapter_keeps_generic_serde_numbers_defaults_and_errors() {
    type Data = WorldServiceProjectionData<serde_json::Value>;
    type Adapter = WorldServiceProjection<serde_json::Value>;
    let mut data = oasis7_world_state::observation_projection::original_formatter::base_data_v1(
        serde_json::json!({"negative_zero": -0.0, "fraction": 1.25, "array": [null, true, "雪"]}),
        Some(serde_json::json!({"fraction": -2.5})),
        None,
        None,
        None,
    );
    data.continuation_contexts.insert(
        "context".into(),
        WorldServiceContinuationContext {
            baseline_observation_digest: "baseline".into(),
            goal_digest: "goal".into(),
            policy_digest: "policy".into(),
            policy_revision: 3,
            precondition_summary: "summary".into(),
            precondition_digest: "precondition".into(),
        },
    );
    let json = serde_json::to_vec(&data).unwrap();
    let cbor = serde_cbor::to_vec(&data).unwrap();
    let adapter = Adapter::from(data);
    assert_eq!(serde_json::to_vec(&adapter).unwrap(), json);
    assert_eq!(serde_cbor::to_vec(&adapter).unwrap(), cbor);
    assert_eq!(serde_json::to_vec(&adapter.into_data()).unwrap(), json);
    for adapter in [
        serde_json::from_slice::<Adapter>(&json).unwrap(),
        serde_cbor::from_slice::<Adapter>(&cbor).unwrap(),
    ] {
        assert_eq!(serde_json::to_vec(&adapter).unwrap(), json);
        assert_eq!(serde_cbor::to_vec(&adapter).unwrap(), cbor);
        assert!(
            adapter.state["negative_zero"]
                .as_f64()
                .unwrap()
                .is_sign_negative()
        );
    }
    let mut future: serde_json::Value = serde_json::from_slice(&json).unwrap();
    future
        .as_object_mut()
        .unwrap()
        .insert("future".into(), true.into());
    assert_eq!(
        serde_json::to_vec(&serde_json::from_value::<Adapter>(future).unwrap()).unwrap(),
        json
    );
    let source = std::str::from_utf8(&json).unwrap();
    let duplicate = format!("{{\"state\":{{}},{rest}", rest = &source[1..]);
    for invalid in [b"null".as_slice(), b"{}", b"{", duplicate.as_bytes()] {
        assert_eq!(
            serde_json::from_slice::<Adapter>(invalid)
                .unwrap_err()
                .to_string(),
            serde_json::from_slice::<Data>(invalid)
                .unwrap_err()
                .to_string()
        );
    }
    for invalid in [b"".as_slice(), &[0xff], &[0xa1, 0x65, b's']] {
        assert_eq!(
            serde_cbor::from_slice::<Adapter>(invalid)
                .unwrap_err()
                .to_string(),
            serde_cbor::from_slice::<Data>(invalid)
                .unwrap_err()
                .to_string()
        );
    }
}
