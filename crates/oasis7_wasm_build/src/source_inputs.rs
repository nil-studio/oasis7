//! The v2 source-input contract shared by build producers and identity readers.
use super::*;
use serde::{Deserialize, Serialize};
use std::path::Component;
use syn::visit::Visit;

pub const SOURCE_INPUT_RULE_VERSION: u32 = 2;
const DECLARATION_KEY: &str = "oasis7_source_inputs";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceInputRecord {
    pub kind: String,
    pub path: String,
    pub sha256: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceInputs {
    pub source_input_rule_version: u32,
    pub source_hash: String,
    pub source_inputs: Vec<SourceInputRecord>,
}
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Edge {
    from: String,
    to: String,
}
#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Declaration {
    rule_version: u32,
    includes: Vec<Edge>,
    build_inputs: BTreeMap<String, Vec<String>>,
}

fn unsupported(detail: impl fmt::Display) -> SourceHashError {
    SourceHashError::UnsupportedInput(format!("unsupported-input: {detail}"))
}
fn read(path: &Path) -> Result<Vec<u8>, SourceHashError> {
    fs::read(path).map_err(|source| SourceHashError::Io {
        path: Some(path.into()),
        source,
    })
}
fn label(root: &Path, path: &Path) -> Result<String, SourceHashError> {
    let rel = path
        .strip_prefix(root)
        .map_err(|_| unsupported(format!("path outside source root: {}", path.display())))?;
    let mut labels = Vec::new();
    for part in rel.components() {
        let Component::Normal(value) = part else {
            return Err(unsupported("ambiguous relative path"));
        };
        let text = value
            .to_str()
            .ok_or_else(|| unsupported("non-UTF-8 path"))?;
        if text.contains('\\') || text.is_empty() {
            return Err(unsupported("ambiguous path component"));
        }
        labels.push(text);
    }
    if labels.is_empty() {
        return Err(unsupported("empty input path"));
    }
    Ok(labels.join("/"))
}
fn declared_path(root: &Path, value: &str) -> Result<PathBuf, SourceHashError> {
    let path = root.join(value);
    if label(root, &path)? != value || Path::new(value).is_absolute() {
        return Err(unsupported(format!("noncanonical declared path {value}")));
    }
    validate_file(root, &path)?;
    Ok(path)
}
pub(super) fn validate_file(root: &Path, path: &Path) -> Result<(), SourceHashError> {
    label(root, path)?;
    let rel = path
        .strip_prefix(root)
        .map_err(|_| unsupported("outside root"))?;
    let mut current = root.to_path_buf();
    for part in rel.components() {
        current.push(part);
        let meta = fs::symlink_metadata(&current).map_err(|source| SourceHashError::Io {
            path: Some(current.clone()),
            source,
        })?;
        if meta.file_type().is_symlink() {
            return Err(unsupported(format!("symlink input {}", current.display())));
        }
    }
    if !path.is_file() {
        return Err(unsupported(format!(
            "input is not a file: {}",
            path.display()
        )));
    }
    Ok(())
}
pub(super) fn include_path(
    root: &Path,
    from: &Path,
    value: &str,
) -> Result<PathBuf, SourceHashError> {
    if Path::new(value).is_absolute() || value.contains('\\') {
        return Err(unsupported("absolute/ambiguous include path"));
    }
    let mut resolved = from
        .parent()
        .ok_or_else(|| unsupported("input lacks parent"))?
        .to_path_buf();
    let components = Path::new(value).components().collect::<Vec<_>>();
    for (index, part) in components.iter().enumerate() {
        match part {
            Component::Normal(v) => {
                resolved.push(v);
                let metadata =
                    fs::symlink_metadata(&resolved).map_err(|source| SourceHashError::Io {
                        path: Some(resolved.clone()),
                        source,
                    })?;
                if metadata.file_type().is_symlink() {
                    return Err(unsupported("symlink include component"));
                }
                if index + 1 < components.len() && !metadata.is_dir() {
                    return Err(unsupported("include traverses a non-directory"));
                }
            }
            Component::CurDir => {}
            Component::ParentDir => {
                if resolved == root || !resolved.pop() {
                    return Err(unsupported("include escapes source root"));
                }
            }
            _ => return Err(unsupported("unsupported include path")),
        }
    }
    validate_file(root, &resolved)?;
    Ok(resolved)
}

/// Missing version is legacy, never a new-rule proof. No reader falls back to v1.
pub fn validate_source_evidence(
    version: Option<u32>,
    records: Option<&[SourceInputRecord]>,
    hash: &str,
    expected: &SourceInputs,
) -> Result<(), SourceHashError> {
    match version {
        None => {
            return Err(unsupported(
                "legacy receipt lacks source_input_rule_version; canonical rebuild required",
            ));
        }
        Some(SOURCE_INPUT_RULE_VERSION) => {}
        Some(value) => {
            return Err(unsupported(format!(
                "unknown/mismatched source_input_rule_version={value}; canonical rebuild required"
            )));
        }
    }
    if records != Some(expected.source_inputs.as_slice()) {
        return Err(unsupported(
            "source input details mismatch; canonical rebuild required",
        ));
    }
    if hash != expected.source_hash {
        return Err(unsupported(
            "source hash mismatch; canonical rebuild required",
        ));
    }
    Ok(())
}

pub(super) fn validate_macro_owners(
    metadata: &CargoMetadata,
    manifest: &Path,
) -> Result<(), SourceHashError> {
    validate_macro_owners_with_procedural(metadata, manifest, &["serde_derive"])
}

pub(super) fn validate_macro_owners_with_procedural(
    metadata: &CargoMetadata,
    manifest: &Path,
    procedural: &[&str],
) -> Result<(), SourceHashError> {
    for package in collect_reachable_packages(metadata, manifest)? {
        let crates_io = package.source.as_deref()
            == Some("registry+https://github.com/rust-lang/crates.io-index");
        if matches!(package.name.as_str(), "serde" | "serde_json") && !crates_io {
            return Err(unsupported(
                "unknown owner for the supported serde macro lane",
            ));
        }
        if package.name == "oasis7_wasm_sdk" && package.source.is_some() {
            return Err(unsupported(
                "SDK macro definitions must be in the checked local closure",
            ));
        }
        for dependency in package
            .dependencies
            .iter()
            .filter(|_| package.source.is_none())
        {
            let binding = dependency
                .rename
                .as_deref()
                .unwrap_or(&dependency.name)
                .replace('-', "_");
            if matches!(binding.as_str(), "std" | "core" | "alloc")
                || matches!(binding.as_str(), "serde" | "serde_json" | "oasis7_wasm_sdk")
                    && binding != dependency.name
            {
                return Err(unsupported(
                    "dependency aliases cannot impersonate a supported macro owner",
                ));
            }
        }

        if package
            .targets
            .iter()
            .any(|target| target.kind.iter().any(|kind| kind == "proc-macro"))
            && !(procedural.contains(&package.name.as_str())
                && package.source.as_deref()
                    == Some("registry+https://github.com/rust-lang/crates.io-index"))
        {
            return Err(unsupported(format!(
                "unknown procedural-macro input lane: {}",
                package.name
            )));
        }
    }

    Ok(())
}

pub fn compute_source_inputs(
    manifest_path: &Path,
    target: &str,
) -> Result<SourceInputs, SourceHashError> {
    if fs::symlink_metadata(manifest_path)
        .map_err(unsupported)?
        .file_type()
        .is_symlink()
    {
        return Err(unsupported("symlink manifest"));
    }
    let original_manifest = std::path::absolute(manifest_path).map_err(unsupported)?;
    if original_manifest
        .components()
        .any(|part| matches!(part, Component::ParentDir))
    {
        return Err(unsupported(
            "manifest path must not contain parent traversal",
        ));
    }
    let manifest = fs::canonicalize(manifest_path).map_err(unsupported)?;
    let metadata = read_cargo_metadata(&manifest, target)?;
    let packages = collect_local_package_dirs(&metadata, &manifest)?;
    validate_macro_owners(&metadata, &manifest)?;

    let owners = collect_owning_workspace_manifests(&packages)?;
    let mut dirs = packages.clone();
    dirs.extend(
        owners
            .iter()
            .filter_map(|v| v.parent().map(Path::to_path_buf)),
    );
    if let Some(lock) = find_lockfile_for_metadata(&manifest) {
        dirs.push(lock.parent().unwrap().to_path_buf());
    }
    let root = workspace_root(&metadata, &manifest, &dirs);
    let anchor = original_manifest
        .ancestors()
        .find(|ancestor| fs::canonicalize(ancestor).is_ok_and(|path| path == root))
        .ok_or_else(|| unsupported("manifest path has no unambiguous source-root anchor"))?;
    if fs::symlink_metadata(anchor)
        .map_err(unsupported)?
        .file_type()
        .is_symlink()
    {
        return Err(unsupported("symlink manifest source-root anchor"));
    }
    let relative = original_manifest
        .strip_prefix(anchor)
        .map_err(unsupported)?;
    validate_file(&root, &root.join(relative))?;
    validate_file(&root, &manifest)?;
    let mut files = owners.into_iter().collect::<BTreeSet<_>>();
    if let Some(lock) = find_lockfile_for_metadata(&manifest) {
        files.insert(lock);
    }
    let mut declarations = Vec::new();
    let mut edges = BTreeSet::new();
    let mut build_inputs = BTreeMap::new();
    for package in &packages {
        files.extend(collect_source_files_for_hash(package)?);
        let path = package.join("Cargo.toml");
        let cargo_package = metadata
            .packages
            .iter()
            .find(|p| Path::new(&p.manifest_path) == path)
            .ok_or_else(|| unsupported("missing local package metadata"))?;
        if let Some(value) = cargo_package.metadata.get(DECLARATION_KEY) {
            validate_file(&root, &path)?;
            let mut declaration: Declaration = serde_json::from_value(value.clone())
                .map_err(|e| unsupported(format!("{}: {e}", path.display())))?;
            if declaration.rule_version != SOURCE_INPUT_RULE_VERSION {
                return Err(unsupported("unknown declaration rule version"));
            }
            declaration.includes.sort();
            declaration.includes.dedup();
            for edge in &declaration.includes {
                declared_path(&root, &edge.from)?;
                declared_path(&root, &edge.to)?;
                edges.insert(edge.clone());
            }
            for (from, inputs) in &mut declaration.build_inputs {
                declared_path(&root, from)?;
                inputs.sort();
                inputs.dedup();
                for input in inputs.iter() {
                    declared_path(&root, input)?;
                }
                if build_inputs.insert(from.clone(), inputs.clone()).is_some() {
                    return Err(unsupported("duplicate build input declaration"));
                }
            }
            let declaration_hash = hash_declaration(&declaration);
            declarations.push(SourceInputRecord {
                kind: "input-declaration".into(),
                path: label(&root, &path)?,
                sha256: declaration_hash,
            });
        }
    }
    let mut rust_roots = BTreeSet::new();
    let mut build_roots = BTreeMap::new();
    for package in metadata.packages.iter().filter(|p| p.source.is_none()) {
        if !packages
            .iter()
            .any(|dir| dir.join("Cargo.toml") == Path::new(&package.manifest_path))
        {
            continue;
        }
        for target in &package.targets {
            if target
                .kind
                .iter()
                .any(|kind| matches!(kind.as_str(), "test" | "bench" | "example"))
            {
                continue;
            }
            let source = Path::new(&target.src_path);
            rust_roots.insert(source.to_path_buf());
            if !files.contains(source) {
                return Err(unsupported(format!(
                    "Cargo target outside supported source roots: {}",
                    source.display()
                )));
            }
            if target.kind.iter().any(|k| k == "custom-build") {
                build_roots.insert(
                    source.to_path_buf(),
                    Path::new(&package.manifest_path)
                        .parent()
                        .ok_or_else(|| unsupported("build package lacks parent"))?
                        .to_path_buf(),
                );
                if package
                    .dependencies
                    .iter()
                    .any(|dependency| dependency.kind.as_deref() == Some("build"))
                {
                    return Err(unsupported(
                        "external build dependencies are outside the supported literal-input lane",
                    ));
                }
            }
        }
    }
    let initial = files.clone();
    let mut closure = Closure {
        root: &root,
        package_files: files.clone(),
        files,
        edges: &edges,
        build_roots: &build_roots,
        used: BTreeSet::new(),
        actual_build_inputs: BTreeMap::new(),
        visited: BTreeSet::new(),
        active: BTreeSet::new(),
    };
    for file in initial {
        closure.walk(
            &file,
            if rust_roots.contains(&file) {
                InputKind::RustRoot
            } else {
                InputKind::Data
            },
            None,
        )?;
    }
    if closure.used != edges {
        return Err(unsupported(
            "declared include edge is not an actual reachable literal include",
        ));
    }
    let observed = closure
        .actual_build_inputs
        .iter()
        .map(|(script, inputs)| (script.clone(), inputs.iter().cloned().collect::<Vec<_>>()))
        .collect::<BTreeMap<_, _>>();
    if observed != build_inputs {
        return Err(unsupported(
            "build.rs requires exact explicit build_inputs for the Cargo script and package working directory",
        ));
    }
    let files = closure.files;
    let mut records = declarations;
    for file in files {
        validate_file(&root, &file)?;
        records.push(SourceInputRecord {
            kind: "source-file".into(),
            path: label(&root, &file)?,
            sha256: sha256_hex(&read(&file)?),
        });
    }
    records.sort_by(|a, b| (&a.path, &a.kind).cmp(&(&b.path, &b.kind)));
    let source_hash = hash_records(&label(&root, &manifest)?, target, &records)?;
    Ok(SourceInputs {
        source_input_rule_version: SOURCE_INPUT_RULE_VERSION,
        source_hash,
        source_inputs: records,
    })
}
fn hash_records(
    manifest: &str,
    target: &str,
    records: &[SourceInputRecord],
) -> Result<String, SourceHashError> {
    let mut hash = Sha256::new();
    hash.update(b"oasis7-source-input-v2");
    put_string(&mut hash, manifest);
    put_string(&mut hash, target);
    hash.update((records.len() as u64).to_be_bytes());
    for record in records {
        put_string(&mut hash, &record.kind);
        put_string(&mut hash, &record.path);
        hash.update(hex::decode(&record.sha256).map_err(unsupported)?);
    }
    Ok(hex::encode(hash.finalize()))
}
fn hash_declaration(declaration: &Declaration) -> String {
    let mut records = declaration
        .includes
        .iter()
        .map(|edge| ("include-edge", edge.from.as_str(), edge.to.as_str()))
        .collect::<Vec<_>>();
    for (from, inputs) in &declaration.build_inputs {
        records.push(("build-script", from.as_str(), ""));
        for to in inputs {
            records.push(("build-input", from.as_str(), to.as_str()));
        }
    }
    records.sort_by(|a, b| (a.1, a.2, a.0).cmp(&(b.1, b.2, b.0)));
    let mut hash = Sha256::new();
    hash.update(b"oasis7-source-input-declaration-v2");
    hash.update((records.len() as u64).to_be_bytes());
    for (kind, from, to) in records {
        put_string(&mut hash, kind);
        put_string(&mut hash, from);
        put_string(&mut hash, to);
    }
    hex::encode(hash.finalize())
}

pub(super) fn put_string(hash: &mut Sha256, value: &str) {
    hash.update((value.len() as u64).to_be_bytes());
    hash.update(value.as_bytes());
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum InputKind {
    Data,
    RustRoot,
    RustModule,
    RustInclude,
}

struct Closure<'a> {
    root: &'a Path,
    files: BTreeSet<PathBuf>,
    package_files: BTreeSet<PathBuf>,
    edges: &'a BTreeSet<Edge>,
    build_roots: &'a BTreeMap<PathBuf, PathBuf>,
    used: BTreeSet<Edge>,
    actual_build_inputs: BTreeMap<String, BTreeSet<String>>,
    visited: BTreeSet<(PathBuf, InputKind, Option<PathBuf>)>,
    active: BTreeSet<(PathBuf, InputKind, Option<PathBuf>)>,
}
impl Closure<'_> {
    fn walk(
        &mut self,
        file: &Path,
        kind: InputKind,
        build_script: Option<&Path>,
    ) -> Result<(), SourceHashError> {
        let rust = kind != InputKind::Data;
        let script = if rust {
            if kind == InputKind::RustRoot && self.build_roots.contains_key(file) {
                Some(file.to_path_buf())
            } else {
                build_script.map(Path::to_path_buf)
            }
        } else {
            None
        };
        let key = (file.to_path_buf(), kind, script.clone());
        if self.active.contains(&key) {
            return Err(unsupported("include cycle"));
        }
        if !self.visited.insert(key.clone()) {
            return Ok(());
        }
        validate_file(self.root, file)?;
        self.active.insert(key.clone());
        if rust {
            let bytes = read(file)?;
            let text = std::str::from_utf8(&bytes).map_err(unsupported)?;
            let syntax = syn::parse_file(text)
                .map_err(|e| unsupported(format!("{}: {e}", file.display())))?;
            let build = script.is_some();
            let parent = file
                .parent()
                .ok_or_else(|| unsupported("Rust file lacks parent"))?;
            let module_dir = if kind == InputKind::RustRoot
                || matches!(file.file_name().and_then(|v| v.to_str()), Some("mod.rs"))
            {
                parent.to_path_buf()
            } else {
                parent.join(
                    file.file_stem()
                        .ok_or_else(|| unsupported("Rust file lacks stem"))?,
                )
            };
            let mut scanner = Scanner {
                includes: Vec::new(),
                reads: Vec::new(),
                modules: Vec::new(),
                module_dir,
                error: None,
                build,
            };
            scanner.visit_file(&syntax);
            if let Some(error) = scanner.error {
                return Err(unsupported(error));
            }
            if kind == InputKind::RustInclude && !scanner.modules.is_empty() {
                return Err(unsupported(
                    "non-inline module inside include! is outside the supported literal-input lane",
                ));
            }
            let from = label(self.root, file)?;
            let mut actual_reads = Vec::new();
            for value in scanner.reads {
                let script_path = script
                    .as_deref()
                    .ok_or_else(|| unsupported("file read outside build context"))?;
                let package = self
                    .build_roots
                    .get(script_path)
                    .ok_or_else(|| unsupported("unknown Cargo build package context"))?;
                let origin = package.join("Cargo.toml");
                actual_reads.push(label(
                    self.root,
                    &include_path(self.root, &origin, &value)?,
                )?);
            }
            actual_reads.sort();
            actual_reads.dedup();
            if let Some(script) = &script {
                self.actual_build_inputs
                    .entry(label(self.root, script)?)
                    .or_default()
                    .extend(actual_reads.iter().cloned());
            }
            for (value, parse_rust) in scanner.includes {
                let next = include_path(self.root, file, &value)?;
                let edge = Edge {
                    from: from.clone(),
                    to: label(self.root, &next)?,
                };
                if !self.package_files.contains(&next) && !self.edges.contains(&edge) {
                    return Err(unsupported(format!(
                        "undeclared include {} -> {}",
                        edge.from, edge.to
                    )));
                }
                if self.edges.contains(&edge) {
                    self.used.insert(edge);
                }
                self.files.insert(next.clone());
                self.walk(
                    &next,
                    if parse_rust {
                        InputKind::RustInclude
                    } else {
                        InputKind::Data
                    },
                    script.as_deref(),
                )?;
            }
            for (directory, name) in scanner.modules {
                let plain = directory.join(format!("{name}.rs"));
                let nested = directory.join(&name).join("mod.rs");
                let next = match (plain.exists(), nested.exists()) {
                    (true, false) => plain,
                    (false, true) => nested,
                    _ => return Err(unsupported(format!("missing/ambiguous module {name}"))),
                };
                if !self.package_files.contains(&next) {
                    return Err(unsupported("module outside supported source roots"));
                }
                self.walk(&next, InputKind::RustModule, script.as_deref())?;
            }
            for input in actual_reads {
                self.files.insert(self.root.join(input));
            }
        }
        self.active.remove(&key);
        Ok(())
    }
}

#[path = "source_input_scan.rs"]
pub(super) mod scan;
use scan::Scanner;

#[cfg(test)]
#[path = "source_inputs_tests.rs"]
mod tests;
