//! Read-only privacy boundary for records written by retired app-server integration.
use crate::{Error, Result};
use rusqlite::Connection;

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
    tx.execute_batch("CREATE TABLE IF NOT EXISTS personal(key TEXT PRIMARY KEY,value TEXT NOT NULL); CREATE TABLE IF NOT EXISTS cache(key TEXT PRIMARY KEY,value TEXT NOT NULL); CREATE TABLE IF NOT EXISTS history(key TEXT PRIMARY KEY,value TEXT NOT NULL); CREATE TABLE IF NOT EXISTS search(id TEXT PRIMARY KEY,body TEXT NOT NULL,value TEXT NOT NULL); CREATE VIRTUAL TABLE IF NOT EXISTS search_fts USING fts5(id UNINDEXED,body,tokenize='trigram'); CREATE TABLE IF NOT EXISTS cc_runtime_private_inputs(scope TEXT NOT NULL,thread TEXT NOT NULL,reference TEXT NOT NULL,PRIMARY KEY(scope,thread,reference)); PRAGMA user_version=2;")?;
    tx.commit()?;
    Ok(())
}
