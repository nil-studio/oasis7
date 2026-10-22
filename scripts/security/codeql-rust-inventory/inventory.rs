//! Observation only: every extraction decision remains in main.rs unchanged.
use std::collections::HashMap;
use std::time::Instant;
use std::sync::atomic::{AtomicUsize, Ordering};
use ra_ap_hir::{Crate, Semantics};
use ra_ap_ide_db::RootDatabase;
use ra_ap_vfs::{FileId, Vfs};
use serde_json::{Value, json};

const MAP_LIMIT: usize = 16_384;
const ROW_LIMIT: usize = 4_096;
const CONTEXT_LIMIT: usize = 16;

pub struct Inventory {
    manifest: String,
    owners: HashMap<FileId, Vec<Value>>,
    includes: HashMap<FileId, Vec<Value>>,
    map_incomplete: bool,
    rows: usize,
    dropped_rows: usize,
    initial_observations: usize,
    vfs_observations: usize,
    selected_library_observations: usize,
    elapsed: std::time::Duration,
}

fn bounded_push(map: &mut HashMap<FileId, Vec<Value>>, key: FileId, value: Value) -> bool {
    if map.len() >= MAP_LIMIT && !map.contains_key(&key) { return false; }
    let entries = map.entry(key).or_default();
    if entries.len() >= CONTEXT_LIMIT { return false; }
    entries.push(value);
    true
}

impl Inventory {
    pub fn new(sema: &Semantics<'_, RootDatabase>, manifest: &str) -> Self {
        let start = Instant::now();
        let db = sema.db;
        let mut result = Self { manifest: manifest.chars().take(1024).collect(), owners: HashMap::new(), includes: HashMap::new(),
            map_incomplete: manifest.chars().count()>1024, rows: 0, dropped_rows: 0, initial_observations: 0,
            vfs_observations: 0, selected_library_observations: 0, elapsed: Default::default() };
        // These calls can force lazy RA queries. Their time is observation cost, not extraction benefit.
        for krate in Crate::all(db) {
            for module in krate.modules(db) {
                let origin = module.definition_source_file_id(db);
                let context = json!({"crate":format!("{krate:?}"), "module":format!("{module:?}"),
                    "origin_hir_file":format!("{origin:?}")});
                if let Some(file) = module.as_source_file_id(db) {
                    if !bounded_push(&mut result.owners, file.file_id(db), context) {
                        result.map_incomplete = true;
                    }
                } else {
                    // Inline/macro origins lack a direct physical source ID here. Never infer exclusion.
                    result.map_incomplete = true;
                }
            }
            for &(call, included) in ra_ap_hir_def::include_macro_invoc(db, krate.base()) {
                if !bounded_push(&mut result.includes, included.file_id(db),
                    json!({"crate":format!("{krate:?}"),"macro_call":format!("{call:?}")})) {
                    result.map_incomplete = true;
                }
            }
        }
        result.elapsed += start.elapsed();
        result
    }

    pub fn observe(&mut self, sema: &Semantics<'_, RootDatabase>, vfs: &Vfs,
        file_id: Option<FileId>, physical: &std::path::Path, phase: &str,
        already_processed: bool, is_library: Option<bool>, selected_library: bool) {
        let start = Instant::now();
        if phase == "initial" { self.initial_observations += 1; }
        else { self.vfs_observations += 1; }
        if selected_library { self.selected_library_observations += 1; }
        if self.rows >= ROW_LIMIT { self.dropped_rows += 1; self.elapsed += start.elapsed(); return; }
        let mut modules = Vec::new();
        let mut incomplete = self.map_incomplete;
        for module in file_id.into_iter().flat_map(|id| sema.file_to_module_defs(id)) {
            if modules.len() >= CONTEXT_LIMIT { incomplete = true; break; }
            modules.push(json!({"crate":format!("{:?}",module.krate(sema.db)),
                "module":format!("{module:?}"),
                "source_hir_file":format!("{:?}",module.definition_source_file_id(sema.db))}));
        }
        let owners = file_id.and_then(|id| self.owners.get(&id)).cloned().unwrap_or_default();
        let includes = file_id.and_then(|id| self.includes.get(&id)).cloned().unwrap_or_default();
        let category = if phase == "initial" { "KEEP_EXPLICIT_INPUT" }
            else if !includes.is_empty() { "KEEP_POSITIVE_INCLUDE" }
            else if !owners.is_empty() { "KEEP_PHYSICAL_MODULE" }
            else { "KEEP_UNKNOWN_NO_PHYSICAL_MODULE" };
        // VFS path is diagnostic context only, never a filter or a rewritten source coordinate.
        let vfs_path = file_id.map(|id| format!("{:?}",vfs.file_path(id))).unwrap_or_else(|| "UNKNOWN_NO_VFS_ID".into());
        let path = physical.to_string_lossy();
        let bounded_path: String = path.chars().take(1024).collect();
        incomplete |= path.chars().count() > 1024 || vfs_path.len() > 1024;
        tracing::info!(target: "codeql_inventory", "OASIS7_INVENTORY {}", json!({"process_id":std::process::id(),"kind":"file","manifest":self.manifest,
            "phase":phase,"path":bounded_path,"file_id":format!("{file_id:?}"),
            "vfs_path":vfs_path.chars().take(1024).collect::<String>(),
            "already_processed":already_processed,"is_library":is_library,
            "selected_library":selected_library,"category":category,
            "physical_module_contexts":owners,"positive_include_contexts":includes,
            "plural_module_contexts":modules,"provenance_complete":false,
            "observation_incomplete":incomplete,"extraction_action":"UNCHANGED"}));
        self.rows += 1;
        self.elapsed += start.elapsed();
    }

    pub fn finish(self) {
        tracing::info!(target: "codeql_inventory", "OASIS7_INVENTORY {}", json!({"process_id":std::process::id(),"kind":"summary","manifest":self.manifest,
            "rows":self.rows,"dropped_rows":self.dropped_rows,"map_incomplete":self.map_incomplete,
            "initial_observations":self.initial_observations,"vfs_observations":self.vfs_observations,
            "selected_library_observations":self.selected_library_observations,
            "hook_elapsed_micros":self.elapsed.as_micros(),
            "hook_elapsed_scope":"new and file observe only; excludes summary and fallback logging","extraction_action":"UNCHANGED",
            "closure_proven":false,"performance_comparison":"INVALID_INSTRUMENTED_LAZY_QUERIES"}));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn context_bound_does_not_become_an_exclusion() {
        let mut map = HashMap::new();
        let file = FileId::from_raw(1);
        for _ in 0..CONTEXT_LIMIT { assert!(bounded_push(&mut map,file,json!({"keep":true}))); }
        assert!(!bounded_push(&mut map,file,json!({"keep":true})));
        assert_eq!(map[&file].len(),CONTEXT_LIMIT);
    }
}

static FALLBACK_OBSERVED: AtomicUsize = AtomicUsize::new(0);
static FALLBACK_DROPPED: AtomicUsize = AtomicUsize::new(0);

pub fn without_semantics(path: &std::path::Path, library: bool, reason: &str) {
    if std::env::var("OASIS7_RUST_INVENTORY").as_deref() != Ok("1") { return; }
    let index = FALLBACK_OBSERVED.fetch_add(1, Ordering::Relaxed);
    if index >= ROW_LIMIT { FALLBACK_DROPPED.fetch_add(1, Ordering::Relaxed); return; }
    let text = path.to_string_lossy();
    tracing::info!(target:"codeql_inventory", "OASIS7_INVENTORY {}", json!({"process_id":std::process::id(),"kind":"without_semantics","manifest":"UNKNOWN_WITHOUT_SEMANTICS",
        "path":text.chars().take(1024).collect::<String>(), "library":library,
        "reason":reason, "category":if library && reason.is_empty() { "KEEP_BUILTIN" }
            else { "KEEP_UNKNOWN_NO_SEMANTICS" }, "extraction_action":"UNCHANGED",
        "observation_incomplete":text.chars().count()>1024,"provenance_complete":false}));
}

pub fn finish_fallback() {
    if std::env::var("OASIS7_RUST_INVENTORY").as_deref() != Ok("1") { return; }
    tracing::info!(target:"codeql_inventory", "OASIS7_INVENTORY {}", json!({"process_id":std::process::id(),"kind":"without_semantics_summary",
        "observations":FALLBACK_OBSERVED.load(Ordering::Relaxed),
        "dropped_rows":FALLBACK_DROPPED.load(Ordering::Relaxed),
        "extraction_action":"UNCHANGED","closure_proven":false}));
}
