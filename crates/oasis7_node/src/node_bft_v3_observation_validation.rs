//! Pure, source-pinned shape validation of a wire-3 worker observation.
//!
//! This is a dark diagnostics contract, not current BFT qualification.
//! Worker-observed lease generation, historical H and applied receipts do not
//! establish protected owner fencing, current membership or signing authority.
use crate::node_bft_v3_runtime::NodeBftObservationV1;

const MAX_WINDOW_HEIGHTS: u64 = 64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BftNodeObservedIdentityPinsV1 {
    pub world_id: String,
    pub chain_id: String,
    pub operator_instance_id: String,
    pub binding_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BftNodeObservedCutV1 {
    pub height: u64,
    pub round: u64,
    pub window_start_height: u64,
    pub committed_window_heights: u64,
    pub observed_lease_generation: u64,
    pub last_applied_height: Option<u64>,
    pub last_applied_header_id: Option<String>,
}

/// Validate only a historical diagnostic against independent operator pins.
/// A successful result must never set can_propose, can_sign or write_ready:
/// those require the actual protected provider, QC and applied receipt chain.
pub fn validate_bft_node_observation_v1(
    observed: &NodeBftObservationV1,
    pins: &BftNodeObservedIdentityPinsV1,
) -> Result<BftNodeObservedCutV1, &'static str> {
    if !valid_text(&pins.world_id)
        || !valid_text(&pins.chain_id)
        || !valid_text(&pins.operator_instance_id)
        || !digest(&pins.binding_digest)
        || observed.world_id != pins.world_id
        || observed.chain_id != pins.chain_id
        || observed.operator_instance_id != pins.operator_instance_id
        || observed.binding_digest != pins.binding_digest
    {
        return Err("wire-3 diagnostic independent identity mismatch");
    }
    if observed.observed_at_unix_ms < 0
        || observed.authority_epoch == 0
        || observed.height == 0
        || observed.window_start_height == 0
        || observed.height < observed.window_start_height
        || observed.committed_window_heights > MAX_WINDOW_HEIGHTS as usize
        || u64::try_from(observed.committed_window_heights)
            .ok()
            .is_none_or(|n| n > observed.height - observed.window_start_height)
        || observed.observed_lease_generation == 0
        || !digest(&observed.active_set_hash)
        || observed.pending_set_hash.as_deref().is_some_and(|h| !digest(h))
    {
        return Err("wire-3 diagnostic window, lease or digest invalid");
    }
    if observed.last_applied_height.is_some() != observed.last_applied_header_id.is_some()
        || observed
            .last_applied_height
            .is_some_and(|applied| applied > observed.height)
        || observed.last_applied_header_id.as_deref().is_some_and(|h| !digest(h))
        || observed.pending_driver_apply && observed.last_applied_height == Some(observed.height)
    {
        return Err("wire-3 diagnostic applied cut inconsistent");
    }
    Ok(BftNodeObservedCutV1 {
        height: observed.height,
        round: observed.round,
        window_start_height: observed.window_start_height,
        committed_window_heights: observed.committed_window_heights as u64,
        observed_lease_generation: observed.observed_lease_generation,
        last_applied_height: observed.last_applied_height,
        last_applied_header_id: observed.last_applied_header_id.clone(),
    })
}

fn valid_text(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 256
        && !value.bytes().any(|b| b.is_ascii_control())
}

fn digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn h(n: u8) -> String {
        hex::encode([n; 32])
    }

    fn pins() -> BftNodeObservedIdentityPinsV1 {
        BftNodeObservedIdentityPinsV1 {
            world_id: "world".into(),
            chain_id: "chain".into(),
            operator_instance_id: "worker-1".into(),
            binding_digest: h(1),
        }
    }

    fn observation() -> NodeBftObservationV1 {
        NodeBftObservationV1 {
            observed_at_unix_ms: 1234,
            world_id: "world".into(),
            chain_id: "chain".into(),
            operator_instance_id: "worker-1".into(),
            authority_epoch: 3,
            height: 11,
            round: 2,
            active_set_hash: h(2),
            pending_set_hash: None,
            binding_digest: h(1),
            checkpoint_revision: 5,
            observed_lease_generation: 7,
            window_start_height: 10,
            committed_window_heights: 1,
            pending_driver_apply: false,
            last_applied_height: Some(10),
            last_applied_header_id: Some(h(3)),
        }
    }

    #[test]
    fn pinned_diagnostic_exposes_exact_historical_cut_without_authority() {
        let cut = validate_bft_node_observation_v1(&observation(), &pins()).unwrap();
        assert_eq!(cut.height, 11);
        assert_eq!(cut.last_applied_height, Some(10));
        assert_eq!(cut.observed_lease_generation, 7);
    }

    #[test]
    fn wrong_independent_pin_or_lease_cannot_be_reused() {
        let mut p = pins();
        p.operator_instance_id = "restarted-worker".into();
        assert!(validate_bft_node_observation_v1(&observation(), &p).is_err());
        let mut o = observation();
        o.observed_lease_generation = 0;
        assert!(validate_bft_node_observation_v1(&o, &pins()).is_err());
        o = observation();
        o.binding_digest = h(90);
        assert!(validate_bft_node_observation_v1(&o, &pins()).is_err());
    }

    #[test]
    fn rejects_window_overrun_wrong_applied_cut_and_malformed_digest() {
        let mut o = observation();
        o.committed_window_heights = 65;
        assert!(validate_bft_node_observation_v1(&o, &pins()).is_err());
        o = observation();
        o.last_applied_height = Some(12);
        assert!(validate_bft_node_observation_v1(&o, &pins()).is_err());
        o = observation();
        o.last_applied_header_id = None;
        assert!(validate_bft_node_observation_v1(&o, &pins()).is_err());
        o = observation();
        o.active_set_hash = "Z".repeat(64);
        assert!(validate_bft_node_observation_v1(&o, &pins()).is_err());
    }
}
