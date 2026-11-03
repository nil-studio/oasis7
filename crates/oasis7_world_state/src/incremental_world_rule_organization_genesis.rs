use super::*;
use std::collections::BTreeMap;
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct WorldRuleOrganizationGenesisCountsV1 {
    pub cases: u64,
    pub estate: u64,
    pub grants: u64,
    pub recovery: u64,
}
pub(super) fn owners(
    source: &WorldRulesStateV1,
) -> (
    &BTreeMap<String, OrganizationCaseV1>,
    &BTreeMap<String, OrganizationEstateDepositV1>,
) {
    let WorldRulesStateV1 {
        organization_cases,
        organization_estate,
        requests: _,
        governance_effects: _,
        supply_authorizations: _,
        supply_batches: _,
        supply_lineages: _,
        supply_shipments: _,
        organization_continuity: _,
        emergency_supply: _,
        agent_orders: _,
        industrial_work: _,
        substitution: _,
    } = source;
    (organization_cases, organization_estate)
}
pub(super) fn emit<
    W: ValueNodeWriter,
    C: FnMut() -> bool,
    S: FnMut(ReservedValue<ExecutionWriteV1>) -> Result<(), ValueTreeError>,
>(
    record: K<'_>,
    path: RecordPathV1<'_>,
    payload: MemberPayloadRefV1<'_>,
    context: &mut MemberRecordWriteContext<'_, W, C>,
    remaining: &mut u64,
    sink: &mut S,
) -> Result<(), MemberRecordError> {
    if (context.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    *remaining = remaining
        .checked_sub(1)
        .ok_or(ValueTreeError::ResourceLimited(
            "organization genesis records",
        ))?;
    let row = record.write_path(path, payload, context)?;
    crate::incremental_field_write::emit(row.field_key, row.value_root, context, sink)
}
