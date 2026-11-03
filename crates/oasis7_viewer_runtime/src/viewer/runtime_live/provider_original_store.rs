//! Durable native owner for original provider request/response materials.
//!
//! This is an independent pending ledger, not a World RootSet or a global GC
//! registry. The verified Runtime owner supplies the exact scope and the
//! authority callbacks; this module never infers either from a legacy lineage.

#[path = "provider_original_store/capture.rs"]
mod capture;
#[path = "provider_original_store/pending.rs"]
mod pending;
#[path = "provider_original_store/terminal.rs"]
mod terminal;

use std::io::{Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use oasis7::simulator::{
    ContinuousAgentRequestContextV1, ContinuousAgentResponseContextV1,
    ProviderAttemptAuthorityBindingV1, ProviderDispositionEvidenceV1, ProviderMaterialAttemptV1,
    ProviderMaterialOwnerBindingV1, ProviderOriginalMaterialRefsV1, ProviderOriginalMaterialStore,
    ProviderPendingAttemptPhaseV1, ProviderPendingAttemptSummaryV1, ProviderPendingStateRootsV1,
    ProviderSpoolReservation,
};
use oasis7_distfs::incremental_page_store::{
    IncrementalPageStore, PageCompression, PageIoBudget,
    objects::{IncrementalObjectStore, ObjectIo},
};
use oasis7_formal_protocol::incremental_state::{
    MerkleNodeReader, StateDeltaV1, StateNamespaceV1, StatePageWriter, StateRootV1, StateWriteV1,
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, MemoryReservation};

use super::provider_original_store_codec::{
    EventPhase, EventRecord, MAX_EVENT_BYTES, MAX_EVENTS, MAX_OPERATION_ID_BYTES,
    MAX_OWNER_ID_BYTES, MAX_PENDING_ATTEMPTS, MAX_PROTECTED_ROOT_PAGE, OwnerHead, decode_event,
    decode_head, encode_event, event_key, operation_key, pending_root_digest,
};
use super::provider_original_store_fs::OwnerFiles;

const OWNER_DELTA_WORKSPACE: usize = 2 * 1024 * 1024;
const MAX_PENDING_NODE_BYTES: usize = 512 * 1024;
const MAX_CONTEXT_BYTES: usize = 1024 * 1024;
const MAX_REQUEST_BYTES: u64 = 16 * 1024 * 1024;
const MAX_RESPONSE_BYTES: u64 = 16 * 1024 * 1024;
const MAX_EVENTS_PER_OWNER: u64 = MAX_EVENTS;
const OPERATION_DOMAIN: &[u8] = b"oasis7/iws/provider-attempt/v1\0";
type Result<T, E = String> = std::result::Result<T, E>;

fn finite_page_io() -> PageIoBudget {
    PageIoBudget {
        reads_remaining: 100_000,
        read_bytes_remaining: 256 * 1024 * 1024,
        writes_remaining: 100_000,
        write_bytes_remaining: 256 * 1024 * 1024,
    }
}

fn finite_value_io() -> oasis7_formal_protocol::incremental_value::ValueIoBudget {
    oasis7_formal_protocol::incremental_value::ValueIoBudget {
        nodes_remaining: 100_000,
        read_bytes_remaining: 64 * 1024 * 1024,
        write_bytes_remaining: 64 * 1024 * 1024,
    }
}

struct Inner {
    head: OwnerHead,
    poisoned: bool,
}

#[derive(Clone, Copy)]
struct ProtectedRootsCursorState {
    pending_root: [u8; 32],
    after_key_digest: Option<[u8; 32]>,
    active_attempts_seen: u64,
}

/// A store-local pending owner. Dropping/reopening it preserves every root and
/// operation identity; there is no API that retries a dispatched attempt.
pub struct ProviderOriginalMaterialOwnerV1 {
    owner: ProviderMaterialOwnerBindingV1,
    memory: MemoryBudget,
    pages: IncrementalPageStore,
    files: OwnerFiles,
    authority: Arc<dyn oasis7::simulator::ProviderMaterialAuthority>,
    inner: Mutex<Inner>,
    cancelled: Arc<AtomicBool>,
    writable: bool,
}

impl ProviderOriginalMaterialOwnerV1 {
    /// Create a new independent owner directory. `owner` and `authority` must
    /// come from a verified new-format Runtime activation path.
    pub fn create(
        root: impl AsRef<Path>,
        owner: ProviderMaterialOwnerBindingV1,
        memory: MemoryBudget,
        compression: PageCompression,
        authority: Arc<dyn oasis7::simulator::ProviderMaterialAuthority>,
    ) -> Result<Arc<Self>, String> {
        authority.verify_owner_scope(&owner)?;
        let root = root.as_ref();
        let files = OwnerFiles::create(root)?;
        let pages_path = root.join("pages");
        let pages = IncrementalPageStore::create(&pages_path, memory.clone(), compression)
            .map_err(|error| error.to_string())?;
        let mut physical = finite_page_io();
        let cancelled = AtomicBool::new(false);
        IncrementalObjectStore::create(&pages, &mut physical, &cancelled)
            .map_err(|error| error.to_string())?;
        let empty = StateRootV1::empty(StateNamespaceV1::Effects)?;
        let head = OwnerHead {
            owner: owner.clone(),
            sequence: 0,
            previous_event_id: [0; 32],
            events: empty.clone(),
            pending: empty.clone(),
            seen: empty,
        };
        files.publish_head(&head, &memory)?;
        files.sync_root()?;
        Ok(Arc::new(Self {
            owner,
            memory,
            pages,
            files,
            authority,
            inner: Mutex::new(Inner {
                head,
                poisoned: false,
            }),
            cancelled: Arc::new(AtomicBool::new(false)),
            writable: true,
        }))
    }

    /// Reopen the exact independent store. Recovery-class opens are read-only;
    /// a live writer must use Ordinary memory and a fresh verified binding.
    pub fn open(
        root: impl AsRef<Path>,
        expected_owner: &ProviderMaterialOwnerBindingV1,
        memory: MemoryBudget,
        compression: PageCompression,
        class: MemoryClass,
        authority: Arc<dyn oasis7::simulator::ProviderMaterialAuthority>,
    ) -> Result<Arc<Self>, String> {
        authority.verify_owner_scope(expected_owner)?;
        let root = root.as_ref();
        let files = OwnerFiles::open(root)?;
        let pages =
            IncrementalPageStore::open(&root.join("pages"), memory.clone(), compression, class)
                .map_err(|error| error.to_string())?;
        let mut physical = finite_page_io();
        let cancelled = AtomicBool::new(false);
        IncrementalObjectStore::open(&pages, class, &mut physical, &cancelled)
            .map_err(|error| error.to_string())?;
        let encoded = files.read_head(&memory, class)?;
        let head = decode_head(encoded.as_slice())?;
        if &head.owner != expected_owner {
            return Err("provider pending owner scope differs from verified activation".into());
        }
        Ok(Arc::new(Self {
            owner: expected_owner.clone(),
            memory,
            pages,
            files,
            authority,
            inner: Mutex::new(Inner {
                head,
                poisoned: false,
            }),
            cancelled: Arc::new(AtomicBool::new(false)),
            writable: class == MemoryClass::Ordinary,
        }))
    }

    /// Set cancellation for the active operation. Once set, mutations fail
    /// closed and the owner must be reopened before further writes.
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }

    /// Store-local readiness check. This does not claim that a process-wide
    /// World/GC registry has registered this independent owner.
    pub fn close_ready(&self) -> Result<(), String> {
        let inner = self.lock_inner()?;
        self.ensure_healthy(&inner)?;
        if inner.head.pending.leaf_count != 0 {
            return Err("provider pending attempts remain protected".into());
        }
        Ok(())
    }

    pub fn owner_root(&self) -> &Path {
        self.files.root()
    }

    #[cfg(test)]
    pub(super) fn diagnostic_head_state(&self) -> String {
        match self.inner.try_lock() {
            Ok(inner) => format!(
                "owner_mutex=available sequence={} events={} pending={} seen={} poisoned={}",
                inner.head.sequence,
                inner.head.events.leaf_count,
                inner.head.pending.leaf_count,
                inner.head.seen.leaf_count,
                inner.poisoned,
            ),
            Err(std::sync::TryLockError::WouldBlock) => "owner_mutex=busy".into(),
            Err(std::sync::TryLockError::Poisoned(_)) => "owner_mutex=poisoned".into(),
        }
    }

    fn lock_inner(&self) -> Result<std::sync::MutexGuard<'_, Inner>, String> {
        self.inner
            .lock()
            .map_err(|_| "provider owner mutex poisoned".to_string())
    }

    fn ensure_healthy(&self, inner: &Inner) -> Result<(), String> {
        if inner.poisoned {
            return Err("provider pending owner requires reopen after uncertain write".into());
        }
        if self.cancelled() {
            return Err("provider pending owner cancelled".into());
        }
        Ok(())
    }

    fn require_writable(&self) -> Result<(), String> {
        if !self.writable {
            return Err("recovery-class provider owner is read-only".into());
        }
        Ok(())
    }

    fn read_state_value(
        &self,
        root: &StateRootV1,
        key: &[u8],
        class: MemoryClass,
        physical: &mut PageIoBudget,
    ) -> Result<Option<[u8; 32]>, String> {
        if self.cancelled() {
            return Err("provider pending state read cancelled".into());
        }
        let session = self
            .pages
            .state_session(physical.clone(), class)
            .map_err(|error| error.to_string())?;
        let source = CancelReader {
            inner: &session,
            cancelled: &self.cancelled,
        };
        let result = root.get(key, &source).map_err(|error| {
            session
                .take_read_error()
                .map(|typed| typed.to_string())
                .unwrap_or(error)
        });
        *physical = session.remaining_io().map_err(|error| error.to_string())?;
        if self.cancelled() {
            return Err("provider pending state read cancelled".into());
        }
        result
    }

    fn read_event_object(
        &self,
        id: &[u8; 32],
        class: MemoryClass,
        physical: &mut PageIoBudget,
    ) -> Result<EventRecord, String> {
        let mut logical = finite_value_io();
        let objects = IncrementalObjectStore::open(&self.pages, class, physical, &self.cancelled)
            .map_err(|error| error.to_string())?;
        let bytes = objects
            .read_object_owned(
                id,
                MAX_EVENT_BYTES,
                &mut ObjectIo {
                    physical,
                    logical: &mut logical,
                    class,
                    cancelled: &self.cancelled,
                },
            )
            .map_err(|error| error.to_string())?;
        let _decode_workspace = self
            .memory
            .reserve(MAX_EVENT_BYTES, class)
            .map_err(|error| error.to_string())?;
        decode_event(bytes.as_slice())
    }

    fn latest_event(
        &self,
        head: &OwnerHead,
        operation_id: &str,
        root: &StateRootV1,
        class: MemoryClass,
        physical: &mut PageIoBudget,
    ) -> Result<Option<([u8; 32], EventRecord)>, String> {
        let key = operation_key(operation_id)?;
        let Some(id) = self.read_state_value(root, &key, class, physical)? else {
            return Ok(None);
        };
        let event = self.read_event_object(&id, class, physical)?;
        if event.operation_id != operation_id || event.sequence > head.sequence {
            return Err("provider pending index/event mismatch".into());
        }
        Ok(Some((id, event)))
    }

    fn validate_attempt(
        &self,
        attempt: &ProviderMaterialAttemptV1,
        record: &EventRecord,
    ) -> Result<(), String> {
        if attempt.owner != self.owner
            || attempt.operation_id != record.operation_id
            || attempt.transport_attempt != record.transport_attempt
            || attempt.attempt_id != record.attempt_id
            || attempt.parent_root_set != record.parent_root_set
            || attempt.authority_snapshot != record.authority_snapshot
            || attempt.request_object_id != record.request_object_id
            || attempt.request_context_object_id != record.request_context_object_id
        {
            return Err("provider attempt does not match durable pending event".into());
        }
        Ok(())
    }

    fn append_event(&self, inner: &mut Inner, mut record: EventRecord) -> Result<[u8; 32], String> {
        self.ensure_healthy(inner)?;
        let next = inner
            .head
            .sequence
            .checked_add(1)
            .filter(|next| *next <= MAX_EVENTS_PER_OWNER)
            .ok_or("provider event history bound")?;
        record.sequence = next;
        record.previous_event_id = inner.head.previous_event_id;
        let _event_charge = self
            .memory
            .reserve(MAX_EVENT_BYTES, MemoryClass::Ordinary)
            .map_err(|error| error.to_string())?;
        let encoded = encode_event(&record)?;
        if encoded.len() > MAX_EVENT_BYTES {
            return Err("provider event record bound".into());
        }
        let event_id = *blake3::hash(&encoded).as_bytes();
        let mut physical = finite_page_io();
        let mut logical = finite_value_io();
        let objects = IncrementalObjectStore::open(
            &self.pages,
            MemoryClass::Ordinary,
            &mut physical,
            &self.cancelled,
        )
        .map_err(|error| error.to_string())?;
        let persisted = objects
            .write_object(
                &mut encoded.as_slice(),
                encoded.len() as u64,
                Some(event_id),
                &mut ObjectIo {
                    physical: &mut physical,
                    logical: &mut logical,
                    class: MemoryClass::Ordinary,
                    cancelled: &self.cancelled,
                },
            )
            .map_err(|error| error.to_string())?;
        if persisted != event_id {
            return Err("provider event content ID changed".into());
        }
        if self.cancelled() {
            return Err("provider owner cancelled after event persistence".into());
        }

        let event_write = StateWriteV1 {
            key: event_key(next),
            before: None,
            after: Some(event_id),
        };
        let events = self.apply_state_write(&inner.head.events, event_write, &mut physical)?;
        let op_key = operation_key(&record.operation_id)?;
        let old_pending = self.read_state_value(
            &inner.head.pending,
            &op_key,
            MemoryClass::Ordinary,
            &mut physical,
        )?;
        let old_seen = self.read_state_value(
            &inner.head.seen,
            &op_key,
            MemoryClass::Ordinary,
            &mut physical,
        )?;
        if inner.head.pending.leaf_count > MAX_PENDING_ATTEMPTS
            || (record.phase == EventPhase::RequestPrepared
                && inner.head.pending.leaf_count == MAX_PENDING_ATTEMPTS)
        {
            return Err("provider pending attempt count bound".into());
        }
        if record.phase == EventPhase::RequestPrepared {
            if old_pending.is_some() || old_seen.is_some() {
                return Err("provider operation ID was already used".into());
            }
        } else if old_pending.is_none() || old_seen.is_none() || old_pending != old_seen {
            return Err("provider phase pending/seen index mismatch".into());
        }
        let terminal = matches!(record.phase, EventPhase::Committed | EventPhase::Rejected);
        let pending_write = StateWriteV1 {
            key: op_key.clone(),
            before: old_pending,
            after: if terminal { None } else { Some(event_id) },
        };
        let seen_write = StateWriteV1 {
            key: op_key,
            before: old_seen,
            after: Some(event_id),
        };
        let pending = self.apply_state_write(&inner.head.pending, pending_write, &mut physical)?;
        let seen = self.apply_state_write(&inner.head.seen, seen_write, &mut physical)?;
        if self.cancelled() {
            return Err("provider owner cancelled before head publication".into());
        }
        let head = OwnerHead {
            owner: self.owner.clone(),
            sequence: next,
            previous_event_id: event_id,
            events,
            pending,
            seen,
        };
        self.files.publish_head(&head, &self.memory)?;
        self.files.sync_root()?;
        inner.head = head;
        Ok(event_id)
    }

    fn apply_state_write(
        &self,
        before: &StateRootV1,
        write: StateWriteV1,
        physical: &mut PageIoBudget,
    ) -> Result<StateRootV1, String> {
        let _workspace = self
            .memory
            .reserve(OWNER_DELTA_WORKSPACE, MemoryClass::Ordinary)
            .map_err(|error| error.to_string())?;
        if self.cancelled() {
            return Err("provider owner cancelled before state delta".into());
        }
        let reader = self
            .pages
            .state_session(physical.clone(), MemoryClass::Ordinary)
            .map_err(|error| error.to_string())?;
        let cancel_reader = CancelReader {
            inner: &reader,
            cancelled: &self.cancelled,
        };
        let delta = StateDeltaV1::build(
            before.clone(),
            vec![write],
            &cancel_reader,
            MAX_PENDING_NODE_BYTES,
        )
        .map_err(|error| {
            reader
                .take_read_error()
                .map(|typed| typed.to_string())
                .unwrap_or(error)
        })?;
        *physical = reader.remaining_io().map_err(|error| error.to_string())?;
        drop(reader);
        let pending_bytes = delta
            .new_nodes
            .iter()
            .try_fold(0usize, |sum, node| sum.checked_add(node.bytes.len()));
        if pending_bytes.is_none_or(|bytes| bytes > MAX_PENDING_NODE_BYTES) {
            return Err("provider state delta node budget".into());
        }
        let retained = self
            .memory
            .reserve(OWNER_DELTA_WORKSPACE, MemoryClass::Ordinary)
            .map_err(|error| error.to_string())?
            .retain(delta);
        let mut writer = self.pages.value_session(physical.clone());
        writer
            .put_state_pages_cancelled(&retained, &self.memory, &mut || self.cancelled())
            .map_err(|error| error.to_string())?;
        *physical = writer.remaining_io().map_err(|error| error.to_string())?;
        if self.cancelled() {
            return Err("provider owner cancelled after state pages".into());
        }
        Ok(retained.after.clone())
    }

    fn latest_for_attempt(
        &self,
        inner: &Inner,
        attempt: &ProviderMaterialAttemptV1,
        physical: &mut PageIoBudget,
    ) -> Result<EventRecord, String> {
        let (_, record) = self
            .latest_event(
                &inner.head,
                &attempt.operation_id,
                &inner.head.seen,
                MemoryClass::Ordinary,
                physical,
            )?
            .ok_or("provider attempt is not in the durable seen index")?;
        self.validate_attempt(attempt, &record)?;
        Ok(record)
    }

    fn attempt_from_event(&self, record: &EventRecord) -> ProviderMaterialAttemptV1 {
        ProviderMaterialAttemptV1 {
            owner: self.owner.clone(),
            operation_id: record.operation_id.clone(),
            transport_attempt: record.transport_attempt,
            attempt_id: record.attempt_id,
            parent_root_set: record.parent_root_set,
            authority_snapshot: record.authority_snapshot,
            request_object_id: record.request_object_id,
            request_context_object_id: record.request_context_object_id,
        }
    }

    fn json_bytes<T: serde::Serialize>(
        &self,
        value: &T,
        limit: usize,
    ) -> Result<AdmittedBytes, String> {
        let charge = self
            .memory
            .reserve(limit.saturating_add(128), MemoryClass::Ordinary)
            .map_err(|error| error.to_string())?;
        let mut output = LimitedBytes {
            bytes: Vec::with_capacity(limit.min(64 * 1024)),
            limit,
        };
        serde_json::to_writer(&mut output, value).map_err(|error| error.to_string())?;
        if output.bytes.len() > limit || output.bytes.capacity() > charge.bytes() {
            return Err("provider context allocation exceeded its admitted bound".into());
        }
        Ok(AdmittedBytes {
            bytes: output.bytes,
            _charge: charge,
        })
    }

    fn persist_object(&self, bytes: &[u8], expected: [u8; 32]) -> Result<(), String> {
        if bytes.len() as u64 > MAX_CONTEXT_BYTES as u64 {
            return Err("provider context object bound".into());
        }
        let mut physical = finite_page_io();
        let mut logical = finite_value_io();
        let objects = IncrementalObjectStore::open(
            &self.pages,
            MemoryClass::Ordinary,
            &mut physical,
            &self.cancelled,
        )
        .map_err(|error| error.to_string())?;
        let mut input = bytes;
        let id = objects
            .write_object(
                &mut input,
                bytes.len() as u64,
                Some(expected),
                &mut ObjectIo {
                    physical: &mut physical,
                    logical: &mut logical,
                    class: MemoryClass::Ordinary,
                    cancelled: &self.cancelled,
                },
            )
            .map_err(|error| error.to_string())?;
        if id != expected {
            return Err("provider typed object identity mismatch".into());
        }
        Ok(())
    }

    fn read_object_bytes(
        &self,
        id: &[u8; 32],
        max: usize,
        class: MemoryClass,
    ) -> Result<Arc<oasis7_proto::memory_budget::ReservedValue<Vec<u8>>>, String> {
        let mut physical = finite_page_io();
        let mut logical = finite_value_io();
        let objects =
            IncrementalObjectStore::open(&self.pages, class, &mut physical, &self.cancelled)
                .map_err(|error| error.to_string())?;
        objects
            .read_object_owned(
                id,
                max,
                &mut ObjectIo {
                    physical: &mut physical,
                    logical: &mut logical,
                    class,
                    cancelled: &self.cancelled,
                },
            )
            .map_err(|error| error.to_string())
    }

    fn refs_from_event(
        &self,
        record: &EventRecord,
    ) -> Result<ProviderOriginalMaterialRefsV1, String> {
        Ok(ProviderOriginalMaterialRefsV1 {
            owner: self.owner.clone(),
            operation_id: record.operation_id.clone(),
            transport_attempt: record.transport_attempt,
            attempt_id: record.attempt_id,
            parent_root_set: record.parent_root_set,
            authority_snapshot: record.authority_snapshot,
            request_object_id: record.request_object_id,
            request_context_object_id: record.request_context_object_id,
            response_object_id: record
                .response_object_id
                .ok_or("provider response body is absent")?,
            response_context_object_id: record
                .response_context_object_id
                .ok_or("provider typed response is absent")?,
            response_status: record
                .response_status
                .ok_or("provider response status is absent")?,
        })
    }
}

struct AdmittedBytes {
    bytes: Vec<u8>,
    _charge: MemoryReservation,
}

struct LimitedBytes {
    bytes: Vec<u8>,
    limit: usize,
}
impl Write for LimitedBytes {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        let next = self
            .bytes
            .len()
            .checked_add(bytes.len())
            .filter(|next| *next <= self.limit)
            .ok_or_else(|| std::io::Error::other("provider context size limit"))?;
        self.bytes.try_reserve(next - self.bytes.len())?;
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

struct CancelReader<'a, R> {
    inner: &'a R,
    cancelled: &'a AtomicBool,
}
impl<R: MerkleNodeReader> MerkleNodeReader for CancelReader<'_, R> {
    type Node = R::Node;
    fn allocation_class(&self) -> Option<MemoryClass> {
        self.inner.allocation_class()
    }
    fn allocation_budget(&self) -> Option<&MemoryBudget> {
        self.inner.allocation_budget()
    }
    fn read_node(&self, digest: &str, max: usize) -> Result<Option<Self::Node>, String> {
        if self.cancelled.load(Ordering::Acquire) {
            return Err("provider owner cancelled".into());
        }
        let value = self.inner.read_node(digest, max)?;
        if self.cancelled.load(Ordering::Acquire) {
            return Err("provider owner cancelled".into());
        }
        Ok(value)
    }
}

impl ProviderOriginalMaterialStore for ProviderOriginalMaterialOwnerV1 {
    fn owner_binding(&self) -> &ProviderMaterialOwnerBindingV1 {
        &self.owner
    }

    fn pending_protection_root(&self) -> Result<[u8; 32], String> {
        self.pending_protection_root_impl()
    }
    fn protected_roots_page(
        &self,
        cursor: Option<&oasis7::simulator::ProviderPendingRootsCursorV1>,
        limit: usize,
    ) -> Result<oasis7::simulator::ProviderPendingRootsPageV1, String> {
        self.protected_roots_page_impl(cursor, limit)
    }
    fn validate_request_owner(
        &self,
        request: &ContinuousAgentRequestContextV1,
    ) -> Result<(), String> {
        self.validate_request_owner_impl(request)
    }
    fn memory_budget(&self) -> MemoryBudget {
        self.memory.clone()
    }
    fn request_body_limit_bytes(&self) -> u64 {
        MAX_REQUEST_BYTES
    }
    fn response_body_limit_bytes(&self) -> u64 {
        MAX_RESPONSE_BYTES
    }

    fn reserve_body_spools(
        &self,
        request_limit: u64,
        response_limit: u64,
    ) -> Result<Box<dyn ProviderSpoolReservation>, String> {
        if request_limit > MAX_REQUEST_BYTES || response_limit > MAX_RESPONSE_BYTES {
            return Err("provider spool reservation exceeds owner limit".into());
        }
        let bytes = request_limit
            .checked_add(response_limit)
            .and_then(|value| usize::try_from(value).ok())
            .ok_or("provider spool reservation overflow")?;
        self.memory
            .reserve(bytes, MemoryClass::Ordinary)
            .map(|reservation| {
                Box::new(SpoolReservation {
                    _lease: reservation,
                }) as _
            })
            .map_err(|error| error.to_string())
    }

    fn cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }

    fn persist_request(
        &self,
        request: &ContinuousAgentRequestContextV1,
        expected_body_id: [u8; 32],
        original_body: &mut dyn Read,
        length: u64,
    ) -> Result<ProviderMaterialAttemptV1, String> {
        self.persist_request_impl(request, expected_body_id, original_body, length)
    }
    fn mark_dispatched(&self, attempt: &ProviderMaterialAttemptV1) -> Result<(), String> {
        self.mark_dispatched_impl(attempt)
    }
    fn persist_response_body(
        &self,
        attempt: &ProviderMaterialAttemptV1,
        status: u16,
        expected_body_id: [u8; 32],
        original_body: &mut dyn Read,
        length: u64,
    ) -> Result<[u8; 32], String> {
        self.persist_response_body_impl(attempt, status, expected_body_id, original_body, length)
    }
    fn finish_response(
        &self,
        attempt: &ProviderMaterialAttemptV1,
        status: u16,
        response_object_id: [u8; 32],
        response: &ContinuousAgentResponseContextV1,
    ) -> Result<ProviderOriginalMaterialRefsV1, String> {
        self.finish_response_impl(attempt, status, response_object_id, response)
    }
    fn verify_commit_materials(
        &self,
        materials: &ProviderOriginalMaterialRefsV1,
        request: &ContinuousAgentRequestContextV1,
        response: &ContinuousAgentResponseContextV1,
    ) -> Result<(), String> {
        self.verify_commit_materials_impl(materials, request, response)
    }
    fn mark_committed(
        &self,
        materials: &ProviderOriginalMaterialRefsV1,
        receipt_id: &str,
        action_id: u64,
    ) -> Result<(), String> {
        self.mark_committed_impl(materials, receipt_id, action_id)
    }
    fn mark_rejected(
        &self,
        attempt: &ProviderMaterialAttemptV1,
        evidence: ProviderDispositionEvidenceV1,
    ) -> Result<(), String> {
        self.mark_rejected_impl(attempt, evidence)
    }
}

fn make_attempt_id(
    owner: &ProviderMaterialOwnerBindingV1,
    operation_id: &str,
    transport_attempt: u64,
    binding: ProviderAttemptAuthorityBindingV1,
    request_object: [u8; 32],
    context_object: [u8; 32],
) -> [u8; 32] {
    let mut hash = blake3::Hasher::new();
    hash.update(OPERATION_DOMAIN);
    hash.update(&owner.genesis_root);
    hash.update(&owner.owner_id);
    hash.update(&(operation_id.len() as u16).to_be_bytes());
    hash.update(operation_id.as_bytes());
    hash.update(&transport_attempt.to_be_bytes());
    hash.update(&binding.parent_root_set);
    hash.update(&binding.authority_snapshot);
    hash.update(&request_object);
    hash.update(&context_object);
    *hash.finalize().as_bytes()
}

struct SpoolReservation {
    _lease: MemoryReservation,
}
impl ProviderSpoolReservation for SpoolReservation {}

/// Explicit installer. Call only after the Runtime owner has verified the
/// current World/RootSet/authority; legacy Viewer construction never creates
/// this store implicitly.
pub fn install_provider_original_material_owner(
    server: &mut crate::viewer::runtime_live::ViewerRuntimeLiveServer,
    owner: Arc<ProviderOriginalMaterialOwnerV1>,
) -> Result<(), String> {
    let store: Arc<dyn ProviderOriginalMaterialStore> = owner;
    server.install_provider_original_material_store(store)
}
