//! Parent-bound organization tail Current owners.
//!
//! This module is deliberately private to the normal-tick driver.  The data
//! carried here is tied to the active transaction and its accepted parent;
//! it cannot be constructed from an organization ID or a caller DTO.
use crate::world::incremental_normal_sources::{
    BoundNormalTickParentV1,
};
use crate::{WorldEvent, WorldEventBody};
use crate::world::incremental_body::BodyMutationError;
use oasis7_formal_protocol::{
    controlled_authority::formal::bounded_context::{FormalBoundedContextV1, WorkIdentityV1},
    incremental_execution::{ExecutionError, ExecutionFence, ExecutionTransaction, PreparedGuard},
    incremental_state::MerkleNodeReader,
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, MemoryReservation, ReservedValue};
use oasis7_world_state::{
    incremental_world_rule_continuity::{
        CurrentOrganizationContinuityV1, read_current_organization_continuity_v1,
    },
    incremental_world_rules_family::WorldRulesQualificationV1,
};
use std::mem::size_of;
use crate::world::incremental_normal_tail::NormalTailSourcesV1;

fn invalid(reason: &'static str) -> BodyMutationError {
    BodyMutationError::Invalid(reason)
}

/// A read of the OrganizationContinuity branch at this transaction's actual
/// Current root.  The accepted family owner, transaction identity, and fees
/// remain attached until the caller drops the owner.
pub(in crate::world) struct CurrentOrganizationContinuityOwnerV1<'p> {
    parent_roots: [u8; 32],
    guard: PreparedGuard,
    identity: WorkIdentityV1,
    memory: MemoryBudget,
    rules: &'p ReservedValue<WorldRulesQualificationV1>,
    current: ReservedValue<CurrentOrganizationContinuityV1>,
    _owner: MemoryReservation,
}

impl<'p> CurrentOrganizationContinuityOwnerV1<'p> {
    #[allow(clippy::too_many_arguments)]
    pub(in crate::world) fn open<M, S, R, E, C>(
        parent: &BoundNormalTickParentV1<'_>,
        sources: &'p NormalTailSourcesV1<'_, '_>,
        common: &mut crate::world::incremental_normal_tail::NormalTailWorkV1<'_, '_>,
        current_state: &S,
        context: &mut oasis7_world_state::incremental_field_overlay::WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
        scan: &mut oasis7_formal_protocol::incremental_state::StateScanBudget,
        work: &mut oasis7_world_state::incremental_world_rule_composite::WorldRuleCompositeReadWorkV1,
        live: &ExecutionFence,
    ) -> Result<Self, BodyMutationError>
    where
        M: MerkleNodeReader,
        S: MerkleNodeReader,
        R: oasis7_formal_protocol::incremental_value::ValueNodeReader,
        E: FnMut() -> Option<oasis7_formal_protocol::incremental_value::ValueTreeError>,
        C: FnMut() -> bool,
    {
        let result = (|| {
            common
                .debit(1, size_of::<Self>() as u64)
                .map_err(|error| BodyMutationError::Original(Box::new(error)))?;
            if !context.transaction.uses_budget(common.memory())
                || !context.values.memory.same_pool(common.memory())
                || context.values.class != MemoryClass::Ordinary
            {
                return Err(invalid("organization continuity Current pool"));
            }
            let guard = context.transaction.guard();
            sources
                .validate(context.transaction, common.memory(), &guard, live)
                .map_err(|error| BodyMutationError::Original(Box::new(error)))?;
            parent
                .validate_work_binding(common.memory(), common.identity())
                .map_err(|error| BodyMutationError::Original(Box::new(error)))?;
            let rules = sources.native().rules();
            let current = read_current_organization_continuity_v1(
                rules.continuity(),
                sources.roots(),
                current_state,
                context,
                scan,
                work,
                &guard,
                live,
            )
            .map_err(|error| BodyMutationError::Original(Box::new(error)))?;
            current
                .validate(context.transaction, common.memory(), live)
                .map_err(|error| BodyMutationError::Original(Box::new(error)))?;
            let roots = sources.roots().digest().map_err(|error| {
                BodyMutationError::Original(Box::new(error))
            })?;
            let owner = common
                .memory()
                .reserve(size_of::<Self>(), MemoryClass::Ordinary)
                .map_err(oasis7_formal_protocol::incremental_value::ValueTreeError::Memory)?;
            Ok(Self {
                parent_roots: roots,
                guard: context.transaction.guard(),
                identity: common.identity(),
                memory: common.memory().clone(),
                rules,
                current: common.memory().reserve(size_of::<CurrentOrganizationContinuityV1>(), MemoryClass::Ordinary)
                    .map_err(oasis7_formal_protocol::incremental_value::ValueTreeError::Memory)?
                    .retain(current),
                _owner: owner,
            })
        })();
        if result.is_err() {
            let _: Result<(), ExecutionError> = context
                .transaction
                .contribute(|_| Err(ExecutionError::Aborted));
        }
        result
    }

    pub(in crate::world) fn value(&self) -> Option<&oasis7_world_state::world_rules::OrganizationContinuityStateV1> {
        self.current.value()
    }

    pub(in crate::world) fn validate<M: MerkleNodeReader>(
        &self,
        tx: &mut ExecutionTransaction<'_, M>,
        common: &FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        let result = (|| {
            tx.validate_transaction_identity(&self.guard)?;
            tx.validate_guard(&tx.guard(), live)?;
            if tx.parent_fence().parent_roots != self.parent_roots
                || self.identity != common.work().identity()
                || !tx.uses_budget(common.memory())
                || !self.memory.same_pool(common.memory())
                || !self.rules.uses_budget(common.memory())
                || !self.current.uses_budget(common.memory())
            {
                return Err(invalid("organization continuity Current owner binding"));
            }
            Ok(())
        })();
        if result.is_err() {
            let _: Result<(), ExecutionError> = tx.contribute(|_| Err(ExecutionError::Aborted));
        }
        result
    }
}

/// Private publication owner minted only after the corresponding Current
/// write succeeds.  The fixed event sink checks the same transaction, pool,
/// work identity, tick and live fence before recording the original.
pub(in crate::world) struct PreparedNormalOrganizationEventV1 {
    event: ReservedValue<WorldEvent>,
    guard: PreparedGuard,
    identity: WorkIdentityV1,
    now: u64,
    memory: MemoryBudget,
    _phase: ReservedValue<()>,
    header: Option<header::OrganizationSystemHeaderV1>,
    activity_inputs: Option<(
        continuity_facts::CurrentOrganizationFactsV1,
        Option<ReservedValue<oasis7_world_state::incremental_world_event_decode::DecodedWorldEventV1>>,
        ReservedValue<oasis7_world_state::world_rules::OrganizationContinuityStateV1>,
        ReservedValue<CurrentOrganizationContinuityV1>,
        ReservedValue<Option<String>>,
    )>,
}

impl PreparedNormalOrganizationEventV1 {
    pub(in crate::world) fn event(&self) -> &WorldEvent {
        &self.event
    }

    pub(in crate::world) fn validate<M: MerkleNodeReader>(
        &self,
        transaction: &ExecutionTransaction<'_, M>,
        memory: &MemoryBudget,
        now: u64,
        identity: WorkIdentityV1,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        transaction.validate_guard(&self.guard, live)?;
        if !transaction.uses_budget(memory)
            || !self.memory.same_pool(memory)
            || !self.event.uses_budget(memory)
            || self.identity != identity
            || self.now != now
            || self.event.time != now
            || self.event.caused_by.is_some()
            || !matches!(&self.event.body, WorldEventBody::Domain(crate::DomainEvent::WorldRuleCommitted { action_id: 0, .. }))
        {
            return Err(invalid("organization tail event owner binding"));
        }
        Ok(())
    }

    /// Reserve the complete closed event envelope before the caller clones
    /// command strings or builds the event value.
    pub(in crate::world) fn reserve_event_v1(
        common: &mut FormalBoundedContextV1<'_>,
        max_event_bytes: usize,
    ) -> Result<MemoryReservation, BodyMutationError> {
        let bytes = size_of::<WorldEvent>()
            .checked_add(max_event_bytes)
            .ok_or(oasis7_formal_protocol::incremental_value::ValueTreeError::ResourceLimited(
                "organization event owner",
            ))?;
        common
            .charge(
                1,
                u64::try_from(bytes).map_err(|_| {
                    oasis7_formal_protocol::incremental_value::ValueTreeError::ResourceLimited(
                        "organization event owner",
                    )
                })?,
            )
            .map_err(|error| BodyMutationError::Original(Box::new(error)))?;
        common
            .memory()
            .reserve(bytes, MemoryClass::Ordinary)
            .map_err(oasis7_formal_protocol::incremental_value::ValueTreeError::Memory)
            .map_err(Into::into)
    }

    fn after_current_write<M: MerkleNodeReader>(
        event: WorldEvent,
        owner: &CurrentOrganizationContinuityOwnerV1,
        transaction: &mut ExecutionTransaction<'_, M>,
        common: &mut FormalBoundedContextV1<'_>,
        now: u64,
        live: &ExecutionFence,
        phase: MemoryReservation,
    ) -> Result<Self, BodyMutationError> {
        let result = (|| {
            owner.validate(transaction, common, live)?;
            if event.time != now || event.caused_by.is_some() || !matches!(&event.body, WorldEventBody::Domain(crate::DomainEvent::WorldRuleCommitted { action_id: 0, .. })) {
                return Err(invalid("organization tail system event"));
            }
            let phase_bytes = phase.bytes();
            let phase = phase.retain(());
            if !phase.uses_budget(common.memory()) || phase_bytes < size_of::<WorldEvent>() {
                return Err(invalid("organization event owner fee"));
            }
            let event = common.memory().reserve(size_of::<WorldEvent>(), MemoryClass::Ordinary)
                .map_err(oasis7_formal_protocol::incremental_value::ValueTreeError::Memory)?
                .retain(event);
            transaction.validate_guard(&transaction.guard(), live)?;
            Ok(Self { event, guard: transaction.guard(), identity: common.work().identity(), now, memory: common.memory().clone(), _phase: phase, activity_inputs: None, header: None })
        })();
        if result.is_err() {
            let _: Result<(), ExecutionError> = transaction.contribute(|_| Err(ExecutionError::Aborted));
        }
        result
    }
}

#[path = "incremental_normal_organization_activity_journal.rs"]
mod activity_journal;

#[path = "incremental_normal_organization_activity_facts.rs"]
mod activity_facts;

/// The continuously consumed read/write work is supplied by the fixed driver.
pub(in crate::world) struct NormalOrganizationTailWorkV1 {
    pub diplomacy_index: oasis7_world_state::incremental_diplomacy::DiplomacyIndexWorkV1,
    pub cells_remaining: u64,
    pub max_text_bytes: usize,
    pub orders: crate::world::incremental_factory_directory_orders::FactoryOrderDirectoryWorkV1,

    pub scan: oasis7_formal_protocol::incremental_state::StateScanBudget,
    pub read: oasis7_world_state::incremental_world_rule_composite::WorldRuleCompositeReadWorkV1,
    pub write: oasis7_world_state::incremental_world_state_genesis::TypedRecordWriteWorkV1,
    pub eligibility: crate::world::incremental_industrial_eligibility::IndustrialEligibilityWorkV1,
    pub pending_entries_remaining: usize,
    pub journal: crate::world::incremental_normal_industry_due::delivery::ProductValidationDeliveryWorkV1,
}
#[path = "incremental_normal_organization_activity_driver.rs"]
mod activity_driver;

#[path = "incremental_normal_organization_activity_io.rs"]
mod activity_io;

#[path = "incremental_normal_organization_continuity_contract.rs"]
mod continuity_contract;

#[path = "incremental_normal_organization_continuity_facts.rs"]
mod continuity_facts;

#[path = "incremental_normal_organization_continuity_driver.rs"]
mod continuity_driver;

#[path = "incremental_normal_organization_driver.rs"]
mod driver;

#[path = "incremental_normal_organization_header.rs"]
mod header;

#[path = "incremental_normal_organization_admission.rs"]
mod admission;
