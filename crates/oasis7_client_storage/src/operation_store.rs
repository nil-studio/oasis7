//! Durable device recovery operations, using the private-goal namespace unchanged.
use oasis7_client_api::world_service::client_state::validate_client_state;
pub use oasis7_client_api::world_service::client_state::{ClientStateKind, ClientStateV1};
use serde::Serialize;
use std::path::Path;
use zeroize::Zeroizing;

pub struct OperationStore {
    #[cfg(unix)]
    directory: cap_std::fs::Dir,
    key: Zeroizing<[u8; 32]>,
}
impl OperationStore {
    pub fn open(root: &Path, key: [u8; 32]) -> Result<Self, String> {
        let key = Zeroizing::new(key);
        #[cfg(unix)]
        {
            let directory = crate::private_storage::open_root(root)?;
            crate::private_storage::initialize_key(&directory, &key)?;
            Ok(Self { directory, key })
        }
        #[cfg(not(unix))]
        {
            let _ = (root, key);
            Err("private client operation storage unsupported".into())
        }
    }
    /// Read the immutable feedback journal using the caller's payload validator.
    /// This checks storage integrity; the validator establishes record semantics.
    pub fn read_feedback_state<T: serde::de::DeserializeOwned>(
        &self,
        operation: &str,
        validate: impl Fn(&ClientStateV1<T>, ClientStateKind) -> Result<(), String>,
    ) -> Result<Option<ClientStateV1<T>>, String> {
        read_client_state_with(
            #[cfg(unix)]
            &self.directory,
            &self.key,
            ClientStateKind::FeedbackAcceptance,
            operation,
            validate,
        )
    }
    /// Publish a feedback journal candidate and return the immutable winner.
    /// The caller must validate its payload and compare the returned original.
    pub fn put_feedback_state<T: Serialize + serde::de::DeserializeOwned>(
        &self,
        operation: &str,
        record: &ClientStateV1<T>,
        validate: impl Fn(&ClientStateV1<T>, ClientStateKind) -> Result<(), String>,
    ) -> Result<ClientStateV1<T>, String> {
        put_client_state_with(
            #[cfg(unix)]
            &self.directory,
            &self.key,
            ClientStateKind::FeedbackAcceptance,
            operation,
            record,
            validate,
        )
    }
    pub fn read_client_state(
        &self,
        kind: ClientStateKind,
        operation: &str,
    ) -> Result<Option<ClientStateV1>, String> {
        read_client_state_with(
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
        put_client_state_with(
            #[cfg(unix)]
            &self.directory,
            &self.key,
            kind,
            operation,
            record,
            validate_client_state,
        )
    }
}
#[cfg(unix)]
const DOMAIN: &str = "oasis7/private-task-client-state/v1";
#[cfg(unix)]
const MAGIC: &[u8; 8] = b"O7CLIENT";
#[cfg(unix)]
const MAXIMUM: usize = oasis7_agent_api::game::MAX_REQUEST_BYTES + 1024;

#[cfg(unix)]
pub fn client_state_name(
    key_bytes: &[u8; 32],
    kind: ClientStateKind,
    operation: &str,
) -> Result<String, String> {
    if operation.trim().is_empty() || operation.len() > 256 {
        return Err("invalid client operation identity".into());
    }
    let key = Zeroizing::new(blake3::derive_key(
        "oasis7/private-task-client-index/v1",
        key_bytes.as_slice(),
    ));
    let scope = oasis7_wasm_abi::encode_canonical_cbor(&(kind, operation))
        .map_err(|_| "client operation encoding failed")?;
    Ok(format!("client-{}", blake3::keyed_hash(&key, &scope)))
}
pub fn read_client_state_with<Payload: serde::de::DeserializeOwned>(
    #[cfg(unix)] directory: &cap_std::fs::Dir,
    key: &[u8; 32],
    kind: ClientStateKind,
    operation: &str,
    validate: impl Fn(&ClientStateV1<Payload>, ClientStateKind) -> Result<(), String>,
) -> Result<Option<ClientStateV1<Payload>>, String> {
    #[cfg(unix)]
    {
        let name = client_state_name(key, kind, operation)?;
        let aad =
            serde_cbor::to_vec(&(DOMAIN, &name)).map_err(|_| "client operation encoding failed")?;
        let Some(bytes) = crate::private_storage::read_bounded_record(
            directory, key, &name, MAGIC, &aad, MAXIMUM,
        )?
        else {
            return Ok(None);
        };
        let record: ClientStateV1<Payload> =
            serde_cbor::from_slice(&bytes).map_err(|_| "client operation recovery required")?;
        validate(&record, kind)?;
        Ok(Some(record))
    }
    #[cfg(not(unix))]
    {
        let _ = (kind, operation);
        Err("private client operation storage unsupported".into())
    }
}
/// Returns the immutable winner. Callers check exact input compatibility;
/// a concurrent first nonce is retained rather than silently replaced.
pub fn put_client_state_with<Payload: Serialize + serde::de::DeserializeOwned>(
    #[cfg(unix)] directory: &cap_std::fs::Dir,
    key: &[u8; 32],
    kind: ClientStateKind,
    operation: &str,
    record: &ClientStateV1<Payload>,
    validate: impl Fn(&ClientStateV1<Payload>, ClientStateKind) -> Result<(), String>,
) -> Result<ClientStateV1<Payload>, String> {
    validate(record, kind)?;
    #[cfg(unix)]
    {
        let name = client_state_name(key, kind, operation)?;
        let aad =
            serde_cbor::to_vec(&(DOMAIN, &name)).map_err(|_| "client operation encoding failed")?;
        let bytes = Zeroizing::new(
            serde_cbor::to_vec(record).map_err(|_| "client operation encoding failed")?,
        );
        crate::private_storage::publish_bounded_record(
            directory, key, &name, MAGIC, &aad, &bytes, MAXIMUM,
        )?;
        read_client_state_with(directory, key, kind, operation, validate)?
            .ok_or_else(|| "client operation recovery required".into())
    }
    #[cfg(not(unix))]
    {
        let _ = (operation, key);
        Err("private client operation storage unsupported".into())
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use oasis7_agent_api::game::{GameTaskControlV1, GameTaskFenceV1};
    use oasis7_client_api::world_service as authority;
    use oasis7_client_api::world_service::task_context::TaskContextPrepareV1;
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
            oasis7_client_api::world_service::task_context::TASK_CONTEXT_PREPARE_PATH,
            request.clone(),
            &signer,
        )
        .unwrap();
        let store = OperationStore::open(&directory, [61; 32]).unwrap();
        store
            .put_client_state(
                ClientStateKind::Prepare,
                "stable",
                &ClientStateV1::ContextPrepare(Box::new(proof.clone())),
            )
            .unwrap();
        drop(store);
        let store = OperationStore::open(&directory, [61; 32]).unwrap();
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
            oasis7_client_api::world_service::task_context::TASK_CONTEXT_PREPARE_PATH,
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
        let name = client_state_name(&[61; 32], ClientStateKind::Prepare, "stable").unwrap();
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

#[path = "native_run_budget.rs"]
mod native_run_budget;
