use super::*;
use oasis7_formal_protocol::{
    incremental_execution::RootSetV1,
    incremental_state::{StateEntryV1, StateScanContext, StateScanError},
};
use std::cell::RefCell;
#[derive(Debug)]
pub struct DiplomacyFamilyQualificationV1 {
    pub(super) roots: [u8; 32],
    pub(super) counts: [u64; 4],
}
impl DiplomacyFamilyQualificationV1 {
    pub fn validate_binding(&self, roots: &[u8; 32]) -> Result<(), ValueTreeError> {
        if self.roots != *roots {
            Err(closure::invalid())
        } else {
            Ok(())
        }
    }
    pub fn record_counts(&self) -> [u64; 4] {
        self.counts
    }
}
type Outer = (DiplomacyFieldV1, [u8; 32], ReservedValue<StateRootV1>);
pub(super) fn outer<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    entry: &StateEntryV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
) -> Result<Option<Outer>, ValueTreeError> {
    if !entry.key.starts_with(b"oasis7.iws.world-field-key.v1\0") {
        return Ok(None);
    }
    let binding = WorldFieldKeyV1::from_bytes(&entry.key).map_err(ValueTreeError::Invalid)?;
    let Some(field) = DiplomacyFieldV1::from_name(binding.name) else {
        return Ok(None);
    };
    let key = WorldFieldKeyV1::new(field.descriptor()?, binding.kind, binding.semantic_key)
        .map_err(ValueTreeError::Invalid)?;
    if key.as_bytes() != entry.key || entry.namespace != StateNamespaceV1::Business {
        return Err(closure::invalid());
    }
    if binding.kind == WorldFieldRecordKindV1::Metadata {
        ctx.records().presence(&entry.value, &key, 0)?;
        return Ok(None);
    }
    if binding.kind != WorldFieldRecordKindV1::Entry {
        return Err(closure::invalid());
    }
    let info = crate::incremental_field_member::read_member_record_header_v1(
        &entry.value,
        &key,
        &mut ctx.values,
        MemberRecordLimitsV1 {
            semantic_bytes: ctx.semantic_bytes,
            payload_bytes: RECORD_ROOT_BYTES as u64,
            record_bytes: ctx.record_bytes,
        },
        &mut ctx.cancelled,
    )?;
    let root = ctx.values.load_root(&entry.value)?;
    let _scratch = ctx.values.memory.reserve(4096, ctx.values.class)?;
    let domain = crate::incremental_semantic_key::SEMANTIC_KEY_DOMAIN;
    let mut prefix = [0; 128];
    let n = domain.len() + 9;
    if info.semantic_bytes < n as u64 {
        return Err(closure::invalid());
    }
    ctx.values.read_range(
        &root,
        (FIELD_MEMBER_HEADER_BYTES + key.as_bytes().len()) as u64,
        n as u64,
        &mut Cursor::new(&mut prefix[..n]),
        &mut ctx.cancelled,
    )?;
    if &prefix[..domain.len()] != domain || prefix[domain.len()] != 1 {
        return Err(closure::invalid());
    }
    let length = u64::from_be_bytes(prefix[domain.len() + 1..n].try_into().unwrap());
    if length.checked_add(n as u64) != Some(info.semantic_bytes) {
        return Err(closure::invalid());
    }
    let mut sink = std::io::sink();
    let mut utf8 =
        crate::incremental_utf8_output::Utf8Output::new(&mut sink, "diplomacy outer UTF-8");
    ctx.values.read_range(
        &root,
        (FIELD_MEMBER_HEADER_BYTES + key.as_bytes().len() + n) as u64,
        length,
        &mut utf8,
        &mut ctx.cancelled,
    )?;
    if !utf8.is_complete() {
        return Err(closure::invalid());
    }
    let digest = binding.semantic_key.ok_or_else(closure::invalid)?;
    let child = root::decode(
        field,
        &(key, info.semantic_bytes),
        entry.value,
        &mut ctx.values,
        ctx.record_bytes,
        &mut ctx.cancelled,
    )?;
    ctx.check()?;
    Ok(Some((field, digest, child)))
}
pub fn qualify_diplomacy_family_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    roots: &RootSetV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    outer_scan: &mut StateScanBudget,
    inner_scan: &mut StateScanBudget,
    cells: &mut u64,
    max: u64,
) -> Result<ReservedValue<DiplomacyFamilyQualificationV1>, ValueTreeError> {
    roots.validate().map_err(|_| closure::invalid())?;
    ctx.check()?;
    let business = roots
        .state
        .iter()
        .find(|r| r.namespace == StateNamespaceV1::Business)
        .ok_or_else(closure::invalid)?;
    if ctx.root != business {
        return Err(closure::invalid());
    }
    for field in DiplomacyFieldV1::ALL {
        let key = WorldFieldKeyV1::new(field.descriptor()?, WorldFieldRecordKindV1::Metadata, None)
            .map_err(ValueTreeError::Invalid)?;
        let value = ctx.lookup(&key)?.ok_or_else(closure::invalid)?;
        ctx.records().presence(&value, &key, 0)?;
    }
    let retained = ctx.values.memory.reserve(512, ctx.values.class)?;
    let cancel = RefCell::new(&mut ctx.cancelled);
    let mut counts = [0u64; 4];
    for state_root in &roots.state {
        let mut local = WorldFieldReadContextV1 {
            root: state_root,
            state: ctx.state,
            values: oasis7_formal_protocol::incremental_value::ValueReadContext {
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
        let result = StateScanContext {
            source: ctx.state,
            memory: local.values.memory,
            class: local.values.class,
            io: &mut *outer_scan,
        }
        .visit_entries(
            state_root,
            |entry| {
                *cells = cells
                    .checked_sub(1)
                    .ok_or(ValueTreeError::ResourceLimited("diplomacy family cells"))?;
                if let Some((field, digest, child)) = outer(entry, &mut local)? {
                    let mut body = WorldFieldReadContextV1 {
                        root: &child,
                        state: local.state,
                        values: oasis7_formal_protocol::incremental_value::ValueReadContext {
                            source: local.values.source,
                            memory: local.values.memory,
                            class: local.values.class,
                            io: &mut *local.values.io,
                        },
                        key_work: &mut *local.key_work,
                        semantic_bytes: local.semantic_bytes,
                        record_bytes: local.record_bytes,
                        take_state_error: &mut local.take_state_error,
                        cancelled: &mut local.cancelled,
                    };
                    qualify_diplomacy_body_v1(field, digest, &mut body, inner_scan, cells, max)?;
                    let i = field as usize;
                    counts[i] = counts[i].checked_add(1).ok_or_else(closure::invalid)?;
                }
                Ok(())
            },
            || (cancel.borrow_mut())(),
        );
        let error = (local.take_state_error)();
        if (cancel.borrow_mut())() {
            return Err(ValueTreeError::Cancelled);
        }
        if let Some(error) = error {
            return Err(error);
        }
        match result {
            Ok(_) => {}
            Err(StateScanError::Visitor(e)) => return Err(e),
            Err(StateScanError::Memory(e)) => return Err(e.into()),
            Err(StateScanError::Cancelled) => return Err(ValueTreeError::Cancelled),
            Err(StateScanError::ResourceLimited) => {
                return Err(ValueTreeError::ResourceLimited("diplomacy family scan"));
            }
            Err(_) => return Err(closure::invalid()),
        }
    }
    let digest = roots.digest().map_err(|_| closure::invalid())?;
    if (cancel.borrow_mut())() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(retained.retain(DiplomacyFamilyQualificationV1 {
        roots: digest,
        counts,
    }))
}
