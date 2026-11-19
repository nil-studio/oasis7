//! Original industrial-root predicates over actual Current selected roots.
//! Neither the resident reverse index nor this data result is a terminal cap.
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
    incremental_world_rule_composite_family::*,
};
use std::cell::RefCell;
fn limited() -> BodyMutationError {
    ValueTreeError::ResourceLimited("industrial terminal graph work").into()
}
fn map(error: WorldFieldOverlayErrorV1) -> BodyMutationError {
    match error {
        WorldFieldOverlayErrorV1::Execution(e) => e.into(),
        WorldFieldOverlayErrorV1::Value(e) => e.into(),
        e => BodyMutationError::Original(Box::new(e)),
    }
}
pub(super) enum SelectedIndustrialRootV1 {
    Unlinked,
    Linked {
        source: ReservedValue<CurrentIndustrialWorkV1>,
        root: oasis7_world_state::incremental_world_rule_composite::RecoveredWorldRuleCompositeV1<
            oasis7_agent_api::world_action::RootOperationV1,
        >,
    },
    Rejected(crate::WorldError, MemoryReservation),
}
fn invalid(reason: &str) -> crate::WorldError {
    crate::WorldError::ResourceBalanceInvalid {
        reason: reason.into(),
    }
}
// The six scalar step lanes and three byte lanes below are actual caller
// counters. No mirror meter or copied balance is used to execute the reads.
#[allow(clippy::too_many_arguments)]
pub(super) fn select_industrial_root_v1<M, P, R, E, C>(
    job: &crate::RecipeJobState,
    parent_q: &ReservedValue<
        oasis7_world_state::incremental_world_rules_family::WorldRulesQualificationV1,
    >,
    roots: &oasis7_formal_protocol::incremental_execution::RootSetV1,
    state: &P,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    scan: &mut StateScanBudget,
    typed: &mut WorldRuleCompositeReadWorkV1,
    common: &mut FormalBoundedContextV1<'_>,
    live: &ExecutionFence,
) -> Result<SelectedIndustrialRootV1, BodyMutationError>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    if !ctx.values.memory.same_pool(common.memory()) || ctx.values.class != MemoryClass::Ordinary {
        return Err(BodyMutationError::Invalid("industrial graph Common pool"));
    }
    let metadata = roots
        .world_id
        .len()
        .checked_add(4096)
        .and_then(|n| n.checked_mul(5))
        .ok_or_else(limited)?;
    common
        .charge(5, u64::try_from(metadata).map_err(|_| limited())?)
        .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
    let error_bytes = 128 + std::mem::size_of::<crate::WorldError>();
    common
        .charge(1, u64::try_from(error_bytes).map_err(|_| limited())?)
        .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
    let error_fee = common
        .memory()
        .reserve(error_bytes, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let (_, meter, stop) = common.parts();
    let child = meter
        .begin_child()
        .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
    let steps = child.remaining_steps() / 5;
    let bytes = child.remaining_read_bytes() / 4;
    for value in [
        &mut ctx.values.io.nodes_remaining,
        &mut scan.nodes_remaining,
        &mut scan.leaves_remaining,
        &mut typed.json.cells_remaining,
        &mut typed.json.json_nodes_remaining,
    ] {
        *value = (*value).min(steps);
    }
    for value in [
        &mut ctx.values.io.read_bytes_remaining,
        &mut ctx.values.io.write_bytes_remaining,
        &mut ctx.key_work.bytes_remaining,
        &mut scan.read_bytes_remaining,
    ] {
        *value = (*value).min(bytes);
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
        let cancelled = RefCell::new(stop);
        let mut stop = || (*cancelled.borrow_mut())() || (ctx.cancelled)();
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
        (|| {
            let guard = local.transaction.guard();
            let source = bind_current_industrial_work_directory_v1(
                parent_q.industrial(),
                roots,
                state,
                &mut local,
                scan,
                typed,
                &guard,
                live,
            )
            .map_err(map)?;
            let Some(requested) = &job.industrial_root_ref else {
                let linked = source.directory().any_started_job_v1(
                    job.job_id,
                    local.values.memory,
                    MemoryClass::Ordinary,
                    scan,
                    &mut local.cancelled,
                )?;
                return Ok(if linked {
                    SelectedIndustrialRootV1::Rejected(
                        invalid("industrial_completion_job_link_missing"),
                        error_fee,
                    )
                } else {
                    SelectedIndustrialRootV1::Unlinked
                });
            };
            if !source.branch_present() {
                return Ok(SelectedIndustrialRootV1::Rejected(
                    invalid("industrial_completion_graph_missing"),
                    error_fee,
                ));
            }
            let selected = read_current_industrial_completion_root_v1(
                &source,
                &requested.id,
                state,
                &mut local,
                scan,
                typed,
                &guard,
                live,
            )
            .map_err(map)?;
            let Some(selected) = selected else {
                return Ok(SelectedIndustrialRootV1::Rejected(
                    invalid("industrial_completion_root_missing"),
                    error_fee,
                ));
            };
            let root = selected.value();
            // Charge all actual selected DTO strings/slots rather than a fixed
            // assumed ID width. The original serializer streams only into a debit
            // sink; it does not allocate or construct a replacement wire object.
            struct Debit<'a, F> {
                charge: &'a mut F,
            }
            impl<F: FnMut(usize) -> Result<(), BodyMutationError>> std::io::Write for Debit<'_, F> {
                fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
                    (self.charge)(b.len())
                        .map_err(|_| std::io::Error::other("industrial comparison debit"))?;
                    Ok(b.len())
                }
                fn flush(&mut self) -> std::io::Result<()> {
                    Ok(())
                }
            }
            let mut first = None;
            let mut debit =
                |n: usize| match child.consume(1, u64::try_from(n).map_err(|_| limited())?) {
                    Ok(()) => Ok(()),
                    Err(e) => {
                        first = Some(BodyMutationError::Original(Box::new(e)));
                        Err(limited())
                    }
                };
            let measured =
                serde_json::to_writer(Debit { charge: &mut debit }, &(root, requested, job));
            drop(debit);
            if let Some(error) = first {
                return Err(error);
            }
            if measured.is_err() {
                return Err(limited());
            }
            if root.root_ref != *requested || root.started_job_id != Some(job.job_id) {
                return Ok(SelectedIndustrialRootV1::Rejected(
                    invalid("industrial_completion_root_missing"),
                    error_fee,
                ));
            }
            if source.directory().has_conflicting_started_job_v1(
                job.job_id,
                requested,
                local.values.memory,
                MemoryClass::Ordinary,
                scan,
                &mut local.cancelled,
            )? {
                return Ok(SelectedIndustrialRootV1::Rejected(
                    invalid("industrial_completion_job_identity_conflict"),
                    error_fee,
                ));
            }
            child
                .consume(u64::try_from(root.slots.len()).map_err(|_| limited())?, 0)
                .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
            if let Err(e) =
                oasis7_world_state::world_rules::validate_industrial_completion_selected_fields_v1(
                    root, job,
                )
            {
                return Ok(SelectedIndustrialRootV1::Rejected(e, error_fee));
            }
            Ok(SelectedIndustrialRootV1::Linked {
                source,
                root: selected,
            })
        })()
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
    let accounted = (|| {
        let mut s = 0u64;
        let mut b = 0u64;
        for (a, z) in before.0.into_iter().zip(after.0) {
            s = s
                .checked_add(a.checked_sub(z).ok_or_else(limited)?)
                .ok_or_else(limited)?;
        }
        for (a, z) in before.1.into_iter().zip(after.1) {
            b = b
                .checked_add(a.checked_sub(z).ok_or_else(limited)?)
                .ok_or_else(limited)?;
        }
        child
            .consume(s, b)
            .map_err(|e| BodyMutationError::Original(Box::new(e)))
    })();
    if let Err(e) = accounted {
        return match result {
            Err(original) => Err(original),
            Ok(_) => Err(e),
        };
    }
    child.finish();
    result
}
