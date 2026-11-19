//! Complete fixed seven-source DATA comparison. Accepted-head qualification
//! is intentionally a separate private owner step, never a caller assertion.
use super::read::{AuthorizationRootSourceReadWorkV1, SourceError};
use super::*;
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionFence, RootSetV1},
    incremental_state::MerkleNodeReader,
    incremental_value::ValueNodeReader,
};
use oasis7_world_state::{
    incremental_field_overlay::WorldFieldOverlayContextV1,
    incremental_field_read::WorldFieldReadContextV1,
    incremental_runtime_capability_records::{
        RuntimeCapabilityRecordKeyV1, RuntimeCapabilityRecordKindV1,
    },
    incremental_runtime_revocation::RuntimeRevocationCurrentV1,
    incremental_runtime_typed_complete::RuntimeTypedCompleteQualificationV1,
};
#[allow(clippy::too_many_arguments)]
pub(super) fn compare_seven_current<M, R, E, C, V, ER, CR>(
    data: &ProvisionalAuthorizationProjectionV1,
    qualified: &ReservedValue<RuntimeTypedCompleteQualificationV1>,
    revocation: &RuntimeRevocationCurrentV1,
    roots: &RootSetV1,
    overlay: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    parent: &mut WorldFieldReadContextV1<'_, M, V, ER, CR>,
    work: &mut AuthorizationRootSourceReadWorkV1,
    common: &mut FormalBoundedContextV1<'_>,
    live: &ExecutionFence,
) -> Result<(), SourceError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    V: ValueNodeReader,
    ER: FnMut() -> Option<ValueTreeError>,
    CR: FnMut() -> bool,
{
    data.check_pool(common)?;
    if data.class != MemoryClass::Ordinary
        || qualified.allocation_class() != MemoryClass::Ordinary
        || !qualified.uses_budget(common.memory())
    {
        return Err(invalid("authorization source pool/class").into());
    }
    sources::require_parent_authorization_unchanged(
        overlay.transaction,
        &mut work.pending_entries,
        common,
    )?;
    // The metadata debit precedes the actual Q digest validation. No raw RootSet
    // or seven naked DTOs can replace this already-qualified complete family.
    read::metadata(roots, common)?;
    qualified.validate_binding(roots)?;
    let rev = data
        .at(
            &[AuthorizationProjectionPathV1::Field("revocation")],
            common,
        )?
        .ok_or_else(|| invalid("authorization revocation absent"))?;
    sources::authority(
        qualified.revocation(),
        revocation,
        roots,
        &rev,
        data.limits,
        overlay,
        parent,
        work,
        common,
        live,
    )?;
    let caps = qualified.capabilities().record_counts();
    for (name, count, kind) in [
        ("grants", qualified.grants().entries(), None),
        (
            "receipts",
            qualified.authorization_records().audit_receipts(),
            None,
        ),
        (
            "invocation_contexts",
            qualified.authorization_records().invocation_contexts(),
            None,
        ),
        (
            "nonce_records",
            caps[0],
            Some(RuntimeCapabilityRecordKindV1::Nonce),
        ),
        (
            "budget_accounts",
            caps[1],
            Some(RuntimeCapabilityRecordKindV1::Budget),
        ),
        (
            "effect_receipt_links",
            caps[2],
            Some(RuntimeCapabilityRecordKindV1::EffectLink),
        ),
    ] {
        let map = data
            .at(&[AuthorizationProjectionPathV1::Field(name)], common)?
            .ok_or_else(|| invalid("authorization source map absent"))?;
        let CanonicalValueV1::Map { len, entries } = &**map else {
            return Err(invalid("authorization source map domain").into());
        };
        if *len != count {
            return Err(invalid("authorization source count closure").into());
        }
        let mut seen = 0u64;
        tree::visit::<SourceError>(entries, common, &mut |key, value, common| {
            let key = cbor::key_text(key)?;
            if let Some(kind) = kind {
                sources::capability(
                    qualified.capabilities(),
                    roots,
                    RuntimeCapabilityRecordKeyV1 { kind, id: key },
                    value,
                    data.limits,
                    overlay,
                    parent,
                    work,
                    common,
                    live,
                )?;
            } else {
                match name {
                    "grants" => sources::grant(
                        qualified.grants(),
                        roots,
                        key,
                        value,
                        data.limits,
                        overlay,
                        parent,
                        work,
                        common,
                        live,
                    )?,
                    "receipts" => sources::audit(
                        qualified.authorization_records(),
                        roots,
                        key,
                        value,
                        data.limits,
                        overlay,
                        parent,
                        work,
                        common,
                        live,
                    )?,
                    "invocation_contexts" => sources::invocation(
                        qualified.authorization_records(),
                        roots,
                        key,
                        value,
                        data.limits,
                        overlay,
                        parent,
                        work,
                        common,
                        live,
                    )?,
                    _ => return Err(invalid("authorization fixed source dispatch").into()),
                }
            }
            seen = seen.checked_add(1).ok_or_else(limited)?;
            Ok(())
        })?;
        if seen != count {
            return Err(invalid("authorization visited count closure").into());
        }
    }
    sources::require_parent_authorization_unchanged(
        overlay.transaction,
        &mut work.pending_entries,
        common,
    )?;
    let guard = overlay.transaction.guard();
    overlay
        .transaction
        .validate_guard(&guard, live)
        .map_err(SourceError::Execution)?;
    common.consume_steps(0)?;
    Ok(())
}
