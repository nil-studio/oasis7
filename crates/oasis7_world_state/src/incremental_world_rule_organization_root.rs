//! Integrity edges of the relative organization record profile.
//! Parsing or writing an edge never grants an organization closure proof.
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
use std::io::Cursor;
#[path = "incremental_world_rule_organization_closure_tree.rs"]
mod tree;

#[derive(Clone, Copy)]
pub enum OrganizationRootKeyV1<'a> {
    Case(&'a str),
    Estate(&'a str),
}
impl<'a> OrganizationRootKeyV1<'a> {
    pub fn schema(self) -> [u8; 32] {
        *blake3::hash(match self {
            Self::Case(_) => b"oasis7.iws.organization.case-relative.v1\0",
            Self::Estate(_) => b"oasis7.iws.organization.estate-relative.v1\0",
        })
        .as_bytes()
    }
    fn outer(self) -> K<'a> {
        match self {
            Self::Case(key) => K::OrganizationCase(key),
            Self::Estate(key) => K::OrganizationEstate(key),
        }
    }
}
/// Reads and binds the integrity edge. The returned root remains unqualified:
/// owner bootstrap/transition validation must finish before creation or use.
pub fn inspect_organization_root_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    owner: OrganizationRootKeyV1<'_>,
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
        .map_err(|_| ValueTreeError::Invalid("organization root reference"))?;
    let owner_key = WorldFieldKeyV1::from_bytes(key.as_bytes())
        .map_err(ValueTreeError::Invalid)?
        .semantic_key
        .ok_or(ValueTreeError::Invalid("organization owner key"))?;
    let root = reference
        .validate_binding(&owner.schema(), &owner_key, StateNamespaceV1::Authorization)
        .map_err(|_| ValueTreeError::Invalid("organization root binding"))?
        .clone();
    if (context.cancelled)() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(Some(retained.retain(root)))
}
/// Produces a provisional edge only; no authority or qualification is minted.
pub fn write_organization_root_reference_v1<
    W: oasis7_formal_protocol::incremental_value::ValueNodeWriter,
    C: FnMut() -> bool,
>(
    owner: OrganizationRootKeyV1<'_>,
    child: &StateRootV1,
    context: &mut MemberRecordWriteContext<'_, W, C>,
) -> Result<MemberRecordV1, MemberRecordError> {
    if child.namespace != StateNamespaceV1::Authorization {
        return Err(ValueTreeError::Invalid("organization root namespace").into());
    }
    let _scratch = context
        .memory
        .reserve(4096, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let semantic = match owner {
        OrganizationRootKeyV1::Case(key) => crate::incremental_semantic_key::hash_semantic_key_v1(
            &("organization_cases", key),
            context.memory,
            MemoryClass::Ordinary,
            context.key_work,
            &mut context.cancelled,
        ),
        OrganizationRootKeyV1::Estate(key) => {
            crate::incremental_semantic_key::hash_semantic_key_v1(
                &("organization_estate", key),
                context.memory,
                MemoryClass::Ordinary,
                context.key_work,
                &mut context.cancelled,
            )
        }
    }
    .map_err(MemberRecordError::Key)?;
    let reference = RecordRootRefV1::new(owner.schema(), semantic.digest, child.clone())
        .map_err(|_| ValueTreeError::Invalid("organization root child"))?;
    owner.outer().write_path(
        RecordPathV1::Record,
        MemberPayloadRefV1::RecordRoot(&reference),
        context,
    )
}
pub use tree::OrganizationChangedTreeContextV1;
/// Streams the complete borrowed relative body, including all collection
/// members. These provisional writes do not certify the resulting tree.
pub fn prepare_organization_relative_body_v1<
    W: oasis7_formal_protocol::incremental_value::ValueNodeWriter,
    C: FnMut() -> bool,
    S: FnMut(
        oasis7_proto::memory_budget::ReservedValue<
            oasis7_formal_protocol::incremental_execution::ExecutionWriteV1,
        >,
    ) -> Result<(), ValueTreeError>,
>(
    source: super::WorldRuleOrganizationScalarsRefV1<'_>,
    context: &mut MemberRecordWriteContext<'_, W, C>,
    remaining: &mut u64,
    sink: &mut S,
) -> Result<(), MemberRecordError> {
    visit_relative_body(source, &mut |record, path, payload| {
        super::genesis::emit(record, path, payload, context, remaining, sink)
    })?;
    if (context.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok(())
}
pub(super) fn visit_relative_body<'a>(
    source: super::WorldRuleOrganizationScalarsRefV1<'a>,
    visit: &mut impl FnMut(
        K<'a>,
        RecordPathV1<'a>,
        MemberPayloadRefV1<'a>,
    ) -> Result<(), MemberRecordError>,
) -> Result<(), MemberRecordError> {
    use super::WorldRuleOrganizationScalarsRefV1 as V;
    match source {
        V::Case(value) => {
            relative_columns(K::OrganizationCaseInner, source, visit)?;
            for member in &value.members {
                visit(
                    K::OrganizationCaseInner,
                    RecordPathV1::Member("members", member),
                    MemberPayloadRefV1::Unit,
                )?;
            }
            for (key, grant) in &value.grants {
                relative_columns(K::OrganizationGrantInner(key), V::Grant(grant), visit)?;
                for scope in &grant.scopes {
                    visit(
                        K::OrganizationGrantInner(key),
                        super::members::scope_path(scope),
                        MemberPayloadRefV1::Unit,
                    )?;
                }
            }
            for (key, request) in &value.requests {
                relative_columns(
                    K::OrganizationRecoveryInner(key),
                    V::Recovery(request),
                    visit,
                )?;
            }
            for (index, receipt) in value.stage_receipts.iter().enumerate() {
                let ordinal = u64::try_from(index)
                    .map_err(|_| ValueTreeError::ResourceLimited("organization receipt ordinal"))?;
                visit(
                    K::OrganizationCaseInner,
                    RecordPathV1::Ordinal("stage_receipts", ordinal),
                    MemberPayloadRefV1::Text(receipt),
                )?;
            }
        }
        V::Estate(_) => relative_columns(K::OrganizationEstateInner, source, visit)?,
        V::Grant(_) | V::Recovery(_) => {
            return Err(MemberRecordError::Invalid(
                "organization body requires case or estate",
            ));
        }
    }
    Ok(())
}
fn relative_columns<'a>(
    record: K<'a>,
    source: super::WorldRuleOrganizationScalarsRefV1<'a>,
    visit: &mut impl FnMut(
        K<'a>,
        RecordPathV1<'a>,
        MemberPayloadRefV1<'a>,
    ) -> Result<(), MemberRecordError>,
) -> Result<(), MemberRecordError> {
    for (spec, payload) in source
        .profile()
        .specs()
        .iter()
        .zip(super::payloads(source)?)
    {
        if let Some(payload) = payload {
            visit(record, spec.path, payload)?;
        }
    }
    Ok(())
}

#[path = "incremental_world_rule_organization_qualification.rs"]
mod qualification;
pub use qualification::{
    ExpectedOrganizationRecordProofV1, verify_expected_organization_record_v1,
};

#[path = "incremental_world_rule_organization_closure_schema.rs"]
mod schema;
pub use schema::{
    OrganizationRelativeKeyV1, OrganizationRelativeRecordV1, inspect_organization_relative_key_v1,
};

#[cfg(test)]
#[path = "incremental_world_rule_organization_closure_tests.rs"]
mod tests;

#[path = "incremental_world_rule_organization_closure.rs"]
pub(crate) mod closure;
pub use closure::{OrganizationRecordQualificationV1, qualify_organization_body_v1};

#[path = "incremental_world_rule_organization_family.rs"]
mod family;
pub use family::{OrganizationFamilyQualificationV1, qualify_organization_family_v1};

#[path = "incremental_world_rule_organization_root_create.rs"]
mod create;
pub use create::{OrganizationEmptyRootCreateV1, create_qualified_organization_empty_v1};

#[path = "incremental_world_rule_organization_root_tree.rs"]
pub(crate) mod inner_tree;

#[path = "incremental_world_rule_organization_root_member.rs"]
mod root_member;
pub use root_member::{OrganizationRootMemberChangeV1, change_qualified_organization_member_v1};

#[path = "incremental_world_rule_organization_receiver.rs"]
mod receiver;
pub use receiver::accept_organization_transition_v1;

#[path = "incremental_world_rule_organization_fields_schema.rs"]
mod fields_schema;
pub use fields_schema::{
    OrganizationFieldReadV1, OrganizationFieldRefV1, OrganizationFieldValueV1,
    OrganizationScalarFieldV1,
};
#[path = "incremental_world_rule_organization_fields_read.rs"]
mod fields_read;
pub use fields_read::{OrganizationFieldRequestV1, read_qualified_organization_field_v1};
#[path = "incremental_world_rule_organization_root_open.rs"]
mod open;
pub use open::open_qualified_organization_record_v1;

#[path = "incremental_world_rule_organization_root_access.rs"]
mod access;

#[path = "incremental_world_rule_organization_fields_write.rs"]
mod fields_write;
pub use fields_write::{OrganizationFieldChangeV1, change_qualified_organization_field_v1};

#[path = "incremental_world_rule_organization_root_child_create.rs"]
mod child_create;
pub use child_create::{
    OrganizationEmptyChildCreateV1, create_qualified_organization_child_empty_v1,
};

#[path = "incremental_world_rule_organization_root_set.rs"]
mod root_set;
pub use root_set::{
    OrganizationRelativeSetKeyV1, OrganizationSetChangeV1, OrganizationSetReadV1,
    OrganizationSetRequestV1, change_qualified_organization_set_v1,
    read_qualified_organization_set_v1,
};

#[path = "incremental_world_rule_organization_root_genesis.rs"]
mod root_genesis;
pub use root_genesis::prepare_qualified_organization_genesis_v1;

#[path = "incremental_world_rule_organization_root_receipts.rs"]
mod root_receipts;
pub use root_receipts::{
    change_qualified_organization_receipt_v1, read_qualified_organization_receipt_v1,
};

#[path = "incremental_world_rule_organization_root_delete.rs"]
mod root_delete;
pub use root_delete::{
    OrganizationClosedDeleteResultV1, OrganizationClosedDeleteV1,
    delete_qualified_organization_closed_v1,
};

#[path = "incremental_world_rule_organization_pages.rs"]
mod pages;
pub use pages::{
    OrganizationFamilyPagesV1, OrganizationPagesWorkV1, mark_organization_family_pages_v1,
    transfer_organization_family_pages_v1,
};
