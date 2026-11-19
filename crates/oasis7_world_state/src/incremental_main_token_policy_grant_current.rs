//! Selected actual Current grant and its original closed-record qualification.
//! This is DATA, not issuance authority or an epoch-execution capability.
use super::*;
use crate::{
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1, lookup},
    incremental_field_read::WorldFieldReadContextV1,
    incremental_inventory_value_read::InventoryValueReader,
};
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionFence, PreparedGuard, RootSetV1},
    incremental_state::{MerkleNodeReader, StateScanBudget},
    incremental_value::{ValueNodeReader, ValueReadContext},
};
use oasis7_proto::memory_budget::{MemoryClass, MemoryReservation, ReservedValue};
use std::io::{self, Write};
fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("Current restricted grant original")
}
fn limited() -> ValueTreeError {
    ValueTreeError::ResourceLimited("Current restricted grant")
}
pub struct CurrentRestrictedClaimGrantV1 {
    value: Option<RestrictedStarterClaimGrantState>,
    qualification: Option<ReservedValue<MainTokenPolicyRecordQualificationV1>>,
    _texts: Option<[MemoryReservation; 7]>,
}
impl CurrentRestrictedClaimGrantV1 {
    pub fn value(&self) -> Option<&RestrictedStarterClaimGrantState> {
        self.value.as_ref()
    }
    pub fn qualification(&self) -> Option<&ReservedValue<MainTokenPolicyRecordQualificationV1>> {
        self.qualification.as_ref()
    }
}
struct Output {
    bytes: Vec<u8>,
    max: usize,
}
impl Write for Output {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let end = self
            .bytes
            .len()
            .checked_add(bytes.len())
            .filter(|n| *n <= self.max)
            .ok_or_else(|| io::Error::other("grant text bound"))?;
        if end > self.bytes.capacity() {
            return Err(io::Error::other("grant text capacity"));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
#[expect(
    clippy::too_many_arguments,
    reason = "Bounded inputs are independently validated"
)]
fn cell<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    id: &str,
    q: &ReservedValue<MainTokenPolicyRecordQualificationV1>,
    column: MainTokenPolicyColumnV1,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    cells: &mut u64,
    max: u64,
    out: &mut impl Write,
    live: &ExecutionFence,
) -> Result<MainTokenPolicyValueV1, WorldFieldOverlayErrorV1> {
    *cells = cells.checked_sub(1).ok_or_else(limited)?;
    let guard = ctx.transaction.guard();
    read_qualified_main_token_policy_cell_overlay_v1(
        MainTokenPolicyCellRequestV1 {
            key: MainTokenPolicyKeyV1::Grant(id),
            cell: MainTokenPolicyCellV1::Column(column),
            qualification: q,
        },
        ctx,
        max,
        out,
        &guard,
        live,
    )
}
#[expect(
    clippy::too_many_arguments,
    reason = "Bounded inputs are independently validated"
)]
fn text<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    id: &str,
    q: &ReservedValue<MainTokenPolicyRecordQualificationV1>,
    column: MainTokenPolicyColumnV1,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    cells: &mut u64,
    max: u64,
    optional: bool,
    live: &ExecutionFence,
) -> Result<(Option<String>, MemoryReservation), WorldFieldOverlayErrorV1> {
    let measured = cell(id, q, column, ctx, cells, max, &mut io::sink(), live)?;
    if measured == MainTokenPolicyValueV1::Unit && optional {
        return Ok((
            None,
            ctx.values
                .memory
                .reserve(0, MemoryClass::Ordinary)
                .map_err(ValueTreeError::Memory)?,
        ));
    }
    let MainTokenPolicyValueV1::Text { utf8_bytes } = measured else {
        return Err(invalid().into());
    };
    let n = usize::try_from(utf8_bytes).map_err(|_| limited())?;
    let fee = ctx
        .values
        .memory
        .reserve(n, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let mut bytes = Vec::new();
    bytes.try_reserve_exact(n).map_err(|_| limited())?;
    if bytes.capacity() > n {
        return Err(limited().into());
    }
    let mut out = Output { bytes, max: n };
    let second = cell(id, q, column, ctx, cells, max, &mut out, live)?;
    if second != measured || out.bytes.len() != n {
        return Err(invalid().into());
    }
    Ok((
        Some(String::from_utf8(out.bytes).map_err(|_| invalid())?),
        fee,
    ))
}
fn number<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    id: &str,
    q: &ReservedValue<MainTokenPolicyRecordQualificationV1>,
    column: MainTokenPolicyColumnV1,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    cells: &mut u64,
    optional: bool,
    live: &ExecutionFence,
) -> Result<Option<u64>, WorldFieldOverlayErrorV1> {
    match cell(id, q, column, ctx, cells, 8, &mut io::sink(), live)? {
        MainTokenPolicyValueV1::Unsigned(n) => Ok(Some(n)),
        MainTokenPolicyValueV1::Unit if optional => Ok(None),
        _ => Err(invalid().into()),
    }
}
#[allow(clippy::too_many_arguments)]
pub fn read_current_restricted_claim_grant_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    id: &str,
    parent: &ReservedValue<MainTokenPolicyFamilyQualificationV1>,
    roots: &RootSetV1,
    state: &M,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    scan: &mut StateScanBudget,
    cells: &mut u64,
    max_payload: u64,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<ReservedValue<CurrentRestrictedClaimGrantV1>, WorldFieldOverlayErrorV1> {
    let result = (|| {
        ctx.transaction.validate_guard(guard, live)?;
        if (ctx.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        if state.allocation_class() != Some(ctx.values.class)
            || !state
                .allocation_budget()
                .is_some_and(|pool| pool.same_pool(ctx.values.memory))
        {
            return Err(invalid().into());
        }
        if ctx.values.class != MemoryClass::Ordinary
            || !parent.uses_budget(ctx.values.memory)
            || parent.allocation_class() != MemoryClass::Ordinary
            || !ctx.transaction.uses_budget(ctx.values.memory)
        {
            return Err(invalid().into());
        }
        let digest = roots.digest().map_err(|_| invalid())?;
        parent.validate_binding(&digest)?;
        if digest != ctx.transaction.parent_fence().parent_roots {
            return Err(invalid().into());
        }
        let retained = ctx
            .values
            .memory
            .reserve(
                std::mem::size_of::<CurrentRestrictedClaimGrantV1>(),
                MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)?;
        let key = MainTokenPolicyKeyV1::Grant(id);
        let resolved = {
            let mut records = InventoryValueReader {
                values: &mut ctx.values,
                key_work: &mut *ctx.key_work,
                semantic_bytes: ctx.semantic_bytes,
                record_bytes: ctx.record_bytes,
                cancelled: &mut ctx.cancelled,
            };
            point::metadata(
                key,
                ctx.transaction,
                &mut records,
                &mut ctx.take_state_error,
            )?;
            let outer = read::outer_key(key, &mut records)?;
            match lookup(
                ctx.transaction,
                &outer.0,
                &mut records,
                &mut ctx.take_state_error,
            )? {
                None => None,
                Some(root) => {
                    let child =
                        read::outer(MainTokenPolicyKindV1::Grant, &outer, &root, &mut records)?;
                    let owner = WorldFieldKeyV1::from_bytes(outer.0.as_bytes())
                        .map_err(ValueTreeError::Invalid)?
                        .semantic_key
                        .ok_or_else(invalid)?;
                    Some((child, owner))
                }
            }
        };
        let Some((child, owner)) = resolved else {
            return Ok(retained.retain(CurrentRestrictedClaimGrantV1 {
                value: None,
                qualification: None,
                _texts: None,
            }));
        };
        let qualification = {
            let mut read = WorldFieldReadContextV1 {
                root: &child,
                state,
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
                cancelled: &mut ctx.cancelled,
            };
            qualify_main_token_policy_body_v1(
                MainTokenPolicyKindV1::Grant,
                owner,
                &mut read,
                scan,
                cells,
                max_payload,
            )?
        };
        use MainTokenPolicyColumnV1::*;
        let (beneficiary, a) = text(
            id,
            &qualification,
            BeneficiaryAccountId,
            ctx,
            cells,
            max_payload,
            false,
            live,
        )?;
        let (issuer, b) = text(
            id,
            &qualification,
            IssuerId,
            ctx,
            cells,
            max_payload,
            false,
            live,
        )?;
        let (reason, c) = text(
            id,
            &qualification,
            IssuanceReason,
            ctx,
            cells,
            max_payload,
            false,
            live,
        )?;
        let (scope, d) = text(
            id,
            &qualification,
            SpendScope,
            ctx,
            cells,
            max_payload,
            false,
            live,
        )?;
        let (bucket, e) = text(
            id,
            &qualification,
            SourceTreasuryBucketId,
            ctx,
            cells,
            max_payload,
            false,
            live,
        )?;
        let amount = number(id, &qualification, IssuedAmount, ctx, cells, false, live)?
            .ok_or_else(invalid)?;
        let issued = number(id, &qualification, IssuedAtEpoch, ctx, cells, false, live)?
            .ok_or_else(invalid)?;
        let expires = number(id, &qualification, ExpiresAtEpoch, ctx, cells, false, live)?
            .ok_or_else(invalid)?;
        let (status, f) = text(
            id,
            &qualification,
            Status,
            ctx,
            cells,
            max_payload,
            false,
            live,
        )?;
        let status = match status.as_deref() {
            Some("issued") => RestrictedStarterClaimGrantStatus::Issued,
            Some("expired") => RestrictedStarterClaimGrantStatus::Expired,
            Some("revoked") => RestrictedStarterClaimGrantStatus::Revoked,
            _ => return Err(invalid().into()),
        };
        let updated = number(
            id,
            &qualification,
            StatusUpdatedAtEpoch,
            ctx,
            cells,
            true,
            live,
        )?;
        let (status_reason, g) = text(
            id,
            &qualification,
            StatusReason,
            ctx,
            cells,
            max_payload,
            true,
            live,
        )?;
        if (ctx.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        if state.allocation_class() != Some(ctx.values.class)
            || !state
                .allocation_budget()
                .is_some_and(|pool| pool.same_pool(ctx.values.memory))
        {
            return Err(invalid().into());
        }
        ctx.transaction
            .validate_guard(&ctx.transaction.guard(), live)?;
        Ok(retained.retain(CurrentRestrictedClaimGrantV1 {
            value: Some(RestrictedStarterClaimGrantState {
                beneficiary_account_id: beneficiary.ok_or_else(invalid)?,
                issuer_id: issuer.ok_or_else(invalid)?,
                issuance_reason: reason.ok_or_else(invalid)?,
                spend_scope: scope.ok_or_else(invalid)?,
                source_treasury_bucket_id: bucket.ok_or_else(invalid)?,
                issued_amount: amount,
                issued_at_epoch: issued,
                expires_at_epoch: expires,
                status,
                status_updated_at_epoch: updated,
                status_reason,
            }),
            qualification: Some(qualification),
            _texts: Some([a, b, c, d, e, f, g]),
        }))
    })();
    if result.is_err() {
        let _: Result<(), oasis7_formal_protocol::incremental_execution::ExecutionError> =
            ctx.transaction.contribute(|_| {
                Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted)
            });
    }
    result
}
