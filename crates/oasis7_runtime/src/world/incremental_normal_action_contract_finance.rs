//! Original atomic-exchange financial projection on actual Current points.
//! No event can be minted by this private data plan.
use super::*;
use crate::world::incremental_step::IwsStepContext;
use oasis7_agent_api::world_action::ResourceKind;
use oasis7_formal_protocol::incremental_value::ValueNodeWriter;
use oasis7_world_state::{
    incremental_agent_fields::{AgentFieldV1, AgentScalarRefV1},
    incremental_agent_subtree::*,
    incremental_field_member::MemberRecordWriteContext,
    incremental_inventory_resource_overlay::{
        change_current_world_resource_v1, read_current_world_resource_v1,
    },
    incremental_value_mutation::ValueMutationContextV1,
    incremental_world_number_maps::*,
};
#[derive(Clone, Copy)]
struct Number {
    before: Option<WorldNumberV1>,
    after: Option<WorldNumberV1>,
}
#[derive(Clone, Copy)]
struct Resource {
    before: Option<i64>,
    after: i64,
}
#[derive(Clone, Copy)]
struct Window {
    start: Number,
    total: Number,
    write_start: bool,
    write_total: bool,
}
pub(super) struct FinancialPlanV1 {
    creator: Resource,
    counterparty: Option<Resource>,
    treasury: Resource,
    reputation: [Option<Number>; 2],
    window: [Window; 2],
    pair: String,
    pair_before: Option<WorldNumberV1>,
    now: u64,
}
fn number<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    field: WorldNumberMapFieldV1,
    id: &str,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<Option<WorldNumberV1>, BodyMutationError> {
    charge(common, id.len())?;
    read_world_number_map_overlay_v1(field, WorldNumberMapKeyRefV1::Text(id), context)
        .map_err(overlay)
}
fn as_signed(n: Option<WorldNumberV1>) -> Result<i64, BodyMutationError> {
    match n {
        None => Ok(0),
        Some(WorldNumberV1::Signed(n)) => Ok(n),
        _ => Err(bad()),
    }
}
fn as_unsigned(n: Option<WorldNumberV1>) -> Result<Option<u64>, BodyMutationError> {
    match n {
        None => Ok(None),
        Some(WorldNumberV1::Unsigned(n)) => Ok(Some(n)),
        _ => Err(bad()),
    }
}
fn agent<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    id: &str,
    kind: ResourceKind,
    sources: &ContractSourcesV1<'_, '_>,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    common: &mut FormalBoundedContextV1<'_>,
    live: &ExecutionFence,
) -> Result<Option<i64>, BodyMutationError> {
    charge(common, id.len())?;
    let guard = context.transaction.guard();
    let ordinal = match kind {
        ResourceKind::Electricity => 0,
        ResourceKind::Data => 1,
    };
    match read_agent_subtree_scalar_overlay_v1(
        AgentSubtreeQualifiedRequestV1 {
            cell: AgentSubtreeScalarRequestV1 {
                agent: id,
                field: AgentFieldV1::ResourceAmount,
                ordinal,
            },
            qualification: sources.shared.agents,
        },
        context,
        64,
        &mut std::io::sink(),
        &guard,
        live,
    )
    .map_err(overlay)?
    {
        None => Ok(None),
        Some(AgentSubtreeValueV1::I64(n)) => Ok(Some(n)),
        _ => Err(bad()),
    }
}
#[allow(clippy::too_many_arguments)]
pub(super) fn prepare_financial_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    record: &EconomicContractState,
    body: &crate::WorldEventBody,
    sources: &ContractSourcesV1<'_, '_>,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    now: u64,
    common: &mut FormalBoundedContextV1<'_>,
    live: &ExecutionFence,
) -> Result<ReservedValue<FinancialPlanV1>, BodyMutationError> {
    let crate::WorldEventBody::Domain(crate::DomainEvent::EconomicContractSettled {
        success,
        transfer_amount,
        tax_amount,
        creator_reputation_delta,
        counterparty_reputation_delta,
        ..
    }) = body
    else {
        return Err(bad());
    };
    // A fixed error phase covers the original diagnostic templates plus the two
    // actual IDs before any diagnostic formatting; returned errors retain their
    // original category, without claiming long-lived error heap admission.
    let ids = record
        .creator_agent_id
        .len()
        .checked_add(record.counterparty_agent_id.len())
        .ok_or_else(limited)?;
    const ERRORS: &[&str] = &[
        "economic contract status invalid for settlement: ",
        "service contracts unavailable: collateral/evidence/remedy not implemented",
        "atomic exchange settlement requires success=true",
        "economic contract settlement transfer must be > 0, got ",
        "economic contract settlement tax must be >= 0, got ",
        "economic contract settlement debit overflow transfer= tax=",
        "economic contract settlement debit failed agent= kind= amount= available=",
        "economic contract settlement credit failed agent= kind= amount= overflow",
        "economic contract settlement treasury overflow kind= current= delta=",
        " reputation overflow agent= current= delta=",
    ];
    // One diagnostic uses <=2 IDs, 3 signed decimal values (20 bytes each),
    // ResourceKind's <=11-byte Debug name, an <=8-byte status and role <=12.
    // Summing all literal templates is conservative, explicit and input-independent;
    // doubling covers the formatter String's geometric allocation.
    let literal = ERRORS
        .iter()
        .try_fold(0usize, |n, s| n.checked_add(s.len()))
        .ok_or_else(limited)?;
    let diagnostic = ids
        .checked_add(literal)
        .and_then(|n| n.checked_add(3 * 20 + 11 + 8 + 12))
        .and_then(|n| n.checked_mul(2))
        .and_then(|n| n.checked_add(2 * size_of::<String>()))
        .ok_or_else(limited)?;
    let _error = common
        .memory()
        .reserve(diagnostic, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    charge(common, diagnostic)?;
    if record.status != EconomicContractStatus::Accepted {
        return Err(original(crate::WorldError::ResourceBalanceInvalid {
            reason: format!(
                "economic contract status invalid for settlement: {:?}",
                record.status
            ),
        }));
    }
    if record.fulfillment_kind == oasis7_world_state::EconomicContractFulfillmentKind::Service {
        return Err(original(crate::WorldError::ResourceBalanceInvalid {
            reason: "service contracts unavailable: collateral/evidence/remedy not implemented"
                .into(),
        }));
    }
    if !success {
        return Err(original(crate::WorldError::ResourceBalanceInvalid {
            reason: "atomic exchange settlement requires success=true".into(),
        }));
    }
    if *transfer_amount <= 0 {
        return Err(original(crate::WorldError::ResourceBalanceInvalid {
            reason: format!(
                "economic contract settlement transfer must be > 0, got {transfer_amount}"
            ),
        }));
    }
    if *tax_amount < 0 {
        return Err(original(crate::WorldError::ResourceBalanceInvalid {
            reason: format!("economic contract settlement tax must be >= 0, got {tax_amount}"),
        }));
    }
    let debit=transfer_amount.checked_add(*tax_amount).ok_or_else(||original(crate::WorldError::ResourceBalanceInvalid{reason:format!("economic contract settlement debit overflow transfer={transfer_amount} tax={tax_amount}")}))?;
    let before = agent(
        &record.creator_agent_id,
        record.settlement_kind,
        sources,
        context,
        common,
        live,
    )?;
    let current = before.unwrap_or(0);
    if current < debit {
        return Err(original(crate::WorldError::ResourceBalanceInvalid {
            reason: format!(
                "economic contract settlement debit failed agent={} kind={:?} amount={debit} available={current}",
                record.creator_agent_id, record.settlement_kind
            ),
        }));
    }
    let creator_after = current - debit;
    charge(common, ids)?;
    let same = record.creator_agent_id == record.counterparty_agent_id;
    let counter_before = if same {
        None
    } else {
        agent(
            &record.counterparty_agent_id,
            record.settlement_kind,
            sources,
            context,
            common,
            live,
        )?
    };
    let counter_after=(if same{creator_after}else{counter_before.unwrap_or(0)}).checked_add(*transfer_amount).ok_or_else(||original(crate::WorldError::ResourceBalanceInvalid{reason:format!("economic contract settlement credit failed agent={} kind={:?} amount={transfer_amount} overflow",record.counterparty_agent_id,record.settlement_kind)}))?;
    let guard = context.transaction.guard();
    let treasury_before =
        read_current_world_resource_v1(record.settlement_kind, context, &guard, live)
            .map_err(overlay)?;
    let treasury_current = treasury_before.unwrap_or(0);
    let treasury_after=treasury_current.checked_add(*tax_amount).ok_or_else(||original(crate::WorldError::ResourceBalanceInvalid{reason:format!("economic contract settlement treasury overflow kind={:?} current={treasury_current} delta={tax_amount}",record.settlement_kind)}))?;
    let mut reputation = [None, None];
    for (i, (id, delta, role)) in [
        (
            &record.creator_agent_id,
            *creator_reputation_delta,
            "creator",
        ),
        (
            &record.counterparty_agent_id,
            *counterparty_reputation_delta,
            "counterparty",
        ),
    ]
    .into_iter()
    .enumerate()
    {
        if delta != 0 {
            let before = number(WorldNumberMapFieldV1::ReputationScores, id, context, common)?;
            let current = as_signed(before)?;
            let after = current.checked_add(delta).ok_or_else(|| {
                original(crate::WorldError::ResourceBalanceInvalid {
                    reason: format!(
                        "{role} reputation overflow agent={id} current={current} delta={delta}"
                    ),
                })
            })?;
            reputation[i] = Some(Number {
                before,
                after: Some(WorldNumberV1::Signed(after)),
            });
        }
    }
    let empty = Number {
        before: None,
        after: None,
    };
    let mut window = [Window {
        start: empty,
        total: empty,
        write_start: false,
        write_total: false,
    }; 2];
    for (i, (id, delta)) in [
        (&record.creator_agent_id, *creator_reputation_delta),
        (
            &record.counterparty_agent_id,
            *counterparty_reputation_delta,
        ),
    ]
    .into_iter()
    .enumerate()
    {
        if delta > 0 {
            let mut start = number(
                WorldNumberMapFieldV1::ReputationWindowStarted,
                id,
                context,
                common,
            )?;
            let mut total = number(
                WorldNumberMapFieldV1::ReputationWindowAccumulated,
                id,
                context,
                common,
            )?;
            if i == 1 && same {
                if window[0].write_start {
                    start = window[0].start.after;
                }
                if window[0].write_total {
                    total = window[0].total.after;
                }
            }
            let in_window = as_unsigned(start)?.is_some_and(|n| now.saturating_sub(n) < 20);
            window[i] = if in_window {
                Window {
                    start: Number {
                        before: start,
                        after: start,
                    },
                    total: Number {
                        before: total,
                        after: Some(WorldNumberV1::Signed(
                            as_signed(total)?.saturating_add(delta),
                        )),
                    },
                    write_start: false,
                    write_total: true,
                }
            } else {
                Window {
                    start: Number {
                        before: start,
                        after: Some(WorldNumberV1::Unsigned(now)),
                    },
                    total: Number {
                        before: total,
                        after: Some(WorldNumberV1::Signed(delta)),
                    },
                    write_start: true,
                    write_total: true,
                }
            };
        }
    }
    let n = ids.checked_add(1).ok_or_else(limited)?;
    let capacity = n.max(8).checked_next_power_of_two().ok_or_else(limited)?;
    let fee = common
        .memory()
        .reserve(
            size_of::<FinancialPlanV1>()
                .checked_add(capacity)
                .ok_or_else(limited)?,
            MemoryClass::Ordinary,
        )
        .map_err(ValueTreeError::Memory)?;
    charge(common, n)?;
    let pair = oasis7_world_state::WorldState::economic_contract_pair_key(
        &record.creator_agent_id,
        &record.counterparty_agent_id,
    );
    if pair.capacity() > capacity {
        return Err(limited());
    }
    let pair_before = number(
        WorldNumberMapFieldV1::ContractPairLastSuccess,
        &pair,
        context,
        common,
    )?;
    Ok(fee.retain(FinancialPlanV1 {
        creator: Resource {
            before,
            after: if same { counter_after } else { creator_after },
        },
        counterparty: (!same).then_some(Resource {
            before: counter_before,
            after: counter_after,
        }),
        treasury: Resource {
            before: treasury_before,
            after: treasury_after,
        },
        reputation,
        window,
        pair,
        pair_before,
        now,
    }))
}

#[allow(clippy::too_many_arguments)]
pub(super) fn apply_financial_v1<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    V: ValueNodeReader,
    R: ValueNodeReader,
    W: ValueNodeReader + ValueNodeWriter,
    D: FnMut() -> bool,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    CW: FnMut() -> bool,
>(
    plan: &FinancialPlanV1,
    record: &EconomicContractState,
    sources: &ContractSourcesV1<'_, '_>,
    step: &mut IwsStepContext<'_, '_, '_, '_, M, V, W, D>,
    read: &mut WorldFieldReadContextV1<'_, P, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    common: &mut FormalBoundedContextV1<'_>,
    live: &ExecutionFence,
) -> Result<(), BodyMutationError> {
    let mut context = WorldFieldOverlayContextV1 {
        transaction: &mut *step.transaction,
        values: ValueReadContext {
            source: read.values.source,
            memory: read.values.memory,
            class: read.values.class,
            io: &mut *read.values.io,
        },
        key_work: &mut *read.key_work,
        semantic_bytes: read.semantic_bytes,
        record_bytes: read.record_bytes,
        take_state_error: &mut read.take_state_error,
        cancelled: &mut read.cancelled,
    };
    for (id, point) in [
        Some((&record.creator_agent_id, plan.creator)),
        plan.counterparty
            .map(|n| (&record.counterparty_agent_id, n)),
    ]
    .into_iter()
    .flatten()
    {
        charge(common, id.len())?;
        let ordinal = match record.settlement_kind {
            ResourceKind::Electricity => 0,
            ResourceKind::Data => 1,
        };
        let mut sink = ValueMutationContextV1 {
            sink: &mut *step.mutation.sink,
            memory: step.mutation.memory,
            io: &mut *step.mutation.io,
            cancelled: &mut step.mutation.cancelled,
        };
        let guard = context.transaction.guard();
        change_agent_subtree_scalar_v1(
            AgentSubtreeCellChangeV1 {
                query: AgentSubtreeQualifiedRequestV1 {
                    cell: AgentSubtreeScalarRequestV1 {
                        agent: id,
                        field: AgentFieldV1::ResourceAmount,
                        ordinal,
                    },
                    qualification: sources.shared.agents,
                },
                expected: point.before.map(AgentScalarRefV1::I64),
                after: (point.after != 0).then_some(AgentScalarRefV1::I64(point.after)),
                max_scalar_bytes: 64,
            },
            &mut context,
            &mut sink,
            &guard,
            live,
        )
        .map_err(overlay)?;
    }
    let guard = context.transaction.guard();
    charge(common, 3 * size_of::<i64>())?;
    change_current_world_resource_v1(
        record.settlement_kind,
        plan.treasury.before,
        (plan.treasury.after != 0).then_some(plan.treasury.after),
        &mut context,
        write,
        &guard,
        live,
    )
    .map_err(overlay)?;
    charge(
        common,
        record
            .creator_agent_id
            .len()
            .checked_add(record.counterparty_agent_id.len())
            .ok_or_else(limited)?,
    )?;
    let same = record.creator_agent_id == record.counterparty_agent_id;
    for (i, (id, change)) in [
        (&record.creator_agent_id, plan.reputation[0]),
        (&record.counterparty_agent_id, plan.reputation[1]),
    ]
    .into_iter()
    .enumerate()
    {
        if i == 0 && same && plan.reputation[1].is_some() {
            continue;
        }
        if let Some(change) = change {
            charge(common, id.len())?;
            let guard = context.transaction.guard();
            change_world_number_map_overlay_v1(
                WorldNumberMapChangeV1 {
                    field: WorldNumberMapFieldV1::ReputationScores,
                    key: WorldNumberMapKeyRefV1::Text(id),
                    expected: change.before,
                    after: change.after,
                },
                &mut context,
                write,
                &guard,
                live,
            )
            .map_err(overlay)?;
        }
    }
    charge(common, plan.pair.len())?;
    let guard = context.transaction.guard();
    change_world_number_map_overlay_v1(
        WorldNumberMapChangeV1 {
            field: WorldNumberMapFieldV1::ContractPairLastSuccess,
            key: WorldNumberMapKeyRefV1::Text(&plan.pair),
            expected: plan.pair_before,
            after: Some(WorldNumberV1::Unsigned(plan.now)),
        },
        &mut context,
        write,
        &guard,
        live,
    )
    .map_err(overlay)?;
    for (id, window) in [
        (&record.creator_agent_id, plan.window[0]),
        (&record.counterparty_agent_id, plan.window[1]),
    ] {
        for (field, change, apply) in [
            (
                WorldNumberMapFieldV1::ReputationWindowStarted,
                window.start,
                window.write_start,
            ),
            (
                WorldNumberMapFieldV1::ReputationWindowAccumulated,
                window.total,
                window.write_total,
            ),
        ] {
            if apply {
                charge(common, id.len())?;
                let guard = context.transaction.guard();
                change_world_number_map_overlay_v1(
                    WorldNumberMapChangeV1 {
                        field,
                        key: WorldNumberMapKeyRefV1::Text(id),
                        expected: change.before,
                        after: change.after,
                    },
                    &mut context,
                    write,
                    &guard,
                    live,
                )
                .map_err(overlay)?;
            }
        }
    }
    Ok(())
}
