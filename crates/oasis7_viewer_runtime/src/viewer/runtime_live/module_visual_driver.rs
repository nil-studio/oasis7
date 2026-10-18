//! Test-only runtime helper for producing module visual events through the
//! real module execution path.
//!
//! `runtime_live.rs` includes this module only under `cfg(test)`.  It keeps a
//! reproducible real-runtime regression available in a fresh ephemeral world
//! without exposing an environment gate, file-backed command channel, or
//! synthetic module installation hook in release builds.

use crate::runtime::{World as RuntimeWorld, WorldEventBody as RuntimeWorldEventBody};
use oasis7_agent_api::world_entity::{ModuleVisualAnchor, ModuleVisualEntity};
use oasis7_simulator::WorldModel;
use oasis7_wasm_abi::{
    ModuleCallFailure, ModuleCallRequest, ModuleEmit, ModuleOutput, ModuleSandbox,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

const DRIVER_SCHEMA_VERSION: u32 = 1;
const UPSERT_KIND: &str = "module_visual_entity_upserted";
const REMOVE_KIND: &str = "module_visual_entity_removed";

#[derive(Debug, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case")]
enum DriverCommand {
    Upsert {
        entity_id: String,
        module_id: String,
        anchor: ModuleVisualAnchor,
    },
    Remove {
        entity_id: String,
    },
}

#[derive(Debug, Serialize)]
struct DriverAck {
    schema_version: u32,
    status: &'static str,
    operation: String,
    world_id: String,
    reorg_epoch: u64,
    event_sequence: Option<u64>,
    runtime_module_id: String,
    event_kind: Option<String>,
    event_payload: Option<Value>,
    module_visual_entity_id: Option<String>,
    snapshot_module_visual_entity: Option<ModuleVisualEntity>,
    error: Option<String>,
}

#[derive(Debug)]
pub(super) struct RuntimeModuleVisualDriver {
    command_path: PathBuf,
    ack_path: PathBuf,
    read_offset: u64,
    runtime_module_id: String,
}

#[derive(Debug)]
struct EmissionSandbox {
    output: ModuleOutput,
}

impl ModuleSandbox for EmissionSandbox {
    fn call(&mut self, _request: &ModuleCallRequest) -> Result<ModuleOutput, ModuleCallFailure> {
        Ok(self.output.clone())
    }
}

impl RuntimeModuleVisualDriver {
    pub(super) fn poll(
        &mut self,
        world: &mut RuntimeWorld,
        seed_model: &mut Option<WorldModel>,
        world_id: &str,
        reorg_epoch: u64,
    ) -> Result<Vec<crate::runtime::WorldEvent>, String> {
        let mut file = OpenOptions::new()
            .read(true)
            .open(&self.command_path)
            .map_err(|err| format!("read runtime module visual driver: {err}"))?;
        let mut contents = String::new();
        file.read_to_string(&mut contents)
            .map_err(|err| format!("read runtime module visual driver contents: {err}"))?;
        let start = usize::try_from(self.read_offset)
            .map_err(|_| "runtime module visual driver offset overflow".to_string())?;
        if start > contents.len() {
            return Err("runtime module visual driver was truncated while running".to_string());
        }
        let pending = &contents[start..];
        let complete_len = pending.rfind('\n').map(|index| index + 1).unwrap_or(0);
        let mut emitted_events = Vec::new();
        for line in pending[..complete_len].lines() {
            if line.trim().is_empty() {
                continue;
            }
            let command = match serde_json::from_str::<DriverCommand>(line) {
                Ok(command) => command,
                Err(err) => {
                    self.write_ack(&DriverAck {
                        schema_version: DRIVER_SCHEMA_VERSION,
                        status: "error",
                        operation: "invalid_command".to_string(),
                        world_id: world_id.to_string(),
                        reorg_epoch,
                        event_sequence: None,
                        runtime_module_id: self.runtime_module_id.clone(),
                        event_kind: None,
                        event_payload: None,
                        module_visual_entity_id: None,
                        snapshot_module_visual_entity: None,
                        error: Some(err.to_string()),
                    })?;
                    continue;
                }
            };
            if let Some(event) = self.execute(command, world, seed_model, world_id, reorg_epoch)? {
                emitted_events.push(event);
            }
        }
        self.read_offset = self.read_offset.saturating_add(complete_len as u64);
        Ok(emitted_events)
    }

    fn execute(
        &mut self,
        command: DriverCommand,
        world: &mut RuntimeWorld,
        seed_model: &mut Option<WorldModel>,
        world_id: &str,
        reorg_epoch: u64,
    ) -> Result<Option<crate::runtime::WorldEvent>, String> {
        let (operation, entity_id, anchor, event_kind, payload) = match command {
            DriverCommand::Upsert {
                entity_id,
                module_id,
                anchor,
            } => {
                let entity = ModuleVisualEntity {
                    entity_id: entity_id.trim().to_string(),
                    module_id: module_id.trim().to_string(),
                    kind: "runtime_driver".to_string(),
                    label: Some("Runtime module visual driver".to_string()),
                    anchor,
                }
                .sanitized();
                if entity.entity_id.is_empty() || entity.module_id.is_empty() {
                    return self
                        .write_error(
                            world_id,
                            reorg_epoch,
                            "upsert",
                            "entity_id and module_id must be non-empty",
                        )
                        .map(|()| None);
                }
                let payload = json!({ "entity": entity });
                (
                    "upsert".to_string(),
                    entity.entity_id.clone(),
                    Some(entity),
                    UPSERT_KIND,
                    payload,
                )
            }
            DriverCommand::Remove { entity_id } => {
                let entity_id = entity_id.trim().to_string();
                if entity_id.is_empty() {
                    return self
                        .write_error(
                            world_id,
                            reorg_epoch,
                            "remove",
                            "entity_id must be non-empty",
                        )
                        .map(|()| None);
                }
                (
                    "remove".to_string(),
                    entity_id.clone(),
                    None,
                    REMOVE_KIND,
                    json!({ "entity_id": entity_id }),
                )
            }
        };

        let before_len = world.journal().events.len();
        let trace_id = format!("runtime-module-visual-driver-{}-{before_len}", entity_id);
        let mut sandbox = EmissionSandbox {
            output: ModuleOutput {
                new_state: None,
                effects: Vec::new(),
                emits: vec![ModuleEmit {
                    kind: event_kind.to_string(),
                    payload: payload.clone(),
                }],
                tick_lifecycle: None,
                output_bytes: 0,
            },
        };
        let call = world.execute_module_call(
            self.runtime_module_id.as_str(),
            trace_id,
            serde_json::to_vec(&payload).map_err(|err| err.to_string())?,
            &mut sandbox,
        );
        if let Err(err) = call {
            return self
                .write_error(
                    world_id,
                    reorg_epoch,
                    operation.as_str(),
                    &format!("{err:?}"),
                )
                .map(|()| None);
        }

        let event = world.journal().events[before_len..]
            .iter()
            .find(|event| {
                matches!(
                    &event.body,
                    RuntimeWorldEventBody::ModuleEmitted(emitted)
                        if emitted.kind == event_kind
                )
            })
            .ok_or_else(|| {
                "runtime module call completed without ModuleEmitted journal event".to_string()
            })?;

        let snapshot_entity = match (&mut *seed_model, anchor) {
            (seed_model @ None, Some(entity)) => {
                let mut model = WorldModel::default();
                model
                    .module_visual_entities
                    .insert(entity.entity_id.clone(), entity.clone());
                *seed_model = Some(model);
                Some(entity)
            }
            (Some(model), Some(entity)) => {
                model
                    .module_visual_entities
                    .insert(entity.entity_id.clone(), entity.clone());
                Some(entity)
            }
            (None, None) => None,
            (Some(model), None) => {
                model.module_visual_entities.remove(&entity_id);
                None
            }
        };

        self.write_ack(&DriverAck {
            schema_version: DRIVER_SCHEMA_VERSION,
            status: "applied",
            operation,
            world_id: world_id.to_string(),
            reorg_epoch,
            event_sequence: Some(event.id),
            runtime_module_id: self.runtime_module_id.clone(),
            event_kind: Some(event_kind.to_string()),
            event_payload: Some(payload),
            module_visual_entity_id: Some(entity_id),
            snapshot_module_visual_entity: snapshot_entity,
            error: None,
        })?;
        Ok(Some(event.clone()))
    }

    fn write_error(
        &self,
        world_id: &str,
        reorg_epoch: u64,
        operation: &str,
        error: &str,
    ) -> Result<(), String> {
        self.write_ack(&DriverAck {
            schema_version: DRIVER_SCHEMA_VERSION,
            status: "error",
            operation: operation.to_string(),
            world_id: world_id.to_string(),
            reorg_epoch,
            event_sequence: None,
            runtime_module_id: self.runtime_module_id.clone(),
            event_kind: None,
            event_payload: None,
            module_visual_entity_id: None,
            snapshot_module_visual_entity: None,
            error: Some(error.to_string()),
        })
    }

    fn write_ack(&self, ack: &DriverAck) -> Result<(), String> {
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.ack_path)
            .map_err(|err| format!("open runtime module visual driver ack: {err}"))?;
        serde_json::to_writer(&mut file, ack).map_err(|err| err.to_string())?;
        file.write_all(b"\n")
            .map_err(|err| format!("write runtime module visual driver ack: {err}"))
    }
}

fn ack_path_for(command_path: &Path) -> PathBuf {
    let mut ack_path = command_path.as_os_str().to_os_string();
    ack_path.push(".ack.jsonl");
    PathBuf::from(ack_path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::WorldEventBody;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn install_visual_driver(
        world: &mut RuntimeWorld,
    ) -> Result<String, crate::runtime::WorldError> {
        use crate::runtime::{
            ModuleKind, ModuleLimits, ModuleManifest, ModuleRecord, ModuleRegistry, ModuleRole,
        };
        const MODULE_ID: &str = "runtime.qa.module_visual_driver";
        const VERSION: &str = "1.0.0";
        const ARTIFACT: &[u8] = b"oasis7-runtime-module-visual-driver-v1";

        let wasm_hash = {
            use sha2::Digest;
            hex::encode(sha2::Sha256::digest(ARTIFACT))
        };
        world.register_module_artifact(wasm_hash.clone(), ARTIFACT)?;
        let mut snapshot = world.snapshot();
        let manifest = ModuleManifest {
            module_id: MODULE_ID.to_string(),
            name: "Runtime module visual QA driver".to_string(),
            version: VERSION.to_string(),
            kind: ModuleKind::Pure,
            role: ModuleRole::AgentInternal,
            wasm_hash,
            interface_version: "wasm-1".to_string(),
            exports: vec!["call".to_string()],
            subscriptions: Vec::new(),
            required_caps: Vec::new(),
            abi_contract: Default::default(),
            artifact_identity: None,
            limits: ModuleLimits {
                max_mem_bytes: 1024,
                max_gas: 10_000,
                max_call_rate: u32::MAX,
                max_output_bytes: 4096,
                max_effects: 0,
                max_emits: 1,
            },
        };
        let key = ModuleRegistry::record_key(MODULE_ID, VERSION);
        snapshot.module_registry.records.insert(
            key,
            ModuleRecord {
                manifest,
                registered_at: world.state().time,
                registered_by: "runtime-module-visual-driver".to_string(),
                audit_event_id: None,
            },
        );
        snapshot
            .module_registry
            .active
            .insert(MODULE_ID.to_string(), VERSION.to_string());
        *world = crate::runtime_host::world::from_snapshot(snapshot, world.journal().clone())?;
        Ok(MODULE_ID.to_string())
    }

    fn test_path() -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "oasis7-runtime-module-visual-driver-{}-{nonce}.jsonl",
            std::process::id()
        ))
    }

    fn append_command(path: &Path, command: Value) {
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .expect("driver command file");
        serde_json::to_writer(&mut file, &command).expect("command json");
        file.write_all(b"\n").expect("command newline");
    }

    #[test]
    fn fresh_world_does_not_bootstrap_test_driver_module() {
        let world = crate::runtime_host::world::new();
        assert!(
            !world
                .module_registry()
                .active
                .contains_key("runtime.qa.module_visual_driver")
        );
        assert!(
            !world
                .module_registry()
                .records
                .keys()
                .any(|key| key.starts_with("runtime.qa.module_visual_driver@"))
        );
    }

    #[test]
    fn driver_uses_runtime_module_call_for_upsert_and_remove() {
        let command_path = test_path();
        let _ = fs::remove_file(&command_path);
        let _ = fs::remove_file(ack_path_for(&command_path));
        let mut world = crate::runtime_host::world::new();
        let mut driver = RuntimeModuleVisualDriver {
            command_path: command_path.clone(),
            ack_path: ack_path_for(&command_path),
            read_offset: 0,
            runtime_module_id: install_visual_driver(&mut world).expect("install driver module"),
        };
        let mut seed_model = None;
        let runtime_module_id = driver.runtime_module_id.clone();
        append_command(
            &command_path,
            json!({
                "operation": "upsert",
                "entity_id": "qa-entity",
                "module_id": runtime_module_id,
                "anchor": {
                    "type": "absolute",
                    "data": {"pos": {"x_cm": 100, "y_cm": 200, "z_cm": 300}}
                }
            }),
        );
        let emitted_upsert = driver
            .poll(&mut world, &mut seed_model, "qa-world", 4)
            .expect("upsert through runtime");
        assert_eq!(emitted_upsert.len(), 1);
        assert!(matches!(
            &emitted_upsert[0].body,
            WorldEventBody::ModuleEmitted(emitted) if emitted.kind == UPSERT_KIND
        ));
        assert!(world.journal().events.iter().any(|event| matches!(
            &event.body,
            WorldEventBody::ModuleEmitted(emitted)
                if emitted.kind == UPSERT_KIND
                    && emitted.payload["entity"]["entity_id"] == "qa-entity"
        )));
        assert_eq!(
            seed_model
                .as_ref()
                .and_then(|model| model.module_visual_entities.get("qa-entity"))
                .map(|entity| entity.module_id.as_str()),
            Some(driver.runtime_module_id.as_str())
        );
        assert_eq!(
            world
                .state()
                .module_visual_entities
                .get("qa-entity")
                .map(|entity| entity.module_id.as_str()),
            Some(driver.runtime_module_id.as_str())
        );

        append_command(
            &command_path,
            json!({"operation": "remove", "entity_id": "qa-entity"}),
        );
        let emitted_remove = driver
            .poll(&mut world, &mut seed_model, "qa-world", 4)
            .expect("remove through runtime");
        assert_eq!(emitted_remove.len(), 1);
        assert!(matches!(
            &emitted_remove[0].body,
            WorldEventBody::ModuleEmitted(emitted) if emitted.kind == REMOVE_KIND
        ));
        assert!(world.journal().events.iter().any(|event| matches!(
            &event.body,
            WorldEventBody::ModuleEmitted(emitted)
                if emitted.kind == REMOVE_KIND
                    && emitted.payload["entity_id"] == "qa-entity"
        )));
        assert!(
            !seed_model
                .as_ref()
                .is_some_and(|model| model.module_visual_entities.contains_key("qa-entity"))
        );
        assert!(
            !world
                .state()
                .module_visual_entities
                .contains_key("qa-entity")
        );

        let ack = fs::read_to_string(ack_path_for(&command_path)).expect("driver ack");
        let applied = ack
            .lines()
            .filter_map(|line| serde_json::from_str::<Value>(line).ok())
            .filter(|value| value["status"] == "applied")
            .collect::<Vec<_>>();
        assert_eq!(applied.len(), 2);
        assert_eq!(applied[0]["world_id"], "qa-world");
        assert_eq!(applied[0]["reorg_epoch"], 4);
        assert_eq!(applied[0]["event_kind"], UPSERT_KIND);
        assert_eq!(applied[0]["module_visual_entity_id"], "qa-entity");
        assert_eq!(applied[1]["event_kind"], REMOVE_KIND);

        let _ = fs::remove_file(command_path);
        let _ = fs::remove_file(ack_path_for(&driver.command_path));
    }
}
