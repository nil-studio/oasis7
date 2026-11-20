//! Cargo ownership anchors and the workspace manifests a package inherits.
use super::*;

#[derive(Debug)]
pub(super) struct WorkspaceManifests {
    // Keep every owner for stable source-root-relative labels, even when its
    // contents do not configure a dependency in this build workspace.
    pub(super) all: Vec<PathBuf>,
    pub(super) inherited: Vec<PathBuf>,
}

pub(super) fn collect_owning_workspace_manifests(
    package_dirs: &[PathBuf],
) -> Result<WorkspaceManifests, SourceHashError> {
    let package_manifests = package_dirs
        .iter()
        .map(|dir| dir.join("Cargo.toml"))
        .collect::<BTreeSet<_>>();
    let mut manifests = WorkspaceManifests {
        all: Vec::new(),
        inherited: Vec::new(),
    };
    for manifest_path in &package_manifests {
        let output = Command::new("cargo")
            .env_remove("RUSTC_WRAPPER")
            .args(["locate-project", "--workspace", "--manifest-path"])
            .arg(manifest_path)
            .args(["--message-format", "plain"])
            .output()
            .map_err(|source| SourceHashError::Io {
                path: Some(manifest_path.clone()),
                source,
            })?;
        if !output.status.success() {
            return Err(SourceHashError::WorkspaceCommand {
                manifest_path: manifest_path.clone(),
                stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
                status_code: output.status.code(),
            });
        }
        let path = String::from_utf8(output.stdout).map_err(|source| SourceHashError::Io {
            path: Some(manifest_path.clone()),
            source: io::Error::new(io::ErrorKind::InvalidData, source),
        })?;
        if path.trim().is_empty() {
            return Err(SourceHashError::InvalidManifestPath(format!(
                "cargo locate-project --workspace returned an empty path for {}",
                manifest_path.display()
            )));
        }
        let owner_path = Path::new(path.trim());
        let owner = fs::canonicalize(owner_path).map_err(|source| SourceHashError::Io {
            path: Some(owner_path.to_path_buf()),
            source,
        })?;
        // Local package manifests already enter the source-file set directly.
        if !package_manifests.contains(&owner) {
            let text = fs::read_to_string(manifest_path).map_err(|source| SourceHashError::Io {
                path: Some(manifest_path.clone()),
                source,
            })?;
            let manifest = toml::from_str::<toml::Table>(&text).map_err(|error| {
                SourceHashError::UnsupportedInput(format!(
                    "unsupported-input: cannot parse workspace inheritance in {}: {error}",
                    manifest_path.display()
                ))
            })?;
            if inherits_workspace(&manifest) {
                manifests.inherited.push(owner.clone());
            }
            manifests.all.push(owner);
        }
    }
    sort_dedup_paths(&mut manifests.all);
    sort_dedup_paths(&mut manifests.inherited);
    Ok(manifests)
}

fn workspace_true(value: &toml::Value) -> bool {
    value.get("workspace").and_then(toml::Value::as_bool) == Some(true)
}

fn dependency_inheritance(table: &toml::Table) -> bool {
    [
        "dependencies",
        "build-dependencies",
        "dev-dependencies",
        // Cargo still accepts these aliases for pre-2024 manifests.
        "build_dependencies",
        "dev_dependencies",
    ]
    .iter()
    .filter_map(|key| table.get(*key).and_then(toml::Value::as_table))
    .any(|dependencies| dependencies.values().any(workspace_true))
}

fn inherits_workspace(manifest: &toml::Table) -> bool {
    ["package", "project"]
        .iter()
        .filter_map(|key| manifest.get(*key).and_then(toml::Value::as_table))
        .any(|package| {
            package
                .iter()
                .any(|(key, value)| key != "metadata" && workspace_true(value))
        })
        || manifest.get("lints").is_some_and(workspace_true)
        || dependency_inheritance(manifest)
        || manifest
            .get("target")
            .and_then(toml::Value::as_table)
            .is_some_and(|targets| {
                targets
                    .values()
                    .filter_map(toml::Value::as_table)
                    .any(dependency_inheritance)
            })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_cargo_workspace_inheritance_syntax() {
        for text in [
            "[package]\nedition.workspace = true",
            "[package]\nversion = { workspace = true }",
            "[project]\nlicense.workspace = true",
            "[dependencies]\nshared = { workspace = true, features = ['wire'] }",
            "[build-dependencies.shared]\nworkspace = true",
            "[dev-dependencies]\nshared.workspace = true",
            "[build_dependencies.shared]\nworkspace = true",
            "[dev_dependencies]\nshared.workspace = true",
            "[target.'cfg(target_arch = \"wasm32\")'.dependencies.shared]\nworkspace = true",
            "[target.'cfg(unix)'.build-dependencies]\nshared.workspace = true",
            "[target.'cfg(unix)'.dev-dependencies]\nshared.workspace = true",
            "[target.'cfg(unix)'.build_dependencies]\nshared.workspace = true",
            "[target.'cfg(unix)'.dev_dependencies]\nshared.workspace = true",
            "['lints']\n'workspace' = true",
        ] {
            assert!(inherits_workspace(&toml::from_str(text).unwrap()), "{text}");
        }
    }

    #[test]
    fn ignores_comments_metadata_and_explicit_dependencies() {
        for text in [
            "# edition.workspace = true\n[package]\nedition = '2024'",
            "[package.metadata]\nworkspace = true\n[package.metadata.dependencies.shared]\nworkspace = true",
            "[workspace.metadata]\nworkspace = true",
            "[dependencies]\nshared = { path = '../shared' }\nworkspace = '1'",
            "[lints.rust]\nunsafe_code = 'forbid'",
        ] {
            assert!(
                !inherits_workspace(&toml::from_str(text).unwrap()),
                "{text}"
            );
        }
    }
}
