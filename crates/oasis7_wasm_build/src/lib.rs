use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

pub const DEFAULT_WASM_TARGET: &str = "wasm32-unknown-unknown";

#[derive(Debug)]
pub enum SourceHashError {
    InvalidManifestPath(String),
    MetadataCommand {
        manifest_path: PathBuf,
        stderr: String,
        status_code: Option<i32>,
    },
    WorkspaceCommand {
        manifest_path: PathBuf,
        stderr: String,
        status_code: Option<i32>,
    },
    MetadataJson(serde_json::Error),
    MissingRootPackage(PathBuf),
    MissingResolveGraph,
    MissingPackageNode(String),
    PathOutsideWorkspace {
        workspace_root: PathBuf,
        path: PathBuf,
    },
    NoTrackedFiles(PathBuf),
    Io {
        path: Option<PathBuf>,
        source: io::Error,
    },
}

impl fmt::Display for SourceHashError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SourceHashError::InvalidManifestPath(detail) => write!(f, "{detail}"),
            SourceHashError::MetadataCommand {
                manifest_path,
                stderr,
                status_code,
            } => write!(
                f,
                "cargo metadata failed for {} (status={status_code:?}): {}",
                manifest_path.display(),
                stderr
            ),
            SourceHashError::WorkspaceCommand {
                manifest_path,
                stderr,
                status_code,
            } => write!(
                f,
                "cargo locate-project --workspace failed for {} (status={status_code:?}): {}",
                manifest_path.display(),
                stderr
            ),
            SourceHashError::MetadataJson(source) => {
                write!(f, "failed to parse cargo metadata output: {source}")
            }
            SourceHashError::MissingRootPackage(path) => {
                write!(f, "root package not found for manifest {}", path.display())
            }
            SourceHashError::MissingResolveGraph => {
                write!(f, "cargo metadata did not return a resolve graph")
            }
            SourceHashError::MissingPackageNode(id) => {
                write!(f, "cargo metadata missing package for node {id}")
            }
            SourceHashError::PathOutsideWorkspace {
                workspace_root,
                path,
            } => write!(
                f,
                "path {} is outside workspace root {}",
                path.display(),
                workspace_root.display()
            ),
            SourceHashError::NoTrackedFiles(path) => {
                write!(
                    f,
                    "source whitelist produced no tracked files under {}",
                    path.display()
                )
            }
            SourceHashError::Io { path, source } => {
                if let Some(path) = path {
                    write!(f, "io error at {}: {}", path.display(), source)
                } else {
                    write!(f, "io error: {source}")
                }
            }
        }
    }
}

impl std::error::Error for SourceHashError {}

pub fn compute_source_hash(manifest_path: &Path, target: &str) -> Result<String, SourceHashError> {
    let manifest_path = canonical_or_original(manifest_path);
    let Some(module_dir) = manifest_path.parent() else {
        return Err(SourceHashError::InvalidManifestPath(format!(
            "manifest has no parent: {}",
            manifest_path.display()
        )));
    };
    let source_manifest_rel = manifest_path
        .strip_prefix(module_dir)
        .unwrap_or(manifest_path.as_path())
        .to_string_lossy()
        .to_string();
    let metadata = read_cargo_metadata(&manifest_path, target)?;
    let package_dirs = collect_local_package_dirs(&metadata, &manifest_path)?;
    let workspace_manifests = collect_owning_workspace_manifests(&package_dirs)?;
    let mut input_dirs = package_dirs.clone();
    input_dirs.extend(
        workspace_manifests
            .iter()
            .filter_map(|path| path.parent().map(Path::to_path_buf)),
    );
    let workspace_root = workspace_root(&metadata, &manifest_path, &input_dirs);
    let mut hasher = Sha256::new();
    hasher.update(format!("source_manifest_rel={source_manifest_rel}\n").as_bytes());
    if let Some(lockfile) = find_lockfile_for_metadata(&manifest_path) {
        hash_metadata_lockfile(&mut hasher, &workspace_root, lockfile.as_path())?;
    }

    for manifest in workspace_manifests {
        let rel = manifest.strip_prefix(&workspace_root).map_err(|_| {
            SourceHashError::PathOutsideWorkspace {
                workspace_root: workspace_root.clone(),
                path: manifest.clone(),
            }
        })?;
        let bytes = fs::read(&manifest).map_err(|source| SourceHashError::Io {
            path: Some(manifest.clone()),
            source,
        })?;
        hasher.update(
            format!(
                "workspace_manifest:{}:{}\n",
                portable_label(rel),
                sha256_hex(&bytes)
            )
            .as_bytes(),
        );
    }

    for package_dir in package_dirs {
        let package_manifest = package_dir.join("Cargo.toml");
        let manifest_rel = package_manifest
            .strip_prefix(&workspace_root)
            .map_err(|_| SourceHashError::PathOutsideWorkspace {
                workspace_root: workspace_root.clone(),
                path: package_dir.join("Cargo.toml"),
            })?;
        let manifest_rel = portable_label(manifest_rel);
        hasher.update(format!("package_manifest_rel={manifest_rel}\n").as_bytes());
        let files = collect_source_files_for_hash(&package_dir)?;
        for file in files {
            let rel = file
                .strip_prefix(&package_dir)
                .map_err(|_| SourceHashError::Io {
                    path: Some(file.clone()),
                    source: io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!(
                            "failed to strip package dir prefix {}",
                            package_dir.display()
                        ),
                    ),
                })?;
            let bytes = fs::read(&file).map_err(|source| SourceHashError::Io {
                path: Some(file.clone()),
                source,
            })?;
            hasher.update(
                format!(
                    "package_file:{}:{}:{}\n",
                    manifest_rel,
                    portable_label(rel),
                    sha256_hex(&bytes)
                )
                .as_bytes(),
            );
        }
    }

    Ok(hex::encode(hasher.finalize()))
}

fn portable_label(path: &Path) -> String {
    path.components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

fn collect_owning_workspace_manifests(
    package_dirs: &[PathBuf],
) -> Result<Vec<PathBuf>, SourceHashError> {
    let package_manifests = package_dirs
        .iter()
        .map(|dir| dir.join("Cargo.toml"))
        .collect::<BTreeSet<_>>();
    let mut manifests = Vec::new();
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
        if !package_manifests.contains(&owner) {
            manifests.push(owner);
        }
    }
    sort_dedup_paths(&mut manifests);
    Ok(manifests)
}

fn hash_metadata_lockfile(
    hasher: &mut Sha256,
    workspace_root: &Path,
    lockfile: &Path,
) -> Result<(), SourceHashError> {
    if !lockfile.is_file() {
        return Ok(());
    }
    let canonical_lockfile = canonical_or_original(lockfile);
    let label = stable_lockfile_label(workspace_root, canonical_lockfile.as_path());
    let bytes = fs::read(canonical_lockfile.as_path()).map_err(|source| SourceHashError::Io {
        path: Some(canonical_lockfile.clone()),
        source,
    })?;
    hasher.update(format!("metadata_lockfile:{label}:{}\n", sha256_hex(&bytes)).as_bytes());
    Ok(())
}

fn stable_lockfile_label(workspace_root: &Path, lockfile: &Path) -> String {
    let workspace_root = canonical_or_original(workspace_root);
    if let Ok(rel) = lockfile.strip_prefix(workspace_root.as_path()) {
        return portable_label(rel);
    }
    let paths = [workspace_root, lockfile.to_path_buf()];
    if let Some(anchor) = common_ancestor(&paths)
        && let Ok(rel) = lockfile.strip_prefix(anchor.as_path())
    {
        return portable_label(rel);
    }
    lockfile
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| "Cargo.lock".to_string())
}

fn read_cargo_metadata(
    manifest_path: &Path,
    target: &str,
) -> Result<CargoMetadata, SourceHashError> {
    let mut args = vec![
        "metadata".to_string(),
        "--manifest-path".to_string(),
        manifest_path.to_string_lossy().to_string(),
        "--format-version".to_string(),
        "1".to_string(),
        "--filter-platform".to_string(),
        target.to_string(),
    ];
    let lock_path = find_lockfile_for_metadata(manifest_path);
    if lock_path.is_some() {
        args.push("--locked".to_string());
    }

    let mut command = Command::new("cargo");
    command.env_remove("RUSTC_WRAPPER");
    command.args(&args);
    let output = command
        .output()
        .map_err(|source| SourceHashError::Io { path: None, source })?;
    if !output.status.success() {
        return Err(SourceHashError::MetadataCommand {
            manifest_path: manifest_path.to_path_buf(),
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
            status_code: output.status.code(),
        });
    }
    serde_json::from_slice(&output.stdout).map_err(SourceHashError::MetadataJson)
}

fn find_lockfile_for_metadata(manifest_path: &Path) -> Option<PathBuf> {
    let mut current = manifest_path.parent();
    while let Some(dir) = current {
        let lock_path = dir.join("Cargo.lock");
        if lock_path.exists() {
            return Some(lock_path);
        }
        current = dir.parent();
    }
    None
}

fn workspace_root(
    metadata: &CargoMetadata,
    manifest_path: &Path,
    package_dirs: &[PathBuf],
) -> PathBuf {
    let metadata_root = if metadata.workspace_root.trim().is_empty() {
        manifest_path
            .parent()
            .map(canonical_or_original)
            .unwrap_or_else(|| canonical_or_original(manifest_path))
    } else {
        canonical_or_original(Path::new(metadata.workspace_root.as_str()))
    };
    if package_dirs
        .iter()
        .all(|package_dir| package_dir.starts_with(&metadata_root))
    {
        return metadata_root;
    }
    common_ancestor(package_dirs).unwrap_or(metadata_root)
}

fn collect_local_package_dirs(
    metadata: &CargoMetadata,
    manifest_path: &Path,
) -> Result<Vec<PathBuf>, SourceHashError> {
    let canonical_manifest = canonical_or_original(manifest_path);
    let root_package = metadata
        .packages
        .iter()
        .find(|package| {
            canonical_or_original(Path::new(package.manifest_path.as_str())) == canonical_manifest
        })
        .ok_or_else(|| SourceHashError::MissingRootPackage(canonical_manifest.clone()))?;
    let resolve = metadata
        .resolve
        .as_ref()
        .ok_or(SourceHashError::MissingResolveGraph)?;
    let package_by_id = metadata
        .packages
        .iter()
        .map(|package| (package.id.clone(), package))
        .collect::<BTreeMap<_, _>>();
    let node_by_id = resolve
        .nodes
        .iter()
        .map(|node| (node.id.clone(), node))
        .collect::<BTreeMap<_, _>>();
    let mut visited = BTreeSet::new();
    let mut ordered_dirs = Vec::new();
    let mut queue = VecDeque::from([root_package.id.clone()]);

    while let Some(package_id) = queue.pop_front() {
        if !visited.insert(package_id.clone()) {
            continue;
        }
        let package = package_by_id
            .get(&package_id)
            .copied()
            .ok_or_else(|| SourceHashError::MissingPackageNode(package_id.clone()))?;
        if package.source.is_none() {
            let manifest_path = canonical_or_original(Path::new(package.manifest_path.as_str()));
            let package_dir = manifest_path.parent().ok_or_else(|| {
                SourceHashError::InvalidManifestPath(format!(
                    "package manifest has no parent: {}",
                    manifest_path.display()
                ))
            })?;
            ordered_dirs.push(package_dir.to_path_buf());
        }
        if let Some(node) = node_by_id.get(&package_id) {
            for dep in &node.dependencies {
                queue.push_back(dep.clone());
            }
        }
    }

    sort_dedup_paths(&mut ordered_dirs);
    Ok(ordered_dirs)
}

fn collect_source_files_for_hash(package_dir: &Path) -> Result<Vec<PathBuf>, SourceHashError> {
    let mut files = Vec::new();
    for rel in ["Cargo.toml", "Cargo.lock", "build.rs"] {
        let path = package_dir.join(rel);
        if path.is_file() {
            files.push(path);
        }
    }
    for root in ["src", "wit", ".cargo", "assets"] {
        collect_files_recursively(package_dir.join(root).as_path(), &mut files)?;
    }
    sort_dedup_paths(&mut files);
    if files.is_empty() {
        return Err(SourceHashError::NoTrackedFiles(package_dir.to_path_buf()));
    }
    Ok(files)
}

fn sort_dedup_paths(paths: &mut Vec<PathBuf>) {
    paths.sort_by_cached_key(|path| path.to_string_lossy().into_owned());
    paths.dedup();
}

fn common_ancestor(paths: &[PathBuf]) -> Option<PathBuf> {
    let mut iter = paths.iter();
    let first = iter.next()?.components().collect::<Vec<_>>();
    let mut common_len = first.len();
    for path in iter {
        let components = path.components().collect::<Vec<_>>();
        let shared = first
            .iter()
            .zip(components.iter())
            .take_while(|(left, right)| left == right)
            .count();
        common_len = common_len.min(shared);
        if common_len == 0 {
            break;
        }
    }
    if common_len == 0 {
        return None;
    }
    let mut root = PathBuf::new();
    for component in first.into_iter().take(common_len) {
        root.push(component.as_os_str());
    }
    Some(root)
}

fn collect_files_recursively(dir: &Path, output: &mut Vec<PathBuf>) -> Result<(), SourceHashError> {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(source) => {
            return Err(SourceHashError::Io {
                path: Some(dir.to_path_buf()),
                source,
            });
        }
    };
    for entry in entries {
        let entry = entry.map_err(|source| SourceHashError::Io {
            path: Some(dir.to_path_buf()),
            source,
        })?;
        let path = entry.path();
        let file_type = entry.file_type().map_err(|source| SourceHashError::Io {
            path: Some(path.clone()),
            source,
        })?;
        if file_type.is_dir() {
            collect_files_recursively(path.as_path(), output)?;
            continue;
        }
        if file_type.is_file() {
            output.push(path);
        }
    }
    Ok(())
}

fn canonical_or_original(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

#[derive(Debug, Deserialize)]
struct CargoMetadata {
    #[serde(default)]
    workspace_root: String,
    packages: Vec<CargoPackage>,
    resolve: Option<CargoResolve>,
}

#[derive(Debug, Deserialize)]
struct CargoPackage {
    id: String,
    manifest_path: String,
    #[serde(default)]
    source: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CargoResolve {
    nodes: Vec<CargoResolveNode>,
}

#[derive(Debug, Deserialize)]
struct CargoResolveNode {
    id: String,
    #[serde(default)]
    dependencies: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_temp_dir(prefix: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time before unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("{prefix}-{}-{nanos}", std::process::id()))
    }

    fn write_file(path: &Path, contents: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create parent dirs");
        }
        fs::write(path, contents).expect("write file");
    }

    fn write_fixture_workspace(
        root: &Path,
        shared_body: &str,
        extra_note: Option<&str>,
    ) -> PathBuf {
        write_file(
            &root.join("Cargo.toml"),
            "[workspace]\nmembers = [\"module\", \"shared\"]\nresolver = \"2\"\n",
        );
        write_file(
            &root.join("module/Cargo.toml"),
            "[package]\nname = \"fixture_module\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[lib]\ncrate-type = [\"cdylib\"]\n\n[dependencies]\nfixture_shared = { path = \"../shared\" }\n",
        );
        write_file(
            &root.join("module/src/lib.rs"),
            "pub fn call() -> u32 { fixture_shared::value() }\n",
        );
        if let Some(note) = extra_note {
            write_file(&root.join("module/README.md"), note);
        }
        write_file(
            &root.join("shared/Cargo.toml"),
            "[package]\nname = \"fixture_shared\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
        );
        write_file(&root.join("shared/src/lib.rs"), shared_body);
        root.join("module/Cargo.toml")
    }

    fn write_fixture_split_workspace(root: &Path) -> PathBuf {
        write_file(
            &root.join("module/Cargo.toml"),
            "[workspace]\n\n[package]\nname = \"fixture_module\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[lib]\ncrate-type = [\"cdylib\"]\n\n[dependencies]\nfixture_shared = { path = \"../shared\" }\n",
        );
        write_file(
            &root.join("module/src/lib.rs"),
            "pub fn call() -> u32 { fixture_shared::value() }\n",
        );
        write_file(
            &root.join("shared/Cargo.toml"),
            "[package]\nname = \"fixture_shared\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
        );
        write_file(
            &root.join("shared/src/lib.rs"),
            "pub fn value() -> u32 { 1 }\n",
        );
        root.join("module/Cargo.toml")
    }

    fn write_fixture_workspace_lockfile(root: &Path, note: &str) {
        write_file(
            &root.join("Cargo.lock"),
            &format!(
                "# This file is automatically @generated by Cargo.\n# {note}\nversion = 4\n\n[[package]]\nname = \"fixture_module\"\nversion = \"0.1.0\"\ndependencies = [\n \"fixture_shared\",\n]\n\n[[package]]\nname = \"fixture_shared\"\nversion = \"0.1.0\"\n"
            ),
        );
    }

    fn write_fixture_inherited_workspace(root: &Path, edition: &str) -> PathBuf {
        let manifest = write_fixture_split_workspace(&root.join("crates"));
        write_file(
            &root.join("Cargo.toml"),
            &format!(
                "[workspace]\nmembers = [\"crates/shared\"]\nexclude = [\"crates/module\"]\nresolver = \"2\"\n\n[workspace.package]\nedition = \"{edition}\"\nlicense = \"MIT\"\n"
            ),
        );
        write_file(
            &root.join("crates/shared/Cargo.toml"),
            "[package]\nname = \"fixture_shared\"\nversion = \"0.1.0\"\nedition.workspace = true\nlicense.workspace = true\n",
        );
        manifest
    }

    #[test]
    fn source_hash_tracks_dependency_owning_workspace_and_is_portable() {
        let root = unique_temp_dir("oasis7-wasm-build-inherited-workspace");
        let relocated = unique_temp_dir("oasis7-wasm-build-inherited-relocated");
        let manifest = write_fixture_inherited_workspace(&root, "2021");
        let initial = compute_source_hash(&manifest, DEFAULT_WASM_TARGET).expect("initial hash");
        let local_manifest = fs::read(root.join("crates/shared/Cargo.toml")).unwrap();
        let lockfile = fs::read(root.join("crates/module/Cargo.lock")).unwrap();
        write_fixture_inherited_workspace(&root, "2024");
        let changed = compute_source_hash(&manifest, DEFAULT_WASM_TARGET).expect("changed hash");
        assert_eq!(
            local_manifest,
            fs::read(root.join("crates/shared/Cargo.toml")).unwrap()
        );
        assert_eq!(
            lockfile,
            fs::read(root.join("crates/module/Cargo.lock")).unwrap()
        );
        assert_ne!(initial, changed, "inherited edition must be a source input");
        let relocated_manifest = write_fixture_inherited_workspace(&relocated, "2024");
        let relocated_hash =
            compute_source_hash(&relocated_manifest, DEFAULT_WASM_TARGET).expect("relocated hash");
        assert_eq!(
            changed, relocated_hash,
            "labels must not contain absolute paths"
        );
        let _ = fs::remove_dir_all(root);
        let _ = fs::remove_dir_all(relocated);
    }

    #[test]
    fn source_hash_changes_when_local_path_dependency_changes() {
        let root = unique_temp_dir("oasis7-wasm-build-source-hash");
        let manifest_path =
            write_fixture_workspace(&root, "pub fn value() -> u32 { 1 }\n", Some("ignored note"));

        let initial =
            compute_source_hash(&manifest_path, DEFAULT_WASM_TARGET).expect("initial source hash");
        write_file(
            &root.join("shared/src/lib.rs"),
            "pub fn value() -> u32 { 2 }\n",
        );
        let changed =
            compute_source_hash(&manifest_path, DEFAULT_WASM_TARGET).expect("changed source hash");
        assert_ne!(initial, changed);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn source_hash_changes_when_workspace_ancestor_lockfile_changes() {
        let root = unique_temp_dir("oasis7-wasm-build-source-hash-lockfile");
        let manifest_path = write_fixture_workspace(&root, "pub fn value() -> u32 { 1 }\n", None);
        write_fixture_workspace_lockfile(&root, "initial lockfile");

        let initial =
            compute_source_hash(&manifest_path, DEFAULT_WASM_TARGET).expect("initial source hash");
        write_fixture_workspace_lockfile(&root, "changed lockfile");
        let changed =
            compute_source_hash(&manifest_path, DEFAULT_WASM_TARGET).expect("changed source hash");
        assert_ne!(initial, changed);

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn source_hash_ignores_non_whitelisted_files() {
        let root = unique_temp_dir("oasis7-wasm-build-source-hash-note");
        let manifest_path =
            write_fixture_workspace(&root, "pub fn value() -> u32 { 1 }\n", Some("note one"));

        let initial =
            compute_source_hash(&manifest_path, DEFAULT_WASM_TARGET).expect("initial source hash");
        write_file(&root.join("module/README.md"), "note two");
        let unchanged = compute_source_hash(&manifest_path, DEFAULT_WASM_TARGET)
            .expect("unchanged source hash");
        assert_eq!(initial, unchanged);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn source_hash_allows_local_path_dependencies_outside_metadata_workspace_root() {
        let root = unique_temp_dir("oasis7-wasm-build-split-workspace");
        let manifest_path = write_fixture_split_workspace(&root);

        let hash = compute_source_hash(&manifest_path, DEFAULT_WASM_TARGET).expect("source hash");
        assert!(!hash.is_empty());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn find_lockfile_for_metadata_searches_workspace_ancestors() {
        let root = unique_temp_dir("oasis7-wasm-build-lockfile");
        let manifest_path = write_fixture_workspace(&root, "pub fn value() -> u32 { 1 }\n", None);
        let workspace_lock = root.join("Cargo.lock");
        write_file(&workspace_lock, "# lockfile\n");

        let found = find_lockfile_for_metadata(&manifest_path);
        assert_eq!(found.as_deref(), Some(workspace_lock.as_path()));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn owning_workspace_lookup_fails_closed_for_invalid_manifest() {
        let root = unique_temp_dir("oasis7-wasm-build-invalid-owner");
        write_file(&root.join("Cargo.toml"), "invalid manifest");
        let error = collect_owning_workspace_manifests(std::slice::from_ref(&root)).unwrap_err();
        assert!(matches!(error, SourceHashError::WorkspaceCommand { .. }));
        assert!(
            error
                .to_string()
                .contains("cargo locate-project --workspace failed")
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn owning_workspace_manifests_are_distinct_and_skip_local_manifests() {
        let root = unique_temp_dir("oasis7-wasm-build-owner-dedup");
        write_fixture_workspace(&root, "pub fn value() -> u32 { 1 }\n", None);
        let dirs = [root.join("module"), root.join("shared")];
        assert_eq!(
            collect_owning_workspace_manifests(&dirs).unwrap(),
            vec![canonical_or_original(&root.join("Cargo.toml"))]
        );
        // A package manifest already in the closure must not be hashed a second time.
        let manifest = write_fixture_split_workspace(&root.join("standalone"));
        let standalone = canonical_or_original(manifest.parent().unwrap());
        assert!(
            collect_owning_workspace_manifests(&[standalone])
                .unwrap()
                .is_empty()
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn sort_dedup_paths_preserves_source_hash_label_order() {
        let mut paths = vec![
            PathBuf::from("/tmp/workspace/zeta/src/lib.rs"),
            PathBuf::from("/tmp/workspace/zeta/src/foo/bar.rs"),
            PathBuf::from("/tmp/workspace/zeta/src/foo.rs"),
            PathBuf::from("/tmp/workspace/alpha/Cargo.toml"),
            PathBuf::from("/tmp/workspace/zeta/src/lib.rs"),
        ];

        sort_dedup_paths(&mut paths);

        assert_eq!(
            paths,
            vec![
                PathBuf::from("/tmp/workspace/alpha/Cargo.toml"),
                PathBuf::from("/tmp/workspace/zeta/src/foo.rs"),
                PathBuf::from("/tmp/workspace/zeta/src/foo/bar.rs"),
                PathBuf::from("/tmp/workspace/zeta/src/lib.rs"),
            ]
        );
    }
}
