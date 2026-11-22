use oasis7_wasm_abi::{ModuleLimits, ModuleSubscription, ModuleSubscriptionStage};
use serde::Deserialize;
use serde_json::Value as JsonValue;
use std::fs;
use std::path::{Path, PathBuf};

const DEFAULT_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Deserialize)]
pub struct ModuleObserveSpec {
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    pub module: ModuleUnderTestSpec,
    #[serde(default)]
    pub subscriptions: Vec<ModuleSubscription>,
    #[serde(default)]
    pub cases: Vec<ObserveCaseSpec>,
    #[serde(default)]
    pub router_probes: Vec<RouterProbeSpec>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ModuleUnderTestSpec {
    pub module_id: String,
    pub manifest_path: String,
    #[serde(default = "default_entrypoint")]
    pub entrypoint: String,
    #[serde(default = "default_profile")]
    pub profile: String,
    #[serde(default = "default_target")]
    pub target: String,
    #[serde(default = "default_module_limits")]
    pub limits: ModuleLimits,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ObserveCaseSpec {
    pub name: String,
    #[serde(default = "default_repeat")]
    pub repeat: u32,
    pub request: CaseRequestSpec,
    pub expect: CaseExpectationSpec,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CaseRequestSpec {
    #[serde(default)]
    pub trace_id: Option<String>,
    #[serde(default)]
    pub entrypoint: Option<String>,
    #[serde(default = "default_context_version")]
    pub ctx_version: String,
    #[serde(default)]
    pub time: u64,
    #[serde(default = "default_origin_kind")]
    pub origin_kind: String,
    #[serde(default = "default_origin_id")]
    pub origin_id: String,
    #[serde(default)]
    pub stage: Option<String>,
    #[serde(default)]
    pub world_config_hash: Option<String>,
    #[serde(default)]
    pub manifest_hash: Option<String>,
    #[serde(default)]
    pub journal_height: Option<u64>,
    #[serde(default)]
    pub module_version: Option<String>,
    #[serde(default)]
    pub module_kind: Option<String>,
    #[serde(default)]
    pub module_role: Option<String>,
    #[serde(default)]
    pub limits: Option<ModuleLimits>,
    #[serde(default)]
    pub event_json: Option<JsonValue>,
    #[serde(default)]
    pub action_json: Option<JsonValue>,
    #[serde(default)]
    pub state_json: Option<JsonValue>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CaseExpectationSpec {
    #[serde(default = "default_expect_success")]
    pub success: bool,
    #[serde(default)]
    pub failure_code: Option<String>,
    #[serde(default)]
    pub failure_detail_substring: Option<String>,
    #[serde(default)]
    pub emit_count: Option<usize>,
    #[serde(default)]
    pub effect_count: Option<usize>,
    #[serde(default)]
    pub new_state_present: Option<bool>,
    #[serde(default)]
    pub tick_lifecycle: Option<TickLifecycleExpectation>,
    #[serde(default)]
    pub emits: Vec<ExpectedEmitSpec>,
    #[serde(default)]
    pub state_json: Option<JsonValue>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ExpectedEmitSpec {
    pub kind: String,
    #[serde(default)]
    pub payload_json: Option<JsonValue>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum TickLifecycleExpectation {
    WakeAfterTicks { ticks: u64 },
    Suspend,
    Absent,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RouterProbeSpec {
    pub name: String,
    #[serde(default = "default_repeat")]
    pub repeat: u32,
    #[serde(default = "default_true")]
    pub use_prepared: bool,
    pub probe: RouterProbeInputSpec,
    #[serde(default = "default_true")]
    pub expect_match: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RouterProbeInputSpec {
    Event {
        event_kind: String,
        payload_json: JsonValue,
    },
    Action {
        stage: ModuleSubscriptionStage,
        action_kind: String,
        payload_json: JsonValue,
    },
}

#[derive(Debug, Clone)]
pub struct ResolvedModuleObserveSpec {
    pub spec_path: PathBuf,
    pub schema_version: u32,
    pub module: ResolvedModuleUnderTestSpec,
    pub subscriptions: Vec<ModuleSubscription>,
    pub cases: Vec<ObserveCaseSpec>,
    pub router_probes: Vec<RouterProbeSpec>,
}

#[derive(Debug, Clone)]
pub struct ResolvedModuleUnderTestSpec {
    pub module_id: String,
    pub manifest_path: PathBuf,
    pub entrypoint: String,
    pub profile: String,
    pub target: String,
    pub limits: ModuleLimits,
}

pub fn load_spec(path: &Path) -> Result<ResolvedModuleObserveSpec, String> {
    let raw = fs::read_to_string(path)
        .map_err(|err| format!("read observe spec {} failed: {err}", path.display()))?;
    let spec: ModuleObserveSpec = serde_json::from_str(&raw)
        .map_err(|err| format!("parse observe spec {} failed: {err}", path.display()))?;
    if spec.schema_version != DEFAULT_SCHEMA_VERSION {
        return Err(format!(
            "observe spec {} uses unsupported schema_version={} (expected {})",
            path.display(),
            spec.schema_version,
            DEFAULT_SCHEMA_VERSION
        ));
    }
    let spec_dir = path
        .parent()
        .ok_or_else(|| format!("observe spec {} has no parent directory", path.display()))?;
    let manifest_candidate = spec_dir.join(&spec.module.manifest_path);
    if !manifest_candidate.exists() {
        return Err(format!(
            "observe spec manifest path does not exist: {}",
            manifest_candidate.display()
        ));
    }
    let manifest_path = std::fs::canonicalize(&manifest_candidate).map_err(|err| {
        format!(
            "canonicalize observe spec manifest path {} failed: {err}",
            manifest_candidate.display()
        )
    })?;
    if spec.cases.is_empty() {
        return Err(format!(
            "observe spec {} must contain at least one case",
            path.display()
        ));
    }
    for case in &spec.cases {
        if case.repeat == 0 {
            return Err(format!("case {} repeat must be >= 1", case.name));
        }
    }
    for probe in &spec.router_probes {
        if probe.repeat == 0 {
            return Err(format!("router probe {} repeat must be >= 1", probe.name));
        }
    }
    Ok(ResolvedModuleObserveSpec {
        spec_path: path.to_path_buf(),
        schema_version: spec.schema_version,
        module: ResolvedModuleUnderTestSpec {
            module_id: spec.module.module_id,
            manifest_path,
            entrypoint: spec.module.entrypoint,
            profile: spec.module.profile,
            target: spec.module.target,
            limits: spec.module.limits,
        },
        subscriptions: spec.subscriptions,
        cases: spec.cases,
        router_probes: spec.router_probes,
    })
}

fn default_schema_version() -> u32 {
    DEFAULT_SCHEMA_VERSION
}

fn default_entrypoint() -> String {
    "reduce".to_string()
}

fn default_profile() -> String {
    "release".to_string()
}

fn default_target() -> String {
    "wasm32-unknown-unknown".to_string()
}

fn default_module_limits() -> ModuleLimits {
    ModuleLimits {
        max_mem_bytes: 64 * 1024 * 1024,
        max_gas: 2_000_000,
        max_call_rate: 10,
        max_output_bytes: 4096,
        max_effects: 8,
        max_emits: 8,
    }
}

fn default_repeat() -> u32 {
    1
}

fn default_context_version() -> String {
    "wasm-1".to_string()
}

fn default_origin_kind() -> String {
    "observe_runner".to_string()
}

fn default_origin_id() -> String {
    "local".to_string()
}

fn default_expect_success() -> bool {
    true
}

fn default_true() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_spec_path() -> PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time before unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("wasm-module-observe-spec-{suffix}.json"))
    }

    fn unique_temp_root(label: &str) -> std::path::PathBuf {
        static TEMP_DIR_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let seq = TEMP_DIR_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        std::env::temp_dir().join(format!("{label}-{}-{seq}", std::process::id()))
    }

    #[test]
    fn load_spec_resolves_manifest_relative_to_spec() {
        let temp_root = unique_temp_root("wasm-module-observe-spec-test");
        let module_dir = temp_root.join("module");
        let observe_dir = module_dir.join("observability");
        std::fs::create_dir_all(&observe_dir).expect("create observe dir");
        let manifest_path = module_dir.join("Cargo.toml");
        std::fs::write(&manifest_path, "[package]\nname=\"m\"\nversion=\"0.1.0\"\n")
            .expect("write manifest");
        let spec_path = observe_dir.join("module_observe.json");
        std::fs::write(
            &spec_path,
            r#"{
  "module": {
    "module_id": "m.test",
    "manifest_path": "../Cargo.toml"
  },
  "cases": [
    {
      "name": "noop",
      "request": {},
      "expect": {}
    }
  ]
}"#,
        )
        .expect("write spec");

        let resolved = load_spec(&spec_path).expect("load spec");
        let expected_manifest_path =
            std::fs::canonicalize(&manifest_path).expect("canonicalize expected manifest path");
        assert_eq!(resolved.schema_version, 1);
        assert_eq!(resolved.module.module_id, "m.test");
        assert_eq!(resolved.module.manifest_path, expected_manifest_path);
        assert_eq!(resolved.module.entrypoint, "reduce");

        let _ = std::fs::remove_file(spec_path);
        let _ = std::fs::remove_file(manifest_path);
        let _ = std::fs::remove_dir_all(temp_root);
    }

    #[test]
    fn load_spec_rejects_zero_repeat() {
        let spec_path = temp_spec_path();
        std::fs::write(
            &spec_path,
            format!(
                r#"{{
  "module": {{
    "module_id": "m.test",
    "manifest_path": "{}"
  }},
  "cases": [
    {{
      "name": "bad",
      "repeat": 0,
      "request": {{}},
      "expect": {{}}
    }}
  ]
}}"#,
                spec_path.display()
            ),
        )
        .expect("write spec");
        let err = load_spec(&spec_path).expect_err("spec should fail");
        assert!(err.contains("repeat must be >= 1"));
        let _ = std::fs::remove_file(spec_path);
    }

    #[test]
    fn load_spec_rejects_unknown_schema_version() {
        let temp_root = unique_temp_root("wasm-module-observe-schema-version-test");
        let module_dir = temp_root.join("module");
        let observe_dir = module_dir.join("observability");
        std::fs::create_dir_all(&observe_dir).expect("create observe dir");
        let manifest_path = module_dir.join("Cargo.toml");
        std::fs::write(&manifest_path, "[package]\nname=\"m\"\nversion=\"0.1.0\"\n")
            .expect("write manifest");
        let spec_path = observe_dir.join("module_observe.json");
        std::fs::write(
            &spec_path,
            r#"{
  "schema_version": 2,
  "module": {
    "module_id": "m.test",
    "manifest_path": "../Cargo.toml"
  },
  "cases": [
    {
      "name": "noop",
      "request": {},
      "expect": {}
    }
  ]
}"#,
        )
        .expect("write spec");

        let err = load_spec(&spec_path).expect_err("schema version should fail");
        assert!(err.contains("unsupported schema_version=2"));

        let _ = std::fs::remove_file(spec_path);
        let _ = std::fs::remove_file(manifest_path);
        let _ = std::fs::remove_dir_all(temp_root);
    }
}
