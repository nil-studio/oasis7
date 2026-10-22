//! SQLite identity authority. Call from the launcher's blocking HTTP workers only.
//! Tablestore probes native local-transaction capability before identity writes.
use super::model::{IdentityNamespace, RpConfig};
use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior, params};
use serde::{Serialize, de::DeserializeOwned};
use std::{path::Path, time::Duration};

#[derive(Debug)]
pub enum StoreError {
    Conflict,
    Rejected(String),
    Busy,
    Unavailable(String),
    Corrupt,
    OutcomeUnknown,
}
impl std::fmt::Display for StoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Conflict => write!(f, "state_changed"),
            Self::Rejected(s) => write!(f, "{s}"),
            Self::Busy => write!(f, "identity_store_busy"),
            Self::Unavailable(s) => write!(f, "identity_store_unavailable: {s}"),
            Self::Corrupt => write!(f, "identity_store_corrupt"),
            Self::OutcomeUnknown => write!(f, "operation_pending"),
        }
    }
}
impl std::error::Error for StoreError {}
impl From<rusqlite::Error> for StoreError {
    fn from(e: rusqlite::Error) -> Self {
        match e {
            rusqlite::Error::SqliteFailure(ref code, _)
                if matches!(
                    code.code,
                    rusqlite::ErrorCode::DatabaseBusy | rusqlite::ErrorCode::DatabaseLocked
                ) =>
            {
                Self::Busy
            }
            rusqlite::Error::SqliteFailure(ref code, _)
                if code.code == rusqlite::ErrorCode::ConstraintViolation =>
            {
                Self::Conflict
            }
            _ => Self::Unavailable("database operation failed".into()),
        }
    }
}
#[derive(Clone, Debug)]
pub struct Versioned<T> {
    pub row_version: u64,
    pub value: T,
}
pub struct IdentityStore {
    connection: Option<Connection>,
    remote: Option<tablestore::TablestoreStore>,
    namespace: String,
}
pub struct IdentityTransaction<'a> {
    transaction: Option<Transaction<'a>>,
    remote: Option<tablestore::RemoteTransaction>,
    namespace: String,
}
const SCHEMA: i64 = 2;
const MAX_PAGE: usize = 1000;
fn sql_integer(value: u64) -> Result<i64, StoreError> {
    i64::try_from(value).map_err(|_| StoreError::Rejected("integer_out_of_range".into()))
}
impl IdentityStore {
    pub fn open(
        path: &Path,
        namespace: IdentityNamespace,
        rp: RpConfig,
    ) -> Result<Self, StoreError> {
        if namespace.identity_service_id.is_empty()
            || namespace.environment_id.is_empty()
            || rp.rp_id.is_empty()
            || rp.allowed_origins.is_empty()
            || rp.version == 0
            || rp.identity_instance_epoch == 0
        {
            return Err(StoreError::Rejected(
                "invalid_identity_configuration".into(),
            ));
        }
        if path.extension().is_some_and(|s| s == "json") {
            return Err(StoreError::Rejected(
                "sqlite_path_must_not_replace_legacy_json".into(),
            ));
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(false)
                .mode(0o600)
                .open(path)
                .map_err(|_| StoreError::Unavailable("database file setup failed".into()))?;
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
                .map_err(|_| StoreError::Unavailable("database permission setup failed".into()))?;
        }
        let mut connection = Connection::open(path)?;
        if rusqlite::version_number() < 3_051_003 {
            return Err(StoreError::Unavailable(
                "SQLite WAL reset fix required".into(),
            ));
        }
        let has_meta: bool = connection.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='identity_meta')", [], |row| row.get(0))?;
        if !has_meta {
            let existing_tables: i64 = connection.query_row("SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'", [], |row| row.get(0))?;
            if existing_tables != 0 {
                return Err(StoreError::Rejected(
                    "unrecognized_identity_database".into(),
                ));
            }
        }
        connection.busy_timeout(Duration::from_millis(750))?;
        connection.pragma_update(None, "journal_mode", "WAL")?;
        connection.pragma_update(None, "synchronous", "FULL")?;
        connection.execute_batch("CREATE TABLE IF NOT EXISTS identity_meta (singleton INTEGER PRIMARY KEY CHECK(singleton=1),schema_version INTEGER NOT NULL,namespace TEXT NOT NULL,rp_config BLOB NOT NULL); CREATE TABLE IF NOT EXISTS identity_records(namespace TEXT NOT NULL,record_key TEXT NOT NULL,kind TEXT NOT NULL,account_id TEXT,row_version INTEGER NOT NULL CHECK(row_version>0),expires_at INTEGER,payload BLOB NOT NULL,PRIMARY KEY(namespace,record_key)); CREATE INDEX IF NOT EXISTS identity_by_account ON identity_records(namespace,account_id,kind,record_key); CREATE INDEX IF NOT EXISTS identity_by_expiry ON identity_records(namespace,expires_at) WHERE expires_at IS NOT NULL;")?;
        let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let old: Option<(i64, String, Vec<u8>)> = tx
            .query_row(
                "SELECT schema_version,namespace,rp_config FROM identity_meta WHERE singleton=1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?;
        let key = namespace.key();
        if let Some((schema, stored, bytes)) = old {
            let stored_rp: RpConfig =
                serde_json::from_slice(&bytes).map_err(|_| StoreError::Corrupt)?;
            if schema != SCHEMA || stored != key || stored_rp != rp {
                return Err(StoreError::Rejected(
                    "identity_namespace_or_rp_mismatch".into(),
                ));
            }
        } else {
            let count: i64 =
                tx.query_row("SELECT COUNT(*) FROM identity_records", [], |r| r.get(0))?;
            if count != 0 {
                return Err(StoreError::Corrupt);
            }
            tx.execute(
                "INSERT INTO identity_meta VALUES(1,?1,?2,?3)",
                params![
                    SCHEMA,
                    key,
                    serde_json::to_vec(&rp).map_err(|_| StoreError::Corrupt)?
                ],
            )?;
        }
        tx.commit()?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
                .map_err(|_| StoreError::Unavailable("database permission setup failed".into()))?;
        }
        Ok(Self {
            connection: Some(connection),
            remote: None,
            namespace: key,
        })
    }
    /// Offline maintenance only. Callers must independently verify revocation history
    /// and fence every writer before supplying these attestations.
    pub fn advance_instance_epoch(
        path: &Path,
        namespace: &IdentityNamespace,
        new_rp: &RpConfig,
        writes_fenced: bool,
        revocation_history_verified: bool,
    ) -> Result<(), StoreError> {
        if !writes_fenced || !revocation_history_verified {
            return Err(StoreError::Rejected(
                "maintenance_fence_and_revocation_history_required".into(),
            ));
        }
        let mut connection =
            Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE)?;
        connection.busy_timeout(Duration::from_millis(750))?;
        connection.pragma_update(None, "synchronous", "FULL")?;
        let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let (schema, key, bytes): (i64, String, Vec<u8>) = tx.query_row(
            "SELECT schema_version,namespace,rp_config FROM identity_meta WHERE singleton=1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )?;
        let mut old: RpConfig = serde_json::from_slice(&bytes).map_err(|_| StoreError::Corrupt)?;
        if schema != SCHEMA
            || key != namespace.key()
            || new_rp.identity_instance_epoch <= old.identity_instance_epoch
        {
            return Err(StoreError::Rejected("invalid_epoch_advance".into()));
        }
        old.identity_instance_epoch = new_rp.identity_instance_epoch;
        if &old != new_rp {
            return Err(StoreError::Rejected(
                "epoch_advance_must_preserve_rp".into(),
            ));
        }
        tx.execute(
            "UPDATE identity_meta SET rp_config=?1 WHERE singleton=1",
            params![serde_json::to_vec(new_rp).map_err(|_| StoreError::Corrupt)?],
        )?;
        tx.commit()?;
        Ok(())
    }
    pub fn transaction<T>(
        &mut self,
        operation: impl FnOnce(&mut IdentityTransaction<'_>) -> Result<T, StoreError>,
    ) -> Result<T, StoreError> {
        let mut tx = if let Some(remote) = &self.remote {
            IdentityTransaction {
                transaction: None,
                remote: Some(remote.begin()?),
                namespace: self.namespace.clone(),
            }
        } else {
            let transaction = self
                .connection
                .as_mut()
                .ok_or(StoreError::Corrupt)?
                .transaction_with_behavior(TransactionBehavior::Immediate)?;
            IdentityTransaction {
                transaction: Some(transaction),
                remote: None,
                namespace: self.namespace.clone(),
            }
        };
        let result = operation(&mut tx)?;
        if let Some(remote) = &mut tx.remote {
            remote.commit()?;
        } else {
            tx.transaction
                .take()
                .ok_or(StoreError::Corrupt)?
                .commit()
                .map_err(|error| match StoreError::from(error) {
                    StoreError::Busy => StoreError::Busy,
                    _ => StoreError::OutcomeUnknown,
                })?;
        }
        Ok(result)
    }
    pub fn open_tablestore(
        config: tablestore::TablestoreConfig,
        namespace: IdentityNamespace,
        rp: RpConfig,
    ) -> Result<Self, StoreError> {
        let key = namespace.key();
        let remote = tablestore::TablestoreStore::open(config, namespace, rp)?;
        Ok(Self {
            connection: None,
            remote: Some(remote),
            namespace: key,
        })
    }
    pub fn cleanup_expired(&mut self, now: u64) -> Result<usize, StoreError> {
        self.transaction(|tx|{let rows=if let Some(remote)=&tx.remote{remote.expired(now,10)?}else{let mut stmt=tx.sqlite().prepare("SELECT record_key,row_version FROM identity_records WHERE namespace=?1 AND expires_at<=?2 ORDER BY expires_at,record_key LIMIT 100")?;let rows=stmt.query_map(params![tx.namespace,sql_integer(now)?],|r|Ok((r.get::<_,String>(0)?,r.get::<_,i64>(1)?)))?;rows.map(|r|{let(key,v)=r?;Ok((key,u64::try_from(v).map_err(|_|StoreError::Corrupt)?))}).collect::<Result<Vec<_>,StoreError>>()?};let count=rows.len();for(key,v)in rows{tx.delete(&key,v)?;}Ok(count)})
    }
    pub fn backup(&self, destination: &Path) -> Result<(), StoreError> {
        #[cfg(unix)]
        {
            use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
            std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(destination)
                .map_err(|_| StoreError::Unavailable("backup destination must be new".into()))?;
            std::fs::set_permissions(destination, std::fs::Permissions::from_mode(0o600))
                .map_err(|_| StoreError::Unavailable("backup permission setup failed".into()))?;
        }
        self.connection
            .as_ref()
            .ok_or_else(|| StoreError::Rejected("use_tablestore_consistent_snapshot".into()))?
            .backup("main", destination, None)?;
        Ok(())
    }
}
impl IdentityTransaction<'_> {
    fn sqlite(&self) -> &Transaction<'_> {
        self.transaction
            .as_ref()
            .expect("SQLite transaction variant")
    }

    pub fn get<T: DeserializeOwned>(&self, key: &str) -> Result<Option<Versioned<T>>, StoreError> {
        if let Some(remote) = &self.remote {
            return remote.get(key);
        }
        let row:Option<(i64,Vec<u8>)>=self.sqlite().query_row("SELECT row_version,payload FROM identity_records WHERE namespace=?1 AND record_key=?2",params![self.namespace,key],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
        row.map(|(row_version, bytes)| {
            Ok(Versioned {
                row_version: u64::try_from(row_version).map_err(|_| StoreError::Corrupt)?,
                value: serde_json::from_slice(&bytes).map_err(|_| StoreError::Corrupt)?,
            })
        })
        .transpose()
    }
    pub fn create<T: Serialize>(
        &mut self,
        key: &str,
        kind: &str,
        account_id: Option<&str>,
        expires: Option<u64>,
        payload: &T,
    ) -> Result<(), StoreError> {
        if let Some(remote) = &mut self.remote {
            return remote.create(key, kind, account_id, expires, payload);
        }
        let bytes = serde_json::to_vec(payload).map_err(|_| StoreError::Corrupt)?;
        self.sqlite().execute(
            "INSERT INTO identity_records VALUES(?1,?2,?3,?4,1,?5,?6)",
            params![
                self.namespace,
                key,
                kind,
                account_id,
                expires.map(sql_integer).transpose()?,
                bytes
            ],
        )?;
        Ok(())
    }
    pub fn replace<T: Serialize>(
        &mut self,
        key: &str,
        expected_version: u64,
        kind: &str,
        account_id: Option<&str>,
        expires: Option<u64>,
        payload: &T,
    ) -> Result<(), StoreError> {
        let bytes = serde_json::to_vec(payload).map_err(|_| StoreError::Corrupt)?;
        if let Some(remote) = &mut self.remote {
            return remote.replace(key, expected_version, kind, account_id, expires, payload);
        }
        let changed=self.sqlite().execute("UPDATE identity_records SET kind=?3,account_id=?4,row_version=row_version+1,expires_at=?5,payload=?6 WHERE namespace=?1 AND record_key=?2 AND row_version=?7",params![self.namespace,key,kind,account_id,expires.map(sql_integer).transpose()?,bytes,sql_integer(expected_version)?])?;
        if changed != 1 {
            return Err(StoreError::Conflict);
        }
        Ok(())
    }
    pub fn delete(&mut self, key: &str, expected_version: u64) -> Result<(), StoreError> {
        if let Some(remote) = &mut self.remote {
            return remote.delete(key, expected_version);
        }
        if self.sqlite().execute(
            "DELETE FROM identity_records WHERE namespace=?1 AND record_key=?2 AND row_version=?3",
            params![self.namespace, key, sql_integer(expected_version)?],
        )? != 1
        {
            return Err(StoreError::Conflict);
        }
        Ok(())
    }
    /// Security counts must inspect every page, including revoked tombstones.
    pub fn all<T: DeserializeOwned>(
        &self,
        kind: &str,
        account_id: Option<&str>,
    ) -> Result<Vec<(String, Versioned<T>)>, StoreError> {
        let mut result = Vec::new();
        let mut cursor = None;
        for _ in 0..50 {
            let page = self.list(kind, account_id, cursor.as_deref(), MAX_PAGE)?;
            let done = page.len() < MAX_PAGE;
            cursor = page.last().map(|(key, _)| key.clone());
            result.extend(page);
            if done {
                return Ok(result);
            }
        }
        Err(StoreError::Rejected("identity_record_limit".into()))
    }
    pub fn list<T: DeserializeOwned>(
        &self,
        kind: &str,
        account_id: Option<&str>,
        after: Option<&str>,
        limit: usize,
    ) -> Result<Vec<(String, Versioned<T>)>, StoreError> {
        if limit == 0 || limit > MAX_PAGE {
            return Err(StoreError::Rejected("invalid_page_size".into()));
        }
        if let Some(remote) = &self.remote {
            return remote.list(kind, account_id, after, limit);
        }
        let mut stmt=self.sqlite().prepare("SELECT record_key,row_version,payload FROM identity_records WHERE namespace=?1 AND kind=?2 AND (?3 IS NULL OR account_id=?3) AND record_key>?4 ORDER BY record_key LIMIT ?5")?;
        let rows = stmt.query_map(
            params![
                self.namespace,
                kind,
                account_id,
                after.unwrap_or(""),
                limit as i64
            ],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, Vec<u8>>(2)?,
                ))
            },
        )?;
        rows.map(|row| {
            let (key, row_version, bytes) = row?;
            Ok((
                key,
                Versioned {
                    row_version: u64::try_from(row_version).map_err(|_| StoreError::Corrupt)?,
                    value: serde_json::from_slice(&bytes).map_err(|_| StoreError::Corrupt)?,
                },
            ))
        })
        .collect()
    }
}
#[path = "store/migration.rs"]
mod migration;
#[path = "store/tablestore.rs"]
pub mod tablestore;
#[cfg(test)]
#[path = "store/tests.rs"]
mod tests;

#[path = "store/snapshot.rs"]
pub mod snapshot;
