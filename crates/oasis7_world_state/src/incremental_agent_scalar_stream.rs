//! The original borrowed scalar encoder's single ValueTree stream producer.
use super::*;
use crate::incremental_value_mutation::ValueMutationContextV1;
use oasis7_formal_protocol::incremental_value::{
    ValueNodeWriter, ValueRootV1, ValueStreamWriter, ValueTreeError,
};
/// The caller owns the field/profile/size admission and result adoption. This
/// streams the existing scalar wire only; it grants no Agent qualification.
pub fn write_agent_scalar_stream_v1<W: ValueNodeWriter, C: FnMut() -> bool>(
    value: AgentScalarRefV1<'_>,
    context: &mut ValueMutationContextV1<'_, W, C>,
) -> Result<ValueRootV1, ValueTreeError> {
    let length = value.encoded_len().map_err(ValueTreeError::Invalid)?;
    let mut writer = ValueStreamWriter::new(
        context.sink,
        context.memory,
        context.io,
        length,
        &mut context.cancelled,
    )?;
    if value.write_to(&mut writer).is_err() {
        return Err(writer
            .finish()
            .err()
            .unwrap_or(ValueTreeError::Invalid("agent scalar write failed")));
    }
    writer.finish()
}
