use super::*;
use std::collections::BTreeMap;
pub(super) fn validate_origin(origin: &str) -> Result<(), HttpPopError> {
    let authority = origin
        .strip_prefix("https://")
        .ok_or(HttpPopError::Target)?;
    if origin.len() > 512
        || authority.is_empty()
        || !authority.is_ascii()
        || authority.contains(['/', '?', '#', '@', '\\'])
    {
        return Err(HttpPopError::Target);
    }
    let (host, port) = if let Some((h, p)) = authority.rsplit_once(':') {
        (h, Some(p))
    } else {
        (authority, None)
    };
    if host.len() > 253
        || !host.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        })
    {
        return Err(HttpPopError::Target);
    }
    if let Some(p) = port {
        let n = p.parse::<u16>().map_err(|_| HttpPopError::Target)?;
        if n == 0 || n == 443 || n.to_string() != p {
            return Err(HttpPopError::Target);
        }
    }
    Ok(())
}
pub(super) fn validate_profile(
    profile: HttpPopProfileV1,
    target: &CanonicalHttpTarget,
    binding_id: &str,
) -> Result<(), HttpPopError> {
    let valid = match profile {
        HttpPopProfileV1::Mcp => matches!(target.route, HttpPopRoute::Mcp),
        HttpPopProfileV1::Custody => matches!(
            target.route,
            HttpPopRoute::Actions
                | HttpPopRoute::Operation(_)
                | HttpPopRoute::TestSession(_)
                | HttpPopRoute::Heartbeat(_)
                | HttpPopRoute::Finish(_)
                | HttpPopRoute::Abort(_)
        ),
        HttpPopProfileV1::EnrollmentProof => {
            matches!(&target.route,HttpPopRoute::EnrollmentProve(id) if id==binding_id)
        }
        HttpPopProfileV1::CredentialClaim => {
            matches!(target.route, HttpPopRoute::EnrollmentClaim(_))
        }
        _ => matches!(target.route, HttpPopRoute::ProofChallenges),
    };
    if !valid || !identifier(binding_id) {
        return Err(HttpPopError::Binding);
    }
    if matches!(&target.route,HttpPopRoute::TestSession(id)|HttpPopRoute::Heartbeat(id)|HttpPopRoute::Finish(id)|HttpPopRoute::Abort(id) if id!=binding_id)
    {
        return Err(HttpPopError::Binding);
    }
    Ok(())
}
pub(super) fn validate_params(
    profile: HttpPopProfileV1,
    key: &str,
    binding: &str,
    request: &str,
    nonce: Option<&str>,
    created: UnixSeconds,
    expires: UnixSeconds,
) -> Result<(), HttpPopError> {
    if !key_id(key)
        || !identifier(binding)
        || !identifier(request)
        || profile.is_challenge() != nonce.is_none()
        || nonce.is_some_and(|s| !nonce_valid(s))
    {
        return Err(HttpPopError::Binding);
    }
    if created.get() > 999_999_999_999_999
        || expires.get() > 999_999_999_999_999
        || expires.get() <= created.get()
        || expires.get() - created.get() > 30
    {
        return Err(HttpPopError::Time);
    }
    Ok(())
}
pub(super) fn validate_time(
    created: UnixSeconds,
    expires: UnixSeconds,
    now: UnixMillis,
    deadline: UnixMillis,
) -> Result<(), HttpPopError> {
    if created.get()
        > now
            .floor_seconds()
            .get()
            .checked_add(5)
            .ok_or(HttpPopError::Time)?
        || now.floor_seconds().get() >= expires.get()
        || expires
            .checked_millis()
            .map_err(|_| HttpPopError::Time)?
            .get()
            > deadline.get()
        || now.get() >= deadline.get()
    {
        return Err(HttpPopError::Time);
    }
    Ok(())
}
pub(super) fn headers<'a>(
    message: &HttpPopMessage<'a>,
) -> Result<BTreeMap<String, &'a str>, HttpPopError> {
    if message.body.len() > MAX_HTTP_BODY_BYTES || message.headers.len() > MAX_HTTP_HEADER_INSTANCES
    {
        return Err(HttpPopError::Bounds);
    }
    let mut result = BTreeMap::new();
    let mut total = 0usize;
    for h in message.headers {
        total = total
            .checked_add(h.name.len())
            .and_then(|v| v.checked_add(h.value.len()))
            .and_then(|v| v.checked_add(4))
            .ok_or(HttpPopError::Bounds)?;
        if total > MAX_HTTP_HEADER_BYTES
            || h.value.len() > MAX_HTTP_HEADER_VALUE_BYTES
            || h.name.is_empty()
            || h.name.len() > 128
            || !h
                .name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&b))
            || !h.value.bytes().all(|b| (0x20..=0x7e).contains(&b))
            || h.value.trim() != h.value
        {
            return Err(HttpPopError::Header);
        }
        let name = h.name.to_ascii_lowercase();
        if result.insert(name.clone(), h.value).is_some() {
            return Err(HttpPopError::Header);
        }
        if matches!(
            name.as_str(),
            "content-encoding"
                | "trailer"
                | "transfer-encoding"
                | "forwarded"
                | "x-forwarded-host"
                | "x-original-url"
                | "x-rewrite-url"
        ) {
            return Err(HttpPopError::Header);
        }
        if message.representation == HttpHeaderRepresentation::WorkerMergedSingletons
            && singleton(&name)
            && h.value.contains(',')
        {
            return Err(HttpPopError::Header);
        }
    }
    Ok(result)
}
fn singleton(name: &str) -> bool {
    matches!(
        name,
        "signature"
            | "signature-input"
            | "content-type"
            | "content-digest"
            | "content-length"
            | "host"
            | "authorization"
            | "oasis7-capability"
            | "oasis7-token-digest"
            | "oasis7-test-session"
            | "oasis7-enrollment"
            | "oasis7-request-id"
            | "mcp-protocol-version"
    )
}
pub(super) fn verify_ed25519(proof: &PreparedHttpPop) -> Result<(), HttpPopError> {
    if proof.public_key_hex.len() != 64
        || !proof
            .public_key_hex
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(HttpPopError::Signature);
    }
    let mut bytes = [0u8; 32];
    for (i, pair) in proof.public_key_hex.as_bytes().chunks_exact(2).enumerate() {
        bytes[i] = u8::from_str_radix(
            std::str::from_utf8(pair).map_err(|_| HttpPopError::Signature)?,
            16,
        )
        .map_err(|_| HttpPopError::Signature)?;
    }
    let key =
        ed25519_dalek::VerifyingKey::from_bytes(&bytes).map_err(|_| HttpPopError::Signature)?;
    if key.is_weak() {
        return Err(HttpPopError::Signature);
    }
    key.verify_strict(
        &proof.base,
        &ed25519_dalek::Signature::from_bytes(&proof.signature),
    )
    .map_err(|_| HttpPopError::Signature)
}
