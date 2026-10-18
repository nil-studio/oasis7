use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

use oasis7_agent_api::world_action::ResourceKind;

use super::super::state::LogisticsRouteV1;
use super::super::util::sha256_hex;
use super::super::{
    DomainEvent, MaterialDefaultPriority, MaterialLedgerId, MaterialTransitPriority,
    MaterialTransportLossClass, RejectReason, WorldError, WorldEvent, WorldEventBody, WorldTime,
};
use super::{LogisticsSlaMetrics, World};

struct PreparedDueMaterialTransits {
    bodies: Vec<WorldEventBody>,
    logistics_sla_metrics: LogisticsSlaMetrics,
}

pub(super) const MATERIAL_TRANSFER_MAX_DISTANCE_KM: i64 = 10_000;
pub(super) const MATERIAL_TRANSFER_LOSS_PER_KM_BPS: i64 = 5;
pub(super) const MATERIAL_TRANSFER_SPEED_KM_PER_TICK: i64 = 100;
pub(super) const MATERIAL_TRANSFER_MAX_INFLIGHT: usize = 2;
pub(super) const LOGISTICS_MAX_HOPS: usize = 8;
pub(super) const LOGISTICS_MAX_PATH_SEARCHES: usize = 4_096;

pub(super) fn normalize_logistics_route_kind(kind: &str) -> Option<String> {
    let normalized = kind.trim().to_ascii_lowercase();
    (!normalized.is_empty()).then_some(normalized)
}

pub(super) fn logistics_route_id(
    from_ledger: &MaterialLedgerId,
    to_ledger: &MaterialLedgerId,
    kind: &str,
    distance_km: i64,
    priority: MaterialTransitPriority,
) -> String {
    let tuple = (from_ledger, to_ledger, kind, distance_km, priority);
    let bytes = serde_json::to_vec(&tuple).expect("logistics route tuple is serializable");
    sha256_hex(&bytes)
}

pub(super) fn logistics_route_matches(
    route: &LogisticsRouteV1,
    from_ledger: &MaterialLedgerId,
    to_ledger: &MaterialLedgerId,
    kind: &str,
    distance_km: i64,
    priority: Option<MaterialTransitPriority>,
) -> bool {
    route.from_ledger == *from_ledger
        && route.to_ledger == *to_ledger
        && route.kind == kind
        && route.distance_km == distance_km
        && priority.is_none_or(|priority| priority == route.priority)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct LogisticsPathSelection {
    pub route_ids: Vec<String>,
    pub path_id: Option<String>,
    pub total_distance_km: i64,
    pub total_tariff_electricity: i64,
    pub total_expected_loss_amount: i64,
    pub reroute_count: u32,
}

pub(super) fn logistics_path_id(route_ids: &[String]) -> Option<String> {
    if route_ids.is_empty() {
        return None;
    }
    let bytes = serde_json::to_vec(route_ids).expect("logistics path ids are serializable");
    Some(sha256_hex(&bytes))
}

fn route_available_for_amount(route: &LogisticsRouteV1, amount: i64) -> bool {
    route.available
        && route.capacity_units > 0
        && route.reserved_capacity_units.saturating_add(amount) <= route.capacity_units
}

fn path_candidate_key(
    world: &World,
    route_ids: &[String],
    amount: i64,
) -> Option<(u64, i64, i64, usize, Vec<String>)> {
    let mut total_distance = 0_i64;
    let mut total_tariff = 0_i64;
    let mut path_kind = None;
    for route_id in route_ids {
        let route = world.state.logistics_routes.get(route_id)?;
        path_kind.get_or_insert(route.kind.as_str());
        total_distance = total_distance.checked_add(route.distance_km)?;
        total_tariff =
            total_tariff.checked_add(route.tariff_electricity_per_unit.checked_mul(amount)?)?;
    }
    let total_loss = material_transit_loss_amount(
        amount,
        total_distance,
        material_transit_loss_bps_for_kind(world, path_kind?),
    );
    Some((
        material_transit_ticks(total_distance),
        total_tariff,
        total_loss,
        route_ids.len(),
        route_ids.to_vec(),
    ))
}

fn path_matches_route_tuple(
    world: &World,
    route_ids: &[String],
    from_ledger: &MaterialLedgerId,
    to_ledger: &MaterialLedgerId,
    kind: &str,
) -> bool {
    if route_ids.is_empty() || route_ids.len() > LOGISTICS_MAX_HOPS {
        return false;
    }
    let mut current = from_ledger;
    let mut seen_route_ids = BTreeSet::new();
    let mut seen_ledgers = BTreeSet::new();
    seen_ledgers.insert(from_ledger.clone());
    for route_id in route_ids {
        if !seen_route_ids.insert(route_id) {
            return false;
        }
        let Some(route) = world.state.logistics_routes.get(route_id) else {
            return false;
        };
        if route.from_ledger != *current || route.kind != kind {
            return false;
        }
        if !seen_ledgers.insert(route.to_ledger.clone()) {
            return false;
        }
        current = &route.to_ledger;
    }
    current == to_ledger
}

#[expect(
    clippy::too_many_arguments,
    reason = "DFS carries each deterministic route-search accumulator explicitly."
)]
fn find_paths_dfs(
    world: &World,
    current: &MaterialLedgerId,
    destination: &MaterialLedgerId,
    kind: &str,
    amount: i64,
    path: &mut Vec<String>,
    visited: &mut BTreeSet<MaterialLedgerId>,
    candidates: &mut Vec<Vec<String>>,
    searches: &mut usize,
) {
    if current == destination {
        candidates.push(path.clone());
        return;
    }
    if *searches >= LOGISTICS_MAX_PATH_SEARCHES || path.len() >= LOGISTICS_MAX_HOPS {
        return;
    }
    let edges: Vec<(String, MaterialLedgerId)> = world
        .state
        .logistics_routes
        .iter()
        .filter(|(_, route)| {
            route.from_ledger == *current
                && route.kind == kind
                && route_available_for_amount(route, amount)
                && !visited.contains(&route.to_ledger)
        })
        .map(|(route_id, route)| (route_id.clone(), route.to_ledger.clone()))
        .collect();
    for (route_id, next) in edges {
        *searches = searches.saturating_add(1);
        path.push(route_id);
        visited.insert(next.clone());
        find_paths_dfs(
            world,
            &next,
            destination,
            kind,
            amount,
            path,
            visited,
            candidates,
            searches,
        );
        visited.remove(&next);
        path.pop();
    }
}

pub(super) fn select_logistics_path(
    world: &World,
    from_ledger: &MaterialLedgerId,
    to_ledger: &MaterialLedgerId,
    kind: &str,
    amount: i64,
    requested_route_ids: &[String],
    auto_reroute: bool,
) -> Result<LogisticsPathSelection, RejectReason> {
    let Some(normalized_kind) = normalize_logistics_route_kind(kind) else {
        return Err(RejectReason::RuleDenied {
            notes: vec!["material kind cannot be empty".to_string()],
        });
    };
    let mut reroute_count = 0;
    let mut candidates = Vec::new();
    if !requested_route_ids.is_empty() {
        if !path_matches_route_tuple(
            world,
            requested_route_ids,
            from_ledger,
            to_ledger,
            normalized_kind.as_str(),
        ) {
            return Err(RejectReason::RuleDenied {
                notes: vec![
                    "logistics route path is cyclic, disconnected, or incompatible".to_string(),
                ],
            });
        }
        let requested_available = requested_route_ids.iter().all(|route_id| {
            world
                .state
                .logistics_routes
                .get(route_id)
                .is_some_and(|route| route_available_for_amount(route, amount))
        });
        if requested_available {
            candidates.push(requested_route_ids.to_vec());
        } else if !auto_reroute {
            return Err(RejectReason::RuleDenied {
                notes: vec!["requested logistics path is unavailable or at capacity".to_string()],
            });
        } else {
            reroute_count = 1;
        }
    }
    if candidates.is_empty() {
        let mut path = Vec::new();
        let mut visited = BTreeSet::new();
        let mut searches = 0;
        visited.insert(from_ledger.clone());
        find_paths_dfs(
            world,
            from_ledger,
            to_ledger,
            normalized_kind.as_str(),
            amount,
            &mut path,
            &mut visited,
            &mut candidates,
            &mut searches,
        );
        if reroute_count > 0 {
            candidates.retain(|candidate| candidate != requested_route_ids);
        }
    }
    let route_ids = candidates
        .into_iter()
        .filter_map(|candidate| {
            path_candidate_key(world, &candidate, amount).map(|key| (key, candidate))
        })
        .min_by(|(left, _), (right, _)| left.cmp(right))
        .map(|(_, candidate)| candidate)
        .ok_or_else(|| RejectReason::RuleDenied {
            notes: vec!["no available logistics path to destination".to_string()],
        })?;
    let key =
        path_candidate_key(world, &route_ids, amount).ok_or_else(|| RejectReason::RuleDenied {
            notes: vec!["logistics path tariff or distance overflow".to_string()],
        })?;
    Ok(LogisticsPathSelection {
        path_id: logistics_path_id(&route_ids),
        total_distance_km: route_ids
            .iter()
            .filter_map(|route_id| world.state.logistics_routes.get(route_id))
            .map(|route| route.distance_km)
            .sum(),
        total_tariff_electricity: key.1,
        total_expected_loss_amount: key.2,
        route_ids,
        reroute_count,
    })
}

const MATERIAL_TRANSIT_URGENT_KEYWORDS: &[&str] = &[
    "survival",
    "lifeline",
    "critical",
    "repair",
    "maintenance",
    "oxygen",
    "water",
    "emergency",
];

/// A non-reserving projection of a material transfer at the current world state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogisticsTransferQuote {
    pub requester_agent_id: String,
    pub from_ledger: MaterialLedgerId,
    pub to_ledger: MaterialLedgerId,
    pub kind: String,
    pub requested_amount: i64,
    /// Whether the transfer can be submitted against the currently observed state.
    /// Submission still revalidates before it reserves materials or capacity.
    pub submission_feasible: bool,
    /// The source-ledger amount currently available for this material.
    pub max_transferable_amount: i64,
    pub sent_amount: i64,
    pub distance_km: i64,
    pub loss_bps: i64,
    pub expected_loss_amount: i64,
    pub expected_received_amount: i64,
    pub source_amount_before: i64,
    pub source_amount_after: i64,
    pub destination_amount_before: i64,
    pub destination_expected_amount_after: i64,
    pub ticks_until_arrival: u64,
    pub ready_at: WorldTime,
    pub effective_priority: MaterialTransitPriority,
    pub priority_reason: String,
    pub inflight_before: usize,
    pub inflight_capacity: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path_id: Option<String>,
    #[serde(default)]
    pub route_ids: Vec<String>,
    #[serde(default)]
    pub tariff_electricity_total: i64,
    #[serde(default)]
    pub reroute_count: u32,
    pub recommendation: String,
    /// Quotes do not reserve balance or transit capacity; submission revalidates both.
    pub conditional: bool,
}

/// The pure, route-aware portion of a material transfer action.
///
/// This is shared by quote and action evaluation so path identity, effective distance,
/// tariff, and priority cannot drift between pre-submit projection and authoritative
/// submission. It deliberately does not inspect or mutate any reservations or ledgers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct LogisticsTransferPathPlan {
    pub effective_kind: String,
    pub route_priority: Option<MaterialTransitPriority>,
    pub effective_distance_km: i64,
    pub path_id: Option<String>,
    pub route_ids: Vec<String>,
    pub tariff_electricity_total: i64,
    pub reroute_count: u32,
}

pub(super) fn material_transit_priority_for_kind(
    world: &World,
    kind: &str,
) -> MaterialTransitPriority {
    if let Some(profile) = world.material_profile(kind) {
        return match profile.default_priority {
            MaterialDefaultPriority::Urgent => MaterialTransitPriority::Urgent,
            MaterialDefaultPriority::Standard => MaterialTransitPriority::Standard,
        };
    }

    let normalized = kind.to_ascii_lowercase();
    if MATERIAL_TRANSIT_URGENT_KEYWORDS
        .iter()
        .any(|keyword| normalized.contains(keyword))
    {
        MaterialTransitPriority::Urgent
    } else {
        MaterialTransitPriority::Standard
    }
}

pub(super) fn material_transit_loss_bps_for_kind(world: &World, kind: &str) -> i64 {
    let base = MATERIAL_TRANSFER_LOSS_PER_KM_BPS.max(0);
    let factor = world
        .material_profile(kind)
        .map(|profile| match profile.transport_loss_class {
            MaterialTransportLossClass::Low => 1_i64,
            MaterialTransportLossClass::Medium => 2_i64,
            MaterialTransportLossClass::High => 4_i64,
        })
        .unwrap_or(1);
    base.saturating_mul(factor)
}

pub(super) fn material_transit_ticks(distance_km: i64) -> u64 {
    if distance_km == 0 {
        0
    } else {
        ((distance_km + MATERIAL_TRANSFER_SPEED_KM_PER_TICK - 1)
            / MATERIAL_TRANSFER_SPEED_KM_PER_TICK)
            .max(1) as u64
    }
}

pub(super) fn material_transit_loss_amount(amount: i64, distance_km: i64, loss_bps: i64) -> i64 {
    ((amount as i128)
        .saturating_mul(distance_km as i128)
        .saturating_mul(loss_bps as i128)
        / 10_000)
        .clamp(0, amount as i128) as i64
}

/// Resolves the immutable route/path inputs for a material transfer.
///
/// Route availability and capacity are intentionally evaluated here but not reserved. The
/// action evaluator and the quote both consume this same pure result; the action still performs
/// the authoritative reservation and ledger checks while applying the resulting event.
#[expect(
    clippy::too_many_arguments,
    reason = "Transfer path resolution preserves the action/quote input tuple explicitly."
)]
pub(super) fn resolve_logistics_transfer_path(
    world: &World,
    from_ledger: &MaterialLedgerId,
    to_ledger: &MaterialLedgerId,
    kind: &str,
    amount: i64,
    distance_km: i64,
    priority: Option<MaterialTransitPriority>,
    route_id: Option<&str>,
    route_ids: &[String],
    auto_reroute: bool,
) -> Result<LogisticsTransferPathPlan, RejectReason> {
    let normalized_kind =
        normalize_logistics_route_kind(kind).ok_or_else(|| RejectReason::RuleDenied {
            notes: vec!["material kind cannot be empty".to_string()],
        })?;

    let legacy_route = if let Some(route_id) = route_id {
        let Some(route) = world.state.logistics_routes.get(route_id) else {
            return Err(RejectReason::RuleDenied {
                notes: vec![format!("logistics route not found: route_id={route_id}")],
            });
        };
        if route_ids.is_empty()
            && !logistics_route_matches(
                route,
                from_ledger,
                to_ledger,
                normalized_kind.as_str(),
                distance_km,
                priority,
            )
        {
            return Err(RejectReason::RuleDenied {
                notes: vec![format!(
                    "logistics route tuple mismatch: route_id={route_id}"
                )],
            });
        }
        Some(route)
    } else {
        None
    };

    let mut requested_route_ids = route_ids.to_vec();
    if requested_route_ids.is_empty()
        && let Some(route_id) = route_id
    {
        requested_route_ids.push(route_id.to_string());
    }
    let graph_mode = !requested_route_ids.is_empty() || auto_reroute;
    let path_selection = if graph_mode {
        Some(select_logistics_path(
            world,
            from_ledger,
            to_ledger,
            kind,
            amount,
            requested_route_ids.as_slice(),
            auto_reroute,
        )?)
    } else {
        None
    };
    let selected_route = path_selection
        .as_ref()
        .and_then(|selection| selection.route_ids.first())
        .and_then(|route_id| world.state.logistics_routes.get(route_id))
        .or(legacy_route);
    let effective_kind = selected_route
        .map(|route| route.kind.clone())
        .unwrap_or_else(|| kind.to_string());
    let effective_distance_km = path_selection
        .as_ref()
        .map(|selection| selection.total_distance_km)
        .unwrap_or(distance_km);
    if effective_distance_km > MATERIAL_TRANSFER_MAX_DISTANCE_KM {
        return Err(RejectReason::MaterialTransferDistanceExceeded {
            distance_km: effective_distance_km,
            max_distance_km: MATERIAL_TRANSFER_MAX_DISTANCE_KM,
        });
    }

    Ok(LogisticsTransferPathPlan {
        effective_kind,
        route_priority: selected_route.map(|route| route.priority),
        effective_distance_km,
        path_id: path_selection
            .as_ref()
            .and_then(|selection| selection.path_id.clone()),
        route_ids: path_selection
            .as_ref()
            .map(|selection| selection.route_ids.clone())
            .unwrap_or_default(),
        tariff_electricity_total: path_selection
            .as_ref()
            .map(|selection| selection.total_tariff_electricity)
            .unwrap_or(0),
        reroute_count: path_selection
            .as_ref()
            .map(|selection| selection.reroute_count)
            .unwrap_or(0),
    })
}

fn route_capacity_available_amount(world: &World, route_ids: &[String]) -> Option<i64> {
    route_ids
        .iter()
        .map(|route_id| {
            world.state.logistics_routes.get(route_id).map(|route| {
                route
                    .capacity_units
                    .saturating_sub(route.reserved_capacity_units)
                    .max(0)
            })
        })
        .collect::<Option<Vec<_>>>()
        .and_then(|capacities| capacities.into_iter().min())
}

#[expect(
    clippy::too_many_arguments,
    reason = "Blocked-path diagnostics mirror the route quote input tuple for deterministic recovery hints."
)]
fn blocked_route_path_plan(
    world: &World,
    from_ledger: &MaterialLedgerId,
    to_ledger: &MaterialLedgerId,
    kind: &str,
    amount: i64,
    distance_km: i64,
    priority: Option<MaterialTransitPriority>,
    route_id: Option<&str>,
    route_ids: &[String],
) -> Option<(LogisticsTransferPathPlan, bool)> {
    let requested_route_ids = if route_ids.is_empty() {
        route_id
            .map(|route_id| vec![route_id.to_string()])
            .unwrap_or_default()
    } else {
        route_ids.to_vec()
    };
    let normalized_kind = normalize_logistics_route_kind(kind)?;
    if let Some(route_id) = route_id {
        let route = world.state.logistics_routes.get(route_id)?;
        if route_ids.is_empty()
            && !logistics_route_matches(
                route,
                from_ledger,
                to_ledger,
                normalized_kind.as_str(),
                distance_km,
                priority,
            )
        {
            return None;
        }
    }
    // Only turn a resolver error into a conditional quote after proving that the
    // requested path is structurally valid. Missing, disconnected, cyclic, and
    // incompatible paths must retain their authoritative rejection reason.
    if !path_matches_route_tuple(
        world,
        requested_route_ids.as_slice(),
        from_ledger,
        to_ledger,
        normalized_kind.as_str(),
    ) || !requested_route_ids.iter().any(|route_id| {
        world
            .state
            .logistics_routes
            .get(route_id)
            .is_some_and(|route| !route_available_for_amount(route, amount))
    }) {
        return None;
    }

    let key = path_candidate_key(world, requested_route_ids.as_slice(), amount)?;
    let selected_route = requested_route_ids
        .first()
        .and_then(|route_id| world.state.logistics_routes.get(route_id))?;
    let effective_distance_km = requested_route_ids
        .iter()
        .filter_map(|route_id| world.state.logistics_routes.get(route_id))
        .try_fold(0_i64, |total, route| total.checked_add(route.distance_km))?;
    if effective_distance_km > MATERIAL_TRANSFER_MAX_DISTANCE_KM {
        return None;
    }
    let path_unavailable = requested_route_ids.iter().any(|route_id| {
        world
            .state
            .logistics_routes
            .get(route_id)
            .is_some_and(|route| !route.available)
    });
    Some((
        LogisticsTransferPathPlan {
            effective_kind: selected_route.kind.clone(),
            route_priority: Some(selected_route.priority),
            effective_distance_km,
            path_id: logistics_path_id(requested_route_ids.as_slice()),
            route_ids: requested_route_ids,
            tariff_electricity_total: key.1,
            reroute_count: 0,
        },
        path_unavailable,
    ))
}

fn resolver_error_allows_blocked_quote(reason: &RejectReason) -> bool {
    matches!(
        reason,
        RejectReason::RuleDenied { notes }
            if notes.iter().any(|note| {
                note == "requested logistics path is unavailable or at capacity"
                    || note == "no available logistics path to destination"
            })
    )
}

impl World {
    pub fn pending_material_transits_len(&self) -> usize {
        self.state.pending_material_transits.len()
    }

    /// Derives the existing transfer priority, loss, and timing rules without
    /// reserving materials or consuming a transit slot.
    #[expect(
        clippy::too_many_arguments,
        reason = "Public quote API preserves its stable transfer request argument order."
    )]
    pub fn logistics_transfer_quote(
        &self,
        requester_agent_id: &str,
        from_ledger: &MaterialLedgerId,
        to_ledger: &MaterialLedgerId,
        kind: &str,
        amount: i64,
        distance_km: i64,
        requested_priority: Option<MaterialTransitPriority>,
    ) -> Result<LogisticsTransferQuote, RejectReason> {
        self.logistics_transfer_quote_with_path(
            requester_agent_id,
            from_ledger,
            to_ledger,
            kind,
            amount,
            distance_km,
            requested_priority,
            &[],
            false,
        )
    }

    /// Derives a route-aware transfer quote without reserving materials, capacity, or a
    /// journal entry. Empty route bindings retain the legacy direct-transfer semantics.
    #[expect(
        clippy::too_many_arguments,
        reason = "Public route-aware quote API preserves its stable transfer request argument order."
    )]
    pub fn logistics_transfer_quote_with_path(
        &self,
        requester_agent_id: &str,
        from_ledger: &MaterialLedgerId,
        to_ledger: &MaterialLedgerId,
        kind: &str,
        amount: i64,
        distance_km: i64,
        requested_priority: Option<MaterialTransitPriority>,
        route_ids: &[String],
        auto_reroute: bool,
    ) -> Result<LogisticsTransferQuote, RejectReason> {
        self.logistics_transfer_quote_with_route_id(
            requester_agent_id,
            from_ledger,
            to_ledger,
            kind,
            amount,
            distance_km,
            requested_priority,
            None,
            route_ids,
            auto_reroute,
        )
    }

    /// Derives a route-aware transfer quote while preserving the legacy route
    /// binding as a distinct input for exact action/quote tuple validation.
    #[expect(
        clippy::too_many_arguments,
        reason = "Public route-bound quote API preserves its stable transfer request argument order."
    )]
    pub fn logistics_transfer_quote_with_route_id(
        &self,
        requester_agent_id: &str,
        from_ledger: &MaterialLedgerId,
        to_ledger: &MaterialLedgerId,
        kind: &str,
        amount: i64,
        distance_km: i64,
        requested_priority: Option<MaterialTransitPriority>,
        route_id: Option<&str>,
        route_ids: &[String],
        auto_reroute: bool,
    ) -> Result<LogisticsTransferQuote, RejectReason> {
        if !self.state.agents.contains_key(requester_agent_id) {
            return Err(RejectReason::AgentNotFound {
                agent_id: requester_agent_id.to_string(),
            });
        }
        if from_ledger == to_ledger {
            return Err(RejectReason::RuleDenied {
                notes: vec!["from_ledger and to_ledger cannot be the same".to_string()],
            });
        }
        if kind.trim().is_empty() {
            return Err(RejectReason::RuleDenied {
                notes: vec!["material kind cannot be empty".to_string()],
            });
        }
        if amount <= 0 {
            return Err(RejectReason::InvalidAmount { amount });
        }
        if distance_km < 0 {
            return Err(RejectReason::RuleDenied {
                notes: vec!["distance_km must be >= 0".to_string()],
            });
        }
        if distance_km > MATERIAL_TRANSFER_MAX_DISTANCE_KM {
            return Err(RejectReason::MaterialTransferDistanceExceeded {
                distance_km,
                max_distance_km: MATERIAL_TRANSFER_MAX_DISTANCE_KM,
            });
        }

        let (path_plan, route_resolution_available, path_unavailable) =
            match resolve_logistics_transfer_path(
                self,
                from_ledger,
                to_ledger,
                kind,
                amount,
                distance_km,
                requested_priority,
                route_id,
                route_ids,
                auto_reroute,
            ) {
                Ok(plan) => (Some(plan), true, false),
                Err(reason) if resolver_error_allows_blocked_quote(&reason) => {
                    if let Some((blocked_plan, path_unavailable)) = blocked_route_path_plan(
                        self,
                        from_ledger,
                        to_ledger,
                        kind,
                        amount,
                        distance_km,
                        requested_priority,
                        route_id,
                        route_ids,
                    ) {
                        // A quote reports a conditional, non-mutating capacity blocker while the
                        // action returns the same rejection reason from the shared resolver. Retain
                        // the validated path metadata so the player can identify and compare it.
                        (Some(blocked_plan), false, path_unavailable)
                    } else {
                        return Err(reason);
                    }
                }
                Err(reason) => return Err(reason),
            };

        let requested_route_ids = if route_ids.is_empty() {
            route_id
                .map(|route_id| vec![route_id.to_string()])
                .unwrap_or_default()
        } else {
            route_ids.to_vec()
        };
        let effective_kind = path_plan
            .as_ref()
            .map(|plan| plan.effective_kind.as_str())
            .or_else(|| {
                requested_route_ids
                    .first()
                    .and_then(|route_id| self.state.logistics_routes.get(route_id))
                    .map(|route| route.kind.as_str())
            })
            .unwrap_or(kind);
        let source_amount_before = self.ledger_material_balance(from_ledger, effective_kind);
        let destination_amount_before = self.ledger_material_balance(to_ledger, effective_kind);
        let route_capacity = if path_unavailable {
            Some(0)
        } else {
            path_plan
                .as_ref()
                .and_then(|plan| route_capacity_available_amount(self, plan.route_ids.as_slice()))
                .or_else(|| route_capacity_available_amount(self, requested_route_ids.as_slice()))
        };
        let max_transferable_amount = source_amount_before
            .max(0)
            .min(route_capacity.unwrap_or(i64::MAX));
        let (effective_priority, priority_reason) = match requested_priority {
            Some(priority) => (priority, "explicit_priority".to_string()),
            None => (
                path_plan
                    .as_ref()
                    .and_then(|plan| plan.route_priority)
                    .unwrap_or_else(|| material_transit_priority_for_kind(self, effective_kind)),
                if path_plan
                    .as_ref()
                    .and_then(|plan| plan.route_priority)
                    .is_some()
                {
                    "route_priority".to_string()
                } else {
                    "material_default_priority".to_string()
                },
            ),
        };
        let effective_distance_km = path_plan
            .as_ref()
            .map(|plan| plan.effective_distance_km)
            .unwrap_or(distance_km);
        let loss_bps = if effective_distance_km == 0 {
            0
        } else {
            material_transit_loss_bps_for_kind(self, effective_kind)
        };
        let expected_loss_amount =
            material_transit_loss_amount(amount, effective_distance_km, loss_bps);
        let expected_received_amount = amount.saturating_sub(expected_loss_amount);
        let path_bound = path_plan
            .as_ref()
            .is_some_and(|plan| plan.path_id.is_some());
        let ticks_until_arrival =
            material_transit_ticks(effective_distance_km).max(u64::from(path_bound));
        let inflight_before = self.state.pending_material_transits.len();
        let tariff_electricity_total = path_plan
            .as_ref()
            .map(|plan| plan.tariff_electricity_total)
            .unwrap_or(0);
        let electricity_available = self
            .state
            .agents
            .get(requester_agent_id)
            .map(|cell| cell.state.resources.get(ResourceKind::Electricity))
            .unwrap_or_default();
        let capacity_available = if path_bound || effective_distance_km > 0 {
            inflight_before < MATERIAL_TRANSFER_MAX_INFLIGHT
        } else {
            true
        };
        let path_capacity_available = path_plan
            .as_ref()
            .map(|plan| {
                plan.route_ids.iter().all(|route_id| {
                    self.state
                        .logistics_routes
                        .get(route_id)
                        .is_some_and(|route| route_available_for_amount(route, amount))
                })
            })
            .unwrap_or(false);
        let submission_feasible = route_resolution_available
            && source_amount_before >= amount
            && capacity_available
            && path_capacity_available
            && electricity_available >= tariff_electricity_total;
        let (sent_amount, expected_loss_amount, expected_received_amount) = if submission_feasible {
            (amount, expected_loss_amount, expected_received_amount)
        } else {
            (0, 0, 0)
        };
        let recommendation = if path_unavailable {
            "route_unavailable"
        } else if source_amount_before < amount {
            "reduce_amount_or_source_materials"
        } else if !path_capacity_available
            || effective_distance_km > 0 && inflight_before >= MATERIAL_TRANSFER_MAX_INFLIGHT
        {
            "wait_for_transit_capacity"
        } else if electricity_available < tariff_electricity_total {
            "restore_power_or_use_lower_tariff_route"
        } else if effective_distance_km == 0 {
            "submit_immediate_transfer"
        } else {
            "submit_transfer"
        };

        Ok(LogisticsTransferQuote {
            requester_agent_id: requester_agent_id.to_string(),
            from_ledger: from_ledger.clone(),
            to_ledger: to_ledger.clone(),
            kind: effective_kind.to_string(),
            requested_amount: amount,
            submission_feasible,
            max_transferable_amount,
            sent_amount,
            distance_km: effective_distance_km,
            loss_bps,
            expected_loss_amount,
            expected_received_amount,
            source_amount_before,
            source_amount_after: source_amount_before.saturating_sub(sent_amount),
            destination_amount_before,
            destination_expected_amount_after: destination_amount_before
                .saturating_add(expected_received_amount),
            ticks_until_arrival,
            ready_at: self
                .state
                .time
                .saturating_add(1)
                .saturating_add(ticks_until_arrival),
            effective_priority,
            priority_reason,
            inflight_before,
            inflight_capacity: MATERIAL_TRANSFER_MAX_INFLIGHT,
            path_id: path_plan.as_ref().and_then(|plan| plan.path_id.clone()),
            route_ids: path_plan
                .as_ref()
                .map(|plan| plan.route_ids.clone())
                .unwrap_or_default(),
            tariff_electricity_total,
            reroute_count: path_plan
                .as_ref()
                .map(|plan| plan.reroute_count)
                .unwrap_or(0),
            recommendation: recommendation.to_string(),
            conditional: true,
        })
    }

    fn prepare_due_material_transits_at(&self, now: WorldTime) -> PreparedDueMaterialTransits {
        let mut due_jobs: Vec<_> = self
            .state
            .pending_material_transits
            .values()
            .filter(|job| job.ready_at <= now)
            .cloned()
            .collect();
        due_jobs.sort_by_key(|job| (job.ready_at, job.priority, job.job_id));

        let mut logistics_sla_metrics = self.logistics_sla_metrics.clone();
        let mut bodies = Vec::with_capacity(due_jobs.len());
        for job in due_jobs {
            let loss_amount =
                material_transit_loss_amount(job.amount, job.distance_km, job.loss_bps);
            let received_amount = job.amount.saturating_sub(loss_amount);
            logistics_sla_metrics.record_completion(job.ready_at, now, job.priority);
            bodies.push(WorldEventBody::Domain(
                DomainEvent::MaterialTransitCompleted {
                    job_id: job.job_id,
                    requester_agent_id: job.requester_agent_id,
                    from_ledger: job.from_ledger,
                    to_ledger: job.to_ledger,
                    kind: job.kind,
                    sent_amount: job.amount,
                    received_amount,
                    loss_amount,
                    distance_km: job.distance_km,
                    priority: job.priority,
                    route_id: job.route_id,
                    path_id: job.path_id,
                    route_ids: job.route_ids,
                    tariff_electricity_total: job.tariff_electricity_total,
                    reroute_count: job.reroute_count,
                },
            ));
        }
        PreparedDueMaterialTransits {
            bodies,
            logistics_sla_metrics,
        }
    }

    #[cfg(test)]
    pub(crate) fn prepared_due_material_transits_for_test(
        &self,
        now: WorldTime,
    ) -> (Vec<(u64, MaterialTransitPriority)>, LogisticsSlaMetrics) {
        let prepared = self.prepare_due_material_transits_at(now);
        let ordered = prepared
            .bodies
            .iter()
            .filter_map(|body| match body {
                WorldEventBody::Domain(DomainEvent::MaterialTransitCompleted {
                    job_id,
                    priority,
                    ..
                }) => Some((*job_id, *priority)),
                _ => None,
            })
            .collect();
        (ordered, prepared.logistics_sla_metrics)
    }

    pub(super) fn process_due_material_transits(&mut self) -> Result<Vec<WorldEvent>, WorldError> {
        let prepared = self.prepare_due_material_transits_at(self.state.time);
        let mut emitted = Vec::with_capacity(prepared.bodies.len());
        for body in prepared.bodies {
            self.append_event(body, None)?;
            if let Some(event) = self.journal.events.last() {
                emitted.push(event.clone());
            }
        }
        self.logistics_sla_metrics = prepared.logistics_sla_metrics;

        Ok(emitted)
    }
}
