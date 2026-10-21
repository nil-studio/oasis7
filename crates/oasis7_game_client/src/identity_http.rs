use super::*;

pub(super) fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
pub(super) fn validate_config(config: &IdentityClientConfig) -> Result<(), String> {
    let url = reqwest::Url::parse(&config.endpoint).map_err(|_| "invalid identity endpoint")?;
    #[cfg(not(test))]
    let loopback = false;
    #[cfg(test)]
    let loopback = config.test_loopback
        && url
            .host_str()
            .is_some_and(|v| v.parse::<std::net::IpAddr>().is_ok_and(|v| v.is_loopback()))
        && url.scheme() == "http";
    if (!loopback && url.scheme() != "https")
        || url.origin().ascii_serialization() != config.endpoint
        || url.query().is_some()
        || url.fragment().is_some()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err("identity endpoint must be an exact trusted HTTPS origin".into());
    }
    let audience =
        reqwest::Url::parse(&config.audience).map_err(|_| "invalid identity audience")?;
    if audience.scheme() != "https"
        || audience.origin().ascii_serialization() != config.audience
        || config.identity_service_id.is_empty()
        || config.identity_service_id.len() > 4096
        || config.environment_id.is_empty()
        || config.environment_id.len() > 4096
    {
        return Err("identity trust configuration invalid".into());
    }
    Ok(())
}
impl IdentityClient {
    pub(super) fn call(
        &self,
        method: &str,
        path: &str,
        original: Option<&OriginalRequest>,
    ) -> Result<(Value, Option<String>), String> {
        // Never follow response-provided endpoints or attach credentials elsewhere.
        if ![
            "/capabilities",
            "/session",
            "/machine-registration/options",
            "/machine-registration/complete",
            "/native-key/options",
            "/native-key/complete",
            "/participant/authorize",
            "/participant/submit",
            "/participant/lookup",
            "/participant/owner-view",
            "/participant/starter-eligibility",
            "/entry-owner/control",
            "/entry-executor/control",
        ]
        .contains(&path)
        {
            return Err("identity route not allowed".into());
        }
        let url = format!("{}{IDENTITY_PREFIX}{path}", self.config.endpoint);
        let mut request = match method {
            "GET" => self.http.get(url),
            "POST" => self.http.post(url),
            _ => return Err("identity method not allowed".into()),
        };
        if let Some(original) = original {
            if original.path != path
                || original.cookie.contains(['\r', '\n'])
                || original.csrf.contains(['\r', '\n'])
            {
                return Err("identity original malformed".into());
            }
            request = request.header("Cookie", &original.cookie);
            if method == "POST" {
                request = request
                    .header("Origin", &self.config.audience)
                    .header("X-Oasis7-Csrf", &original.csrf)
                    .header("Content-Type", "application/json")
                    .body(original.body.clone());
            }
        }
        let response = request
            .send()
            .map_err(|_| "identity response unknown; retain original operation")?;
        let status = response.status();
        let mut cookie = None;
        for h in response.headers().get_all(reqwest::header::SET_COOKIE) {
            let text = h.to_str().map_err(|_| "identity cookie malformed")?;
            let first = text.split(';').next().ok_or("identity cookie malformed")?;
            let Some((name, value)) = first.split_once('=') else {
                return Err("identity cookie malformed".into());
            };
            if !["__Host-oasis7_pre", "__Host-oasis7_sid"].contains(&name) {
                continue;
            }
            if !URL_SAFE_NO_PAD
                .decode(value)
                .is_ok_and(|v| v.len() == 32 && URL_SAFE_NO_PAD.encode(v) == value)
                || !text
                    .split(';')
                    .any(|p| p.trim().eq_ignore_ascii_case("secure"))
                || !text
                    .split(';')
                    .any(|p| p.trim().eq_ignore_ascii_case("httponly"))
                || cookie.is_some()
            {
                return Err("identity cookie malformed".into());
            }
            cookie = Some(first.into());
        }
        let mut bytes = Vec::new();
        let maximum = if path == "/entry-executor/control" {
            4 * 1024 * 1024
        } else {
            65536
        };
        response
            .take(maximum as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| "identity response unknown; retain original operation")?;
        if bytes.len() > maximum {
            return Err("identity response exceeds bound; retain original operation".into());
        }
        let result: Value = oasis7_agent_api::game::parse_bounded_json(&bytes, maximum)
            .map_err(|_| "identity response malformed; retain original operation")?;
        if status.as_u16() != 200 || result["api_version"] != 2 || result["ok"] != true {
            let code = match result["error_code"].as_str() {
                Some("registration_closed") => "registration_closed",
                Some("rate_limited") => "rate_limited",
                Some("authentication_failed" | "session_expired") => "authentication_failed",
                Some("ceremony_expired") => "ceremony_expired",
                Some("idempotency_conflict" | "credential_conflict") => "identity_conflict",
                _ => "identity_outcome_unknown",
            };
            return Err(format!("{code}; retain original operation"));
        }
        Ok((result, cookie))
    }
}
