//! Native Host transport URL policy extracted from the original provider.
//!
//! This checks only URL/transport admission; no HTTP request is issued.
//! The shipping provider client delegates admission here and retains its
//! original error mapping, request handling, and capture bounds.
use std::fmt;
use std::net::IpAddr;

use url::Url;

pub const LOOPBACK_HTTP_PROVIDER_TRANSPORT: &str =
    oasis7_client_api::provider::PROVIDER_TRANSPORT_LOOPBACK_HTTP;
pub const REMOTE_HTTPS_PROVIDER_TRANSPORT: &str =
    oasis7_client_api::provider::PROVIDER_TRANSPORT_REMOTE_HTTPS;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderUrlAdmissionError(pub String);

impl fmt::Display for ProviderUrlAdmissionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ProviderUrlAdmissionError {}

fn invalid(reason: impl Into<String>) -> ProviderUrlAdmissionError {
    ProviderUrlAdmissionError(reason.into())
}

pub fn validate_provider_loopback_http_base_url(
    base_url: &str,
) -> Result<Url, ProviderUrlAdmissionError> {
    validate_provider_http_base_url(base_url, LOOPBACK_HTTP_PROVIDER_TRANSPORT)
}

pub fn validate_provider_http_base_url(
    base_url: &str,
    transport: &str,
) -> Result<Url, ProviderUrlAdmissionError> {
    let trimmed = base_url.trim();
    if trimmed.is_empty() {
        return Err(invalid("base url cannot be empty"));
    }
    let url = Url::parse(trimmed).map_err(|error| invalid(error.to_string()))?;
    let Some(host) = url.host_str() else {
        return Err(invalid("host is required"));
    };
    match transport.trim() {
        LOOPBACK_HTTP_PROVIDER_TRANSPORT => {
            if url.scheme() != "http" {
                return Err(invalid("scheme must be http for localhost provider"));
            }
            if !matches!(host, "127.0.0.1" | "localhost" | "::1") {
                return Err(invalid(
                    "host must be loopback (127.0.0.1 / localhost / ::1)",
                ));
            }
        }
        REMOTE_HTTPS_PROVIDER_TRANSPORT => {
            if url.scheme() != "https" {
                return Err(invalid(
                    "scheme must be https for remote provider transport",
                ));
            }
            if !is_public_remote_https_host(host) {
                return Err(invalid("host must be a public remote https host"));
            }
        }
        _ => {
            return Err(invalid(format!(
                "unsupported provider transport `{transport}`"
            )));
        }
    }
    Ok(url)
}

fn is_public_remote_https_host(host: &str) -> bool {
    if matches!(host.trim(), "127.0.0.1" | "localhost" | "::1") {
        return false;
    }
    let Ok(ip) = host.parse::<IpAddr>() else {
        return true;
    };
    match ip {
        IpAddr::V4(ipv4) => !ipv4.is_private() && !ipv4.is_loopback() && !ipv4.is_link_local(),
        IpAddr::V6(ipv6) => !ipv6.is_loopback() && !ipv6.is_unicast_link_local(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loopback_accepts_only_http_and_explicit_loopback_hosts() {
        assert_eq!(
            validate_provider_loopback_http_base_url(" http://127.0.0.1:8000/ ")
                .unwrap()
                .host_str(),
            Some("127.0.0.1"),
        );
        assert!(validate_provider_loopback_http_base_url("http://localhost:8000").is_ok());
        for denied in [
            "https://localhost",
            "http://example.com",
            "http://192.168.1.1",
            "",
        ] {
            assert!(
                validate_provider_loopback_http_base_url(denied).is_err(),
                "loopback transport accepted forbidden URL: {denied}",
            );
        }
    }

    #[test]
    fn remote_requires_https_and_rejects_local_private_or_linklocal_hosts() {
        let transport = REMOTE_HTTPS_PROVIDER_TRANSPORT;
        assert!(validate_provider_http_base_url("https://example.com/v1/", transport).is_ok());
        for denied in [
            "http://example.com",
            "https://localhost",
            "https://127.0.0.1",
            "https://192.168.1.10",
            "https://169.254.1.1",
        ] {
            assert!(
                validate_provider_http_base_url(denied, transport).is_err(),
                "remote transport accepted forbidden URL: {denied}",
            );
        }
    }

    #[test]
    fn unknown_transport_and_url_syntax_fail_closed() {
        let err = validate_provider_http_base_url("https://example.com", "unregistered-transport")
            .unwrap_err();
        assert_eq!(
            err.0,
            "unsupported provider transport `unregistered-transport`"
        );
        assert!(
            validate_provider_http_base_url("not a url", REMOTE_HTTPS_PROVIDER_TRANSPORT).is_err()
        );
    }
}
