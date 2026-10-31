//! Pure canonical origin grammar shared by custody proof codecs; no authority.
pub fn is_canonical_https_origin(origin: &str) -> bool {
    let Some(authority) = origin.strip_prefix("https://") else {
        return false;
    };
    if origin.len() > 512
        || authority.is_empty()
        || !authority.is_ascii()
        || authority.contains(['/', '?', '#', '@', '\\'])
    {
        return false;
    }
    let (host, port) = authority
        .rsplit_once(':')
        .map_or((authority, None), |(h, p)| (h, Some(p)));
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
        return false;
    }
    if let Some(p) = port {
        let Ok(n) = p.parse::<u16>() else {
            return false;
        };
        if n == 0 || n == 443 || n.to_string() != p {
            return false;
        }
    }
    true
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn canonical_origin_uses_existing_closed_https_grammar() {
        for origin in [
            "https://example.test",
            "https://127.0.0.1:8443",
            "https://localhost",
        ] {
            assert!(is_canonical_https_origin(origin));
        }
        for origin in [
            "http://example.test",
            "https://example.test/",
            "https://example.test?x",
            "https://example.test#x",
            "https://user@example.test",
            "https://EXAMPLE.test",
            "https://example.test:443",
            "https://example.test:0444",
            "https://example.test:0",
            "https://example.test:65536",
            "https://a..test",
        ] {
            assert!(!is_canonical_https_origin(origin));
        }
    }
}
