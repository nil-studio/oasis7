//! Local CLI presentation after captured-context verification; not a wire DTO.
use oasis7_client_api::world_service::task_context::NativeTaskAuthorityV1;
use serde_json::Value;

pub(super) fn annotate_verified_native_context(
    mut display: Value,
    authority: Option<&NativeTaskAuthorityV1>,
) -> Value {
    // Unlike legacy observation Vec defaults, this authority field is required.
    if let (Some(object), Some(authority)) = (display.as_object_mut(), authority) {
        object.insert(
            "native_action_validation".into(),
            Value::from(if authority.native_delegation_authorizations.is_empty() {
                "default_action_validation"
            } else {
                "grant_scoped"
            }),
        );
    }
    display
}

#[cfg(test)]
mod tests {
    use super::*;
    use oasis7_client_api::world_service::task_context::GoalExecutionSnapshotV1;
    use serde_json::json;

    fn snapshot() -> GoalExecutionSnapshotV1 {
        let snapshot: GoalExecutionSnapshotV1 = serde_json::from_slice(include_bytes!(
            "../testdata/task-context/native-snapshot.json"
        ))
        .unwrap();
        snapshot.validate().unwrap();
        snapshot
    }
    #[test]
    fn required_native_context_list_confirms_default_validation() {
        let snapshot = snapshot();
        let authority = snapshot.native_authority.as_ref().unwrap();
        assert!(authority.native_delegation_authorizations.is_empty());
        let display = annotate_verified_native_context(
            json!({"native_authority": authority}),
            Some(authority),
        );
        assert_eq!(
            display["native_action_validation"],
            "default_action_validation"
        );
        assert_eq!(
            display["native_authority"],
            serde_json::to_value(authority).unwrap()
        );
    }
    #[test]
    fn missing_native_context_grant_field_is_not_silently_empty() {
        let mut value = serde_json::to_value(snapshot()).unwrap();
        value["native_authority"]
            .as_object_mut()
            .unwrap()
            .remove("native_delegation_authorizations");
        assert!(serde_json::from_value::<GoalExecutionSnapshotV1>(value).is_err());
    }
    #[test]
    fn nonempty_native_context_keeps_revoked_grants_scoped() {
        let mut snapshot = snapshot();
        let authority = snapshot.native_authority.as_mut().unwrap();
        authority.native_delegation_authorizations.push(serde_json::from_value(json!({
            "grant":{"grant_id":"grant","source_id":"source","issuer_id":"issuer","owner_id":"owner",
            "organization_id":null,"agent_id":authority.fence.agent_id,"object_id":"object","action_kinds":["MoveAgent"],
            "revision":1,"period_id":"period","valid_from_tick":0,"valid_until_tick":0,"limit_units":7,
            "resource_kind":"electricity","revoked":true},"cost_units":0,"spent_units":0,"remaining_units":7
        })).unwrap());
        let display = annotate_verified_native_context(
            json!({"native_authority":authority}),
            Some(authority),
        );
        assert_eq!(display["native_action_validation"], "grant_scoped");
    }
    #[test]
    fn provider_local_display_bytes_are_unchanged() {
        let display = json!({"capabilities":{"snapshot_id":"provider"},"native_authority":null});
        let before = serde_json::to_vec(&display).unwrap();
        assert_eq!(
            serde_json::to_vec(&annotate_verified_native_context(display, None)).unwrap(),
            before
        );
    }
}
