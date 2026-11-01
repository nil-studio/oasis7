use super::*;
use crate::incremental_field_read::WorldFieldReadContextV1;
use oasis7_formal_protocol::{
    incremental_execution::RootSetV1,
    incremental_state::{
        MerkleNodeReader, StateEntryV1, StateRootV1, StateScanBudget, StateScanContext,
        StateScanError,
    },
    incremental_value::{ValueNodeReader, ValueReadContext},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
use std::cell::RefCell;
#[derive(Debug)]
pub struct RewardFamilyQualificationV1 {
    roots: [u8; 32],
    counts: [u64; 2],
    mint_length: u64,
}
impl RewardFamilyQualificationV1 {
    pub fn validate_binding(&self, roots: &[u8; 32]) -> Result<(), ValueTreeError> {
        if self.roots != *roots {
            Err(read::invalid())
        } else {
            Ok(())
        }
    }
    pub fn record_counts(&self) -> [u64; 2] {
        self.counts
    }
    pub fn mint_length(&self) -> u64 {
        self.mint_length
    }
}
pub(super) fn mint(
    roots: [u8; 32],
    counts: [u64; 2],
    mint_length: u64,
    memory: &MemoryBudget,
    class: MemoryClass,
) -> Result<ReservedValue<RewardFamilyQualificationV1>, ValueTreeError> {
    Ok(memory
        .reserve(512, class)?
        .retain(RewardFamilyQualificationV1 {
            roots,
            counts,
            mint_length,
        }))
}
pub(super) enum Outer {
    Metadata(RewardRecordKindV1),
    Format(RewardRecordKindV1),
    Length(u64),
    Record(RewardRecordKeyV1, StateRootV1),
}
pub(super) fn index(kind: RewardRecordKindV1) -> usize {
    match kind {
        RewardRecordKindV1::Pool => 0,
        RewardRecordKindV1::Mint => 1,
    }
}
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
    let Some(kind) = RewardRecordKindV1::ALL
        .into_iter()
        .find(|kind| kind.name() == binding.name)
    else {
        return Ok(None);
    };
    let key = WorldFieldKeyV1::new(kind.descriptor()?, binding.kind, binding.semantic_key)
        .map_err(ValueTreeError::Invalid)?;
    if key.as_bytes() != entry.key || entry.namespace != kind.namespace() {
        return Err(read::invalid());
    }
    let mut records = ctx.records();
    records
        .values
        .verify_closure(&entry.value, records.record_bytes, &mut *records.cancelled)?;
    if binding.kind == WorldFieldRecordKindV1::Metadata {
        records.presence(&entry.value, &key, 0)?;
        return Ok(Some(Outer::Metadata(kind)));
    }
    if binding.kind != WorldFieldRecordKindV1::Entry {
        return Err(read::invalid());
    }
    let format = read::format_key(kind, &mut records)?;
    if key == format.0 {
        read::primitive(
            MemberPayloadCodecV1::Unit,
            &entry.value,
            &format,
            &mut records,
            0,
            &mut std::io::sink(),
        )?;
        return Ok(Some(Outer::Format(kind)));
    }
    if kind == RewardRecordKindV1::Mint {
        let length = read::length_key(&mut records)?;
        if key == length.0 {
            let RewardValueV1::Unsigned(n) = read::primitive(
                MemberPayloadCodecV1::U64,
                &entry.value,
                &length,
                &mut records,
                8,
                &mut std::io::sink(),
            )?
            else {
                return Err(read::invalid());
            };
            return Ok(Some(Outer::Length(n)));
        }
    }
    let (index, child) = read::outer(kind, &key, &entry.value, &mut records)?;
    ctx.check()?;
    Ok(Some(Outer::Record(
        RewardRecordKeyV1 { kind, index },
        child,
    )))
}
/// Complete bounded root-only scope for these two fields; never a World proof.
pub fn qualify_reward_family_v1<
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
) -> Result<ReservedValue<RewardFamilyQualificationV1>, ValueTreeError> {
    roots.validate().map_err(|_| read::invalid())?;
    ctx.check()?;
    if !roots.state.iter().any(|root| root == ctx.root) {
        return Err(read::invalid());
    }
    let retained = ctx.values.memory.reserve(512, ctx.values.class)?;
    let mut metadata = [false; 2];
    let mut format = [false; 2];
    let mut counts = [0u64; 2];
    let mut length = None;
    let mut maximum = None;
    let cancel = RefCell::new(&mut ctx.cancelled);
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
        let result = StateScanContext {
            source: ctx.state,
            memory: local.values.memory,
            class: local.values.class,
            io: &mut *outer_scan,
        }
        .visit_entries(
            root,
            |entry| {
                *cells = cells
                    .checked_sub(1)
                    .ok_or(ValueTreeError::ResourceLimited("reward family cells"))?;
                if let Some(item) = outer(entry, &mut local)? {
                    match item {
                        Outer::Metadata(kind) => {
                            let i = index(kind);
                            if metadata[i] {
                                return Err(read::invalid());
                            }
                            metadata[i] = true;
                        }
                        Outer::Format(kind) => {
                            let i = index(kind);
                            if format[i] {
                                return Err(read::invalid());
                            }
                            format[i] = true;
                        }
                        Outer::Length(n) => {
                            if length.replace(n).is_some() {
                                return Err(read::invalid());
                            }
                        }
                        Outer::Record(key, child) => {
                            let mut body = WorldFieldReadContextV1 {
                                root: &child,
                                state: local.state,
                                values: ValueReadContext {
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
                            qualify_reward_body_v1(key, &mut body, inner_scan, cells, max)?;
                            let i = index(key.kind);
                            counts[i] = counts[i].checked_add(1).ok_or_else(read::invalid)?;
                            if key.kind == RewardRecordKindV1::Mint {
                                maximum =
                                    Some(maximum.map_or(key.index, |old: u64| old.max(key.index)));
                            }
                        }
                    }
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
                return Err(ValueTreeError::ResourceLimited("reward family scan"));
            }
            Err(_) => return Err(read::invalid()),
        }
    }
    let length = length.ok_or_else(read::invalid)?;
    if metadata != [true; 2]
        || format != [true; 2]
        || counts[1] != length
        || maximum.is_some_and(|i| i >= length)
    {
        return Err(read::invalid());
    }
    if (cancel.borrow_mut())() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(retained.retain(RewardFamilyQualificationV1 {
        roots: roots.digest().map_err(|_| read::invalid())?,
        counts,
        mint_length: length,
    }))
}
pub fn open_qualified_reward_record_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    key: RewardRecordKeyV1,
    roots: &RootSetV1,
    family: &ReservedValue<RewardFamilyQualificationV1>,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
) -> Result<Option<ReservedValue<RewardRecordQualificationV1>>, ValueTreeError> {
    ctx.check()?;
    if !family.uses_budget(ctx.values.memory) {
        return Err(read::invalid());
    }
    family.validate_binding(&roots.digest().map_err(|_| read::invalid())?)?;
    let top = roots
        .state
        .iter()
        .find(|root| root.namespace == key.kind.namespace())
        .ok_or_else(read::invalid)?;
    if ctx.root != top {
        return Err(read::invalid());
    }
    let child = inspect_reward_record_root_v1(key, ctx)?;
    if key.kind == RewardRecordKindV1::Mint
        && ((key.index < family.mint_length()) != child.is_some())
    {
        return Err(read::invalid());
    }
    let proof = child
        .map(|child| closure::mint(key, (*child).clone(), ctx.values.memory, ctx.values.class))
        .transpose()?;
    ctx.check()?;
    Ok(proof)
}
