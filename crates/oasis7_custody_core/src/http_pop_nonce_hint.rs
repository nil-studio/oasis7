//! A bounded lookup hint, never proof, authentication or admission.
use super::*;
use std::collections::BTreeMap;
/// Reads only the fixed profile's singleton Signature-Input. Full preparation
/// MUST still compare all components, parameters, actual bytes and stored nonce.
pub fn http_pop_nonce_hint(
    headers: &[HttpHeader<'_>],
    representation: HttpHeaderRepresentation,
) -> Result<Option<String>, HttpPopError> {
    let map = super::headers(&HttpPopMessage {
        method: "",
        target_uri: "",
        headers,
        representation,
        body: &[],
    })?;
    let input = map
        .get("signature-input")
        .copied()
        .ok_or(HttpPopError::Header)?;
    if input.contains(',') {
        return Err(HttpPopError::Header);
    }
    let (fields, parameters) = input
        .strip_prefix("oasis7=(")
        .and_then(|v| v.split_once(");"))
        .ok_or(HttpPopError::Header)?;
    if !fields.starts_with('"')
        || !fields.ends_with('"')
        || fields.contains("  ")
        || !fields
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"@- \"".contains(&b))
    {
        return Err(HttpPopError::Header);
    }
    let mut params = BTreeMap::new();
    for item in parameters.split(';') {
        let (name, value) = item.split_once('=').ok_or(HttpPopError::Header)?;
        if !matches!(
            name,
            "created" | "expires" | "nonce" | "alg" | "keyid" | "tag"
        ) || params.insert(name, value).is_some()
        {
            return Err(HttpPopError::Header);
        }
    }
    for name in ["created", "expires"] {
        let value = params.get(name).ok_or(HttpPopError::Header)?;
        let number = value.parse::<u64>().map_err(|_| HttpPopError::Header)?;
        if number.to_string() != *value {
            return Err(HttpPopError::Header);
        }
    }
    if params.get("alg") != Some(&"\"ed25519\"") {
        return Err(HttpPopError::Header);
    }
    let quoted = |name| {
        params
            .get(name)
            .and_then(|v| v.strip_prefix('"'))
            .and_then(|v| v.strip_suffix('"'))
            .ok_or(HttpPopError::Header)
    };
    if !super::identifier(quoted("keyid")?) {
        return Err(HttpPopError::Header);
    }
    let tag = quoted("tag")?;
    if ![
        HttpPopProfileV1::Custody,
        HttpPopProfileV1::Mcp,
        HttpPopProfileV1::EnrollmentProof,
        HttpPopProfileV1::CredentialClaim,
        HttpPopProfileV1::CustodyChallenge,
        HttpPopProfileV1::McpChallenge,
    ]
    .iter()
    .any(|p| p.tag() == tag)
    {
        return Err(HttpPopError::Header);
    }
    params
        .get("nonce")
        .map(|_| {
            let nonce = quoted("nonce")?;
            let decoded = URL_SAFE_NO_PAD
                .decode(nonce)
                .map_err(|_| HttpPopError::Header)?;
            if decoded.len() != 32 || URL_SAFE_NO_PAD.encode(decoded) != nonce {
                return Err(HttpPopError::Header);
            }
            Ok(nonce.to_owned())
        })
        .transpose()
}
