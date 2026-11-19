//! Typed point reads of the actual current module capability/policy owners.
//! No complete capability map or policy vector is materialized.
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionFence, PreparedGuard},
    incremental_state::MerkleNodeReader,
    incremental_value::{ValueNodeReader, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryClass, ReservedValue};
use oasis7_world_state::{
    effect::CapabilityGrant,
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1},
    incremental_runtime_module_authorization::*,
};
use std::io::{self, Write};
fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("root module authorization point")
}
struct Output {
    bytes: Vec<u8>,
    max: usize,
}
impl Write for Output {
    fn write(&mut self, b: &[u8]) -> io::Result<usize> {
        if b.len() > self.max.saturating_sub(self.bytes.len()) {
            return Err(io::Error::other("module authorization text bound"));
        }
        self.bytes.extend_from_slice(b);
        Ok(b.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
pub struct RootModuleAuthorizationLimitsV1 {
    pub max_text_bytes: usize,
    pub max_effect_kinds: usize,
}
struct Cell {
    value: ReservedValue<ModuleAuthorizationValueV1>,
    bytes: Vec<u8>,
}
#[expect(
    clippy::too_many_arguments,
    reason = "Point qualification, real fence and continuous read work remain explicit."
)]
fn cell<M, R, E, C>(
    key: ModuleAuthorizationRecordKeyV1<'_>,
    column: &'static str,
    index: Option<u64>,
    family: &ReservedValue<ModuleAuthorizationFamilyQualificationV1>,
    body: &ReservedValue<ModuleAuthorizationBodyQualificationV1>,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    work: &mut ModuleAuthorizationReadWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
    max: usize,
) -> Result<Cell, WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    // Reads themselves advance transaction revision. The caller's genuine
    // guard still fixes owner/fence; each point uses the actual current guard,
    // while the original reader rechecks exact current body root/owner/schema.
    ctx.transaction.validate_transaction_identity(guard)?;
    let point_guard = ctx.transaction.guard();
    let mut output = Output {
        bytes: Vec::with_capacity(max),
        max,
    };
    let value = read_module_authorization_overlay_v1(
        ModuleAuthorizationCellRequestV1 {
            key,
            address: ModuleAuthorizationAddressV1 { column, index },
            family,
            body,
            max_payload_bytes: max as u64,
        },
        ctx,
        work,
        &point_guard,
        live,
        &mut output,
    )?;
    Ok(Cell {
        value,
        bytes: output.bytes,
    })
}
fn text(cell: Cell) -> Result<String, ValueTreeError> {
    match *cell.value {
        ModuleAuthorizationValueV1::Text { utf8_bytes }
            if utf8_bytes == cell.bytes.len() as u64 =>
        {
            String::from_utf8(cell.bytes).map_err(|_| invalid())
        }
        ModuleAuthorizationValueV1::Label(label) if cell.bytes.is_empty() => Ok(label.to_owned()),
        _ => Err(invalid()),
    }
}
/// Returns a charged actual grant, never an authorization boolean. Its existing
/// business `allows`/`is_expired` kernel remains the sole capability policy.
pub fn read_root_module_capability_v1<M, R, E, C>(
    cap_ref: &str,
    family: &ReservedValue<ModuleAuthorizationFamilyQualificationV1>,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    work: &mut ModuleAuthorizationReadWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
    limits: &RootModuleAuthorizationLimitsV1,
) -> Result<Option<ReservedValue<CapabilityGrant>>, WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    ctx.transaction.validate_transaction_identity(guard)?;
    ctx.transaction
        .validate_guard(&ctx.transaction.guard(), live)?;
    if ctx.values.class != MemoryClass::Ordinary {
        return Err(invalid().into());
    }
    // Admission covers one reusable cell buffer, the returned String+Vec and
    // every retained kind String. Fixed labels are at most 16 bytes.
    let width = limits.max_text_bytes.max(16);
    let bytes = limits
        .max_effect_kinds
        .checked_add(2)
        .and_then(|n| n.checked_mul(width.checked_add(std::mem::size_of::<String>())?))
        .and_then(|n| n.checked_add(std::mem::size_of::<CapabilityGrant>() + 512))
        .ok_or(ValueTreeError::ResourceLimited(
            "module capability point heap",
        ))?;
    let charge = ctx
        .values
        .memory
        .reserve(bytes, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let key = ModuleAuthorizationRecordKeyV1::Grant(cap_ref);
    let Some(body) = open_module_authorization_body_overlay_v1(key, family, ctx)? else {
        return Ok(None);
    };
    let name = text(cell(
        key,
        "name",
        None,
        family,
        &body,
        ctx,
        work,
        guard,
        live,
        limits.max_text_bytes,
    )?)?;
    let expiry = cell(
        key,
        "expiry",
        None,
        family,
        &body,
        ctx,
        work,
        guard,
        live,
        limits.max_text_bytes,
    )?;
    if !expiry.bytes.is_empty() {
        return Err(invalid().into());
    }
    let expiry = match *expiry.value {
        ModuleAuthorizationValueV1::Unit => None,
        ModuleAuthorizationValueV1::Unsigned(n) => Some(n),
        _ => return Err(invalid().into()),
    };
    let length = cell(
        key,
        "effect_kinds.length",
        None,
        family,
        &body,
        ctx,
        work,
        guard,
        live,
        limits.max_text_bytes,
    )?;
    if !length.bytes.is_empty() {
        return Err(invalid().into());
    }
    let ModuleAuthorizationValueV1::Unsigned(n) = *length.value else {
        return Err(invalid().into());
    };
    let n = usize::try_from(n)
        .map_err(|_| ValueTreeError::ResourceLimited("module capability kind count"))?;
    if n > limits.max_effect_kinds {
        return Err(ValueTreeError::ResourceLimited("module capability kind count").into());
    }
    let mut effect_kinds = Vec::with_capacity(n);
    for i in 0..n {
        effect_kinds.push(text(cell(
            key,
            "effect_kinds",
            Some(i as u64),
            family,
            &body,
            ctx,
            work,
            guard,
            live,
            limits.max_text_bytes,
        )?)?);
    }
    if (ctx.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok(Some(charge.retain(CapabilityGrant {
        name,
        effect_kinds,
        expiry,
    })))
}

fn optional_text(cell: Cell) -> Result<Option<String>, ValueTreeError> {
    if matches!(*cell.value, ModuleAuthorizationValueV1::Unit) {
        if !cell.bytes.is_empty() {
            return Err(invalid());
        }
        Ok(None)
    } else {
        text(cell).map(Some)
    }
}
/// Original ordered first-match policy semantics, reading one actual rule at a
/// time. The underlying rule/length/body qualification remains authoritative;
/// no supplied rule DTO or policy decision boolean can substitute for it.
#[expect(
    clippy::too_many_arguments,
    reason = "Actual rule owner, intent, transaction fence and finite reader work stay visible."
)]
pub fn decide_root_module_effect_v1<M, R, E, C>(
    intent: &oasis7_world_state::effect::EffectIntent,
    family: &ReservedValue<ModuleAuthorizationFamilyQualificationV1>,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    work: &mut ModuleAuthorizationReadWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
    max_text_bytes: usize,
    max_rules: u64,
) -> Result<ReservedValue<oasis7_world_state::policy::PolicyDecision>, WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    use oasis7_world_state::{
        effect::OriginKind,
        policy::{PolicyDecision, PolicyWhen},
    };
    ctx.transaction.validate_transaction_identity(guard)?;
    ctx.transaction
        .validate_guard(&ctx.transaction.guard(), live)?;
    if ctx.values.class != MemoryClass::Ordinary {
        return Err(invalid().into());
    }
    let heap = max_text_bytes
        .max(32)
        .checked_mul(8)
        .and_then(|n| n.checked_add(1024))
        .ok_or(ValueTreeError::ResourceLimited("module policy point heap"))?;
    let result_charge = ctx
        .values
        .memory
        .reserve(
            max_text_bytes.max(32).checked_add(256).ok_or(invalid())?,
            MemoryClass::Ordinary,
        )
        .map_err(ValueTreeError::Memory)?;
    let length = read_module_policy_length_overlay_v1(family, ctx)?;
    if length > max_rules {
        return Err(ValueTreeError::ResourceLimited("module policy rule visits").into());
    }
    for ordinal in 0..length {
        let _phase = ctx
            .values
            .memory
            .reserve(heap, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let key = ModuleAuthorizationRecordKeyV1::Policy(ordinal);
        let body = open_module_authorization_body_overlay_v1(key, family, ctx)?.ok_or(invalid())?;
        let effect_kind = optional_text(cell(
            key,
            "when.effect_kind",
            None,
            family,
            &body,
            ctx,
            work,
            guard,
            live,
            max_text_bytes,
        )?)?;
        let origin_kind = optional_text(cell(
            key,
            "when.origin_kind",
            None,
            family,
            &body,
            ctx,
            work,
            guard,
            live,
            max_text_bytes,
        )?)?;
        let origin_kind = match origin_kind.as_deref() {
            None => None,
            Some("reducer") => Some(OriginKind::Reducer),
            Some("plan") => Some(OriginKind::Plan),
            Some("module") => Some(OriginKind::Module),
            Some("system") => Some(OriginKind::System),
            _ => return Err(invalid().into()),
        };
        let cap_name = optional_text(cell(
            key,
            "when.cap_name",
            None,
            family,
            &body,
            ctx,
            work,
            guard,
            live,
            max_text_bytes,
        )?)?;
        let when = PolicyWhen {
            effect_kind,
            origin_kind,
            cap_name,
        };
        // Read all fields of each visited rule, even when it does not match;
        // exact owner/schema must not hide invalid decision relations.
        let decision = text(cell(
            key,
            "decision.kind",
            None,
            family,
            &body,
            ctx,
            work,
            guard,
            live,
            max_text_bytes,
        )?)?;
        let reason = optional_text(cell(
            key,
            "decision.reason",
            None,
            family,
            &body,
            ctx,
            work,
            guard,
            live,
            max_text_bytes,
        )?)?;
        let decision = match (decision.as_str(), reason) {
            ("allow", None) => PolicyDecision::Allow,
            ("deny", Some(reason)) => PolicyDecision::Deny { reason },
            _ => return Err(invalid().into()),
        };
        if (ctx.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        if when.matches(intent) {
            return Ok(result_charge.retain(decision));
        }
    }
    if (ctx.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok(result_charge.retain(PolicyDecision::Deny {
        reason: "default_deny".to_owned(),
    }))
}
