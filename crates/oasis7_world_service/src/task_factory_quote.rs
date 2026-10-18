//! Compatibility exports for the unique client quote contract.
pub use oasis7_client_api::world_service::task_factory_quote::*;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SignedReadRequest;
    use crate::authority::sign_read_request;
    use crate::private_task_context::TASK_CONTEXT_READ_PATH;
    use crate::private_task_context::TaskContextGetV1;
    use oasis7_agent_api::game::TaskContextIdentityV1;
    use oasis7_agent_api::game::{
        DecimalU64, GameExecutorBindingV1, GameGoalReferenceV1, GameTaskFenceV1, GameTaskStatusV1,
    };
    use oasis7_agent_api::world_cognition::RuntimeCognitionBaseBindingV1;

    fn request() -> SignedReadRequest<TaskFactoryQuoteRequestV1> {
        let key = hex::encode([73; 32]);
        let public = sign_read_request("key", (), &key)
            .unwrap()
            .subject_public_key;
        let context = TaskContextIdentityV1 {
            schema_revision: 1,
            fence: GameTaskFenceV1 {
                world_id: "world".into(),
                genesis_digest: "genesis".into(),
                branch_id: "main".into(),
                reorg_epoch: DecimalU64(0),
                agent_id: "agent".into(),
                owner_binding: "owner".into(),
                owner_generation: DecimalU64(1),
                owner_public_key: public.clone(),
                task_id: "task".into(),
                task_revision: DecimalU64(3),
                task_status: GameTaskStatusV1::Active,
                latest_goal_revision: DecimalU64(1),
                active_goal_revision: DecimalU64(1),
                active_goal_reference: Some(GameGoalReferenceV1 {
                    goal_revision: DecimalU64(1),
                    goal_ref: "ab".repeat(32),
                    prepare_operation_id: "prepare".into(),
                    publish_operation_id: "publish".into(),
                    activation_committed_height: DecimalU64(4),
                }),
                executor_epoch: DecimalU64(1),
                executor_binding: Some(GameExecutorBindingV1 {
                    executor_id: "executor".into(),
                    executor_public_key: public,
                    delegation_generation: DecimalU64(1),
                    executor_epoch: DecimalU64(1),
                }),
            },
            agent_session_id: "session".into(),
            agent_turn_id: "turn".into(),
            decision_request_id: "decision".into(),
            retry_seq: 1,
            transport_attempt: 1,
            request_commitment: format!("blake3:{}", "11".repeat(32)),
            context_commitment: format!("blake3:{}", "22".repeat(32)),
            provider_invocation_commitment: format!("blake3:{}", "33".repeat(32)),
            valid_until_tick: DecimalU64(16),
        };
        let context_proof = sign_read_request(
            TASK_CONTEXT_READ_PATH,
            TaskContextGetV1 {
                schema_revision: 1,
                context,
                prepare_operation_id: "prepare-context".into(),
            },
            &key,
        )
        .unwrap();
        sign_read_request(
            TASK_FACTORY_QUOTE_PATH,
            TaskFactoryQuoteRequestV1 {
                schema_revision: 1,
                context_proof,
                expected_base_binding: RuntimeCognitionBaseBindingV1 {
                    world_id: "world".into(),
                    branch_id: "main".into(),
                    finality_epoch: 0,
                    finality_block_hash: None,
                    finality_status: "pending".into(),
                    base_tick: 0,
                    base_world_hash: format!("blake3:{}", "55".repeat(32)),
                    reorg_epoch: 0,
                    runtime_manifest_hash: format!("blake3:{}", "66".repeat(32)),
                },
                factory_id: "factory".into(),
                factory_kind: "smelter".into(),
                site_id: "site".into(),
            },
            &key,
        )
        .unwrap()
    }
    #[test]
    fn task_factory_quote_requires_matching_strict_nested_signatures_and_history() {
        let original = request();
        assert!(verify_factory_quote_request(&original).is_ok());
        for field in ["site_id", "factory_id", "factory_kind"] {
            let mut tampered = original.clone();
            match field {
                "site_id" => tampered.request.site_id = "foreign-site".into(),
                "factory_id" => tampered.request.factory_id = "foreign-factory".into(),
                _ => tampered.request.factory_kind = "assembler".into(),
            }
            assert!(verify_factory_quote_request(&tampered).is_err());
        }
        let wrong_path = sign_read_request(
            super::super::task_recipe_quote::TASK_RECIPE_QUOTE_PATH,
            original.request.clone(),
            &hex::encode([73; 32]),
        )
        .unwrap();
        assert!(verify_factory_quote_request(&wrong_path).is_err());
        let outsider = sign_read_request(
            TASK_FACTORY_QUOTE_PATH,
            original.request.clone(),
            &hex::encode([74; 32]),
        )
        .unwrap();
        assert!(verify_factory_quote_request(&outsider).is_err());
        let mut changed = original.request.clone();
        changed.context_proof.request.prepare_operation_id = "changed".into();
        let changed =
            sign_read_request(TASK_FACTORY_QUOTE_PATH, changed, &hex::encode([73; 32])).unwrap();
        assert!(verify_factory_quote_request(&changed).is_err());
        let mut changed = original.request;
        changed.expected_base_binding.branch_id = "different".into();
        let changed =
            sign_read_request(TASK_FACTORY_QUOTE_PATH, changed, &hex::encode([73; 32])).unwrap();
        assert!(verify_factory_quote_request(&changed).is_err());
    }
    #[test]
    fn task_factory_quote_wire_rejects_caller_plan_authorization_and_duplicate_fields() {
        let original = request();
        for field in ["spec", "grant", "cost", "authorization", "action"] {
            let mut value = serde_json::to_value(&original).unwrap();
            value["request"][field] = serde_json::json!({});
            assert!(
                oasis7_agent_api::game::parse_bounded_json::<
                    SignedReadRequest<TaskFactoryQuoteRequestV1>,
                >(&serde_json::to_vec(&value).unwrap(), 4096)
                .is_err()
            );
        }
        let encoded = serde_json::to_string(&original).unwrap().replace(
            "\"site_id\":\"site\"",
            "\"site_id\":\"site\",\"site_id\":\"other\"",
        );
        assert!(
            oasis7_agent_api::game::parse_bounded_json::<
                SignedReadRequest<TaskFactoryQuoteRequestV1>,
            >(encoded.as_bytes(), 4096)
            .is_err()
        );
    }
}
