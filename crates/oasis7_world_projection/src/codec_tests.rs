use super::*;
use crate::codec::*;

fn projection() -> WorldServiceProjection {
    WorldServiceProjection {
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
    }
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
    let json = projection_to_json(&projection()).unwrap();
    let source = std::str::from_utf8(&json).unwrap();
    let duplicate = format!("{{\"state\":{{}},{rest}", rest = &source[1..]);
    for invalid in [b"null".as_slice(), b"{}", b"{", duplicate.as_bytes()] {
        let actual = projection_from_json(invalid).unwrap_err().to_string();
        let legacy = serde_json::from_slice::<WorldServiceProjection>(invalid)
            .unwrap_err()
            .to_string();
        assert_eq!(actual, legacy);
    }
    for invalid in [b"".as_slice(), &[0xff], &[0xa1, 0x65, b's']] {
        assert_eq!(
            projection_from_cbor(invalid).unwrap_err().to_string(),
            serde_cbor::from_slice::<WorldServiceProjection>(invalid)
                .unwrap_err()
                .to_string()
        );
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
