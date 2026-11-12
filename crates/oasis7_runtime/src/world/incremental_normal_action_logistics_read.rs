//! Fixed original logistics tuple reads and route-ID encoding, on one Current.
use crate::world::incremental_body::BodyMutationError;
use oasis7_formal_protocol::{
    controlled_authority::formal::{FormalError, bounded_context::FormalBoundedContextV1},
    incremental_state::MerkleNodeReader,
    incremental_value::{ValueNodeReader, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryClass, ReservedValue};
use oasis7_world_state::{
    MaterialLedgerId, MaterialTransitPriority,
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1},
    incremental_ledger_value::LedgerValueKindV1,
    incremental_logistics_records::{
        LogisticsCellReadV1, LogisticsCellV1, LogisticsColumnV1 as Column, LogisticsRecordKeyV1,
        LogisticsRecordKindV1, LogisticsValueV1, read_logistics_cell_overlay_v1,
    },
};
use std::{
    io::{self, Write},
    mem::size_of,
};
pub(super) fn charge(
    ctx: &mut FormalBoundedContextV1<'_>,
    n: usize,
) -> Result<(), BodyMutationError> {
    ctx.charge(
        1,
        u64::try_from(n).map_err(|_| ValueTreeError::ResourceLimited("logistics work length"))?,
    )
    .map_err(|e| BodyMutationError::Original(Box::new(e)))
}
pub(super) fn overlay(e: WorldFieldOverlayErrorV1) -> BodyMutationError {
    match e {
        WorldFieldOverlayErrorV1::Execution(e) => e.into(),
        WorldFieldOverlayErrorV1::Value(e) => e.into(),
        WorldFieldOverlayErrorV1::Member(e) => BodyMutationError::Original(Box::new(e)),
    }
}
fn bad() -> BodyMutationError {
    BodyMutationError::Invalid("current logistics tuple profile")
}
struct Compare<'a, 'c, 'w> {
    remaining: &'a [u8],
    equal: bool,
    common: &'c mut FormalBoundedContextV1<'w>,
    error: Option<FormalError>,
}
impl Write for Compare<'_, '_, '_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if let Err(error) = self.common.charge(1, bytes.len() as u64) {
            self.error = Some(error);
            return Err(io::Error::other("logistics comparison work"));
        }
        let n = bytes.len().min(self.remaining.len());
        self.equal &= n == bytes.len() && self.remaining[..n] == bytes[..n];
        self.remaining = &self.remaining[n..];
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
fn ledger(value: &MaterialLedgerId) -> (LedgerValueKindV1, &str) {
    match value {
        MaterialLedgerId::World => (LedgerValueKindV1::World, ""),
        MaterialLedgerId::Agent(id) => (LedgerValueKindV1::Agent, id),
        MaterialLedgerId::Site(id) => (LedgerValueKindV1::Site, id),
        MaterialLedgerId::Factory(id) => (LedgerValueKindV1::Factory, id),
    }
}
fn point<M, R, E, C>(
    id: &str,
    column: Column,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    max: u64,
    output: &mut impl Write,
) -> Result<LogisticsValueV1, BodyMutationError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    match read_logistics_cell_overlay_v1(
        LogisticsRecordKeyV1 {
            kind: LogisticsRecordKindV1::Route,
            id,
        },
        LogisticsCellV1::Column(column),
        context,
        max,
        output,
    )
    .map_err(overlay)?
    {
        LogisticsCellReadV1::Value(value) => Ok(value),
        _ => Err(bad()),
    }
}
fn compare_text<M, R, E, C>(
    id: &str,
    column: Column,
    text: &str,
    kind: Option<LedgerValueKindV1>,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    common: &mut FormalBoundedContextV1<'_>,
    max: u64,
) -> Result<bool, BodyMutationError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    charge(common, id.len())?;
    let mut output = Compare {
        remaining: text.as_bytes(),
        equal: true,
        common,
        error: None,
    };
    let value = point(id, column, context, max, &mut output);
    if let Some(error) = output.error.take() {
        return Err(BodyMutationError::Original(Box::new(error)));
    }
    let profile = match (kind, value?) {
        (None, LogisticsValueV1::Text { .. }) => true,
        (Some(expected), LogisticsValueV1::Ledger(info)) => info.kind == expected,
        _ => return Err(bad()),
    };
    Ok(profile && output.equal && output.remaining.is_empty())
}
/// Original predicate order: from, to, normalized kind, distance, priority.
/// In particular the map key is not replaced by the embedded route_id column.
#[allow(clippy::too_many_arguments)]
pub(super) fn matches_current_tuple_v1<M, R, E, C>(
    id: &str,
    from: &MaterialLedgerId,
    to: &MaterialLedgerId,
    kind: &str,
    distance: i64,
    priority: MaterialTransitPriority,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    common: &mut FormalBoundedContextV1<'_>,
    max: u64,
) -> Result<bool, BodyMutationError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    let (tag, text) = ledger(from);
    if !compare_text(
        id,
        Column::FromLedger,
        text,
        Some(tag),
        context,
        common,
        max,
    )? {
        return Ok(false);
    }
    let (tag, text) = ledger(to);
    if !compare_text(id, Column::ToLedger, text, Some(tag), context, common, max)? {
        return Ok(false);
    }
    if !compare_text(id, Column::Kind, kind, None, context, common, max)? {
        return Ok(false);
    }
    charge(common, 8)?;
    match point(id, Column::DistanceKm, context, 64, &mut io::sink())? {
        LogisticsValueV1::Signed(value) if value == distance => {}
        LogisticsValueV1::Signed(_) => return Ok(false),
        _ => return Err(bad()),
    }
    charge(common, 8)?;
    let expected = match priority {
        MaterialTransitPriority::Standard => 0,
        MaterialTransitPriority::Urgent => 1,
    };
    match point(id, Column::Priority, context, 64, &mut io::sink())? {
        LogisticsValueV1::Unsigned(value) => Ok(value == expected),
        _ => Err(bad()),
    }
}
struct JsonPass<'c, 'w> {
    common: &'c mut FormalBoundedContextV1<'w>,
    output: Option<Vec<u8>>,
    used: usize,
    limit: usize,
    error: Option<FormalError>,
    overflow: bool,
}
impl Write for JsonPass<'_, '_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if let Err(error) = self.common.charge(1, bytes.len() as u64) {
            self.error = Some(error);
            return Err(io::Error::other("logistics route-id work"));
        }
        let Some(end) = self
            .used
            .checked_add(bytes.len())
            .filter(|n| *n <= self.limit)
        else {
            self.overflow = true;
            return Err(io::Error::other("logistics route-id length"));
        };
        if let Some(output) = &mut self.output {
            output.extend_from_slice(bytes);
        }
        self.used = end;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
impl JsonPass<'_, '_> {
    fn finish(&mut self, result: Result<(), serde_json::Error>) -> Result<(), BodyMutationError> {
        if let Some(error) = self.error.take() {
            return Err(BodyMutationError::Original(Box::new(error)));
        }
        if self.overflow {
            return Err(ValueTreeError::ResourceLimited("logistics route-id length").into());
        }
        result.map_err(|_| BodyMutationError::Invalid("original logistics route tuple encoding"))
    }
}
/// Two passes use the original serde_json tuple codec and original SHA-256;
/// no formatted-ledger copy or replacement hash grammar is introduced.
pub(super) fn route_id_bounded_v1(
    from: &MaterialLedgerId,
    to: &MaterialLedgerId,
    kind: &str,
    distance: i64,
    priority: MaterialTransitPriority,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<ReservedValue<String>, BodyMutationError> {
    let tuple = (from, to, kind, distance, priority);
    let mut measure = JsonPass {
        common,
        output: None,
        used: 0,
        limit: usize::MAX,
        error: None,
        overflow: false,
    };
    let result = serde_json::to_writer(&mut measure, &tuple);
    measure.finish(result)?;
    let n = measure.used;
    drop(measure);
    let layout = n
        .checked_add(64)
        .and_then(|v| {
            v.checked_add(
                size_of::<Vec<u8>>() + size_of::<String>() + size_of::<JsonPass<'_, '_>>(),
            )
        })
        .ok_or(ValueTreeError::ResourceLimited("logistics route-id layout"))?;
    let fee = common
        .memory()
        .reserve(layout, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    charge(common, n)?;
    let bytes = Vec::with_capacity(n);
    if bytes.capacity() > n {
        return Err(ValueTreeError::ResourceLimited("logistics route-id capacity").into());
    }
    let mut write = JsonPass {
        common,
        output: Some(bytes),
        used: 0,
        limit: n,
        error: None,
        overflow: false,
    };
    let result = serde_json::to_writer(&mut write, &tuple);
    write.finish(result)?;
    if write.used != n {
        return Err(BodyMutationError::Invalid(
            "logistics route-id encoding changed",
        ));
    }
    let bytes = write.output.take().ok_or_else(bad)?;
    drop(write);
    charge(common, n)?;
    let hash = crate::util::sha256_hex(&bytes);
    if hash.capacity() > 64 {
        return Err(ValueTreeError::ResourceLimited("logistics route-id hash capacity").into());
    }
    Ok(fee.retain(hash))
}
