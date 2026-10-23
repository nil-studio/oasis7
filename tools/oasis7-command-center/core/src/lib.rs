pub mod context;
pub mod document_directory;
mod document_directory_pairings;
mod document_directory_references;
pub mod document_inventory;
pub mod document_semantic_tree;
pub mod document_tree_reading;
pub mod domain;
mod index;
mod legacy_privacy;
pub mod process;
pub mod tools;
use domain::*;
use rusqlite::{Connection, params};
use std::path::Path;
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Invalid(String),
    #[error(transparent)]
    Context(#[from] oasis7_context::error::ContextError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Database(#[from] rusqlite::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}
pub type Result<T> = std::result::Result<T, Error>;
pub fn now() -> String {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .to_string()
}
/// The host executes this synchronous core on its blocking worker, never the WebView thread.
pub struct CommandCenter {
    db: Connection,
    current: Snapshot,
    runtime_privacy: Option<Connection>,
}
pub struct LocalRead {
    result: Result<(Project, index::IndexCandidate)>,
}
impl CommandCenter {
    pub fn use_runtime_privacy_database(&mut self, path: &Path) -> Result<()> {
        let db = Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        db.query_row("SELECT COUNT(*) FROM cc_runtime_private_inputs", [], |r| {
            r.get::<_, i64>(0)
        })?;
        self.runtime_privacy = Some(db);
        Ok(())
    }
    pub fn check_local_path(&self, path: &Path) -> Result<()> {
        context::check_project(&self.db, path)
    }
    pub fn collect_local(path: &Path) -> LocalRead {
        LocalRead {
            result: index::project(path)
                .and_then(|p| index::read_with_inventory(&p).map(|candidate| (p, candidate))),
        }
    }
    pub fn publish_local(&mut self, candidate: LocalRead, opening: bool) -> Result<Snapshot> {
        let before = self.current.clone();
        let result = (|| {
            if opening {
                let (project, _) = candidate
                    .result
                    .as_ref()
                    .map_err(|e| Error::Invalid(e.to_string()))?;
                context::register_project(&self.db, Path::new(&project.path))?;
                if self
                    .current
                    .project
                    .as_ref()
                    .is_some_and(|old| old.id != project.id)
                {
                    self.current.sections.clear();
                    self.current.sources.retain(|source| source.kind != "local");
                    self.current.git = GitSnapshot::default();
                }
                self.current.project = Some(project.clone());
                self.load_device()?;
                self.current.preferences.project_path = Some(project.path.clone());
            }
            let current = self
                .current
                .project
                .as_ref()
                .ok_or_else(|| Error::Invalid("select a project first".into()))?;
            context::check_project(&self.db, Path::new(&current.path))?;
            if let Ok((p, _)) = &candidate.result
                && (p.id != current.id || p.path != current.path)
            {
                return Err(Error::Invalid("local candidate identity changed".into()));
            }
            self.publish_local_result(candidate.result)
        })();
        if result.is_err() {
            self.current = before;
        }
        result
    }
}
impl CommandCenter {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let db = Connection::open(path)?;
        legacy_privacy::migrate(&db)?;
        context::migrate(&db)?;
        let preferences = load(&db, "device", "preferences")?.unwrap_or_default();
        let current = Snapshot {
            project: None,
            sections: Vec::new(),
            preferences,
            reading_positions: Vec::new(),
            git: GitSnapshot::default(),
            sources: Vec::new(),
        };
        let mut this = Self {
            db,
            current,
            runtime_privacy: None,
        };
        if let Some(snapshot) = load::<Snapshot>(&this.db, "cache", "device_snapshot_v1")? {
            this.current = snapshot;
            this.current.sources.retain(|source| source.kind == "local");
        }
        this.load_device()?;
        this.filter_private_cache()?;
        if let Some(project) = &this.current.project
            && index::project(Path::new(&project.path)).is_ok_and(|actual| actual.id == project.id)
            && context::check_project(&this.db, Path::new(&project.path)).is_ok()
        {
            context::register_project(&this.db, Path::new(&project.path))?;
        }
        Ok(this)
    }
    fn device_key(&self, kind: &str) -> String {
        format!(
            "{}:{kind}",
            self.current
                .project
                .as_ref()
                .map(|p| p.id.as_str())
                .unwrap_or("none")
        )
    }
    fn load_device(&mut self) -> Result<()> {
        self.current.preferences = load(&self.db, "device", &self.device_key("preferences"))?
            .or(load(&self.db, "device", "preferences")?)
            .unwrap_or_default();
        self.current.reading_positions =
            load(&self.db, "device", &self.device_key("reading"))?.unwrap_or_default();
        Ok(())
    }
    pub fn reload_device(&mut self) -> Result<()> {
        self.load_device()
    }
    pub fn current_project(&self) -> Option<Project> {
        self.current.project.clone()
    }
    pub fn snapshot(&self) -> Snapshot {
        let mut snapshot = self.current.clone();
        if snapshot
            .project
            .as_ref()
            .is_some_and(|p| context::check_project(&self.db, Path::new(&p.path)).is_err())
        {
            snapshot.sections.clear();
            snapshot.reading_positions.clear();
            snapshot.git = GitSnapshot::default();
        }
        snapshot
    }
    fn filter_private_cache(&mut self) -> Result<()> {
        if self
            .current
            .project
            .as_ref()
            .is_some_and(|p| context::check_project(&self.db, Path::new(&p.path)).is_err())
        {
            self.current.sections.clear();
            self.current.reading_positions.clear();
            self.current.git = GitSnapshot::default();
            let tx = self.db.unchecked_transaction()?;
            tx.execute("DELETE FROM search", [])?;
            tx.execute("DELETE FROM search_fts", [])?;
            save(&tx, "cache", "device_snapshot_v1", &self.current)?;
            tx.commit()?;
        }
        Ok(())
    }
    pub fn open_project(&mut self, path: impl AsRef<Path>) -> Result<Snapshot> {
        let before = self.current.clone();
        let result = self.open_project_candidate(path.as_ref());
        if result.is_err() {
            self.current = before;
        }
        result
    }
    fn open_project_candidate(&mut self, path: &Path) -> Result<Snapshot> {
        let project = index::project(path)?;
        context::register_project(&self.db, Path::new(&project.path))?;
        if self
            .current
            .project
            .as_ref()
            .is_some_and(|old| old.id != project.id)
        {
            self.current.sections.clear();
            self.current.sources.retain(|s| s.kind != "local");
            self.current.git = GitSnapshot::default();
        }
        self.current.preferences.project_path = Some(project.path.clone());
        self.current.project = Some(project);
        self.load_device()?;
        self.current.preferences.project_path =
            Some(self.current.project.as_ref().unwrap().path.clone());
        self.refresh()
    }
    pub fn refresh(&mut self) -> Result<Snapshot> {
        let before = self.current.clone();
        let result = self.refresh_candidate();
        if result.is_err() {
            self.current = before;
        }
        result
    }
    fn refresh_candidate(&mut self) -> Result<Snapshot> {
        let old = self
            .current
            .project
            .clone()
            .ok_or_else(|| Error::Invalid("select a project first".into()))?;
        context::check_project(&self.db, Path::new(&old.path))?;
        let result = index::project(Path::new(&old.path)).and_then(|p| {
            if p.id != old.id {
                return Err(Error::Invalid(
                    "repository identity changed; select project again".into(),
                ));
            }
            context::register_project(&self.db, Path::new(&p.path))?;
            index::read_with_inventory(&p).map(|candidate| (p, candidate))
        });
        self.publish_local_result(result)
    }
    fn publish_local_result(
        &mut self,
        result: Result<(Project, index::IndexCandidate)>,
    ) -> Result<Snapshot> {
        let attempt = now();
        let previous = self
            .current
            .sources
            .iter()
            .find(|s| s.kind == "local")
            .cloned();
        self.current.sources.retain(|s| s.kind != "local");
        let generation = previous
            .as_ref()
            .and_then(|s| s.generation.parse::<u64>().ok())
            .unwrap_or(0)
            + 1;
        let mut inventory = None;
        match result {
            Ok((p, candidate)) => {
                let index::IndexCandidate {
                    sections,
                    git,
                    coverage,
                    omitted,
                    inventory: mut files,
                } = candidate;
                files.source_generation = generation.to_string();
                inventory = Some(files);
                self.current.sources.push(SourceStatus {
                    kind: "local".into(),
                    state: if sections.is_empty() && !omitted {
                        "empty"
                    } else {
                        "partial"
                    }
                    .into(),
                    last_attempt: Some(attempt.clone()),
                    last_success: Some(attempt),
                    source_revision: Some(p.head.clone()),
                    generation: generation.to_string(),
                    coverage,
                    error: None,
                });
                self.current.project = Some(p);
                self.current.sections = sections;
                self.current.git = git;
            }
            Err(error) => self.current.sources.push(SourceStatus {
                kind: "local".into(),
                state: "error".into(),
                last_attempt: Some(attempt),
                last_success: previous.as_ref().and_then(|s| s.last_success.clone()),
                source_revision: previous.and_then(|s| s.source_revision),
                generation: generation.to_string(),
                coverage: "last successful facts retained".into(),
                error: Some(error.to_string()),
            }),
        }
        let tx = self.db.unchecked_transaction()?;
        tx.execute("DELETE FROM search", [])?;
        tx.execute("DELETE FROM search_fts", [])?;
        for section in &self.current.sections {
            let value = serde_json::to_string(section)?;
            tx.execute(
                "INSERT INTO search_fts(id,body) VALUES (?1,?2)",
                params![
                    section.id,
                    format!(
                        "{} {} {} {}",
                        section.id, section.path, section.title, section.content
                    )
                ],
            )?;
            tx.execute(
                "INSERT INTO search(id,body,value) VALUES (?1,?2,?3)",
                params![
                    section.id,
                    format!(
                        "{} {} {} {}",
                        section.id, section.path, section.title, section.content
                    ),
                    value
                ],
            )?;
        }
        save(&tx, "cache", "device_snapshot_v1", &self.current)?;
        if let Some(inventory) = inventory {
            save(
                &tx,
                "cache",
                &format!("document_inventory_v1:{}", inventory.project_id),
                &inventory,
            )?;
        }
        save(&tx, "device", "preferences", &self.current.preferences)?;
        save(
            &tx,
            "device",
            &self.device_key("preferences"),
            &self.current.preferences,
        )?;
        tx.commit()?;
        Ok(self.snapshot())
    }
    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<DocumentSection>> {
        let project = self
            .current
            .project
            .as_ref()
            .ok_or_else(|| Error::Invalid("select a project first".into()))?;
        context::check_project(&self.db, Path::new(&project.path))?;
        let escaped = query
            .trim()
            .replace('\\', "\\\\")
            .replace('%', "\\%")
            .replace('_', "\\_");
        let (sql, parameter) = if query.chars().count() >= 3 {
            (
                "SELECT search.value FROM search_fts JOIN search ON search.id=search_fts.id WHERE search_fts MATCH ?1 LIMIT ?2",
                format!("\"{}\"", query.trim().replace('"', "\"\"")),
            )
        } else {
            (
                "SELECT value FROM search WHERE body LIKE ?1 ESCAPE '\\' LIMIT ?2",
                format!("%{escaped}%"),
            )
        };
        let mut stmt = self.db.prepare(sql)?;
        let rows = stmt.query_map(params![parameter, limit.min(200)], |r| {
            r.get::<_, String>(0)
        })?;
        rows.map(|r| Ok(serde_json::from_str(&r?)?)).collect()
    }

    pub fn save_preferences(&mut self, preferences: Preferences) -> Result<()> {
        let tx = self.db.unchecked_transaction()?;
        save(&tx, "device", "preferences", &preferences)?;
        save(&tx, "device", &self.device_key("preferences"), &preferences)?;
        tx.commit()?;
        self.current.preferences = preferences;
        Ok(())
    }
    pub fn record_reading(&mut self, section_id: &str, revision: &str, line: usize) -> Result<()> {
        if let Some(project) = &self.current.project {
            context::check_project(&self.db, Path::new(&project.path))?;
        }
        if !self
            .current
            .sections
            .iter()
            .any(|s| s.id == section_id && s.revision == revision)
        {
            return Err(Error::Invalid(
                "reading revision does not match indexed source".into(),
            ));
        }
        let section = self
            .current
            .sections
            .iter()
            .find(|s| s.id == section_id)
            .unwrap();
        let tx = self.db.unchecked_transaction()?;
        save(
            &tx,
            "device_history",
            &format!("{}:{section_id}:{revision}", self.device_key("read")),
            section,
        )?;
        let mut positions = self.current.reading_positions.clone();
        positions.retain(|p| p.section_id != section_id);
        positions.push(ReadingPosition {
            section_id: section_id.into(),
            revision: revision.into(),
            line,
        });
        save(&tx, "device", &self.device_key("reading"), &positions)?;
        tx.commit()?;
        self.current.reading_positions = positions;
        Ok(())
    }
    pub fn read_version(
        &self,
        section_id: &str,
        revision: &str,
    ) -> Result<Option<DocumentSection>> {
        if let Some(project) = &self.current.project {
            context::check_project(&self.db, Path::new(&project.path))?;
        }
        load(
            &self.db,
            "device_history",
            &format!("{}:{section_id}:{revision}", self.device_key("read")),
        )
    }
    pub fn rebuild_cache(&mut self) -> Result<Snapshot> {
        // Refresh rebuilds derived tables atomically, retaining successful data on source failure.
        self.refresh()
    }
    pub fn document_path(&self, relative: &str) -> Result<std::path::PathBuf> {
        let project = self
            .current
            .project
            .as_ref()
            .ok_or_else(|| Error::Invalid("select a project first".into()))?;
        if !self.current.sections.iter().any(|s| s.path == relative) {
            return Err(Error::Invalid("path is not an indexed document".into()));
        }
        context::check_project(&self.db, Path::new(&project.path))?;
        index::safe_path(Path::new(&project.path), relative)
    }
}
fn load<T: serde::de::DeserializeOwned>(
    db: &Connection,
    table: &str,
    key: &str,
) -> Result<Option<T>> {
    use rusqlite::OptionalExtension;
    let value: Option<String> = db
        .query_row(
            &format!("SELECT value FROM {table} WHERE key=?1"),
            [key],
            |row| row.get(0),
        )
        .optional()?;
    value
        .map(|v| serde_json::from_str(&v).map_err(Error::from))
        .transpose()
}
fn save<T: serde::Serialize>(db: &Connection, table: &str, key: &str, value: &T) -> Result<()> {
    db.execute(&format!("INSERT INTO {table}(key,value) VALUES (?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value"), params![key, serde_json::to_string(value)?])?;
    Ok(())
}
