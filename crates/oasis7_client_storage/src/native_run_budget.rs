//! Client-local physical model-call reservations. Not a world task database.
use super::OperationStore;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct BudgetIdentity {
    schema_revision: u32,
    run_id: String,
    binding_digest: String,
    max_calls: u32,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct BudgetCounter {
    identity: BudgetIdentity,
    spent: u32,
}
const MAX_CALLS: u32 = 1024;
#[cfg(unix)]
const MAGIC: &[u8; 8] = b"O7BUDG01";
#[cfg(unix)]
const MAX_BYTES: usize = 2048;
impl OperationStore {
    /// Fix immutable run limits and read the number already reserved. A crash
    /// after reservation consumes that call, even if its response was lost.
    pub fn initialize_native_run(
        &self,
        run_id: &str,
        binding: &Value,
        max_calls: u32,
    ) -> Result<u32, String> {
        self.native_run_budget(run_id, binding, max_calls, None)
    }
    /// Durable increment completes before the caller may invoke the provider.
    pub fn reserve_native_model_call(
        &self,
        run_id: &str,
        binding: &Value,
        max_calls: u32,
        operation: &str,
    ) -> Result<u32, String> {
        if operation.trim().is_empty() || operation.len() > 256 {
            return Err("invalid native model operation".into());
        }
        self.native_run_budget(run_id, binding, max_calls, Some(operation))
    }
    fn native_run_budget(
        &self,
        run_id: &str,
        binding: &Value,
        max_calls: u32,
        operation: Option<&str>,
    ) -> Result<u32, String> {
        if run_id.is_empty()
            || run_id.len() > 128
            || run_id.chars().any(|c| c.is_whitespace() || c.is_control())
            || max_calls == 0
            || max_calls > MAX_CALLS
            || !binding.is_object()
            || serde_json::to_vec(binding)
                .map_err(|_| "invalid native run binding")?
                .len()
                > 8192
        {
            return Err("invalid native run budget".into());
        }
        let identity = BudgetIdentity {
            schema_revision: 1,
            run_id: run_id.into(),
            max_calls,
            binding_digest: oasis7_client_api::world_service::request_digest(
                "native-model-run-binding/v1",
                binding,
            )?,
        };
        #[cfg(unix)]
        {
            use crate::private_storage as storage;
            use zeroize::Zeroizing;
            let name = |kind: &str| -> Result<String, String> {
                let index_key = Zeroizing::new(blake3::derive_key(
                    "oasis7/native-model-budget-index/v1",
                    self.key.as_slice(),
                ));
                let scope = oasis7_wasm_abi::encode_canonical_cbor(&(run_id, kind))
                    .map_err(|_| "native budget encoding failed")?;
                Ok(format!(
                    "model-budget-{}",
                    blake3::keyed_hash(&index_key, &scope)
                ))
            };
            let config_name = name("config")?;
            let counter_name = name("counter")?;
            let lock_name = name("lock")?;
            let (_lock, created) = storage::lock_record(&self.directory, &self.key, &lock_name)?;
            let aad = |name: &str| {
                serde_cbor::to_vec(&("oasis7/native-model-budget/v1", name))
                    .map_err(|_| "native budget encoding failed")
            };
            let config_aad = aad(&config_name)?;
            let counter_aad = aad(&counter_name)?;
            let config = storage::read_bounded_record(
                &self.directory,
                &self.key,
                &config_name,
                MAGIC,
                &config_aad,
                MAX_BYTES,
            )?;
            let counter = storage::read_bounded_record(
                &self.directory,
                &self.key,
                &counter_name,
                MAGIC,
                &counter_aad,
                MAX_BYTES,
            )?;
            let mut counter = match (config, counter) {
                (None, None) if created => {
                    let bytes = Zeroizing::new(
                        serde_cbor::to_vec(&identity)
                            .map_err(|_| "native budget encoding failed")?,
                    );
                    if !storage::publish_bounded_record(
                        &self.directory,
                        &self.key,
                        &config_name,
                        MAGIC,
                        &config_aad,
                        &bytes,
                        MAX_BYTES,
                    )? {
                        return Err("native budget initialization conflict".into());
                    }
                    let value = BudgetCounter {
                        identity: identity.clone(),
                        spent: 0,
                    };
                    let bytes = Zeroizing::new(
                        serde_cbor::to_vec(&value).map_err(|_| "native budget encoding failed")?,
                    );
                    storage::replace_bounded_record(
                        &self.directory,
                        &self.key,
                        &counter_name,
                        MAGIC,
                        &counter_aad,
                        &bytes,
                        MAX_BYTES,
                    )?;
                    value
                }
                (Some(config), Some(counter)) => {
                    let fixed: BudgetIdentity = serde_cbor::from_slice(&config)
                        .map_err(|_| "native budget recovery required")?;
                    let value: BudgetCounter = serde_cbor::from_slice(&counter)
                        .map_err(|_| "native budget recovery required")?;
                    if fixed != identity || value.identity != identity || value.spent > max_calls {
                        return Err("native run limits changed or budget corrupt".into());
                    }
                    value
                }
                _ => return Err("native model budget missing; recovery required".into()),
            };
            if operation.is_some() {
                if counter.spent >= max_calls {
                    return Err("native model budget exhausted".into());
                }
                counter.spent += 1;
                let bytes = Zeroizing::new(
                    serde_cbor::to_vec(&counter).map_err(|_| "native budget encoding failed")?,
                );
                storage::replace_bounded_record(
                    &self.directory,
                    &self.key,
                    &counter_name,
                    MAGIC,
                    &counter_aad,
                    &bytes,
                    MAX_BYTES,
                )?;
            }
            Ok(counter.spent)
        }
        #[cfg(not(unix))]
        {
            let _ = (identity, operation);
            Err("private native model budget storage unsupported".into())
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::sync::Arc;
    fn root() -> std::path::PathBuf {
        let mut bytes = [0; 16];
        getrandom::fill(&mut bytes).unwrap();
        std::env::temp_dir()
            .canonicalize()
            .unwrap()
            .join(format!("oasis7-model-budget-{}", hex::encode(bytes)))
    }
    #[test]
    fn reservations_survive_restart_crash_and_concurrent_callers() {
        let root = root();
        let binding = serde_json::json!({"model":"fixture","max_turns":4});
        let store = Arc::new(OperationStore::open(&root, [7; 32]).unwrap());
        assert_eq!(store.initialize_native_run("run", &binding, 8).unwrap(), 0);
        let threads: Vec<_> = (0..8)
            .map(|_| {
                let store = store.clone();
                let binding = binding.clone();
                std::thread::spawn(move || {
                    store
                        .reserve_native_model_call("run", &binding, 8, "run:0")
                        .unwrap()
                })
            })
            .collect();
        let mut slots: Vec<_> = threads.into_iter().map(|t| t.join().unwrap()).collect();
        slots.sort();
        assert_eq!(slots, (1..=8).collect::<Vec<_>>());
        assert!(
            store
                .reserve_native_model_call("run", &binding, 8, "run:1")
                .is_err()
        );
        drop(store);
        let reopened = OperationStore::open(&root, [7; 32]).unwrap();
        assert_eq!(
            reopened.initialize_native_run("run", &binding, 8).unwrap(),
            8
        );
        assert!(reopened.initialize_native_run("run", &binding, 9).is_err());
        assert!(
            reopened
                .initialize_native_run("run", &serde_json::json!({"model":"other"}), 8)
                .is_err()
        );
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn corrupt_or_missing_budget_does_not_reset_spending() {
        for remove in [false, true] {
            let root = root();
            let binding = serde_json::json!({"model":"fixture"});
            let store = OperationStore::open(&root, [7; 32]).unwrap();
            store
                .reserve_native_model_call("run", &binding, 2, "run:0")
                .unwrap();
            let files: Vec<_> = std::fs::read_dir(&root)
                .unwrap()
                .map(|e| e.unwrap().path())
                .filter(|p| {
                    p.file_name()
                        .unwrap()
                        .to_string_lossy()
                        .starts_with("model-budget-")
                })
                .collect();
            for path in files {
                if std::fs::metadata(&path).unwrap().len() > 0 {
                    if remove {
                        std::fs::remove_file(path).unwrap();
                    } else {
                        std::fs::write(path, b"corrupt").unwrap();
                    }
                }
            }
            assert!(store.initialize_native_run("run", &binding, 2).is_err());
            assert!(
                store
                    .reserve_native_model_call("run", &binding, 2, "run:1")
                    .is_err()
            );
            std::fs::remove_dir_all(root).unwrap();
        }
    }
}
