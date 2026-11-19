//! Complete-region Observe preparation from the actual Current directory.
//! The fixed producer retains every selected original through coverage; no
//! caller-provided service list or registration DTO establishes completeness.
use super::delivery::{IndustrialDeliveryErrorV1, IndustrialDeliveryWorkV1};
use super::emergency_close::{
    EmergencyCloseSourcesV1,
    sources::{
        CurrentEmergencyObservationServiceSourcesV1, CurrentEmergencyServiceSelectionV1,
        EmergencyServiceSourceWorkV1, read_current_emergency_observation_service_sources_v1,
    },
};
use super::*;
use crate::world::incremental_normal_tail::NormalTailWorkV1;
use oasis7_formal_protocol::incremental_value::{ValueNodeReader, ValueTreeError};
use oasis7_proto::memory_budget::MemoryClass;
use oasis7_world_state::{
    incremental_field_overlay::WorldFieldOverlayContextV1,
    incremental_field_read::WorldFieldReadContextV1,
    incremental_world_rules_family::{
        CurrentWorldRuleLifecycleDirectoryV1, read_current_emergency_minimum_service_v1,
    },
};
use std::collections::BTreeMap;

pub(in crate::world) struct PreparedCurrentObservationCoverageV2 {
    region: String,
    services: Vec<ReservedValue<CurrentEmergencyObservationServiceSourcesV1>>,
    normal: BTreeMap<String, i64>,
    intervals: BTreeMap<String, i64>,
    guard: PreparedGuard,
    identity: WorkIdentityV1,
    memory: MemoryBudget,
    now: u64,
    // Reservations include container peaks and original-kernel output nodes.
    _fees: Vec<MemoryReservation>,
    _fee_slots: MemoryReservation,
}
impl PreparedCurrentObservationCoverageV2 {
    pub(in crate::world) fn normal(&self) -> &BTreeMap<String, i64> {
        &self.normal
    }
    pub(in crate::world) fn intervals(&self) -> &BTreeMap<String, i64> {
        &self.intervals
    }
    pub(in crate::world) fn validate<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        tail: &mut NormalTailWorkV1<'_, '_>,
        now: u64,
        live: &ExecutionFence,
    ) -> Result<(), IndustrialDeliveryErrorV1> {
        tx.validate_guard(&self.guard, live)?;
        tail.charge_record()?;
        if self.now != now
            || self.identity != tail.identity()
            || !self.memory.same_pool(tail.memory())
            || !tx.uses_budget(tail.memory())
            || self.services.is_empty()
        {
            return Err(BodyMutationError::Invalid("Current observation coverage scope").into());
        }
        Ok(())
    }
}
fn unavailable() -> IndustrialDeliveryErrorV1 {
    crate::world_rules::invalid("need_source_unverifiable").into()
}
fn limited() -> ValueTreeError {
    ValueTreeError::ResourceLimited("Current observation coverage")
}

/// Must run inside the lifecycle's existing paid IO adapter. All explicit work
/// and every reservation use the same tail; the adapter reconciles reader work
/// even on error. No selected source is exposed until the entire region passed.
#[allow(clippy::too_many_arguments)]
pub(in crate::world) fn prepare_current_observation_coverage_v2<M, P, V, R, E, C, PE, PC>(
    region: &str,
    directory: &ReservedValue<CurrentWorldRuleLifecycleDirectoryV1>,
    sources: &EmergencyCloseSourcesV1<'_, '_>,
    state: &M,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    parent: &mut WorldFieldReadContextV1<'_, P, V, PE, PC>,
    work: &mut IndustrialDeliveryWorkV1,
    extra: &mut EmergencyServiceSourceWorkV1<'_>,
    tail: &mut NormalTailWorkV1<'_, '_>,
    now: u64,
    live: &ExecutionFence,
) -> Result<PreparedCurrentObservationCoverageV2, IndustrialDeliveryErrorV1>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    V: ValueNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
{
    (|| {
        if now == 0 || !now.is_multiple_of(60) {
            return Err(crate::world_rules::invalid("missing_authority").into());
        }
        directory.validate(
            ctx.transaction,
            sources.delivery.eligibility.roots,
            tail.memory(),
            live,
        )?;
        let window = now - 60;
        // Fixed maximum region size is the original Observe bound. Reserve
        // the fee handles first, before creating any variable-length output.
        let fee_slots = tail.reserve(
            65536usize
                .checked_mul(std::mem::size_of::<MemoryReservation>())
                .ok_or_else(limited)?,
            MemoryClass::Ordinary,
        )?;
        let mut fees = Vec::new();
        fees.try_reserve_exact(65536).map_err(|_| limited())?;
        if fees.capacity() > 65536 {
            return Err(limited().into());
        }
        fees.push(tail.reserve(
            std::mem::size_of::<PreparedCurrentObservationCoverageV2>(),
            MemoryClass::Ordinary,
        )?);
        tail.charge(region.len())?;
        fees.push(tail.reserve(region.len(), MemoryClass::Ordinary)?);
        let region_owned = region.to_owned();
        if region_owned.capacity() > region.len() {
            return Err(limited().into());
        }
        let mut selected: Vec<ReservedValue<CurrentEmergencyObservationServiceSourcesV1>> =
            Vec::new();
        let mut balances = BTreeMap::new();
        let mut eligible: Vec<ReservedValue<oasis7_world_state::incremental_world_rules_family::CurrentEmergencyMinimumServiceV1>> = Vec::new();
        // Metadata order is the original minimum_services outer-key order.
        // Eligibility uses the actual contract.registration, not its outer
        // contract timestamp and not an emergency-only service registry.
        for metadata in directory.emergency().minimum_services() {
            tail.charge(metadata.region_id.len().min(region.len()))?;
            if metadata.region_id != region {
                continue;
            }
            let guard = ctx.transaction.guard();
            let original = read_current_emergency_minimum_service_v1(
                directory.emergency_source(),
                sources.rules.emergency(),
                sources.delivery.eligibility.roots,
                state,
                &metadata.outer_id,
                ctx,
                &mut work.eligibility.scan,
                &mut work.composite_read,
                &guard,
                live,
            )?;
            let contract = original.value().ok_or_else(unavailable)?;
            let registration = &contract.registration;
            tail.charge(registration.region_id.len().min(region.len()))?;
            if registration.region_id != region
                || registration
                    .registered_at
                    .checked_add(600)
                    .is_none_or(|n| n > window)
            {
                continue;
            }
            if eligible.len() == 4096 {
                return Err(unavailable());
            }
            if eligible.len() == eligible.capacity() {
                let cap = eligible
                    .capacity()
                    .checked_mul(2)
                    .ok_or_else(limited)?
                    .max(4);
                fees.push(tail.reserve(cap.checked_mul(std::mem::size_of::<ReservedValue<oasis7_world_state::incremental_world_rules_family::CurrentEmergencyMinimumServiceV1>>()).ok_or_else(limited)?,MemoryClass::Ordinary)?);
                eligible
                    .try_reserve_exact(cap - eligible.len())
                    .map_err(|_| limited())?;
                if eligible.capacity() > cap {
                    return Err(limited().into());
                }
            }
            eligible.push(original);
        }
        if eligible.is_empty() {
            return Err(unavailable());
        }
        // Preserve the original count/empty rejection before any service's
        // owner/location/policy checks. Every actual selected owner stays live.
        for original in eligible {
            let source = read_current_emergency_observation_service_sources_v1(
                CurrentEmergencyServiceSelectionV1::MinimumRegistration(original),
                directory.emergency_source(),
                sources,
                state,
                ctx,
                parent,
                work,
                extra,
                tail,
                now,
                live,
            )?;
            source.validate(ctx.transaction, tail, now, live)?;
            let service = source.service();
            tail.charge(
                service
                    .beneficiary_id
                    .len()
                    .checked_mul(16)
                    .ok_or_else(limited)?,
            )?;
            if !balances.contains_key(&service.beneficiary_id) {
                fees.push(
                    tail.reserve(
                        service
                            .beneficiary_id
                            .len()
                            .checked_add(2048)
                            .ok_or_else(limited)?,
                        MemoryClass::Ordinary,
                    )?,
                );
                balances.insert(
                    service.beneficiary_id.clone(),
                    source.electricity_max_zero(),
                );
            }
            if selected.len() == selected.capacity() {
                let cap = selected
                    .capacity()
                    .checked_mul(2)
                    .ok_or_else(limited)?
                    .max(4);
                fees.push(
                    tail.reserve(
                        cap.checked_mul(std::mem::size_of::<
                            ReservedValue<CurrentEmergencyObservationServiceSourcesV1>,
                        >())
                        .ok_or_else(limited)?,
                        MemoryClass::Ordinary,
                    )?,
                );
                selected
                    .try_reserve_exact(cap - selected.len())
                    .map_err(|_| limited())?;
                if selected.capacity() > cap {
                    return Err(limited().into());
                }
            }
            selected.push(source);
        }
        if selected.is_empty() {
            return Err(unavailable());
        }
        fees.push(
            tail.reserve(
                selected
                    .len()
                    .checked_mul(std::mem::size_of::<
                        &oasis7_agent_api::world_action::EmergencyServiceRegistrationV1,
                    >())
                    .ok_or_else(limited)?,
                MemoryClass::Ordinary,
            )?,
        );
        let borrowed: Vec<_> = selected.iter().map(|source| source.service()).collect();
        let mut first_work = None;
        let projected =
            oasis7_world_state::world_rules::project_emergency_observation_coverage_with_work_v1(
                &borrowed,
                &mut balances,
                now,
                &mut |charge| {
                    use oasis7_world_state::world_rules::EmergencyObservationWorkV1::*;
                    let (bytes, owned) = match charge {
                        Read { bytes } => (bytes, false),
                        OwnedId { bytes } => (bytes, true),
                        BorrowedSetNode => (0, true),
                    };
                    if let Err(error) = tail.debit(
                        32,
                        u64::try_from(bytes.checked_mul(32).ok_or_else(limited)?)
                            .map_err(|_| limited())?,
                    ) {
                        if first_work.is_none() {
                            first_work = Some(error);
                        }
                        return Err(limited());
                    }
                    if owned {
                        if fees.len() == fees.capacity() {
                            return Err(limited());
                        }
                        fees.push(tail.reserve(
                            bytes.checked_add(2048).ok_or_else(limited)?,
                            MemoryClass::Ordinary,
                        )?);
                    }
                    Ok(())
                },
            );
        drop(borrowed);
        if let Some(error) = first_work {
            return Err(error.into());
        }
        let (normal, intervals) = match projected {
            Ok(value) => value,
            Err(
                oasis7_world_state::world_rules::EmergencyObservationValidationErrorV1::Policy(
                    error,
                ),
            ) => return Err(error.into()),
            Err(
                oasis7_world_state::world_rules::EmergencyObservationValidationErrorV1::Resource(
                    error,
                ),
            ) => return Err(error.into()),
        };
        let guard = ctx.transaction.guard();
        ctx.transaction.validate_guard(&guard, live)?;
        Ok(PreparedCurrentObservationCoverageV2 {
            region: region_owned,
            services: selected,
            normal,
            intervals,
            guard,
            identity: tail.identity(),
            memory: tail.memory().clone(),
            now,
            _fees: fees,
            _fee_slots: fee_slots,
        })
    })()
}

#[path = "incremental_normal_world_rule_observe_publish.rs"]
mod publish;
pub(in crate::world) use publish::{
    CurrentObservationPublishErrorV2, PublishedCurrentObservationV2, publish_current_observation_v2,
};

#[path = "incremental_normal_world_rule_observe_commit.rs"]
mod commit;
pub(in crate::world) use commit::{
    CommittedCurrentObservationV2, RetainedCurrentObservationV2, commit_current_observation_v2,
    retain_current_observation_v2,
};

#[path="incremental_normal_world_rule_observe_execute.rs"]
mod execute;
pub(in crate::world) use execute::execute_lifecycle_observe_v2;
