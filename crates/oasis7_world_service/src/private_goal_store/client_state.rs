//! Compatibility adapter sharing the native client's original operation cache.
use super::PrivateGoalStore;
pub use oasis7_client_api::world_service::client_state::ClientStateKind;
pub type ClientStateV1 =
    oasis7_client_api::world_service::client_state::ClientStateV1<crate::WorldServicePayloadV1>;
use oasis7_client_api::world_service::client_state::validate_client_state;
impl PrivateGoalStore {
    pub fn read_client_state(
        &self,
        kind: ClientStateKind,
        operation: &str,
    ) -> Result<Option<ClientStateV1>, String> {
        oasis7_client_storage::operation_store::read_client_state_with(
            #[cfg(unix)]
            &self.directory,
            &self.key,
            kind,
            operation,
            validate_client_state,
        )
    }
    pub fn put_client_state(
        &self,
        kind: ClientStateKind,
        operation: &str,
        record: &ClientStateV1,
    ) -> Result<ClientStateV1, String> {
        oasis7_client_storage::operation_store::put_client_state_with(
            #[cfg(unix)]
            &self.directory,
            &self.key,
            kind,
            operation,
            record,
            validate_client_state,
        )
    }
    #[cfg(all(test, unix))]
    fn client_state_name(&self, kind: ClientStateKind, operation: &str) -> Result<String, String> {
        oasis7_client_storage::operation_store::client_state_name(&self.key, kind, operation)
    }
}
#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::private_task_context::TaskContextPrepareV1;
    use crate::*;
    use oasis7_agent_api::game::{GameTaskControlV1, GameTaskFenceV1};
    use serde_json::json;

    #[test]
    fn client_original_operation_survives_restart_conflict_and_corruption() {
        let directory = std::env::temp_dir().canonicalize().unwrap().join(format!(
            "oasis7-client-state-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let signer = hex::encode([79; 32]);
        let public = authority::sign_read_request("key", (), &signer)
            .unwrap()
            .subject_public_key;
        let control: GameTaskControlV1 = serde_json::from_value(json!({"schema_revision":1,"task_id":"task","world_id":"world","genesis_digest":"genesis","branch_id":"main","reorg_epoch":"0","agent_id":"agent","owner_binding":"owner","owner_generation":"1","owner_public_key":public,"task_revision":"3","task_status":"active","latest_goal_revision":"1","active_goal_revision":"1","active_goal_reference":{"goal_revision":"1","goal_ref":"cd".repeat(32),"prepare_operation_id":"goal","publish_operation_id":"publish","activation_committed_height":"2"},"executor_epoch":"1","executor_id":"executor","executor_binding":{"executor_id":"executor","executor_public_key":public,"delegation_generation":"1","executor_epoch":"1"},"created_operation_id":"create"})).unwrap();
        let request = TaskContextPrepareV1 {
            schema_revision: 1,
            fence: GameTaskFenceV1::from_control(&control),
            client_operation_id: "stable".into(),
            client_nonce: "aa".repeat(32),
        };
        let proof = authority::sign_read_request(
            crate::private_task_context::TASK_CONTEXT_PREPARE_PATH,
            request.clone(),
            &signer,
        )
        .unwrap();
        let store = PrivateGoalStore::open(&directory, [61; 32]).unwrap();
        store
            .put_client_state(
                ClientStateKind::Prepare,
                "stable",
                &ClientStateV1::ContextPrepare(Box::new(proof.clone())),
            )
            .unwrap();
        drop(store);
        let native =
            oasis7_client_storage::operation_store::OperationStore::open(&directory, [61; 32])
                .unwrap();
        let Some(oasis7_client_storage::operation_store::ClientStateV1::ContextPrepare(
            native_proof,
        )) = native
            .read_client_state(ClientStateKind::Prepare, "stable")
            .unwrap()
        else {
            panic!("native SDK must reopen the same encrypted cache")
        };
        assert!(*native_proof == proof);
        drop(native);
        let store = PrivateGoalStore::open(&directory, [61; 32]).unwrap();
        let ClientStateV1::ContextPrepare(recovered) = store
            .read_client_state(ClientStateKind::Prepare, "stable")
            .unwrap()
            .unwrap()
        else {
            panic!("wrong record kind")
        };
        assert!(*recovered == proof);
        let mut changed = request;
        changed.client_nonce = "bb".repeat(32);
        let changed = authority::sign_read_request(
            crate::private_task_context::TASK_CONTEXT_PREPARE_PATH,
            changed,
            &signer,
        )
        .unwrap();
        let ClientStateV1::ContextPrepare(winner) = store
            .put_client_state(
                ClientStateKind::Prepare,
                "stable",
                &ClientStateV1::ContextPrepare(Box::new(changed)),
            )
            .unwrap()
        else {
            panic!("wrong record kind")
        };
        assert!(
            *winner == proof,
            "the original nonce must win after restart"
        );
        assert!(
            store
                .put_client_state(
                    ClientStateKind::Action,
                    "stable",
                    &ClientStateV1::ContextPrepare(Box::new(proof.clone()))
                )
                .is_err()
        );
        assert!(
            store
                .read_client_state(ClientStateKind::Prepare, "other")
                .unwrap()
                .is_none()
        );
        let name = store
            .client_state_name(ClientStateKind::Prepare, "stable")
            .unwrap();
        let path = directory.join(name);
        let mut bytes = std::fs::read(&path).unwrap();
        assert!(
            !bytes
                .windows(proof.request.client_nonce.len())
                .any(|part| part == proof.request.client_nonce.as_bytes())
        );
        let last = bytes.len() - 1;
        bytes[last] ^= 1;
        std::fs::write(path, bytes).unwrap();
        assert!(
            store
                .read_client_state(ClientStateKind::Prepare, "stable")
                .is_err()
        );
        drop(store);
        std::fs::remove_dir_all(directory).unwrap();
    }
}
