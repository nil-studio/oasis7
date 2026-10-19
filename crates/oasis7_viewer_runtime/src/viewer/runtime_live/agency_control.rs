use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io::Write;

pub(super) use oasis7_proto::agency_control::AGENCY_CONTROL_REQUEST_TYPE;
pub(super) const AGENCY_CONTROL_RESPONSE_TYPE: &str = "agency_control_response";

pub(super) use oasis7_proto::agency_control::{AgencyControlOperation, AgencyControlRequest};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct AgencyControlError {
    pub(super) code: String,
    pub(super) message: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct AgencyControlResponse {
    #[serde(rename = "type")]
    pub(super) frame_type: String,
    pub(super) request_id: String,
    pub(super) status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) data: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) error: Option<AgencyControlError>,
}

impl AgencyControlResponse {
    pub(super) fn success(request_id: impl Into<String>, data: Value) -> Self {
        Self {
            frame_type: AGENCY_CONTROL_RESPONSE_TYPE.to_string(),
            request_id: request_id.into(),
            status: "ok".to_string(),
            data: Some(data),
            error: None,
        }
    }

    pub(super) fn error(
        request_id: impl Into<String>,
        code: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            frame_type: AGENCY_CONTROL_RESPONSE_TYPE.to_string(),
            request_id: request_id.into(),
            status: "error".to_string(),
            data: None,
            error: Some(AgencyControlError {
                code: code.into(),
                message: message.into(),
            }),
        }
    }
}

pub(super) enum ParsedAgencyControlFrame {
    NotAgencyControl,
    Request(AgencyControlRequest),
    Invalid(AgencyControlResponse),
}

pub(super) fn parse_agency_control_frame(raw: &str) -> ParsedAgencyControlFrame {
    let Ok(value) = serde_json::from_str::<Value>(raw) else {
        return ParsedAgencyControlFrame::NotAgencyControl;
    };
    if value.get("type").and_then(Value::as_str) != Some(AGENCY_CONTROL_REQUEST_TYPE) {
        return ParsedAgencyControlFrame::NotAgencyControl;
    }
    let request_id = value
        .get("request_id")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    match serde_json::from_value::<AgencyControlRequest>(value) {
        Ok(request) => ParsedAgencyControlFrame::Request(request),
        Err(error) => ParsedAgencyControlFrame::Invalid(AgencyControlResponse::error(
            request_id,
            "invalid_request",
            error.to_string(),
        )),
    }
}

pub(super) fn write_agency_control_response(
    writer: &mut impl Write,
    response: &AgencyControlResponse,
) -> std::io::Result<()> {
    serde_json::to_writer(&mut *writer, response).map_err(std::io::Error::other)?;
    writer.write_all(b"\n")?;
    writer.flush()
}
