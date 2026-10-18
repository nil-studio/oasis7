use std::collections::BTreeMap;

use super::super::types::ResourceKind;
use super::micro_depot::{
    MICRO_DEPOT_INITIAL_INVENTORY_UNITS_PER_KIND, MICRO_DEPOT_INSTALL_DATA_COST,
    MICRO_DEPOT_INSTALL_SINK_DATA_COST, MICRO_DEPOT_THROUGHPUT_LIMIT_UNITS_PER_EPOCH,
};
use super::types::MicroDepotResourceDebit;

pub(super) struct MicroDepotCommissioning {
    pub install_cost_resources: Vec<MicroDepotResourceDebit>,
    pub sink_resources: Vec<MicroDepotResourceDebit>,
    pub inventory_by_kind: BTreeMap<String, i64>,
    pub throughput_limit_units_per_epoch: i64,
    pub throughput_remaining_units: i64,
}

impl MicroDepotCommissioning {
    pub(super) fn v2() -> Self {
        Self {
            install_cost_resources: vec![MicroDepotResourceDebit {
                kind: ResourceKind::Data,
                amount: MICRO_DEPOT_INSTALL_DATA_COST,
            }],
            sink_resources: vec![MicroDepotResourceDebit {
                kind: ResourceKind::Data,
                amount: MICRO_DEPOT_INSTALL_SINK_DATA_COST,
            }],
            inventory_by_kind: [(
                "data".to_string(),
                MICRO_DEPOT_INITIAL_INVENTORY_UNITS_PER_KIND,
            )]
            .into(),
            throughput_limit_units_per_epoch: MICRO_DEPOT_THROUGHPUT_LIMIT_UNITS_PER_EPOCH,
            throughput_remaining_units: MICRO_DEPOT_THROUGHPUT_LIMIT_UNITS_PER_EPOCH,
        }
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "Stable commissioning command seam keeps deterministic validation inputs explicit."
)]
pub(super) fn validate_install_commissioning(
    measured_supply_schema_version: u8,
    facility_id: &str,
    supported_resource_kinds: &[String],
    install_cost_resources: &[MicroDepotResourceDebit],
    sink_resources: &[MicroDepotResourceDebit],
    inventory_by_kind: &BTreeMap<String, i64>,
    throughput_limit_units_per_epoch: i64,
    throughput_remaining_units: i64,
) -> Result<bool, String> {
    match measured_supply_schema_version {
        0 | 1 => {
            if !sink_resources.is_empty()
                || !inventory_by_kind.is_empty()
                || throughput_limit_units_per_epoch != 0
                || throughput_remaining_units != 0
            {
                return Err(format!(
                    "micro_depot legacy install carries v2 commissioning provenance: {facility_id}"
                ));
            }
            return Ok(false);
        }
        2 => {}
        version => {
            return Err(format!(
                "unsupported micro_depot measured supply schema version {version}: {facility_id}"
            ));
        }
    }
    let expected = MicroDepotCommissioning::v2();
    if supported_resource_kinds != ["data"] {
        return Err(format!(
            "micro_depot v2 install requires canonical supported resources [data]: {facility_id}"
        ));
    }
    if install_cost_resources != expected.install_cost_resources {
        return Err(format!(
            "micro_depot v2 install requires exact Data commissioning debit: {facility_id}"
        ));
    }
    if sink_resources != expected.sink_resources
        || inventory_by_kind != &expected.inventory_by_kind
        || throughput_limit_units_per_epoch != expected.throughput_limit_units_per_epoch
        || throughput_remaining_units != throughput_limit_units_per_epoch
    {
        return Err(format!(
            "micro_depot v2 install has invalid commissioning provenance: {facility_id}"
        ));
    }
    let commissioned_total = sink_resources[0]
        .amount
        .checked_add(*inventory_by_kind.get("data").unwrap())
        .ok_or_else(|| {
            format!("micro_depot v2 commissioning provenance overflows: {facility_id}")
        })?;
    if commissioned_total != install_cost_resources[0].amount {
        return Err(format!(
            "micro_depot v2 commissioning provenance does not reconcile: {facility_id}"
        ));
    }
    Ok(true)
}
