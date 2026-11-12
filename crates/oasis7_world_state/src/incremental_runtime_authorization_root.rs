//! Original authorization root string, with exact field identity.
use crate::incremental_field_member::*;
use oasis7_formal_protocol::{
    incremental_state::{MerkleNodeReader, StateNamespaceV1},
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueTreeError},
};
pub fn authorization_root_descriptor_v1() -> FieldDescriptor {
    FieldDescriptor {
        owner: FieldOwner::Runtime,
        name: "capability_authorization_root",
        shape: FieldShape::Value,
        destination: FieldDestination::State(StateNamespaceV1::Authorization),
    }
}
pub fn prepare_authorization_root_value_v1<W: ValueNodeWriter, C: FnMut() -> bool>(
    source: &str,
    ctx: &mut MemberRecordWriteContext<'_, W, C>,
) -> Result<MemberRecordV1, MemberRecordError> {
    write_member_record_v1(
        authorization_root_descriptor_v1(),
        WorldFieldRecordKindV1::Entry,
        &(),
        MemberPayloadRefV1::Text(source),
        ctx,
    )
}
/// Streams exact original UTF-8 into caller output; never hydrates the string.
pub fn read_authorization_root_value_v1<M, R, E, C>(
    ctx: &mut crate::incremental_field_read::WorldFieldReadContextV1<'_, M, R, E, C>,
    max_bytes: u64,
    output: &mut impl std::io::Write,
) -> Result<u64, ValueTreeError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    ctx.check()?;
    if ctx.root.namespace != StateNamespaceV1::Authorization {
        return Err(ValueTreeError::Invalid("authorization root namespace"));
    }
    let descriptor = authorization_root_descriptor_v1();
    let metadata = WorldFieldKeyV1::new(descriptor, WorldFieldRecordKindV1::Metadata, None)
        .map_err(ValueTreeError::Invalid)?;
    let meta = ctx.lookup(&metadata)?.ok_or(ValueTreeError::Invalid(
        "authorization root metadata missing",
    ))?;
    ctx.records().presence(&meta, &metadata, 0)?;
    let key = ctx.key(descriptor, WorldFieldRecordKindV1::Entry, &())?;
    let value = ctx
        .lookup(&key.0)?
        .ok_or(ValueTreeError::Invalid("authorization root value missing"))?;
    use crate::incremental_record_value_read::*;
    let profile = authorization_root_read_profile_v1();
    let result =
        read_record_value_v1(&value, &key, &mut ctx.records(), max_bytes, output, profile)?;
    ctx.check()?;
    match result {
        RecordValueV1::Text { utf8_bytes } => Ok(utf8_bytes),
        _ => Err(ValueTreeError::Invalid("authorization root codec")),
    }
}

fn authorization_root_read_profile_v1() -> crate::incremental_record_value_read::RecordReadProfileV1
{
    use crate::incremental_record_value_read::*;
    RecordReadProfileV1 {
        codec: MemberPayloadCodecV1::Text,
        nullable: false,
        integer_max: u64::MAX,
        labels: RecordReadLabelsV1 {
            utf8: "authorization root UTF8",
            ledger_utf8: "authorization root ledger UTF8",
            ledger_frame: "authorization root ledger frame",
            boolean: "authorization root bool",
            fixed: "authorization root fixed",
            profile: "authorization root profile",
            integer: "authorization root integer",
        },
    }
}

#[path = "incremental_runtime_authorization_root_family.rs"]
mod family;
pub use family::{
    RuntimeAuthorizationRootQualificationV1, accept_runtime_authorization_root_transition_v1,
    qualify_runtime_authorization_root_v1, readmit_runtime_authorization_root_ordinary_v1,
};

#[cfg(test)]
#[path = "incremental_runtime_authorization_root_family_tests.rs"]
mod family_tests;

pub(crate) use family::accept_runtime_authorization_root_verified_v1;

#[path = "incremental_runtime_authorization_root_overlay.rs"]
mod overlay;
pub use overlay::replace_current_authorization_root_v1;
