//! Private diplomacy child: the original outer/closure/scalar codec is the only
//! parser. Metadata is resident selector DATA, never a business permission.
use super::*;
use crate::incremental_normal_gameplay_cycles_directory::{
    self as directory, GameplayDirectoryKindV1 as K, GameplayDirectoryRowV1,
    GameplayDirectoryWorkV1, GameplaySelectorRefV1 as S,
};
use oasis7_formal_protocol::incremental_state::StateEntryV1;
use std::sync::Arc;
fn scalar<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    field: DiplomacyFieldV1,
    id: &str,
    column: DiplomacyScalarV1,
    proof: &ReservedValue<DiplomacyRecordQualificationV1>,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    work: &mut GameplayDirectoryWorkV1,
) -> Result<RecordValueV1, ValueTreeError> {
    work.step(0, &mut ctx.cancelled)?;
    read_qualified_diplomacy_scalar_v1(
        id,
        field,
        column,
        proof,
        ctx,
        work.max_payload_bytes,
        &mut std::io::sink(),
    )?
    .ok_or_else(|| ValueTreeError::Invalid("gameplay selector scalar absent"))
}
fn text<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    field: DiplomacyFieldV1,
    id: &str,
    column: DiplomacyScalarV1,
    proof: &ReservedValue<DiplomacyRecordQualificationV1>,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    work: &mut GameplayDirectoryWorkV1,
) -> Result<ReservedValue<Vec<u8>>, ValueTreeError> {
    work.step(0, &mut ctx.cancelled)?;
    let cap = work.max_text_bytes;
    let fee = ctx.values.memory.reserve(cap, ctx.values.class)?;
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(cap)
        .map_err(|_| ValueTreeError::ResourceLimited("gameplay selector text"))?;
    if bytes.capacity() > cap {
        return Err(ValueTreeError::ResourceLimited(
            "gameplay selector text capacity",
        ));
    }
    let value =
        read_qualified_diplomacy_scalar_v1(id, field, column, proof, ctx, cap as u64, &mut bytes)?
            .ok_or_else(|| ValueTreeError::Invalid("gameplay selector text absent"))?;
    if value
        != (RecordValueV1::Text {
            utf8_bytes: bytes.len() as u64,
        })
    {
        return Err(ValueTreeError::Invalid("gameplay selector text shape"));
    }
    work.step(bytes.len(), &mut ctx.cancelled)?;
    Ok(fee.retain(bytes))
}
fn utf8(v: &[u8]) -> Result<&str, ValueTreeError> {
    std::str::from_utf8(v).map_err(|_| ValueTreeError::Invalid("original gameplay text"))
}
pub(crate) fn inspect_gameplay_diplomacy_directory_row_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    entry: &StateEntryV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    scan: &mut StateScanBudget,
    work: &mut GameplayDirectoryWorkV1,
) -> Result<Option<Arc<ReservedValue<GameplayDirectoryRowV1>>>, ValueTreeError> {
    let Some((field, owner, child)) = family::outer(entry, ctx)? else {
        return Ok(None);
    };
    if field == DiplomacyFieldV1::Alliances {
        return Ok(None);
    }
    let outer = directory::original_text(entry, ctx, work)?;
    let mut body = WorldFieldReadContextV1 {
        root: &child,
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
        cancelled: &mut ctx.cancelled,
    };
    let proof = qualify_diplomacy_body_v1(
        field,
        owner,
        &mut body,
        scan,
        &mut work.visits_remaining,
        work.max_payload_bytes,
    )?;
    let row = match field {
        DiplomacyFieldV1::Wars => {
            let inner = text(
                field,
                &outer,
                DiplomacyScalarV1::War(WarScalarV1::WarId),
                &proof,
                &mut body,
                work,
            )?;
            let RecordValueV1::Bool(active) = scalar(
                field,
                &outer,
                DiplomacyScalarV1::War(WarScalarV1::Active),
                &proof,
                &mut body,
                work,
            )?
            else {
                return Err(closure::invalid());
            };
            let RecordValueV1::Unsigned(declared_at) = scalar(
                field,
                &outer,
                DiplomacyScalarV1::War(WarScalarV1::DeclaredAt),
                &proof,
                &mut body,
                work,
            )?
            else {
                return Err(closure::invalid());
            };
            let RecordValueV1::Unsigned(max_duration_ticks) = scalar(
                field,
                &outer,
                DiplomacyScalarV1::War(WarScalarV1::MaxDurationTicks),
                &proof,
                &mut body,
                work,
            )?
            else {
                return Err(closure::invalid());
            };
            directory::retain_row(
                K::War,
                entry,
                &outer,
                S::War {
                    inner_id: utf8(&inner)?,
                    active,
                    declared_at,
                    max_duration_ticks,
                },
                body.values.memory,
                body.values.class,
                work,
                &mut body.cancelled,
            )?
        }
        DiplomacyFieldV1::Crises => {
            let status = text(
                field,
                &outer,
                DiplomacyScalarV1::Crisis(CrisisScalarV1::Status),
                &proof,
                &mut body,
                work,
            )?;
            let RecordValueV1::Unsigned(expires_at) = scalar(
                field,
                &outer,
                DiplomacyScalarV1::Crisis(CrisisScalarV1::ExpiresAt),
                &proof,
                &mut body,
                work,
            )?
            else {
                return Err(closure::invalid());
            };
            let RecordValueV1::Unsigned(severity) = scalar(
                field,
                &outer,
                DiplomacyScalarV1::Crisis(CrisisScalarV1::Severity),
                &proof,
                &mut body,
                work,
            )?
            else {
                return Err(closure::invalid());
            };
            directory::retain_row(
                K::Crisis,
                entry,
                &outer,
                S::Crisis {
                    active: utf8(&status)? == "active",
                    expires_at,
                    severity: u32::try_from(severity).map_err(|_| closure::invalid())?,
                },
                body.values.memory,
                body.values.class,
                work,
                &mut body.cancelled,
            )?
        }
        DiplomacyFieldV1::EconomicContracts => {
            let inner = text(
                field,
                &outer,
                DiplomacyScalarV1::Contract(ContractScalarV1::ContractId),
                &proof,
                &mut body,
                work,
            )?;
            let status = text(
                field,
                &outer,
                DiplomacyScalarV1::Contract(ContractScalarV1::Status),
                &proof,
                &mut body,
                work,
            )?;
            let fulfillment = text(
                field,
                &outer,
                DiplomacyScalarV1::Contract(ContractScalarV1::FulfillmentKind),
                &proof,
                &mut body,
                work,
            )?;
            let RecordValueV1::Unsigned(expires_at) = scalar(
                field,
                &outer,
                DiplomacyScalarV1::Contract(ContractScalarV1::ExpiresAt),
                &proof,
                &mut body,
                work,
            )?
            else {
                return Err(closure::invalid());
            };
            directory::retain_row(
                K::Contract,
                entry,
                &outer,
                S::Contract {
                    inner_id: utf8(&inner)?,
                    due_status: matches!(utf8(&status)?, "open" | "accepted"),
                    atomic: utf8(&fulfillment)? == "atomic_exchange",
                    expires_at,
                },
                body.values.memory,
                body.values.class,
                work,
                &mut body.cancelled,
            )?
        }
        DiplomacyFieldV1::Alliances => return Err(closure::invalid()),
    };
    body.check()?;
    Ok(Some(row))
}
