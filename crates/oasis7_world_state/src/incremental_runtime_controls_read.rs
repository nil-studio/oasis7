//! Fixed-root control reads. Output is provisional until final cancellation.
use super::*;
use crate::incremental_field_read::WorldFieldReadContextV1;
use oasis7_formal_protocol::incremental_state::MerkleNodeReader;
pub fn read_runtime_control_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    field: RuntimeControlFieldV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    max_text_bytes: u64,
    output: &mut impl Write,
) -> Result<RuntimeControlReadV1, ValueTreeError> {
    ctx.check()?;
    if ctx.root.namespace != field.namespace() {
        return Err(invalid());
    }
    let metadata = WorldFieldKeyV1::new(field.descriptor(), WorldFieldRecordKindV1::Metadata, None)
        .map_err(ValueTreeError::Invalid)?;
    let id = ctx.lookup(&metadata)?.ok_or_else(invalid)?;
    ctx.values
        .verify_closure(&id, ctx.record_bytes, &mut ctx.cancelled)?;
    ctx.records().presence(&id, &metadata, 0)?;
    let key = ctx.key(field.descriptor(), WorldFieldRecordKindV1::Entry, &())?;
    let id = ctx.lookup(&key.0)?.ok_or_else(invalid)?;
    let value = read_value(field, &id, &key, &mut ctx.records(), max_text_bytes, output)?;
    ctx.check()?;
    value.try_into()
}
