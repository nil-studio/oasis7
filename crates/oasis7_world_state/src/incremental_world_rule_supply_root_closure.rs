//! Root-only supply record schema and dynamic relationship qualification.
use super::*;
use oasis7_formal_protocol::incremental_state::{StateEntryV1, StateScanContext, StateScanError};
use std::cell::RefCell;
#[derive(Debug)]
pub struct SupplyRecordQualificationV1 {
    pub(super) schema: [u8; 32],
    pub(super) owner: [u8; 32],
    pub(super) root: StateRootV1,
}
impl SupplyRecordQualificationV1 {
    pub fn root(&self) -> &StateRootV1 {
        &self.root
    }
    pub fn validate_binding(
        &self,
        owner: SupplyRootKeyV1<'_>,
        digest: &[u8; 32],
        root: &StateRootV1,
    ) -> Result<(), ValueTreeError> {
        if self.schema != owner.schema() || self.owner != *digest || self.root != *root {
            Err(invalid())
        } else {
            Ok(())
        }
    }
}
pub(super) fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("supply record closure")
}
pub(super) fn primitive<R: ValueNodeReader, C: FnMut() -> bool>(
    kind: Kind,
    root: &[u8; 32],
    key: &(WorldFieldKeyV1, u64),
    values: &mut oasis7_formal_protocol::incremental_value::ValueReadContext<'_, R>,
    record_bytes: u64,
    max: u64,
    cancel: &mut C,
) -> Result<Option<bool>, ValueTreeError> {
    let codec = match kind {
        Kind::Unit => MemberPayloadCodecV1::Unit,
        Kind::Text | Kind::Status => MemberPayloadCodecV1::Text,
        Kind::Bool => MemberPayloadCodecV1::Bool,
        Kind::I64 => MemberPayloadCodecV1::I64,
        Kind::U32 | Kind::U64 => MemberPayloadCodecV1::U64,
    };
    let limit = match kind {
        Kind::Unit => 0,
        Kind::Bool => 1,
        Kind::I64 | Kind::U32 | Kind::U64 => 8,
        Kind::Status => 9,
        Kind::Text => max,
    };
    let profile = MemberReadProfileV1 {
        codec,
        semantic_bytes: key.1,
        limits: MemberRecordLimitsV1 {
            semantic_bytes: key.1,
            payload_bytes: limit,
            record_bytes,
        },
    };
    let mut bytes = [0; 9];
    if matches!(kind, Kind::Text | Kind::Status) {
        let length = if matches!(kind, Kind::Text) {
            crate::incremental_utf8_output::read_text_value(
                root,
                &key.0,
                values,
                profile,
                &mut std::io::sink(),
                "supply UTF-8",
                &mut *cancel,
            )?
        } else {
            crate::incremental_utf8_output::read_text_value(
                root,
                &key.0,
                values,
                profile,
                &mut Cursor::new(bytes.as_mut_slice()),
                "supply status UTF-8",
                &mut *cancel,
            )?
        };
        if matches!(kind, Kind::Status)
            && !matches!(
                &bytes[..length as usize],
                b"active" | b"resolved" | b"timed_out"
            )
        {
            return Err(invalid());
        }
    } else {
        read_member_record_profile_v1(
            root,
            &key.0,
            values,
            profile,
            &mut std::io::sink(),
            &mut Cursor::new(bytes.as_mut_slice()),
            &mut *cancel,
        )?;
        if matches!(kind, Kind::U32)
            && u64::from_be_bytes(bytes[..8].try_into().unwrap()) > u64::from(u32::MAX)
        {
            return Err(invalid());
        }
        if matches!(kind, Kind::Bool) {
            let result = match bytes[0] {
                0 => Ok(Some(false)),
                1 => Ok(Some(true)),
                _ => Err(invalid()),
            };
            if cancel() {
                return Err(ValueTreeError::Cancelled);
            }
            return result;
        }
    }
    if cancel() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(None)
}
pub(super) fn relations<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    owner: SupplyRootKeyV1<'_>,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
) -> Result<(), ValueTreeError> {
    let mut roots = [None; 29];
    let mut keys = std::array::from_fn::<_, 29, _>(|_| None);
    for i in 0..owner.count() {
        let k = owner.relative().fixed_path(owner.spec(i).path, ctx)?;
        roots[i] = ctx.lookup(&k.0)?;
        keys[i] = Some(k);
    }
    for i in 0..owner.count() {
        if let Some(flag) = owner.spec(i).flag {
            let k = keys[flag].as_ref().unwrap();
            let value = roots[flag].ok_or_else(invalid)?;
            let present = primitive(
                Kind::Bool,
                &value,
                k,
                &mut ctx.values,
                ctx.record_bytes,
                1,
                &mut ctx.cancelled,
            )?
            .ok_or_else(invalid)?;
            if present != roots[i].is_some() {
                return Err(invalid());
            }
        } else if roots[i].is_none() {
            return Err(invalid());
        }
    }
    ctx.check()
}
pub(super) fn actual<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    owner: SupplyRootKeyV1<'_>,
    entry: &StateEntryV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    max: u64,
) -> Result<(), ValueTreeError> {
    let (key, len, semantic) =
        crate::incremental_world_rule_organization::root::closure::identity(entry, ctx, max)?;
    let k = (key, len);
    match schema::inspect(owner, &semantic)? {
        schema::Key::Column(slot) => {
            primitive(
                owner.spec(slot).kind,
                &entry.value,
                &k,
                &mut ctx.values,
                ctx.record_bytes,
                max,
                &mut ctx.cancelled,
            )?;
        }
        schema::Key::Member(collection, member) => {
            let kind = if matches!(collection, "beneficiaries" | "frozen_lineages") {
                Kind::Unit
            } else {
                Kind::I64
            };
            primitive(
                kind,
                &entry.value,
                &k,
                &mut ctx.values,
                ctx.record_bytes,
                max,
                &mut ctx.cancelled,
            )?;
            if collection == "frozen_lineages" {
                for name in ["target", "fulfilled", "reserved"] {
                    let key = owner
                        .relative()
                        .fixed_path(RecordPathV1::MemberField(collection, member, name), ctx)?;
                    if ctx.lookup(&key.0)?.is_none() {
                        return Err(invalid());
                    }
                }
            }
        }
        schema::Key::FrozenField(member) => {
            primitive(
                Kind::I64,
                &entry.value,
                &k,
                &mut ctx.values,
                ctx.record_bytes,
                max,
                &mut ctx.cancelled,
            )?;
            let key = owner
                .relative()
                .fixed_path(RecordPathV1::Member("frozen_lineages", member), ctx)?;
            let value = ctx.lookup(&key.0)?.ok_or_else(invalid)?;
            primitive(
                Kind::Unit,
                &value,
                &key,
                &mut ctx.values,
                ctx.record_bytes,
                0,
                &mut ctx.cancelled,
            )?;
        }
    }
    ctx.check()
}
pub fn qualify_supply_body_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    owner: SupplyRootKeyV1<'_>,
    digest: [u8; 32],
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    scan: &mut StateScanBudget,
    cells: &mut u64,
    max: u64,
) -> Result<ReservedValue<SupplyRecordQualificationV1>, ValueTreeError> {
    ctx.check()?;
    let retained = ctx.values.memory.reserve(512, ctx.values.class)?;
    relations(owner, ctx)?;
    let cancel = RefCell::new(&mut ctx.cancelled);
    let mut local = WorldFieldReadContextV1 {
        root: ctx.root,
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
        io: scan,
    }
    .visit_entries(
        ctx.root,
        |entry| {
            *cells = cells
                .checked_sub(1)
                .ok_or(ValueTreeError::ResourceLimited("supply closure cells"))?;
            actual(owner, entry, &mut local, max)
        },
        || (cancel.borrow_mut())(),
    );
    let original = (local.take_state_error)();
    if (cancel.borrow_mut())() {
        return Err(ValueTreeError::Cancelled);
    }
    if let Some(e) = original {
        return Err(e);
    }
    match result {
        Ok(_) => {}
        Err(StateScanError::Visitor(e)) => return Err(e),
        Err(StateScanError::Memory(e)) => return Err(e.into()),
        Err(StateScanError::Cancelled) => return Err(ValueTreeError::Cancelled),
        Err(StateScanError::ResourceLimited) => {
            return Err(ValueTreeError::ResourceLimited("supply closure scan"));
        }
        Err(_) => return Err(invalid()),
    }
    Ok(retained.retain(SupplyRecordQualificationV1 {
        schema: owner.schema(),
        owner: digest,
        root: ctx.root.clone(),
    }))
}

pub(super) fn from_typed(
    owner: SupplyRootKeyV1<'_>,
    digest: [u8; 32],
    root: &StateRootV1,
    memory: &oasis7_proto::memory_budget::MemoryBudget,
) -> Result<ReservedValue<SupplyRecordQualificationV1>, ValueTreeError> {
    Ok(memory
        .reserve(512, MemoryClass::Ordinary)?
        .retain(SupplyRecordQualificationV1 {
            schema: owner.schema(),
            owner: digest,
            root: root.clone(),
        }))
}

pub(super) fn from_family(
    owner: SupplyRootKeyV1<'_>,
    digest: [u8; 32],
    root: &StateRootV1,
    memory: &oasis7_proto::memory_budget::MemoryBudget,
    class: MemoryClass,
) -> Result<ReservedValue<SupplyRecordQualificationV1>, ValueTreeError> {
    Ok(memory
        .reserve(512, class)?
        .retain(SupplyRecordQualificationV1 {
            schema: owner.schema(),
            owner: digest,
            root: root.clone(),
        }))
}

pub(super) fn frozen_relation<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    owner: SupplyRootKeyV1<'_>,
    member: &str,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
) -> Result<(), ValueTreeError> {
    let mut present = [false; 4];
    for (i, path) in [
        RecordPathV1::Member("frozen_lineages", member),
        RecordPathV1::MemberField("frozen_lineages", member, "target"),
        RecordPathV1::MemberField("frozen_lineages", member, "fulfilled"),
        RecordPathV1::MemberField("frozen_lineages", member, "reserved"),
    ]
    .into_iter()
    .enumerate()
    {
        let key = owner.relative().fixed_path(path, ctx)?;
        present[i] = ctx.lookup(&key.0)?.is_some();
    }
    if present.iter().any(|v| *v != present[0]) {
        return Err(invalid());
    }
    ctx.check()
}
