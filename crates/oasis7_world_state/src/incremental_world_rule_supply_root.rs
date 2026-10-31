//! Integrity edges of the relative Supply record profile.
//! Parsing or writing an edge never grants a Supply closure proof.
use crate::incremental_world_rule_organization::root::inner_tree;
use crate::{
    incremental_field_member::*,
    incremental_field_read::WorldFieldReadContextV1,
    incremental_world_rule_record::{RecordPathV1, SupplyRecordKeyV1 as K},
};
use oasis7_formal_protocol::{
    incremental_record_root::{RECORD_ROOT_BYTES, RecordRootRefV1},
    incremental_state::{MerkleNodeReader, StateNamespaceV1, StateRootV1, StateScanBudget},
    incremental_value::{ValueNodeReader, ValueTreeError},
};
use oasis7_proto::memory_budget::MemoryClass;
use oasis7_proto::memory_budget::ReservedValue;
use std::io::Cursor;

#[derive(Clone, Copy)]
pub enum SupplyRootKeyV1<'a> {
    Authorization(&'a str),
    Batch(&'a str),
}
impl<'a> SupplyRootKeyV1<'a> {
    pub fn schema(self) -> [u8; 32] {
        *blake3::hash(match self {
            Self::Authorization(_) => b"oasis7.iws.supply.authorization-relative.v1\0",
            Self::Batch(_) => b"oasis7.iws.supply.batch-relative.v1\0",
        })
        .as_bytes()
    }
    fn outer(self) -> K<'a> {
        match self {
            Self::Authorization(key) => K::Authorization(key),
            Self::Batch(key) => K::Batch(key),
        }
    }
}
/// Reads and binds the integrity edge. The returned root remains unqualified:
/// owner bootstrap/transition validation must finish before creation or use.
pub fn inspect_supply_root_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    owner: SupplyRootKeyV1<'_>,
    context: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
) -> Result<Option<oasis7_proto::memory_budget::ReservedValue<StateRootV1>>, ValueTreeError> {
    let (key, semantic_bytes) = owner.outer().fixed_path(RecordPathV1::Record, context)?;
    let Some(value) = context.lookup(&key)? else {
        return Ok(None);
    };
    let _scratch = context
        .values
        .memory
        .reserve(RECORD_ROOT_BYTES + 4096, context.values.class)?;
    let retained = context.values.memory.reserve(512, context.values.class)?;
    let mut bytes = [0; RECORD_ROOT_BYTES];
    read_member_record_profile_v1(
        &value,
        &key,
        &mut context.values,
        MemberReadProfileV1 {
            codec: MemberPayloadCodecV1::RecordRoot,
            semantic_bytes,
            limits: MemberRecordLimitsV1 {
                semantic_bytes,
                payload_bytes: RECORD_ROOT_BYTES as u64,
                record_bytes: context.record_bytes,
            },
        },
        &mut std::io::sink(),
        &mut Cursor::new(bytes.as_mut_slice()),
        &mut context.cancelled,
    )?;
    let reference = RecordRootRefV1::from_canonical_bytes(&bytes)
        .map_err(|_| ValueTreeError::Invalid("supply root reference"))?;
    let owner_key = WorldFieldKeyV1::from_bytes(key.as_bytes())
        .map_err(ValueTreeError::Invalid)?
        .semantic_key
        .ok_or(ValueTreeError::Invalid("supply owner key"))?;
    let root = reference
        .validate_binding(&owner.schema(), &owner_key, StateNamespaceV1::Authorization)
        .map_err(|_| ValueTreeError::Invalid("supply root binding"))?
        .clone();
    if (context.cancelled)() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(Some(retained.retain(root)))
}
/// Produces a provisional edge only; no authority or qualification is minted.
pub fn write_supply_root_reference_v1<
    W: oasis7_formal_protocol::incremental_value::ValueNodeWriter,
    C: FnMut() -> bool,
>(
    owner: SupplyRootKeyV1<'_>,
    child: &StateRootV1,
    context: &mut MemberRecordWriteContext<'_, W, C>,
) -> Result<MemberRecordV1, MemberRecordError> {
    if child.namespace != StateNamespaceV1::Authorization {
        return Err(ValueTreeError::Invalid("supply root namespace").into());
    }
    let _scratch = context
        .memory
        .reserve(4096, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let semantic = match owner {
        SupplyRootKeyV1::Authorization(key) => {
            crate::incremental_semantic_key::hash_semantic_key_v1(
                &("supply_authorizations", key),
                context.memory,
                MemoryClass::Ordinary,
                context.key_work,
                &mut context.cancelled,
            )
        }
        SupplyRootKeyV1::Batch(key) => crate::incremental_semantic_key::hash_semantic_key_v1(
            &("supply_batches", key),
            context.memory,
            MemoryClass::Ordinary,
            context.key_work,
            &mut context.cancelled,
        ),
    }
    .map_err(MemberRecordError::Key)?;
    let reference = RecordRootRefV1::new(owner.schema(), semantic.digest, child.clone())
        .map_err(|_| ValueTreeError::Invalid("supply root child"))?;
    owner.outer().write_path(
        RecordPathV1::Record,
        MemberPayloadRefV1::RecordRoot(&reference),
        context,
    )
}

#[path = "incremental_world_rule_supply_root_profile.rs"]
mod profile;
use profile::*;

pub use profile::SupplyRootSourceV1;
#[path = "incremental_world_rule_supply_root_closure.rs"]
mod closure;
#[path = "incremental_world_rule_supply_root_schema.rs"]
mod schema;
pub use closure::{SupplyRecordQualificationV1, qualify_supply_body_v1};

#[path = "incremental_world_rule_supply_family.rs"]
mod family;
pub use family::{SupplyFamilyQualificationV1, qualify_supply_family_v1};

#[path = "incremental_world_rule_supply_root_create.rs"]
mod create;
pub use create::{SupplyEmptyRootCreateV1, create_qualified_supply_empty_v1};

#[path = "incremental_world_rule_supply_root_genesis.rs"]
mod genesis;
pub use genesis::{SupplyRootGenesisCountsV1, prepare_supply_root_genesis_v1};

#[path = "incremental_world_rule_supply_root_access.rs"]
mod access;

#[path = "incremental_world_rule_supply_root_member.rs"]
mod member;

#[path = "incremental_world_rule_supply_root_scalars.rs"]
mod scalars;

#[path = "incremental_world_rule_supply_root_receiver.rs"]
mod receiver;

#[path = "incremental_world_rule_supply_root_open.rs"]
mod open;
pub use member::{
    SupplyMemberChangeV1, SupplyMemberKeyV1, SupplyMemberRequestV1, SupplyMemberValueV1,
    change_qualified_supply_member_v1, read_qualified_supply_member_v1,
};
pub use open::open_qualified_supply_record_v1;
pub use receiver::accept_supply_transition_v1;
pub use scalars::{SupplyScalarChangeV1, change_qualified_supply_scalars_v1};

#[path = "incremental_world_rule_supply_root_pages.rs"]
mod pages;
pub use pages::{
    SupplyFamilyPagesV1, SupplyPagesWorkV1, mark_supply_family_pages_v1,
    transfer_supply_family_pages_v1,
};

#[path = "incremental_world_rule_supply_root_scalar_read.rs"]
mod scalar_read;
pub use scalar_read::{
    SupplyAuthorizationFieldV1, SupplyBatchFieldV1, SupplyCrisisFieldV1, SupplyScalarFieldV1,
    SupplyScalarRequestV1, SupplyScalarValueV1, read_qualified_supply_scalar_v1,
};
