//! Complete Runtime proposal map closure with exact RootSet identity.
use super::*;
use crate::{
    incremental_field_read::WorldFieldReadContextV1,
    incremental_json_records::JsonReadWorkV1,
    incremental_semantic_key::{SemanticKeyRefV1, read_semantic_key_v1},
};
use oasis7_formal_protocol::{
    incremental_execution::RootSetV1,
    incremental_record_root::{RECORD_ROOT_BYTES, RecordRootRefV1},
    incremental_state::{MerkleNodeReader, StateScanBudget, StateScanContext, StateScanError},
    incremental_value::{ValueNodeReader, ValueReadContext},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
use std::cell::RefCell;
pub struct RuntimeProposalsQualificationV1 {
    roots: [u8; 32],
    count: u64,
    class: MemoryClass,
}
impl RuntimeProposalsQualificationV1 {
    pub fn proposals(&self) -> u64 {
        self.count
    }
    pub fn validate_binding(&self, roots: &RootSetV1) -> Result<(), ValueTreeError> {
        if self.roots != roots.digest().map_err(|_| invalid())? {
            return Err(invalid());
        }
        Ok(())
    }
}
pub struct RuntimeProposalReadWorkV1 {
    pub cells_remaining: u64,
    pub json: JsonReadWorkV1,
    pub max_selected_heap: usize,
    pub max_text_bytes: u64,
}
pub(crate) fn outer_child<M, R, E, C>(
    key: &WorldFieldKeyV1,
    value: &[u8; 32],
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
) -> Result<(u64, StateRootV1), ValueTreeError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    ctx.check()?;
    ctx.values
        .verify_closure(value, ctx.record_bytes, &mut ctx.cancelled)?;
    let mut semantic = [0u8; 64];
    let mut bytes = [0u8; RECORD_ROOT_BYTES];
    let info = read_member_record_v1(
        value,
        key,
        &mut ctx.values,
        MemberRecordLimitsV1 {
            semantic_bytes: ctx.semantic_bytes.min(64),
            payload_bytes: RECORD_ROOT_BYTES as u64,
            record_bytes: ctx.record_bytes,
        },
        &mut std::io::Cursor::new(semantic.as_mut_slice()),
        &mut &mut bytes[..],
        &mut ctx.cancelled,
    )?;
    if info.codec != MemberPayloadCodecV1::RecordRoot
        || info.payload_bytes != RECORD_ROOT_BYTES as u64
    {
        return Err(invalid());
    }
    let len = usize::try_from(info.semantic_bytes).map_err(|_| invalid())?;
    let SemanticKeyRefV1::Unsigned(original) =
        read_semantic_key_v1(semantic.get(..len).ok_or_else(invalid)?)
            .map_err(ValueTreeError::Invalid)?
    else {
        return Err(invalid());
    };
    let parsed = WorldFieldKeyV1::from_bytes(key.as_bytes()).map_err(ValueTreeError::Invalid)?;
    let reference = RecordRootRefV1::from_canonical_bytes(&bytes).map_err(|_| invalid())?;
    let root = reference
        .validate_binding(
            &schema(),
            &parsed.semantic_key.ok_or_else(invalid)?,
            StateNamespaceV1::Authorization,
        )
        .map_err(|_| invalid())?
        .clone();
    ctx.check()?;
    Ok((original, root))
}
pub fn qualify_runtime_proposals_v1<M, R, E, C>(
    roots: &RootSetV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    scan: &mut StateScanBudget,
    work: &mut RuntimeProposalReadWorkV1,
) -> Result<ReservedValue<RuntimeProposalsQualificationV1>, ValueTreeError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    roots.validate().map_err(|_| invalid())?;
    ctx.check()?;
    if !roots.state.iter().any(|root| root == ctx.root) {
        return Err(invalid());
    }
    let retained = ctx.values.memory.reserve(512, ctx.values.class)?;
    let _workspace = ctx.values.memory.reserve(
        64 * std::mem::size_of::<StateRootV1>() + (32 << 10),
        ctx.values.class,
    )?;
    let mut pending = Vec::with_capacity(64);
    if pending.capacity() * std::mem::size_of::<StateRootV1>() > _workspace.bytes() {
        return Err(invalid());
    }
    let cancel = RefCell::new(&mut ctx.cancelled);
    let mut metadata = false;
    let mut count = 0u64;
    for root in &roots.state {
        let mut local = WorldFieldReadContextV1 {
            root,
            state: ctx.state,
            values: ValueReadContext {
                source: ctx.values.source,
                memory: ctx.values.memory,
                class: ctx.values.class,
                io: &mut *ctx.values.io,
            },
            key_work: &mut *ctx.key_work,
            semantic_bytes: ctx.semantic_bytes,
            record_bytes: ctx.record_bytes,
            take_state_error: &mut ctx.take_state_error,
            cancelled: || (cancel.borrow_mut())(),
        };
        let mut cursor = None;
        let mut visited = 0u64;
        loop {
            pending.clear();
            let result = StateScanContext {
                source: ctx.state,
                memory: local.values.memory,
                class: local.values.class,
                io: &mut *scan,
            }
            .visit_entries_page(
                root,
                cursor,
                64,
                |entry| {
                    if !entry.key.starts_with(b"oasis7.iws.world-field-key.v1\0") {
                        return Ok(());
                    }
                    let parsed =
                        WorldFieldKeyV1::from_bytes(&entry.key).map_err(ValueTreeError::Invalid)?;
                    if parsed.owner != FieldOwner::Runtime || parsed.name != "proposals" {
                        return Ok(());
                    }
                    if parsed.namespace != StateNamespaceV1::Authorization
                        || root.namespace != parsed.namespace
                        || entry.namespace != parsed.namespace
                    {
                        return Err(invalid());
                    }
                    let key = WorldFieldKeyV1::new(
                        runtime_proposal_descriptor_v1(),
                        parsed.kind,
                        parsed.semantic_key,
                    )
                    .map_err(ValueTreeError::Invalid)?;
                    if key.as_bytes() != entry.key {
                        return Err(invalid());
                    }
                    match parsed.kind {
                        WorldFieldRecordKindV1::Metadata => {
                            if metadata || parsed.semantic_key.is_some() {
                                return Err(invalid());
                            }
                            local.records().presence(&entry.value, &key, 0)?;
                            metadata = true;
                        }
                        WorldFieldRecordKindV1::Entry => {
                            let (_, child) = outer_child(&key, &entry.value, &mut local)?;
                            if pending.len() == pending.capacity() {
                                return Err(invalid());
                            }
                            pending.push(child);
                            count = count.checked_add(1).ok_or_else(invalid)?;
                        }
                        _ => return Err(invalid()),
                    }
                    Ok(())
                },
                || (cancel.borrow_mut())(),
            );
            let original = (local.take_state_error)();
            if (cancel.borrow_mut())() {
                return Err(ValueTreeError::Cancelled);
            }
            if let Some(error) = original {
                return Err(error);
            }
            let page = match result {
                Ok(v) => v,
                Err(StateScanError::Visitor(e)) => return Err(e),
                Err(StateScanError::Memory(e)) => return Err(e.into()),
                Err(StateScanError::Cancelled) => return Err(ValueTreeError::Cancelled),
                Err(StateScanError::ResourceLimited) => {
                    return Err(ValueTreeError::ResourceLimited("proposal family scan"));
                }
                Err(_) => return Err(invalid()),
            };
            visited = visited.checked_add(page.visited).ok_or_else(invalid)?;
            for child in pending.drain(..) {
                stream::qualify(&child, &mut local, scan, work)?;
            }
            if !page.has_more {
                break;
            }
            if page.visited == 0 || page.last_key_digest == cursor {
                return Err(invalid());
            }
            cursor = page.last_key_digest;
        }
        if visited != root.leaf_count {
            return Err(invalid());
        }
    }
    if !metadata {
        return Err(invalid());
    }
    if (cancel.borrow_mut())() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(retained.retain(RuntimeProposalsQualificationV1 {
        roots: roots.digest().map_err(|_| invalid())?,
        count,
        class: ctx.values.class,
    }))
}
pub fn readmit_runtime_proposals_ordinary_v1(
    source: &ReservedValue<RuntimeProposalsQualificationV1>,
    roots: &RootSetV1,
    memory: &MemoryBudget,
    mut cancelled: impl FnMut() -> bool,
) -> Result<ReservedValue<RuntimeProposalsQualificationV1>, ValueTreeError> {
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    if !source.uses_budget(memory)
        || source.reserved_bytes() < 512
        || source.class != MemoryClass::Recovery
    {
        return Err(invalid());
    }
    let mut scratch = memory.reserve(128 << 10, MemoryClass::Ordinary)?;
    source.validate_binding(roots)?;
    let result = scratch.split(512).map_err(ValueTreeError::Invalid)?.retain(
        RuntimeProposalsQualificationV1 {
            roots: source.roots,
            count: source.count,
            class: MemoryClass::Ordinary,
        },
    );
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(result)
}

#[path = "incremental_runtime_proposal_status.rs"]
mod status;
pub use status::{
    RuntimeProposalBodyQualificationV1, RuntimeProposalStatusChangeV1,
    change_runtime_proposal_status_v1, read_runtime_proposal_body_qualification_v1,
};
#[path = "incremental_runtime_proposal_delta.rs"]
mod delta;
#[path = "incremental_runtime_proposal_receiver.rs"]
mod receiver;
pub use receiver::accept_runtime_proposals_transition_v1;
pub use status::create_runtime_proposal_overlay_v1;

#[path = "incremental_runtime_proposal_status_read.rs"]
mod header;

#[path = "incremental_runtime_proposal_stream.rs"]
mod stream;

pub(crate) use receiver::accept_runtime_proposals_verified_v1;
