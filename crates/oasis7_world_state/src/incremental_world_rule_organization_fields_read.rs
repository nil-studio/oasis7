//! Draft point consumers of a qualified relative record (not registered yet).
use super::*;
use crate::{
    incremental_utf8_output::read_text_value,
    incremental_world_rule_organization::{Cell, Kind, ReadLimits},
};
use oasis7_proto::memory_budget::ReservedValue;
use std::io::Write;
pub struct OrganizationFieldRequestV1<'a> {
    pub owner: OrganizationRootKeyV1<'a>,
    pub record: OrganizationRelativeRecordV1<'a>,
    pub field: OrganizationScalarFieldV1,
    pub qualification: &'a ReservedValue<OrganizationRecordQualificationV1>,
}
pub(super) fn check_request<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    request: &OrganizationFieldRequestV1<'_>,
    context: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
) -> Result<(), ValueTreeError> {
    context.check()?;
    if !request.qualification.uses_budget(context.values.memory) {
        return Err(ValueTreeError::Invalid(
            "organization field qualification pool",
        ));
    }
    if !matches!(
        (request.owner, request.record),
        (
            OrganizationRootKeyV1::Case(_),
            OrganizationRelativeRecordV1::Case
                | OrganizationRelativeRecordV1::Grant(_)
                | OrganizationRelativeRecordV1::Recovery(_)
        ) | (
            OrganizationRootKeyV1::Estate(_),
            OrganizationRelativeRecordV1::Estate
        )
    ) {
        return Err(ValueTreeError::Invalid(
            "organization field owner/record profile",
        ));
    }
    let (key, _) = request
        .owner
        .outer()
        .fixed_path(RecordPathV1::Record, context)?;
    let digest = WorldFieldKeyV1::from_bytes(key.as_bytes())
        .map_err(ValueTreeError::Invalid)?
        .semantic_key
        .ok_or(ValueTreeError::Invalid("organization field owner digest"))?;
    request
        .qualification
        .validate_binding(&request.owner.schema(), &digest, context.root)
}
fn cell<R: ValueNodeReader, W: Write, C: FnMut() -> bool>(
    kind: Kind,
    root: &[u8; 32],
    key: &(WorldFieldKeyV1, u64),
    values: &mut oasis7_formal_protocol::incremental_value::ValueReadContext<'_, R>,
    limits: ReadLimits,
    output: &mut W,
    cancelled: &mut C,
) -> Result<Cell, ValueTreeError> {
    if matches!(kind, Kind::Text) {
        return Ok(Cell::Text(read_text_value(
            root,
            &key.0,
            values,
            super::super::profile(
                MemberPayloadCodecV1::Text,
                key.1,
                limits.max_payload,
                limits.record_bytes,
            ),
            output,
            "organization field UTF-8",
            cancelled,
        )?));
    }
    closure::primitive(
        kind,
        root,
        key,
        values,
        limits.record_bytes,
        limits.max_payload,
        cancelled,
    )
}
fn value(
    kind: Kind,
    optional: bool,
    cells: [Cell; 2],
) -> Result<OrganizationFieldValueV1, ValueTreeError> {
    use OrganizationFieldValueV1 as V;
    let bad = || ValueTreeError::Invalid("organization field decoded value");
    if optional {
        let Cell::Bool(present) = cells[0] else {
            return Err(bad());
        };
        return Ok(match (kind, present, cells[1]) {
            (Kind::Text, false, Cell::Absent) => V::OptionalText { utf8_bytes: None },
            (Kind::Text, true, Cell::Text(length)) => V::OptionalText {
                utf8_bytes: Some(length),
            },
            (Kind::U64, false, Cell::Absent) => V::OptionalU64(None),
            (Kind::U64, true, Cell::U64(value)) => V::OptionalU64(Some(value)),
            (Kind::Bool, false, Cell::Absent) => V::OptionalBool(None),
            (Kind::Bool, true, Cell::Bool(value)) => V::OptionalBool(Some(value)),
            _ => return Err(bad()),
        });
    }
    Ok(match (kind, cells[0]) {
        (Kind::Text, Cell::Text(length)) => V::Text { utf8_bytes: length },
        (Kind::U32, Cell::U64(value)) => V::U32(u32::try_from(value).map_err(|_| bad())?),
        (Kind::U64, Cell::U64(value)) => V::U64(value),
        (Kind::I64, Cell::I64(value)) => V::I64(value),
        (Kind::Bool, Cell::Bool(value)) => V::Bool(value),
        (Kind::Phase, Cell::Phase(value)) => V::Phase(value),
        (Kind::Disposition, Cell::Disposition(value)) => V::Disposition(value),
        _ => return Err(bad()),
    })
}
/// Visits only the selected original field and its direct Option/record
/// dependencies. Unrelated Text and dynamic collections are never scanned.
pub fn read_qualified_organization_field_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: Write,
>(
    request: OrganizationFieldRequestV1<'_>,
    context: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    max_payload: u64,
    output: &mut W,
) -> Result<OrganizationFieldReadV1, ValueTreeError> {
    check_request(&request, context)?;
    let span = request.field.span(request.record)?;
    let record = closure::local_record(request.record);
    let marker = record.fixed_path(RecordPathV1::Record, context)?;
    let Some(root) = context.lookup(&marker.0)? else {
        context.check()?;
        return Ok(OrganizationFieldReadV1::RecordAbsent);
    };
    super::super::marker(
        &root,
        &marker.0,
        marker.1,
        &mut context.values,
        context.record_bytes,
        &mut context.cancelled,
    )?;
    let specs = closure::profile_of(request.record).specs();
    let mut keys = std::array::from_fn::<_, 2, _>(|_| None);
    let mut roots = [None; 2];
    for (offset, spec) in specs[span.slot..span.slot + span.count].iter().enumerate() {
        let key = record.fixed_path(spec.path, context)?;
        roots[offset] = context.lookup(&key.0)?;
        keys[offset] = Some(key);
    }
    let result = resolved(
        &span,
        &keys,
        &roots,
        &mut context.values,
        ReadLimits {
            record_bytes: context.record_bytes,
            max_payload,
        },
        output,
        &mut context.cancelled,
    )?;
    context.check()?;
    Ok(OrganizationFieldReadV1::Value(result))
}

pub(super) fn resolved<R: ValueNodeReader, W: Write, C: FnMut() -> bool>(
    span: &fields_schema::Span,
    keys: &[Option<(WorldFieldKeyV1, u64)>; 2],
    roots: &[Option<[u8; 32]>; 2],
    values: &mut oasis7_formal_protocol::incremental_value::ValueReadContext<'_, R>,
    limits: ReadLimits,
    output: &mut W,
    cancelled: &mut C,
) -> Result<OrganizationFieldValueV1, ValueTreeError> {
    let mut cells = [Cell::Absent; 2];
    for offset in 0..span.count {
        if span.count == 2 && offset == 1 {
            let Cell::Bool(present) = cells[0] else {
                return Err(ValueTreeError::Invalid("organization field option flag"));
            };
            if present != roots[offset].is_some() {
                return Err(ValueTreeError::Invalid(
                    "organization field option presence",
                ));
            }
        }
        let Some(root) = roots[offset] else {
            if span.count == 1 || offset == 0 {
                return Err(ValueTreeError::Invalid("organization field missing column"));
            }
            continue;
        };
        let kind = if span.count == 2 && offset == 0 {
            Kind::Bool
        } else {
            span.kind
        };
        cells[offset] = cell(
            kind,
            &root,
            keys[offset].as_ref().unwrap(),
            values,
            ReadLimits {
                record_bytes: limits.record_bytes,
                max_payload: limits.max_payload,
            },
            output,
            cancelled,
        )?;
    }
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    value(span.kind, span.count == 2, cells)
}
