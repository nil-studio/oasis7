//! Fixed work bank and family classification for same-transaction Rule Modify.
//!
//! The bank owns no authority. It only groups the existing bounded work values
//! and already-qualified family sources so the genuine effective action can
//! choose its existing after-rules owner without reconstructing Selected.
use super::NonmovementSelectedWorkV1;
use super::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::world) enum EffectiveActionFamilyV1 {
    Core,
    NormalExtension,
    LogisticsRegistration,
    Alliance,
    Crisis,
    ContractAccept,
    FactorySchedule,
    ContractLifecycle,
    MetaProgress,
    FactoryLifecycle,
    RouteAvailability,
    MaterialTransfer,
    Body,
    DataAccess,
    CollectData,
    AuthenticatedCollectData,
    TransferResource,
}

impl EffectiveActionFamilyV1 {
    pub(in crate::world) fn for_action(action: &crate::Action) -> Option<Self> {
        Some(match action {
            crate::Action::RegisterAgent { .. } | crate::Action::MoveAgent { .. } => Self::Core,
            crate::Action::EmitObservation { .. } | crate::Action::ExpandBodyInterface { .. } => {
                Self::NormalExtension
            }
            crate::Action::RegisterLogisticsRoute { .. } => Self::LogisticsRegistration,
            crate::Action::FormAlliance { .. }
            | crate::Action::JoinAlliance { .. }
            | crate::Action::LeaveAlliance { .. }
            | crate::Action::DissolveAlliance { .. }
            | crate::Action::DeclareWar { .. } => Self::Alliance,
            crate::Action::ResolveCrisis { .. } => Self::Crisis,
            crate::Action::AcceptEconomicContract { .. } => Self::ContractAccept,
            crate::Action::BuildFactory { .. } | crate::Action::ScheduleRecipe { .. } => {
                Self::FactorySchedule
            }
            crate::Action::OpenEconomicContract { .. }
            | crate::Action::SettleEconomicContract { .. } => Self::ContractLifecycle,
            crate::Action::GrantMetaProgress { .. } => Self::MetaProgress,
            crate::Action::MaintainFactory { .. } | crate::Action::RecycleFactory { .. } => {
                Self::FactoryLifecycle
            }
            crate::Action::SetLogisticsRouteAvailability { .. } => Self::RouteAvailability,
            crate::Action::TransferMaterial { .. } => Self::MaterialTransfer,
            crate::Action::EmitBodyAttributes { .. }
            | crate::Action::QueryObservation { .. }
            | crate::Action::BodyAction { .. } => Self::Body,
            crate::Action::GrantDataAccess { .. } | crate::Action::RevokeDataAccess { .. } => {
                Self::DataAccess
            }
            crate::Action::CollectData { .. } => Self::CollectData,
            crate::Action::CollectDataAuthenticated { .. } => Self::AuthenticatedCollectData,
            crate::Action::TransferResource { .. } | crate::Action::EmitResourceTransfer { .. } => {
                Self::TransferResource
            }
            _ => return None,
        })
    }
}

/// The 12-slot input is a bounded bundle of existing per-family work and
/// genuine Current qualifications. It is consumed by the selected Rule lane;
/// it never acts as an admission or completion receipt.
pub(in crate::world) struct EffectiveActionWorkBankV1<'a> {
    factory_schedule:Option<&'a mut crate::world::incremental_normal_action_factory_schedule::FactoryScheduleWorkV1>,
    schedule_sources:Option<&'a crate::world::incremental_normal_action_factory_schedule::FactoryScheduleSourcesV1<'a>>,
    contract_lifecycle:Option<&'a mut crate::world::incremental_normal_action_diplomacy::contracts::ContractWorkV1>,
    lifecycle_sources:Option<&'a crate::world::incremental_normal_action_diplomacy::contracts::ContractSourcesV1<'a,'a>>,
    alliance:Option<&'a mut crate::world::incremental_normal_action_diplomacy::AllianceWorkV1>,
    alliance_sources:Option<&'a crate::world::incremental_normal_action_diplomacy::AllianceSourcesV1<'a,'a>>,
    logistics_registration:Option<&'a mut crate::world::incremental_normal_action_logistics::LogisticsRegistrationWorkV1>,
    normal_extension: Option<&'a mut super::normal_extension_driver::NormalCoreExtensionWorkV1<'a>>,
    pub(in crate::world) crisis: &'a mut crate::world::incremental_action_crisis::CrisisWorkV1,
    pub(in crate::world) crisis_sources: &'a crate::world::incremental_action_crisis::CrisisSourcesV1<'a>,
    pub(in crate::world) contract_accept: &'a mut crate::world::incremental_action_contract_accept::ContractAcceptWorkV1,
    pub(in crate::world) contract_sources: &'a crate::world::incremental_action_contract_accept::ContractAcceptSourcesV1<'a>,
    pub(in crate::world) meta_progress: &'a mut crate::world::incremental_action_meta_progress::MetaProgressWorkV1,
    pub(in crate::world) meta_sources: &'a crate::world::incremental_action_meta_progress::MetaProgressSourcesV1<'a>,
    pub(in crate::world) factory_lifecycle: &'a mut crate::world::incremental_action_factory_lifecycle::FactoryLifecycleWorkV1,
    pub(in crate::world) factory_sources: &'a crate::world::incremental_action_factory_lifecycle::FactoryLifecycleSourcesV1<'a>,
    pub(in crate::world) route_availability: &'a mut crate::world::incremental_action_route_availability::RouteAvailabilityWorkV1,
    pub(in crate::world) logistics: &'a ReservedValue<oasis7_world_state::incremental_logistics_records::LogisticsFamilyQualificationV1>,
    pub(in crate::world) material_transfer: &'a mut crate::world::incremental_material_normalization::transfer::MaterialTransferWorkV1,
    pub(in crate::world) materials: &'a ReservedValue<oasis7_world_state::incremental_material_records::MaterialFamilyQualificationV1>,
    pub(in crate::world) body: &'a mut crate::world::incremental_action_body::BodyActionWorkV1,
    pub(in crate::world) data_access: &'a mut crate::world::incremental_action_data::data_access::DataAccessWorkV1,
    pub(in crate::world) controls: &'a ReservedValue<oasis7_world_state::incremental_world_collection_controls::WorldCollectionControlsQualificationV1>,
    pub(in crate::world) collect_data: &'a mut crate::world::incremental_action_data::CollectDataWorkV1,
    pub(in crate::world) authenticated_data: &'a mut crate::world::incremental_action_data::CollectDataWorkV1,
    pub(in crate::world) authenticated_work: &'a mut crate::world::incremental_action_data::auth_current::CollectDataAuthorizationWorkV1,
    pub(in crate::world) starter_bindings: &'a ReservedValue<oasis7_world_state::incremental_world_native::StarterBindingIndexV1>,
    pub(in crate::world) transfer_resource: &'a mut crate::world::incremental_action_data::transfer_resource::TransferResourceWorkV1,
}

impl<'a> EffectiveActionWorkBankV1<'a> {
    #[allow(clippy::too_many_arguments)]
    pub(in crate::world) fn new(
        crisis: &'a mut crate::world::incremental_action_crisis::CrisisWorkV1,
        crisis_sources: &'a crate::world::incremental_action_crisis::CrisisSourcesV1<'a>,
        contract_accept: &'a mut crate::world::incremental_action_contract_accept::ContractAcceptWorkV1,
        contract_sources: &'a crate::world::incremental_action_contract_accept::ContractAcceptSourcesV1<'a>,
        meta_progress: &'a mut crate::world::incremental_action_meta_progress::MetaProgressWorkV1,
        meta_sources: &'a crate::world::incremental_action_meta_progress::MetaProgressSourcesV1<'a>,
        factory_lifecycle: &'a mut crate::world::incremental_action_factory_lifecycle::FactoryLifecycleWorkV1,
        factory_sources: &'a crate::world::incremental_action_factory_lifecycle::FactoryLifecycleSourcesV1<'a>,
        route_availability: &'a mut crate::world::incremental_action_route_availability::RouteAvailabilityWorkV1,
        logistics: &'a ReservedValue<
            oasis7_world_state::incremental_logistics_records::LogisticsFamilyQualificationV1,
        >,
        material_transfer: &'a mut crate::world::incremental_material_normalization::transfer::MaterialTransferWorkV1,
        materials: &'a ReservedValue<
            oasis7_world_state::incremental_material_records::MaterialFamilyQualificationV1,
        >,
        body: &'a mut crate::world::incremental_action_body::BodyActionWorkV1,
        data_access: &'a mut crate::world::incremental_action_data::data_access::DataAccessWorkV1,
        controls: &'a ReservedValue<oasis7_world_state::incremental_world_collection_controls::WorldCollectionControlsQualificationV1>,
        collect_data: &'a mut crate::world::incremental_action_data::CollectDataWorkV1,
        authenticated_data: &'a mut crate::world::incremental_action_data::CollectDataWorkV1,
        authenticated_work: &'a mut crate::world::incremental_action_data::auth_current::CollectDataAuthorizationWorkV1,
        starter_bindings: &'a ReservedValue<
            oasis7_world_state::incremental_world_native::StarterBindingIndexV1,
        >,
        transfer_resource: &'a mut crate::world::incremental_action_data::transfer_resource::TransferResourceWorkV1,
    ) -> Self {
        Self {
            factory_schedule: None,
            schedule_sources: None,
            contract_lifecycle: None,
            lifecycle_sources: None,
            alliance: None,
            alliance_sources: None,
            normal_extension: None,
            logistics_registration: None,
            crisis,
            crisis_sources,
            contract_accept,
            contract_sources,
            meta_progress,
            meta_sources,
            factory_lifecycle,
            factory_sources,
            route_availability,
            logistics,
            material_transfer,
            materials,
            body,
            data_access,
            controls,
            collect_data,
            authenticated_data,
            authenticated_work,
            starter_bindings,
            transfer_resource,
        }
    }

    pub(in crate::world) fn with_factory_schedule(
        mut self,
        work:&'a mut crate::world::incremental_normal_action_factory_schedule::FactoryScheduleWorkV1,
        sources:&'a crate::world::incremental_normal_action_factory_schedule::FactoryScheduleSourcesV1<'a>,
    ) -> Self {
        self.factory_schedule = Some(work);
        self.schedule_sources = Some(sources);
        self
    }

    pub(in crate::world) fn with_contract_lifecycle(
        mut self,
        work: &'a mut crate::world::incremental_normal_action_diplomacy::contracts::ContractWorkV1,
        sources:&'a crate::world::incremental_normal_action_diplomacy::contracts::ContractSourcesV1<'a,'a>,
    ) -> Self {
        self.contract_lifecycle = Some(work);
        self.lifecycle_sources = Some(sources);
        self
    }

    pub(in crate::world) fn with_alliance(
        mut self,
        work: &'a mut crate::world::incremental_normal_action_diplomacy::AllianceWorkV1,
        sources: &'a crate::world::incremental_normal_action_diplomacy::AllianceSourcesV1<'a, 'a>,
    ) -> Self {
        self.alliance = Some(work);
        self.alliance_sources = Some(sources);
        self
    }

    pub(in crate::world) fn with_normal_extension(
        mut self,
        work: &'a mut super::normal_extension_driver::NormalCoreExtensionWorkV1<'a>,
    ) -> Self {
        self.normal_extension = Some(work);
        self
    }

    pub(in crate::world) fn with_logistics_registration(
        mut self,
        work:&'a mut crate::world::incremental_normal_action_logistics::LogisticsRegistrationWorkV1,
    ) -> Self {
        self.logistics_registration = Some(work);
        self
    }
    pub(in crate::world) fn select(
        &mut self,
        action: &crate::Action,
    ) -> Option<NonmovementSelectedWorkV1<'_>> {
        use super::NonmovementSelectedWorkV1 as Work;
        Some(match EffectiveActionFamilyV1::for_action(action)? {
            EffectiveActionFamilyV1::Core => Work::Core,
            EffectiveActionFamilyV1::FactorySchedule => Work::FactorySchedule(
                self.factory_schedule.as_deref_mut()?,
                self.schedule_sources?,
                self.normal_extension.as_deref()?.common,
            ),
            EffectiveActionFamilyV1::ContractLifecycle => Work::ContractLifecycle(
                self.contract_lifecycle.as_deref_mut()?,
                self.lifecycle_sources?,
                self.normal_extension.as_deref()?.common,
            ),
            EffectiveActionFamilyV1::Alliance => Work::Alliance(
                self.alliance.as_deref_mut()?,
                self.alliance_sources?,
                self.normal_extension.as_deref()?.common,
            ),
            EffectiveActionFamilyV1::NormalExtension => {
                Work::NormalExtension(self.normal_extension.as_deref_mut()?.view())
            }
            EffectiveActionFamilyV1::LogisticsRegistration => Work::LogisticsRegistration(
                self.logistics_registration.as_deref_mut()?,
                self.logistics,
                self.normal_extension.as_deref()?.common,
            ),
            EffectiveActionFamilyV1::Crisis => Work::Crisis(
                &mut *self.crisis,
                crate::world::incremental_action_crisis::CrisisSourcesV1 {
                    agents: self.crisis_sources.agents,
                    diplomacy: self.crisis_sources.diplomacy,
                },
            ),
            EffectiveActionFamilyV1::ContractAccept => Work::ContractAccept(
                &mut *self.contract_accept,
                crate::world::incremental_action_contract_accept::ContractAcceptSourcesV1 {
                    agents: self.contract_sources.agents,
                    diplomacy: self.contract_sources.diplomacy,
                },
            ),
            EffectiveActionFamilyV1::MetaProgress => Work::MetaProgress(
                &mut *self.meta_progress,
                crate::world::incremental_action_meta_progress::MetaProgressSourcesV1 {
                    agents: self.meta_sources.agents,
                    native: self.meta_sources.native,
                },
            ),
            EffectiveActionFamilyV1::FactoryLifecycle => Work::FactoryLifecycle(
                &mut *self.factory_lifecycle,
                crate::world::incremental_action_factory_lifecycle::FactoryLifecycleSourcesV1 {
                    agents: self.factory_sources.agents,
                    factories: self.factory_sources.factories,
                    industry: self.factory_sources.industry,
                    index: self.factory_sources.index,
                    native: self.factory_sources.native,
                    primitive: self.factory_sources.primitive,
                    inventory: self.factory_sources.inventory,
                },
            ),
            EffectiveActionFamilyV1::RouteAvailability => {
                Work::RouteAvailability(&mut *self.route_availability, self.logistics)
            }
            EffectiveActionFamilyV1::MaterialTransfer => {
                Work::MaterialTransfer(&mut *self.material_transfer, self.materials)
            }
            EffectiveActionFamilyV1::Body => Work::Body(&mut *self.body),
            EffectiveActionFamilyV1::DataAccess => {
                Work::DataAccess(&mut *self.data_access, self.controls)
            }
            EffectiveActionFamilyV1::CollectData => Work::CollectData(&mut *self.collect_data),
            EffectiveActionFamilyV1::AuthenticatedCollectData => Work::AuthenticatedCollectData(
                &mut *self.authenticated_data,
                &mut *self.authenticated_work,
                self.starter_bindings,
                self.controls,
            ),
            EffectiveActionFamilyV1::TransferResource => {
                Work::TransferResource(&mut *self.transfer_resource)
            }
        })
    }

    /// Select the effective owner and, for a cross-family selective action,
    /// borrow the original family's actual Accepted-read quota at the same
    /// time. Same-family owners use their existing work counter directly.
    pub(in crate::world) fn select_for_original(
        &mut self,
        original: &crate::Action,
        effective: &crate::Action,
    ) -> Option<(NonmovementSelectedWorkV1<'_>, Option<&mut usize>)> {
        use super::NonmovementSelectedWorkV1 as Work;
        let target = EffectiveActionFamilyV1::for_action(effective)?;
        let source = if crate::world::World::should_emit_action_accepted(original) {
            EffectiveActionFamilyV1::for_action(original)
        } else {
            None
        };
        match target {
            EffectiveActionFamilyV1::FactorySchedule => {
                let accepted = match source {
                    Some(EffectiveActionFamilyV1::ContractLifecycle) => Some(
                        &mut self
                            .contract_lifecycle
                            .as_deref_mut()?
                            .shared
                            .pending_entries_remaining,
                    ),
                    Some(EffectiveActionFamilyV1::Alliance) => {
                        Some(&mut self.alliance.as_deref_mut()?.pending_entries_remaining)
                    }
                    Some(EffectiveActionFamilyV1::Crisis) => {
                        Some(&mut self.crisis.pending_entries_remaining)
                    }
                    Some(EffectiveActionFamilyV1::MetaProgress) => {
                        Some(&mut self.meta_progress.pending_entries_remaining)
                    }
                    Some(EffectiveActionFamilyV1::ContractAccept) => {
                        Some(&mut self.contract_accept.pending_entries_remaining)
                    }
                    _ => None,
                };
                Some((
                    Work::FactorySchedule(
                        self.factory_schedule.as_deref_mut()?,
                        self.schedule_sources?,
                        self.normal_extension.as_deref()?.common,
                    ),
                    accepted,
                ))
            }
            EffectiveActionFamilyV1::ContractLifecycle => {
                let accepted = match source {
                    Some(EffectiveActionFamilyV1::FactorySchedule) => Some(
                        &mut self
                            .factory_schedule
                            .as_deref_mut()?
                            .pending_entries_remaining,
                    ),
                    Some(EffectiveActionFamilyV1::Alliance) => {
                        Some(&mut self.alliance.as_deref_mut()?.pending_entries_remaining)
                    }
                    Some(EffectiveActionFamilyV1::Crisis) => {
                        Some(&mut self.crisis.pending_entries_remaining)
                    }
                    Some(EffectiveActionFamilyV1::MetaProgress) => {
                        Some(&mut self.meta_progress.pending_entries_remaining)
                    }
                    Some(EffectiveActionFamilyV1::ContractAccept) => {
                        Some(&mut self.contract_accept.pending_entries_remaining)
                    }
                    _ => None,
                };
                Some((
                    Work::ContractLifecycle(
                        self.contract_lifecycle.as_deref_mut()?,
                        self.lifecycle_sources?,
                        self.normal_extension.as_deref()?.common,
                    ),
                    accepted,
                ))
            }
            EffectiveActionFamilyV1::Alliance => {
                let accepted = match source {
                    Some(EffectiveActionFamilyV1::FactorySchedule) => Some(
                        &mut self
                            .factory_schedule
                            .as_deref_mut()?
                            .pending_entries_remaining,
                    ),
                    Some(EffectiveActionFamilyV1::ContractLifecycle) => Some(
                        &mut self
                            .contract_lifecycle
                            .as_deref_mut()?
                            .shared
                            .pending_entries_remaining,
                    ),
                    Some(EffectiveActionFamilyV1::Crisis) => {
                        Some(&mut self.crisis.pending_entries_remaining)
                    }
                    Some(EffectiveActionFamilyV1::MetaProgress) => {
                        Some(&mut self.meta_progress.pending_entries_remaining)
                    }
                    Some(EffectiveActionFamilyV1::ContractAccept) => {
                        Some(&mut self.contract_accept.pending_entries_remaining)
                    }
                    _ => None,
                };
                Some((
                    Work::Alliance(
                        self.alliance.as_deref_mut()?,
                        self.alliance_sources?,
                        self.normal_extension.as_deref()?.common,
                    ),
                    accepted,
                ))
            }

            EffectiveActionFamilyV1::LogisticsRegistration => {
                let accepted = match source {
                    Some(EffectiveActionFamilyV1::FactorySchedule) => Some(
                        &mut self
                            .factory_schedule
                            .as_deref_mut()?
                            .pending_entries_remaining,
                    ),
                    Some(EffectiveActionFamilyV1::ContractLifecycle) => Some(
                        &mut self
                            .contract_lifecycle
                            .as_deref_mut()?
                            .shared
                            .pending_entries_remaining,
                    ),
                    Some(EffectiveActionFamilyV1::Alliance) => {
                        Some(&mut self.alliance.as_deref_mut()?.pending_entries_remaining)
                    }
                    Some(EffectiveActionFamilyV1::Crisis) => {
                        Some(&mut self.crisis.pending_entries_remaining)
                    }
                    Some(EffectiveActionFamilyV1::MetaProgress) => {
                        Some(&mut self.meta_progress.pending_entries_remaining)
                    }
                    Some(EffectiveActionFamilyV1::ContractAccept) => {
                        Some(&mut self.contract_accept.pending_entries_remaining)
                    }
                    _ => None,
                };
                Some((
                    Work::LogisticsRegistration(
                        self.logistics_registration.as_deref_mut()?,
                        self.logistics,
                        self.normal_extension.as_deref()?.common,
                    ),
                    accepted,
                ))
            }

            EffectiveActionFamilyV1::NormalExtension => {
                let accepted = match source {
                    Some(EffectiveActionFamilyV1::FactorySchedule) => Some(
                        &mut self
                            .factory_schedule
                            .as_deref_mut()?
                            .pending_entries_remaining,
                    ),
                    Some(EffectiveActionFamilyV1::ContractLifecycle) => Some(
                        &mut self
                            .contract_lifecycle
                            .as_deref_mut()?
                            .shared
                            .pending_entries_remaining,
                    ),
                    Some(EffectiveActionFamilyV1::Alliance) => {
                        Some(&mut self.alliance.as_deref_mut()?.pending_entries_remaining)
                    }
                    Some(EffectiveActionFamilyV1::Crisis) => {
                        Some(&mut self.crisis.pending_entries_remaining)
                    }
                    Some(EffectiveActionFamilyV1::MetaProgress) => {
                        Some(&mut self.meta_progress.pending_entries_remaining)
                    }
                    Some(EffectiveActionFamilyV1::ContractAccept) => {
                        Some(&mut self.contract_accept.pending_entries_remaining)
                    }
                    _ => None,
                };
                Some((
                    Work::NormalExtension(self.normal_extension.as_deref_mut()?.view()),
                    accepted,
                ))
            }

            EffectiveActionFamilyV1::Core => {
                let accepted = match source {
                    Some(EffectiveActionFamilyV1::FactorySchedule) => Some(
                        &mut self
                            .factory_schedule
                            .as_deref_mut()?
                            .pending_entries_remaining,
                    ),
                    Some(EffectiveActionFamilyV1::ContractLifecycle) => Some(
                        &mut self
                            .contract_lifecycle
                            .as_deref_mut()?
                            .shared
                            .pending_entries_remaining,
                    ),
                    Some(EffectiveActionFamilyV1::Alliance) => {
                        Some(&mut self.alliance.as_deref_mut()?.pending_entries_remaining)
                    }
                    Some(EffectiveActionFamilyV1::Crisis) => {
                        Some(&mut self.crisis.pending_entries_remaining)
                    }
                    Some(EffectiveActionFamilyV1::MetaProgress) => {
                        Some(&mut self.meta_progress.pending_entries_remaining)
                    }
                    Some(EffectiveActionFamilyV1::ContractAccept) => {
                        Some(&mut self.contract_accept.pending_entries_remaining)
                    }
                    _ => None,
                };
                Some((Work::Core, accepted))
            }
            EffectiveActionFamilyV1::Crisis => {
                let accepted = match source {
                    Some(EffectiveActionFamilyV1::FactorySchedule) => Some(
                        &mut self
                            .factory_schedule
                            .as_deref_mut()?
                            .pending_entries_remaining,
                    ),
                    Some(EffectiveActionFamilyV1::ContractLifecycle) => Some(
                        &mut self
                            .contract_lifecycle
                            .as_deref_mut()?
                            .shared
                            .pending_entries_remaining,
                    ),
                    Some(EffectiveActionFamilyV1::Alliance) => {
                        Some(&mut self.alliance.as_deref_mut()?.pending_entries_remaining)
                    }
                    Some(EffectiveActionFamilyV1::MetaProgress) => {
                        Some(&mut self.meta_progress.pending_entries_remaining)
                    }
                    Some(EffectiveActionFamilyV1::ContractAccept) => {
                        Some(&mut self.contract_accept.pending_entries_remaining)
                    }
                    _ => None,
                };
                let work = Work::Crisis(
                    &mut *self.crisis,
                    crate::world::incremental_action_crisis::CrisisSourcesV1 {
                        agents: self.crisis_sources.agents,
                        diplomacy: self.crisis_sources.diplomacy,
                    },
                );
                Some((work, accepted))
            }
            EffectiveActionFamilyV1::MetaProgress => {
                let accepted = match source {
                    Some(EffectiveActionFamilyV1::FactorySchedule) => Some(
                        &mut self
                            .factory_schedule
                            .as_deref_mut()?
                            .pending_entries_remaining,
                    ),
                    Some(EffectiveActionFamilyV1::ContractLifecycle) => Some(
                        &mut self
                            .contract_lifecycle
                            .as_deref_mut()?
                            .shared
                            .pending_entries_remaining,
                    ),
                    Some(EffectiveActionFamilyV1::Alliance) => {
                        Some(&mut self.alliance.as_deref_mut()?.pending_entries_remaining)
                    }
                    Some(EffectiveActionFamilyV1::Crisis) => {
                        Some(&mut self.crisis.pending_entries_remaining)
                    }
                    Some(EffectiveActionFamilyV1::ContractAccept) => {
                        Some(&mut self.contract_accept.pending_entries_remaining)
                    }
                    _ => None,
                };
                let work = Work::MetaProgress(
                    &mut *self.meta_progress,
                    crate::world::incremental_action_meta_progress::MetaProgressSourcesV1 {
                        agents: self.meta_sources.agents,
                        native: self.meta_sources.native,
                    },
                );
                Some((work, accepted))
            }
            EffectiveActionFamilyV1::ContractAccept => {
                let accepted = match source {
                    Some(EffectiveActionFamilyV1::FactorySchedule) => Some(
                        &mut self
                            .factory_schedule
                            .as_deref_mut()?
                            .pending_entries_remaining,
                    ),
                    Some(EffectiveActionFamilyV1::ContractLifecycle) => Some(
                        &mut self
                            .contract_lifecycle
                            .as_deref_mut()?
                            .shared
                            .pending_entries_remaining,
                    ),
                    Some(EffectiveActionFamilyV1::Alliance) => {
                        Some(&mut self.alliance.as_deref_mut()?.pending_entries_remaining)
                    }
                    Some(EffectiveActionFamilyV1::Crisis) => {
                        Some(&mut self.crisis.pending_entries_remaining)
                    }
                    Some(EffectiveActionFamilyV1::MetaProgress) => {
                        Some(&mut self.meta_progress.pending_entries_remaining)
                    }
                    _ => None,
                };
                let work = Work::ContractAccept(
                    &mut *self.contract_accept,
                    crate::world::incremental_action_contract_accept::ContractAcceptSourcesV1 {
                        agents: self.contract_sources.agents,
                        diplomacy: self.contract_sources.diplomacy,
                    },
                );
                Some((work, accepted))
            }
            EffectiveActionFamilyV1::FactoryLifecycle => {
                let accepted = match source {
                    Some(EffectiveActionFamilyV1::FactorySchedule) => Some(
                        &mut self
                            .factory_schedule
                            .as_deref_mut()?
                            .pending_entries_remaining,
                    ),
                    Some(EffectiveActionFamilyV1::ContractLifecycle) => Some(
                        &mut self
                            .contract_lifecycle
                            .as_deref_mut()?
                            .shared
                            .pending_entries_remaining,
                    ),
                    Some(EffectiveActionFamilyV1::Alliance) => {
                        Some(&mut self.alliance.as_deref_mut()?.pending_entries_remaining)
                    }
                    Some(EffectiveActionFamilyV1::Crisis) => {
                        Some(&mut self.crisis.pending_entries_remaining)
                    }
                    Some(EffectiveActionFamilyV1::MetaProgress) => {
                        Some(&mut self.meta_progress.pending_entries_remaining)
                    }
                    Some(EffectiveActionFamilyV1::ContractAccept) => {
                        Some(&mut self.contract_accept.pending_entries_remaining)
                    }
                    _ => None,
                };
                let work = Work::FactoryLifecycle(
                    &mut *self.factory_lifecycle,
                    crate::world::incremental_action_factory_lifecycle::FactoryLifecycleSourcesV1 {
                        agents: self.factory_sources.agents,
                        factories: self.factory_sources.factories,
                        industry: self.factory_sources.industry,
                        index: self.factory_sources.index,
                        native: self.factory_sources.native,
                        primitive: self.factory_sources.primitive,
                        inventory: self.factory_sources.inventory,
                    },
                );
                Some((work, accepted))
            }
            EffectiveActionFamilyV1::RouteAvailability => {
                let accepted = match source {
                    Some(EffectiveActionFamilyV1::FactorySchedule) => Some(
                        &mut self
                            .factory_schedule
                            .as_deref_mut()?
                            .pending_entries_remaining,
                    ),
                    Some(EffectiveActionFamilyV1::ContractLifecycle) => Some(
                        &mut self
                            .contract_lifecycle
                            .as_deref_mut()?
                            .shared
                            .pending_entries_remaining,
                    ),
                    Some(EffectiveActionFamilyV1::Alliance) => {
                        Some(&mut self.alliance.as_deref_mut()?.pending_entries_remaining)
                    }
                    Some(EffectiveActionFamilyV1::Crisis) => {
                        Some(&mut self.crisis.pending_entries_remaining)
                    }
                    Some(EffectiveActionFamilyV1::MetaProgress) => {
                        Some(&mut self.meta_progress.pending_entries_remaining)
                    }
                    Some(EffectiveActionFamilyV1::ContractAccept) => {
                        Some(&mut self.contract_accept.pending_entries_remaining)
                    }
                    _ => None,
                };
                Some((
                    Work::RouteAvailability(&mut *self.route_availability, self.logistics),
                    accepted,
                ))
            }
            EffectiveActionFamilyV1::MaterialTransfer => {
                let accepted = match source {
                    Some(EffectiveActionFamilyV1::FactorySchedule) => Some(
                        &mut self
                            .factory_schedule
                            .as_deref_mut()?
                            .pending_entries_remaining,
                    ),
                    Some(EffectiveActionFamilyV1::ContractLifecycle) => Some(
                        &mut self
                            .contract_lifecycle
                            .as_deref_mut()?
                            .shared
                            .pending_entries_remaining,
                    ),
                    Some(EffectiveActionFamilyV1::Alliance) => {
                        Some(&mut self.alliance.as_deref_mut()?.pending_entries_remaining)
                    }
                    Some(EffectiveActionFamilyV1::Crisis) => {
                        Some(&mut self.crisis.pending_entries_remaining)
                    }
                    Some(EffectiveActionFamilyV1::MetaProgress) => {
                        Some(&mut self.meta_progress.pending_entries_remaining)
                    }
                    Some(EffectiveActionFamilyV1::ContractAccept) => {
                        Some(&mut self.contract_accept.pending_entries_remaining)
                    }
                    _ => None,
                };
                Some((
                    Work::MaterialTransfer(&mut *self.material_transfer, self.materials),
                    accepted,
                ))
            }
            EffectiveActionFamilyV1::Body => {
                let accepted = match source {
                    Some(EffectiveActionFamilyV1::FactorySchedule) => Some(
                        &mut self
                            .factory_schedule
                            .as_deref_mut()?
                            .pending_entries_remaining,
                    ),
                    Some(EffectiveActionFamilyV1::ContractLifecycle) => Some(
                        &mut self
                            .contract_lifecycle
                            .as_deref_mut()?
                            .shared
                            .pending_entries_remaining,
                    ),
                    Some(EffectiveActionFamilyV1::Alliance) => {
                        Some(&mut self.alliance.as_deref_mut()?.pending_entries_remaining)
                    }
                    Some(EffectiveActionFamilyV1::Crisis) => {
                        Some(&mut self.crisis.pending_entries_remaining)
                    }
                    Some(EffectiveActionFamilyV1::MetaProgress) => {
                        Some(&mut self.meta_progress.pending_entries_remaining)
                    }
                    Some(EffectiveActionFamilyV1::ContractAccept) => {
                        Some(&mut self.contract_accept.pending_entries_remaining)
                    }
                    _ => None,
                };
                Some((Work::Body(&mut *self.body), accepted))
            }
            EffectiveActionFamilyV1::DataAccess => {
                let accepted = match source {
                    Some(EffectiveActionFamilyV1::FactorySchedule) => Some(
                        &mut self
                            .factory_schedule
                            .as_deref_mut()?
                            .pending_entries_remaining,
                    ),
                    Some(EffectiveActionFamilyV1::ContractLifecycle) => Some(
                        &mut self
                            .contract_lifecycle
                            .as_deref_mut()?
                            .shared
                            .pending_entries_remaining,
                    ),
                    Some(EffectiveActionFamilyV1::Alliance) => {
                        Some(&mut self.alliance.as_deref_mut()?.pending_entries_remaining)
                    }
                    Some(EffectiveActionFamilyV1::Crisis) => {
                        Some(&mut self.crisis.pending_entries_remaining)
                    }
                    Some(EffectiveActionFamilyV1::MetaProgress) => {
                        Some(&mut self.meta_progress.pending_entries_remaining)
                    }
                    Some(EffectiveActionFamilyV1::ContractAccept) => {
                        Some(&mut self.contract_accept.pending_entries_remaining)
                    }
                    _ => None,
                };
                Some((
                    Work::DataAccess(&mut *self.data_access, self.controls),
                    accepted,
                ))
            }
            EffectiveActionFamilyV1::CollectData => {
                let accepted = match source {
                    Some(EffectiveActionFamilyV1::FactorySchedule) => Some(
                        &mut self
                            .factory_schedule
                            .as_deref_mut()?
                            .pending_entries_remaining,
                    ),
                    Some(EffectiveActionFamilyV1::ContractLifecycle) => Some(
                        &mut self
                            .contract_lifecycle
                            .as_deref_mut()?
                            .shared
                            .pending_entries_remaining,
                    ),
                    Some(EffectiveActionFamilyV1::Alliance) => {
                        Some(&mut self.alliance.as_deref_mut()?.pending_entries_remaining)
                    }
                    Some(EffectiveActionFamilyV1::Crisis) => {
                        Some(&mut self.crisis.pending_entries_remaining)
                    }
                    Some(EffectiveActionFamilyV1::MetaProgress) => {
                        Some(&mut self.meta_progress.pending_entries_remaining)
                    }
                    Some(EffectiveActionFamilyV1::ContractAccept) => {
                        Some(&mut self.contract_accept.pending_entries_remaining)
                    }
                    _ => None,
                };
                Some((Work::CollectData(&mut *self.collect_data), accepted))
            }
            EffectiveActionFamilyV1::AuthenticatedCollectData => {
                let accepted = match source {
                    Some(EffectiveActionFamilyV1::FactorySchedule) => Some(
                        &mut self
                            .factory_schedule
                            .as_deref_mut()?
                            .pending_entries_remaining,
                    ),
                    Some(EffectiveActionFamilyV1::ContractLifecycle) => Some(
                        &mut self
                            .contract_lifecycle
                            .as_deref_mut()?
                            .shared
                            .pending_entries_remaining,
                    ),
                    Some(EffectiveActionFamilyV1::Alliance) => {
                        Some(&mut self.alliance.as_deref_mut()?.pending_entries_remaining)
                    }
                    Some(EffectiveActionFamilyV1::Crisis) => {
                        Some(&mut self.crisis.pending_entries_remaining)
                    }
                    Some(EffectiveActionFamilyV1::MetaProgress) => {
                        Some(&mut self.meta_progress.pending_entries_remaining)
                    }
                    Some(EffectiveActionFamilyV1::ContractAccept) => {
                        Some(&mut self.contract_accept.pending_entries_remaining)
                    }
                    _ => None,
                };
                Some((
                    Work::AuthenticatedCollectData(
                        &mut *self.authenticated_data,
                        &mut *self.authenticated_work,
                        self.starter_bindings,
                        self.controls,
                    ),
                    accepted,
                ))
            }
            EffectiveActionFamilyV1::TransferResource => {
                let accepted = match source {
                    Some(EffectiveActionFamilyV1::FactorySchedule) => Some(
                        &mut self
                            .factory_schedule
                            .as_deref_mut()?
                            .pending_entries_remaining,
                    ),
                    Some(EffectiveActionFamilyV1::ContractLifecycle) => Some(
                        &mut self
                            .contract_lifecycle
                            .as_deref_mut()?
                            .shared
                            .pending_entries_remaining,
                    ),
                    Some(EffectiveActionFamilyV1::Alliance) => {
                        Some(&mut self.alliance.as_deref_mut()?.pending_entries_remaining)
                    }
                    Some(EffectiveActionFamilyV1::Crisis) => {
                        Some(&mut self.crisis.pending_entries_remaining)
                    }
                    Some(EffectiveActionFamilyV1::MetaProgress) => {
                        Some(&mut self.meta_progress.pending_entries_remaining)
                    }
                    Some(EffectiveActionFamilyV1::ContractAccept) => {
                        Some(&mut self.contract_accept.pending_entries_remaining)
                    }
                    _ => None,
                };
                Some((
                    Work::TransferResource(&mut *self.transfer_resource),
                    accepted,
                ))
            }
        }
    }
}
