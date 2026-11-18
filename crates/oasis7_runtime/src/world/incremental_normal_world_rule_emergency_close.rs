//! Fixed Emergency Close owner. The private post-write cap is not a general
//! DomainEvent dispatch or accepted-history capability.
use super::*;
use super::delivery::{IndustrialDeliverySourcesV1,IndustrialDeliveryWorkV1,IndustrialDeliveryErrorV1};
use crate::world::incremental_normal_tail::NormalTailWorkV1;
use oasis7_formal_protocol::{
    incremental_state::{StateNamespaceV1, StatePageWriter},
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueReadContext, ValueTreeError},
};
use oasis7_world_state::{
    incremental_factory_schedule_authority::FactoryScheduleAuthorityWorkV1,
    incremental_field_member::MemberRecordWriteContext,
    incremental_field_overlay::WorldFieldOverlayContextV1,
    incremental_field_read::WorldFieldReadContextV1,
    incremental_world_native::WorldNativeRecoveredRecordV1, incremental_world_rules_family::*,
};
use oasis7_proto::memory_budget::MemoryClass;
#[path="incremental_normal_world_rule_emergency_close_sources.rs"]
pub(in crate::world) mod sources;
pub(in crate::world) struct EmergencyCloseSourcesV1<'a,'source>{pub delivery:&'a IndustrialDeliverySourcesV1<'a,'source>,pub rules:&'a ReservedValue<WorldRulesQualificationV1>}

#[path="incremental_normal_world_rule_emergency_close_active.rs"] mod active;
#[path="incremental_normal_world_rule_emergency_close_commit.rs"] mod commit;
pub(in crate::world) use commit::{CommittedCurrentEmergencyCloseV1,close_current_emergency_authorization_v1};

pub(super) use active::validate_current_active_v1;
