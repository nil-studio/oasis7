use std::net::TcpStream;

use oasis7_distfs::{
    FeedbackCreateRequest, FeedbackMutationReceipt, blake3_hex, sign_feedback_create_request,
};
use serde::{Deserialize, Serialize};

const DEFAULT_PLATFORM: &str = "client_launcher";
const DEFAULT_GAME_VERSION: &str = "unknown";
const FEEDBACK_CONTENT_LIMIT_BYTES: usize = 4 * 1024;
const FEEDBACK_TTL_MS: i64 = 7 * 24 * 60 * 60 * 1000;
const FEEDBACK_ID_HASH_PREFIX_LEN: usize = 16;
const FEEDBACK_NODE_ID_COMPONENT_MAX_LEN: usize = 24;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct FeedbackSubmitSigner {
    pub(super) private_key_hex: String,
    pub(super) public_key_hex: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub(super) struct ChainFeedbackSubmitRequest {
    pub(super) category: String,
    pub(super) title: String,
    pub(super) description: String,
    #[serde(default)]
    pub(super) platform: String,
    #[serde(default)]
    pub(super) game_version: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct ChainFeedbackSubmitResponse {
    pub(super) ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) feedback_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) event_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) audit_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) created_at_ms: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) error: Option<String>,
}

impl ChainFeedbackSubmitResponse {
    pub(super) fn success(receipt: &FeedbackMutationReceipt) -> Self {
        Self {
            ok: true,
            feedback_id: Some(receipt.feedback_id.clone()),
            event_id: Some(receipt.event_id.clone()),
            audit_id: Some(receipt.audit_id.clone()),
            created_at_ms: Some(receipt.created_at_ms),
            error: None,
        }
    }

    pub(super) fn error(message: impl Into<String>) -> Self {
        Self {
            ok: false,
            feedback_id: None,
            event_id: None,
            audit_id: None,
            created_at_ms: None,
            error: Some(message.into()),
        }
    }
}

pub(super) fn parse_feedback_submit_request(
    body: &[u8],
) -> Result<ChainFeedbackSubmitRequest, String> {
    let mut request: ChainFeedbackSubmitRequest = serde_json::from_slice(body)
        .map_err(|err| format!("invalid feedback submit payload: {err}"))?;

    request.category = request.category.trim().to_ascii_lowercase();
    if request.category != "bug" && request.category != "suggestion" {
        return Err("feedback category must be one of: bug|suggestion".to_string());
    }

    request.title = trim_non_empty(request.title.as_str(), "title")?;
    request.description = trim_non_empty(request.description.as_str(), "description")?;
    request.platform =
        trim_or_default_token(request.platform.as_str(), DEFAULT_PLATFORM, "platform")?;
    request.game_version = trim_or_default_token(
        request.game_version.as_str(),
        DEFAULT_GAME_VERSION,
        "game_version",
    )?;

    Ok(request)
}

pub(super) fn build_feedback_create_request(
    request: ChainFeedbackSubmitRequest,
    signer: &FeedbackSubmitSigner,
    node_id: &str,
    submit_ip: &str,
    now_ms: i64,
) -> Result<FeedbackCreateRequest, String> {
    let feedback_id = build_feedback_id(
        node_id,
        request.category.as_str(),
        request.title.as_str(),
        request.description.as_str(),
        now_ms,
    );
    let nonce = format!("create-{feedback_id}-{now_ms}");
    let expires_at_ms = now_ms.saturating_add(FEEDBACK_TTL_MS);

    let mut create_request = FeedbackCreateRequest {
        feedback_id,
        author_public_key_hex: signer.public_key_hex.clone(),
        submit_ip: normalize_submit_ip(submit_ip),
        category: request.category,
        platform: request.platform,
        game_version: request.game_version,
        content: build_feedback_content(request.title.as_str(), request.description.as_str()),
        attachments: Vec::new(),
        nonce,
        timestamp_ms: now_ms,
        expires_at_ms,
        signature_hex: String::new(),
    };

    create_request.signature_hex =
        sign_feedback_create_request(&create_request, signer.private_key_hex.as_str())
            .map_err(|err| format!("sign feedback create request failed: {err:?}"))?;

    Ok(create_request)
}

pub(super) fn extract_http_json_body(request_bytes: &[u8]) -> Result<&[u8], String> {
    let Some(boundary_start) = request_bytes
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
    else {
        return Err("request does not contain HTTP header terminator".to_string());
    };

    let header_bytes = &request_bytes[..boundary_start];
    let body_bytes = &request_bytes[(boundary_start + 4)..];
    let header = std::str::from_utf8(header_bytes)
        .map_err(|_| "request header is not valid UTF-8".to_string())?;

    let mut content_length = None;
    for line in header.lines().skip(1) {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        if name.trim().eq_ignore_ascii_case("content-length") {
            let parsed = value
                .trim()
                .parse::<usize>()
                .map_err(|_| "Content-Length must be a positive integer".to_string())?;
            content_length = Some(parsed);
            break;
        }
    }

    let content_length =
        content_length.ok_or_else(|| "missing Content-Length header".to_string())?;
    if body_bytes.len() < content_length {
        return Err(format!(
            "request body truncated: expected={content_length}, actual={}",
            body_bytes.len()
        ));
    }

    Ok(&body_bytes[..content_length])
}

pub(super) fn write_feedback_submit_error(
    stream: &mut TcpStream,
    status_code: u16,
    error: &str,
) -> Result<(), String> {
    let payload = ChainFeedbackSubmitResponse::error(error);
    let body = serde_json::to_vec_pretty(&payload)
        .map_err(|err| format!("failed to encode feedback submit error payload: {err}"))?;
    super::write_json_response(stream, status_code, body.as_slice(), false)
        .map_err(|err| format!("failed to write feedback submit error response: {err}"))
}

fn build_feedback_id(
    node_id: &str,
    category: &str,
    title: &str,
    description: &str,
    now_ms: i64,
) -> String {
    let node_token = sanitize_node_id_component(node_id);
    let digest = blake3_hex(
        format!(
            "node_id={node_id}|category={category}|title={title}|description={description}|now_ms={now_ms}"
        )
        .as_bytes(),
    );
    let hash_prefix = &digest[..digest.len().min(FEEDBACK_ID_HASH_PREFIX_LEN)];
    format!("{category}-{node_token}-{now_ms}-{hash_prefix}")
}

fn build_feedback_content(title: &str, description: &str) -> String {
    let header = format!("title: {}\n\n", title.trim());
    if header.len() >= FEEDBACK_CONTENT_LIMIT_BYTES {
        return truncate_utf8_by_bytes(header.as_str(), FEEDBACK_CONTENT_LIMIT_BYTES);
    }

    let remaining = FEEDBACK_CONTENT_LIMIT_BYTES - header.len();
    let body = truncate_utf8_by_bytes(description.trim(), remaining);
    format!("{header}{body}")
}

fn trim_non_empty(raw: &str, label: &str) -> Result<String, String> {
    let value = raw.trim();
    if value.is_empty() {
        return Err(format!("feedback {label} cannot be empty"));
    }
    Ok(value.to_string())
}

fn trim_or_default_token(raw: &str, default_value: &str, label: &str) -> Result<String, String> {
    let value = raw.trim();
    if value.is_empty() {
        return Ok(default_value.to_string());
    }
    if !value
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_' || byte == b'.')
    {
        return Err(format!(
            "feedback {label} must only contain ASCII letters, digits, '-', '_' or '.'"
        ));
    }
    Ok(value.to_string())
}

fn normalize_submit_ip(raw: &str) -> String {
    let value = raw.trim();
    if value.is_empty() {
        return "127.0.0.1".to_string();
    }
    value.to_string()
}

fn sanitize_node_id_component(node_id: &str) -> String {
    let mut value = String::new();
    for ch in node_id.chars() {
        if value.len() >= FEEDBACK_NODE_ID_COMPONENT_MAX_LEN {
            break;
        }
        if ch.is_ascii_alphanumeric() {
            value.push(ch.to_ascii_lowercase());
            continue;
        }
        if ch == '-' || ch == '_' {
            value.push(ch);
            continue;
        }
        if ch == '.' {
            value.push('-');
        }
    }

    let value = value.trim_matches('-').trim_matches('_').to_string();
    if value.is_empty() {
        "node".to_string()
    } else {
        value
    }
}

fn truncate_utf8_by_bytes(raw: &str, max_bytes: usize) -> String {
    if raw.len() <= max_bytes {
        return raw.to_string();
    }

    let mut result = String::new();
    let mut used = 0usize;
    for ch in raw.chars() {
        let ch_len = ch.len_utf8();
        if used + ch_len > max_bytes {
            break;
        }
        result.push(ch);
        used += ch_len;
    }
    result
}

#[cfg(test)]
mod tests {
    use super::{
        ChainFeedbackSubmitRequest, ChainFeedbackSubmitResponse, FeedbackSubmitSigner,
        build_feedback_create_request, build_feedback_id, extract_http_json_body,
        parse_feedback_submit_request,
    };
    use oasis7_distfs::{FeedbackActionKind, public_key_hex_from_signing_key_hex};

    const TEST_SIGNING_KEY_HEX: &str =
        "0101010101010101010101010101010101010101010101010101010101010101";

    #[test]
    fn parse_feedback_submit_request_rejects_invalid_category() {
        let body = br#"{"category":"question","title":"a","description":"b"}"#;
        let err = parse_feedback_submit_request(body).expect_err("invalid category should fail");
        assert!(err.contains("bug|suggestion"));
    }

    #[test]
    fn extract_http_json_body_reads_content_length_slice() {
        let request = b"POST /v1/chain/feedback/submit HTTP/1.1\r\nHost: 127.0.0.1:5121\r\nContent-Length: 17\r\n\r\n{\"ok\":true,\"n\":1}";
        let body = extract_http_json_body(request).expect("body");
        assert_eq!(body, b"{\"ok\":true,\"n\":1}");
    }

    #[test]
    fn build_feedback_create_request_generates_signed_payload() {
        let public_key_hex =
            public_key_hex_from_signing_key_hex(TEST_SIGNING_KEY_HEX).expect("derive pubkey");
        let signer = FeedbackSubmitSigner {
            private_key_hex: TEST_SIGNING_KEY_HEX.to_string(),
            public_key_hex,
        };
        let request = ChainFeedbackSubmitRequest {
            category: "bug".to_string(),
            title: "viewer freeze".to_string(),
            description: "open map then switch camera rapidly".to_string(),
            platform: "client_launcher".to_string(),
            game_version: "dev".to_string(),
        };

        let create = build_feedback_create_request(request, &signer, "node-a", "127.0.0.1", 123)
            .expect("build create request");

        assert!(create.feedback_id.starts_with("bug-node-a-123-"));
        assert_eq!(create.category, "bug");
        assert_eq!(create.platform, "client_launcher");
        assert_eq!(create.game_version, "dev");
        assert!(!create.signature_hex.is_empty());
        assert!(create.content.contains("title: viewer freeze"));
        assert!(create.content.len() <= 4 * 1024);
    }

    #[test]
    fn build_feedback_id_is_stable_for_same_input() {
        let first = build_feedback_id("node-a", "bug", "title", "desc", 42);
        let second = build_feedback_id("node-a", "bug", "title", "desc", 42);
        assert_eq!(first, second);
    }

    #[test]
    fn chain_feedback_submit_response_success_fields() {
        let receipt = oasis7_distfs::FeedbackMutationReceipt {
            feedback_id: "fb-1".to_string(),
            action: FeedbackActionKind::Create,
            event_id: "event-1".to_string(),
            audit_id: "audit-1".to_string(),
            accepted: true,
            created_at_ms: 7,
        };
        let response = ChainFeedbackSubmitResponse::success(&receipt);
        assert!(response.ok);
        assert_eq!(response.feedback_id.as_deref(), Some("fb-1"));
        assert_eq!(response.event_id.as_deref(), Some("event-1"));
        assert_eq!(response.audit_id.as_deref(), Some("audit-1"));
        assert_eq!(response.created_at_ms, Some(7));
        assert!(response.error.is_none());
    }

    #[test]
    fn chain_feedback_submit_response_error_fields() {
        let response = ChainFeedbackSubmitResponse::error("failed");
        assert!(!response.ok);
        assert!(response.feedback_id.is_none());
        assert_eq!(response.error.as_deref(), Some("failed"));
    }
}
