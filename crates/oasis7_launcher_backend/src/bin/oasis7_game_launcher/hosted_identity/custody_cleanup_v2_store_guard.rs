//! Store integration for ordinary whole-SQLite backup rollback detection.
//! The separately configured journal is outside the database restore unit;
//! Account's authenticated independent floor remains mandatory for machine use.
use super::*;
use oasis7_custody_core::native_cleanup_v2::NativeStorageHeadV2;
use std::path::{Path, PathBuf};
#[path = "custody_cleanup_v2_restore_guard.rs"]
mod journal;
const KEY: &str = "custody_native_machine_head_v2";
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Marker {
    api_version: u8,
    namespace: String,
    head: NativeStorageHeadV2,
}
pub(super) struct Binding {
    pub(super) path: PathBuf,
    guard: Option<journal::RestoreGuard>,
}
pub(super) struct Snapshot {
    before: Option<NativeStorageHeadV2>,
    changes: u64,
    pub(super) next: Option<NativeStorageHeadV2>,
}
fn unavailable() -> StoreError {
    StoreError::Rejected("native_machine_restore_guard_unavailable".into())
}
fn load(tx: &Transaction<'_>, namespace: &str) -> Result<Option<NativeStorageHeadV2>, StoreError> {
    let value: Option<Vec<u8>> = tx
        .query_row(
            "SELECT payload FROM identity_records WHERE namespace=?1 AND record_key=?2 AND kind=?2",
            params![namespace, KEY],
            |r| r.get(0),
        )
        .optional()?;
    let Some(bytes) = value else {
        return Ok(None);
    };
    let marker: Marker = serde_json::from_slice(&bytes).map_err(|_| StoreError::Corrupt)?;
    marker.head.validate().map_err(|_| StoreError::Corrupt)?;
    if marker.api_version != 2 || marker.namespace != namespace {
        return Err(StoreError::Corrupt);
    }
    Ok(Some(marker.head))
}
fn fresh_head(old: Option<&NativeStorageHeadV2>) -> Result<NativeStorageHeadV2, StoreError> {
    let mut random = [0u8; 32];
    getrandom::fill(&mut random).map_err(|_| unavailable())?;
    let commitment = hex::encode(random);
    let (instance_id, revision) = if let Some(old) = old {
        (
            old.instance_id.clone(),
            old.revision.0.checked_add(1).ok_or_else(unavailable)?,
        )
    } else {
        getrandom::fill(&mut random).map_err(|_| unavailable())?;
        (hex::encode(random), 0)
    };
    Ok(NativeStorageHeadV2 {
        instance_id,
        revision: oasis7_custody_core::DecimalU64(revision),
        commitment,
    })
}
fn store(
    tx: &Transaction<'_>,
    namespace: &str,
    head: &NativeStorageHeadV2,
    initial: bool,
) -> Result<(), StoreError> {
    let value = serde_json::to_vec(&Marker {
        api_version: 2,
        namespace: namespace.into(),
        head: head.clone(),
    })
    .map_err(|_| StoreError::Corrupt)?;
    let changed = if initial {
        tx.execute(
            "INSERT INTO identity_records VALUES(?1,?2,?2,NULL,1,NULL,?3)",
            params![namespace, KEY, value],
        )?
    } else {
        tx.execute("UPDATE identity_records SET payload=?3,row_version=row_version+1 WHERE namespace=?1 AND record_key=?2 AND kind=?2 AND typeof(row_version)='integer' AND row_version>=1 AND row_version<9223372036854775807", params![namespace, KEY, value])?
    };
    if changed != 1 {
        return Err(StoreError::Corrupt);
    }
    Ok(())
}
pub(super) fn before(
    tx: &Transaction<'_>,
    namespace: &str,
    binding: Option<&Binding>,
) -> Result<Snapshot, StoreError> {
    let current = load(tx, namespace)?;
    match (&current, binding.and_then(|b| b.guard.as_ref())) {
        (Some(head), Some(guard)) => guard.reconcile(head).map_err(|_| unavailable())?,
        (None, None) => {}
        _ => return Err(unavailable()),
    }
    let next = current.as_ref().map(|h| fresh_head(Some(h))).transpose()?;
    Ok(Snapshot {
        before: current,
        changes: tx.total_changes(),
        next,
    })
}
pub(super) fn prepare(
    tx: &Transaction<'_>,
    namespace: &str,
    binding: Option<&Binding>,
    snapshot: &Snapshot,
) -> Result<Option<NativeStorageHeadV2>, StoreError> {
    // Callers may not rewrite/delete the marker through generic row operations.
    if load(tx, namespace)? != snapshot.before {
        return Err(StoreError::Corrupt);
    }
    if tx.total_changes() == snapshot.changes {
        return Ok(None);
    }
    let Some(previous) = &snapshot.before else {
        return Ok(None);
    };
    let next = snapshot.next.as_ref().ok_or(StoreError::Corrupt)?;
    let guard = binding
        .and_then(|b| b.guard.as_ref())
        .ok_or_else(unavailable)?;
    guard.prepare(previous, next).map_err(|_| unavailable())?;
    store(tx, namespace, next, false)?;
    Ok(Some(next.clone()))
}
pub(super) fn committed(
    binding: Option<&Binding>,
    head: &NativeStorageHeadV2,
) -> Result<(), StoreError> {
    binding
        .and_then(|b| b.guard.as_ref())
        .ok_or_else(unavailable)?
        .reconcile(head)
        .map_err(|_| StoreError::OutcomeUnknown)
}
pub(super) fn reject_offline_rotation_if_registered(
    tx: &Transaction<'_>,
    namespace: &str,
) -> Result<(), StoreError> {
    if load(tx, namespace)?.is_some() {
        return Err(StoreError::Rejected(
            "native_machine_epoch_rotation_requires_explicit_re_registration".into(),
        ));
    }
    Ok(())
}
impl IdentityStore {
    /// An unsupported remote backend must not perform a Row get before this
    /// new profile's local guard qualification. This does not initialize it.
    pub(in super::super) fn require_guarded_machine_storage(&self) -> Result<(), StoreError> {
        if self.remote.is_some()
            || self
                .machine_guard
                .as_ref()
                .is_none_or(|b| b.guard.is_none())
        {
            return Err(unavailable());
        }
        Ok(())
    }

    /// Only new guarded consent options choose existing guard participation
    /// or first initialization, before UV. Registration never initializes a
    /// missing lineage and legacy records never acquire a producer tag.
    pub(in super::super) fn guarded_consent_transaction<T>(
        &mut self,
        operation: impl FnOnce(&mut IdentityTransaction<'_>) -> Result<T, StoreError>,
    ) -> Result<T, StoreError> {
        if self
            .machine_guard
            .as_ref()
            .is_some_and(|b| b.guard.is_some())
        {
            self.transaction(operation)
        } else {
            self.initialize_machine_restore_guard_with(|tx, _| operation(tx))
        }
    }
    /// Existing registrations require their independently configured guard at startup.
    /// Missing guard files are never recreated merely by restoring a database.
    pub(in super::super) fn attach_machine_restore_guard_from_env(
        &mut self,
        database: &Path,
    ) -> Result<(), StoreError> {
        let configured = std::env::var_os("OASIS7_CUSTODY_CLEANUP_SQLITE_RESTORE_GUARD_PATH");
        if self.remote.is_some() {
            if configured.is_some() {
                return Err(unavailable());
            }
            return Ok(());
        }
        let connection = self.connection.as_mut().ok_or(StoreError::Corrupt)?;
        let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let head = load(&tx, &self.namespace)?;
        let Some(path) = configured.map(PathBuf::from) else {
            if head.is_some() {
                return Err(unavailable());
            }
            return Ok(());
        };
        if !path.is_absolute()
            || path.parent().is_none()
            || database.parent().is_none()
            || path.parent().and_then(|p| p.canonicalize().ok())
                == database.parent().and_then(|p| p.canonicalize().ok())
        {
            return Err(unavailable());
        }
        let guard = if path.exists() {
            let guard = journal::RestoreGuard::open(&path).map_err(|_| unavailable())?;
            guard
                .reconcile(head.as_ref().ok_or_else(unavailable)?)
                .map_err(|_| unavailable())?;
            Some(guard)
        } else {
            if head.is_some() {
                return Err(unavailable());
            }
            None
        };
        tx.commit()?;
        self.machine_guard = Some(Binding { path, guard });
        Ok(())
    }
    /// Only new guarded-consent options call this before storing their original
    /// options record. No readonly, legacy approval or registration operation
    /// invokes first initialization.
    pub(in super::super) fn initialize_machine_restore_guard_with<T>(
        &mut self,
        operation: impl FnOnce(
            &mut IdentityTransaction<'_>,
            &NativeStorageHeadV2,
        ) -> Result<T, StoreError>,
    ) -> Result<T, StoreError> {
        if self.remote.is_some() {
            return Err(unavailable());
        }
        let binding = self.machine_guard.as_mut().ok_or_else(unavailable)?;
        if binding.guard.is_some() || binding.path.exists() {
            return Err(unavailable());
        }
        let transaction = self
            .connection
            .as_mut()
            .ok_or(StoreError::Corrupt)?
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        if load(&transaction, &self.namespace)?.is_some() {
            return Err(unavailable());
        }
        let head = fresh_head(None)?;
        let mut tx = IdentityTransaction {
            transaction: Some(transaction),
            remote: None,
            namespace: self.namespace.clone(),
            source_revocations: self.source_revocations.clone(),
            machine_next_head: Some(head.clone()),
        };
        let result = operation(&mut tx, &head)?;
        let transaction = tx.transaction.take().ok_or(StoreError::Corrupt)?;
        if load(&transaction, &self.namespace)?.is_some() {
            return Err(StoreError::Corrupt);
        }
        store(&transaction, &self.namespace, &head, true)?;
        let guard =
            journal::RestoreGuard::create(&binding.path, &head).map_err(|_| unavailable())?;
        // Keep the external journal even if commit reports unknown. No cleanup,
        // rollback to None or ordinary reopen can silently recreate this state.
        binding.guard = Some(guard);
        transaction
            .commit()
            .map_err(|_| StoreError::OutcomeUnknown)?;
        Ok(result)
    }
}
