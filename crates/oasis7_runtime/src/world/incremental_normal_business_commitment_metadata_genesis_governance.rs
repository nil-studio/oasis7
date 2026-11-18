//! GEN governance DATA from the same immutable World as the emitted columns.
//! The retained source grants no Current, finality or publication authority.
use super::*;
use oasis7_client_api::world_service::{
    FormalGovernanceMembershipV1, PublicMembershipCandidate, PublicMembershipRegistry,
};
use std::collections::BTreeMap;

pub(in crate::world) struct GenesisGovernanceSourceDataV1 {
    epoch: u64,
    candidates: BTreeMap<String, PublicMembershipCandidate>,
    selected: BTreeMap<String, String>,
    registry: Option<oasis7_world_state::GovernanceFinalitySignerRegistry>,
    core: WorkIdentityV1,
    fee: MemoryReservation,
}
pub(in crate::world) struct GenesisGovernanceProjectionDataV1 {
    value: FormalGovernanceMembershipV1,
    core: WorkIdentityV1,
    fee: MemoryReservation,
}

#[derive(Debug)]
struct GovernanceCause {
    error: crate::WorldError,
    _fee: MemoryReservation,
}
impl std::fmt::Display for GovernanceCause {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.error)
    }
}
impl std::error::Error for GovernanceCause {}

impl crate::World {
    pub(in crate::world) fn capture_genesis_governance_source_data_v1(
        &self,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<GenesisGovernanceSourceDataV1, BodyMutationError> {
        // Reuse the existing Current metadata's actual-Serde graph admission.
        // Only these four original governance owners are traversed. There is
        // no WorldState clone, guessed whole-World multiplier or new Core.
        let input = (
            &self.state.governance_validator_admissions,
            &self.state.governance_finality_signer_registry,
            &self.state.governance_finality_key_rotations,
            &self.state.governance_bootstrap_finality_key_rotations,
        );
        let (_, mut fee) = super::super::metadata::admit_serializable_phase(&input, common)?;
        let outer = std::mem::size_of::<GenesisGovernanceSourceDataV1>();
        common.charge(1, outer as u64).map_err(original)?;
        let mut header = common
            .memory()
            .reserve(outer, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        fee.merge(&mut header).map_err(BodyMutationError::Invalid)?;
        let records=crate::world::governance::module_admission_finality::ModuleFinalityRecordsV1::from_world(self);
        let epoch = self.current_governance_epoch();
        let registry = match records
            .resolve_governance_effective_finality_signer_registry_from_admissions_at_epoch(
                &self.state.governance_validator_admissions,
                epoch,
            ) {
            Ok(value) => value,
            Err(error) => return Err(original(GovernanceCause { error, _fee: fee })),
        };
        common.consume_steps(0).map_err(original)?;
        let selected_rows = crate::World::governance_effective_validator_admission_records(
            &self.state.governance_validator_admissions,
        );
        let mut selected = BTreeMap::new();
        for (node, row) in selected_rows {
            common
                .charge(1, (node.len() + row.candidate_id.len()) as u64)
                .map_err(original)?;
            selected.insert(node, row.candidate_id.clone());
        }
        let mut candidates = BTreeMap::new();
        for (key, row) in &self.state.governance_validator_admissions {
            common.charge(1, key.len() as u64).map_err(original)?;
            candidates.insert(
                key.clone(),
                crate::governance_projection::public_candidate(row.clone()),
            );
        }
        common.consume_steps(0).map_err(original)?;
        Ok(GenesisGovernanceSourceDataV1 {
            epoch,
            candidates,
            selected,
            registry,
            core: common.work().identity(),
            fee,
        })
    }
}
impl GenesisGovernanceSourceDataV1 {
    pub(in crate::world) fn validate_context(
        &self,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<(), BodyMutationError> {
        common.consume_steps(1).map_err(original)?;
        if self.core != common.work().identity() || !self.fee.uses_budget(common.memory()) {
            return Err(bad());
        }
        Ok(())
    }
    pub(in crate::world) fn prepare_projection_data_v1(
        &self,
        metadata: &PreparedNormalBusinessMetadataGenesisV1,
        common: &mut FormalBoundedContextV1<'_>,
        canonical: &mut oasis7_wasm_abi::CanonicalCborWorkV1,
    ) -> Result<GenesisGovernanceProjectionDataV1, BodyMutationError> {
        self.validate_context(common)?;
        metadata.validate_context(common)?;
        let digest = crate::world::normal_bootstrap_current::hash::with_current_meter_v1(
            common,
            canonical,
            |meter| {
                oasis7_client_api::world_service::request_digest_with_work_v1(
                    "governance-membership-registry-v1",
                    &self.registry,
                    meter,
                )
            },
        )?
        .map_err(original)?;
        // The exact selected DTO, including the original digest and original
        // semantic State hash, is admitted before its owned graph is copied.
        let borrowed = (
            &self.candidates,
            &self.selected,
            &self.registry,
            digest.as_str(),
            metadata.raw_state_hash_for_bootstrap_v1(),
        );
        let (_, mut fee) = super::super::metadata::admit_serializable_phase(&borrowed, common)?;
        common
            .charge(
                1,
                std::mem::size_of::<GenesisGovernanceProjectionDataV1>() as u64,
            )
            .map_err(original)?;
        let mut header = common
            .memory()
            .reserve(
                std::mem::size_of::<GenesisGovernanceProjectionDataV1>(),
                MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)?;
        fee.merge(&mut header).map_err(BodyMutationError::Invalid)?;
        let effective_registry = self.registry.as_ref().map(|r| PublicMembershipRegistry {
            slot_id: r.slot_id.clone(),
            threshold: r.threshold,
            threshold_bps: r.threshold_bps,
            signer_bindings: r.signer_bindings.clone(),
            validator_stakes: r.validator_stakes.clone(),
        });
        let value = FormalGovernanceMembershipV1 {
            schema_version: 1,
            state_root_hash: metadata.raw_state_hash_for_bootstrap_v1().to_owned(),
            governance_epoch: self.epoch,
            candidates: self.candidates.clone(),
            selected_candidates: self.selected.clone(),
            effective_registry,
            effective_registry_digest: digest.as_str().to_owned(),
        };
        common.consume_steps(0).map_err(original)?;
        Ok(GenesisGovernanceProjectionDataV1 {
            value,
            core: common.work().identity(),
            fee,
        })
    }
}
impl GenesisGovernanceProjectionDataV1 {
    pub(in crate::world) fn validate_context(
        &self,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<(), BodyMutationError> {
        common.consume_steps(1).map_err(original)?;
        if self.core != common.work().identity() || !self.fee.uses_budget(common.memory()) {
            return Err(bad());
        }
        Ok(())
    }
    pub(in crate::world) fn membership_parts_v1(
        &self,
        raw_state: &str,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<(FormalGovernanceMembershipV1, MemoryReservation), BodyMutationError> {
        self.validate_context(common)?;
        common
            .charge(
                1,
                (raw_state.len() + self.value.state_root_hash.len()) as u64,
            )
            .map_err(original)?;
        if raw_state != self.value.state_root_hash {
            return Err(bad());
        }
        let (_, fee) = super::super::metadata::admit_serializable_phase(&self.value, common)?;
        let value = self.value.clone();
        common.consume_steps(0).map_err(original)?;
        Ok((value, fee))
    }
}
