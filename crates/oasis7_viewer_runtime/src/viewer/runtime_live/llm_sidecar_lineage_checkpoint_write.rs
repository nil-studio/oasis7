//! Atomic serialization of the durable private lineage checkpoint.
use super::*;

pub(in crate::viewer::runtime_live) struct LineagePublicationOwner {
    pending: std::sync::atomic::AtomicU64,
    generation: std::sync::atomic::AtomicU64,
    deferred: std::sync::atomic::AtomicBool,
    capture_failed: std::sync::atomic::AtomicBool,
    outstanding: std::sync::atomic::AtomicUsize,
    batch: std::sync::Mutex<Option<PublicationBatch>>,
    inflight: std::sync::Mutex<Option<PublicationBatch>>,
    failed_original: std::sync::Mutex<Option<OwnedLineagePublication>>,
}
impl Default for LineagePublicationOwner {
    fn default() -> Self {
        Self {
            pending: 0.into(),
            generation: 1.into(),
            deferred: false.into(),
            capture_failed: false.into(),
            outstanding: 0.into(),
            batch: None.into(),
            inflight: None.into(),
            failed_original: None.into(),
        }
    }
}
#[derive(Clone, PartialEq, Eq)]
pub(in crate::viewer::runtime_live) struct PublicationSourceScope {
    world: String,
    branch: String,
    epoch: u64,
    finality_hash: Option<String>,
    finality: String,
    reorg: u64,
    manifest: String,
}
impl RuntimeLlmSidecar {
    pub(in crate::viewer::runtime_live) fn publication_source_scope(
        &self,
    ) -> Option<PublicationSourceScope> {
        self.provider_lineage_binding
            .as_ref()
            .map(|binding| PublicationSourceScope {
                world: binding.world_id.clone(),
                branch: binding.branch_id.clone(),
                epoch: binding.finality_epoch,
                finality_hash: binding
                    .finality_block_hash
                    .as_ref()
                    .map(ToString::to_string),
                finality: binding.finality_status.clone(),
                reorg: binding.reorg_epoch,
                manifest: binding.runtime_manifest_hash.to_string(),
            })
    }
    pub(in crate::viewer::runtime_live) fn publication_store_path(&self) -> Option<&Path> {
        self.provider_lineage_store.as_deref()
    }

    pub(in crate::viewer::runtime_live) fn publication_sessions(&self) -> BTreeMap<String, String> {
        self.provider_session_ids.clone()
    }
    pub(in crate::viewer::runtime_live) fn ensure_lineage_published(&self) -> Result<(), String> {
        if self.publication_capture_failed()
            || self
                .publication
                .pending
                .load(std::sync::atomic::Ordering::Acquire)
                != 0
        {
            Err("original Agent checkpoint publication remains unconfirmed; dispatch fenced".into())
        } else {
            Ok(())
        }
    }
    pub(in crate::viewer::runtime_live) fn enable_deferred_publications(&self) {
        self.publication
            .deferred
            .store(true, std::sync::atomic::Ordering::Release);
    }
    fn publication_queue(&self) -> PublicationBatch {
        self.publication
            .batch
            .lock()
            .unwrap()
            .get_or_insert_with(|| std::sync::Arc::new(std::sync::Mutex::new(Vec::new())))
            .clone()
    }
    pub(in crate::viewer::runtime_live) fn take_queued_publications(
        &self,
    ) -> Vec<OwnedLineagePublication> {
        std::mem::take(&mut *self.publication_queue().lock().unwrap())
    }
    pub(in crate::viewer::runtime_live) fn retain_inflight_publications(
        &self,
        batch: PublicationBatch,
    ) {
        *self.publication.inflight.lock().unwrap() = Some(batch);
    }
    pub(in crate::viewer::runtime_live) fn publication_ticket(&self) -> u64 {
        self.publication
            .pending
            .load(std::sync::atomic::Ordering::Acquire)
    }
    pub(in crate::viewer::runtime_live) fn publication_capture_failed(&self) -> bool {
        self.publication
            .capture_failed
            .load(std::sync::atomic::Ordering::Acquire)
    }
    pub(in crate::viewer::runtime_live) fn confirm_lineage_publication(
        &self,
        ticket: u64,
    ) -> Result<(), String> {
        if self.publication_capture_failed() {
            return Err("lineage capture failed; original batch retained".into());
        }
        self.publication
            .pending
            .compare_exchange(
                ticket,
                0,
                std::sync::atomic::Ordering::AcqRel,
                std::sync::atomic::Ordering::Acquire,
            )
            .map_err(|_| "lineage publication completion ticket changed")?;
        self.publication
            .outstanding
            .store(0, std::sync::atomic::Ordering::Release);
        self.publication
            .batch
            .lock()
            .map_err(|_| "lineage publication owner poisoned")?
            .take();
        self.publication
            .inflight
            .lock()
            .map_err(|_| "lineage in-flight publication owner poisoned")?
            .take();
        Ok(())
    }
    pub(in crate::viewer::runtime_live) fn persist_provider_lineage(&self) -> Result<(), String> {
        let result = self.persist_provider_lineage_inner();
        if result.is_err()
            && (self
                .publication
                .deferred
                .load(std::sync::atomic::Ordering::Acquire)
                || is_deferred(self))
        {
            self.publication
                .capture_failed
                .store(true, std::sync::atomic::Ordering::Release);
        }
        result
    }
    fn persist_provider_lineage_inner(&self) -> Result<(), String> {
        if self.publication_capture_failed() {
            return Err("lineage capture failed; originals retained for recovery".into());
        }
        let background = self
            .publication
            .deferred
            .load(std::sync::atomic::Ordering::Acquire);
        if !background && !is_deferred(self) {
            self.ensure_lineage_published()?;
        }

        let Some(path) = self.provider_lineage_store.as_deref() else {
            return Ok(());
        };
        let reservation = ordered_publication::reserve(path)?;
        let capture_phase = crate::viewer::runtime_live::stream_budget_diagnostics::phase(43);
        let checkpoint = PersistedProviderLineageV1 {
            schema_version: PROVIDER_LINEAGE_SCHEMA_VERSION,
            provider_session_ids: self.provider_session_ids.clone(),
            provider_agent_ids: self.provider_agent_ids.clone(),
            provider_context_seq: self.provider_context_seq.clone(),
            provider_contexts: self.provider_contexts.clone(),
            provider_retry_contexts: self.provider_retry_contexts.clone(),
            provider_active_turns: self.provider_active_turns.clone(),
            provider_cognition_leases: self.provider_cognition_leases.clone(),
            provider_capability_identities: self.provider_capability_identities.clone(),
            provider_continuation_proposals: self.provider_continuation_proposals.clone(),
            provider_continuation_recovery_pending: self
                .provider_continuation_recovery_pending
                .clone(),
            provider_recovery_pending: self.provider_recovery_pending.clone(),
            provider_wake_recovery_pending: self.provider_wake_recovery_pending.clone(),
            provider_service_pending: self.provider_service_pending.clone(),
            provider_scheduler_pending: self.provider_scheduler_pending.clone(),
            hosted_admission: self.hosted_admission.clone(),
            hosted_wait: self.hosted_wait.clone(),
            hosted_resume: self.hosted_resume.clone(),
            provider_wait_until: self.provider_wait_until.clone(),
            provider_feedback_seq: self.provider_feedback_seq.clone(),
            provider_feedback_seq_by_session: self.provider_feedback_seq_by_session.clone(),
            provider_memory_store: self.provider_memory_store.clone(),
            provider_completed_decisions: self.provider_completed_decisions.clone(),
            provider_held_decisions: self.provider_held_decisions.clone(),
            provider_stale_replans: self.provider_stale_replans.clone(),
            provider_transport_exhausted: self.provider_transport_exhausted.clone(),
            provider_terminal_states: self.provider_terminal_states.clone(),
            provider_late_response_diagnostics: self.provider_late_response_diagnostics.clone(),
            pending_actions: self.pending_actions.clone(),
            pending_provider_world_events: self.pending_provider_world_events.clone(),
            provider_world_event_quarantine: self.provider_world_event_quarantine.clone(),
            provider_lineage_recovery_pending: self.provider_lineage_recovery_pending.clone(),
            runtime_binding: self.provider_lineage_binding.clone(),
            pending_runtime_wakes: self.pending_runtime_wakes.clone(),
        };
        drop(capture_phase);
        let publication = OwnedLineagePublication {
            path: path.to_owned(),
            checkpoint,
            reservation,
            #[cfg(any(test, feature = "test_tier_required"))]
            from_prepare: publication_gate::in_prepare(),
            unique_id: next_temporary_id()?,
            temporary_written: std::sync::atomic::AtomicBool::new(false),
            temporary: std::sync::OnceLock::new(),
            completed: std::sync::atomic::AtomicBool::new(false),
            blocked: std::sync::atomic::AtomicBool::new(false),
        };
        let captured = DEFERRED.with(|pending| {
            pending
                .borrow()
                .as_ref()
                .filter(|capture| capture.owner == self as *const Self as usize)
                .map(|capture| capture.batch.clone())
        });
        if let Some(batch) = captured.or_else(|| background.then(|| self.publication_queue())) {
            let mut batch = batch
                .lock()
                .map_err(|_| "lineage publication batch poisoned")?;
            if self
                .publication
                .outstanding
                .load(std::sync::atomic::Ordering::Acquire)
                >= 32
            {
                self.publication
                    .capture_failed
                    .store(true, std::sync::atomic::Ordering::Release);
                *self.publication.failed_original.lock().unwrap() = Some(publication);
                return Err(
                    "lineage publication batch capacity exhausted; original batch retained".into(),
                );
            }
            let ticket = self
                .publication
                .generation
                .fetch_update(
                    std::sync::atomic::Ordering::AcqRel,
                    std::sync::atomic::Ordering::Acquire,
                    |next| next.checked_add(1),
                )
                .map_err(|_| "lineage owner generation exhausted")?;
            self.publication
                .pending
                .store(ticket, std::sync::atomic::Ordering::Release);
            self.publication
                .outstanding
                .fetch_add(1, std::sync::atomic::Ordering::AcqRel);
            batch.push(publication);
            Ok(())
        } else {
            publication.execute()
        }
    }
}

pub(in crate::viewer::runtime_live) struct OwnedLineagePublication {
    path: PathBuf,
    unique_id: u64,
    temporary_written: std::sync::atomic::AtomicBool,
    temporary: std::sync::OnceLock<PathBuf>,
    completed: std::sync::atomic::AtomicBool,
    blocked: std::sync::atomic::AtomicBool,
    #[cfg(any(test, feature = "test_tier_required"))]
    from_prepare: bool,
    checkpoint: PersistedProviderLineageV1,
    reservation: ordered_publication::Reservation,
}
impl OwnedLineagePublication {
    pub(in crate::viewer::runtime_live) fn permanently_blocked(&self) -> bool {
        self.blocked.load(std::sync::atomic::Ordering::Acquire)
    }
    pub(in crate::viewer::runtime_live) fn execute(&self) -> Result<(), String> {
        if self.permanently_blocked() {
            return Err(
                "lineage original publication is ambiguous; retained for explicit recovery".into(),
            );
        }
        if self.completed.load(std::sync::atomic::Ordering::Acquire) {
            return Ok(());
        }
        #[cfg(any(test, feature = "test_tier_required"))]
        let _prepare_scope = self.from_prepare.then(publication_gate::prepare_scope);
        #[cfg(any(test, feature = "test_tier_required"))]
        let fresh_original = self
            .checkpoint
            .hosted_admission
            .as_ref()
            .is_some_and(|admission| {
                self.checkpoint
                    .provider_scheduler_pending
                    .values()
                    .any(|pending| admission.has_exact_fresh_scheduler_original(pending))
            });
        let encode_phase = crate::viewer::runtime_live::stream_budget_diagnostics::phase(44);
        let encoded = serde_json::to_vec_pretty(&self.checkpoint).map_err(|error| {
            self.blocked
                .store(true, std::sync::atomic::Ordering::Release);
            format!("provider lineage checkpoint encode failed; original retained: {error}")
        })?;
        drop(encode_phase);
        let path = self.path.as_path();
        let directory_phase = crate::viewer::runtime_live::stream_budget_diagnostics::phase(45);
        let parent = path.parent().unwrap_or_else(|| Path::new("."));
        fs::create_dir_all(parent).map_err(|error| {
            format!(
                "provider lineage checkpoint directory creation failed ({}): {error}",
                parent.display()
            )
        })?;
        drop(directory_phase);

        let temp_path = self.temporary.get_or_init(|| {
            path.with_extension(format!("tmp-{}-{}", std::process::id(), self.unique_id))
        });
        #[cfg(any(test, feature = "test_tier_required"))]
        publication_gate::wait_at(path, temp_path, true, fresh_original)?;
        let write_phase = crate::viewer::runtime_live::stream_budget_diagnostics::phase(46);
        if !self
            .temporary_written
            .load(std::sync::atomic::Ordering::Acquire)
        {
            use std::io::Write;
            let mut options = fs::OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            let mut file = options.open(temp_path).map_err(|error| {
                if error.kind() == std::io::ErrorKind::AlreadyExists {
                    self.blocked
                        .store(true, std::sync::atomic::Ordering::Release);
                }
                format!("provider lineage private temporary creation failed: {error}")
            })?;
            file.write_all(&encoded).map_err(|error| {
                self.blocked
                    .store(true, std::sync::atomic::Ordering::Release);
                format!("provider lineage temporary write failed; original retained: {error}")
            })?;
            self.temporary_written
                .store(true, std::sync::atomic::Ordering::Release);
        }
        drop(write_phase);
        #[cfg(any(test, feature = "test_tier_required"))]
        publication_gate::wait_at(path, temp_path, false, fresh_original)?;
        #[cfg(any(test, feature = "test_tier_required"))]
        publication_failure_probe::before_rename(path, &encoded)?;
        let rename_phase = crate::viewer::runtime_live::stream_budget_diagnostics::phase(47);
        if let Err(error) = self.reservation.publish(temp_path) {
            if error.permanently_blocked() {
                self.blocked
                    .store(true, std::sync::atomic::Ordering::Release);
            }
            #[cfg(any(test, feature = "test_tier_required"))]
            if !error.permanently_blocked() {
                publication_failure_probe::observe_unpublished(path, temp_path, &encoded)?;
            }
            return Err(format!(
                "provider lineage checkpoint commit failed ({}): {error}",
                path.display()
            ));
        }
        drop(rename_phase);
        #[cfg(any(test, feature = "test_tier_required"))]
        publication_gate::after_rename(path, temp_path);
        #[cfg(any(test, feature = "test_tier_required"))]
        publication_failure_probe::observe_published(path, &encoded)?;
        self.completed
            .store(true, std::sync::atomic::Ordering::Release);
        Ok(())
    }
}
pub(in crate::viewer::runtime_live) type PublicationBatch =
    std::sync::Arc<std::sync::Mutex<Vec<OwnedLineagePublication>>>;
thread_local! {
    static DEFERRED: std::cell::RefCell<Option<Capture>> = const { std::cell::RefCell::new(None) };
}
struct Capture {
    owner: usize,
    batch: PublicationBatch,
}
pub(in crate::viewer::runtime_live) struct DeferredPublications;
impl DeferredPublications {
    pub(in crate::viewer::runtime_live) fn begin(
        owner: &RuntimeLlmSidecar,
    ) -> Result<Self, String> {
        if owner.publication_capture_failed() {
            return Err("lineage capture failed; original batch retained".into());
        }
        let batch = owner.publication_queue();
        DEFERRED.with(|pending| {
            assert!(pending.borrow().is_none());
            *pending.borrow_mut() = Some(Capture {
                owner: owner as *const RuntimeLlmSidecar as usize,
                batch,
            });
        });
        Ok(Self)
    }
    pub(in crate::viewer::runtime_live) fn finish(self) -> PublicationBatch {
        DEFERRED.with(|pending| pending.borrow_mut().take().unwrap().batch)
    }
}
impl Drop for DeferredPublications {
    fn drop(&mut self) {
        // The owner retains its Arc until exact-ticket acknowledgement, even
        // if an error unwinds this capture scope before finish.
        DEFERRED.with(|pending| {
            pending.borrow_mut().take();
        });
    }
}
fn next_temporary_id() -> Result<u64, String> {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    NEXT.fetch_update(
        std::sync::atomic::Ordering::AcqRel,
        std::sync::atomic::Ordering::Acquire,
        |next| next.checked_add(1),
    )
    .map_err(|_| "lineage temporary identity exhausted".into())
}

pub(in crate::viewer::runtime_live) fn is_deferred(owner: &RuntimeLlmSidecar) -> bool {
    DEFERRED.with(|pending| {
        pending
            .borrow()
            .as_ref()
            .is_some_and(|capture| capture.owner == owner as *const RuntimeLlmSidecar as usize)
    })
}

#[cfg(any(test, feature = "test_tier_required"))]
#[path = "llm_sidecar_lineage_publication_gate.rs"]
pub(in crate::viewer::runtime_live) mod publication_gate;

#[path = "llm_sidecar_lineage_ordered_publication.rs"]
mod ordered_publication;

#[cfg(any(test, feature = "test_tier_required"))]
#[path = "llm_sidecar_lineage_publication_failure_probe.rs"]
mod publication_failure_probe;
