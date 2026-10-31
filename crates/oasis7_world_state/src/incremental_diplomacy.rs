//! Closed Business owner records, not diplomacy/settlement authority.
use crate::{
    gameplay_state::{
        AllianceState, CrisisState, CrisisStatus, EconomicContractFulfillmentKind,
        EconomicContractState, EconomicContractStatus, WarParticipantOutcome, WarState,
    },
    incremental_field_member::*,
    incremental_field_read::WorldFieldReadContextV1,
    incremental_inventory_value_read::InventoryValueReader,
    incremental_projection::{FieldDestination, FieldOwner, FieldShape, WORLD_STATE_FIELDS},
    incremental_record_value_read::{
        RecordReadLabelsV1, RecordReadProfileV1, RecordValueV1, read_record_value_v1,
    },
    incremental_semantic_key::hash_semantic_key_v1,
};
use oasis7_agent_api::world_action::ResourceKind;
use oasis7_formal_protocol::{
    incremental_record_root::{RECORD_ROOT_BYTES, RecordRootRefV1},
    incremental_state::{MerkleNodeReader, StateNamespaceV1, StateRootV1, StateScanBudget},
    incremental_value::{ValueNodeReader, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryClass, ReservedValue};
use std::io::Cursor;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiplomacyFieldV1 {
    Alliances,
    Wars,
    Crises,
    EconomicContracts,
}
impl DiplomacyFieldV1 {
    pub const ALL: [Self; 4] = [
        Self::Alliances,
        Self::Wars,
        Self::Crises,
        Self::EconomicContracts,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Self::Alliances => "alliances",
            Self::Wars => "wars",
            Self::Crises => "crises",
            Self::EconomicContracts => "economic_contracts",
        }
    }
    pub fn schema(self) -> [u8; 32] {
        let mut h = blake3::Hasher::new();
        h.update(b"oasis7.iws.diplomacy-relative.v1\0");
        h.update(self.name().as_bytes());
        *h.finalize().as_bytes()
    }
    pub fn descriptor(self) -> Result<FieldDescriptor, ValueTreeError> {
        WORLD_STATE_FIELDS
            .iter()
            .copied()
            .find(|f| {
                f.name == self.name()
                    && f.owner == FieldOwner::WorldState
                    && f.shape == FieldShape::Map
                    && f.destination == FieldDestination::State(StateNamespaceV1::Business)
            })
            .ok_or(ValueTreeError::Invalid("diplomacy registration"))
    }
    fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|f| f.name() == name)
    }
}
#[derive(Clone, Copy)]
pub enum DiplomacySourceV1<'a> {
    Alliance(&'a AllianceState),
    War(&'a WarState),
    Crisis(&'a CrisisState),
    Contract(&'a EconomicContractState),
}
impl DiplomacySourceV1<'_> {
    fn field(self) -> DiplomacyFieldV1 {
        match self {
            Self::Alliance(_) => DiplomacyFieldV1::Alliances,
            Self::War(_) => DiplomacyFieldV1::Wars,
            Self::Crisis(_) => DiplomacyFieldV1::Crises,
            Self::Contract(_) => DiplomacyFieldV1::EconomicContracts,
        }
    }
    fn length(self) -> u64 {
        match self {
            Self::Alliance(v) => v.members.len() as u64,
            Self::War(v) => v.participant_outcomes.len() as u64,
            _ => 0,
        }
    }
}
#[path = "incremental_diplomacy_key.rs"]
mod key;
use key::Path;
#[path = "incremental_diplomacy_profile.rs"]
mod profile;
use profile::Kind;
#[path = "incremental_diplomacy_root.rs"]
mod root;
pub use root::{inspect_diplomacy_root_v1, write_diplomacy_root_reference_v1};
#[path = "incremental_diplomacy_genesis.rs"]
mod genesis;
pub use genesis::prepare_diplomacy_genesis_v1;

#[path = "incremental_diplomacy_closure.rs"]
mod closure;
#[path = "incremental_diplomacy_schema.rs"]
mod schema;
pub use closure::{DiplomacyRecordQualificationV1, qualify_diplomacy_body_v1};
pub use profile::{
    AllianceScalarV1, ContractScalarV1, CrisisScalarV1, DiplomacyScalarV1, WarScalarV1,
};

#[path = "incremental_diplomacy_family.rs"]
mod family;
pub use family::{DiplomacyFamilyQualificationV1, qualify_diplomacy_family_v1};

#[path = "incremental_diplomacy_access.rs"]
mod access;
pub use crate::incremental_record_value_read::RecordValueV1 as DiplomacyValueV1;
pub use access::{open_qualified_diplomacy_record_v1, read_qualified_diplomacy_scalar_v1};
#[path = "incremental_diplomacy_create.rs"]
mod create;
pub use create::{DiplomacyEmptyCreateV1, create_qualified_diplomacy_empty_v1};

#[path = "incremental_diplomacy_scalars.rs"]
mod scalars;
pub use scalars::{DiplomacyScalarsChangeV1, change_qualified_diplomacy_scalars_v1};

#[path = "incremental_diplomacy_members.rs"]
mod members;
pub use members::{
    DiplomacyMemberChangeV1, DiplomacyMemberV1, change_qualified_diplomacy_member_v1,
};

#[path = "incremental_diplomacy_tree.rs"]
mod inner_tree;

#[path = "incremental_diplomacy_pages.rs"]
mod pages;
pub use pages::{
    DiplomacyFamilyPagesV1, DiplomacyPagesWorkV1, mark_diplomacy_family_pages_v1,
    transfer_diplomacy_family_pages_v1,
};

#[path = "incremental_diplomacy_receiver.rs"]
mod receiver;
pub use receiver::accept_diplomacy_transition_v1;

#[path = "incremental_diplomacy_member_read.rs"]
mod member_read;
pub use member_read::{
    DiplomacyMemberColumnV1, DiplomacyMemberReadV1, read_qualified_diplomacy_length_v1,
    read_qualified_diplomacy_member_v1,
};

#[path = "incremental_diplomacy_delete.rs"]
mod delete;
pub use delete::{DiplomacyClosedDeleteV1, delete_qualified_diplomacy_closed_v1};

#[path = "incremental_diplomacy_scalar_point.rs"]
mod scalar_point;
pub use scalar_point::{
    DiplomacyScalarChangeV1, DiplomacyScalarValueV1, change_qualified_diplomacy_scalar_v1,
};
