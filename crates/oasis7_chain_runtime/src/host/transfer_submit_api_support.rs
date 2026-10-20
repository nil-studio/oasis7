use super::*;

pub(super) fn parse_http_target(request_bytes: &[u8]) -> Result<String, String> {
    let request = std::str::from_utf8(request_bytes)
        .map_err(|_| "invalid HTTP request: non UTF-8 payload".to_string())?;
    let line = request
        .lines()
        .next()
        .ok_or_else(|| "invalid HTTP request: missing request line".to_string())?;
    let target = line
        .split_whitespace()
        .nth(1)
        .ok_or_else(|| "invalid HTTP request: missing request target".to_string())?;
    Ok(target.to_string())
}

pub(super) fn parse_query_params(target: &str) -> BTreeMap<String, String> {
    let mut params = BTreeMap::new();
    let query = match target.split_once('?') {
        Some((_, query)) => query,
        None => return params,
    };

    for pair in query.split('&') {
        if pair.is_empty() {
            continue;
        }
        let (key, value) = match pair.split_once('=') {
            Some((key, value)) => (key, value),
            None => (pair, ""),
        };
        let key = percent_decode(key);
        let value = percent_decode(value);
        if key.is_empty() {
            continue;
        }
        params.insert(key, value);
    }
    params
}

pub(super) fn parse_transfer_lifecycle_status(raw: &str) -> Option<TransferLifecycleStatus> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "accepted" => Some(TransferLifecycleStatus::Accepted),
        "pending" => Some(TransferLifecycleStatus::Pending),
        "confirmed" => Some(TransferLifecycleStatus::Confirmed),
        "failed" => Some(TransferLifecycleStatus::Failed),
        "timeout" => Some(TransferLifecycleStatus::Timeout),
        _ => None,
    }
}

fn percent_decode(raw: &str) -> String {
    let bytes = raw.as_bytes();
    let mut cursor = 0_usize;
    let mut output = Vec::with_capacity(bytes.len());

    while cursor < bytes.len() {
        let byte = bytes[cursor];
        if byte == b'+' {
            output.push(b' ');
            cursor += 1;
            continue;
        }
        if byte == b'%' && cursor + 2 < bytes.len() {
            let high = hex_value(bytes[cursor + 1]);
            let low = hex_value(bytes[cursor + 2]);
            if let (Some(high), Some(low)) = (high, low) {
                output.push((high << 4) | low);
                cursor += 3;
                continue;
            }
        }
        output.push(byte);
        cursor += 1;
    }

    String::from_utf8(output).unwrap_or_else(|_| raw.to_string())
}

fn hex_value(raw: u8) -> Option<u8> {
    match raw {
        b'0'..=b'9' => Some(raw - b'0'),
        b'a'..=b'f' => Some(raw - b'a' + 10),
        b'A'..=b'F' => Some(raw - b'A' + 10),
        _ => None,
    }
}

pub(super) fn normalize_required_field(raw: &str, label: &str) -> Result<String, String> {
    let value = raw.trim();
    if value.is_empty() {
        return Err(format!("{label} is empty"));
    }
    Ok(value.to_string())
}

pub(super) fn normalize_public_key_field(raw: &str, label: &str) -> Result<String, String> {
    let normalized = normalize_required_field(raw, label)?;
    let bytes = decode_hex_array::<32>(normalized.as_str(), label)?;
    Ok(hex::encode(bytes))
}

pub(super) fn normalize_signature_field(raw: &str, label: &str) -> Result<String, String> {
    normalize_required_field(raw, label)
}

fn decode_hex_array<const N: usize>(raw: &str, label: &str) -> Result<[u8; N], String> {
    let bytes = hex::decode(raw).map_err(|err| format!("decode {label} failed: {err}"))?;
    if bytes.len() != N {
        return Err(format!(
            "{label} length mismatch: expected {N} bytes, got {}",
            bytes.len()
        ));
    }
    let mut fixed = [0_u8; N];
    fixed.copy_from_slice(bytes.as_slice());
    Ok(fixed)
}

pub(super) fn normalize_account_id(raw: &str, field: &str) -> Result<String, String> {
    let account_id = raw.trim();
    if account_id.is_empty() {
        return Err(format!("transfer {field} cannot be empty"));
    }
    if account_id.len() > ACCOUNT_ID_MAX_LEN {
        return Err(format!(
            "transfer {field} exceeds max length {ACCOUNT_ID_MAX_LEN}"
        ));
    }
    if !account_id.bytes().all(is_allowed_account_id_byte) {
        return Err(format!(
            "transfer {field} must only contain ASCII letters, digits, ':', '-', '_' or '.'"
        ));
    }
    Ok(account_id.to_string())
}

pub(super) fn is_allowed_account_id_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b':' | b'-' | b'_' | b'.')
}

pub(super) fn next_transfer_action_id() -> Result<u64, String> {
    let action_id = NEXT_TRANSFER_ACTION_ID.fetch_add(1, Ordering::Relaxed);
    if action_id == 0 {
        return Err("transfer action id allocator exhausted".to_string());
    }
    Ok(action_id)
}

pub(super) fn build_transfer_submit_action_payload(
    request: &ChainTransferSubmitRequest,
) -> Result<Vec<u8>, String> {
    let action = build_transfer_submit_action(request);
    let envelope = ConsensusActionPayloadEnvelope::from_runtime_action_with_auth(
        action,
        ConsensusActionAuthEnvelope::MainTokenAction(build_transfer_submit_auth_proof(request)),
    );
    encode_consensus_action_payload(&envelope)
}

pub(super) fn build_transfer_submit_action(request: &ChainTransferSubmitRequest) -> Action {
    Action::TransferMainToken {
        from_account_id: request.from_account_id.clone(),
        to_account_id: request.to_account_id.clone(),
        amount: request.amount,
        nonce: request.nonce,
        asset_id: request.asset_id.clone(),
        memo: request.memo.clone(),
        chain_id: request.chain_id.clone(),
        network_id: request.network_id.clone(),
        tx_version: request.tx_version,
        tx_type: request.tx_type.clone(),
        valid_until_unix_ms: request.valid_until_unix_ms,
        max_fee: request.max_fee,
        fee_asset_id: request.fee_asset_id.clone(),
        application_payload_hash: request.application_payload_hash.clone(),
        client_request_id: request.client_request_id.clone(),
    }
}

pub(super) fn build_transfer_submit_auth_proof(
    request: &ChainTransferSubmitRequest,
) -> MainTokenActionAuthProof {
    MainTokenActionAuthProof {
        scheme: MainTokenActionAuthScheme::Ed25519,
        account_id: request.from_account_id.clone(),
        public_key: Some(request.public_key.clone()),
        signature: Some(request.signature.clone()),
        threshold: None,
        participant_signatures: Vec::new(),
    }
}

pub(super) fn map_transfer_auth_error(error: MainTokenActionAuthError) -> (String, String) {
    match error {
        MainTokenActionAuthError::InvalidSignature(message) => {
            (TRANSFER_ERROR_INVALID_SIGNATURE.to_string(), message)
        }
        MainTokenActionAuthError::AccountMismatch(message) => {
            (TRANSFER_ERROR_ACCOUNT_AUTH_MISMATCH.to_string(), message)
        }
        MainTokenActionAuthError::InvalidRequest(message)
        | MainTokenActionAuthError::UnsupportedAction(message) => {
            (TRANSFER_ERROR_INVALID_REQUEST.to_string(), message)
        }
    }
}

pub(super) fn write_transfer_submit_error(
    stream: &mut TcpStream,
    status_code: u16,
    error_code: &str,
    error: &str,
) -> Result<(), String> {
    let payload = ChainTransferSubmitResponse::error(error_code, error);
    let body = serde_json::to_vec_pretty(&payload)
        .map_err(|err| format!("failed to encode transfer submit error payload: {err}"))?;
    super::super::write_json_response(stream, status_code, body.as_slice(), false)
        .map_err(|err| format!("failed to write transfer submit error response: {err}"))
}

pub(super) fn lock_transfer_tracker() -> std::sync::MutexGuard<'static, TransferTracker> {
    let tracker = TRANSFER_TRACKER.get_or_init(|| Mutex::new(TransferTracker::default()));
    tracker
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

pub(super) fn with_transfer_tracker<T>(f: impl FnOnce(&mut TransferTracker) -> T) -> T {
    let mut tracker = lock_transfer_tracker();
    f(&mut tracker)
}
