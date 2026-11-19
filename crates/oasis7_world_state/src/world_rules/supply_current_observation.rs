//! Current V2 observation data. V1 WorldState JSON hashes remain unchanged.
//! These DTOs alone prove neither provenance nor emergency authorization.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::io::{self, Write};

const EVIDENCE_DOMAIN_V2: &str = "oasis7.emergency-evidence.current.v2";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EmergencyCurrentObservationV2 {
    pub format: u16,
    pub window_start: u64,
    /// Digest of the genuine domain-separated Current cut original.
    pub current_commitment: String,
    /// Actual persisted original locator; consumers must resolve and verify it.
    pub source_original: String,
    pub obligation_ids: BTreeSet<String>,
    pub normal_coverage: BTreeMap<String, i64>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EmergencyCurrentEvidenceV2 {
    pub format: u16,
    pub region_id: String,
    pub region_budget_lineage: String,
    pub source_ref: String,
    pub observations: Vec<EmergencyCurrentObservationV2>,
}

/// Syntax only. Registration must also verify the genuine Current original,
/// exact observation cut and corresponding accepted-parent binding.
pub fn validate_current_observation_shape_v2(
    value: &EmergencyCurrentObservationV2,
) -> Result<(), crate::WorldError> {
    fn digest(value: &str) -> bool {
        value.len() == 64
            && value
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    }
    if value.format != 2
        || !value.window_start.is_multiple_of(60)
        || !digest(&value.current_commitment)
        || !digest(&value.source_original)
        || value.obligation_ids.is_empty()
        || value.obligation_ids.len() > 4096
        || value.normal_coverage.len() != value.obligation_ids.len()
        || !value.normal_coverage.keys().eq(value.obligation_ids.iter())
    {
        return Err(super::invalid("invalid_current_observation_v2"));
    }
    Ok(())
}

/// Policy and exact registered-observation comparison only. The runtime must
/// first authenticate V2 originals/source_ref under the same version; this
/// function cannot register a caller DTO or grant an authorization.
pub fn validate_current_evidence_policy_v2(
    evidence: &EmergencyCurrentEvidenceV2,
    services: &BTreeMap<String, oasis7_agent_api::world_action::EmergencyServiceRegistrationV1>,
    registered: &BTreeMap<String, EmergencyCurrentObservationV2>,
    region_lineage: &str,
    now: u64,
    require_shortage: bool,
) -> Result<bool, crate::WorldError> {
    fn bad() -> crate::WorldError {
        super::invalid("invalid_current_evidence_v2")
    }
    if evidence.format != 2
        || evidence.region_id.is_empty()
        || evidence.region_id.len() > 128
        || evidence.region_budget_lineage != region_lineage
        || region_lineage.is_empty()
        || evidence.observations.len() != 3
    {
        return Err(bad());
    }
    if evidence.source_ref != current_evidence_source_ref_v2(evidence)? {
        return Err(bad());
    }
    let end = evidence.observations[2]
        .window_start
        .checked_add(60)
        .ok_or_else(|| super::invalid("numeric_overflow"))?;
    if end > now || now - end > 60 {
        return Err(super::invalid("evidence_stale"));
    }
    let first = evidence.observations[0].window_start;
    // Bounded callers prepay this actual selected registry traversal and set.
    let eligible: BTreeSet<String> = services
        .values()
        .filter(|service| {
            service.region_id == evidence.region_id
                && service
                    .registered_at
                    .checked_add(600)
                    .is_some_and(|t| t <= first)
        })
        .map(|service| service.obligation_id.clone())
        .collect();
    if eligible.is_empty() || eligible.len() > 4096 {
        return Err(bad());
    }
    let mut recovered = true;
    for (index, observation) in evidence.observations.iter().enumerate() {
        validate_current_observation_shape_v2(observation)?;
        let expected = first
            .checked_add(index as u64 * 60)
            .ok_or_else(|| super::invalid("numeric_overflow"))?;
        if observation.window_start != expected || observation.obligation_ids != eligible {
            return Err(bad());
        }
        let key = current_observation_key_v2(&evidence.region_id, expected);
        if registered.get(&key) != Some(observation) {
            return Err(bad());
        }
        recovered &= super::supply::observation_coverage_status_v1(
            services,
            &eligible,
            &observation.normal_coverage,
            require_shortage,
        )?;
    }
    Ok(recovered)
}

/// Exact versioned evidence digest, independent of V1 JSON/WorldState hashes.
/// This checks data identity only; callers still authenticate each original.
/// Bounded consumers prepay serialization visits, bytes and the returned text.
pub fn current_evidence_source_ref_v2(
    evidence: &EmergencyCurrentEvidenceV2,
) -> Result<String, crate::WorldError> {
    struct HashWriter(Sha256);
    impl Write for HashWriter {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.0.update(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let mut hash = HashWriter(Sha256::new());
    // Exclude source_ref itself. Field order and domain are part of V2.
    serde_json::to_writer(
        &mut hash,
        &(
            EVIDENCE_DOMAIN_V2,
            evidence.format,
            &evidence.region_id,
            &evidence.region_budget_lineage,
            &evidence.observations,
        ),
    )
    .map_err(|_| super::invalid("invalid_current_evidence_v2"))?;
    let digest = hash.0.finalize();
    let mut result = String::with_capacity("current-observations-v2:".len() + 64);
    result.push_str("current-observations-v2:");
    const HEX: &[u8; 16] = b"0123456789abcdef";
    for byte in digest {
        result.push(HEX[(byte >> 4) as usize] as char);
        result.push(HEX[(byte & 15) as usize] as char);
    }
    Ok(result)
}

pub fn current_observation_key_v2(region: &str, window: u64) -> String {
    format!(
        "current-observation-v2:{}",
        blake3::hash(format!("{}:{region}:{window}", region.len()).as_bytes()).to_hex()
    )
}
