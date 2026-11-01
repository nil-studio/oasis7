//! Fixed control reads from caller-selected immutable namespace roots. These
//! point results do not certify the Cognition envelope or any side owner.
use crate::{
    incremental_cognition_fields::CognitionControlFieldV1,
    incremental_field_member::{MemberPayloadCodecV1, MemberReadProfileV1, MemberRecordLimitsV1},
    incremental_field_read::WorldFieldReadContextV1,
    incremental_utf8_output::read_text_value,
    incremental_world_field_key::{WorldFieldKeyV1, WorldFieldRecordKindV1},
};
use oasis7_formal_protocol::{
    incremental_state::MerkleNodeReader,
    incremental_value::{ValueNodeReader, ValueTreeError},
};
use std::io::Write;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CognitionControlReadV1 {
    Text { bytes: u64 },
    Unsigned(u64),
    Absent,
}

/// Text output is provisional until this call returns success; it is streamed
/// without collecting the journal digest or any other unbounded original text.
pub fn read_cognition_control_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    field: CognitionControlFieldV1,
    context: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    output: &mut impl Write,
) -> Result<CognitionControlReadV1, ValueTreeError> {
    context.check()?;
    let descriptor = field.descriptor();
    let metadata = WorldFieldKeyV1::new(descriptor, WorldFieldRecordKindV1::Metadata, None)
        .map_err(ValueTreeError::Invalid)?;
    let optional = field == CognitionControlFieldV1::ReorgInvalidationEpoch;
    let metadata_root = context.lookup(&metadata)?;
    let (key, bytes) = context.key(descriptor, WorldFieldRecordKindV1::Entry, &())?;
    let root = context.lookup(&key)?;
    let Some(metadata_root) = metadata_root else {
        if !optional || root.is_some() {
            return Err(ValueTreeError::Invalid(
                "cognition control missing metadata",
            ));
        }
        context.check()?;
        return Ok(CognitionControlReadV1::Absent);
    };
    context.records().presence(&metadata_root, &metadata, 0)?;
    let root = root.ok_or(ValueTreeError::Invalid("cognition control missing entry"))?;
    let result = match field {
        CognitionControlFieldV1::JournalHeadSeq
        | CognitionControlFieldV1::ReorgInvalidationEpoch => {
            CognitionControlReadV1::Unsigned(context.records().unsigned(&root, &key, bytes)?)
        }
        _ => {
            let bytes = read_text_value(
                &root,
                &key,
                &mut context.values,
                MemberReadProfileV1 {
                    codec: MemberPayloadCodecV1::Text,
                    semantic_bytes: bytes,
                    limits: MemberRecordLimitsV1 {
                        semantic_bytes: context.semantic_bytes,
                        payload_bytes: context.record_bytes,
                        record_bytes: context.record_bytes,
                    },
                },
                output,
                "cognition control UTF-8",
                &mut context.cancelled,
            )?;
            CognitionControlReadV1::Text { bytes }
        }
    };
    context.check()?;
    Ok(result)
}
