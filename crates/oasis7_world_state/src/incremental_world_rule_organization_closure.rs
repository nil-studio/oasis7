//! Root-only validation of the relative organization owner schema.
use super::*;
use crate::incremental_world_rule_organization::{Cell, Kind, MAX};
use oasis7_formal_protocol::incremental_state::{StateEntryV1, StateScanContext, StateScanError};
use oasis7_proto::memory_budget::ReservedValue;
use std::cell::RefCell;

#[derive(Debug)]
pub struct OrganizationRecordQualificationV1 {
    schema: [u8; 32],
    owner: [u8; 32],
    root: StateRootV1,
}
impl OrganizationRecordQualificationV1 {
    pub fn root(&self) -> &StateRootV1 {
        &self.root
    }
    pub fn validate_binding(
        &self,
        schema: &[u8; 32],
        owner: &[u8; 32],
        root: &StateRootV1,
    ) -> Result<(), ValueTreeError> {
        if self.schema != *schema || self.owner != *owner || self.root != *root {
            return Err(ValueTreeError::Invalid(
                "organization record qualification binding",
            ));
        }
        Ok(())
    }
}
fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("organization owner closure")
}
pub(super) fn local_record(record: OrganizationRelativeRecordV1<'_>) -> K<'_> {
    match record {
        OrganizationRelativeRecordV1::Case => K::OrganizationCaseInner,
        OrganizationRelativeRecordV1::Estate => K::OrganizationEstateInner,
        OrganizationRelativeRecordV1::Grant(key) => K::OrganizationGrantInner(key),
        OrganizationRelativeRecordV1::Recovery(key) => K::OrganizationRecoveryInner(key),
    }
}
pub(super) fn owner_record(
    owner: OrganizationRootKeyV1<'_>,
) -> OrganizationRelativeRecordV1<'static> {
    match owner {
        OrganizationRootKeyV1::Case(_) => OrganizationRelativeRecordV1::Case,
        OrganizationRootKeyV1::Estate(_) => OrganizationRelativeRecordV1::Estate,
    }
}
pub(super) fn profile_of(record: OrganizationRelativeRecordV1<'_>) -> super::super::Profile {
    match record {
        OrganizationRelativeRecordV1::Case => super::super::Profile::Case,
        OrganizationRelativeRecordV1::Estate => super::super::Profile::Estate,
        OrganizationRelativeRecordV1::Grant(_) => super::super::Profile::Grant,
        OrganizationRelativeRecordV1::Recovery(_) => super::super::Profile::Recovery,
    }
}
/// Point dependencies only. No Text body or unrelated member is opened here.
pub(super) fn relations<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    record: OrganizationRelativeRecordV1<'_>,
    context: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    allow_absent: bool,
) -> Result<bool, ValueTreeError> {
    let mut roots = [None; MAX];
    let mut keys = std::array::from_fn::<_, MAX, _>(|_| None);
    let specs = profile_of(record).specs();
    for (i, spec) in specs.iter().enumerate() {
        let key = local_record(record).fixed_path(spec.path, context)?;
        roots[i] = context.lookup(&key.0)?;
        keys[i] = Some(key);
    }
    if roots[0].is_none() {
        if !allow_absent || roots[..specs.len()].iter().any(Option::is_some) {
            return Err(invalid());
        }
        return Ok(false);
    }
    for (i, spec) in specs.iter().enumerate() {
        if let Some(flag) = spec.flag {
            let flag = super::super::bool_value(
                &roots[flag].ok_or_else(invalid)?,
                keys[flag].as_ref().unwrap(),
                &mut context.values,
                context.record_bytes,
                &mut context.cancelled,
            )?;
            if roots[i].is_some() != flag {
                return Err(invalid());
            }
        } else if roots[i].is_none() {
            return Err(invalid());
        }
    }
    context.check()?;
    Ok(true)
}
pub(super) fn primitive<R: ValueNodeReader, C: FnMut() -> bool>(
    kind: Kind,
    root: &[u8; 32],
    key: &(WorldFieldKeyV1, u64),
    values: &mut oasis7_formal_protocol::incremental_value::ValueReadContext<'_, R>,
    record_bytes: u64,
    max_payload: u64,
    cancelled: &mut C,
) -> Result<Cell, ValueTreeError> {
    use super::super::{bool_value, disposition, fixed, marker, phase, profile};
    Ok(match kind {
        Kind::Unit => {
            marker(root, &key.0, key.1, values, record_bytes, cancelled)?;
            Cell::Unit
        }
        Kind::Text => Cell::Text(crate::incremental_utf8_output::read_text_value(
            root,
            &key.0,
            values,
            profile(MemberPayloadCodecV1::Text, key.1, max_payload, record_bytes),
            &mut std::io::sink(),
            "organization UTF-8",
            cancelled,
        )?),
        Kind::U64 | Kind::U32 => {
            let value = u64::from_be_bytes(fixed(
                root,
                key,
                MemberPayloadCodecV1::U64,
                values,
                record_bytes,
                cancelled,
            )?);
            if matches!(kind, Kind::U32) && value > u64::from(u32::MAX) {
                return Err(invalid());
            }
            Cell::U64(value)
        }
        Kind::I64 => Cell::I64(i64::from_be_bytes(fixed(
            root,
            key,
            MemberPayloadCodecV1::I64,
            values,
            record_bytes,
            cancelled,
        )?)),
        Kind::Bool => Cell::Bool(bool_value(root, key, values, record_bytes, cancelled)?),
        Kind::Phase | Kind::Disposition => {
            let mut bytes = [0; 11];
            let length = crate::incremental_utf8_output::read_text_value(
                root,
                &key.0,
                values,
                profile(MemberPayloadCodecV1::Text, key.1, 11, record_bytes),
                &mut Cursor::new(bytes.as_mut_slice()),
                "organization enum UTF-8",
                cancelled,
            )?;
            if matches!(kind, Kind::Phase) {
                Cell::Phase(phase(&bytes[..length as usize])?)
            } else {
                Cell::Disposition(disposition(&bytes[..length as usize])?)
            }
        }
    })
}
pub(super) fn length<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    context: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
) -> Result<u64, ValueTreeError> {
    let key = K::OrganizationCaseInner
        .fixed_path(RecordPathV1::Nested("stage_receipts", "length"), context)?;
    let root = context.lookup(&key.0)?.ok_or_else(invalid)?;
    let Cell::U64(length) = primitive(
        Kind::U64,
        &root,
        &key,
        &mut context.values,
        context.record_bytes,
        8,
        &mut context.cancelled,
    )?
    else {
        return Err(invalid());
    };
    Ok(length)
}
pub(super) fn actual<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    owner: OrganizationRootKeyV1<'_>,
    entry: &StateEntryV1,
    context: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    max_payload: u64,
) -> Result<bool, ValueTreeError> {
    actual_mode(owner, entry, context, max_payload, false)
}
fn actual_mode<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    owner: OrganizationRootKeyV1<'_>,
    entry: &StateEntryV1,
    context: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    max_payload: u64,
    complete_scan: bool,
) -> Result<bool, ValueTreeError> {
    let (key, semantic_bytes, semantic) = identity(entry, context, max_payload)?;
    let parsed = inspect_organization_relative_key_v1(owner, &semantic)?;
    let record_key = (key, semantic_bytes);
    let receipt = match parsed {
        OrganizationRelativeKeyV1::Column { record, slot } => {
            primitive(
                profile_of(record).specs()[slot].kind,
                &entry.value,
                &record_key,
                &mut context.values,
                context.record_bytes,
                max_payload,
                &mut context.cancelled,
            )?;
            if matches!(
                record,
                OrganizationRelativeRecordV1::Grant(_) | OrganizationRelativeRecordV1::Recovery(_)
            ) {
                if complete_scan && slot != 0 {
                    nested_marker(record, context)?;
                } else {
                    relations(record, context, false)?;
                }
            }
            false
        }
        OrganizationRelativeKeyV1::Member(_) => {
            primitive(
                Kind::Unit,
                &entry.value,
                &record_key,
                &mut context.values,
                context.record_bytes,
                0,
                &mut context.cancelled,
            )?;
            false
        }
        OrganizationRelativeKeyV1::Scope { grant, .. } => {
            primitive(
                Kind::Unit,
                &entry.value,
                &record_key,
                &mut context.values,
                context.record_bytes,
                0,
                &mut context.cancelled,
            )?;
            if complete_scan {
                nested_marker(OrganizationRelativeRecordV1::Grant(grant), context)?;
            } else {
                relations(OrganizationRelativeRecordV1::Grant(grant), context, false)?;
            }
            false
        }
        OrganizationRelativeKeyV1::Receipt(ordinal) => {
            primitive(
                Kind::Text,
                &entry.value,
                &record_key,
                &mut context.values,
                context.record_bytes,
                max_payload,
                &mut context.cancelled,
            )?;
            if ordinal >= length(context)? {
                return Err(invalid());
            }
            true
        }
    };
    context.check()?;
    Ok(receipt)
}
/// Entire root-only bootstrap/recovery scope. The root is supplied by an
/// authenticated outer reference; no original World/Case snapshot is needed.
/// Qualification remains record-local until the family scan also succeeds.
pub fn qualify_organization_body_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    owner: OrganizationRootKeyV1<'_>,
    owner_digest: [u8; 32],
    context: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    scan: &mut StateScanBudget,
    cells_remaining: &mut u64,
    max_payload: u64,
) -> Result<ReservedValue<OrganizationRecordQualificationV1>, ValueTreeError> {
    context.check()?;
    let memory = context.values.memory;
    let class = context.values.class;
    let state = context.state;
    let root = context.root;
    let retained = memory.reserve(512, class)?;
    relations(owner_record(owner), context, false)?;
    let cancelled = RefCell::new(&mut context.cancelled);
    let mut local = WorldFieldReadContextV1 {
        root,
        state,
        values: oasis7_formal_protocol::incremental_value::ValueReadContext {
            source: context.values.source,
            memory,
            class,
            io: &mut *context.values.io,
        },
        key_work: &mut *context.key_work,
        semantic_bytes: context.semantic_bytes,
        record_bytes: context.record_bytes,
        take_state_error: &mut context.take_state_error,
        cancelled: || (cancelled.borrow_mut())(),
    };
    let mut receipts = 0u64;
    let result = StateScanContext {
        source: state,
        memory,
        class,
        io: scan,
    }
    .visit_entries(
        root,
        |entry| {
            *cells_remaining =
                cells_remaining
                    .checked_sub(1)
                    .ok_or(ValueTreeError::ResourceLimited(
                        "organization raw closure cells",
                    ))?;
            // Every record marker is visited exactly once and authenticates
            // all mandatory fixed relationships. Other leaves only need the
            // direct parent marker; no set of visited IDs is retained.
            if actual_mode(owner, entry, &mut local, max_payload, true)? {
                receipts = receipts.checked_add(1).ok_or_else(invalid)?;
            }
            Ok(())
        },
        || (cancelled.borrow_mut())(),
    );
    let original = (local.take_state_error)();
    if (cancelled.borrow_mut())() {
        return Err(ValueTreeError::Cancelled);
    }
    if let Some(error) = original {
        return Err(error);
    }
    match result {
        Ok(_) => {}
        Err(StateScanError::Visitor(error)) => return Err(error),
        Err(StateScanError::Memory(error)) => return Err(ValueTreeError::Memory(error)),
        Err(StateScanError::Cancelled) => return Err(ValueTreeError::Cancelled),
        Err(StateScanError::ResourceLimited) => {
            return Err(ValueTreeError::ResourceLimited(
                "organization raw closure scan",
            ));
        }
        Err(StateScanError::Invalid(_) | StateScanError::Tree(_)) => return Err(invalid()),
    }
    if matches!(owner, OrganizationRootKeyV1::Case(_)) && receipts != length(&mut local)? {
        return Err(invalid());
    }
    if (cancelled.borrow_mut())() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(retained.retain(OrganizationRecordQualificationV1 {
        schema: owner.schema(),
        owner: owner_digest,
        root: root.clone(),
    }))
}

pub(super) fn from_typed(
    owner: OrganizationRootKeyV1<'_>,
    owner_digest: [u8; 32],
    root: &StateRootV1,
    memory: &oasis7_proto::memory_budget::MemoryBudget,
) -> Result<ReservedValue<OrganizationRecordQualificationV1>, ValueTreeError> {
    let retained = memory.reserve(512, MemoryClass::Ordinary)?;
    Ok(retained.retain(OrganizationRecordQualificationV1 {
        schema: owner.schema(),
        owner: owner_digest,
        root: root.clone(),
    }))
}

pub(crate) fn identity<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    entry: &StateEntryV1,
    context: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    max_payload: u64,
) -> Result<(WorldFieldKeyV1, u64, ReservedValue<Vec<u8>>), ValueTreeError> {
    let binding = WorldFieldKeyV1::from_bytes(&entry.key).map_err(ValueTreeError::Invalid)?;
    let key = WorldFieldKeyV1::new(
        crate::incremental_world_rule_parent::descriptor()?,
        binding.kind,
        binding.semantic_key,
    )
    .map_err(ValueTreeError::Invalid)?;
    if entry.namespace != StateNamespaceV1::Authorization
        || binding.kind != WorldFieldRecordKindV1::Entry
        || key.as_bytes() != entry.key
    {
        return Err(invalid());
    }
    let info = crate::incremental_field_member::read_member_record_header_v1(
        &entry.value,
        &key,
        &mut context.values,
        MemberRecordLimitsV1 {
            semantic_bytes: context.semantic_bytes,
            payload_bytes: max_payload,
            record_bytes: context.record_bytes,
        },
        &mut context.cancelled,
    )?;
    let capacity = usize::try_from(info.semantic_bytes).map_err(|_| invalid())?;
    let key_charge = context
        .values
        .memory
        .reserve(capacity, context.values.class)?;
    let mut semantic = vec![0; capacity];
    let value_root = context.values.load_root(&entry.value)?;
    context.values.read_range(
        &value_root,
        (FIELD_MEMBER_HEADER_BYTES + key.as_bytes().len()) as u64,
        info.semantic_bytes,
        &mut Cursor::new(semantic.as_mut_slice()),
        &mut context.cancelled,
    )?;
    Ok((key, info.semantic_bytes, key_charge.retain(semantic)))
}

pub(super) fn from_family(
    owner: OrganizationRootKeyV1<'_>,
    owner_digest: [u8; 32],
    root: &StateRootV1,
    memory: &oasis7_proto::memory_budget::MemoryBudget,
    class: MemoryClass,
) -> Result<ReservedValue<OrganizationRecordQualificationV1>, ValueTreeError> {
    let retained = memory.reserve(512, class)?;
    Ok(retained.retain(OrganizationRecordQualificationV1 {
        schema: owner.schema(),
        owner: owner_digest,
        root: root.clone(),
    }))
}

fn nested_marker<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    record: OrganizationRelativeRecordV1<'_>,
    context: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
) -> Result<(), ValueTreeError> {
    let key = local_record(record).fixed_path(RecordPathV1::Record, context)?;
    let root = context.lookup(&key.0)?.ok_or_else(invalid)?;
    super::super::marker(
        &root,
        &key.0,
        key.1,
        &mut context.values,
        context.record_bytes,
        &mut context.cancelled,
    )
}
