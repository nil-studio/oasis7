mod actions;
pub mod dto;
mod lifecycle;
mod privacy;
mod protocol;
mod start_work;
mod store;
pub use start_work::StartWorkRecord;
mod transport;
use crate::{Error, Result, domain::Project};
pub use dto::*;
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};
use store::Store;
pub(crate) use store::migrate;
use transport::Transport;
pub const SUPPORTED_VERSION: &str = "0.137.0";
pub struct Runtime {
    store: Store,
    contexts: HashMap<String, RuntimeSnapshot>,
    connections: HashMap<String, Transport>,
    exit_waiting: bool,
}
fn key(project: &Project) -> String {
    format!("{}:{}", project.id, project.environment)
}
fn identity() -> String {
    crate::index::digest(
        format!(
            "{}:{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        )
        .as_bytes(),
    )
}
fn empty(project: &Project) -> RuntimeSnapshot {
    RuntimeSnapshot{project_id:project.id.clone(),environment:project.environment.clone(),enabled:false,experimental:true,state:"disabled".into(),version:None,instance_id:None,capabilities:vec![],coverage:"Only explicitly created local threads; pinned stable schema, connector experimental; no global discovery/control; bounded recent session summaries; durable operation history retained; no transcript persistence".into(),error:None,sessions:vec![],requests:vec![],operations:vec![],logs:vec![],logs_truncated:false,exit_waiting:false}
}
fn older_instance_uncertainty(snapshot: &RuntimeSnapshot, operations: &[RuntimeOperation]) -> bool {
    let older = |instance: &str| Some(instance) != snapshot.instance_id.as_deref();
    let unresolved = |state: &str| ["pending", "awaiting", "unknown"].contains(&state);
    snapshot
        .sessions
        .iter()
        .any(|s| older(&s.instance_id) && (s.state == "unknown" || s.background_state != "clear"))
        || snapshot
            .requests
            .iter()
            .any(|r| older(&r.instance_id) && unresolved(&r.state))
        || operations
            .iter()
            .any(|o| older(&o.instance_id) && unresolved(&o.state))
}
impl Runtime {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let store = Store::open(path.as_ref())?;
        let mut contexts = HashMap::new();
        for mut saved in store.scopes()? {
            let scope = format!("{}:{}", saved.project_id, saved.environment);
            if let Ok(value) = store.db.query_row(
                "SELECT value FROM personal WHERE key=?1",
                [format!("codexEnabled:{scope}")],
                |r| r.get::<_, String>(0),
            ) {
                saved.enabled = value == "true";
            }
            saved.state = if saved.enabled {
                "disconnected"
            } else {
                "disabled"
            }
            .into();
            saved.capabilities.clear();
            saved.exit_waiting = false;
            for session in &mut saved.sessions {
                if !session.restorable_history {
                    session.state = "unknown".into();
                    session.background_state = "unknown".into();
                }
            }
            for request in &mut saved.requests {
                if request.state == "pending" || request.state == "awaiting" {
                    request.state = "unknown".into();
                }
            }
            contexts.insert(format!("{}:{}", saved.project_id, saved.environment), saved);
        }
        Ok(Self {
            store,
            contexts,
            connections: HashMap::new(),
            exit_waiting: false,
        })
    }
    fn context(&mut self, project: &Project) -> Result<String> {
        if project.environment != "local" {
            return Err(Error::Invalid(
                "connector only supports selected local environment".into(),
            ));
        }
        let scope = key(project);
        self.contexts
            .entry(scope.clone())
            .or_insert_with(|| empty(project));
        Ok(scope)
    }
    fn validate(project: &Project) -> Result<()> {
        let actual = crate::index::project(Path::new(&project.path))?;
        if actual.id != project.id || actual.path != project.path || project.environment != "local"
        {
            return Err(Error::Invalid(
                "selected repository/environment identity changed".into(),
            ));
        }
        crate::process::run(
            "git",
            &["cat-file", "-e", &format!("{}^{{commit}}", project.head)],
            Path::new(&project.path),
        )?;
        Ok(())
    }
    fn retire_empty_dead(&mut self, scope: &str) -> Result<bool> {
        let Some(connection) = self.connections.get_mut(scope) else {
            return Ok(false);
        };
        if connection.child.try_wait()?.is_none() {
            return Ok(false);
        }
        let mut saved = connection.snapshot();
        let instance = saved.instance_id.as_deref();
        let has_work = saved
            .sessions
            .iter()
            .any(|s| Some(s.instance_id.as_str()) == instance)
            || saved
                .requests
                .iter()
                .any(|r| Some(r.instance_id.as_str()) == instance)
            || self
                .store
                .operations(scope)?
                .iter()
                .any(|o| Some(o.instance_id.as_str()) == instance);
        if has_work || !lifecycle::survivors(&connection.baseline)?.is_empty() {
            let mut shared = connection.shared.lock().unwrap();
            shared.snapshot.state = "degraded".into();
            shared.snapshot.error=Some("Exited instance retains owned work or unverified background scope; no automatic recovery or retirement".into());
            return Ok(false);
        }
        saved.state = if saved.enabled {
            "disconnected"
        } else {
            "disabled"
        }
        .into();
        saved.capabilities.clear();
        saved.error = Some(
            "Verified empty exited instance retired; reconnect requires explicit action".into(),
        );
        self.store.save(scope, &saved)?;
        self.contexts.insert(scope.to_owned(), saved);
        self.connections.remove(scope);
        Ok(true)
    }
    pub fn snapshot(&mut self, project: &Project) -> Result<RuntimeSnapshot> {
        let scope = self.context(project)?;
        self.retire_empty_dead(&scope)?;
        self.observe(&scope);
        let (mut snapshot, observed_sequence) = self
            .connections
            .get(&scope)
            .map(Transport::observation)
            .unwrap_or_else(|| (self.contexts[&scope].clone(), store::sequence()));
        self.store.apply_private_sources(&scope, &mut snapshot)?;
        snapshot.operations = self.store.operations(&scope)?;
        for op in &mut snapshot.operations {
            if op.state == "awaiting"
                && snapshot.requests.iter().any(|r| {
                    r.state == "resolved"
                        && r.instance_id == op.instance_id
                        && Some(&r.thread_id) == op.thread_id.as_ref()
                        && op.method == format!("answerRequest:{}", r.request_id)
                })
            {
                op.state = "sourceResolved".into();
                op.error=Some("Source no longer waits for this request; command/model outcome remains separate".into());
                self.store.settle(op)?;
            }
        }
        snapshot.exit_waiting = self.exit_waiting;
        snapshot.capabilities = if self
            .connections
            .get_mut(&scope)
            .is_some_and(|c| c.child.try_wait().ok().flatten().is_none())
            && ["ready", "degraded"].contains(&snapshot.state.as_str())
        {
            let mut list = vec![
                "readThread".into(),
                "interruptTurn".into(),
                "answerRequest".into(),
                "reconcileOperation".into(),
            ];
            if snapshot.enabled
                && !self.exit_waiting
                && snapshot.state == "ready"
                && !older_instance_uncertainty(&snapshot, &snapshot.operations)
            {
                list.extend(
                    ["createSession", "continueSession", "startTurn", "steerTurn"]
                        .map(str::to_string),
                );
            }
            list
        } else {
            vec![]
        };
        self.store
            .save_at(&scope, &snapshot, observed_sequence, true)?;
        self.contexts.insert(scope, snapshot.clone());
        Ok(snapshot)
    }
    pub fn set_enabled(&mut self, project: &Project, enabled: bool) -> Result<RuntimeSnapshot> {
        let scope = self.context(project)?;
        // Persist only the preference first; never replace concurrent reader source state.
        self.store.db.execute("INSERT INTO personal(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",rusqlite::params![format!("codexEnabled:{scope}"),enabled.to_string()])?;
        if let Some(connection) = self.connections.get(&scope) {
            connection.shared.lock().unwrap().snapshot.enabled = enabled;
        }
        let context = self.contexts.get_mut(&scope).unwrap();
        context.enabled = enabled;
        if !self.connections.contains_key(&scope) {
            context.state = if enabled { "disconnected" } else { "disabled" }.into();
        }
        self.snapshot(project)
    }
    pub fn connect(&mut self, project: &Project, binary_path: &str) -> Result<RuntimeSnapshot> {
        Self::validate(project)?;
        let scope = self.context(project)?;
        if !self.contexts[&scope].enabled || self.exit_waiting {
            return Err(Error::Invalid(
                "enable the experimental connector before connecting".into(),
            ));
        }
        self.retire_empty_dead(&scope)?;
        if self.connections.contains_key(&scope) {
            return Err(Error::Invalid(
                "existing live or uncertain instance must remain managed; reconnect is not automatic".into(),
            ));
        }
        if !Path::new(binary_path).is_absolute() || !Path::new(binary_path).is_file() {
            return Err(Error::Invalid(
                "select an absolute native Codex executable path".into(),
            ));
        }
        let mut candidate = self.contexts[&scope].clone();
        candidate.instance_id = Some(identity());
        candidate.state = "connecting".into();
        candidate.error = None;
        let mut connection = Transport::spawn(
            binary_path,
            Path::new(&project.path),
            candidate,
            &self.store.path,
        )?;
        self.store.save(&scope, &connection.snapshot())?;
        let initialized=connection.request(&identity(),"initialize",json!({"clientInfo":{"name":"oasis7_command_center","title":"Oasis7 Command Center","version":"0.1.0"},"capabilities":{"experimentalApi":false}})).map_err(Error::Invalid)?;
        if !initialized["userAgent"].is_string() {
            return Err(Error::Invalid(
                "initialization response lacks version capability evidence".into(),
            ));
        }
        connection
            .send(json!({"method":"initialized"}))
            .map_err(Error::Invalid)?;
        connection.baseline = lifecycle::capture_owned(connection.child.id())?;
        {
            let mut shared = connection.shared.lock().unwrap();
            shared.snapshot.state = "ready".into();
            shared.snapshot.version = Some(SUPPORTED_VERSION.into());
        }
        self.connections.insert(scope, connection);
        self.snapshot(project)
    }
    fn gate(&mut self, project: &Project, new_work: bool) -> Result<String> {
        Self::validate(project)?;
        if new_work && crate::index::project(Path::new(&project.path))?.head != project.head {
            return Err(Error::Invalid(
                "selected source HEAD changed; refresh before new work".into(),
            ));
        }
        let scope = self.context(project)?;
        let connection = self
            .connections
            .get_mut(&scope)
            .ok_or_else(|| Error::Invalid("explicitly connect a supported runtime first".into()))?;
        if connection.child.try_wait()?.is_some() {
            return Err(Error::Invalid(
                "owned runtime exited; original targets are unknown".into(),
            ));
        }
        let snap = connection.snapshot();
        if new_work && older_instance_uncertainty(&snap, &self.store.operations(&scope)?) {
            return Err(Error::Invalid("Original instance uncertainty remains; explicit connection does not resolve prior work".into()));
        }
        if (new_work && snap.state != "ready")
            || (!new_work && !["ready", "degraded"].contains(&snap.state.as_str()))
        {
            return Err(Error::Invalid(
                "runtime observation is degraded; new sends disabled".into(),
            ));
        }
        if new_work && (!snap.enabled || self.exit_waiting) {
            return Err(Error::Invalid(
                "new work is disabled; existing instance remains managed".into(),
            ));
        }
        Ok(scope)
    }
    fn target(&self, scope: &str, target: &RuntimeTarget) -> Result<()> {
        let snap = self.connections[scope].snapshot();
        if snap.instance_id.as_deref() != Some(&target.instance_id)
            || !snap.sessions.iter().any(|s| {
                s.owned
                    && s.instance_id == target.instance_id
                    && s.thread_id == target.thread_id
                    && s.turn_id.as_deref() == Some(&target.turn_id)
            })
        {
            return Err(Error::Invalid(
                "operation target is not this owned instance/thread/turn".into(),
            ));
        }
        Ok(())
    }
    fn perform(
        &mut self,
        scope: &str,
        method: &str,
        thread: Option<&str>,
        turn: Option<&str>,
        params: Value,
        management: bool,
    ) -> Result<Option<Value>> {
        let mut op = RuntimeOperation {
            id: identity(),
            instance_id: self.connections[scope].snapshot().instance_id.unwrap(),
            thread_id: thread.map(str::to_owned),
            turn_id: turn.map(str::to_owned),
            method: method.into(),
            state: "pending".into(),
            fingerprint: crate::index::digest(&serde_json::to_vec(&params)?),
            created_at: crate::now(),
            error: None,
        };
        self.store
            .insert(scope, thread.unwrap_or("newSession"), &op, management)?;
        let response = self.connections[scope]
            .request(&op.id, method, params)
            .and_then(|v| {
                protocol::validate_reply(method, &v, thread)?;
                Ok(v)
            });
        let value = match response {
            Ok(value) => {
                op.state = "confirmed".into();
                // Only validated source receipts can attach minted identities to the ledger.
                if method == "thread/start" {
                    op.thread_id = value["thread"]["id"].as_str().map(str::to_owned);
                }
                if method == "turn/start" {
                    op.turn_id = value["turn"]["id"].as_str().map(str::to_owned);
                }
                Some(value)
            }
            Err(message) => {
                op.state = if message == "sourceConfirmedFailure" {
                    "failed"
                } else {
                    "unknown"
                }
                .into();
                op.error = Some(message);
                None
            }
        };
        if let Err(error) = self.store.settle(&op) {
            transport::degrade(
                &self.connections[scope].shared,
                "operation receipt storage failed; results unknown",
            );
            return Err(error);
        }
        if op.state == "unknown" {
            transport::degrade(
                &self.connections[scope].shared,
                "operation receipt missing; reconcile original target, do not resend",
            );
        }
        Ok(value)
    }
    fn observe(&mut self, scope: &str) {
        let Some(connection) = self.connections.get_mut(scope) else {
            return;
        };
        let current = lifecycle::capture_owned(connection.child.id());
        let operations = self.store.operations(scope);
        let mut shared = connection.shared.lock().unwrap();
        if shared.snapshot.state != "ready" {
            return;
        }
        // Before any model turn, source-idle thread creation can start long-lived MCP/service
        // helpers. Freeze their owned-process baseline before the first model input, never after.
        if let Ok(ids) = &current {
            let snapshot = &shared.snapshot;
            if snapshot.state == "ready"
                && snapshot
                    .sessions
                    .iter()
                    .filter(|s| Some(&s.instance_id) == snapshot.instance_id.as_ref())
                    .all(|s| s.turn_id.is_none() && s.state == "idle")
                && operations.as_ref().is_ok_and(|ops| {
                    ops.iter()
                        .all(|o| !["pending", "awaiting", "unknown"].contains(&o.state.as_str()))
                })
                && snapshot.requests.is_empty()
            {
                connection.baseline = ids.clone();
            }
        }
        let storage_available = operations.is_ok();
        let operations = operations.unwrap_or_default();
        let clean = storage_available
            && current.as_ref().is_ok_and(|ids| {
                ids.iter()
                    .all(|id| connection.baseline.iter().any(|old| old.same_birth(id)))
            });
        let requests = shared.snapshot.requests.clone();
        let instance = shared.snapshot.instance_id.clone();
        for session in &mut shared.snapshot.sessions {
            if Some(&session.instance_id) != instance.as_ref() {
                continue;
            }
            if ["idle", "completed", "failed", "interrupted"].contains(&session.state.as_str()) {
                let unresolved = requests.iter().any(|r| {
                    r.thread_id == session.thread_id
                        && ["pending", "awaiting", "unknown"].contains(&r.state.as_str())
                }) || operations.iter().any(|o| {
                    o.thread_id.as_deref() == Some(&session.thread_id)
                        && ["pending", "awaiting", "unknown"].contains(&o.state.as_str())
                });
                session.background_state = if clean && !unresolved {
                    "clear"
                } else {
                    "unknown"
                }
                .into();
            }
        }
    }
    pub fn quit_status(&mut self) -> QuitStatus {
        let scopes: Vec<_> = self.contexts.keys().cloned().collect();
        let mut active = 0;
        let mut unknown = 0;
        let mut pending = 0;
        for scope in scopes {
            if self.retire_empty_dead(&scope).is_err() {
                unknown += 1;
            }
            self.observe(&scope);
            let snap = self
                .connections
                .get(&scope)
                .map(Transport::snapshot)
                .unwrap_or_else(|| self.contexts[&scope].clone());
            if snap.state == "degraded" {
                unknown += 1;
            }
            for s in snap.sessions.iter().filter(|s| s.owned) {
                if ["active", "waiting"].contains(&s.state.as_str()) {
                    active += 1;
                }
                if s.state == "unknown" || s.background_state != "clear" {
                    unknown += 1;
                }
            }
            pending += snap
                .requests
                .iter()
                .filter(|r| ["pending", "awaiting", "unknown"].contains(&r.state.as_str()))
                .count();
            unknown += self
                .store
                .operations(&scope)
                .map(|ops| {
                    ops.iter()
                        .filter(|o| ["pending", "awaiting", "unknown"].contains(&o.state.as_str()))
                        .count()
                })
                .unwrap_or(1);
        }
        QuitStatus{safe:active==0&&unknown==0&&pending==0,active,unknown,pending_requests:pending,waiting:self.exit_waiting,reason:if active==0&&unknown==0&&pending==0{"No active/unknown owned work; process-tree scope verified"}else{"Owned active or unknown execution remains; closing window keeps background, no automatic stop"}.into()}
    }
    pub fn begin_wait_for_exit(&mut self) -> QuitStatus {
        self.exit_waiting = true;
        self.quit_status()
    }
    pub fn cancel_exit_wait(&mut self) -> QuitStatus {
        self.exit_waiting = false;
        self.quit_status()
    }
    pub fn shutdown_if_idle(&mut self) -> Result<QuitStatus> {
        let status = self.quit_status();
        if status.safe {
            self.exit_waiting = true;
            let closed: Vec<_> = self
                .connections
                .iter()
                .map(|(scope, c)| (scope.clone(), c.snapshot()))
                .collect();
            for connection in self.connections.values_mut() {
                connection.shutdown()?;
            }
            for (scope, mut saved) in closed {
                for session in &mut saved.sessions {
                    session.restorable_history = session.owned
                        && ["idle", "completed", "failed", "interrupted"]
                            .contains(&session.state.as_str())
                        && session.background_state == "clear";
                }
                saved.state = "disconnected".into();
                saved.capabilities.clear();
                self.store.save(&scope, &saved)?;
                self.contexts.insert(scope.clone(), saved);
            }
            self.connections.clear();
        }
        Ok(status)
    }
    pub fn stop_for_exit(&mut self) -> Result<QuitStatus> {
        self.exit_waiting = true;
        let targets: Vec<_> = self
            .connections
            .iter()
            .flat_map(|(scope, c)| {
                let snap = c.snapshot();
                snap.sessions
                    .into_iter()
                    .filter(|s| {
                        s.owned && ["active", "waiting", "unknown"].contains(&s.state.as_str())
                    })
                    .filter_map(|s| {
                        s.turn_id.map(|turn| {
                            (
                                scope.clone(),
                                RuntimeTarget {
                                    instance_id: s.instance_id,
                                    thread_id: s.thread_id,
                                    turn_id: turn,
                                },
                            )
                        })
                    })
                    .collect::<Vec<_>>()
            })
            .collect();
        for (scope, target) in targets {
            self.target(&scope, &target)?;
            self.perform(
                &scope,
                "turn/interrupt",
                Some(&target.thread_id),
                Some(&target.turn_id),
                json!({"threadId":target.thread_id,"turnId":target.turn_id}),
                true,
            )?;
        }
        self.shutdown_if_idle()
    }
}
