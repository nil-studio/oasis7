//! Independently versioned device tree reading records. Never part of Snapshot.
use crate::{
    CommandCenter, Error, Result, context, document_inventory::DocumentInventoryQuery, index, now,
    save,
};
use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};
use std::path::Path;
#[path = "document_tree_reading_types.rs"]
mod types;
pub use types::*;
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Record {
    schema_version: u32,
    key: DocumentTreeReadingKey,
    state: DocumentTreeReadingState,
    updated_at: String,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LastViewRecord {
    schema_version: u32,
    project_id: String,
    baseline_mode: String,
    view: String,
}
const LIMIT: usize = 256 * 1024;
fn valid_view(view: &str) -> bool {
    matches!(view, "product" | "professional")
}
fn scope_of(key: &DocumentTreeReadingKey) -> DocumentTreeReadingScope {
    DocumentTreeReadingScope {
        schema_version: key.schema_version,
        project_id: key.project_id.clone(),
        baseline_mode: key.baseline_mode.clone(),
        baseline_oid: key.baseline_oid.clone(),
    }
}
fn storage_key(key: &DocumentTreeReadingKey) -> String {
    format!(
        "document_tree_reading_v1:{}",
        index::digest(
            &serde_json::to_vec(&(&key.project_id, &key.baseline_mode, &key.view))
                .expect("key serializes")
        )
    )
}
fn last_key(scope: &DocumentTreeReadingScope) -> String {
    format!(
        "document_tree_last_view_v1:{}",
        index::digest(
            &serde_json::to_vec(&(&scope.project_id, &scope.baseline_mode))
                .expect("scope serializes")
        )
    )
}
fn valid_path(path: &str) -> bool {
    !path.is_empty()
        && path.len() <= 4096
        && !path.chars().any(char::is_control)
        && !path.contains('\\')
        && path
            .split('/')
            .all(|p| !p.is_empty() && p != "." && p != "..")
        && !path.starts_with('/')
        && !path.contains(':')
}
pub(crate) fn validate_route(route: &[DocumentTreeRouteStep]) -> Result<()> {
    if route.len() > 64 || serde_json::to_vec(route)?.len() > LIMIT {
        return Err(Error::Invalid("tree stable route exceeds budget".into()));
    }
    for step in route {
        if step.duplicate_count == 0
            || step.duplicate_index >= step.duplicate_count
            || step.duplicate_count > 3000
            || step.path.as_deref().is_some_and(|p| !valid_path(p))
            || step.source_path.as_deref().is_some_and(|p| !valid_path(p))
            || step.destination.as_ref().is_some_and(|v| v.len() > 4096)
            || step.group.as_ref().is_some_and(|v| v.len() > 4096)
            || (step.path.is_none() && step.group.is_none() && step.destination.is_none())
        {
            return Err(Error::Invalid("invalid tree stable route".into()));
        }
    }
    Ok(())
}
fn validate_state(state: &DocumentTreeReadingState) -> Result<()> {
    if serde_json::to_vec(state)?.len() > LIMIT
        || state.expanded_routes.len() > 128
        || ![state.tree_scroll_top, state.detail_scroll_top]
            .iter()
            .all(|n| n.is_finite() && *n >= 0.0 && *n <= 1_000_000_000.0)
    {
        return Err(Error::Invalid("tree reading state exceeds budget".into()));
    }
    validate_route(&state.parent_route)?;
    for route in &state.expanded_routes {
        validate_route(route)?;
    }
    let f = &state.filters;
    if f.query.len() > 4096
        || !matches!(f.read_filter.as_str(), "all" | "read" | "omitted")
        || !matches!(f.kind_filter.as_str(), "all" | "prd" | "design" | "entry")
        || !crate::document_directory_pairings::valid(&f.pairing_filter)
        || f.lifecycle_filter.len() > 1024
        || !(matches!(
            f.lifecycle_filter.as_str(),
            "all"
                | "current"
                | "proposed"
                | "draft"
                | "active"
                | "superseded"
                | "retired"
                | "other"
                | "unknown"
        ) || f
            .lifecycle_filter
            .strip_prefix("raw:")
            .is_some_and(|s| !s.is_empty()))
    {
        return Err(Error::Invalid("invalid tree reading filters".into()));
    }
    if let Some(selection) = &state.selection {
        validate_route(&selection.route)?;
        if !valid_path(&selection.path)
            || selection.route.last().and_then(|step| step.path.as_deref())
                != Some(selection.path.as_str())
            || selection
                .section_path
                .as_deref()
                .is_some_and(|p| !valid_path(p))
            || [&selection.section_id, &selection.section_revision]
                .iter()
                .any(|v| v.as_ref().is_some_and(|v| v.len() > 4096))
            || (selection.section_id.is_some() != selection.section_revision.is_some())
            || (selection.section_id.is_some() != selection.section_path.is_some())
        {
            return Err(Error::Invalid("invalid tree reading selection".into()));
        }
    }
    Ok(())
}
fn record_status(raw: &str) -> &'static str {
    if raw.len() > LIMIT {
        return "corrupt";
    }
    match serde_json::from_str::<serde_json::Value>(raw) {
        Ok(v)
            if v.get("schemaVersion")
                .and_then(|v| v.as_u64())
                .is_some_and(|v| v != 1) =>
        {
            "unsupported"
        }
        Ok(v) if v.get("schemaVersion").and_then(|v| v.as_u64()) == Some(1) => "ready",
        _ => "corrupt",
    }
}
impl CommandCenter {
    fn check_tree_reading_scope(&self, scope: &DocumentTreeReadingScope) -> Result<()> {
        if scope.schema_version != 1
            || scope.baseline_mode != "tracked-working-tree"
            || !matches!(scope.baseline_oid.len(), 40 | 64)
            || !scope.baseline_oid.bytes().all(|b| b.is_ascii_hexdigit())
        {
            return Err(Error::Invalid(
                "unsupported tree reading schema or baseline".into(),
            ));
        }
        let project = self
            .current
            .project
            .as_ref()
            .filter(|p| p.id == scope.project_id)
            .ok_or_else(|| Error::Invalid("tree reading project changed".into()))?;
        context::check_project(&self.db, Path::new(&project.path))?;
        let inventory = self.load_document_inventory(&DocumentInventoryQuery {
            schema_version: 1,
            project_id: scope.project_id.clone(),
            snapshot_id: None,
            query: String::new(),
            read_filter: "all".into(),
            cursor: None,
        })?;
        if inventory.baseline_oid != scope.baseline_oid {
            return Err(Error::Invalid(
                "tree reading baseline changed; reload local source".into(),
            ));
        }
        Ok(())
    }
    fn check_tree_reading_key(&self, key: &DocumentTreeReadingKey) -> Result<()> {
        if !valid_view(&key.view) {
            return Err(Error::Invalid("invalid tree reading view".into()));
        }
        self.check_tree_reading_scope(&scope_of(key))
    }
    pub fn load_document_tree_reading(
        &self,
        key: DocumentTreeReadingKey,
    ) -> Result<DocumentTreeReadingLoadResult> {
        self.check_tree_reading_key(&key)?;
        let raw: Option<String> = self
            .db
            .query_row(
                "SELECT value FROM device WHERE key=?1",
                [storage_key(&key)],
                |r| r.get(0),
            )
            .optional()?;
        let mut result = DocumentTreeReadingLoadResult {
            schema_version: 1,
            key: key.clone(),
            status: "missing".into(),
            state: None,
            notice: None,
            source_baseline_oid: None,
            resolved_section_id: None,
        };
        let Some(raw) = raw else { return Ok(result) };
        result.status = record_status(&raw).into();
        if result.status != "ready" {
            result.notice = Some(format!(
                "tree reading record {}; reset this view to recover; other device records are unchanged",
                result.status
            ));
            return Ok(result);
        }
        let record: Record = match serde_json::from_str(&raw) {
            Ok(record) => record,
            Err(_) => {
                result.status = "corrupt".into();
                result.notice =
                    Some("tree reading record is corrupt; reset this view to recover".into());
                return Ok(result);
            }
        };
        if record.key.project_id != key.project_id
            || record.key.baseline_mode != key.baseline_mode
            || record.key.view != key.view
            || validate_state(&record.state).is_err()
            || record.key.schema_version != 1
            || !matches!(record.key.baseline_oid.len(), 40 | 64)
            || !record
                .key
                .baseline_oid
                .bytes()
                .all(|b| b.is_ascii_hexdigit())
        {
            result.status = "corrupt".into();
            result.notice = Some(
                "tree reading record identity or state is invalid; reset this view to recover"
                    .into(),
            );
            return Ok(result);
        }
        if record.key.baseline_oid != key.baseline_oid {
            result.notice = Some("source baseline changed; stable document routes will be reprojected without title matching".into());
        }
        if let Some(selection) = &record.state.selection
            && let Some(id) = &selection.section_id
        {
            result.resolved_section_id = self
                .current
                .sections
                .iter()
                .find(|s| {
                    Some(s.path.as_str()) == selection.section_path.as_deref()
                        && s.id == *id
                        && Some(s.revision.as_str()) == selection.section_revision.as_deref()
                })
                .map(|s| s.id.clone());
            if result.resolved_section_id.is_none() {
                result.notice = Some("saved chapter identity or revision changed; original position retained, no automatic chapter redirect".into());
            }
        }
        result.source_baseline_oid = Some(record.key.baseline_oid);
        result.state = Some(record.state);
        Ok(result)
    }
    pub fn save_document_tree_reading(
        &mut self,
        request: DocumentTreeReadingSaveRequest,
    ) -> Result<DocumentTreeReadingLoadResult> {
        self.check_tree_reading_key(&request.key)?;
        validate_state(&request.state)?;
        let key = storage_key(&request.key);
        let tx = self.db.unchecked_transaction()?;
        let existing: Option<String> = tx
            .query_row("SELECT value FROM device WHERE key=?1", [&key], |r| {
                r.get(0)
            })
            .optional()?;
        if existing
            .as_ref()
            .is_some_and(|raw| record_status(raw) == "unsupported")
        {
            return Err(Error::Invalid(
                "future tree reading version preserved; explicitly reset this view before saving"
                    .into(),
            ));
        }
        let record = Record {
            schema_version: 1,
            key: request.key.clone(),
            state: request.state,
            updated_at: now(),
        };
        if serde_json::to_vec(&record)?.len() > LIMIT {
            return Err(Error::Invalid("tree reading record exceeds budget".into()));
        }
        let scope = scope_of(&request.key);
        let pointer = LastViewRecord {
            schema_version: 1,
            project_id: scope.project_id.clone(),
            baseline_mode: scope.baseline_mode.clone(),
            view: request.key.view.clone(),
        };
        let pointer_key = last_key(&scope);
        let old_pointer: Option<String> = tx
            .query_row(
                "SELECT value FROM device WHERE key=?1",
                [&pointer_key],
                |r| r.get(0),
            )
            .optional()?;
        if old_pointer
            .as_ref()
            .is_some_and(|raw| record_status(raw) == "unsupported")
        {
            return Err(Error::Invalid(
                "future tree last-view version preserved; explicitly reset this view before saving"
                    .into(),
            ));
        }
        save(&tx, "device", &key, &record)?;
        save(&tx, "device", &pointer_key, &pointer)?;
        tx.commit()?;
        self.load_document_tree_reading(request.key)
    }
    pub fn reset_document_tree_reading(
        &mut self,
        key: DocumentTreeReadingKey,
    ) -> Result<DocumentTreeReadingLoadResult> {
        self.check_tree_reading_key(&key)?;
        let tx = self.db.unchecked_transaction()?;
        tx.execute("DELETE FROM device WHERE key=?1", [storage_key(&key)])?;
        // An explicit recovery also removes an unreadable shared pointer, never a
        // valid pointer for the other view or any unrelated device data.
        let pointer_key = last_key(&scope_of(&key));
        let pointer: Option<String> = tx
            .query_row(
                "SELECT value FROM device WHERE key=?1",
                [&pointer_key],
                |r| r.get(0),
            )
            .optional()?;
        if pointer.as_ref().is_some_and(|raw| {
            record_status(raw) != "ready" || serde_json::from_str::<LastViewRecord>(raw).is_err()
        }) {
            tx.execute("DELETE FROM device WHERE key=?1", [&pointer_key])?;
        }
        tx.commit()?;
        self.load_document_tree_reading(key)
    }
    pub fn load_document_tree_last_view(
        &self,
        scope: DocumentTreeReadingScope,
    ) -> Result<DocumentTreeLastView> {
        self.check_tree_reading_scope(&scope)?;
        let raw: Option<String> = self
            .db
            .query_row(
                "SELECT value FROM device WHERE key=?1",
                [last_key(&scope)],
                |r| r.get(0),
            )
            .optional()?;
        let Some(raw) = raw else {
            return Ok(DocumentTreeLastView {
                schema_version: 1,
                status: "missing".into(),
                view: None,
                notice: None,
            });
        };
        let mut status = record_status(&raw).to_owned();
        let record = if status == "ready" {
            serde_json::from_str::<LastViewRecord>(&raw)
                .ok()
                .filter(|r| {
                    r.schema_version == 1
                        && r.project_id == scope.project_id
                        && r.baseline_mode == scope.baseline_mode
                        && valid_view(&r.view)
                })
        } else {
            None
        };
        if status == "ready" && record.is_none() {
            status = "corrupt".into();
        }
        let notice = (status != "ready").then(|| format!("tree last-view record {status}; default view retained without overwriting the record"));
        Ok(DocumentTreeLastView {
            schema_version: 1,
            view: if status == "ready" {
                record.map(|r| r.view)
            } else {
                None
            },
            status,
            notice,
        })
    }
}
