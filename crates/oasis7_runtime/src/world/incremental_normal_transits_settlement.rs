//! Pure settlement planning and typed Current writes for material transits.

use super::common::*;
use super::current_read::{CurrentLogisticsPathV1, CurrentLogisticsRouteV1};

pub(super) struct MaterialTransitSettlementPlanV1 {
    pub(super) no_op: bool,
    pub(super) loss_amount: i64,
    pub(super) received_amount: i64,
    pub(super) completed_path: Option<LogisticsPathAuthorityV1>,
    pub(super) path_before_amounts: Option<(i64, i64)>,
    pub(super) route_reservations: BTreeMap<String, RouteReservationChangeV1>,
    pub(super) completed_route_ids: BTreeSet<String>,
    pub(super) owner_fees: BTreeMap<String, i64>,
    pub(super) owner_payouts: BTreeMap<String, i64>,
    pub(super) receipt: LogisticsSettlementReceiptV1,
}

#[derive(Clone, Copy)]
pub(super) struct RouteReservationChangeV1 {
    before: i64,
    after: i64,
}

/// Reproduce the pure business portion of `PreparedMaterialTransit` from
/// already authenticated Current point reads. The returned data itself grants
/// no write or publication permission; its caller still performs every typed
/// CAS and mints the event receipt only after those writes.
pub(super) fn plan_current_transit_settlement_v1(
    job: &MaterialTransitJobState,
    routes: &BTreeMap<String, Option<ReservedValue<CurrentLogisticsRouteV1>>>,
    existing_path: Option<&CurrentLogisticsPathV1>,
    existing_owners: &BTreeSet<String>,
    settled: bool,
    now: u64,
    memory: &MemoryBudget,
    normal: &mut NormalTailWorkV1<'_, '_>,
) -> Result<ReservedValue<MaterialTransitSettlementPlanV1>, BodyMutationError> {
    let mut layout = size_of::<MaterialTransitSettlementPlanV1>()
        .checked_add(size_of::<BTreeMap<String, i64>>() * 3)
        .and_then(|n| n.checked_add(size_of::<BTreeSet<String>>()))
        .and_then(|n| n.checked_add(size_of::<LogisticsSettlementReceiptV1>()))
        .and_then(|n| n.checked_add(job.route_ids.len().checked_mul(4 * size_of::<String>())?))
        .ok_or(ValueTreeError::ResourceLimited(
            "transit settlement plan layout",
        ))?;
    for value in [
        job.path_id.as_deref(),
        Some(job.requester_agent_id.as_str()),
        Some(job.kind.as_str()),
        job.route_id.as_deref(),
    ]
    .into_iter()
    .flatten()
    {
        layout = layout
            .checked_add(
                value
                    .len()
                    .checked_mul(4)
                    .ok_or(ValueTreeError::ResourceLimited(
                        "transit settlement plan text",
                    ))?,
            )
            .ok_or(ValueTreeError::ResourceLimited(
                "transit settlement plan text",
            ))?;
    }
    let map_count = routes.len();
    layout = layout
        .checked_add(
            map_count
                .checked_mul(size_of::<String>() + size_of::<i64>() + 64)
                .ok_or(ValueTreeError::ResourceLimited("transit settlement maps"))?,
        )
        .ok_or(ValueTreeError::ResourceLimited("transit settlement maps"))?;
    for (id, route) in routes {
        layout = layout
            .checked_add(
                id.len()
                    .checked_mul(4)
                    .ok_or(ValueTreeError::ResourceLimited(
                        "transit settlement route text",
                    ))?,
            )
            .and_then(|n| {
                n.checked_add(
                    route
                        .as_ref()
                        .map_or(0, |route| route.value.owner_agent_id.len()),
                )
            })
            .ok_or(ValueTreeError::ResourceLimited(
                "transit settlement route text",
            ))?;
    }
    for id in &job.route_ids {
        layout = layout
            .checked_add(
                id.len()
                    .checked_mul(4)
                    .ok_or(ValueTreeError::ResourceLimited(
                        "transit settlement route id bytes",
                    ))?,
            )
            .ok_or(ValueTreeError::ResourceLimited(
                "transit settlement route id bytes",
            ))?;
    }
    if let Some(path) = existing_path {
        let route_text_bytes = path.value.route_ids.iter().try_fold(0usize, |sum, id| {
            sum.checked_add(id.len())
                .ok_or(ValueTreeError::ResourceLimited(
                    "transit settlement path route bytes",
                ))
        })?;
        layout = layout
            .checked_add(path.value.path_id.len().checked_mul(2).ok_or(
                ValueTreeError::ResourceLimited("transit settlement path id bytes"),
            )?)
            .and_then(|n| n.checked_add(path.value.kind.len()))
            .and_then(|n| n.checked_add(route_text_bytes))
            .and_then(|n| {
                n.checked_add(
                    path.value
                        .route_ids
                        .len()
                        .checked_mul(size_of::<String>())?,
                )
            })
            .ok_or(ValueTreeError::ResourceLimited(
                "transit settlement path layout",
            ))?;
    }
    let fee = charge_reserve(normal, memory, layout, "transit settlement plan")?;
    let mut route_reservations: BTreeMap<String, RouteReservationChangeV1> = BTreeMap::new();
    let mut completed_route_ids = BTreeSet::new();
    let mut owner_fees = BTreeMap::<String, i64>::new();
    let mut owner_payouts = BTreeMap::<String, i64>::new();
    let tariff_total = job.tariff_electricity_total.max(0);
    let (loss_amount, received_amount, completed_path, path_before_amounts, no_op) = if settled {
        (0, 0, None, None, true)
    } else {
        if now < job.ready_at {
            return Err(BodyMutationError::Invalid(
                "material transit completion is early",
            ));
        }
        let amount = job.amount.max(0);
        let loss = ((amount as i128)
            .saturating_mul(job.distance_km as i128)
            .saturating_mul(job.loss_bps as i128)
            / 10_000)
            .clamp(0, amount as i128) as i64;
        let received = job.amount.saturating_sub(loss);
        if job.amount < 0 || received < 0 {
            return Err(BodyMutationError::Invalid(
                "material transit amount conservation",
            ));
        }
        for route_id in &job.route_ids {
            let Some(Some(route)) = routes.get(route_id) else {
                // Legacy completion tolerates a route that has disappeared;
                // it still records the committed route id and path binding.
                continue;
            };
            let prior = route_reservations
                .get(route_id)
                .map_or(route.value.reserved_capacity_units, |change| change.after);
            route_reservations.insert(
                route_id.clone(),
                RouteReservationChangeV1 {
                    before: route_reservations
                        .get(route_id)
                        .map_or(route.value.reserved_capacity_units, |change| change.before),
                    after: prior.saturating_sub(job.amount),
                },
            );
            let edge_fee = route
                .value
                .tariff_electricity_per_unit
                .saturating_mul(job.amount)
                .max(0);
            if edge_fee > 0 {
                let fee = owner_fees
                    .get(&route.value.owner_agent_id)
                    .copied()
                    .unwrap_or(0)
                    .saturating_add(edge_fee);
                owner_fees.insert(route.value.owner_agent_id.clone(), fee);
            }
        }
        let (mut path, before_amounts) = if let Some(path_id) = job.path_id.as_deref() {
            if let Some(existing) = existing_path {
                let value = &existing.value;
                if value.route_ids != job.route_ids
                    || value.from_ledger != job.from_ledger
                    || value.to_ledger != job.to_ledger
                    || value.kind != job.kind
                    || value.settled_amount < 0
                    || value.remaining_recipe_amount < 0
                    || value.remaining_recipe_amount > value.settled_amount
                {
                    return Err(original_world_error(
                        crate::WorldError::ResourceBalanceInvalid {
                            reason: format!(
                                "material transit path authority does not match settlement: path_id={path_id}"
                            ),
                        },
                    ));
                }
                (
                    Some(value.clone()),
                    Some((value.settled_amount, value.remaining_recipe_amount)),
                )
            } else {
                (
                    Some(LogisticsPathAuthorityV1 {
                        path_id: path_id.to_owned(),
                        route_ids: job.route_ids.clone(),
                        from_ledger: job.from_ledger.clone(),
                        to_ledger: job.to_ledger.clone(),
                        kind: job.kind.clone(),
                        settled_amount: 0,
                        remaining_recipe_amount: 0,
                    }),
                    None,
                )
            }
        } else {
            (None, None)
        };
        if let Some(path) = &mut path {
            path.settled_amount = path
                .settled_amount
                .checked_add(received.max(0))
                .ok_or_else(|| {
                    original_world_error(crate::WorldError::ResourceBalanceInvalid {
                        reason: format!(
                            "material transit path settled quantity overflow: path_id={}",
                            path.path_id
                        ),
                    })
                })?;
            path.remaining_recipe_amount = path
                .remaining_recipe_amount
                .checked_add(received.max(0))
                .ok_or_else(|| {
                    original_world_error(crate::WorldError::ResourceBalanceInvalid {
                        reason: format!(
                            "material transit path recipe quantity overflow: path_id={}",
                            path.path_id
                        ),
                    })
                })?;
        }
        for route_id in &job.route_ids {
            completed_route_ids.insert(route_id.clone());
        }
        if let Some(route_id) = job.route_id.as_ref() {
            completed_route_ids.insert(route_id.clone());
        }
        let mut remaining_tariff = tariff_total;
        for (owner, owner_fee) in &owner_fees {
            if remaining_tariff == 0 {
                break;
            }
            let credit = (*owner_fee).min(remaining_tariff);
            if credit > 0 {
                if existing_owners.contains(owner) {
                    owner_payouts.insert(owner.clone(), credit);
                }
                remaining_tariff = remaining_tariff.saturating_sub(credit);
            }
        }
        (loss, received, path, before_amounts, false)
    };
    let receipt = if no_op {
        LogisticsSettlementReceiptV1::default()
    } else {
        LogisticsSettlementReceiptV1 {
            job_id: job.job_id,
            path_id: job.path_id.clone(),
            route_ids: job.route_ids.clone(),
            tariff_electricity_total: tariff_total,
            owner_payouts: owner_fees.clone(),
            governance_tax_electricity: 0,
        }
    };
    Ok(fee.retain(MaterialTransitSettlementPlanV1 {
        no_op,
        loss_amount,
        received_amount,
        completed_path,
        path_before_amounts,
        route_reservations,
        completed_route_ids,
        owner_fees,
        owner_payouts,
        receipt,
    }))
}

#[allow(clippy::too_many_arguments)]
pub(super) fn apply_current_transit_logistics_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    W: ValueNodeReader + ValueNodeWriter + StatePageWriter,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    CW: FnMut() -> bool,
>(
    plan: &MaterialTransitSettlementPlanV1,
    job: &MaterialTransitJobState,
    qualification: &ReservedValue<LogisticsFamilyQualificationV1>,
    overlay: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    write: &mut oasis7_world_state::incremental_field_member::MemberRecordWriteContext<'_, W, CW>,
    logistics_work: &mut LogisticsWorkBudgetV1,
    industry_parent: &ReservedValue<IndustryFamilyQualificationV1>,
    industry_work: &mut IndustryWorkBudgetV1,
    max_payload_bytes: u64,
    live: &ExecutionFence,
) -> Result<(), BodyMutationError> {
    if plan.no_op {
        return Ok(());
    }
    for (route_id, change) in &plan.route_reservations {
        let key = LogisticsRecordKeyV1 {
            kind: LogisticsRecordKindV1::Route,
            id: route_id,
        };
        let guard = overlay.transaction.guard();
        change_logistics_cell_overlay_v1(
            LogisticsCellChangeV1 {
                key,
                cell: LogisticsCellV1::Column(LogisticsColumn::ReservedCapacityUnits),
                expected: Some(LogisticsValueRefV1::Signed(change.before)),
                after: Some(LogisticsValueRefV1::Signed(change.after)),
                max_current_payload_bytes: max_payload_bytes,
            },
            overlay,
            write,
            logistics_work,
            &guard,
            live,
        )
        .map_err(overlay_error)?;
    }
    if let Some(path) = &plan.completed_path {
        if plan.path_before_amounts.is_none() {
            let key = LogisticsRecordKeyV1 {
                kind: LogisticsRecordKindV1::Path,
                id: &path.path_id,
            };
            let guard = overlay.transaction.guard();
            create_logistics_record_overlay_v1(
                LogisticsRecordCreateV1 {
                    key,
                    after: LogisticsRecordRefV1::Path(path),
                    qualification,
                },
                overlay,
                write,
                logistics_work,
                &guard,
                live,
            )
            .map_err(overlay_error)?;
        } else {
            let (before_settled, before_remaining) = plan.path_before_amounts.unwrap();
            let key = LogisticsRecordKeyV1 {
                kind: LogisticsRecordKindV1::Path,
                id: &path.path_id,
            };
            let guard = overlay.transaction.guard();
            change_logistics_cell_overlay_v1(
                LogisticsCellChangeV1 {
                    key,
                    cell: LogisticsCellV1::Column(LogisticsColumn::SettledAmount),
                    expected: Some(LogisticsValueRefV1::Signed(before_settled)),
                    after: Some(LogisticsValueRefV1::Signed(path.settled_amount)),
                    max_current_payload_bytes: max_payload_bytes,
                },
                overlay,
                write,
                logistics_work,
                &guard,
                live,
            )
            .map_err(overlay_error)?;
            let guard = overlay.transaction.guard();
            change_logistics_cell_overlay_v1(
                LogisticsCellChangeV1 {
                    key,
                    cell: LogisticsCellV1::Column(LogisticsColumn::RemainingRecipeAmount),
                    expected: Some(LogisticsValueRefV1::Signed(before_remaining)),
                    after: Some(LogisticsValueRefV1::Signed(path.remaining_recipe_amount)),
                    max_current_payload_bytes: max_payload_bytes,
                },
                overlay,
                write,
                logistics_work,
                &guard,
                live,
            )
            .map_err(overlay_error)?;
        }
    }
    // The exact committed id-set projections are part of the same Current
    // transaction. Existing members are no-ops, matching BTreeSet::extend.
    for route_id in &plan.completed_route_ids {
        let contains_guard = overlay.transaction.guard();
        let contains = world_id_set_contains_overlay_v1(
            WorldIdSetFieldV1::CompletedLogisticsRoutes,
            WorldIdSetKeyRefV1::Text(route_id),
            overlay,
        )
        .map_err(overlay_error)?;
        if !contains {
            let write_guard = overlay.transaction.guard();
            change_world_id_set_overlay_v1(
                WorldIdSetChangeV1 {
                    field: WorldIdSetFieldV1::CompletedLogisticsRoutes,
                    key: WorldIdSetKeyRefV1::Text(route_id),
                    expected: false,
                    after: true,
                },
                overlay,
                write,
                &write_guard,
                live,
            )
            .map_err(overlay_error)?;
        }
        let _ = contains_guard;
    }
    let settled_guard = overlay.transaction.guard();
    let settled = world_id_set_contains_overlay_v1(
        WorldIdSetFieldV1::SettledLogisticsTransits,
        WorldIdSetKeyRefV1::Action(job.job_id),
        overlay,
    )
    .map_err(overlay_error)?;
    if !settled {
        let guard = overlay.transaction.guard();
        change_world_id_set_overlay_v1(
            WorldIdSetChangeV1 {
                field: WorldIdSetFieldV1::SettledLogisticsTransits,
                key: WorldIdSetKeyRefV1::Action(job.job_id),
                expected: false,
                after: true,
            },
            overlay,
            write,
            &guard,
            live,
        )
        .map_err(overlay_error)?;
    }
    let receipt_key = oasis7_world_state::incremental_industry_jobs::IndustryRecordKeyV1 {
        kind: IndustryRecordKindV1::Settlement,
        id: job.job_id,
    };
    let existing_receipt_guard = overlay.transaction.guard();
    if let Some(root) =
        read_industry_record_root_overlay_v1(receipt_key, overlay).map_err(overlay_error)?
    {
        let delete_guard = overlay.transaction.guard();
        delete_industry_record_overlay_v1(
            IndustryRecordDeleteV1 {
                key: receipt_key,
                expected_root: Some(root),
                qualification: industry_parent,
            },
            overlay,
            write,
            industry_work,
            &delete_guard,
            live,
        )
        .map_err(overlay_error)?;
    }
    let create_guard = overlay.transaction.guard();
    create_industry_record_overlay_v1(
        IndustryRecordCreateV1 {
            key: receipt_key,
            after: IndustryRecordRefV1::Settlement(&plan.receipt),
            qualification: industry_parent,
        },
        overlay,
        write,
        industry_work,
        &create_guard,
        live,
    )
    .map_err(overlay_error)?;
    let pending_key = oasis7_world_state::incremental_industry_jobs::IndustryRecordKeyV1 {
        kind: IndustryRecordKindV1::Transit,
        id: job.job_id,
    };
    let pending_guard = overlay.transaction.guard();
    let pending_root =
        read_industry_record_root_overlay_v1(pending_key, overlay).map_err(overlay_error)?;
    let delete_guard = overlay.transaction.guard();
    let removed = delete_industry_record_overlay_v1(
        IndustryRecordDeleteV1 {
            key: pending_key,
            expected_root: pending_root,
            qualification: industry_parent,
        },
        overlay,
        write,
        industry_work,
        &delete_guard,
        live,
    )
    .map_err(overlay_error)?;
    if !removed {
        return Err(BodyMutationError::Invalid(
            "material transit pending Current absent",
        ));
    }
    let _ = (pending_guard, existing_receipt_guard, settled_guard);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(super) fn apply_current_material_credit_v1<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    V: ValueNodeReader,
    W: ValueNodeReader + ValueNodeWriter + StatePageWriter,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    ME: FnMut() -> Option<ValueTreeError>,
    D: FnMut() -> bool,
>(
    step: &mut IwsStepContext<'_, '_, '_, '_, M, V, W, D>,
    materials: &mut crate::world::incremental_body_event::materials::CorePolicyMaterialContextV1<
        '_,
        M,
        ME,
    >,
    read: &mut WorldFieldReadContextV1<'_, P, V, E, C>,
    ledger: &MaterialLedgerId,
    kind: &str,
    amount: i64,
    live: &ExecutionFence,
) -> Result<(), BodyMutationError> {
    if amount <= 0 && *ledger == MaterialLedgerId::World {
        return Ok(());
    }
    let memory = step.mutation.memory;
    if !materials.parent.uses_budget(memory) || !step.transaction.uses_budget(memory) {
        return Err(BodyMutationError::Invalid("transit material credit pool"));
    }
    let read_cancel = RefCell::new(&mut read.cancelled);
    let mutation_cancel = RefCell::new(&mut step.mutation.cancelled);
    let mut overlay = WorldFieldOverlayContextV1 {
        transaction: &mut *step.transaction,
        values: ValueReadContext {
            source: read.values.source,
            memory,
            class: MemoryClass::Ordinary,
            io: &mut *materials.read_io,
        },
        key_work: &mut *materials.read_keys,
        semantic_bytes: materials.semantic_bytes,
        record_bytes: materials.record_bytes,
        take_state_error: &mut materials.take_state_error,
        cancelled: || (read_cancel.borrow_mut())() || (mutation_cancel.borrow_mut())(),
    };
    let guard = overlay.transaction.guard();
    overlay.transaction.validate_guard(&guard, live)?;
    let expected = overlay.ledger(ledger, kind).map_err(overlay_error)?;
    if amount <= 0 {
        if matches!(expected, LedgerQuantityV1::LedgerAbsent) {
            let mut write =
                oasis7_world_state::incremental_field_member::MemberRecordWriteContext {
                    sink: &mut *step.mutation.sink,
                    memory,
                    io: &mut *step.mutation.io,
                    key_work: &mut *materials.write_keys,
                    limits: materials.write_limits,
                    cancelled: || (read_cancel.borrow_mut())() || (mutation_cancel.borrow_mut())(),
                };
            let guard = overlay.transaction.guard();
            overlay
                .change(
                    InventoryChangeRefV1::LedgerMaterial {
                        ledger,
                        material: kind,
                        expected,
                        after: None,
                        create_ledger: true,
                    },
                    &mut write,
                    &guard,
                    live,
                )
                .map_err(|error| BodyMutationError::Original(Box::new(error)))?;
        }
        let tail = overlay.transaction.guard();
        overlay.transaction.validate_guard(&tail, live)?;
        return Ok(());
    }
    let before = match expected {
        LedgerQuantityV1::LedgerAbsent | LedgerQuantityV1::MaterialAbsent => 0,
        LedgerQuantityV1::Value(value) => value,
    };
    let after =
        oasis7_world_state::state::support::checked_material_balance_add_v1(kind, before, amount)
            .map_err(|reason| {
            original_world_error(crate::WorldError::ResourceBalanceInvalid {
                reason: format!("material transit completion preflight failed: {reason}"),
            })
        })?;
    let mut write = oasis7_world_state::incremental_field_member::MemberRecordWriteContext {
        sink: &mut *step.mutation.sink,
        memory,
        io: &mut *step.mutation.io,
        key_work: &mut *materials.write_keys,
        limits: materials.write_limits,
        cancelled: || (read_cancel.borrow_mut())() || (mutation_cancel.borrow_mut())(),
    };
    let guard = overlay.transaction.guard();
    overlay
        .change(
            InventoryChangeRefV1::LedgerMaterial {
                ledger,
                material: kind,
                expected,
                after: (after != 0).then_some(after),
                create_ledger: matches!(expected, LedgerQuantityV1::LedgerAbsent),
            },
            &mut write,
            &guard,
            live,
        )
        .map_err(|error| BodyMutationError::Original(Box::new(error)))?;
    if *ledger == MaterialLedgerId::World {
        let legacy = overlay.material(kind).map_err(overlay_error)?;
        if legacy.unwrap_or(0) != before {
            return Err(BodyMutationError::Invalid(
                "transit legacy material balance mismatch",
            ));
        }
        let guard = overlay.transaction.guard();
        overlay
            .change(
                InventoryChangeRefV1::Material {
                    key: kind,
                    expected: legacy,
                    after: (after != 0).then_some(after),
                },
                &mut write,
                &guard,
                live,
            )
            .map_err(|error| BodyMutationError::Original(Box::new(error)))?;
    }
    if (read_cancel.borrow_mut())() || (mutation_cancel.borrow_mut())() {
        return Err(ValueTreeError::Cancelled.into());
    }
    let tail = overlay.transaction.guard();
    overlay.transaction.validate_guard(&tail, live)?;
    Ok(())
}
