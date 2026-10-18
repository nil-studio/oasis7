use super::dto::*;
use crate::{Error, Result};
use rusqlite::{Connection, params};
impl Store {
    pub fn reserve_start_intent(&self, scope: &str, record: &super::StartWorkRecord) -> Result<()> {
        self.db
            .execute(
                "INSERT INTO personal(key,value) VALUES (?1,?2)",
                params![
                    format!("start_intent_v1:{scope}:{}", record.intent_id),
                    serde_json::to_string(record)?
                ],
            )
            .map_err(|_| {
                Error::Invalid("该发起意图已记录或无法持久保存；请核对原操作，不重发".into())
            })?;
        Ok(())
    }
    pub fn save_start_intent(&self, scope: &str, record: &super::StartWorkRecord) -> Result<()> {
        crate::save(
            &self.db,
            "personal",
            &format!("start_intent_v1:{scope}:{}", record.intent_id),
            record,
        )
    }
    pub fn start_intents(&self, scope: &str) -> Result<Vec<super::StartWorkRecord>> {
        let mut records = self.all_start_intents(scope)?;
        // Keep unresolved originals visible even when the historical UI window fills.
        records.sort_by_key(|record| super::start_work::settled_intent(&record.state));
        records.truncate(1000);
        Ok(records)
    }
    pub fn all_start_intents(&self, scope: &str) -> Result<Vec<super::StartWorkRecord>> {
        let prefix = format!("start_intent_v1:{scope}:");
        let mut statement = self
            .db
            .prepare("SELECT value FROM personal WHERE substr(key,1,?1)=?2 ORDER BY key")?;
        // Admission must inspect every durable intent, never a UI-sized window.
        let records = statement
            .query_map(params![prefix.len(), prefix], |r| r.get::<_, String>(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        // The ordinary operation snapshot is also a recent window. Recovery must
        // use the durable ledger, including settled children older than that window.
        let mut operation_query = self
            .db
            .prepare("SELECT value,state FROM cc_runtime_operations WHERE scope=?1")?;
        let operations = operation_query
            .query_map([scope], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?
            .map(|row| {
                let (raw, state) = row?;
                let mut operation: RuntimeOperation = serde_json::from_str(&raw)?;
                operation.state = state;
                Ok(operation)
            })
            .collect::<Result<Vec<_>>>()?;
        records
            .into_iter()
            .map(|raw| {
                let mut record: super::StartWorkRecord = serde_json::from_str(&raw)?;
                if record.schema_version != 1
                    || format!("{}:{}", record.project_id, record.environment) != scope
                {
                    return Err(Error::Invalid(
                        "发起意图来自不支持版本或范围；保留原记录，不重放".into(),
                    ));
                }
                let previous = record.state.clone();
                if !super::start_work::settled_intent(&record.state) {
                    // Only original operation receipts can release admission. A reservation
                    // with no child identities is deliberately left unknown.
                    let linked: Vec<_> = record
                        .operation_ids
                        .iter()
                        .filter_map(|id| operations.iter().find(|op| &op.id == id))
                        .collect();
                    if !linked.is_empty()
                        && linked.len() == record.operation_ids.len()
                        && let Some(creation) = linked.iter().find(|op| op.method == "thread/start")
                    {
                        if creation.state == "failed" && linked.len() == 1 {
                            record.state = "inputRejected".into();
                        } else if creation.state == "confirmed"
                            && let Some(thread) = &creation.thread_id
                            && record.thread_id.as_ref().is_none_or(|id| id == thread)
                        {
                            record.thread_id = Some(thread.clone());
                            let turns: Vec<_> = operations
                                .iter()
                                .filter(|op| {
                                    op.method == "turn/start"
                                        && op.instance_id == creation.instance_id
                                        && op.thread_id.as_ref() == Some(thread)
                                })
                                .collect();
                            let resolved = |op: &&RuntimeOperation| {
                                ["confirmed", "acknowledged", "failed", "sourceResolved"]
                                    .contains(&op.state.as_str())
                            };
                            if !turns.is_empty()
                                && linked.iter().all(resolved)
                                && turns.iter().all(resolved)
                            {
                                record.state =
                                    if turns.iter().any(|op| op.state == "sourceResolved") {
                                        "sourceResolved"
                                    } else if turns.iter().any(|op| {
                                        ["confirmed", "acknowledged"].contains(&op.state.as_str())
                                    }) {
                                        "inputAcknowledged"
                                    } else {
                                        "inputRejected"
                                    }
                                    .into();
                                for turn in turns {
                                    if !record.operation_ids.contains(&turn.id) {
                                        record.operation_ids.push(turn.id.clone());
                                    }
                                }
                            } else if turns.is_empty()
                                && record.state == "threadCreatedInputNotSent"
                                && linked.len() == 1
                            {
                                record.state = "inputNotSentConfirmed".into();
                            }
                        }
                    }
                    if ["pending", "inputPending"].contains(&record.state.as_str()) {
                        record.state = "unknown".into();
                    }
                    if record.state != previous {
                        record.updated_at = crate::now();
                        self.save_start_intent(scope, &record)?;
                    }
                }
                Ok(record)
            })
            .collect()
    }
}
pub(crate) fn migrate(db: &Connection) -> Result<()> {
    let version: u32 = db.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version > 2 {
        return Err(Error::Invalid(
            "database belongs to a newer application version".into(),
        ));
    }
    db.busy_timeout(std::time::Duration::from_secs(2))?;
    db.execute_batch("PRAGMA journal_mode=WAL;")?;
    let tx = db.unchecked_transaction()?;
    tx.execute_batch("CREATE TABLE IF NOT EXISTS personal(key TEXT PRIMARY KEY,value TEXT NOT NULL); CREATE TABLE IF NOT EXISTS cache(key TEXT PRIMARY KEY,value TEXT NOT NULL); CREATE TABLE IF NOT EXISTS history(key TEXT PRIMARY KEY,value TEXT NOT NULL); CREATE TABLE IF NOT EXISTS search(id TEXT PRIMARY KEY,body TEXT NOT NULL,value TEXT NOT NULL); CREATE VIRTUAL TABLE IF NOT EXISTS search_fts USING fts5(id UNINDEXED,body,tokenize='trigram'); CREATE TABLE IF NOT EXISTS cc_runtime_state(scope TEXT PRIMARY KEY,value TEXT NOT NULL,sequence INTEGER NOT NULL DEFAULT 0); CREATE TABLE IF NOT EXISTS cc_runtime_operations(id TEXT PRIMARY KEY,scope TEXT NOT NULL,target TEXT NOT NULL,state TEXT NOT NULL,value TEXT NOT NULL); CREATE INDEX IF NOT EXISTS cc_runtime_ops_scope ON cc_runtime_operations(scope,state); CREATE TABLE IF NOT EXISTS cc_runtime_private_inputs(scope TEXT NOT NULL,thread TEXT NOT NULL,reference TEXT NOT NULL,PRIMARY KEY(scope,thread,reference)); PRAGMA user_version=2;")?;
    let has_sequence: bool = {
        let mut stmt = tx.prepare("PRAGMA table_info(cc_runtime_state)")?;
        let names = stmt
            .query_map([], |r| r.get::<_, String>(1))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        names.iter().any(|n| n == "sequence")
    };
    if !has_sequence {
        tx.execute_batch(
            "ALTER TABLE cc_runtime_state ADD COLUMN sequence INTEGER NOT NULL DEFAULT 0;",
        )?;
    }
    tx.commit()?;
    Ok(())
}
pub(crate) struct Store {
    pub db: Connection,
    pub path: std::path::PathBuf,
}
impl Store {
    pub fn open(path: &std::path::Path) -> Result<Self> {
        let db = Connection::open(path)?;
        migrate(&db)?;
        let floor: i64 = db.query_row(
            "SELECT COALESCE(MAX(sequence),0) FROM cc_runtime_state",
            [],
            |r| r.get(0),
        )?;
        SEQUENCE.fetch_max(floor, std::sync::atomic::Ordering::SeqCst);
        db.execute("UPDATE cc_runtime_operations SET state='unknown' WHERE state IN ('pending','awaiting')",[])?;
        Ok(Self {
            db,
            path: path.to_path_buf(),
        })
    }
    pub fn scopes(&self) -> Result<Vec<RuntimeSnapshot>> {
        let mut stmt = self.db.prepare("SELECT value FROM cc_runtime_state")?;
        stmt.query_map([], |r| r.get::<_, String>(0))?
            .map(|r| {
                let mut snapshot: RuntimeSnapshot = serde_json::from_str(&r?)?;
                let scope = format!("{}:{}", snapshot.project_id, snapshot.environment);
                self.apply_private_sources(&scope, &mut snapshot)?;
                Ok(snapshot)
            })
            .collect()
    }
    pub fn writer(path: &std::path::Path) -> Result<Self> {
        let db = Connection::open(path)?;
        db.busy_timeout(std::time::Duration::from_secs(2))?;
        let version: u32 = db.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if version != 2 {
            return Err(Error::Invalid(
                "event writer requires initialized schema 2".into(),
            ));
        }
        let floor: i64 = db.query_row(
            "SELECT COALESCE(MAX(sequence),0) FROM cc_runtime_state",
            [],
            |r| r.get(0),
        )?;
        SEQUENCE.fetch_max(floor, std::sync::atomic::Ordering::SeqCst);
        Ok(Self {
            db,
            path: path.into(),
        })
    }
    pub fn save(&self, scope: &str, snapshot: &RuntimeSnapshot) -> Result<()> {
        self.save_at(scope, snapshot, sequence(), false)
    }
    pub fn save_at(
        &self,
        scope: &str,
        snapshot: &RuntimeSnapshot,
        sequence: i64,
        event: bool,
    ) -> Result<()> {
        let mut saved = snapshot.clone();
        self.apply_private_sources(scope, &mut saved)?;
        saved.logs.clear();
        let changed=self.db.execute("INSERT INTO cc_runtime_state(scope,value,sequence) VALUES(?1,?2,?3) ON CONFLICT(scope) DO UPDATE SET value=excluded.value,sequence=excluded.sequence WHERE excluded.sequence>=cc_runtime_state.sequence AND (?4=0 OR json_extract(cc_runtime_state.value,'$.instanceId') IS json_extract(excluded.value,'$.instanceId'))",params![scope,serde_json::to_string(&saved)?,sequence,event])?;
        if changed == 0 && !event {
            return Err(Error::Invalid(
                "runtime snapshot persistence rejected stale sequence".into(),
            ));
        }
        Ok(())
    }
    pub fn operations(&self, scope: &str) -> Result<Vec<RuntimeOperation>> {
        let mut stmt=self.db.prepare("SELECT value,state FROM cc_runtime_operations WHERE scope=?1 AND (state IN ('pending','awaiting','unknown') OR rowid IN (SELECT rowid FROM cc_runtime_operations WHERE scope=?1 ORDER BY rowid DESC LIMIT 200)) ORDER BY rowid DESC")?;
        stmt.query_map([scope], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })?
        .map(|r| {
            let (value, state) = r?;
            let mut op: RuntimeOperation = serde_json::from_str(&value)?;
            op.state = state;
            Ok(op)
        })
        .collect()
    }
    pub fn insert(
        &self,
        scope: &str,
        target: &str,
        op: &RuntimeOperation,
        management: bool,
    ) -> Result<()> {
        let tx = self.db.unchecked_transaction()?;
        let exists:usize=tx.query_row("SELECT COUNT(*) FROM cc_runtime_operations WHERE scope=?1 AND target=?2 AND state IN ('pending','awaiting','unknown')",params![scope,target],|r|r.get(0))?;
        if exists > 0 && !management {
            return Err(Error::Invalid(
                "target has an unresolved operation; reconcile before sending again".into(),
            ));
        }
        let unresolved:usize=tx.query_row("SELECT COUNT(*) FROM cc_runtime_operations WHERE scope=?1 AND state IN ('pending','awaiting','unknown')",[scope],|r|r.get(0))?;
        if unresolved >= 128 {
            return Err(Error::Invalid("unresolved operation limit reached".into()));
        }
        tx.execute(
            "INSERT INTO cc_runtime_operations(id,scope,target,state,value) VALUES(?1,?2,?3,?4,?5)",
            params![op.id, scope, target, op.state, serde_json::to_string(op)?],
        )?;
        tx.commit()?;
        Ok(())
    }
    pub fn settle(&self, op: &RuntimeOperation) -> Result<()> {
        self.db.execute(
            "UPDATE cc_runtime_operations SET state=?1,value=?2 WHERE id=?3",
            params![op.state, serde_json::to_string(op)?, op.id],
        )?;
        Ok(())
    }
}

static SEQUENCE: std::sync::atomic::AtomicI64 = std::sync::atomic::AtomicI64::new(0);
pub(crate) fn sequence() -> i64 {
    let clock = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
        .min(i64::MAX as u128) as i64;
    SEQUENCE.fetch_max(clock, std::sync::atomic::Ordering::SeqCst);
    SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
}
