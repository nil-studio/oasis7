//! Durable native owner for original provider request/response materials.
//!
//! This is an independent pending ledger, not a World RootSet or a global GC
//! registry. The verified Runtime owner supplies the exact scope and the
//! authority callbacks; this module never infers either from a legacy lineage.

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
    IncrementalPageStore, PageCompression, PageIoBudget, ValuePageSession,
    objects::{IncrementalObjectStore, ObjectIo},
};
use oasis7_formal_protocol::incremental_state::{
    MerkleNodeReader, StateDeltaV1, StateNamespaceV1, StatePageWriter, StateRootV1, StateWriteV1,
};
use oasis7_formal_protocol::incremental_value::ValueTreeError;
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
        let mut cancel_writer = CancelWriter {
            inner: &mut writer,
            cancelled: &self.cancelled,
        };
        cancel_writer
            .put_state_pages(&retained, &self.memory)
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

struct CancelWriter<'s, 'store> {
    inner: &'s mut ValuePageSession<'store>,
    cancelled: &'s AtomicBool,
}
impl oasis7_formal_protocol::incremental_value::ValueNodeWriter for CancelWriter<'_, '_> {
    fn put_node(
        &mut self,
        id: [u8; 32],
        bytes: oasis7_proto::memory_budget::ReservedValue<Vec<u8>>,
    ) -> Result<(), ValueTreeError> {
        if self.cancelled.load(Ordering::Acquire) {
            return Err(ValueTreeError::Cancelled);
        }
        self.inner.put_node(id, bytes)
    }
}
impl StatePageWriter for CancelWriter<'_, '_> {}

impl ProviderOriginalMaterialStore for ProviderOriginalMaterialOwnerV1 {
    fn owner_binding(&self) -> &ProviderMaterialOwnerBindingV1 {
        &self.owner
    }

    fn pending_protection_root(&self) -> Result<[u8; 32], String> {
        let inner = self.lock_inner()?;
        self.ensure_healthy(&inner)?;
        let _workspace = self
            .memory
            .reserve(
                3 * oasis7_formal_protocol::incremental_state::STATE_ROOT_BYTES,
                MemoryClass::Recovery,
            )
            .map_err(|error| error.to_string())?;
        pending_root_digest(&self.owner, &inner.head)
    }

    fn protected_roots_page(
        &self,
        cursor: Option<&oasis7::simulator::ProviderPendingRootsCursorV1>,
        limit: usize,
    ) -> Result<oasis7::simulator::ProviderPendingRootsPageV1, String> {
        if limit == 0 || limit > MAX_PROTECTED_ROOT_PAGE {
            return Err("provider protected-root page bound".into());
        }
        let inner = self.lock_inner()?;
        self.ensure_healthy(&inner)?;
        let _page_workspace = self
            .memory
            .reserve(64 * 1024, MemoryClass::Recovery)
            .map_err(|error| error.to_string())?;
        let root_digest = pending_root_digest(&self.owner, &inner.head)?;
        let cursor_state = match cursor {
            Some(cursor) => {
                let state = cursor
                    .owner_state::<ProtectedRootsCursorState>()
                    .ok_or("provider protected-root cursor owner/type")?;
                if state.pending_root != root_digest {
                    return Err("provider protected-root cursor root changed".into());
                }
                if state.after_key_digest.is_none()
                    || state.active_attempts_seen > inner.head.pending.leaf_count
                {
                    return Err("provider protected-root cursor state".into());
                }
                *state
            }
            None => ProtectedRootsCursorState {
                pending_root: root_digest,
                after_key_digest: None,
                active_attempts_seen: 0,
            },
        };

        // Admit every retained page carrier before collecting indexed leaves.
        // The Merkle path cursor avoids re-scanning prior leaves on each page.
        let _row_charge = self
            .memory
            .reserve(
                limit.saturating_mul(
                    std::mem::size_of::<(String, [u8; 32])>()
                        .saturating_add(MAX_OPERATION_ID_BYTES),
                ),
                MemoryClass::Recovery,
            )
            .map_err(|error| error.to_string())?;
        let object_charge = self
            .memory
            .reserve(
                limit
                    .saturating_mul(6)
                    .saturating_mul(std::mem::size_of::<[u8; 32]>()),
                MemoryClass::Recovery,
            )
            .map_err(|error| error.to_string())?;
        let attempt_charge = self
            .memory
            .reserve(
                limit.saturating_mul(
                    std::mem::size_of::<ProviderPendingAttemptSummaryV1>()
                        .saturating_add(MAX_OWNER_ID_BYTES)
                        .saturating_add(MAX_OPERATION_ID_BYTES),
                ),
                MemoryClass::Recovery,
            )
            .map_err(|error| error.to_string())?;
        let roots_charge = self
            .memory
            .reserve(
                3usize.saturating_mul(
                    std::mem::size_of::<oasis7_formal_protocol::incremental_state::StateRootV1>()
                        + 128,
                ),
                MemoryClass::Recovery,
            )
            .map_err(|error| error.to_string())?;
        let owner_charge = self
            .memory
            .reserve(
                std::mem::size_of::<ProviderMaterialOwnerBindingV1>() + MAX_OWNER_ID_BYTES,
                MemoryClass::Recovery,
            )
            .map_err(|error| error.to_string())?;
        let mut selected = Vec::<(String, [u8; 32])>::new();
        selected
            .try_reserve_exact(limit)
            .map_err(|_| "provider protected-root page allocation")?;
        if selected.capacity() > limit {
            return Err("provider protected-root page allocation exceeded admission".into());
        }
        let mut physical = finite_page_io();
        let session = self
            .pages
            .state_session(physical.clone(), MemoryClass::Recovery)
            .map_err(|error| error.to_string())?;
        let mut scan_budget = oasis7_formal_protocol::incremental_state::StateScanBudget {
            nodes_remaining: 100_000,
            read_bytes_remaining: 256 * 1024 * 1024,
            leaves_remaining: limit as u64,
        };
        let mut scan = oasis7_formal_protocol::incremental_state::StateScanContext {
            source: &session,
            memory: &self.memory,
            class: MemoryClass::Recovery,
            io: &mut scan_budget,
        };
        let prefix = b"oasis7.iws.provider-pending-operation-key.v1\0";
        let page = scan
            .visit_entries_page(
                &inner.head.pending,
                cursor_state.after_key_digest,
                limit,
                |entry| {
                    if !entry.key.starts_with(prefix) {
                        return Err("provider pending key domain".to_owned());
                    }
                    let operation = std::str::from_utf8(&entry.key[prefix.len()..])
                        .map_err(|_| "provider pending operation UTF-8".to_owned())?;
                    operation_key(operation).map_err(|error| error.to_string())?;
                    let mut operation_owned = String::new();
                    operation_owned
                        .try_reserve_exact(operation.len())
                        .map_err(|_| "provider protected-root operation allocation".to_owned())?;
                    if operation_owned.capacity() > MAX_OPERATION_ID_BYTES {
                        return Err(
                            "provider protected-root operation allocation exceeded admission"
                                .into(),
                        );
                    }
                    operation_owned.push_str(operation);
                    selected.push((operation_owned, entry.value));
                    Ok(())
                },
                || self.cancelled(),
            )
            .map_err(|error| error.to_string());
        drop(scan);
        if let Some(error) = session.take_read_error() {
            return Err(error.to_string());
        }
        let page = page?;
        if page.visited as usize != selected.len() {
            return Err("provider protected-root page count mismatch".into());
        }
        physical = session.remaining_io().map_err(|error| error.to_string())?;

        let active_attempts_seen = cursor_state
            .active_attempts_seen
            .checked_add(page.visited)
            .ok_or("provider pending page count overflow")?;
        if !page.has_more && active_attempts_seen != inner.head.pending.leaf_count {
            return Err("provider pending pages do not cover the root leaf count".into());
        }
        let next_cursor = if page.has_more {
            let next_after = page
                .last_key_digest
                .ok_or("provider pending page lacks continuation path")?;
            let cursor_charge = self
                .memory
                .reserve(
                    std::mem::size_of::<oasis7::simulator::ProviderPendingRootsCursorV1>()
                        + std::mem::size_of::<ProtectedRootsCursorState>(),
                    MemoryClass::Recovery,
                )
                .map_err(|error| error.to_string())?;
            let cursor = oasis7::simulator::ProviderPendingRootsCursorV1::from_owner_state(
                ProtectedRootsCursorState {
                    pending_root: root_digest,
                    after_key_digest: Some(next_after),
                    active_attempts_seen,
                },
            );
            Some(Arc::new(cursor_charge.retain(cursor)))
        } else {
            None
        };
        let mut object_ids = Vec::new();
        let max_object_ids = limit.saturating_mul(6);
        object_ids
            .try_reserve_exact(max_object_ids)
            .map_err(|_| "provider protected-root page allocation")?;
        if object_ids.capacity() > max_object_ids {
            return Err("provider protected-root page allocation exceeded admission".into());
        }
        let mut attempts = Vec::new();
        attempts
            .try_reserve_exact(limit)
            .map_err(|_| "provider protected-attempt page allocation")?;
        if attempts.capacity() > limit {
            return Err("provider protected-attempt page allocation exceeded admission".into());
        }
        for (operation_id, event_id) in selected {
            let record = self.read_event_object(&event_id, MemoryClass::Recovery, &mut physical)?;
            if !record.phase.is_pending()
                || record.operation_id != operation_id
                || record.sequence > inner.head.sequence
            {
                return Err("provider pending index/event identity mismatch".into());
            }
            let event_index = self.read_state_value(
                &inner.head.events,
                &event_key(record.sequence),
                MemoryClass::Recovery,
                &mut physical,
            )?;
            let seen_index = self.read_state_value(
                &inner.head.seen,
                &operation_key(&operation_id)?,
                MemoryClass::Recovery,
                &mut physical,
            )?;
            if event_index != Some(event_id) || seen_index != Some(event_id) {
                return Err("provider pending event is not linked by owner roots".into());
            }
            let attempt = self.attempt_from_event(&record);
            let phase = match record.phase {
                EventPhase::RequestPrepared => ProviderPendingAttemptPhaseV1::RequestPrepared,
                EventPhase::DispatchedOrUnknown => {
                    ProviderPendingAttemptPhaseV1::DispatchedOrUnknown
                }
                EventPhase::ResponseBodyPlanned => {
                    ProviderPendingAttemptPhaseV1::ResponseBodyPlanned
                }
                EventPhase::ResponseBodyDurable => {
                    ProviderPendingAttemptPhaseV1::ResponseBodyDurable
                }
                EventPhase::ResponseContextPlanned => {
                    ProviderPendingAttemptPhaseV1::ResponseContextPlanned
                }
                EventPhase::ResponseCaptured => ProviderPendingAttemptPhaseV1::ResponseCaptured,
                EventPhase::RejectionEvidencePlanned => {
                    ProviderPendingAttemptPhaseV1::RejectionEvidencePlanned
                }
                EventPhase::Committed | EventPhase::Rejected => {
                    return Err("provider pending index references terminal event".into());
                }
            };
            attempts.push(ProviderPendingAttemptSummaryV1 {
                attempt,
                phase,
                response_object_id: record.response_object_id,
                response_context_object_id: record.response_context_object_id,
                response_status: record.response_status,
                terminal_evidence_object_id: record.terminal_evidence_object_id,
            });
            // The event record itself holds the durable phase and references
            // for this pending attempt, so it must remain protected alongside
            // the request/response objects it names.
            object_ids.push(event_id);
            object_ids.push(record.request_object_id);
            object_ids.push(record.request_context_object_id);
            if let Some(response) = record.response_object_id {
                object_ids.push(response);
            }
            if let Some(context) = record.response_context_object_id {
                object_ids.push(context);
            }
            if let Some(evidence) = record.terminal_evidence_object_id {
                object_ids.push(evidence);
            }
        }
        object_ids.sort_unstable();
        object_ids.dedup();
        Ok(oasis7::simulator::ProviderPendingRootsPageV1 {
            owner: Arc::new(owner_charge.retain(self.owner.clone())),
            pending_root: root_digest,
            object_ids: Arc::new(object_charge.retain(object_ids)),
            attempts: Arc::new(attempt_charge.retain(attempts)),
            state_roots: Arc::new(roots_charge.retain(ProviderPendingStateRootsV1 {
                events: inner.head.events.clone(),
                pending: inner.head.pending.clone(),
                seen: inner.head.seen.clone(),
            })),
            next_cursor,
        })
    }

    fn validate_request_owner(
        &self,
        request: &ContinuousAgentRequestContextV1,
    ) -> Result<(), String> {
        self.authority.verify_owner_scope(&self.owner)?;
        if request.runtime_binding.world_id != self.owner.world_id {
            return Err("provider request world differs from pending owner".into());
        }
        self.authority.request_binding(&self.owner, request)?;
        Ok(())
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
            .map(|reservation| Box::new(SpoolReservation(reservation)) as _)
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
        self.require_writable()?;
        if length == 0 || length > MAX_REQUEST_BYTES {
            return Err("provider request body bound".into());
        }
        let binding = self.authority.request_binding(&self.owner, request)?;
        let operation_id = request.provider_invocation_key().to_string();
        operation_key(&operation_id)?;
        let mut inner = self.lock_inner()?;
        self.ensure_healthy(&inner)?;
        let mut physical = finite_page_io();
        if self
            .read_state_value(
                &inner.head.seen,
                &operation_key(&operation_id)?,
                MemoryClass::Ordinary,
                &mut physical,
            )?
            .is_some()
        {
            return Err("provider operation identity was already consumed or dispatched".into());
        }
        let context = self.json_bytes(request, MAX_CONTEXT_BYTES)?;
        let context_id = *blake3::hash(&context.bytes).as_bytes();
        let attempt_id = make_attempt_id(
            &self.owner,
            &operation_id,
            request.transport_attempt,
            binding,
            expected_body_id,
            context_id,
        );
        let mut logical = finite_value_io();
        let objects = IncrementalObjectStore::open(
            &self.pages,
            MemoryClass::Ordinary,
            &mut physical,
            &self.cancelled,
        )
        .map_err(|error| error.to_string())?;
        let actual_body_id = objects
            .write_object(
                &mut original_body,
                length,
                Some(expected_body_id),
                &mut ObjectIo {
                    physical: &mut physical,
                    logical: &mut logical,
                    class: MemoryClass::Ordinary,
                    cancelled: &self.cancelled,
                },
            )
            .map_err(|error| error.to_string())?;
        if actual_body_id != expected_body_id {
            return Err("provider request original body ID mismatch".into());
        }
        self.persist_object(&context.bytes, context_id)?;
        let record = EventRecord {
            phase: EventPhase::RequestPrepared,
            sequence: 0,
            previous_event_id: [0; 32],
            operation_id: operation_id.clone(),
            transport_attempt: request.transport_attempt,
            attempt_id,
            parent_root_set: binding.parent_root_set,
            authority_snapshot: binding.authority_snapshot,
            request_object_id: actual_body_id,
            request_context_object_id: context_id,
            response_object_id: None,
            response_context_object_id: None,
            response_status: None,
            terminal_receipt_id: None,
            terminal_action_id: None,
            terminal_evidence_object_id: None,
            terminal_disposition_digest: None,
        };
        if let Err(error) = self.append_event(&mut inner, record) {
            inner.poisoned = true;
            return Err(error);
        }
        Ok(ProviderMaterialAttemptV1 {
            owner: self.owner.clone(),
            operation_id,
            transport_attempt: request.transport_attempt,
            attempt_id,
            parent_root_set: binding.parent_root_set,
            authority_snapshot: binding.authority_snapshot,
            request_object_id: actual_body_id,
            request_context_object_id: context_id,
        })
    }

    fn mark_dispatched(&self, attempt: &ProviderMaterialAttemptV1) -> Result<(), String> {
        self.require_writable()?;
        let mut inner = self.lock_inner()?;
        self.ensure_healthy(&inner)?;
        let mut physical = finite_page_io();
        let record = self.latest_for_attempt(&inner, attempt, &mut physical)?;
        if record.phase != EventPhase::RequestPrepared {
            return Err("provider attempt is not prepared for its first dispatch".into());
        }
        let mut next = record;
        next.phase = EventPhase::DispatchedOrUnknown;
        if let Err(error) = self.append_event(&mut inner, next) {
            inner.poisoned = true;
            return Err(error);
        }
        Ok(())
    }

    fn persist_response_body(
        &self,
        attempt: &ProviderMaterialAttemptV1,
        status: u16,
        expected_body_id: [u8; 32],
        original_body: &mut dyn Read,
        length: u64,
    ) -> Result<[u8; 32], String> {
        self.require_writable()?;
        if length > MAX_RESPONSE_BYTES {
            return Err("provider response body bound".into());
        }
        let mut inner = self.lock_inner()?;
        self.ensure_healthy(&inner)?;
        let mut physical = finite_page_io();
        let mut record = self.latest_for_attempt(&inner, attempt, &mut physical)?;
        if record.phase != EventPhase::DispatchedOrUnknown {
            return Err("provider response has no durable dispatch marker".into());
        }
        record.phase = EventPhase::ResponseBodyPlanned;
        record.response_object_id = Some(expected_body_id);
        record.response_status = Some(status);
        if let Err(error) = self.append_event(&mut inner, record.clone()) {
            inner.poisoned = true;
            return Err(error);
        }
        let mut logical = finite_value_io();
        let objects = IncrementalObjectStore::open(
            &self.pages,
            MemoryClass::Ordinary,
            &mut physical,
            &self.cancelled,
        )
        .map_err(|error| {
            inner.poisoned = true;
            error.to_string()
        })?;
        let actual = objects
            .write_object(
                &mut original_body,
                length,
                Some(expected_body_id),
                &mut ObjectIo {
                    physical: &mut physical,
                    logical: &mut logical,
                    class: MemoryClass::Ordinary,
                    cancelled: &self.cancelled,
                },
            )
            .map_err(|error| {
                inner.poisoned = true;
                error.to_string()
            })?;
        if actual != expected_body_id {
            inner.poisoned = true;
            return Err("provider response original body ID mismatch".into());
        }
        record.phase = EventPhase::ResponseBodyDurable;
        if let Err(error) = self.append_event(&mut inner, record) {
            inner.poisoned = true;
            return Err(error);
        }
        Ok(actual)
    }

    fn finish_response(
        &self,
        attempt: &ProviderMaterialAttemptV1,
        status: u16,
        response_object_id: [u8; 32],
        response: &ContinuousAgentResponseContextV1,
    ) -> Result<ProviderOriginalMaterialRefsV1, String> {
        self.require_writable()?;
        let mut inner = self.lock_inner()?;
        self.ensure_healthy(&inner)?;
        let mut physical = finite_page_io();
        let mut record = self.latest_for_attempt(&inner, attempt, &mut physical)?;
        if record.phase != EventPhase::ResponseBodyDurable
            || record.response_object_id != Some(response_object_id)
            || record.response_status != Some(status)
        {
            return Err("provider typed response does not match durable original body".into());
        }
        let context = self.json_bytes(response, MAX_CONTEXT_BYTES)?;
        let context_id = *blake3::hash(&context.bytes).as_bytes();
        record.phase = EventPhase::ResponseContextPlanned;
        record.response_context_object_id = Some(context_id);
        if let Err(error) = self.append_event(&mut inner, record.clone()) {
            inner.poisoned = true;
            return Err(error);
        }
        if let Err(error) = self.persist_object(&context.bytes, context_id) {
            inner.poisoned = true;
            return Err(error);
        }
        record.phase = EventPhase::ResponseCaptured;
        if let Err(error) = self.append_event(&mut inner, record.clone()) {
            inner.poisoned = true;
            return Err(error);
        }
        self.refs_from_event(&record)
    }

    fn verify_commit_materials(
        &self,
        materials: &ProviderOriginalMaterialRefsV1,
        request: &ContinuousAgentRequestContextV1,
        response: &ContinuousAgentResponseContextV1,
    ) -> Result<(), String> {
        let inner = self.lock_inner()?;
        self.ensure_healthy(&inner)?;
        let mut physical = finite_page_io();
        let (_, record) = self
            .latest_event(
                &inner.head,
                &materials.operation_id,
                &inner.head.seen,
                MemoryClass::Ordinary,
                &mut physical,
            )?
            .ok_or("provider committed materials missing from seen index")?;
        if record.phase != EventPhase::ResponseCaptured
            || self.refs_from_event(&record)? != *materials
        {
            return Err(
                "provider committed material references are not the captured attempt".into(),
            );
        }
        let request_context = self.read_object_bytes(
            &record.request_context_object_id,
            MAX_CONTEXT_BYTES,
            MemoryClass::Ordinary,
        )?;
        let stored_request = self
            .memory
            .reserve(MAX_CONTEXT_BYTES, MemoryClass::Ordinary)
            .map_err(|error| error.to_string())?
            .retain(
                serde_json::from_slice::<ContinuousAgentRequestContextV1>(
                    request_context.as_slice(),
                )
                .map_err(|error| error.to_string())?,
            );
        if *stored_request != *request
            || (*stored_request).provider_invocation_key() != request.provider_invocation_key()
        {
            return Err("provider request typed context differs from durable original".into());
        }
        let request_bytes = self.json_bytes(request, MAX_CONTEXT_BYTES)?;
        if *blake3::hash(&request_bytes.bytes).as_bytes() != record.request_object_id {
            return Err("provider request context is not the original HTTP bytes".into());
        }
        {
            let original_request_body = self.read_object_bytes(
                &record.request_object_id,
                usize::try_from(MAX_REQUEST_BYTES).map_err(|_| "provider request bound")?,
                MemoryClass::Ordinary,
            )?;
            if original_request_body.as_slice() != request_bytes.bytes.as_slice() {
                return Err("provider original request body differs from its bound context".into());
            }
        }
        let response_context = self.read_object_bytes(
            &record
                .response_context_object_id
                .ok_or("provider response context absent")?,
            MAX_CONTEXT_BYTES,
            MemoryClass::Ordinary,
        )?;
        let stored_response = self
            .memory
            .reserve(MAX_CONTEXT_BYTES, MemoryClass::Ordinary)
            .map_err(|error| error.to_string())?
            .retain(
                serde_json::from_slice::<ContinuousAgentResponseContextV1>(
                    response_context.as_slice(),
                )
                .map_err(|error| error.to_string())?,
            );
        if *stored_response != *response {
            return Err("provider response typed context differs from durable original".into());
        }
        let response_body = self.read_object_bytes(
            &record
                .response_object_id
                .ok_or("provider response body absent")?,
            usize::try_from(MAX_RESPONSE_BYTES).map_err(|_| "provider response bound")?,
            MemoryClass::Ordinary,
        )?;
        let decoded = self
            .memory
            .reserve(MAX_CONTEXT_BYTES, MemoryClass::Ordinary)
            .map_err(|error| error.to_string())?
            .retain(
                serde_json::from_slice::<ContinuousAgentResponseContextV1>(
                    response_body.as_slice(),
                )
                .map_err(|error| error.to_string())?,
            );
        if *decoded != *response {
            return Err("provider typed response is not the original HTTP body".into());
        }
        Ok(())
    }

    fn mark_committed(
        &self,
        materials: &ProviderOriginalMaterialRefsV1,
        receipt_id: &str,
        action_id: u64,
    ) -> Result<(), String> {
        self.require_writable()?;
        self.authority
            .verify_commit(&self.owner, materials, receipt_id, action_id)?;
        let mut inner = self.lock_inner()?;
        self.ensure_healthy(&inner)?;
        let mut physical = finite_page_io();
        let record = self.latest_for_attempt(
            &inner,
            &self.attempt_from_materials(materials),
            &mut physical,
        )?;
        if record.phase == EventPhase::Committed
            && record.terminal_receipt_id.as_deref() == Some(receipt_id)
            && record.terminal_action_id == Some(action_id)
        {
            return Ok(());
        }
        if record.phase != EventPhase::ResponseCaptured
            || self.refs_from_event(&record)? != *materials
            || receipt_id.is_empty()
            || receipt_id.len() > 256
        {
            return Err("provider commit does not match unresolved captured materials".into());
        }
        let mut terminal = record;
        terminal.phase = EventPhase::Committed;
        terminal.terminal_receipt_id = Some(receipt_id.to_owned());
        terminal.terminal_action_id = Some(action_id);
        if let Err(error) = self.append_event(&mut inner, terminal) {
            inner.poisoned = true;
            return Err(error);
        }
        Ok(())
    }

    fn mark_rejected(
        &self,
        attempt: &ProviderMaterialAttemptV1,
        evidence: ProviderDispositionEvidenceV1,
    ) -> Result<(), String> {
        self.require_writable()?;
        self.authority
            .verify_rejection(&self.owner, attempt, evidence)?;
        let mut inner = self.lock_inner()?;
        self.ensure_healthy(&inner)?;
        let mut physical = finite_page_io();
        let mut record = self.latest_for_attempt(&inner, attempt, &mut physical)?;
        if record.phase == EventPhase::Rejected
            && record.terminal_evidence_object_id == Some(evidence.evidence_object_id)
            && record.terminal_disposition_digest == Some(evidence.disposition_digest)
        {
            return Ok(());
        }
        if !record.phase.is_pending() {
            return Err("provider rejection cannot alter a terminal attempt".into());
        }
        // Verify the evidence content is present in this same original store
        // before the pending index is allowed to stop protecting it.
        let _evidence = self.read_object_bytes(
            &evidence.evidence_object_id,
            MAX_CONTEXT_BYTES,
            MemoryClass::Ordinary,
        )?;
        record.phase = EventPhase::RejectionEvidencePlanned;
        record.terminal_evidence_object_id = Some(evidence.evidence_object_id);
        record.terminal_disposition_digest = Some(evidence.disposition_digest);
        if let Err(error) = self.append_event(&mut inner, record.clone()) {
            inner.poisoned = true;
            return Err(error);
        }
        record.phase = EventPhase::Rejected;
        if let Err(error) = self.append_event(&mut inner, record) {
            inner.poisoned = true;
            return Err(error);
        }
        Ok(())
    }
}

impl ProviderOriginalMaterialOwnerV1 {
    fn attempt_from_materials(
        &self,
        materials: &ProviderOriginalMaterialRefsV1,
    ) -> ProviderMaterialAttemptV1 {
        ProviderMaterialAttemptV1 {
            owner: materials.owner.clone(),
            operation_id: materials.operation_id.clone(),
            transport_attempt: materials.transport_attempt,
            attempt_id: materials.attempt_id,
            parent_root_set: materials.parent_root_set,
            authority_snapshot: materials.authority_snapshot,
            request_object_id: materials.request_object_id,
            request_context_object_id: materials.request_context_object_id,
        }
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

struct SpoolReservation(MemoryReservation);
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
