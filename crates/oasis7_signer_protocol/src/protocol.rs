use std::fmt;
use std::io::{self, Read, Write};

use base64::Engine;
use serde::{Deserialize, Serialize};

pub const IPC_SCHEMA: &str = "oasis7.local_signer_ipc.v1";
pub const RESULT_SCHEMA: &str = "oasis7.local_signer_result.v1";
pub const MAX_FRAME_BYTES: usize = 24 * 1024 * 1024;
pub const MAX_METADATA_BYTES: usize = 64 * 1024;
pub const MAX_ROLLBACK_PAYLOAD_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_IDENTITY_PAYLOAD_BYTES: usize = 1024 * 1024;

#[derive(Debug)]
pub enum ProtocolError {
    Io(io::Error),
    Json(serde_json::Error),
    Invalid(&'static str),
    InvalidOwned(String),
    Oversized,
    TrailingData,
}

impl fmt::Display for ProtocolError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "IPC I/O failed: {error}"),
            Self::Json(error) => write!(formatter, "invalid IPC JSON: {error}"),
            Self::Invalid(reason) => write!(formatter, "invalid IPC request: {reason}"),
            Self::InvalidOwned(reason) => write!(formatter, "invalid IPC request: {reason}"),
            Self::Oversized => formatter.write_str("IPC frame exceeds its size limit"),
            Self::TrailingData => formatter.write_str("IPC input contains trailing bytes"),
        }
    }
}

impl std::error::Error for ProtocolError {}

impl From<io::Error> for ProtocolError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<serde_json::Error> for ProtocolError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}

/// Represents a required JSON field whose value may be either null or a value.
///
/// Unlike `Option<T>`, the wrapper makes an omitted field a deserialization error.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ExplicitNull<T>(pub Option<T>);

impl<T> From<Option<T>> for ExplicitNull<T> {
    fn from(value: Option<T>) -> Self {
        Self(value)
    }
}

/// The flat tagged variants mirror the one-frame IPC JSON shape.
///
/// Boxing one nested value only to reduce this transient enum's stack size
/// would complicate its public construction API without changing wire size.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case", deny_unknown_fields)]
pub enum IpcRequest {
    Doctor {
        schema_version: String,
        installation_id: String,
    },
    Inspect {
        schema_version: String,
        installation_id: String,
        request_id: String,
        purpose: String,
        provider_id: ExplicitNull<String>,
    },
    Sign {
        schema_version: String,
        installation_id: String,
        request_id: String,
        purpose: String,
        provider_id: ExplicitNull<String>,
        signer_id: String,
        grant_id: ExplicitNull<String>,
        context: SignContext,
        payload_base64: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignContext {
    pub deployment_id: String,
    pub network_id: String,
    pub task_uid: String,
    pub source_head_oid: String,
    pub protocol_context: RollbackProtocolContext,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RollbackProtocolContext {
    pub authority_id: String,
    pub rollback_ticket: String,
    pub receipt_id: String,
    pub nonce: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case", deny_unknown_fields)]
pub enum IpcResponse {
    Doctor(DoctorResponse),
    Inspect(InspectResponse),
    Sign(SignResult),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DoctorResponse {
    pub schema_version: String,
    pub installation_id: String,
    pub ready: bool,
    pub status_code: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InspectResponse {
    pub schema_version: String,
    pub request_id: String,
    pub operation_key: ExplicitNull<String>,
    pub status: String,
    pub payload_sha256: ExplicitNull<String>,
    pub audit_digest: ExplicitNull<String>,
    pub error_code: ExplicitNull<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignResult {
    pub schema_version: String,
    pub request_id: String,
    pub operation_key: ExplicitNull<String>,
    pub status: String,
    pub public_key_base64: ExplicitNull<String>,
    pub signature_base64: ExplicitNull<String>,
    pub provider_attestation: ExplicitNull<serde_json::Value>,
    pub audit_digest: ExplicitNull<String>,
    pub error_code: ExplicitNull<String>,
}

impl IpcRequest {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        match self {
            Self::Doctor {
                schema_version,
                installation_id,
            } => {
                check_schema(schema_version)?;
                validate_id(installation_id)?;
            }
            Self::Inspect {
                schema_version,
                installation_id,
                request_id,
                purpose,
                provider_id,
            } => {
                check_schema(schema_version)?;
                validate_id(installation_id)?;
                validate_id(request_id)?;
                validate_id(purpose)?;
                validate_optional_id(provider_id.0.as_deref())?;
            }
            Self::Sign {
                schema_version,
                installation_id,
                request_id,
                purpose,
                provider_id,
                signer_id,
                grant_id,
                context,
                payload_base64,
            } => {
                check_schema(schema_version)?;
                validate_id(installation_id)?;
                validate_id(request_id)?;
                validate_id(purpose)?;
                validate_id(signer_id)?;
                validate_optional_id(provider_id.0.as_deref())?;
                validate_optional_id(grant_id.0.as_deref())?;
                if provider_id.0.is_some() {
                    return Err(ProtocolError::Invalid(
                        "local signing provider_id must be null",
                    ));
                }
                if !matches!(
                    purpose.as_str(),
                    "rollback_strict_audit_v1" | "file_ed25519_v1"
                ) {
                    return Err(ProtocolError::Invalid("unsupported local signing purpose"));
                }
                if grant_id.0.is_none() {
                    return Err(ProtocolError::Invalid("grant_id must not be null"));
                }
                validate_context(context)?;
                decode_payload(payload_base64)?;
            }
        }
        Ok(())
    }

    fn payload_base64(&self) -> Option<&str> {
        match self {
            Self::Sign { payload_base64, .. } => Some(payload_base64),
            Self::Doctor { .. } | Self::Inspect { .. } => None,
        }
    }
}

pub fn validate_id(value: &str) -> Result<(), ProtocolError> {
    let bytes = value.as_bytes();
    if !(2..=128).contains(&bytes.len()) {
        return Err(ProtocolError::Invalid(
            "identifier length must be between 2 and 128 bytes",
        ));
    }
    if !bytes[0].is_ascii_alphanumeric()
        || !bytes
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
        || value == "."
        || value == ".."
    {
        return Err(ProtocolError::Invalid(
            "identifier must use the bounded ASCII identifier alphabet",
        ));
    }
    Ok(())
}

pub fn decode_payload(payload_base64: &str) -> Result<Vec<u8>, ProtocolError> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(payload_base64)
        .map_err(|_| ProtocolError::Invalid("payload_base64 is malformed"))?;
    if base64::engine::general_purpose::STANDARD.encode(&bytes) != payload_base64 {
        return Err(ProtocolError::Invalid(
            "payload_base64 must use canonical padded standard encoding",
        ));
    }
    if bytes.is_empty() {
        return Err(ProtocolError::Invalid("payload must not be empty"));
    }
    if bytes.len() > MAX_ROLLBACK_PAYLOAD_BYTES {
        return Err(ProtocolError::Oversized);
    }
    Ok(bytes)
}

pub fn encode_request_frame(request: &IpcRequest) -> Result<Vec<u8>, ProtocolError> {
    request.validate()?;
    let body = serde_json::to_vec(request)?;
    validate_frame_body_size(&body, request.payload_base64())?;
    let length = u32::try_from(body.len()).map_err(|_| ProtocolError::Oversized)?;
    let mut frame = Vec::with_capacity(body.len() + 4);
    frame.extend_from_slice(&length.to_be_bytes());
    frame.extend_from_slice(&body);
    Ok(frame)
}

pub fn read_request_frame<R: Read>(reader: &mut R) -> Result<IpcRequest, ProtocolError> {
    let mut header = [0; 4];
    reader.read_exact(&mut header)?;
    let body_length = u32::from_be_bytes(header) as usize;
    if body_length == 0 || body_length > MAX_FRAME_BYTES {
        return Err(ProtocolError::Oversized);
    }
    let mut body = vec![0; body_length];
    reader.read_exact(&mut body)?;
    let mut trailing = [0; 1];
    if reader.read(&mut trailing)? != 0 {
        return Err(ProtocolError::TrailingData);
    }
    let request: IpcRequest = serde_json::from_slice(&body)?;
    request.validate()?;
    validate_frame_body_size(&body, request.payload_base64())?;
    Ok(request)
}

pub fn write_response_frame<W: Write>(
    writer: &mut W,
    response: &IpcResponse,
) -> Result<(), ProtocolError> {
    let frame = encode_response_frame(response)?;
    writer.write_all(&frame)?;
    writer.flush()?;
    Ok(())
}

pub fn encode_response_frame(response: &IpcResponse) -> Result<Vec<u8>, ProtocolError> {
    let body = serde_json::to_vec(response)?;
    if body.is_empty() || body.len() > MAX_FRAME_BYTES {
        return Err(ProtocolError::Oversized);
    }
    let length = u32::try_from(body.len()).map_err(|_| ProtocolError::Oversized)?;
    let mut frame = Vec::with_capacity(body.len() + 4);
    frame.extend_from_slice(&length.to_be_bytes());
    frame.extend_from_slice(&body);
    Ok(frame)
}

pub fn read_response_frame<R: Read>(reader: &mut R) -> Result<IpcResponse, ProtocolError> {
    let body = read_frame_body(reader)?;
    Ok(serde_json::from_slice(&body)?)
}

fn read_frame_body<R: Read>(reader: &mut R) -> Result<Vec<u8>, ProtocolError> {
    let mut header = [0; 4];
    reader.read_exact(&mut header)?;
    let body_length = u32::from_be_bytes(header) as usize;
    if body_length == 0 || body_length > MAX_FRAME_BYTES {
        return Err(ProtocolError::Oversized);
    }
    let mut body = vec![0; body_length];
    reader.read_exact(&mut body)?;
    let mut trailing = [0; 1];
    if reader.read(&mut trailing)? != 0 {
        return Err(ProtocolError::TrailingData);
    }
    Ok(body)
}

fn validate_frame_body_size(
    body: &[u8],
    payload_base64: Option<&str>,
) -> Result<(), ProtocolError> {
    if body.is_empty() || body.len() > MAX_FRAME_BYTES {
        return Err(ProtocolError::Oversized);
    }
    let metadata_length = body.len() - payload_base64.map_or(0, str::len);
    if metadata_length > MAX_METADATA_BYTES {
        return Err(ProtocolError::Oversized);
    }
    Ok(())
}

fn check_schema(schema_version: &str) -> Result<(), ProtocolError> {
    if schema_version != IPC_SCHEMA {
        return Err(ProtocolError::Invalid("unsupported schema_version"));
    }
    Ok(())
}

pub(crate) fn validate_context(context: &SignContext) -> Result<(), ProtocolError> {
    for (label, value) in [
        ("deployment_id", context.deployment_id.as_str()),
        ("network_id", context.network_id.as_str()),
        ("task_uid", context.task_uid.as_str()),
    ] {
        if value.trim().is_empty() || value.len() > 128 {
            return Err(ProtocolError::InvalidOwned(format!(
                "{label} must be non-empty and at most 128 bytes"
            )));
        }
    }
    if context.source_head_oid.len() != 40
        || !context
            .source_head_oid
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(ProtocolError::Invalid(
            "source_head_oid must be 40 lowercase hexadecimal characters",
        ));
    }
    for (label, value) in [
        (
            "authority_id",
            context.protocol_context.authority_id.as_str(),
        ),
        (
            "rollback_ticket",
            context.protocol_context.rollback_ticket.as_str(),
        ),
        ("receipt_id", context.protocol_context.receipt_id.as_str()),
        ("nonce", context.protocol_context.nonce.as_str()),
    ] {
        if value.trim().is_empty() || value.len() > 128 {
            return Err(ProtocolError::InvalidOwned(format!(
                "{label} must be non-empty and at most 128 bytes"
            )));
        }
    }
    Ok(())
}

fn validate_optional_id(value: Option<&str>) -> Result<(), ProtocolError> {
    if let Some(value) = value {
        validate_id(value)?;
    }
    Ok(())
}
