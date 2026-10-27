use super::*;
use std::collections::BTreeMap;
fn components(profile: HttpPopProfileV1, method: &str) -> Vec<&'static str> {
    let mut fields = vec!["@method", "@target-uri"];
    if method == "POST" {
        fields.push("content-type");
    }
    fields.extend([
        "content-digest",
        "oasis7-token-digest",
        profile.binding_header(),
        "oasis7-request-id",
    ]);
    if profile.is_mcp() {
        fields.push("mcp-protocol-version");
    }
    fields
}
fn parameters(
    profile: HttpPopProfileV1,
    method: &str,
    key: &str,
    nonce: Option<&str>,
    created: UnixSeconds,
    expires: UnixSeconds,
) -> String {
    let fields = components(profile, method)
        .into_iter()
        .map(|s| format!("\"{s}\""))
        .collect::<Vec<_>>()
        .join(" ");
    let nonce = nonce.map_or_else(String::new, |n| format!(";nonce=\"{n}\""));
    format!(
        "({fields});created={};expires={}{nonce};alg=\"ed25519\";keyid=\"{key}\";tag=\"{}\"",
        created.get(),
        expires.get(),
        profile.tag()
    )
}
fn base(
    profile: HttpPopProfileV1,
    method: &str,
    uri: &str,
    params: &str,
    headers: &BTreeMap<String, &str>,
) -> Result<Vec<u8>, HttpPopError> {
    let mut lines = Vec::new();
    for field in components(profile, method) {
        let value = match field {
            "@method" => method,
            "@target-uri" => uri,
            _ => headers.get(field).copied().ok_or(HttpPopError::Header)?,
        };
        lines.push(format!("\"{field}\": {value}"));
    }
    lines.push(format!("\"@signature-params\": {params}"));
    Ok(lines.join("\n").into_bytes())
}
fn token_hash(profile: HttpPopProfileV1, token: Option<&str>) -> Result<[u8; 32], HttpPopError> {
    match token {
        Some(_) if profile == HttpPopProfileV1::EnrollmentProof => Err(HttpPopError::Binding),
        Some(t) => {
            if !profile.is_mcp()
                && !URL_SAFE_NO_PAD
                    .decode(t)
                    .is_ok_and(|v| v.len() == 32 && URL_SAFE_NO_PAD.encode(v) == t)
            {
                return Err(HttpPopError::Binding);
            }
            token_digest(t)
        }
        None if profile.empty_token() => Ok(sha256(b"")),
        _ => Err(HttpPopError::Binding),
    }
}
/// Deterministic RFC base and headers only. The caller retains its private key
/// and directly signs these bytes using Ed25519, without another pre-hash.
pub fn build_http_pop_signing_material(
    input: &HttpPopSigningInput<'_>,
) -> Result<HttpPopSigningMaterial, HttpPopError> {
    validate_profile(input.profile, input.target, input.binding_id)?;
    validate_params(
        input.profile,
        input.key_id,
        input.binding_id,
        input.request_id,
        input.nonce,
        input.created,
        input.expires,
    )?;
    if input.body.len() > MAX_HTTP_BODY_BYTES
        || input.target.method() == "GET" && !input.body.is_empty()
    {
        return Err(HttpPopError::Bounds);
    }
    let token = token_hash(input.profile, input.token)?;
    let params = parameters(
        input.profile,
        input.target.method(),
        input.key_id,
        input.nonce,
        input.created,
        input.expires,
    );
    let mut h = vec![];
    if input.target.method() == "POST" {
        h.push(OwnedHttpHeader {
            name: "content-type".into(),
            value: "application/json".into(),
        });
    }
    for (name, value) in [
        ("content-digest", content_digest(input.body)),
        (
            "oasis7-token-digest",
            format!(":{}:", STANDARD.encode(token)),
        ),
        (input.profile.binding_header(), input.binding_id.into()),
        ("oasis7-request-id", input.request_id.into()),
        ("signature-input", format!("oasis7={params}")),
    ] {
        h.push(OwnedHttpHeader {
            name: name.into(),
            value,
        });
    }
    if input.profile.is_mcp() {
        h.push(OwnedHttpHeader {
            name: "mcp-protocol-version".into(),
            value: MCP_PROTOCOL_VERSION.into(),
        });
    }
    if let Some(t) = input.token {
        h.push(OwnedHttpHeader {
            name: if input.profile.is_mcp() {
                "authorization"
            } else {
                "oasis7-capability"
            }
            .into(),
            value: if input.profile.is_mcp() {
                format!("Bearer {t}")
            } else {
                t.into()
            },
        });
    }
    let map = h
        .iter()
        .map(|h| (h.name.clone(), h.value.as_str()))
        .collect();
    let base = base(
        input.profile,
        input.target.method(),
        input.target.uri(),
        &params,
        &map,
    )?;
    Ok(HttpPopSigningMaterial { base, headers: h })
}
fn integer(s: &str) -> Result<UnixSeconds, HttpPopError> {
    let v = s.parse::<u64>().map_err(|_| HttpPopError::Time)?;
    if v.to_string() != s {
        return Err(HttpPopError::Time);
    }
    Ok(UnixSeconds::new(v))
}
fn time_parameters(
    signature_input: &str,
    profile: HttpPopProfileV1,
    method: &str,
) -> Result<(UnixSeconds, UnixSeconds), HttpPopError> {
    let quoted = components(profile, method)
        .into_iter()
        .map(|s| format!("\"{s}\""))
        .collect::<Vec<_>>()
        .join(" ");
    let prefix = format!("oasis7=({quoted});created=");
    let rest = signature_input
        .strip_prefix(&prefix)
        .ok_or(HttpPopError::Header)?;
    let (c, rest) = rest.split_once(";expires=").ok_or(HttpPopError::Header)?;
    let (e, _) = rest.split_once(';').ok_or(HttpPopError::Header)?;
    Ok((integer(c)?, integer(e)?))
}
fn bytes_field(value: &str, prefix: &str) -> Result<[u8; 32], HttpPopError> {
    let inner = value
        .strip_prefix(prefix)
        .and_then(|v| v.strip_suffix(':'))
        .ok_or(HttpPopError::Digest)?;
    let bytes: [u8; 32] = STANDARD
        .decode(inner)
        .map_err(|_| HttpPopError::Digest)?
        .try_into()
        .map_err(|_| HttpPopError::Digest)?;
    if STANDARD.encode(bytes) != inner {
        return Err(HttpPopError::Digest);
    }
    Ok(bytes)
}
pub fn prepare_http_pop(
    message: &HttpPopMessage<'_>,
    expected: &HttpPopBinding<'_>,
    now: UnixMillis,
) -> Result<PreparedHttpPop, HttpPopError> {
    validate_profile(expected.profile, expected.target, expected.binding_id)?;
    if message.method != expected.target.method() || message.target_uri != expected.target.uri() {
        return Err(HttpPopError::Target);
    }
    let map = headers(message)?;
    let get = |name: &str| map.get(name).copied().ok_or(HttpPopError::Header);
    if message.method == "POST" {
        if get("content-type")? != "application/json" {
            return Err(HttpPopError::Header);
        }
    } else if !message.body.is_empty() || map.contains_key("content-type") {
        return Err(HttpPopError::Header);
    }
    if map.get("host").is_some_and(|v| {
        *v != expected
            .target
            .origin()
            .strip_prefix("https://")
            .unwrap_or("")
    }) || map
        .get("content-length")
        .is_some_and(|v| *v != message.body.len().to_string())
    {
        return Err(HttpPopError::Header);
    }
    if get(expected.profile.binding_header())? != expected.binding_id
        || get("oasis7-request-id")? != expected.request_id
        || map.contains_key(if expected.profile == HttpPopProfileV1::EnrollmentProof {
            "oasis7-test-session"
        } else {
            "oasis7-enrollment"
        })
    {
        return Err(HttpPopError::Binding);
    }
    if expected.profile.is_mcp() {
        if get("mcp-protocol-version")? != MCP_PROTOCOL_VERSION
            || map.contains_key("oasis7-capability")
        {
            return Err(HttpPopError::Binding);
        }
    } else if map.contains_key("authorization") || map.contains_key("mcp-protocol-version") {
        return Err(HttpPopError::Binding);
    }
    let token = if expected.profile.is_mcp() {
        match map.get("authorization") {
            Some(v) => Some(v.strip_prefix("Bearer ").ok_or(HttpPopError::Binding)?),
            None => None,
        }
    } else {
        map.get("oasis7-capability").copied()
    };
    let token = token_hash(expected.profile, token)?;
    let claimed_token = bytes_field(get("oasis7-token-digest")?, ":")?;
    if !ct_equal(&token, &claimed_token) || !ct_equal(&token, expected.credential_digest) {
        return Err(HttpPopError::Digest);
    }
    let body = sha256(message.body);
    if !ct_equal(&body, &bytes_field(get("content-digest")?, "sha-256=:")?) {
        return Err(HttpPopError::Digest);
    }
    let (created, expires) =
        time_parameters(get("signature-input")?, expected.profile, message.method)?;
    validate_params(
        expected.profile,
        expected.key_id,
        expected.binding_id,
        expected.request_id,
        expected.nonce,
        created,
        expires,
    )?;
    validate_time(created, expires, now, expected.valid_until)?;
    let params = parameters(
        expected.profile,
        message.method,
        expected.key_id,
        expected.nonce,
        created,
        expires,
    );
    if get("signature-input")? != format!("oasis7={params}") {
        return Err(HttpPopError::Header);
    }
    let signature = get("signature")?
        .strip_prefix("oasis7=:")
        .and_then(|v| v.strip_suffix(':'))
        .ok_or(HttpPopError::Signature)?;
    let bytes: [u8; 64] = STANDARD
        .decode(signature)
        .map_err(|_| HttpPopError::Signature)?
        .try_into()
        .map_err(|_| HttpPopError::Signature)?;
    if STANDARD.encode(bytes) != signature {
        return Err(HttpPopError::Signature);
    }
    let proof = PreparedHttpPop {
        key_id: expected.key_id.into(),
        public_key_hex: expected.public_key_hex.into(),
        signature: bytes,
        base: base(
            expected.profile,
            message.method,
            message.target_uri,
            &params,
            &map,
        )?,
        profile: expected.profile,
        binding_id: expected.binding_id.into(),
        request_id: expected.request_id.into(),
        nonce: expected.nonce.map(str::to_owned),
        method: message.method.into(),
        target_uri: message.target_uri.into(),
        body_digest: body,
        token_digest: token,
        created,
        expires,
        valid_until: expected.valid_until,
    };
    verify_ed25519(&proof)?;
    Ok(proof)
}
