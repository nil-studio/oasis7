//! Persisted graph reservations are not observations of dispatched HTTP.
use super::*;
use crate::native_model_graph_v2::NativeModelGraphPortClassV2;
use crate::native_model_proxy_query_v2::SignedNativeModelReservationQueryV2;
use crate::time_units::UnixMillis;
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::provisioning) struct NativeModelReservedPortV2 {
    pub reservation_id: String,
    pub graph_index: u16,
    pub class: NativeModelGraphPortClassV2,
    pub reserved_at: UnixMillis,
}
pub(super) fn port_id(
    query: &SignedNativeModelReservationQueryV2,
    ingress: &str,
    attempt: &str,
    index: usize,
) -> Result<String> {
    digest(&(
        "oasis7.custody.native-model-reserved-port.v2",
        query,
        ingress,
        attempt,
        index,
    ))
}
pub(super) fn validate_ports(
    s: &TestSessionAuthorityV2,
    model: &SessionNativeModelBudgetV2,
) -> Result<()> {
    let mut seen = std::collections::BTreeSet::new();
    for (id, record) in &model.reservations {
        let q = &record.query.query;
        let ports = q.physical_graph.ports(q.call_kind);
        if record.port_ids.len() != ports.len().checked_sub(5).ok_or_else(invalid)?
            || record.physical_units
                != u64::try_from(record.port_ids.len()).map_err(|_| invalid())?
            || super::session_event_ordinal_v2(
                s,
                &super::SessionEventKindV2::NativeModelGraphReserved {
                    reservation_id: id.clone(),
                },
            )? != Some(record.accepted_ordinal)
        {
            return Err(invalid());
        }
        for (offset, key) in record.port_ids.iter().enumerate() {
            let index = offset.checked_add(5).ok_or_else(invalid)?;
            let port = model.graph_ports.get(key).ok_or_else(invalid)?;
            if !seen.insert(key)
                || key
                    != &port_id(
                        &record.query,
                        &record.host_ingress_attempt_id,
                        &record.host_signed_attempt_digest,
                        index,
                    )?
                || port.reservation_id != *id
                || usize::from(port.graph_index) != index
                || port.class != ports[index]
                || port.reserved_at != record.claimed_at
            {
                return Err(invalid());
            }
        }
    }
    if seen.len() != model.graph_ports.len() {
        return Err(invalid());
    }
    Ok(())
}
