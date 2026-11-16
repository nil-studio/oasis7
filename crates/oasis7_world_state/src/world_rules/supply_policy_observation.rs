//! Original observation validation over admitted evidence and caller-supplied budgets.
use super::*;

/// Shared original numeric policy; this does not certify source provenance.
/// Callers validate the exact complete obligation/coverage key sets first.
/// Fee-only events for the original observation reducer. They cannot change
/// policy data or replace a predicate. The caller retains admitted heap until
/// this reducer returns; every event also provides a cancellation boundary.
pub enum EmergencyObservationWorkV1 {
    Read { bytes: usize },
    OwnedId { bytes: usize },
    BorrowedSetNode,
}
pub enum EmergencyObservationValidationErrorV1 {
    Policy(WorldError),
    Resource(oasis7_formal_protocol::incremental_value::ValueTreeError),
}
impl From<WorldError> for EmergencyObservationValidationErrorV1 {
    fn from(value: WorldError) -> Self {
        Self::Policy(value)
    }
}
fn original_observation_result(
    value: Result<bool, EmergencyObservationValidationErrorV1>,
) -> Result<bool, WorldError> {
    match value {
        Ok(v) => Ok(v),
        Err(EmergencyObservationValidationErrorV1::Policy(e)) => Err(e),
        Err(EmergencyObservationValidationErrorV1::Resource(_)) => {
            unreachable!("the original no-op fee callback cannot fail")
        }
    }
}
pub fn observation_coverage_status_v1(
    services: &BTreeMap<String, Service>,
    eligible: &BTreeSet<String>,
    coverage: &BTreeMap<String, i64>,
    require_shortage: bool,
) -> Result<bool, WorldError> {
    original_observation_result(observation_coverage_with_work_v1(
        services,
        eligible,
        coverage,
        require_shortage,
        &mut |_| Ok(()),
    ))
}
fn observation_coverage_with_work_v1<K, S>(
    services: &BTreeMap<K, S>,
    eligible: &BTreeSet<String>,
    coverage: &BTreeMap<String, i64>,
    require_shortage: bool,
    fee: &mut impl FnMut(
        EmergencyObservationWorkV1,
    ) -> Result<(), oasis7_formal_protocol::incremental_value::ValueTreeError>,
) -> Result<bool, EmergencyObservationValidationErrorV1>
where
    K: Ord + std::borrow::Borrow<str>,
    S: std::borrow::Borrow<Service>,
{
    use EmergencyObservationValidationErrorV1::Resource;
    use EmergencyObservationWorkV1::*;
    let mut total = 0i128;
    let mut missing = 0i128;
    let mut all = BTreeSet::new();
    let mut affected = BTreeSet::new();
    let mut places = BTreeSet::new();
    for id in eligible {
        fee(Read { bytes: id.len() }).map_err(Resource)?;
        let s: &Service = services
            .get(id.as_str())
            .expect("original eligible service lookup")
            .borrow();
        let normal = coverage[id];
        if normal < 0
            || normal
                > s.electricity_per_interval
                    .checked_mul(10)
                    .ok_or_else(|| invalid("numeric_overflow"))?
        {
            return Err(invalid("invalid_evidence").into());
        }
        total += i128::from(s.electricity_per_interval) * 10;
        missing += i128::from(s.electricity_per_interval) * 10 - i128::from(normal);
        fee(Read {
            bytes: s.controller_id.len(),
        })
        .map_err(Resource)?;
        fee(BorrowedSetNode).map_err(Resource)?;
        all.insert(&s.controller_id);
        if i128::from(normal) < i128::from(s.electricity_per_interval) * 10 {
            fee(Read {
                bytes: s.controller_id.len(),
            })
            .map_err(Resource)?;
            fee(BorrowedSetNode).map_err(Resource)?;
            affected.insert(&s.controller_id);
            fee(Read {
                bytes: s.location_id.len(),
            })
            .map_err(Resource)?;
            fee(BorrowedSetNode).map_err(Resource)?;
            places.insert(&s.location_id);
        }
    }
    let recovered = total > 0 && missing * 100 < total * 10;
    if total == 0
        || require_shortage
            && (missing * 100 < total * 30
                || affected.len() < 3
                || places.len() < 2
                || affected.len() * 100 < all.len() * 30)
    {
        return Err(invalid("invalid_evidence").into());
    }
    Ok(recovered)
}
/// Original lazy observation prefix, shared before any service source is read.
pub fn validate_emergency_observation_prefix_v1(e: &Evidence, now: u64) -> Result<(), WorldError> {
    require_text(&e.region_id)?;
    require_text(&e.region_budget_lineage)?;
    require_text(&e.source_ref)?;
    if e.observations.len() != 3 || !fresh(e, now) {
        return Err(invalid("evidence_stale"));
    }
    Ok(())
}
/// Original V1 observation policy. The legacy no-op callback preserves the
/// original policy/error order; Current uses the same borrowed reducer below.
pub fn validate_emergency_observations_from_services_v1(
    services: &BTreeMap<String, Service>,
    e: &Evidence,
    now: u64,
    require_shortage: bool,
) -> Result<bool, WorldError> {
    original_observation_result(validate_emergency_observations_with_work_v1(
        services,
        e,
        now,
        require_shortage,
        &mut |_| Ok(()),
    ))
}
/// Borrowed actual service records are DATA, never source qualification. Only
/// the fixed Current consumer may grant a completion cap after real joins.
pub fn validate_emergency_observations_with_work_v1<K, S>(
    services: &BTreeMap<K, S>,
    e: &Evidence,
    now: u64,
    require_shortage: bool,
    fee: &mut impl FnMut(
        EmergencyObservationWorkV1,
    ) -> Result<(), oasis7_formal_protocol::incremental_value::ValueTreeError>,
) -> Result<bool, EmergencyObservationValidationErrorV1>
where
    K: Ord + std::borrow::Borrow<str>,
    S: std::borrow::Borrow<Service>,
{
    use EmergencyObservationValidationErrorV1::Resource;
    use EmergencyObservationWorkV1::*;
    fee(Read {
        bytes: e.region_id.len(),
    })
    .map_err(Resource)?;
    fee(Read {
        bytes: e.region_budget_lineage.len(),
    })
    .map_err(Resource)?;
    fee(Read {
        bytes: e.source_ref.len(),
    })
    .map_err(Resource)?;
    validate_emergency_observation_prefix_v1(e, now)?;
    let first = e.observations[0].window_start;
    let mut eligible = BTreeSet::new();
    for value in services.values() {
        let s: &Service = value.borrow();
        fee(Read {
            bytes: s.region_id.len().min(e.region_id.len()),
        })
        .map_err(Resource)?;
        if s.region_id == e.region_id
            && s.registered_at.checked_add(600).is_some_and(|t| t <= first)
        {
            fee(OwnedId {
                bytes: s.obligation_id.len(),
            })
            .map_err(Resource)?;
            eligible.insert(s.obligation_id.clone());
        }
    }
    if eligible.is_empty() || eligible.len() > 4096 {
        return Err(invalid("invalid_evidence").into());
    }
    let mut recovered = true;
    for (i, w) in e.observations.iter().enumerate() {
        fee(Read { bytes: 0 }).map_err(Resource)?;
        if w.window_start % 60 != 0
            || w.window_start
                != first
                    .checked_add(i as u64 * 60)
                    .ok_or_else(|| invalid("numeric_overflow"))?
        {
            return Err(invalid("invalid_evidence").into());
        }
        // Preserve the original two exact set comparisons and their order.
        for id in &w.obligation_ids {
            fee(Read { bytes: id.len() }).map_err(Resource)?;
        }
        if w.obligation_ids != eligible {
            return Err(invalid("invalid_evidence").into());
        }
        let mut coverage_keys = BTreeSet::new();
        for id in w.normal_coverage.keys() {
            fee(OwnedId { bytes: id.len() }).map_err(Resource)?;
            coverage_keys.insert(id.clone());
        }
        if coverage_keys != eligible {
            return Err(invalid("invalid_evidence").into());
        }
        fee(Read {
            bytes: w.world_root.len(),
        })
        .map_err(Resource)?;
        require_text(&w.world_root)?;
        recovered &= observation_coverage_with_work_v1(
            services,
            &eligible,
            &w.normal_coverage,
            require_shortage,
            fee,
        )?;
    }
    Ok(recovered)
}
/// The original overflow and active-time checks, after evidence lookup.
pub fn validate_emergency_active_window_v1(
    a: &EmergencyAuthorizationV1,
    overflowed: bool,
    now: u64,
) -> Result<(), WorldError> {
    if overflowed {
        return Err(invalid("batch_overflow"));
    }
    if !a.active || now < a.starts_at || now >= a.expires_at {
        return Err(invalid("authorization_inactive"));
    }
    Ok(())
}
/// Original candidate filter; observation validation and Current source checks
/// remain distinct mandatory phases owned by their real source consumers.
pub fn emergency_evidence_supersedes_v1(candidate: &Evidence, e: &Evidence) -> bool {
    candidate.region_id == e.region_id
        && candidate.region_budget_lineage == e.region_budget_lineage
        && evidence_end(candidate) >= evidence_end(e)
}
pub fn validate_emergency_active_freshness_v1(e: &Evidence, now: u64) -> Result<(), WorldError> {
    if !fresh(e, now) {
        return Err(invalid("evidence_stale"));
    }
    Ok(())
}
