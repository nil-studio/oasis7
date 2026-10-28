use oasis7_wasm_abi::{ModuleManifest, ModuleRecord, ModuleRegistry};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub const M1_MOVE_RULE_MODULE_ID: &str = "m1.rule.move";
pub const M1_VISIBILITY_RULE_MODULE_ID: &str = "m1.rule.visibility";
pub const M1_TRANSFER_RULE_MODULE_ID: &str = "m1.rule.transfer";
pub const M1_BODY_MODULE_ID: &str = "m1.body.core";
pub const M1_SENSOR_MODULE_ID: &str = "m1.sensor.basic";
pub const M1_MOBILITY_MODULE_ID: &str = "m1.mobility.basic";
pub const M1_MEMORY_MODULE_ID: &str = "m1.memory.core";
pub const M1_STORAGE_CARGO_MODULE_ID: &str = "m1.storage.cargo";
pub const M1_RADIATION_POWER_MODULE_ID: &str = "m1.power.radiation_harvest";
pub const M1_STORAGE_POWER_MODULE_ID: &str = "m1.power.storage";

pub const M1_AGENT_DEFAULT_MODULE_VERSION: &str = "0.1.0";
pub const M1_POWER_MODULE_VERSION: &str = "0.1.0";
pub const M1_STORAGE_POWER_MODULE_VERSION: &str = "0.2.0";
pub const M4_ECONOMY_MODULE_VERSION: &str = "0.1.0";
pub const M5_GAMEPLAY_MODULE_VERSION: &str = "0.1.0";

pub const M4_FACTORY_MINER_MODULE_ID: &str = "m4.factory.miner.mk1";
pub const M4_FACTORY_SMELTER_MODULE_ID: &str = "m4.factory.smelter.mk1";
pub const M4_FACTORY_ASSEMBLER_MODULE_ID: &str = "m4.factory.assembler.mk1";
pub const M4_RECIPE_SMELT_IRON_MODULE_ID: &str = "m4.recipe.smelter.iron_ingot";
pub const M4_RECIPE_SMELT_COPPER_WIRE_MODULE_ID: &str = "m4.recipe.smelter.copper_wire";
pub const M4_RECIPE_SMELT_POLYMER_RESIN_MODULE_ID: &str = "m4.recipe.smelter.polymer_resin";
pub const M4_RECIPE_ASSEMBLE_GEAR_MODULE_ID: &str = "m4.recipe.assembler.gear";
pub const M4_RECIPE_ASSEMBLE_CONTROL_CHIP_MODULE_ID: &str = "m4.recipe.assembler.control_chip";
pub const M4_RECIPE_ASSEMBLE_MOTOR_MODULE_ID: &str = "m4.recipe.assembler.motor_mk1";
pub const M4_RECIPE_ASSEMBLE_DRONE_MODULE_ID: &str = "m4.recipe.assembler.logistics_drone";
pub const M4_RECIPE_SMELT_ALLOY_PLATE_MODULE_ID: &str = "m4.recipe.smelter.alloy_plate";
pub const M4_RECIPE_ASSEMBLE_SENSOR_PACK_MODULE_ID: &str = "m4.recipe.assembler.sensor_pack";
pub const M4_RECIPE_ASSEMBLE_MODULE_RACK_MODULE_ID: &str = "m4.recipe.assembler.module_rack";
pub const M4_RECIPE_ASSEMBLE_FACTORY_CORE_MODULE_ID: &str = "m4.recipe.assembler.factory_core";
pub const M4_PRODUCT_IRON_INGOT_MODULE_ID: &str = "m4.product.material.iron_ingot";
pub const M4_PRODUCT_CONTROL_CHIP_MODULE_ID: &str = "m4.product.component.control_chip";
pub const M4_PRODUCT_MOTOR_MODULE_ID: &str = "m4.product.component.motor_mk1";
pub const M4_PRODUCT_LOGISTICS_DRONE_MODULE_ID: &str = "m4.product.finished.logistics_drone";
pub const M4_PRODUCT_ALLOY_PLATE_MODULE_ID: &str = "m4.product.material.alloy_plate";
pub const M4_PRODUCT_SENSOR_PACK_MODULE_ID: &str = "m4.product.component.sensor_pack";
pub const M4_PRODUCT_MODULE_RACK_MODULE_ID: &str = "m4.product.finished.module_rack";
pub const M4_PRODUCT_FACTORY_CORE_MODULE_ID: &str = "m4.product.infrastructure.factory_core";

pub const M5_GAMEPLAY_WAR_MODULE_ID: &str = "m5.gameplay.war.core";
pub const M5_GAMEPLAY_GOVERNANCE_MODULE_ID: &str = "m5.gameplay.governance.council";
pub const M5_GAMEPLAY_CRISIS_MODULE_ID: &str = "m5.gameplay.crisis.cycle";
pub const M5_GAMEPLAY_ECONOMIC_MODULE_ID: &str = "m5.gameplay.economic.overlay";
pub const M5_GAMEPLAY_META_MODULE_ID: &str = "m5.gameplay.meta.progression";

pub const M1_BODY_ACTION_COST_ELECTRICITY: i64 = 10;
pub const M1_MEMORY_MAX_ENTRIES: usize = 256;
pub const M1_POWER_STORAGE_CAPACITY: i64 = 12;
pub const M1_POWER_STORAGE_INITIAL_LEVEL: i64 = 6;
pub const M1_POWER_STORAGE_MOVE_COST_PER_KM: i64 = 3;
pub const M1_POWER_HARVEST_BASE_PER_TICK: i64 = 1;
pub const M1_POWER_HARVEST_DISTANCE_STEP_CM: i64 = 800_000;
pub const M1_POWER_HARVEST_DISTANCE_BONUS_CAP: i64 = 1;

const REGISTRY_VERSION: u64 = 1;
const REGISTRY_FILE: &str = "module_registry.json";
const MODULES_DIR: &str = "modules";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModuleStoreError {
    VersionMismatch { expected: u64, found: u64 },
    InvalidArtifactKey(String),
    Io(String),
    Serde(String),
}

impl From<io::Error> for ModuleStoreError {
    fn from(error: io::Error) -> Self {
        Self::Io(error.to_string())
    }
}

impl From<serde_json::Error> for ModuleStoreError {
    fn from(error: serde_json::Error) -> Self {
        Self::Serde(error.to_string())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct ModuleRegistryFile {
    pub version: u64,
    pub updated_at: i64,
    pub records: BTreeMap<String, ModuleRecord>,
    pub active: BTreeMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct ModuleStore {
    root: PathBuf,
    registry_path: PathBuf,
    modules_dir: PathBuf,
}

impl ModuleStore {
    pub fn new(root: impl AsRef<Path>) -> Self {
        let root = root.as_ref().to_path_buf();
        let registry_path = root.join(REGISTRY_FILE);
        let modules_dir = root.join(MODULES_DIR);
        Self {
            root,
            registry_path,
            modules_dir,
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn registry_path(&self) -> &Path {
        &self.registry_path
    }

    pub fn modules_dir(&self) -> &Path {
        &self.modules_dir
    }

    pub fn write_artifact(
        &self,
        wasm_hash: &str,
        bytes: &[u8],
    ) -> Result<PathBuf, ModuleStoreError> {
        validate_artifact_key(wasm_hash)?;
        self.ensure_dirs()?;
        let path = self.modules_dir.join(format!("{wasm_hash}.wasm"));
        write_bytes_atomic(&path, bytes)?;
        Ok(path)
    }

    pub fn read_artifact(&self, wasm_hash: &str) -> Result<Vec<u8>, ModuleStoreError> {
        validate_artifact_key(wasm_hash)?;
        let path = self.modules_dir.join(format!("{wasm_hash}.wasm"));
        Ok(fs::read(path)?)
    }

    pub fn write_meta(&self, manifest: &ModuleManifest) -> Result<PathBuf, ModuleStoreError> {
        validate_artifact_key(&manifest.wasm_hash)?;
        self.ensure_dirs()?;
        let path = self
            .modules_dir
            .join(format!("{}.meta.json", manifest.wasm_hash));
        write_json_atomic(manifest, &path)?;
        Ok(path)
    }

    pub fn read_meta(&self, wasm_hash: &str) -> Result<ModuleManifest, ModuleStoreError> {
        validate_artifact_key(wasm_hash)?;
        let path = self.modules_dir.join(format!("{wasm_hash}.meta.json"));
        read_json_from_path(&path)
    }

    pub fn save_registry(&self, registry: &ModuleRegistry) -> Result<(), ModuleStoreError> {
        self.ensure_dirs()?;
        let file = ModuleRegistryFile {
            version: REGISTRY_VERSION,
            updated_at: now_unix(),
            records: registry.records.clone(),
            active: registry.active.clone(),
        };
        write_json_atomic(&file, &self.registry_path)
    }

    pub fn load_registry(&self) -> Result<ModuleRegistry, ModuleStoreError> {
        if !self.registry_path.exists() {
            return Ok(ModuleRegistry::default());
        }
        decode_module_registry(&fs::read(&self.registry_path)?)
    }

    fn ensure_dirs(&self) -> Result<(), ModuleStoreError> {
        fs::create_dir_all(&self.root)?;
        fs::create_dir_all(&self.modules_dir)?;
        Ok(())
    }
}

/// Parse a registry without opening paths or materializing a store.
pub fn decode_module_registry(bytes: &[u8]) -> Result<ModuleRegistry, ModuleStoreError> {
    let file: ModuleRegistryFile = serde_json::from_slice(bytes)?;
    if file.version != REGISTRY_VERSION {
        return Err(ModuleStoreError::VersionMismatch {
            expected: REGISTRY_VERSION,
            found: file.version,
        });
    }
    Ok(ModuleRegistry {
        records: file.records,
        active: file.active,
    })
}

fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0)
}

fn validate_artifact_key(value: &str) -> Result<(), ModuleStoreError> {
    let trimmed = value.trim();
    let valid = !trimmed.is_empty()
        && trimmed == value
        && trimmed != "."
        && trimmed != ".."
        && trimmed
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'));
    if valid {
        Ok(())
    } else {
        Err(ModuleStoreError::InvalidArtifactKey(value.to_string()))
    }
}

fn write_json_atomic<T: Serialize>(value: &T, path: &Path) -> Result<(), ModuleStoreError> {
    let tmp = unique_tmp_path(path);
    write_json_to_path(value, &tmp)?;
    fs::rename(tmp, path)?;
    Ok(())
}

fn write_bytes_atomic(path: &Path, bytes: &[u8]) -> Result<(), ModuleStoreError> {
    let tmp = unique_tmp_path(path);
    fs::write(&tmp, bytes)?;
    fs::rename(tmp, path)?;
    Ok(())
}

fn unique_tmp_path(path: &Path) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("module-store");
    path.with_file_name(format!("{file_name}.{}.{}.tmp", std::process::id(), nonce))
}

fn write_json_to_path<T: Serialize>(value: &T, path: &Path) -> Result<(), ModuleStoreError> {
    let data = serde_json::to_vec_pretty(value)?;
    fs::write(path, data)?;
    Ok(())
}

fn read_json_from_path<T: DeserializeOwned>(path: &Path) -> Result<T, ModuleStoreError> {
    let data = fs::read(path)?;
    Ok(serde_json::from_slice(&data)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use oasis7_wasm_abi::{
        ModuleKind, ModuleLimits, ModuleManifest, ModuleRecord, ModuleRole, ModuleSubscription,
    };

    fn temp_dir(prefix: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        std::env::temp_dir().join(format!("{prefix}-{unique}"))
    }

    fn sample_manifest(hash: &str) -> ModuleManifest {
        ModuleManifest {
            module_id: "m.store".to_string(),
            name: "Store".to_string(),
            version: "0.1.0".to_string(),
            kind: ModuleKind::Reducer,
            role: ModuleRole::Domain,
            wasm_hash: hash.to_string(),
            interface_version: "wasm-1".to_string(),
            abi_contract: oasis7_wasm_abi::ModuleAbiContract::default(),
            exports: vec!["reduce".to_string()],
            subscriptions: Vec::<ModuleSubscription>::new(),
            required_caps: Vec::new(),
            artifact_identity: None,
            limits: ModuleLimits::unbounded(),
        }
    }

    #[test]
    fn roundtrip_artifact_meta_and_registry() {
        let dir = temp_dir("oasis7-wasm-store");
        let store = ModuleStore::new(&dir);

        let wasm_hash = "abc123";
        let bytes = vec![1_u8, 2, 3];
        store
            .write_artifact(wasm_hash, &bytes)
            .expect("write artifact");
        assert_eq!(
            store.read_artifact(wasm_hash).expect("read artifact"),
            bytes
        );

        let manifest = sample_manifest(wasm_hash);
        store.write_meta(&manifest).expect("write meta");
        assert_eq!(store.read_meta(wasm_hash).expect("read meta"), manifest);

        let mut registry = ModuleRegistry::default();
        let key = ModuleRegistry::record_key("m.store", "0.1.0");
        registry.records.insert(
            key,
            ModuleRecord {
                manifest,
                registered_at: 1,
                registered_by: "tester".to_string(),
                audit_event_id: Some(2),
            },
        );
        registry
            .active
            .insert("m.store".to_string(), "0.1.0".to_string());

        store.save_registry(&registry).expect("save registry");
        assert_eq!(store.load_registry().expect("load registry"), registry);

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn artifact_paths_reject_path_shaped_wasm_hashes() {
        let dir = temp_dir("oasis7-wasm-store-path-safe");
        let store = ModuleStore::new(&dir);

        assert!(matches!(
            store.write_artifact("../escape", &[1, 2, 3]),
            Err(ModuleStoreError::InvalidArtifactKey(_))
        ));
        assert!(matches!(
            store.read_artifact("../escape"),
            Err(ModuleStoreError::InvalidArtifactKey(_))
        ));

        let manifest = sample_manifest("abc/def");
        assert!(matches!(
            store.write_meta(&manifest),
            Err(ModuleStoreError::InvalidArtifactKey(_))
        ));
        assert!(matches!(
            store.read_meta("abc/def"),
            Err(ModuleStoreError::InvalidArtifactKey(_))
        ));

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn load_registry_rejects_version_mismatch() {
        let dir = temp_dir("oasis7-wasm-store-version");
        let store = ModuleStore::new(&dir);
        fs::create_dir_all(store.root()).expect("create root");
        let invalid = serde_json::json!({
            "version": 99,
            "updated_at": 0,
            "records": {},
            "active": {}
        });
        fs::write(
            store.registry_path(),
            serde_json::to_vec_pretty(&invalid).expect("encode"),
        )
        .expect("write registry");

        assert!(matches!(
            store.load_registry(),
            Err(ModuleStoreError::VersionMismatch {
                expected: 1,
                found: 99
            })
        ));

        let _ = fs::remove_dir_all(dir);
    }
}
