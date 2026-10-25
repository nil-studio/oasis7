//! Strict identity JSON and security headers for trusted, generated hosted HTML.
use base64::{Engine, engine::general_purpose::STANDARD};
use serde::de::{self, Deserialize, Deserializer, MapAccess, SeqAccess, Visitor};
use serde_json::{Map, Number, Value};
use sha2::{Digest, Sha256};
use std::{collections::BTreeSet, fmt};

struct UniqueJson(Value);
impl<'de> Deserialize<'de> for UniqueJson {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct JsonVisitor;
        impl<'de> Visitor<'de> for JsonVisitor {
            type Value = UniqueJson;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("JSON without duplicate object fields")
            }
            fn visit_bool<E: de::Error>(self, value: bool) -> Result<Self::Value, E> {
                Ok(UniqueJson(Value::Bool(value)))
            }
            fn visit_i64<E: de::Error>(self, value: i64) -> Result<Self::Value, E> {
                Ok(UniqueJson(Value::Number(value.into())))
            }
            fn visit_u64<E: de::Error>(self, value: u64) -> Result<Self::Value, E> {
                Ok(UniqueJson(Value::Number(value.into())))
            }
            fn visit_f64<E: de::Error>(self, value: f64) -> Result<Self::Value, E> {
                Number::from_f64(value)
                    .map(|n| UniqueJson(Value::Number(n)))
                    .ok_or_else(|| E::custom("invalid JSON number"))
            }
            fn visit_str<E: de::Error>(self, value: &str) -> Result<Self::Value, E> {
                Ok(UniqueJson(Value::String(value.into())))
            }
            fn visit_string<E: de::Error>(self, value: String) -> Result<Self::Value, E> {
                Ok(UniqueJson(Value::String(value)))
            }
            fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
                Ok(UniqueJson(Value::Null))
            }
            fn visit_none<E: de::Error>(self) -> Result<Self::Value, E> {
                self.visit_unit()
            }
            fn visit_seq<A: SeqAccess<'de>>(
                self,
                mut sequence: A,
            ) -> Result<Self::Value, A::Error> {
                let mut values = Vec::new();
                while let Some(value) = sequence.next_element::<UniqueJson>()? {
                    values.push(value.0);
                }
                Ok(UniqueJson(Value::Array(values)))
            }
            fn visit_map<A: MapAccess<'de>>(self, mut object: A) -> Result<Self::Value, A::Error> {
                let mut values = Map::new();
                while let Some(key) = object.next_key::<String>()? {
                    if values.contains_key(&key) {
                        return Err(de::Error::custom("duplicate JSON field"));
                    }
                    values.insert(key, object.next_value::<UniqueJson>()?.0);
                }
                Ok(UniqueJson(Value::Object(values)))
            }
        }
        deserializer.deserialize_any(JsonVisitor)
    }
}

pub(super) fn parse_identity_json(bytes: &[u8]) -> Result<Value, String> {
    let mut deserializer = serde_json::Deserializer::from_slice(bytes);
    let value = UniqueJson::deserialize(&mut deserializer)
        .map_err(|_| "invalid_request".to_owned())?
        .0;
    deserializer
        .end()
        .map_err(|_| "invalid_request".to_owned())?;
    if !value.is_object() {
        return Err("invalid_request".into());
    }
    Ok(value)
}

pub(super) fn hosted_html_security_headers(
    body: &[u8],
    runtime_config_json: &str,
) -> Result<String, String> {
    let config = parse_identity_json(runtime_config_json.as_bytes())?;
    let endpoint = config
        .get("viewerWsEndpoint")
        .and_then(Value::as_str)
        .ok_or("invalid_runtime_endpoint")?;
    if endpoint.split("://").nth(1).is_some_and(|rest| {
        rest.split(['/', '?', '#'])
            .next()
            .is_some_and(|authority| authority.contains('@'))
    }) {
        return Err("invalid_runtime_endpoint".into());
    }
    let endpoint = url::Url::parse(endpoint).map_err(|_| "invalid_runtime_endpoint")?;
    if !matches!(endpoint.scheme(), "ws" | "wss")
        || endpoint.host_str().is_none()
        || !endpoint.username().is_empty()
        || endpoint.password().is_some()
        || endpoint.fragment().is_some()
    {
        return Err("invalid_runtime_endpoint".into());
    }
    // Only the validated server config contributes a connection origin. Paths,
    // queries, request Host and forwarded headers are never CSP source inputs.
    let connection_origin = endpoint.origin().ascii_serialization();
    if connection_origin == "null" {
        return Err("invalid_runtime_endpoint".into());
    }
    let mut hashes = BTreeSet::new();
    for script in inline_scripts(body)? {
        // HTML input preprocessing normalizes CRLF/CR before constructing the
        // script text used by the browser's CSP inline check.
        let normalized = std::str::from_utf8(script)
            .map_err(|_| "invalid_hosted_html")?
            .replace("\r\n", "\n")
            .replace('\r', "\n");
        hashes.insert(format!(
            "'sha256-{}'",
            STANDARD.encode(Sha256::digest(normalized.as_bytes()))
        ));
    }
    let scripts = hashes.into_iter().collect::<Vec<_>>().join(" ");
    let model_origin = if config.get("browserGameClient").is_some() {
        " https://api.deepseek.com"
    } else {
        ""
    };
    Ok(format!(
        "Content-Security-Policy: default-src 'self'; script-src 'self' 'wasm-unsafe-eval' {scripts}; connect-src 'self' {connection_origin}{model_origin}; style-src 'self' 'unsafe-inline'; img-src 'self' data: blob:; worker-src 'self' blob:; frame-ancestors 'none'; object-src 'none'; base-uri 'none'; form-action 'self'\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\nReferrer-Policy: no-referrer\r\n"
    ))
}

// The launcher serves repository-owned HTML with generated bootstrap/data nodes.
// Scan raw-text script bodies (ASCII-insensitive HTML names) without trimming or
// decoding their text: CSP hashes cover browser script text after HTML newline
// preprocessing, with no trimming or JavaScript reserialization.
// Reject unterminated/ambiguous markup rather than relaxing script-src.
fn inline_scripts(body: &[u8]) -> Result<Vec<&[u8]>, String> {
    std::str::from_utf8(body).map_err(|_| "invalid_hosted_html")?;
    if body.contains(&0) {
        return Err("invalid_hosted_html".into());
    }
    let lower = body.iter().map(u8::to_ascii_lowercase).collect::<Vec<_>>();
    let mut cursor = 0;
    let mut scripts = Vec::new();
    while cursor < body.len() {
        let Some(relative) = lower[cursor..].iter().position(|&b| b == b'<') else {
            break;
        };
        let start = cursor + relative;
        if lower[start..].starts_with(b"<!--") {
            let end = find_bytes(&lower, start + 4, b"-->").ok_or("invalid_hosted_html")?;
            cursor = end + 3;
            continue;
        }
        if !lower[start..].starts_with(b"<script") || !tag_boundary(lower.get(start + 7).copied()) {
            cursor = start + 1;
            continue;
        }
        let open_end = tag_end(body, start + 7)?;
        let text_start = open_end + 1;
        let mut closing = text_start;
        let close_start = loop {
            let candidate =
                find_bytes(&lower, closing, b"</script").ok_or("invalid_hosted_html")?;
            if tag_boundary(lower.get(candidate + 8).copied()) {
                break candidate;
            }
            closing = candidate + 8;
        };
        let close_end = tag_end(body, close_start + 8)?;
        let text = &body[text_start..close_start];
        // HTML's double-escaped script state changes what counts as the closing
        // tag. It is unnecessary in our generated pages and deliberately rejected.
        if find_bytes(&lower, text_start, b"<!--").is_some_and(|p| p < close_start) {
            return Err("unsupported_hosted_script_markup".into());
        }
        scripts.push(text);
        cursor = close_end + 1;
    }
    Ok(scripts)
}
fn tag_boundary(byte: Option<u8>) -> bool {
    byte.is_some_and(|b| b.is_ascii_whitespace() || b == b'>' || b == b'/')
}
fn tag_end(bytes: &[u8], start: usize) -> Result<usize, String> {
    let mut quote = None;
    for (offset, &byte) in bytes[start..].iter().enumerate() {
        match (quote, byte) {
            (Some(q), b) if q == b => quote = None,
            (None, b'\'' | b'"') => quote = Some(byte),
            (None, b'>') => return Ok(start + offset),
            (None, b'<') => return Err("invalid_hosted_html".into()),
            _ => {}
        }
    }
    Err("invalid_hosted_html".into())
}
fn find_bytes(bytes: &[u8], from: usize, needle: &[u8]) -> Option<usize> {
    bytes
        .get(from..)?
        .windows(needle.len())
        .position(|window| window == needle)
        .map(|p| from + p)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_duplicate_actions_and_nested_credential_fields() {
        for body in [
            br#"{"action_type":"bind","action_type":"freeze"}"#.as_slice(),
            br#"{"response":{"clientDataJSON":"AQ","clientDataJSON":"Ag"}}"#,
            br#"{"items":[{"id":1,"\u0069d":2}]}"#,
            b"{} {}",
            b"[]",
            b"null",
        ] {
            assert!(parse_identity_json(body).is_err());
        }
        let parsed =
            parse_identity_json(br#"{"action_type":"bind","nested":[{"x":1},true,null,1.25]}"#)
                .unwrap();
        assert_eq!(parsed["nested"][0]["x"], 1);
    }
    #[test]
    fn hashes_exact_inline_contents_and_uses_only_canonical_ws_origin() {
        let html = b"<!-- <script>not executable</script> --><ScRiPt data-test='>'>const x = 1;\n</SCRIPT><script src='/viewer.js'></script>";
        let headers = hosted_html_security_headers(html, r#"{"viewerWsEndpoint":"wss://trusted.example:8443/runtime?lane=1","deploymentMode":"hosted_public_join"}"#).unwrap();
        assert!(headers.contains("'sha256-lb791uaR1NiQMaKikBzHT8YkIQmYCwYOCN34eCmSRIM='"));
        let crlf = hosted_html_security_headers(
            b"<script>const x = 1;\r\n</script>",
            r#"{"viewerWsEndpoint":"wss://trusted.example"}"#,
        )
        .unwrap();
        assert!(crlf.contains("'sha256-lb791uaR1NiQMaKikBzHT8YkIQmYCwYOCN34eCmSRIM='"));
        assert!(headers.contains("connect-src 'self' wss://trusted.example:8443;"));
        assert!(!headers.contains("runtime"));
        assert!(!headers.contains("lane=1"));
        let script_directive = headers
            .split(';')
            .find(|d| d.trim_start().starts_with("script-src "))
            .unwrap();
        assert!(!script_directive.contains("'unsafe-inline'"));
        assert!(!headers.contains(" wss:;"));
        assert!(!headers.contains(" wss: "));
        assert!(headers.contains("frame-ancestors 'none'"));
        assert!(headers.contains("Cache-Control: no-store"));
        assert!(headers.contains("Referrer-Policy: no-referrer"));
    }
    #[test]
    fn browser_model_origin_is_allowed_only_when_enabled() {
        let disabled =
            hosted_html_security_headers(b"", r#"{"viewerWsEndpoint":"wss://trusted.example"}"#)
                .unwrap();
        assert!(!disabled.contains("deepseek"));
        let enabled = hosted_html_security_headers(
            b"",
            r#"{"viewerWsEndpoint":"wss://trusted.example","browserGameClient":{}}"#,
        )
        .unwrap();
        assert!(
            enabled.contains("connect-src 'self' wss://trusted.example https://api.deepseek.com;")
        );
        assert!(!enabled.contains("https:;"));
    }
    #[test]
    fn rejects_bad_config_and_ambiguous_html_instead_of_weakening_policy() {
        for config in [
            r#"{"viewerWsEndpoint":"wss://user@evil.example"}"#,
            r#"{"viewerWsEndpoint":"wss://@evil.example"}"#,
            r#"{"viewerWsEndpoint":"https://evil.example"}"#,
            r#"{"host":"evil.example"}"#,
            r#"{"viewerWsEndpoint":"wss://good.example","viewerWsEndpoint":"wss://evil.example"}"#,
        ] {
            assert!(hosted_html_security_headers(b"<script>x()</script>", config).is_err());
        }
        let config = r#"{"viewerWsEndpoint":"wss://trusted.example"}"#;
        assert!(hosted_html_security_headers(b"<script>unterminated", config).is_err());
        assert!(
            hosted_html_security_headers(b"<script><!-- <script>double escaped</script>", config)
                .is_err()
        );
        let headers = hosted_html_security_headers(b"<script>safe()</script>", config).unwrap();
        assert!(!headers.contains("evil.example"));
    }
}
