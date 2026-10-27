//! Declared producer-content recipe, deliberately separate from realized OCI bytes.
use super::*;
use crate::source_inputs::{include_path, put_string, validate_file};
use serde::Serialize;

const PRODUCERS: [&str; 2] = ["crates/oasis7_wasm_build", "tools/wasm_build_suite"];
const FIXED: [&str; 5] = [
    "docker/wasm-builder/Dockerfile",
    ".dockerignore",
    "scripts/build-wasm-module.sh",
    "Cargo.toml",
    "Cargo.lock",
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecipeArgument {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BuilderRecipe {
    pub recipe_schema_version: u32,
    pub kind: String,
    pub producer_recipe_hash: String,
    pub arguments: Vec<RecipeArgument>,
    pub inputs: Vec<SourceInputRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BuilderExecution {
    pub execution_schema_version: u32,
    pub kind: String,
    pub image_ref: String,
    pub image_id: String,
    pub docker_platform: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CandidateBuilderEvidence {
    pub purpose: String,
    pub build_manifest_schema_version: u32,
    pub builder_recipe: BuilderRecipe,
    pub builder_execution: BuilderExecution,
}
impl CandidateBuilderEvidence {
    pub fn validate(&self) -> Result<(), SourceHashError> {
        if self.purpose != "candidate-build" || self.build_manifest_schema_version != 3 {
            return Err(invalid("unknown candidate builder evidence"));
        }
        self.builder_execution.validate(&self.builder_recipe)
    }
}

fn invalid(reason: impl std::fmt::Display) -> SourceHashError {
    SourceHashError::UnsupportedInput(format!("producer recipe: {reason}"))
}
fn arguments() -> Vec<RecipeArgument> {
    [
        ("WASM_TARGET", "wasm32-unknown-unknown"),
        ("WASM_TOOLCHAIN", "nightly-2025-12-11"),
        ("platform", "linux/amd64"),
        ("purpose", "candidate-build"),
    ]
    .into_iter()
    .map(|(name, value)| RecipeArgument {
        name: name.into(),
        value: value.into(),
    })
    .collect()
}
fn digest(recipe: &BuilderRecipe) -> Result<String, SourceHashError> {
    let mut hash = Sha256::new();
    hash.update(b"oasis7-builder-producer-recipe-v1");
    hash.update((recipe.arguments.len() as u64).to_be_bytes());
    for arg in &recipe.arguments {
        put_string(&mut hash, &arg.name);
        put_string(&mut hash, &arg.value);
    }
    hash.update((recipe.inputs.len() as u64).to_be_bytes());
    let mut previous = None;
    for input in &recipe.inputs {
        if input.path.is_empty()
            || input.path.contains('\\')
            || Path::new(&input.path)
                .components()
                .any(|part| !matches!(part, std::path::Component::Normal(_)))
            || input.kind != "source-file"
            || previous.is_some_and(|path: &str| path >= input.path.as_str())
        {
            return Err(invalid("unordered or duplicate recipe input"));
        }
        previous = Some(input.path.as_str());
        put_string(&mut hash, &input.path);
        let bytes = hex::decode(&input.sha256).map_err(invalid)?;
        if bytes.len() != 32 || input.sha256 != hex::encode(&bytes) {
            return Err(invalid("invalid recipe digest"));
        }
        hash.update(bytes);
    }
    Ok(hex::encode(hash.finalize()))
}
impl BuilderRecipe {
    pub fn validate(&self) -> Result<(), SourceHashError> {
        if self.recipe_schema_version != 1
            || self.kind != "producer-content"
            || self.arguments != arguments()
            || digest(self)? != self.producer_recipe_hash
        {
            return Err(invalid("unknown/mixed recipe identity"));
        }
        Ok(())
    }
}
impl BuilderExecution {
    pub fn validate(&self, recipe: &BuilderRecipe) -> Result<(), SourceHashError> {
        recipe.validate()?;
        let id = self
            .image_id
            .strip_prefix("sha256:")
            .ok_or_else(|| invalid("missing actual Docker ID"))?;
        if self.execution_schema_version != 1
            || self.kind != "local-docker-build"
            || self.docker_platform != "linux/amd64"
            || self.image_ref
                != format!(
                    "oasis7/wasm-builder:candidate-{}",
                    recipe.producer_recipe_hash
                )
            || id.len() != 64
            || !id
                .bytes()
                .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
        {
            return Err(invalid("unknown/mixed execution identity"));
        }
        Ok(())
    }
}

fn validate_dockerfile(root: &Path) -> Result<(), SourceHashError> {
    let text = fs::read_to_string(root.join("docker/wasm-builder/Dockerfile")).map_err(invalid)?;
    // Recognize the current instruction lane only. Dockerfile bytes remain recipe
    // inputs; this guard prevents new input-producing instructions hiding behind
    // different case or shell continuations.
    let mut logical = Vec::new();
    let mut pending = String::new();
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let continuation = line.ends_with('\\');
        pending.push_str(line.trim_end_matches('\\'));
        pending.push(' ');
        if !continuation {
            logical.push(pending.split_whitespace().collect::<Vec<_>>().join(" "));
            pending.clear();
        }
    }
    if !pending.is_empty() {
        return Err(invalid("unterminated Dockerfile instruction"));
    }
    let runs = [
        "RUN apt-get update && apt-get install -y --no-install-recommends bash build-essential ca-certificates curl git pkg-config xz-utils && rm -rf /var/lib/apt/lists/*",
        "RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain none",
        "RUN rustup toolchain install \"${WASM_TOOLCHAIN}\" --profile minimal --component rust-src && rustup target add \"${WASM_TARGET}\" --toolchain \"${WASM_TOOLCHAIN}\"",
        "RUN chmod +x /opt/oasis7/scripts/build-wasm-module.sh && env -u RUSTC_WRAPPER cargo install --locked --path /opt/oasis7/tools/wasm_build_suite && chmod -R a+rX /cargo /rustup /opt/oasis7",
    ];
    for instruction in &logical {
        let opcode = instruction.split_whitespace().next().unwrap_or_default();
        let supported = match opcode {
            "COPY" | "WORKDIR" | "LABEL" => true,
            "FROM" => instruction == "FROM debian:bookworm-slim",
            "ARG" => matches!(
                instruction.as_str(),
                "ARG WASM_TOOLCHAIN=nightly-2025-12-11" | "ARG WASM_TARGET=wasm32-unknown-unknown"
            ),
            "ENTRYPOINT" => {
                instruction == "ENTRYPOINT [\"/opt/oasis7/scripts/build-wasm-module.sh\"]"
            }
            "RUN" => runs.contains(&instruction.as_str()),
            "ENV" => matches!(
                instruction.as_str(),
                "ENV DEBIAN_FRONTEND=noninteractive"
                    | "ENV CARGO_HOME=/cargo"
                    | "ENV RUSTUP_HOME=/rustup"
                    | "ENV PATH=/cargo/bin:${PATH}"
                    | "ENV OASIS7_WASM_TOOLCHAIN=${WASM_TOOLCHAIN}"
                    | "ENV OASIS7_WASM_TARGET=${WASM_TARGET}"
                    | "ENV OASIS7_WASM_BUILD_STD=1"
                    | "ENV OASIS7_WASM_BUILD_STD_COMPONENTS=std,panic_abort"
                    | "ENV OASIS7_WASM_BUILD_STD_FEATURES="
                    | "ENV OASIS7_WASM_BUILD_IN_CONTAINER=1"
                    | "ENV OASIS7_WASM_BUILD_SUITE_BIN=/cargo/bin/wasm_build_suite"
                    | "ENV TZ=UTC"
                    | "ENV LANG=C"
                    | "ENV LC_ALL=C"
                    | "ENV SOURCE_DATE_EPOCH=0"
            ),
            _ => false,
        };
        if !supported {
            return Err(invalid(format!(
                "unsupported Dockerfile instruction: {opcode}"
            )));
        }
    }
    let copies = text
        .lines()
        .map(str::trim)
        .filter(|line| line.starts_with("COPY ") || line.starts_with("ADD "))
        .collect::<Vec<_>>();
    let expected = [
        "COPY Cargo.toml Cargo.lock /opt/oasis7/",
        "COPY vendor-egui-winit-0.33.3 /opt/oasis7/vendor-egui-winit-0.33.3",
        "COPY crates /opt/oasis7/crates",
        "COPY vendor-libp2p-swarm-0.48.0 /opt/oasis7/vendor-libp2p-swarm-0.48.0",
        "COPY vendor-libp2p-rendezvous-0.18.1 /opt/oasis7/vendor-libp2p-rendezvous-0.18.1",
        "COPY tools/scenario_test_runner /opt/oasis7/tools/scenario_test_runner",
        "COPY tools/wasm_build_suite /opt/oasis7/tools/wasm_build_suite",
        "COPY tools/wasm_module_observe /opt/oasis7/tools/wasm_module_observe",
        "COPY scripts/build-wasm-module.sh /opt/oasis7/scripts/build-wasm-module.sh",
    ];
    let workdirs = text
        .lines()
        .map(str::trim)
        .filter(|line| line.starts_with("WORKDIR "))
        .collect::<Vec<_>>();
    if copies != expected
        || workdirs != ["WORKDIR /opt/oasis7", "WORKDIR /workspace"]
        || !text.contains(
            "env -u RUSTC_WRAPPER cargo install --locked --path /opt/oasis7/tools/wasm_build_suite",
        )
    {
        return Err(invalid(
            "unsupported Dockerfile COPY/WORKDIR/producer install recipe",
        ));
    }
    Ok(())
}

fn files(root: &Path, image: bool) -> Result<BTreeSet<PathBuf>, SourceHashError> {
    if image {
        if root.join(".cargo/config").exists() || root.join(".cargo/config.toml").exists() {
            return Err(invalid("unexpected image root Cargo config"));
        }
    } else {
        validate_dockerfile(root)?;
    }
    let manifest = root.join("tools/wasm_build_suite/Cargo.toml");
    let metadata = read_cargo_metadata_from(&manifest, "x86_64-unknown-linux-gnu", Some(root))?;
    crate::source_inputs::validate_macro_owners_with_procedural(
        &metadata,
        &manifest,
        &["serde_derive", "bytemuck_derive"],
    )?;
    let local = collect_reachable_packages(&metadata, &manifest)?
        .into_iter()
        .filter(|package| package.source.is_none())
        .collect::<Vec<_>>();
    for package in collect_reachable_packages(&metadata, &manifest)? {
        if package.name == "syn"
            && package.source.as_deref()
                != Some("registry+https://github.com/rust-lang/crates.io-index")
        {
            return Err(invalid("unsupported producer syn macro owner"));
        }
        if package.source.is_none()
            && package.dependencies.iter().any(|dependency| {
                dependency.rename.as_deref() == Some("syn") && dependency.name != "syn"
            })
        {
            return Err(invalid("producer dependency alias cannot impersonate syn"));
        }
    }
    let actual = local
        .iter()
        .map(|package| {
            Path::new(&package.manifest_path)
                .parent()
                .map(canonical_or_original)
                .ok_or_else(|| invalid("missing producer package root"))
        })
        .collect::<Result<BTreeSet<_>, _>>()?;
    let expected = PRODUCERS
        .into_iter()
        .map(|path| root.join(path))
        .collect::<BTreeSet<_>>();
    if actual != expected {
        return Err(invalid("unknown local producer dependency"));
    }
    let mut paths = BTreeSet::new();
    for package in &local {
        let dir = Path::new(&package.manifest_path)
            .parent()
            .ok_or_else(|| invalid("missing producer root"))?;
        if package.targets.iter().any(|target| {
            target
                .kind
                .iter()
                .any(|kind| kind == "custom-build" || kind == "proc-macro")
        }) || dir.join("build.rs").exists()
            || dir.join(".cargo").exists()
        {
            return Err(invalid("unsupported producer build/config input"));
        }
        paths.extend(collect_source_files_for_hash(dir)?);
        let mut visited = BTreeSet::new();
        for target in &package.targets {
            if target
                .kind
                .iter()
                .any(|kind| kind == "lib" || kind == "bin")
            {
                guard::walk(root, Path::new(&target.src_path), true, &mut visited)?;
            }
        }
    }
    for name in FIXED {
        if !image || !matches!(name, "docker/wasm-builder/Dockerfile" | ".dockerignore") {
            paths.insert(root.join(name));
        }
    }
    Ok(paths)
}
fn records(root: &Path, image: bool) -> Result<Vec<SourceInputRecord>, SourceHashError> {
    let mut records = Vec::new();
    for file in files(root, image)? {
        validate_file(root, &file)?;
        let path = file
            .strip_prefix(root)
            .map_err(invalid)?
            .to_str()
            .ok_or_else(|| invalid("non UTF8 recipe path"))?
            .replace('\\', "/");
        records.push(SourceInputRecord {
            kind: "source-file".into(),
            path,
            sha256: sha256_hex(&fs::read(&file).map_err(|source| SourceHashError::Io {
                path: Some(file),
                source,
            })?),
        });
    }
    records.sort_by(|a, b| a.path.as_bytes().cmp(b.path.as_bytes()));
    Ok(records)
}
pub fn compute_builder_recipe(root: &Path) -> Result<BuilderRecipe, SourceHashError> {
    let root = root.canonicalize().map_err(|source| SourceHashError::Io {
        path: Some(root.into()),
        source,
    })?;
    let mut recipe = BuilderRecipe {
        recipe_schema_version: 1,
        kind: "producer-content".into(),
        producer_recipe_hash: String::new(),
        arguments: arguments(),
        inputs: records(&root, false)?,
    };
    recipe.producer_recipe_hash = digest(&recipe)?;
    Ok(recipe)
}
pub fn verify_image_producer_recipe(
    root: &Path,
    recipe: &BuilderRecipe,
) -> Result<(), SourceHashError> {
    recipe.validate()?;
    let root = root.canonicalize().map_err(|source| SourceHashError::Io {
        path: Some(root.into()),
        source,
    })?;
    let expected = recipe
        .inputs
        .iter()
        .filter(|record| {
            !matches!(
                record.path.as_str(),
                "docker/wasm-builder/Dockerfile" | ".dockerignore"
            )
        })
        .cloned()
        .collect::<Vec<_>>();
    if expected != records(&root, true)?
        || recipe
            .inputs
            .iter()
            .filter(|record| {
                matches!(
                    record.path.as_str(),
                    "docker/wasm-builder/Dockerfile" | ".dockerignore"
                )
            })
            .count()
            != 2
    {
        return Err(invalid(
            "image producer source does not match wrapper recipe",
        ));
    }
    Ok(())
}

#[path = "producer_recipe_guard.rs"]
mod guard;

/// Bind the actual image returned by this build's iidfile to its declared recipe.
/// Docker labels are wrapper provenance, not a signature or OCI reproducibility proof.
pub fn candidate_builder_evidence(
    recipe: BuilderRecipe,
    inspected: &serde_json::Value,
) -> Result<CandidateBuilderEvidence, String> {
    let images = inspected
        .as_array()
        .ok_or("Docker inspection must be an array")?;
    if images.len() != 1 {
        return Err("Docker inspection must contain exactly one image".into());
    }
    let image = &images[0];
    let labels = &image["Config"]["Labels"];
    if image["Os"] != "linux"
        || image["Architecture"] != "amd64"
        || labels["io.oasis7.candidate.purpose"] != "candidate-build"
        || labels["io.oasis7.candidate.producer-recipe"] != recipe.producer_recipe_hash
    {
        return Err("Docker inspection does not match candidate recipe/platform labels".into());
    }
    let evidence = CandidateBuilderEvidence {
        purpose: "candidate-build".into(),
        build_manifest_schema_version: 3,
        builder_execution: BuilderExecution {
            execution_schema_version: 1,
            kind: "local-docker-build".into(),
            image_ref: format!(
                "oasis7/wasm-builder:candidate-{}",
                recipe.producer_recipe_hash
            ),
            image_id: image["Id"]
                .as_str()
                .ok_or("Docker inspection lacks actual image ID")?
                .into(),
            docker_platform: "linux/amd64".into(),
        },
        builder_recipe: recipe,
    };
    evidence.validate().map_err(|e| e.to_string())?;
    Ok(evidence)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn recipe() -> BuilderRecipe {
        let mut value = BuilderRecipe {
            recipe_schema_version: 1,
            kind: "producer-content".into(),
            producer_recipe_hash: String::new(),
            arguments: arguments(),
            inputs: vec![SourceInputRecord {
                kind: "source-file".into(),
                path: "Cargo.lock".into(),
                sha256: "0".repeat(64),
            }],
        };
        value.producer_recipe_hash = digest(&value).unwrap();
        value
    }

    #[test]
    fn actual_execution_inspection_is_separate_from_content_recipe() {
        let recipe = recipe();
        let image = serde_json::json!([{"Id":format!("sha256:{}", "a".repeat(64)), "Os":"linux", "Architecture":"amd64", "Config":{"Labels":{"io.oasis7.candidate.purpose":"candidate-build","io.oasis7.candidate.producer-recipe":recipe.producer_recipe_hash}}}]);
        let first = candidate_builder_evidence(recipe.clone(), &image).unwrap();
        let mut other = image.clone();
        other[0]["Id"] = format!("sha256:{}", "b".repeat(64)).into();
        let second = candidate_builder_evidence(recipe.clone(), &other).unwrap();
        assert_eq!(first.builder_recipe, second.builder_recipe);
        assert_ne!(
            first.builder_execution.image_id,
            second.builder_execution.image_id
        );
        for (field, value) in [
            ("Id", "old-config-pin"),
            ("Architecture", "arm64"),
            ("Os", "darwin"),
        ] {
            let mut invalid = image.clone();
            invalid[0][field] = value.into();
            assert!(candidate_builder_evidence(recipe.clone(), &invalid).is_err());
        }
        let mut invalid = image;
        invalid[0]["Config"]["Labels"]["io.oasis7.candidate.producer-recipe"] =
            "other-recipe".into();
        assert!(candidate_builder_evidence(recipe, &invalid).is_err());
    }

    #[test]
    fn recipe_unknown_versions_mixed_execution_and_input_mutations_fail_closed() {
        let original = recipe();
        let mut changed = original.clone();
        changed.inputs[0].sha256 = "1".repeat(64);
        assert!(changed.validate().is_err());
        let mut changed = original.clone();
        changed.recipe_schema_version = 99;
        assert!(changed.validate().is_err());
        let mut changed = original.clone();
        changed.inputs[0].path = "../escape".into();
        assert!(digest(&changed).is_err());
        let mut json = serde_json::to_value(&original).unwrap();
        json["unknown"] = true.into();
        assert!(serde_json::from_value::<BuilderRecipe>(json).is_err());
    }
    #[test]
    fn dockerfile_lane_rejects_unknown_copy_config_and_shell_instructions() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let original = fs::read_to_string(root.join("docker/wasm-builder/Dockerfile")).unwrap();
        let dir = std::env::temp_dir().join(format!(
            "producer-dockerfile-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(dir.join("docker/wasm-builder")).unwrap();
        let path = dir.join("docker/wasm-builder/Dockerfile");
        fs::write(&path, &original).unwrap();
        validate_dockerfile(&dir).unwrap();
        for addition in [
            "COPY .cargo /opt/oasis7/.cargo",
            "copy . /opt/oasis7",
            "ADD external /opt/oasis7/input",
            "RUN cp /external/config /opt/oasis7/.cargo/config",
            "ENV CARGO_ENCODED_RUSTFLAGS=external",
            "WORKDIR /other",
        ] {
            fs::write(&path, format!("{original}\n{addition}\n")).unwrap();
            assert!(validate_dockerfile(&dir).is_err(), "accepted {addition}");
        }
        fs::remove_dir_all(dir).unwrap();
    }
}
