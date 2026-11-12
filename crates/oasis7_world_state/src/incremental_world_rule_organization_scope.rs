//! Fixed metadata projection of already-validated selected organization bodies.
use super::*;
use crate::incremental_organization_scope_index::{
    OrganizationScopeFlagsV1, OrganizationScopeIndexBuilderV1, OrganizationScopeKindV1,
};
use crate::world_rules::{
    OrganizationDispositionPhaseV1 as Disposition, OrganizationProtectionPhaseV1 as Phase,
};
use oasis7_proto::memory_budget::{MemoryBudget, ReservedValue};
use std::io::{self, Write};
fn bad() -> ValueTreeError {
    ValueTreeError::Invalid("organization scope source")
}
fn limited() -> ValueTreeError {
    ValueTreeError::ResourceLimited("organization scope layout")
}
struct Capture {
    bytes: Vec<u8>,
    cap: usize,
    limited: bool,
}
impl Write for Capture {
    fn write(&mut self, b: &[u8]) -> io::Result<usize> {
        if self
            .bytes
            .len()
            .checked_add(b.len())
            .is_none_or(|n| n > self.cap)
        {
            self.limited = true;
            return Err(io::Error::other("organization outer capture bound"));
        }
        self.bytes.extend_from_slice(b);
        Ok(b.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
pub(super) fn spend(
    scan: &mut StateScanBudget,
    stop: &mut impl FnMut() -> bool,
    s: u64,
    b: u64,
) -> Result<(), ValueTreeError> {
    if stop() {
        return Err(ValueTreeError::Cancelled);
    }
    scan.nodes_remaining =
        scan.nodes_remaining
            .checked_sub(s)
            .ok_or(ValueTreeError::ResourceLimited(
                "organization resident metadata work",
            ))?;
    scan.read_bytes_remaining =
        scan.read_bytes_remaining
            .checked_sub(b)
            .ok_or(ValueTreeError::ResourceLimited(
                "organization resident metadata bytes",
            ))?;
    Ok(())
}
pub(super) fn original_outer<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    value: &[u8; 32],
    key: &WorldFieldKeyV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
) -> Result<Option<(OrganizationRootKeyV1<'static>, ReservedValue<String>)>, ValueTreeError> {
    let info = crate::incremental_field_member::read_member_record_header_v1(
        value,
        key,
        &mut ctx.values,
        MemberRecordLimitsV1 {
            semantic_bytes: ctx.semantic_bytes,
            payload_bytes: ctx.record_bytes,
            record_bytes: ctx.record_bytes,
        },
        &mut ctx.cancelled,
    )?;
    let cap = usize::try_from(info.semantic_bytes).map_err(|_| limited())?;
    let fee = ctx.values.memory.reserve(
        cap.checked_add(std::mem::size_of::<String>() + std::mem::size_of::<Capture>())
            .ok_or_else(limited)?,
        ctx.values.class,
    )?;
    let bytes = Vec::with_capacity(cap);
    if bytes.capacity() > cap {
        return Err(limited());
    }
    let mut output = Capture {
        bytes,
        cap,
        limited: false,
    };
    let root = ctx.values.load_root(value)?;
    let mut probe = family::outer_key::OuterProbe::new(&mut output);
    let result = ctx.values.read_range(
        &root,
        (FIELD_MEMBER_HEADER_BYTES + key.as_bytes().len()) as u64,
        info.semantic_bytes,
        &mut probe,
        &mut ctx.cancelled,
    );
    if let Some(reason) = probe.bad {
        return Err(ValueTreeError::Invalid(reason));
    }
    let owner = probe.finish();
    drop(probe);
    if output.limited {
        return Err(limited());
    }
    result?;
    let Some(owner) = owner? else {
        return Ok(None);
    };
    let id = String::from_utf8(output.bytes).map_err(|_| bad())?;
    Ok(Some((owner, fee.retain(id))))
}
/// Call only after the original bootstrap or changed closure validated this
/// exact child. Neither the builder nor the metadata provides that proof.
#[allow(clippy::too_many_arguments)]
pub(super) fn project<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    owner: OrganizationRootKeyV1<'_>,
    outer: &str,
    owner_digest: [u8; 32],
    body: &StateRootV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    scan: &mut StateScanBudget,
    cells: &mut u64,
    max: u64,
    builder: &mut OrganizationScopeIndexBuilderV1,
) -> Result<(), ValueTreeError> {
    let owner = match owner {
        OrganizationRootKeyV1::Case(_) => OrganizationRootKeyV1::Case(outer),
        OrganizationRootKeyV1::Estate(_) => OrganizationRootKeyV1::Estate(outer),
    };
    let proof = closure::from_family(
        owner,
        owner_digest,
        body,
        ctx.values.memory,
        ctx.values.class,
    )?;
    let cap = usize::try_from(max).map_err(|_| limited())?;
    let fee = ctx.values.memory.reserve(
        cap.checked_add(std::mem::size_of::<String>() + std::mem::size_of::<Capture>())
            .ok_or_else(limited)?,
        ctx.values.class,
    )?;
    let bytes = Vec::with_capacity(cap);
    if bytes.capacity() > cap {
        return Err(limited());
    }
    let mut output = Capture {
        bytes,
        cap,
        limited: false,
    };
    let record = match owner {
        OrganizationRootKeyV1::Case(_) => OrganizationRelativeRecordV1::Case,
        OrganizationRootKeyV1::Estate(_) => OrganizationRelativeRecordV1::Estate,
    };
    let mut local = WorldFieldReadContextV1 {
        root: body,
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
    let mut field = |field, output: &mut Capture| {
        *cells = cells.checked_sub(1).ok_or(ValueTreeError::ResourceLimited(
            "organization scope field cells",
        ))?;
        let result = read_qualified_organization_field_v1(
            OrganizationFieldRequestV1 {
                owner,
                record,
                field,
                qualification: &proof,
            },
            &mut local,
            max,
            output,
        );
        if output.limited {
            return Err(limited());
        }
        match result? {
            OrganizationFieldReadV1::Value(value) => Ok(value),
            _ => Err(bad()),
        }
    };
    if !matches!(
        field(OrganizationScalarFieldV1::OrganizationId, &mut output)?,
        OrganizationFieldValueV1::Text { .. }
    ) {
        return Err(bad());
    }
    let organization =
        fee.retain(String::from_utf8(std::mem::take(&mut output.bytes)).map_err(|_| bad())?);
    let mut sink = Capture {
        bytes: Vec::new(),
        cap: 0,
        limited: false,
    };
    let (kind, flags) = match owner {
        OrganizationRootKeyV1::Case(_) => {
            let OrganizationFieldValueV1::Phase(phase) =
                field(OrganizationScalarFieldV1::Phase, &mut sink)?
            else {
                return Err(bad());
            };
            let OrganizationFieldValueV1::Disposition(disposition) =
                field(OrganizationScalarFieldV1::Disposition, &mut sink)?
            else {
                return Err(bad());
            };
            (
                OrganizationScopeKindV1::Protection,
                OrganizationScopeFlagsV1 {
                    active_case: phase != Phase::Resolved,
                    risk_frozen: phase != Phase::Resolved
                        && (disposition != Disposition::None
                            || matches!(phase, Phase::Protected | Phase::Limited)),
                    undisposed_estate: false,
                },
            )
        }
        OrganizationRootKeyV1::Estate(_) => {
            let OrganizationFieldValueV1::Bool(disposed) =
                field(OrganizationScalarFieldV1::Disposed, &mut sink)?
            else {
                return Err(bad());
            };
            (
                OrganizationScopeKindV1::Estate,
                OrganizationScopeFlagsV1 {
                    active_case: false,
                    risk_frozen: false,
                    undisposed_estate: !disposed,
                },
            )
        }
    };
    drop(field);
    drop(local);
    builder.put(kind, outer, &organization, flags, &mut |s, b| {
        spend(scan, &mut ctx.cancelled, s, b)
    })
}
