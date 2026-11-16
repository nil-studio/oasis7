//! Closed system-event publication input, minted from completed reducers.
//! The borrowed event keeps the reducer's original paid payload alive.
use super::{
    incremental_body::BodyMutationError,
    incremental_normal_tail::factory_built::PreparedNormalFactoryBuiltV1,
};
use oasis7_formal_protocol::{
    controlled_authority::formal::bounded_context::{FormalBoundedContextV1, WorkIdentityV1},
    incremental_execution::{ExecutionFence, ExecutionTransaction, PreparedGuard},
    incremental_state::MerkleNodeReader,
    incremental_value::ValueTreeError,
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
#[derive(Clone, Copy)]
enum NormalSystemScopeV1 {
    Uncaused,
    FactoryModuleFollowup(u64),
    ModuleLifecycle(u64),
    ModuleGovernance,
    BootstrapAuthorization,
}
pub(in crate::world) struct PreparedNormalSystemEventV1<'event> {
    scope: NormalSystemScopeV1,
    event: &'event crate::WorldEvent,
    identity: PreparedGuard,
    core: WorkIdentityV1,
    memory: MemoryBudget,
    _outer: ReservedValue<()>,
}
// Each expansion accepts a concrete private post-write producer. There is no
// trait implementation or raw-event constructor for an external caller to mint.
macro_rules! due_event_source {
    ($method:ident, $source:ident) => {
        pub(in crate::world) fn $method<M: MerkleNodeReader>(
            source: &'event super::incremental_normal_industry_due::$source,
            tx: &ExecutionTransaction<'_, M>,
            now: u64,
            common: &mut FormalBoundedContextV1<'_>,
            live: &ExecutionFence,
        ) -> Result<Self, BodyMutationError> {
            common
                .consume_steps(1)
                .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
            source.validate(tx, common.memory(), now, common.work().identity(), live)?;
            let fee = common
                .memory()
                .reserve(std::mem::size_of::<Self>(), MemoryClass::Ordinary)
                .map_err(ValueTreeError::Memory)?;
            Ok(Self {
                scope: NormalSystemScopeV1::Uncaused,
                event: source.event(),
                identity: tx.guard(),
                core: common.work().identity(),
                memory: common.memory().clone(),
                _outer: fee.retain(()),
            })
        }
    };
}
impl<'event> PreparedNormalSystemEventV1<'event> {
    pub(in crate::world) fn from_bootstrap_authorization<M: MerkleNodeReader>(
        source: &'event super::normal_bootstrap_current::CommittedCurrentBootstrapAuthorizationV1,
        tx: &ExecutionTransaction<'_, M>,
        now: u64,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<Self, BodyMutationError> {
        source.validate_evidence(tx, common, now, live)?;
        common
            .consume_steps(1)
            .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
        let fee = common
            .memory()
            .reserve(std::mem::size_of::<Self>(), MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        Ok(Self {
            scope: NormalSystemScopeV1::BootstrapAuthorization,
            event: source.event(),
            identity: tx.guard(),
            core: common.work().identity(),
            memory: common.memory().clone(),
            _outer: fee.retain(()),
        })
    }

    pub(in crate::world) fn from_module_apply_event<M: MerkleNodeReader>(
        source: &'event super::incremental_normal_action_module_lifecycle::governance_apply::PreparedAppliedModuleEventV1<'_>,
        tx: &ExecutionTransaction<'_, M>,
        now: u64,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<Self, BodyMutationError> {
        common
            .consume_steps(1)
            .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
        source.validate(tx, common.memory(), common.work().identity(), now, live)?;
        let fee = common
            .memory()
            .reserve(std::mem::size_of::<Self>(), MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        Ok(Self {
            scope: NormalSystemScopeV1::ModuleGovernance,
            event: source.event(),
            identity: tx.guard(),
            core: common.work().identity(),
            memory: common.memory().clone(),
            _outer: fee.retain(()),
        })
    }

    pub(in crate::world) fn from_release_profile<M: MerkleNodeReader>(
        source: &'event super::incremental_normal_action_module_lifecycle::release_completion::PreparedCurrentReleaseProfileEventV1<'_, '_>,
        tx: &ExecutionTransaction<'_, M>,
        now: u64,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<Self, BodyMutationError> {
        common
            .consume_steps(1)
            .map_err(|error| BodyMutationError::Original(Box::new(error)))?;
        source.validate(tx, common.memory(), common.work().identity(), now, live)?;
        let fee = common
            .memory()
            .reserve(std::mem::size_of::<Self>(), MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        Ok(Self {
            scope: NormalSystemScopeV1::ModuleLifecycle(source.action_id()),
            event: source.event(),
            identity: tx.guard(),
            core: common.work().identity(),
            memory: common.memory().clone(),
            _outer: fee.retain(()),
        })
    }

    pub(in crate::world) fn from_module_lifecycle<M: MerkleNodeReader>(
        source: &'event super::incremental_normal_action_module_lifecycle::PreparedNormalModuleLifecycleEventV1,
        tx: &ExecutionTransaction<'_, M>,
        now: u64,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<Self, BodyMutationError> {
        common
            .consume_steps(1)
            .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
        tx.validate_guard(&tx.guard(), live)?;
        source.validate(tx, common.memory(), now, common.work().identity(), live)?;
        let scope = match &source.event().body {
            crate::WorldEventBody::Governance(_) if source.event().caused_by.is_none() => {
                NormalSystemScopeV1::ModuleGovernance
            }
            crate::WorldEventBody::Domain(_)
                if source.event().caused_by
                    == Some(crate::CausedBy::Action(source.action_id())) =>
            {
                NormalSystemScopeV1::ModuleLifecycle(source.action_id())
            }
            _ => return Err(BodyMutationError::Invalid("module lifecycle action source")),
        };
        let fee = common
            .memory()
            .reserve(std::mem::size_of::<Self>(), MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        Ok(Self {
            scope,
            event: source.event(),
            identity: tx.guard(),
            core: common.work().identity(),
            memory: common.memory().clone(),
            _outer: fee.retain(()),
        })
    }

    pub(in crate::world) fn from_factory_module_followup<M: MerkleNodeReader>(
        source: &'event super::incremental_module_invocation::PreparedFactoryModuleFollowupV1,
        tx: &ExecutionTransaction<'_, M>,
        now: u64,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<Self, BodyMutationError> {
        common
            .consume_steps(1)
            .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
        source.validate(tx, common.memory(), now, common.work().identity(), live)?;
        let action = match (&source.event().body, source.event().caused_by.as_ref()) {
            (
                crate::WorldEventBody::Domain(crate::DomainEvent::FactoryProductionBlocked {
                    action_id,
                    ..
                }),
                Some(crate::CausedBy::Action(id)),
            ) if action_id == id => *id,
            _ => return Err(BodyMutationError::Invalid("factory followup action source")),
        };
        let fee = common
            .memory()
            .reserve(std::mem::size_of::<Self>(), MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        Ok(Self {
            scope: NormalSystemScopeV1::FactoryModuleFollowup(action),
            event: source.event(),
            identity: tx.guard(),
            core: common.work().identity(),
            memory: common.memory().clone(),
            _outer: fee.retain(()),
        })
    }

    /// Only the fixed lifecycle reducer may provide this post-write source.
    pub(in crate::world) fn from_world_rule_lifecycle<M: MerkleNodeReader>(
        source: &'event super::incremental_normal_world_rule_lifecycle::PreparedNormalWorldRuleLifecycleEventV1,
        tx: &ExecutionTransaction<'_, M>,
        now: u64,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<Self, BodyMutationError> {
        common
            .consume_steps(1)
            .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
        source.validate(tx, common.memory(), now, common.work().identity(), live)?;
        if source.event().caused_by.is_some()
            || !matches!(
                &source.event().body,
                crate::WorldEventBody::Domain(crate::DomainEvent::WorldRuleCommitted {
                    action_id: 0,
                    ..
                })
            )
        {
            return Err(BodyMutationError::Invalid(
                "normal world-rule lifecycle source",
            ));
        }
        let fee = common
            .memory()
            .reserve(std::mem::size_of::<Self>(), MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        Ok(Self {
            scope: NormalSystemScopeV1::Uncaused,
            event: source.event(),
            identity: tx.guard(),
            core: common.work().identity(),
            memory: common.memory().clone(),
            _outer: fee.retain(()),
        })
    }
    /// Only the fixed lifecycle reducer may provide this post-write source.
    pub(in crate::world) fn from_claim_epoch<M: MerkleNodeReader>(
        source: &'event super::incremental_normal_claim_epochs::PreparedNormalClaimEpochEventV1,
        tx: &ExecutionTransaction<'_, M>,
        now: u64,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<Self, BodyMutationError> {
        common
            .consume_steps(1)
            .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
        source.validate(tx, common.memory(), now, common.work().identity(), live)?;
        if source.event().caused_by.is_some()
            || !matches!(&source.event().body, crate::WorldEventBody::Domain(_))
        {
            return Err(BodyMutationError::Invalid(
                "normal world-rule lifecycle source",
            ));
        }
        let fee = common
            .memory()
            .reserve(std::mem::size_of::<Self>(), MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        Ok(Self {
            scope: NormalSystemScopeV1::Uncaused,
            event: source.event(),
            identity: tx.guard(),
            core: common.work().identity(),
            memory: common.memory().clone(),
            _outer: fee.retain(()),
        })
    }
    /// Only the fixed lifecycle reducer may provide this post-write source.
    pub(in crate::world) fn from_organization_tail<M: MerkleNodeReader>(
        source: &'event super::incremental_normal_organization_tail::PreparedNormalOrganizationEventV1,
        tx: &ExecutionTransaction<'_, M>,
        now: u64,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<Self, BodyMutationError> {
        common
            .consume_steps(1)
            .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
        source.validate(tx, common.memory(), now, common.work().identity(), live)?;
        if source.event().caused_by.is_some()
            || !matches!(
                &source.event().body,
                crate::WorldEventBody::Domain(crate::DomainEvent::WorldRuleCommitted {
                    action_id: 0,
                    ..
                })
            )
        {
            return Err(BodyMutationError::Invalid(
                "normal organization lifecycle source",
            ));
        }
        let fee = common
            .memory()
            .reserve(std::mem::size_of::<Self>(), MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        Ok(Self {
            scope: NormalSystemScopeV1::Uncaused,
            event: source.event(),
            identity: tx.guard(),
            core: common.work().identity(),
            memory: common.memory().clone(),
            _outer: fee.retain(()),
        })
    }
    /// Only the fixed lifecycle reducer may provide this post-write source.
    pub(in crate::world) fn from_gameplay_cycle<M: MerkleNodeReader>(
        source: &'event super::incremental_normal_gameplay_cycles::PreparedGameplayCycleEventV1,
        tx: &ExecutionTransaction<'_, M>,
        now: u64,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<Self, BodyMutationError> {
        common
            .consume_steps(1)
            .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
        source.validate(tx, common.memory(), now, common.work().identity(), live)?;
        if source.event().caused_by.is_some()
            || !matches!(&source.event().body, crate::WorldEventBody::Domain(_))
        {
            return Err(BodyMutationError::Invalid(
                "normal world-rule lifecycle source",
            ));
        }
        let fee = common
            .memory()
            .reserve(std::mem::size_of::<Self>(), MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        Ok(Self {
            scope: NormalSystemScopeV1::Uncaused,
            event: source.event(),
            identity: tx.guard(),
            core: common.work().identity(),
            memory: common.memory().clone(),
            _outer: fee.retain(()),
        })
    }
    pub(in crate::world) fn from_gameplay_directive_failure<M: MerkleNodeReader>(
        source: &'event super::incremental_normal_gameplay_cycles::PreparedGameplayDirectiveFailureV1,
        tx: &ExecutionTransaction<'_, M>,
        now: u64,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<Self, BodyMutationError> {
        common
            .consume_steps(1)
            .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
        source.validate(tx, common.memory(), now, common.work().identity(), live)?;
        if source.event().caused_by.is_some()
            || !matches!(&source.event().body, crate::WorldEventBody::ModuleCallFailed(f) if f.code==oasis7_wasm_abi::ModuleCallErrorCode::InvalidOutput)
        {
            return Err(BodyMutationError::Invalid(
                "normal gameplay directive failure source",
            ));
        }
        let fee = common
            .memory()
            .reserve(std::mem::size_of::<Self>(), MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        Ok(Self {
            scope: NormalSystemScopeV1::Uncaused,
            event: source.event(),
            identity: tx.guard(),
            core: common.work().identity(),
            memory: common.memory().clone(),
            _outer: fee.retain(()),
        })
    }
    due_event_source!(from_product_attempt, PreparedNormalProductAttemptV1);
    due_event_source!(from_product_recorded, PreparedNormalProductRecordedV1);
    due_event_source!(from_product_result, PreparedNormalProductResultV1);
    due_event_source!(from_recipe_failure, PreparedNormalRecipeFailureV1);
    due_event_source!(
        from_product_module_failure,
        PreparedNormalProductModuleFailureV1
    );
    due_event_source!(from_product_cursor, PreparedNormalProductCursorV1);
    pub(in crate::world) fn from_factory_built<M: MerkleNodeReader>(
        source: &'event PreparedNormalFactoryBuiltV1,
        tx: &ExecutionTransaction<'_, M>,
        now: u64,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<Self, BodyMutationError> {
        common
            .consume_steps(1)
            .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
        source.validate(tx, common.memory(), now, common.work().identity(), live)?;
        if source.event().caused_by.is_some()
            || !matches!(
                source.event().body,
                crate::WorldEventBody::Domain(crate::DomainEvent::FactoryBuilt { .. })
            )
        {
            return Err(BodyMutationError::Invalid(
                "normal FactoryBuilt event source",
            ));
        }
        let fee = common
            .memory()
            .reserve(std::mem::size_of::<Self>(), MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        Ok(Self {
            scope: NormalSystemScopeV1::Uncaused,
            event: source.event(),
            identity: tx.guard(),
            core: common.work().identity(),
            memory: common.memory().clone(),
            _outer: fee.retain(()),
        })
    }
    pub(in crate::world) fn from_recipe_completed<M: MerkleNodeReader>(
        source: &'event super::incremental_normal_industry_due::PreparedNormalRecipeCompletedV1,
        tx: &ExecutionTransaction<'_, M>,
        now: u64,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<Self, BodyMutationError> {
        common
            .consume_steps(1)
            .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
        source.validate(tx, common.memory(), now, common.work().identity(), live)?;
        if source.event().caused_by.is_some()
            || !matches!(
                source.event().body,
                crate::WorldEventBody::Domain(crate::DomainEvent::RecipeCompleted { .. })
            )
        {
            return Err(BodyMutationError::Invalid(
                "normal RecipeCompleted event source",
            ));
        }
        let fee = common
            .memory()
            .reserve(std::mem::size_of::<Self>(), MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        Ok(Self {
            scope: NormalSystemScopeV1::Uncaused,
            event: source.event(),
            identity: tx.guard(),
            core: common.work().identity(),
            memory: common.memory().clone(),
            _outer: fee.retain(()),
        })
    }
    pub(in crate::world) fn from_material_transit<M: MerkleNodeReader>(
        source: &'event super::incremental_normal_tail::transits::PreparedNormalMaterialTransitV1,
        tx: &ExecutionTransaction<'_, M>,
        now: u64,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<Self, BodyMutationError> {
        common
            .consume_steps(1)
            .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
        source.validate(tx, common.memory(), now, common.work().identity(), live)?;
        if source.event().caused_by.is_some()
            || !matches!(
                source.event().body,
                crate::WorldEventBody::Domain(crate::DomainEvent::MaterialTransitCompleted { .. })
            )
        {
            return Err(BodyMutationError::Invalid(
                "normal MaterialTransit event source",
            ));
        }
        let fee = common
            .memory()
            .reserve(std::mem::size_of::<Self>(), MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        Ok(Self {
            scope: NormalSystemScopeV1::Uncaused,
            event: source.event(),
            identity: tx.guard(),
            core: common.work().identity(),
            memory: common.memory().clone(),
            _outer: fee.retain(()),
        })
    }
    pub(in crate::world) fn event(&self) -> &'event crate::WorldEvent {
        self.event
    }
    pub(in crate::world) fn validate_common(
        &self,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<(), BodyMutationError> {
        common
            .consume_steps(1)
            .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
        if self.core != common.work().identity() || !self.memory.same_pool(common.memory()) {
            return Err(BodyMutationError::Invalid(
                "normal system event Common source",
            ));
        }
        Ok(())
    }
    pub(in crate::world) fn validate<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        memory: &MemoryBudget,
        now: u64,
    ) -> Result<(), BodyMutationError> {
        tx.validate_transaction_identity(&self.identity)?;
        if !tx.uses_budget(memory)
            || !self.memory.same_pool(memory)
            || !self._outer.uses_budget(memory)
            || self.event.time != now
            || !match self.scope {
                NormalSystemScopeV1::Uncaused => self.event.caused_by.is_none(),
                NormalSystemScopeV1::ModuleLifecycle(id) => {
                    self.event.caused_by == Some(crate::CausedBy::Action(id))
                        && matches!(
                            &self.event.body,
                            crate::WorldEventBody::Domain(
                                crate::DomainEvent::ProductProfileGoverned { .. }
                                    | crate::DomainEvent::RecipeProfileGoverned { .. }
                                    | crate::DomainEvent::FactoryProfileGoverned { .. }
                                    | crate::DomainEvent::ModuleReleaseRequested { .. }
                                    | crate::DomainEvent::ModuleReleaseShadowed { .. }
                                    | crate::DomainEvent::ModuleReleaseAttested { .. }
                                    | crate::DomainEvent::ModuleReleaseRoleApproved { .. }
                                    | crate::DomainEvent::ModuleReleaseRolesBound { .. }
                                    | crate::DomainEvent::ModuleReleaseRejected { .. }
                                    | crate::DomainEvent::ModuleReleaseApplied { .. }
                                    | crate::DomainEvent::ActionRejected { .. }
                                    | crate::DomainEvent::ModuleRollbackApplied { .. }
                                    | crate::DomainEvent::ModuleUpgraded { .. }
                                    | crate::DomainEvent::ModuleInstalled { .. }
                                    | crate::DomainEvent::ModuleArtifactDeployed { .. }
                                    | crate::DomainEvent::ModuleArtifactListed { .. }
                                    | crate::DomainEvent::ModuleArtifactBidPlaced { .. }
                                    | crate::DomainEvent::ModuleArtifactBidCancelled { .. }
                                    | crate::DomainEvent::ModuleArtifactSaleCompleted { .. }
                                    | crate::DomainEvent::ModuleArtifactDelisted { .. }
                                    | crate::DomainEvent::ModuleArtifactDestroyed { .. }
                            )
                        )
                }
                NormalSystemScopeV1::BootstrapAuthorization => self.event.caused_by.is_none()
                    && matches!(
                        &self.event.body,
                        crate::WorldEventBody::CapabilityAuthorization(
                            crate::CapabilityAuthorizationEvent::AgentIdentityInstalled { .. }
                                | crate::CapabilityAuthorizationEvent::AuthorityInstalledWithProof { .. }
                                | crate::CapabilityAuthorizationEvent::GrantRegistered { .. }
                                | crate::CapabilityAuthorizationEvent::InvocationContextInstalled { .. }
                        )
                    ),
                NormalSystemScopeV1::ModuleGovernance => {
                    self.event.caused_by.is_none()
                        && matches!(
                            &self.event.body,
                            crate::WorldEventBody::Governance(
                                crate::GovernanceEvent::Proposed { .. }
                                    | crate::GovernanceEvent::ShadowReport { .. }
                                    | crate::GovernanceEvent::Approved { .. }
                                    | crate::GovernanceEvent::Queued { .. }
                                    | crate::GovernanceEvent::Applied { .. }
                            ) | crate::WorldEventBody::ModuleEvent(_)
                                | crate::WorldEventBody::ManifestUpdated(_)
                        )
                }
                NormalSystemScopeV1::FactoryModuleFollowup(id) => {
                    self.event.caused_by == Some(crate::CausedBy::Action(id))
                        && matches!(&self.event.body,crate::WorldEventBody::Domain(crate::DomainEvent::FactoryProductionBlocked{action_id,..}) if *action_id==id)
                }
            }
        {
            return Err(BodyMutationError::Invalid(
                "normal system event transaction",
            ));
        }
        Ok(())
    }
}
