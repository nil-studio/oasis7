//! Exact owner-digest then actual top-level dispositions check. Its result is
//! data for the fixed terminal driver, never a complete execution capability.
use super::*;
use oasis7_formal_protocol::{
    controlled_authority::formal::bounded_context::FormalBoundedContextV1,
    incremental_state::StateScanBudget,
    incremental_value::{ValueNodeReader, ValueReadContext, ValueTreeError},
};
use oasis7_proto::memory_budget::MemoryClass;
use oasis7_world_state::{
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1},
    incremental_world_rule_composite::WorldRuleCompositeReadWorkV1,
    incremental_world_rule_composite_family::{
        CurrentIndustrialWorkV1, current_industrial_disposition_present_v1,
    },
};
fn limit() -> BodyMutationError {
    ValueTreeError::ResourceLimited("industrial terminal disposition work").into()
}
fn map(e: WorldFieldOverlayErrorV1) -> BodyMutationError {
    match e {
        WorldFieldOverlayErrorV1::Execution(e) => e.into(),
        WorldFieldOverlayErrorV1::Value(e) => e.into(),
        e => BodyMutationError::Original(Box::new(e)),
    }
}
#[allow(clippy::too_many_arguments)]
pub(super) fn check_current_disposition_v1<M, P, R, E, C>(
    source: &ReservedValue<CurrentIndustrialWorkV1>,
    root: &oasis7_agent_api::world_action::RootOperationV1,
    binding: Option<&ReservedValue<String>>,
    state: &P,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    scan: &mut StateScanBudget,
    typed: &mut WorldRuleCompositeReadWorkV1,
    common: &mut FormalBoundedContextV1<'_>,
    live: &ExecutionFence,
) -> Result<Option<ReservedValue<crate::WorldError>>, BodyMutationError>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    let error_fee = common
        .memory()
        .reserve(
            128 + std::mem::size_of::<ReservedValue<crate::WorldError>>(),
            MemoryClass::Ordinary,
        )
        .map_err(ValueTreeError::Memory)?;
    common
        .charge(
            1,
            u64::try_from(
                root.owner_binding_digest
                    .len()
                    .checked_add(binding.map_or(0, |v| v.len()))
                    .and_then(|n| n.checked_add(128))
                    .ok_or_else(limit)?,
            )
            .map_err(|_| limit())?,
        )
        .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
    let Some(binding) = binding else {
        return Ok(Some(error_fee.retain(
            crate::WorldError::ResourceBalanceInvalid {
                reason: "industrial_owner_authority_missing".into(),
            },
        )));
    };
    if !binding.uses_budget(common.memory()) {
        return Err(BodyMutationError::Invalid(
            "industrial owner digest paid pool",
        ));
    }
    if root.owner_binding_digest != **binding {
        return Ok(Some(error_fee.retain(
            crate::WorldError::ResourceBalanceInvalid {
                reason: "industrial_completion_authority_unavailable".into(),
            },
        )));
    }
    common
        .charge(
            1,
            u64::try_from(root.root_ref.id.len().checked_add(4096).ok_or_else(limit)?)
                .map_err(|_| limit())?,
        )
        .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
    let (_, meter, stop) = common.parts();
    let child = meter
        .begin_child()
        .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
    let steps = child.remaining_steps() / 5;
    let bytes = child.remaining_read_bytes() / 4;
    for n in [
        &mut ctx.values.io.nodes_remaining,
        &mut scan.nodes_remaining,
        &mut scan.leaves_remaining,
        &mut typed.json.cells_remaining,
        &mut typed.json.json_nodes_remaining,
    ] {
        *n = (*n).min(steps);
    }
    for n in [
        &mut ctx.values.io.read_bytes_remaining,
        &mut ctx.values.io.write_bytes_remaining,
        &mut ctx.key_work.bytes_remaining,
        &mut scan.read_bytes_remaining,
    ] {
        *n = (*n).min(bytes);
    }
    let before = (
        [
            ctx.values.io.nodes_remaining,
            scan.nodes_remaining,
            scan.leaves_remaining,
            typed.json.cells_remaining,
            typed.json.json_nodes_remaining,
        ],
        [
            ctx.values.io.read_bytes_remaining,
            ctx.values.io.write_bytes_remaining,
            ctx.key_work.bytes_remaining,
            scan.read_bytes_remaining,
        ],
    );
    let result = {
        let mut stop = || stop() || (ctx.cancelled)();
        let mut local = WorldFieldOverlayContextV1 {
            transaction: &mut *ctx.transaction,
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
            cancelled: &mut stop,
        };
        let guard = local.transaction.guard();
        current_industrial_disposition_present_v1(
            source,
            &root.root_ref.id,
            state,
            &mut local,
            scan,
            typed,
            &guard,
            live,
        )
        .map_err(map)
    };
    let after = (
        [
            ctx.values.io.nodes_remaining,
            scan.nodes_remaining,
            scan.leaves_remaining,
            typed.json.cells_remaining,
            typed.json.json_nodes_remaining,
        ],
        [
            ctx.values.io.read_bytes_remaining,
            ctx.values.io.write_bytes_remaining,
            ctx.key_work.bytes_remaining,
            scan.read_bytes_remaining,
        ],
    );
    let account = (|| {
        let mut s = 0u64;
        let mut b = 0u64;
        for (a, z) in before.0.into_iter().zip(after.0) {
            s = s
                .checked_add(a.checked_sub(z).ok_or_else(limit)?)
                .ok_or_else(limit)?;
        }
        for (a, z) in before.1.into_iter().zip(after.1) {
            b = b
                .checked_add(a.checked_sub(z).ok_or_else(limit)?)
                .ok_or_else(limit)?;
        }
        child
            .consume(s, b)
            .map_err(|e| BodyMutationError::Original(Box::new(e)))
    })();
    if let Err(e) = account {
        return match result {
            Err(original) => Err(original),
            Ok(_) => Err(e),
        };
    }
    child.finish();
    Ok(if result? {
        Some(error_fee.retain(crate::WorldError::ResourceBalanceInvalid {
            reason: "industrial_completion_authority_unavailable".into(),
        }))
    } else {
        None
    })
}
