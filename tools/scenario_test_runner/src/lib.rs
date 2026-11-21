use oasis7_simulator::{
    WorldConfig, WorldInitConfig, WorldInitReport, WorldModel, WorldScenario, build_world_model,
};
use serde::Deserialize;
use std::borrow::Cow;
use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub enum ScenarioError {
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    UnsupportedFormat {
        path: PathBuf,
    },
    Parse {
        path: PathBuf,
        message: String,
    },
    InvalidScenario {
        message: String,
    },
    Init {
        message: String,
    },
}

impl fmt::Display for ScenarioError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ScenarioError::Io { path, source } => {
                write!(f, "failed to read {}: {}", path.display(), source)
            }
            ScenarioError::UnsupportedFormat { path } => {
                write!(f, "unsupported scenario format: {}", path.display())
            }
            ScenarioError::Parse { path, message } => {
                write!(f, "failed to parse {}: {}", path.display(), message)
            }
            ScenarioError::InvalidScenario { message } => write!(f, "invalid scenario: {message}"),
            ScenarioError::Init { message } => write!(f, "init failed: {message}"),
        }
    }
}

impl std::error::Error for ScenarioError {}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScenarioFile {
    pub version: u32,
    pub name: String,
    #[serde(default)]
    pub scenario: Option<String>,
    #[serde(default)]
    pub seed: Option<u64>,
    #[serde(default)]
    pub init: Option<WorldInitConfig>,
    #[serde(default)]
    pub config: Option<WorldConfig>,
    pub expect: Expectations,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Expectations {
    #[serde(default)]
    pub agents: Option<usize>,
    #[serde(default)]
    pub locations: Option<usize>,
    #[serde(default)]
    pub require_locations: Vec<String>,
    #[serde(default)]
    pub require_agents: Vec<String>,
    #[serde(default)]
    pub require_power_plants: Vec<String>,
    #[serde(default)]
    pub require_power_storages: Vec<String>,
    #[serde(default)]
    pub expect_asteroid_fragment: Option<bool>,
    #[serde(default)]
    pub agent_locations: BTreeMap<String, String>,
}

impl Expectations {
    fn is_empty(&self) -> bool {
        let Self {
            agents,
            locations,
            require_locations,
            require_agents,
            require_power_plants,
            require_power_storages,
            expect_asteroid_fragment,
            agent_locations,
        } = self;

        agents.is_none()
            && locations.is_none()
            && require_locations.is_empty()
            && require_agents.is_empty()
            && require_power_plants.is_empty()
            && require_power_storages.is_empty()
            && expect_asteroid_fragment.is_none()
            && agent_locations.is_empty()
    }
}

#[derive(Debug)]
pub struct ScenarioOutcome {
    pub name: String,
    pub source: String,
    pub passed: bool,
    pub failures: Vec<String>,
}

pub fn run_scenario_file(path: &Path) -> Result<ScenarioOutcome, ScenarioError> {
    let scenario = load_scenario_file(path)?;
    run_loaded_scenario(&scenario, path.to_string_lossy().as_ref())
}

pub fn run_loaded_scenario(
    scenario: &ScenarioFile,
    source: &str,
) -> Result<ScenarioOutcome, ScenarioError> {
    if scenario.version != 1 {
        return Err(ScenarioError::InvalidScenario {
            message: format!("unsupported version {}", scenario.version),
        });
    }
    if scenario.scenario.is_some() && scenario.init.is_some() {
        return Err(ScenarioError::InvalidScenario {
            message: "scenario and init are mutually exclusive".to_string(),
        });
    }

    if scenario.expect.is_empty() {
        return Err(ScenarioError::InvalidScenario {
            message: "expect must contain at least one assertion".to_string(),
        });
    }

    let config = scenario.config.as_ref().cloned().unwrap_or_default();
    let mut init = if let Some(init) = scenario.init.as_ref() {
        Cow::Borrowed(init)
    } else if let Some(name) = scenario.scenario.as_ref() {
        let parsed = WorldScenario::parse(name).ok_or_else(|| ScenarioError::InvalidScenario {
            message: format!("unknown scenario: {name}"),
        })?;
        Cow::Owned(WorldInitConfig::from_scenario(parsed, &config))
    } else {
        return Err(ScenarioError::InvalidScenario {
            message: "scenario or init must be provided".to_string(),
        });
    };

    if let Some(seed) = scenario.seed {
        init.to_mut().seed = seed;
    }

    let (model, report) =
        build_world_model(&config, init.as_ref()).map_err(|err| ScenarioError::Init {
            message: format!("{err:?}"),
        })?;

    let failures = evaluate_expectations(&scenario.expect, &model, &report);
    Ok(ScenarioOutcome {
        name: scenario.name.clone(),
        source: source.to_string(),
        passed: failures.is_empty(),
        failures,
    })
}

pub fn discover_scenario_files(dir: &Path) -> Result<Vec<PathBuf>, ScenarioError> {
    let mut entries = Vec::new();
    let read_dir = std::fs::read_dir(dir).map_err(|err| ScenarioError::Io {
        path: dir.to_path_buf(),
        source: err,
    })?;

    for entry in read_dir {
        let entry = entry.map_err(|err| ScenarioError::Io {
            path: dir.to_path_buf(),
            source: err,
        })?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        if is_supported_format(&path) {
            entries.push(path);
        }
    }

    entries.sort();
    Ok(entries)
}

pub fn load_scenario_file(path: &Path) -> Result<ScenarioFile, ScenarioError> {
    let contents = std::fs::read_to_string(path).map_err(|err| ScenarioError::Io {
        path: path.to_path_buf(),
        source: err,
    })?;
    let ext = path.extension().and_then(|value| value.to_str());

    if supported_scenario_extension(ext, "yaml") || supported_scenario_extension(ext, "yml") {
        serde_saphyr::from_str(&contents).map_err(|err| ScenarioError::Parse {
            path: path.to_path_buf(),
            message: err.to_string(),
        })
    } else if supported_scenario_extension(ext, "json") {
        serde_json::from_str(&contents).map_err(|err| ScenarioError::Parse {
            path: path.to_path_buf(),
            message: err.to_string(),
        })
    } else {
        Err(ScenarioError::UnsupportedFormat {
            path: path.to_path_buf(),
        })
    }
}

fn is_supported_format(path: &Path) -> bool {
    let ext = path.extension().and_then(|value| value.to_str());
    supported_scenario_extension(ext, "yaml")
        || supported_scenario_extension(ext, "yml")
        || supported_scenario_extension(ext, "json")
}

fn supported_scenario_extension(ext: Option<&str>, expected: &str) -> bool {
    ext.is_some_and(|value| value.eq_ignore_ascii_case(expected))
}

fn evaluate_expectations(
    expect: &Expectations,
    model: &WorldModel,
    report: &WorldInitReport,
) -> Vec<String> {
    let mut failures = Vec::new();

    if let Some(expected) = expect.agents {
        let actual = model.agents.len();
        if actual != expected {
            failures.push(format!(
                "agents mismatch: expected {expected}, got {actual}"
            ));
        }
    }

    if let Some(expected) = expect.locations {
        let actual = model.locations.len();
        if actual != expected {
            failures.push(format!(
                "locations mismatch: expected {expected}, got {actual}"
            ));
        }
    }

    for location_id in &expect.require_locations {
        if !model.locations.contains_key(location_id) {
            failures.push(format!("missing location: {location_id}"));
        }
    }

    for agent_id in &expect.require_agents {
        if !model.agents.contains_key(agent_id) {
            failures.push(format!("missing agent: {agent_id}"));
        }
    }

    for plant_id in &expect.require_power_plants {
        if !model.power_plants.contains_key(plant_id) {
            failures.push(format!("missing power plant: {plant_id}"));
        }
    }

    for storage_id in &expect.require_power_storages {
        failures.push(format!(
            "unsupported legacy expectation `require_power_storages`: {storage_id}"
        ));
    }

    if let Some(expect_asteroid_fragment) = expect.expect_asteroid_fragment {
        let actual = report.asteroid_fragment_seed.is_some();
        if actual != expect_asteroid_fragment {
            failures.push(format!("asteroid fragment enabled mismatch: expected {expect_asteroid_fragment}, got {actual}"));
        }
    }

    for (agent_id, location_id) in &expect.agent_locations {
        match model.agents.get(agent_id) {
            Some(agent) => {
                if agent.location_id != *location_id {
                    failures.push(format!(
                        "agent location mismatch: {agent_id} expected {location_id}, got {}",
                        agent.location_id
                    ));
                }
            }
            None => failures.push(format!("missing agent for location check: {agent_id}")),
        }
    }

    failures
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn triad_yaml_fixture_runs_with_its_declared_expectations() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("scenarios/triad_p2p.yaml");
        let outcome = run_scenario_file(&path).expect("load and run checked-in YAML fixture");
        assert!(outcome.passed, "{:#?}", outcome.failures);
    }

    #[test]
    fn yaml_and_json_reject_empty_expectations() {
        let yaml = "version: 1\nname: minimal\nscenario: minimal\nexpect: {}\n";
        let json = r#"{"version":1,"name":"minimal","scenario":"minimal","expect":{}}"#;
        let empty_lists = r#"{"version":1,"name":"minimal","scenario":"minimal","expect":{"require_agents":[],"agent_locations":{}}}"#;
        let cases: [ScenarioFile; 3] = [
            serde_saphyr::from_str(yaml).expect("parse YAML"),
            serde_json::from_str(json).expect("parse JSON"),
            serde_json::from_str(empty_lists).expect("parse JSON with empty collections"),
        ];

        for scenario in cases {
            let err = run_loaded_scenario(&scenario, "memory")
                .expect_err("a scenario without assertions must not pass");
            assert!(matches!(
                err,
                ScenarioError::InvalidScenario { message }
                    if message == "expect must contain at least one assertion"
            ));
        }
    }

    #[test]
    fn zero_and_false_are_nonempty_expectations() {
        let zero: ScenarioFile = serde_saphyr::from_str(
            "version: 1\nname: minimal\nscenario: minimal\nexpect:\n  agents: 0\n",
        )
        .expect("parse YAML");
        let outcome = run_loaded_scenario(&zero, "memory").expect("evaluate zero as an assertion");
        assert!(!outcome.passed);
        assert_eq!(outcome.failures.len(), 1);
        assert!(outcome.failures[0].contains("agents mismatch"));

        let disabled: ScenarioFile = serde_saphyr::from_str(
            "version: 1\nname: minimal\nscenario: minimal\nexpect:\n  expect_asteroid_fragment: false\n",
        )
        .expect("parse YAML");
        let outcome =
            run_loaded_scenario(&disabled, "memory").expect("evaluate false as an assertion");
        assert!(outcome.passed, "{:#?}", outcome.failures);
    }

    #[test]
    fn scenario_from_builtin_template_passes_expectations() {
        let scenario = ScenarioFile {
            version: 1,
            name: "minimal".to_string(),
            scenario: Some("minimal".to_string()),
            seed: None,
            init: None,
            config: None,
            expect: Expectations {
                agents: Some(1),
                locations: Some(1),
                require_locations: vec!["origin".to_string()],
                require_agents: vec!["agent-0".to_string()],
                require_power_plants: Vec::new(),
                require_power_storages: Vec::new(),
                expect_asteroid_fragment: Some(false),
                agent_locations: BTreeMap::from([("agent-0".to_string(), "origin".to_string())]),
            },
        };

        let outcome = run_loaded_scenario(&scenario, "memory").expect("run scenario");
        assert!(outcome.passed, "{:#?}", outcome.failures);
    }

    #[test]
    fn agent_location_expectation_detects_mismatch() {
        let mut agent_locations = BTreeMap::new();
        agent_locations.insert("agent-0".to_string(), "missing".to_string());

        let scenario = ScenarioFile {
            version: 1,
            name: "minimal".to_string(),
            scenario: Some("minimal".to_string()),
            seed: None,
            init: None,
            config: None,
            expect: Expectations {
                agents: None,
                locations: None,
                require_locations: Vec::new(),
                require_agents: Vec::new(),
                require_power_plants: Vec::new(),
                require_power_storages: Vec::new(),
                expect_asteroid_fragment: None,
                agent_locations,
            },
        };

        let outcome = run_loaded_scenario(&scenario, "memory").expect("run scenario");
        assert!(!outcome.passed);
        assert_eq!(outcome.failures.len(), 1);
    }

    #[test]
    fn explicit_init_passes_without_requiring_seed_override() {
        let config = WorldConfig::default();
        let scenario_name = WorldScenario::parse("minimal").expect("minimal scenario");
        let init = WorldInitConfig::from_scenario(scenario_name, &config);
        let scenario = ScenarioFile {
            version: 1,
            name: "explicit-init".to_string(),
            scenario: None,
            seed: None,
            init: Some(init),
            config: Some(config),
            expect: Expectations {
                agents: Some(1),
                locations: Some(1),
                require_locations: vec!["origin".to_string()],
                require_agents: vec!["agent-0".to_string()],
                require_power_plants: Vec::new(),
                require_power_storages: Vec::new(),
                expect_asteroid_fragment: Some(false),
                agent_locations: BTreeMap::new(),
            },
        };

        let outcome = run_loaded_scenario(&scenario, "memory").expect("run explicit init scenario");
        assert!(outcome.passed, "{:#?}", outcome.failures);
    }

    #[test]
    fn scenario_parse_rejects_unknown_top_level_fields() {
        let input = r#"
version: 1
name: minimal
scenario: minimal
unexpected: ignored-before
expect:
  agents: 1
"#;

        let err = serde_saphyr::from_str::<ScenarioFile>(input)
            .expect_err("unknown top-level fields should fail parse");
        assert!(
            err.to_string().contains("unknown field `unexpected`"),
            "{err}"
        );
    }

    #[test]
    fn scenario_parse_rejects_unknown_expectation_fields() {
        let input = r#"
version: 1
name: minimal
scenario: minimal
expect:
  agents: 1
  require_agentz:
    - agent-0
"#;

        let err = serde_saphyr::from_str::<ScenarioFile>(input)
            .expect_err("unknown expectation fields should fail parse");
        assert!(
            err.to_string().contains("unknown field `require_agentz`"),
            "{err}"
        );
    }

    #[test]
    fn supported_format_accepts_uppercase_extensions_without_allocating() {
        assert!(is_supported_format(Path::new("scenario.YAML")));
        assert!(is_supported_format(Path::new("scenario.JSON")));
        assert!(!is_supported_format(Path::new("scenario.txt")));
    }
}
