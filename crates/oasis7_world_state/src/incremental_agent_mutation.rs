//! The original Agent/body mutation errors and sole fixed-byte writer.
//! All output remains provisional; these APIs grant no owner/history authority.
use crate::incremental_agent_body::AgentBodyValueV1;
use oasis7_formal_protocol::{
    incremental_execution::ExecutionError,
    incremental_value::{ValueNodeWriter, ValueStreamWriter, ValueTreeError},
};
use std::io::Write;
#[derive(Debug)]
pub enum BodyMutationError {
    Execution(ExecutionError),
    Value(ValueTreeError),
    Invalid(&'static str),
    Rejected(String),
    Original(Box<dyn std::error::Error + Send + Sync>),
}
impl From<ExecutionError> for BodyMutationError {
    fn from(error: ExecutionError) -> Self {
        Self::Execution(error)
    }
}
impl From<ValueTreeError> for BodyMutationError {
    fn from(error: ValueTreeError) -> Self {
        Self::Value(error)
    }
}
impl std::fmt::Display for BodyMutationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Execution(error) => error.fmt(f),
            Self::Value(error) => error.fmt(f),
            Self::Invalid(reason) => write!(f, "invalid IWS body mutation: {reason}"),
            Self::Rejected(reason) => write!(f, "rejected body update: {reason}"),
            Self::Original(error) => write!(f, "original event storage: {error}"),
        }
    }
}
impl std::error::Error for BodyMutationError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Execution(error) => Some(error),
            Self::Value(error) => Some(error),
            Self::Original(error) => Some(error.as_ref()),
            Self::Invalid(_) | Self::Rejected(_) => None,
        }
    }
}

pub use crate::incremental_value_mutation::ValueMutationContextV1 as BodyMutationContext;

/// Reusable by new-world genesis lowering and the typed prepared reducer.
/// All output is provisional until the outer transaction is accepted.
pub fn write_body_value<W: ValueNodeWriter, C: FnMut() -> bool>(
    value: &AgentBodyValueV1,
    context: &mut BodyMutationContext<'_, W, C>,
) -> Result<[u8; 32], BodyMutationError> {
    write_field_bytes(&value.canonical_bytes(), context)
}

pub fn write_field_bytes<W: ValueNodeWriter, C: FnMut() -> bool>(
    bytes: &[u8],
    context: &mut BodyMutationContext<'_, W, C>,
) -> Result<[u8; 32], BodyMutationError> {
    let mut writer = ValueStreamWriter::new(
        context.sink,
        context.memory,
        context.io,
        bytes.len() as u64,
        &mut context.cancelled,
    )?;
    if writer.write_all(bytes).is_err() {
        // finish retains the original typed IO/resource/cancellation failure.
        return Err(writer
            .finish()
            .err()
            .unwrap_or(ValueTreeError::Invalid("body value write failed"))
            .into());
    }
    Ok(writer.finish()?.digest()?)
}

/// Hash an expected value using the sole producer without publishing pages.
/// This sink never makes a durable-value or closure claim.
pub struct DiscardValueNodesV1;
impl ValueNodeWriter for DiscardValueNodesV1 {
    fn put_node(
        &mut self,
        _: [u8; 32],
        _: oasis7_proto::memory_budget::ReservedValue<Vec<u8>>,
    ) -> Result<(), ValueTreeError> {
        Ok(())
    }
}
