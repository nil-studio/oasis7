//! Concrete Viewer JSON adaptation. Callers own visibility, framing and transport.
use std::io::Write;

use oasis7::simulator::{AgentDecisionTrace, RunnerMetrics, WorldEvent, WorldSnapshot};
use oasis7_proto::viewer as proto;

pub use oasis7_client_api::viewer_protocol::ViewerRequest;
pub type ViewerResponse = proto::ViewerResponse<
    WorldSnapshot,
    WorldEvent,
    AgentDecisionTrace,
    RunnerMetrics,
    oasis7::simulator::WorldTime,
>;

// Both capacities use the same fixed serializer instance. Taking the buffer
// apart before propagating errors prevents BufWriter's Drop from retrying IO.
fn encode_buffered_response(
    writer: &mut dyn Write,
    response: &ViewerResponse,
    capacity: usize,
) -> Result<Vec<u8>, serde_json::Error> {
    let mut buffered = std::io::BufWriter::with_capacity(capacity, writer);
    let result = serde_json::to_writer(&mut buffered, response);
    let (_, pending) = buffered.into_parts();
    result?;
    pending.map_err(|error| serde_json::Error::io(std::io::Error::other(error)))
}

/// Encode through an 8 KiB buffer for transports with a concrete buffered writer.
/// The caller still owns framing and transport flushing. Runtime Outbox callers
/// must keep using the direct streaming entry.
///
/// # Errors
/// Returns serialization or writer errors. On a returned error, pending buffered
/// bytes are discarded without retrying the writer or flushing the transport.
pub fn write_buffered_response_json(
    writer: &mut dyn Write,
    response: &ViewerResponse,
) -> Result<(), serde_json::Error> {
    let pending = encode_buffered_response(writer, response, 8192)?;
    if !pending.is_empty() {
        writer.write_all(&pending).map_err(serde_json::Error::io)?;
    }
    Ok(())
}

/// Encode one selected response without framing or flushing.
///
/// # Errors
/// Returns serialization or writer errors without buffering the full response.
pub fn write_response_json(
    writer: &mut dyn Write,
    response: &ViewerResponse,
) -> Result<(), serde_json::Error> {
    serde_json::to_writer(writer, response)
}

#[derive(serde::Serialize)]
struct CorrelatedResponse<'a> {
    #[serde(flatten)]
    response: &'a ViewerResponse,
    #[serde(skip_serializing_if = "Option::is_none")]
    request_auth_nonce: Option<u64>,
}

/// Encode additive correlation metadata. The nonce is not authorization proof.
///
/// # Errors
/// Returns serialization or writer errors without committing a transport frame.
pub fn write_correlated_response_json(
    writer: &mut dyn Write,
    response: &ViewerResponse,
    request_auth_nonce: Option<u64>,
) -> Result<(), serde_json::Error> {
    serde_json::to_writer(
        writer,
        &CorrelatedResponse {
            response,
            request_auth_nonce,
        },
    )
}

/// Encode a concrete response for transports that already require a string.
///
/// # Errors
/// Returns any JSON encoding error.
///
/// # Panics
/// Panics if the JSON serializer emits invalid UTF-8 (which `serde_json` guarantees against).
pub fn response_to_json(response: &ViewerResponse) -> Result<String, serde_json::Error> {
    let mut bytes = Vec::new();
    // Reuse the pending 1 KiB allocation for small responses; larger responses
    // are emitted incrementally into bytes, with only this bounded extra buffer.
    let pending = encode_buffered_response(&mut bytes, response, 1024)?;
    if bytes.is_empty() {
        bytes = pending;
    } else {
        bytes.extend_from_slice(&pending);
    }
    // serde_json always emits UTF-8, just as serde_json::to_string does.
    Ok(String::from_utf8(bytes).expect("JSON encoding is UTF-8"))
}

/// Parse the existing request contract; callers retain their own trim/read limits.
///
/// # Errors
/// Returns the original JSON syntax or request contract error.
pub fn request_from_json(json: &str) -> Result<ViewerRequest, serde_json::Error> {
    serde_json::from_str(json)
}

#[cfg(test)]
mod tests;
