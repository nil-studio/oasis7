pub mod codex;
pub mod context;
pub mod daily_work;
pub mod document_directory;
mod document_directory_pairings;
mod document_directory_references;
pub mod document_inventory;
pub mod document_semantic_tree;
pub mod document_tree_reading;
pub mod domain;
pub mod github;
mod index;
pub mod process;
pub mod tools;
pub mod work_projection;
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
                self.load_personal()?;
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
        codex::migrate(&db)?;
        context::migrate(&db)?;
        let preferences = load(&db, "personal", "preferences")?.unwrap_or_default();
        let current = Snapshot {
            project: None,
            associations: Vec::new(),
            sections: Vec::new(),
            decisions: Vec::new(),
            preferences,
            reading_positions: Vec::new(),
            git: GitSnapshot::default(),
            sources: vec![disconnected("github"), disconnected("codex")],
        };
        let mut this = Self {
            db,
            current,
            runtime_privacy: None,
        };
        if let Some(snapshot) = load::<Snapshot>(&this.db, "cache", "snapshot")? {
            this.current = snapshot;
        }
        this.load_personal()?;
        this.filter_private_cache()?;
        if let Some(project) = &this.current.project
            && index::project(Path::new(&project.path)).is_ok_and(|actual| actual.id == project.id)
            && context::check_project(&this.db, Path::new(&project.path)).is_ok()
        {
            context::register_project(&this.db, Path::new(&project.path))?;
        }
        Ok(this)
    }
    fn personal_key(&self, kind: &str) -> String {
        format!(
            "{}:{kind}",
            self.current
                .project
                .as_ref()
                .map(|p| p.id.as_str())
                .unwrap_or("none")
        )
    }
    fn load_personal(&mut self) -> Result<()> {
        self.current.preferences = load(&self.db, "personal", &self.personal_key("preferences"))?
            .or(load(&self.db, "personal", "preferences")?)
            .unwrap_or_default();
        self.current.associations =
            load(&self.db, "personal", &self.personal_key("associations"))?.unwrap_or_default();
        self.current.decisions =
            load(&self.db, "personal", &self.personal_key("decisions"))?.unwrap_or_default();
        self.current.reading_positions =
            load(&self.db, "personal", &self.personal_key("reading"))?.unwrap_or_default();
        Ok(())
    }
    pub fn reload_personal(&mut self) -> Result<()> {
        self.load_personal()
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
            snapshot.decisions.clear();
            snapshot.associations.clear();
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
            save(&tx, "cache", "snapshot", &self.current)?;
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
        self.load_personal()?;
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
        save(&tx, "cache", "snapshot", &self.current)?;
        if let Some(inventory) = inventory {
            save(
                &tx,
                "cache",
                &format!("document_inventory_v1:{}", inventory.project_id),
                &inventory,
            )?;
        }
        save(&tx, "personal", "preferences", &self.current.preferences)?;
        save(
            &tx,
            "personal",
            &self.personal_key("preferences"),
            &self.current.preferences,
        )?;
        tx.commit()?;
        Ok(self.snapshot())
    }
    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<DocumentSection>> {
        if let Some(project) = &self.current.project {
            context::check_project(&self.db, Path::new(&project.path))?;
        }
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
        save(&tx, "personal", "preferences", &preferences)?;
        save(
            &tx,
            "personal",
            &self.personal_key("preferences"),
            &preferences,
        )?;
        tx.commit()?;
        self.current.preferences = preferences;
        Ok(())
    }
    pub fn decision_history(&self, id: &str) -> Result<Vec<DecisionRecord>> {
        if id.len() > 512 {
            return Err(Error::Invalid("决定身份过长".into()));
        }
        let prefix = if id.is_empty() {
            format!("{}:", self.personal_key("decision"))
        } else {
            format!("{}:{}:", self.personal_key("decision"), id)
        };
        let mut stmt = self.db.prepare(
            "SELECT value FROM history WHERE substr(key,1,length(?1))=?1 ORDER BY key LIMIT 1000",
        )?;
        let rows = stmt.query_map([prefix], |r| r.get::<_, String>(0))?;
        rows.map(|r| Ok(serde_json::from_str(&r?)?)).collect()
    }
    pub fn save_decision(&mut self, draft: DecisionDraft) -> Result<DecisionRecord> {
        self.save_decision_revision(draft, None)
    }
    pub fn replace_decision(
        &mut self,
        mut draft: DecisionDraft,
        previous_id: String,
    ) -> Result<DecisionRecord> {
        draft.id = None;
        draft.confirmed = true;
        self.save_decision_revision(draft, Some(previous_id))
    }
    fn save_decision_revision(
        &mut self,
        draft: DecisionDraft,
        previous_id: Option<String>,
    ) -> Result<DecisionRecord> {
        let tx = self.db.unchecked_transaction()?;
        let stored: Vec<DecisionRecord> =
            load(&tx, "personal", &self.personal_key("decisions"))?.unwrap_or_default();
        if serde_json::to_vec(&stored)? != serde_json::to_vec(&self.current.decisions)? {
            return Err(Error::Invalid(
                "决定已被其他窗口或旧版修改；未覆盖，请先重新读取".into(),
            ));
        }
        if self.current.project.is_none() {
            return Err(Error::Invalid("select a project first".into()));
        }
        if draft.content.trim().is_empty() || draft.content.len() > 32768 {
            return Err(Error::Invalid(
                "decision content is empty or too long".into(),
            ));
        }
        if draft.reason.len() > 32768
            || draft.scope.len() > 4096
            || (draft.confirmed
                && (draft.reason.trim().is_empty() || draft.scope.trim().is_empty()))
        {
            return Err(Error::Invalid(
                "decision reason/scope is missing or too long".into(),
            ));
        }
        if let Some(id) = &draft.id {
            if !self
                .current
                .decisions
                .iter()
                .any(|d| &d.id == id && d.state != "superseded")
            {
                return Err(Error::Invalid(
                    "decision does not belong to current project".into(),
                ));
            }
            if !draft.confirmed
                && self
                    .current
                    .decisions
                    .iter()
                    .any(|d| &d.id == id && d.state == "confirmed")
            {
                return Err(Error::Invalid(
                    "confirmed decisions cannot be changed into drafts".into(),
                ));
            }
        }
        let id = draft.id.unwrap_or_else(|| {
            index::digest(
                format!(
                    "{}:{}:{}",
                    now(),
                    self.current.decisions.len(),
                    draft.content
                )
                .as_bytes(),
            )
        });
        let record = DecisionRecord {
            supersedes: previous_id.clone().or_else(|| {
                self.current
                    .decisions
                    .iter()
                    .find(|d| d.id == id)
                    .and_then(|d| d.supersedes.clone())
            }),
            id: id.clone(),
            content: draft.content,
            reason: draft.reason,
            scope: draft.scope,
            related_section: draft.related_section,
            state: if draft.confirmed {
                "confirmed"
            } else {
                "draft"
            }
            .into(),
            confirmed_at: self
                .current
                .decisions
                .iter()
                .find(|d| d.id == id)
                .and_then(|d| d.confirmed_at.clone())
                .or_else(|| draft.confirmed.then(now)),
            formal_update: "notVerified".into(),
            implementation_sync: "notVerified".into(),
        };
        let mut decisions = self.current.decisions.clone();
        decisions.retain(|d| d.id != record.id);
        decisions.push(record.clone());
        if let Some(previous_id) = &previous_id {
            let previous = decisions
                .iter_mut()
                .find(|d| {
                    &d.id == previous_id && d.confirmed_at.is_some() && d.state != "superseded"
                })
                .ok_or_else(|| Error::Invalid("被替代决定不存在或未确认".into()))?;
            previous.state = "superseded".into();
        }
        if let Some(previous) = self.current.decisions.iter().find(|d| d.id == record.id) {
            save(
                &tx,
                "history",
                &format!(
                    "{}:{}:{}",
                    self.personal_key("decision"),
                    previous.id,
                    index::digest(&serde_json::to_vec(previous)?)
                ),
                previous,
            )?;
        }
        save(
            &tx,
            "history",
            &format!(
                "{}:{}:{}",
                self.personal_key("decision"),
                record.id,
                index::digest(&serde_json::to_vec(&record)?)
            ),
            &record,
        )?;
        daily_work::record_owned_legacy_edit(self, &tx, &decisions)?;
        save(&tx, "personal", &self.personal_key("decisions"), &decisions)?;
        if let Some(previous_id) = previous_id {
            save(
                &tx,
                "history",
                &format!(
                    "{}:{}:{}",
                    self.personal_key("decision_successor_v1"),
                    previous_id,
                    record.id
                ),
                &record.id,
            )?;
        }
        tx.commit()?;
        self.current.decisions = decisions;
        Ok(record)
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
            "history",
            &format!("{}:{section_id}:{revision}", self.personal_key("read")),
            section,
        )?;
        let mut positions = self.current.reading_positions.clone();
        positions.retain(|p| p.section_id != section_id);
        positions.push(ReadingPosition {
            section_id: section_id.into(),
            revision: revision.into(),
            line,
        });
        save(&tx, "personal", &self.personal_key("reading"), &positions)?;
        tx.commit()?;
        self.current.reading_positions = positions;
        Ok(())
    }
    pub fn load_github_cache(&self) -> Result<Option<github::GitHubSnapshot>> {
        load(&self.db, "cache", &self.personal_key("github"))
    }
    pub fn save_github_cache(&self, snapshot: &github::GitHubSnapshot) -> Result<()> {
        save(&self.db, "cache", &self.personal_key("github"), snapshot)
    }
    pub fn associate_work(
        &mut self,
        work_url: &str,
        goal_section_ids: Vec<String>,
    ) -> Result<WorkAssociation> {
        if !work_url.starts_with("https://github.com/") || goal_section_ids.len() > 100 {
            return Err(Error::Invalid(
                "expected a GitHub work URL and at most 100 goal links".into(),
            ));
        }
        if goal_section_ids
            .iter()
            .any(|id| !self.current.sections.iter().any(|s| &s.id == id))
        {
            return Err(Error::Invalid(
                "goal link does not reference indexed source".into(),
            ));
        }
        let link = WorkAssociation {
            work_url: work_url.into(),
            goal_section_ids,
            confirmed_at: now(),
        };
        let mut candidate = self.current.associations.clone();
        candidate.retain(|l| l.work_url != work_url);
        candidate.push(link.clone());
        save(
            &self.db,
            "personal",
            &self.personal_key("associations"),
            &candidate,
        )?;
        self.current.associations = candidate;
        Ok(link)
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
            "history",
            &format!("{}:{section_id}:{revision}", self.personal_key("read")),
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
fn disconnected(kind: &str) -> SourceStatus {
    SourceStatus {
        kind: kind.into(),
        state: "disconnected".into(),
        last_attempt: None,
        last_success: None,
        source_revision: None,
        generation: "0".into(),
        coverage: "not connected; no control capability".into(),
        error: None,
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
