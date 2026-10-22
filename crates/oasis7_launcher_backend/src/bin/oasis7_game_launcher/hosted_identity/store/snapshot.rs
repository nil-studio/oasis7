//! Complete, credential-free SQLite identity evidence from one read transaction.
use super::*;
use crate::hosted_identity::model::{AccountRecord, PlayerBinding};
pub use oasis7_client_api::world_service::participant_identity_snapshot::*;
impl IdentityStore {
    /// Does not initialize or migrate a database. Remote stores must implement an
    /// equivalent consistent snapshot before they can be used for migration.
    pub fn export_sqlite_snapshot(
        path: &Path,
        namespace: IdentityNamespace,
        expected_rp: RpConfig,
    ) -> Result<IdentitySnapshot, StoreError> {
        let mut connection =
            Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        connection.busy_timeout(Duration::from_millis(750))?;
        let tx = connection.transaction_with_behavior(TransactionBehavior::Deferred)?;
        let (schema, key, bytes): (i64, String, Vec<u8>) = tx.query_row(
            "SELECT schema_version,namespace,rp_config FROM identity_meta WHERE singleton=1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )?;
        let rp: RpConfig = serde_json::from_slice(&bytes).map_err(|_| StoreError::Corrupt)?;
        if schema != SCHEMA || key != namespace.key() || rp != expected_rp {
            return Err(StoreError::Rejected(
                "identity_namespace_or_rp_mismatch".into(),
            ));
        }
        let mut accounts = Vec::new();
        let mut bindings = Vec::new();
        let mut hash = blake3::Hasher::new();
        hash.update(b"oasis7.identity.complete-sqlite-watermark.v1\0");
        hash.update(&serde_json::to_vec(&(&namespace, &rp)).map_err(|_| StoreError::Corrupt)?);
        let mut count = 0usize;
        {
            let mut query = tx.prepare("SELECT namespace,record_key,kind,account_id,row_version,expires_at,payload,length(payload) FROM identity_records ORDER BY namespace,record_key")?;
            let mut rows = query.query([])?;
            while let Some(row) = rows.next()? {
                count += 1;
                if count > 1_000_000 {
                    return Err(StoreError::Rejected("snapshot_record_limit".into()));
                }
                let ns: String = row.get(0)?;
                let record_key: String = row.get(1)?;
                let kind: String = row.get(2)?;
                let account_id: Option<String> = row.get(3)?;
                let version: i64 = row.get(4)?;
                let expires: Option<i64> = row.get(5)?;
                let payload_len: i64 = row.get(7)?;
                if !(0..=16_777_216).contains(&payload_len) {
                    return Err(StoreError::Rejected("snapshot_record_byte_limit".into()));
                }
                let payload: Vec<u8> = row.get(6)?;
                if ns != key || version <= 0 {
                    return Err(StoreError::Corrupt);
                }
                let digest = blake3::hash(&payload).to_hex().to_string();
                let evidence = serde_json::to_vec(&(
                    &ns,
                    &record_key,
                    &kind,
                    &account_id,
                    version,
                    expires,
                    digest,
                ))
                .map_err(|_| StoreError::Corrupt)?;
                hash.update(&(evidence.len() as u64).to_le_bytes());
                hash.update(&evidence);
                if kind == "account" {
                    let a: AccountRecord =
                        serde_json::from_slice(&payload).map_err(|_| StoreError::Corrupt)?;
                    if account_id.as_deref() != Some(a.account_id.as_str())
                        || record_key != format!("account:{}", a.account_id)
                    {
                        return Err(StoreError::Corrupt);
                    }
                    accounts.push(SnapshotAccount {
                        account_id: a.account_id,
                        status: a.status,
                        security_epoch: a.security_epoch,
                        row_version: version as u64,
                    });
                } else if kind == "binding" {
                    let b: PlayerBinding =
                        serde_json::from_slice(&payload).map_err(|_| StoreError::Corrupt)?;
                    if account_id.as_deref() != Some(b.account_id.as_str())
                        || record_key
                            != format!("binding:{}:{}", b.account_id, b.world_identity_scope)
                    {
                        return Err(StoreError::Corrupt);
                    }
                    bindings.push(SnapshotBinding {
                        account_id: b.account_id,
                        world_identity_scope: b.world_identity_scope,
                        player_id: b.player_id,
                        binding_version: b.binding_version,
                        row_version: version as u64,
                    });
                }
            }
        }
        accounts.sort_by(|a, b| a.account_id.cmp(&b.account_id));
        bindings.sort_by(|a, b| {
            (&a.account_id, &a.world_identity_scope).cmp(&(&b.account_id, &b.world_identity_scope))
        });
        tx.commit()?;
        let mut snapshot = IdentitySnapshot {
            api_rev: 1,
            identity_namespace: SnapshotIdentityNamespace {
                identity_service_id: namespace.identity_service_id,
                environment_id: namespace.environment_id,
            },
            rp_config: SnapshotRpConfig {
                rp_id: rp.rp_id.clone(),
                allowed_origins: rp.allowed_origins.clone(),
                version: rp.version,
                identity_instance_epoch: rp.identity_instance_epoch,
            },
            source_snapshot_epoch: rp.identity_instance_epoch,
            source_watermark: format!("sqlite-content-v1:{}:{}", count, hash.finalize()),
            source_snapshot_digest: String::new(),
            complete: true,
            source_kind: "sqlite_single_read_transaction_v1".into(),
            account_count: accounts.len(),
            binding_count: bindings.len(),
            accounts,
            bindings,
        };
        snapshot.source_snapshot_digest = snapshot
            .computed_digest()
            .map_err(|_| StoreError::Corrupt)?;
        snapshot.verify().map_err(|_| StoreError::Corrupt)?;
        Ok(snapshot)
    }
}

#[cfg(test)]
mod tests {
    fn config() -> (IdentityNamespace, RpConfig) {
        (
            IdentityNamespace {
                identity_service_id: "test".into(),
                environment_id: "isolated".into(),
            },
            RpConfig {
                rp_id: "localhost".into(),
                allowed_origins: vec!["http://localhost:4173".into()],
                version: 1,
                identity_instance_epoch: 1,
            },
        )
    }
    fn path() -> std::path::PathBuf {
        std::env::temp_dir().join(format!("oasis7-snapshot-{}.sqlite", uuid::Uuid::new_v4()))
    }
    fn open(path: &Path) -> IdentityStore {
        let (ns, rp) = config();
        IdentityStore::open(path, ns, rp).unwrap()
    }
    use super::*;
    #[test]
    fn snapshot_is_full_deterministic_and_credential_free() {
        let p = path();
        let mut store = open(&p);
        store
            .transaction(|tx| {
                for i in 0..1001 {
                    let id = format!("a{i:04}");
                    let account = AccountRecord {
                        account_id: id.clone(),
                        user_handle: vec![91],
                        display_name: "PRIVATE DISPLAY".into(),
                        status: "active".into(),
                        policy_version: 1,
                        security_epoch: 2,
                        has_ever_public_key: true,
                        created_at: 1,
                        last_authenticated_at: None,
                        last_recovery_id: None,
                    };
                    tx.create(
                        &format!("account:{id}"),
                        "account",
                        Some(&id),
                        None,
                        &account,
                    )?;
                }
                tx.create(
                    "session:secret",
                    "session",
                    Some("a0000"),
                    None,
                    &"PRIVATE TOKEN",
                )?;
                tx.create(
                    "binding:a0000:world",
                    "binding",
                    Some("a0000"),
                    None,
                    &PlayerBinding {
                        account_id: "a0000".into(),
                        world_identity_scope: "world".into(),
                        player_id: "player".into(),
                        source: "PRIVATE SOURCE".into(),
                        binding_version: 1,
                    },
                )
            })
            .unwrap();
        let (ns, rp) = config();
        let snapshot = IdentityStore::export_sqlite_snapshot(&p, ns.clone(), rp.clone()).unwrap();
        assert_eq!(snapshot.account_count, 1001);
        assert_eq!(snapshot.binding_count, 1);
        assert!(
            snapshot
                .source_watermark
                .starts_with("sqlite-content-v1:1003:")
        );
        assert_eq!(
            snapshot,
            IdentityStore::export_sqlite_snapshot(&p, ns.clone(), rp.clone()).unwrap()
        );
        let json = serde_json::to_string(&snapshot).unwrap();
        assert!(!json.contains("PRIVATE"));
        assert!(!json.contains("user_handle"));
        let mut bad = snapshot.clone();
        bad.accounts[0].account_id = "tampered".into();
        assert!(bad.verify().is_err());
        store
            .transaction(|tx| {
                tx.replace(
                    "session:secret",
                    1,
                    "session",
                    Some("a0000"),
                    None,
                    &"changed",
                )
            })
            .unwrap();
        let changed = IdentityStore::export_sqlite_snapshot(&p, ns.clone(), rp.clone()).unwrap();
        assert_ne!(snapshot.source_watermark, changed.source_watermark);
        assert_ne!(
            snapshot.source_snapshot_digest,
            changed.source_snapshot_digest
        );
        let mut wrong = ns;
        wrong.environment_id = "wrong".into();
        assert!(IdentityStore::export_sqlite_snapshot(&p, wrong, rp).is_err());
    }
    #[test]
    fn malformed_or_orphan_binding_fails_without_partial_export_or_database_changes() {
        let p = path();
        let mut store = open(&p);
        store
            .transaction(|tx| {
                tx.create(
                    "binding:missing:world",
                    "binding",
                    Some("missing"),
                    None,
                    &PlayerBinding {
                        account_id: "missing".into(),
                        world_identity_scope: "world".into(),
                        player_id: "player".into(),
                        source: "legacy".into(),
                        binding_version: 1,
                    },
                )
            })
            .unwrap();
        let (ns, rp) = config();
        assert!(IdentityStore::export_sqlite_snapshot(&p, ns.clone(), rp.clone()).is_err());
        assert_eq!(
            store
                .transaction(|tx| tx.all::<PlayerBinding>("binding", None))
                .unwrap()
                .len(),
            1
        );
        store
            .transaction(|tx| tx.create("account:bad", "account", Some("bad"), None, &"invalid"))
            .unwrap();
        assert!(IdentityStore::export_sqlite_snapshot(&p, ns, rp).is_err());
    }
}
