//! Exact unfixed public Submit originals, owned by the same admission gate as
//! Node Frozen preparation. Node's outer queue is insufficient: the worker may
//! already have moved an admitted action into its private proposal.
use super::*;
use oasis7_distfs::controlled_authority::{
    LocalRequestIdentity,
    formal::{RecoveryPendingHandoffV1, RecoveryPendingRequestV1},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::Write,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

const MAX_EXPORT_BYTES: u64 = 64 * 1024 * 1024;
static EXPORT_TEMP: AtomicU64 = AtomicU64::new(0);

#[derive(Default)]
struct PendingState {
    originals: BTreeMap<LocalRequestIdentity, Vec<u8>>,
    frozen: Option<RecoveryPendingHandoffV1>,
    halted: Option<String>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PendingExportV1 {
    schema_version: u32,
    authority_id: String,
    pending: RecoveryPendingHandoffV1,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PendingFreezeMarkerV1 {
    schema_version: u32,
    authority_id: String,
    root: String,
    count: usize,
}

/// The queue is still volatile admission. Only a separately verified recovery
/// plan and protected endpoint publication make its exported originals durable.
pub(crate) struct FormalPendingHandoffQueue {
    authority_id: String,
    export_path: PathBuf,
    state: Mutex<PendingState>,
}

impl FormalPendingHandoffQueue {
    pub(crate) fn open(writer_directory: &Path, authority_id: &str) -> Result<Self, String> {
        crate::controlled_live_config::require_private_owned(writer_directory, true)?;
        let export_path = writer_directory.join("pending-handoff.cbor");
        let frozen = read_export(&export_path, authority_id)?;
        let originals = frozen
            .as_ref()
            .map(|pending| {
                pending
                    .requests
                    .iter()
                    .map(|entry| (entry.identity.clone(), entry.signed_request.clone()))
                    .collect()
            })
            .unwrap_or_default();
        Ok(Self {
            authority_id: authority_id.into(),
            export_path,
            state: Mutex::new(PendingState {
                originals,
                frozen,
                halted: None,
            }),
        })
    }

    /// Caller holds the shared admission gate. Validate the entire prospective
    /// collection before enqueue, then retain precisely the successful bytes.
    pub(crate) fn admit(
        &self,
        identity: LocalRequestIdentity,
        original: Vec<u8>,
        enqueue: impl FnOnce() -> Result<(), String>,
    ) -> Result<(), String> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| "pending handoff lock poisoned")?;
        require_open(&state)?;
        if let Some(existing) = state.originals.get(&identity) {
            return if existing == &original {
                // This exact action is already queued or in Node's proposal.
                Ok(())
            } else {
                Err("pending Submit identity already binds different original bytes".into())
            };
        }
        let mut prospective = entries(&state);
        prospective.push(RecoveryPendingRequestV1 {
            identity: identity.clone(),
            signed_request: original.clone(),
        });
        if let Err(error) = RecoveryPendingHandoffV1::from_requests(prospective) {
            let reason = format!("pending handoff capacity requires recovery: {error}");
            state.halted = Some(reason.clone());
            return Err(reason);
        }
        enqueue()?;
        state.originals.insert(identity, original);
        Ok(())
    }

    pub(crate) fn ensure_new_frozen_allowed(&self) -> Result<(), String> {
        let state = self
            .state
            .lock()
            .map_err(|_| "pending handoff lock poisoned")?;
        require_open(&state)
    }

    pub(crate) fn is_frozen(&self) -> Result<bool, String> {
        Ok(self
            .state
            .lock()
            .map_err(|_| "pending handoff lock poisoned")?
            .frozen
            .is_some())
    }

    pub(super) fn halt_incomplete_import(&self) -> Result<(), String> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| "pending handoff lock poisoned")?;
        state.halted = Some("original recovery pending import remains unresolved".into());
        Ok(())
    }

    /// Called only after the actual writer has successfully fixed this exact
    /// continuation. A failed/unknown prepare keeps every original recoverable.
    pub(crate) fn note_frozen(
        &self,
        continuation: &oasis7_node::NodeLocalExecutionContinuation,
    ) -> Result<(), String> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| "pending handoff lock poisoned")?;
        state.originals.retain(|_, bytes| {
            !continuation
                .context
                .committed_actions
                .iter()
                .any(|action| &action.payload_cbor == bytes)
        });
        Ok(())
    }

    /// Freeze is irreversible for this owner, including after reopening it.
    /// On I/O failure, the complete in-memory original set remains frozen and a
    /// retry publishes the same bytes; no truncated collection is returned.
    fn freeze(&self) -> Result<RecoveryPendingHandoffV1, String> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| "pending handoff lock poisoned")?;
        if state.frozen.is_none() {
            state.frozen = Some(
                RecoveryPendingHandoffV1::from_requests(entries(&state))
                    .map_err(|error| format!("freeze complete pending handoff: {error}"))?,
            );
        }
        let pending = state.frozen.as_ref().expect("frozen above");
        publish_export(&self.export_path, &self.authority_id, pending)?;
        Ok(pending.clone())
    }

    pub(crate) fn export_path(&self) -> &Path {
        &self.export_path
    }
}

fn require_open(state: &PendingState) -> Result<(), String> {
    if state.frozen.is_some() {
        Err("formal pending handoff is frozen; new Submit/Frozen is refused".into())
    } else if let Some(reason) = &state.halted {
        Err(reason.clone())
    } else {
        Ok(())
    }
}

fn entries(state: &PendingState) -> Vec<RecoveryPendingRequestV1> {
    state
        .originals
        .iter()
        .map(|(identity, original)| RecoveryPendingRequestV1 {
            identity: identity.clone(),
            signed_request: original.clone(),
        })
        .collect()
}

pub(crate) fn request_identity(
    request: &oasis7_client_api::world_service::SubmitIntentRequest<
        oasis7::world_service::wire::WorldServicePayloadV1,
    >,
) -> LocalRequestIdentity {
    let key = &request.correlation.key;
    LocalRequestIdentity {
        verified_subject: key.verified_subject.clone(),
        operation_domain: key.operation_domain.clone(),
        nonce_scope: key.nonce_scope.clone(),
        request_id: key.request_id_or_nonce.clone(),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PendingImportObservation {
    ReceivedVolatile,
    AlreadyCanonical,
    Rejected { status: u16, code: &'static str },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PendingImportReport {
    pub(crate) root: String,
    pub(crate) observations: Vec<(LocalRequestIdentity, PendingImportObservation)>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PendingImportSummary {
    pub(crate) root: String,
    pub(crate) received_volatile: usize,
    pub(crate) already_canonical: usize,
    pub(crate) rejected: usize,
}

impl FormalLiveRuntime {
    pub(crate) fn freeze_pending_handoff(
        &self,
        timeout: Duration,
    ) -> Result<RecoveryPendingHandoffV1, String> {
        if timeout.is_zero() || timeout > Duration::from_secs(60) {
            return Err("pending handoff freeze timeout in (0,60s]".into());
        }
        let deadline = std::time::Instant::now() + timeout;
        let _admission = quiesce::acquire_for_stop(&self.admission_gate, deadline)?.ok_or(
            "OutcomeUnknown: pending handoff freeze did not acquire admission before deadline",
        )?;
        let pending = self.pending_handoff.freeze()?;
        if std::time::Instant::now() >= deadline {
            return Err(
                "OutcomeUnknown: original pending handoff may be frozen; retry its exact owner"
                    .into(),
            );
        }
        Ok(pending)
    }

    pub(crate) fn pending_handoff_export_path(&self) -> &Path {
        self.pending_handoff.export_path()
    }

    #[cfg(test)]
    pub(crate) fn pending_import_report(&self) -> Option<&PendingImportReport> {
        self.pending_import_report.as_ref()
    }

    pub(crate) fn pending_import_summary(&self) -> Option<PendingImportSummary> {
        self.pending_import_report
            .as_ref()
            .map(|report| PendingImportSummary {
                root: report.root.clone(),
                received_volatile: report
                    .observations
                    .iter()
                    .filter(|(_, value)| {
                        matches!(value, PendingImportObservation::ReceivedVolatile)
                    })
                    .count(),
                already_canonical: report
                    .observations
                    .iter()
                    .filter(|(_, value)| {
                        matches!(value, PendingImportObservation::AlreadyCanonical)
                    })
                    .count(),
                rejected: report
                    .observations
                    .iter()
                    .filter(|(_, value)| matches!(value, PendingImportObservation::Rejected { .. }))
                    .count(),
            })
    }

    pub(super) fn import_recovery_pending(
        &mut self,
        service: &FormalLiveService,
        admission: &std::sync::MutexGuard<'_, ()>,
    ) -> Result<(), String> {
        let Some(recovery) = &self.validated.recovery else {
            return Ok(());
        };
        let Some(plan) = recovery.recovery().recovery_plan() else {
            if recovery.recovery().body().schema_version >= 3 {
                return Err("recovery pending handoff requires its verified original plan".into());
            }
            // Explicit legacy recovery retains its original compatibility path.
            return Ok(());
        };
        let expected = &plan.body.pending_handoff_root;
        let deadline = std::time::Instant::now() + self.validated.timeout;
        let mut copies = Vec::with_capacity(2);
        for (address, role) in [
            (self.validated.config.primary, EndpointRole::Primary),
            (self.validated.config.replica, EndpointRole::Replica),
        ] {
            let mut endpoint = RemoteFormalEndpoint::new(
                address,
                self.validated.timeout,
                self.active_authority.clone(),
                role,
                self.validated.writer_key.clone(),
                self.published.current()?.anchor.head().head().clone(),
            )
            .map_err(|error| format!("pending handoff endpoint: {error}"))?;
            endpoint.set_operation_deadline(deadline);
            let pending = endpoint
                .recovery_pending_handoff()
                .map_err(|error| format!("read original pending handoff: {error}"))?;
            if &pending.root().map_err(|error| error.to_string())? != expected {
                return Err("pending handoff differs from verified recovery plan root".into());
            }
            copies.push(pending);
        }
        if copies[0] != copies[1] {
            return Err("protected endpoints disagree on original pending handoff".into());
        }
        let mut report = PendingImportReport {
            root: expected.clone(),
            observations: Vec::with_capacity(copies[0].requests.len()),
        };
        for original in &copies[0].requests {
            if std::time::Instant::now() >= deadline {
                return Err(
                    "recovery pending import exceeded its original startup deadline".into(),
                );
            }
            let observation = service.resume_pending_handoff_locked(original, admission)?;
            report
                .observations
                .push((original.identity.clone(), observation));
        }
        if std::time::Instant::now() >= deadline {
            return Err("recovery pending import exceeded its original startup deadline".into());
        }
        self.pending_import_report = Some(report);
        Ok(())
    }
}

fn encode_export(
    authority_id: &str,
    pending: &RecoveryPendingHandoffV1,
) -> Result<Vec<u8>, String> {
    pending.validate().map_err(|error| error.to_string())?;
    let export = PendingExportV1 {
        schema_version: 1,
        authority_id: authority_id.into(),
        pending: pending.clone(),
    };
    let bytes = serde_cbor::to_vec(&export).map_err(|error| error.to_string())?;
    if bytes.len() as u64 > MAX_EXPORT_BYTES {
        return Err("pending handoff export byte bound".into());
    }
    Ok(bytes)
}

pub(super) fn read_export(
    path: &Path,
    authority_id: &str,
) -> Result<Option<RecoveryPendingHandoffV1>, String> {
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return match fs::symlink_metadata(marker_path(path)) {
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
                Err(error) => Err(error.to_string()),
                Ok(_) => {
                    Err("frozen owner is missing its complete original pending handoff".into())
                }
            };
        }
        Err(error) => return Err(error.to_string()),
        Ok(_) => {}
    }
    let bytes = crate::world_initialization::read_private_owned_bounded(path, MAX_EXPORT_BYTES)?;
    let export: PendingExportV1 =
        serde_cbor::from_slice(&bytes).map_err(|error| error.to_string())?;
    if export.schema_version != 1
        || export.authority_id != authority_id
        || encode_export(authority_id, &export.pending)? != bytes
    {
        return Err("pending handoff export authority/schema/canonical bytes mismatch".into());
    }
    let marker = marker_path(path);
    match fs::symlink_metadata(&marker) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            // Crash after the immutable collection was published but before
            // its compact marker. The full original collection still freezes.
        }
        Err(error) => return Err(error.to_string()),
        Ok(_) => {
            let bytes =
                crate::world_initialization::read_private_owned_bounded(&marker, 16 * 1024)?;
            if bytes != marker_bytes(authority_id, &export.pending)? {
                return Err(
                    "pending handoff frozen marker differs from the complete original".into(),
                );
            }
        }
    }
    Ok(Some(export.pending))
}

fn publish_export(
    path: &Path,
    authority_id: &str,
    pending: &RecoveryPendingHandoffV1,
) -> Result<(), String> {
    if let Some(existing) = read_export(path, authority_id)? {
        if &existing != pending {
            return Err("pending handoff export conflicts with the original frozen set".into());
        }
        sync_export(path)?;
    } else {
        publish_immutable_bytes(
            path,
            &encode_export(authority_id, pending)?,
            MAX_EXPORT_BYTES,
        )?;
    }
    publish_immutable_bytes(
        &marker_path(path),
        &marker_bytes(authority_id, pending)?,
        16 * 1024,
    )
}

fn marker_path(path: &Path) -> PathBuf {
    path.with_extension("freeze")
}

fn marker_bytes(authority_id: &str, pending: &RecoveryPendingHandoffV1) -> Result<Vec<u8>, String> {
    serde_json::to_vec(&PendingFreezeMarkerV1 {
        schema_version: 1,
        authority_id: authority_id.into(),
        root: pending.root().map_err(|error| error.to_string())?,
        count: pending.requests.len(),
    })
    .map_err(|error| error.to_string())
}

fn publish_immutable_bytes(path: &Path, bytes: &[u8], maximum: u64) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or("pending handoff export parent missing")?;
    crate::controlled_live_config::require_private_owned(parent, true)?;
    let temp = path.with_extension(format!(
        "{}-{}.tmp",
        std::process::id(),
        EXPORT_TEMP.fetch_add(1, Ordering::Relaxed)
    ));
    let mut options = OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
    }
    let mut file = options.open(&temp).map_err(|error| error.to_string())?;
    let result = file.write_all(bytes).and_then(|_| file.sync_all());
    if let Err(error) = result {
        let _ = fs::remove_file(&temp);
        return Err(format!("persist original pending handoff: {error}"));
    }
    let published = fs::hard_link(&temp, path);
    let _ = fs::remove_file(&temp);
    match published {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            if crate::world_initialization::read_private_owned_bounded(path, maximum)? != bytes {
                return Err("pending handoff immutable publication conflict".into());
            }
        }
        Err(error) => return Err(format!("publish original pending handoff: {error}")),
    }
    sync_export(path)
}

#[cfg(all(test, unix))]
#[path = "formal_live_pending_handoff_tests.rs"]
mod tests;

fn sync_export(path: &Path) -> Result<(), String> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW);
    }
    options
        .open(path)
        .and_then(|file| file.sync_all())
        .map_err(|error| format!("sync pending handoff export: {error}"))?;
    File::open(path.parent().ok_or("pending handoff parent missing")?)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| format!("sync pending handoff directory: {error}"))
}
