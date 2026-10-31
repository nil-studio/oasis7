use super::*;
use oasis7_formal_protocol::incremental_state::{StateEntryV1, StateScanContext, StateScanError};
use std::cell::RefCell;
const LABELS: RecordReadLabelsV1 = RecordReadLabelsV1 {
    utf8: "diplomacy UTF-8",
    ledger_utf8: "diplomacy ledger UTF-8",
    ledger_frame: "diplomacy ledger frame",
    boolean: "diplomacy bool",
    fixed: "diplomacy fixed value",
    profile: "diplomacy value profile",
    integer: "diplomacy integer domain",
};
pub(super) fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("diplomacy record closure")
}
#[derive(Debug)]
pub struct DiplomacyRecordQualificationV1 {
    pub(super) field: DiplomacyFieldV1,
    pub(super) owner: [u8; 32],
    pub(super) root: StateRootV1,
}
impl DiplomacyRecordQualificationV1 {
    pub fn root(&self) -> &StateRootV1 {
        &self.root
    }
    pub fn validate_binding(
        &self,
        field: DiplomacyFieldV1,
        owner: &[u8; 32],
        root: &StateRootV1,
    ) -> Result<(), ValueTreeError> {
        if self.field != field || self.owner != *owner || self.root != *root {
            Err(invalid())
        } else {
            Ok(())
        }
    }
}
pub(super) fn primitive<R: ValueNodeReader, C: FnMut() -> bool>(
    kind: Kind,
    value: &[u8; 32],
    key: &(WorldFieldKeyV1, u64),
    records: &mut InventoryValueReader<'_, '_, R, C>,
    max: u64,
    mut output: &mut dyn std::io::Write,
) -> Result<RecordValueV1, ValueTreeError> {
    let codec = match kind {
        Kind::Unit => MemberPayloadCodecV1::Unit,
        Kind::Text | Kind::Enum(_) => MemberPayloadCodecV1::Text,
        Kind::U64 | Kind::U32 => MemberPayloadCodecV1::U64,
        Kind::I64 => MemberPayloadCodecV1::I64,
        Kind::Bool => MemberPayloadCodecV1::Bool,
    };
    let profile = RecordReadProfileV1 {
        codec,
        nullable: false,
        integer_max: if matches!(kind, Kind::U32) {
            u64::from(u32::MAX)
        } else {
            u64::MAX
        },
        labels: LABELS,
    };
    let result = if let Kind::Enum(labels) = kind {
        let mut bytes = [0; 32];
        let value = read_record_value_v1(
            value,
            key,
            records,
            32,
            &mut Cursor::new(bytes.as_mut_slice()),
            profile,
        )?;
        let RecordValueV1::Text { utf8_bytes } = value else {
            return Err(invalid());
        };
        let body = &bytes[..utf8_bytes as usize];
        if !labels.iter().any(|label| label.as_bytes() == body) {
            return Err(invalid());
        }
        output.write_all(body).map_err(ValueTreeError::Io)?;
        value
    } else {
        read_record_value_v1(value, key, records, max, &mut output, profile)?
    };
    if (records.cancelled)() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(result)
}
pub(super) fn relations<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    field: DiplomacyFieldV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
) -> Result<(), ValueTreeError> {
    let mut roots = [None; 32];
    let mut keys = std::array::from_fn::<_, 32, _>(|_| None);
    for (i, spec) in profile::specs(field).iter().enumerate() {
        let key = spec.path.read(field, ctx)?;
        roots[i] = ctx.lookup(&key.0)?;
        keys[i] = Some(key);
    }
    for (i, spec) in profile::specs(field).iter().enumerate() {
        if let Some(flag) = spec.flag {
            let key = keys[flag].as_ref().unwrap();
            let value = roots[flag].ok_or_else(invalid)?;
            let value = primitive(
                Kind::Bool,
                &value,
                key,
                &mut ctx.records(),
                1,
                &mut std::io::sink(),
            )?;
            let RecordValueV1::Bool(flag) = value else {
                return Err(invalid());
            };
            if flag != roots[i].is_some() {
                return Err(invalid());
            }
        } else if roots[i].is_none() {
            return Err(invalid());
        }
    }
    ctx.check()
}
pub(super) fn length<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    field: DiplomacyFieldV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
) -> Result<u64, ValueTreeError> {
    let collection = match field {
        DiplomacyFieldV1::Alliances => "members",
        DiplomacyFieldV1::Wars => "participant_outcomes",
        _ => return Ok(0),
    };
    let key = Path::Length(collection).read(field, ctx)?;
    let value = ctx.lookup(&key.0)?.ok_or_else(invalid)?;
    let RecordValueV1::Unsigned(n) = primitive(
        Kind::U64,
        &value,
        &key,
        &mut ctx.records(),
        8,
        &mut std::io::sink(),
    )?
    else {
        return Err(invalid());
    };
    Ok(n)
}
type Identity = ((WorldFieldKeyV1, u64), ReservedValue<Vec<u8>>);
pub(super) fn identity<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    field: DiplomacyFieldV1,
    entry: &StateEntryV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    max: u64,
) -> Result<Identity, ValueTreeError> {
    let binding = WorldFieldKeyV1::from_bytes(&entry.key).map_err(ValueTreeError::Invalid)?;
    let key = WorldFieldKeyV1::new(
        field.descriptor()?,
        WorldFieldRecordKindV1::Entry,
        binding.semantic_key,
    )
    .map_err(ValueTreeError::Invalid)?;
    if entry.namespace != StateNamespaceV1::Business || key.as_bytes() != entry.key {
        return Err(invalid());
    }
    let info = crate::incremental_field_member::read_member_record_header_v1(
        &entry.value,
        &key,
        &mut ctx.values,
        MemberRecordLimitsV1 {
            semantic_bytes: ctx.semantic_bytes,
            payload_bytes: max,
            record_bytes: ctx.record_bytes,
        },
        &mut ctx.cancelled,
    )?;
    let capacity = usize::try_from(info.semantic_bytes).map_err(|_| invalid())?;
    let charge = ctx.values.memory.reserve(capacity, ctx.values.class)?;
    let mut semantic = vec![0; capacity];
    let root = ctx.values.load_root(&entry.value)?;
    ctx.values.read_range(
        &root,
        (FIELD_MEMBER_HEADER_BYTES + key.as_bytes().len()) as u64,
        info.semantic_bytes,
        &mut Cursor::new(semantic.as_mut_slice()),
        &mut ctx.cancelled,
    )?;
    Ok(((key, info.semantic_bytes), charge.retain(semantic)))
}
pub(super) fn outcome<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    index: u64,
    full: bool,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
) -> Result<(), ValueTreeError> {
    let key = Path::Outcome(index).read(DiplomacyFieldV1::Wars, ctx)?;
    let value = ctx.lookup(&key.0)?.ok_or_else(invalid)?;
    primitive(
        Kind::Unit,
        &value,
        &key,
        &mut ctx.records(),
        0,
        &mut std::io::sink(),
    )?;
    if full {
        for name in [
            "agent_id",
            "electricity_delta",
            "data_delta",
            "reputation_delta",
        ] {
            let key = Path::OutcomeField(index, name).read(DiplomacyFieldV1::Wars, ctx)?;
            if ctx.lookup(&key.0)?.is_none() {
                return Err(invalid());
            }
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
    field: DiplomacyFieldV1,
    entry: &StateEntryV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    max: u64,
) -> Result<bool, ValueTreeError> {
    let (key, semantic) = identity(field, entry, ctx, max)?;
    let row = schema::inspect(field, &semantic)?;
    let (kind, count) = match row {
        schema::Row::Column(slot) => (profile::specs(field)[slot].kind, false),
        schema::Row::Member(index) => {
            if index >= length(field, ctx)? {
                return Err(invalid());
            }
            (Kind::Text, true)
        }
        schema::Row::Outcome(index, column) => {
            if index >= length(field, ctx)? {
                return Err(invalid());
            }
            outcome(index, column.is_none(), ctx)?;
            (
                match column {
                    None => Kind::Unit,
                    Some("agent_id") => Kind::Text,
                    Some(_) => Kind::I64,
                },
                column.is_none(),
            )
        }
    };
    primitive(
        kind,
        &entry.value,
        &key,
        &mut ctx.records(),
        max,
        &mut std::io::sink(),
    )?;
    ctx.check()?;
    Ok(count)
}
pub fn qualify_diplomacy_body_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    field: DiplomacyFieldV1,
    digest: [u8; 32],
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    scan: &mut StateScanBudget,
    cells: &mut u64,
    max: u64,
) -> Result<ReservedValue<DiplomacyRecordQualificationV1>, ValueTreeError> {
    ctx.check()?;
    let retained = ctx.values.memory.reserve(512, ctx.values.class)?;
    relations(field, ctx)?;
    let length = length(field, ctx)?;
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
    let mut count = 0u64;
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
                .ok_or(ValueTreeError::ResourceLimited("diplomacy closure cells"))?;
            if actual(field, entry, &mut local, max)? {
                count = count.checked_add(1).ok_or_else(invalid)?;
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
            return Err(ValueTreeError::ResourceLimited("diplomacy closure scan"));
        }
        Err(_) => return Err(invalid()),
    };
    if count != length {
        return Err(invalid());
    }
    Ok(retained.retain(DiplomacyRecordQualificationV1 {
        field,
        owner: digest,
        root: ctx.root.clone(),
    }))
}
