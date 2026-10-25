use super::DeploymentMode;

pub(super) fn browser_game_trust(trust: &str) -> Result<serde_json::Value, String> {
    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Trust {
        endpoint: String,
        service_public_key: String,
        world: oasis7_client_api::world_service::WorldIdentity,
    }
    let trust: Trust = serde_json::from_str(trust).map_err(|_| "invalid browser world trust")?;
    trust
        .world
        .validate()
        .map_err(|_| "invalid browser world")?;
    let bytes: [u8; 32] = hex::decode(&trust.service_public_key)
        .map_err(|_| "invalid browser service key")?
        .try_into()
        .map_err(|_| "invalid browser service key")?;
    let key = ed25519_dalek::VerifyingKey::from_bytes(&bytes)
        .map_err(|_| "invalid browser service key")?;
    let upstream = reqwest::Url::parse(&trust.endpoint).map_err(|_| "invalid browser upstream")?;
    if key.is_weak()
        || trust.service_public_key != hex::encode(bytes)
        || upstream.scheme() != "http"
        || upstream.origin().ascii_serialization() != trust.endpoint
        || !upstream.host_str().is_some_and(|host| {
            host.parse::<std::net::IpAddr>()
                .is_ok_and(|ip| ip.is_loopback())
        })
    {
        return Err("browser trust must match the hosted participant gateway".into());
    }
    Ok(
        serde_json::json!({"endpoint":"/api/public/hosted-account/v2", "servicePublicKey":trust.service_public_key,"world":trust.world}),
    )
}

pub(super) fn enable_browser_game(config: &str, trust: &str) -> Result<String, String> {
    let mut config: serde_json::Value =
        serde_json::from_str(config).map_err(|_| "invalid viewer config")?;
    config["browserGameClient"] = browser_game_trust(trust)?;
    serde_json::to_string(&config).map_err(|_| "invalid viewer config".into())
}

pub(super) fn viewer_runtime_config_json(
    deployment_mode: DeploymentMode,
    web_bind: &str,
    configured: Option<&str>,
) -> Result<String, String> {
    let endpoint = match configured {
        Some(value) => {
            let url = reqwest::Url::parse(value)
                .map_err(|err| format!("invalid OASIS7_VIEWER_PUBLIC_WS_URL: {err}"))?;
            if !matches!(url.scheme(), "ws" | "wss")
                || value.split("://").nth(1).is_some_and(|rest| {
                    rest.split(['/', '?', '#'])
                        .next()
                        .is_some_and(|authority| authority.contains('@'))
                })
                || !url.username().is_empty()
                || url.password().is_some()
                || url.fragment().is_some()
                || (deployment_mode == DeploymentMode::HostedPublicJoin && url.scheme() != "wss")
            {
                return Err("OASIS7_VIEWER_PUBLIC_WS_URL has forbidden URL components or insecure hosted transport".to_string());
            }
            url.to_string()
        }
        None => {
            let bind: std::net::SocketAddr = web_bind
                .parse()
                .map_err(|err| format!("invalid viewer runtime bind: {err}"))?;
            let ip = if bind.ip().is_unspecified() {
                if bind.is_ipv4() {
                    std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST)
                } else {
                    std::net::IpAddr::V6(std::net::Ipv6Addr::LOCALHOST)
                }
            } else {
                bind.ip()
            };
            if deployment_mode == DeploymentMode::HostedPublicJoin && !ip.is_loopback() {
                return Err("remote hosted Viewer requires OASIS7_VIEWER_PUBLIC_WS_URL with its browser-accessible WSS target".to_string());
            }
            format!("ws://{}/", std::net::SocketAddr::new(ip, bind.port()))
        }
    };
    serde_json::to_string(&serde_json::json!({
        "deploymentMode": deployment_mode.as_str(),
        "viewerWsEndpoint": endpoint,
    }))
    .map_err(|err| format!("serialize viewer runtime configuration: {err}"))
}

pub(super) fn inject_config(body: &[u8], config_json: &str) -> Result<Vec<u8>, String> {
    let html = String::from_utf8_lossy(body);
    if html.contains("oasis7-viewer-runtime-config") {
        return Err("static HTML must not supply its own viewer runtime configuration".to_string());
    }
    let payload = config_json.replace('<', "\\u003c");
    let node = format!(
        "<script id=\"oasis7-viewer-runtime-config\" type=\"application/json\">{payload}</script>"
    );
    let insert_at = html
        .rfind("</head>")
        .or_else(|| html.rfind("</body>"))
        .unwrap_or(html.len());
    Ok(format!("{}{}{}", &html[..insert_at], node, &html[insert_at..]).into_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn browser_trust_is_fixed_to_hosted_gateway_without_exposing_upstream() {
        let key = hex::encode(
            ed25519_dalek::SigningKey::from_bytes(&[19; 32])
                .verifying_key()
                .to_bytes(),
        );
        let trust = serde_json::json!({"endpoint":"http://127.0.0.1:5180","service_public_key":key,"world":{"world_id":"world","genesis_digest":"genesis"}}).to_string();
        let value = browser_game_trust(&trust).unwrap();
        assert_eq!(value["endpoint"], "/api/public/hosted-account/v2");
        assert!(!value.to_string().contains("5180"));
        assert!(browser_game_trust(&trust.replace("127.0.0.1", "remote.example")).is_err());
        assert!(browser_game_trust(&trust.replace(&key, &"00".repeat(32))).is_err());
    }
    #[test]
    fn local_wildcard_binds_are_explicit_loopback_targets() {
        for (bind, endpoint) in [
            ("0.0.0.0:5011", "ws://127.0.0.1:5011/"),
            ("[::]:5011", "ws://[::1]:5011/"),
        ] {
            let json =
                viewer_runtime_config_json(DeploymentMode::TrustedLocalOnly, bind, None).unwrap();
            let value: serde_json::Value = serde_json::from_str(&json).unwrap();
            assert_eq!(value["viewerWsEndpoint"], endpoint);
            assert_eq!(value["deploymentMode"], "trusted_local_only");
        }
    }
    #[test]
    fn configured_hosted_endpoint_requires_wss_and_rejects_untrusted_url_components() {
        let valid = viewer_runtime_config_json(
            DeploymentMode::HostedPublicJoin,
            "0.0.0.0:5011",
            Some("wss://trusted.example/runtime?lane=1"),
        )
        .unwrap();
        assert!(valid.contains("trusted.example/runtime?lane=1"));
        for endpoint in [
            "ws://trusted.example/runtime",
            "https://trusted.example/runtime",
            "wss://user@trusted.example/runtime",
            "wss://@trusted.example/runtime",
            "wss://trusted.example/runtime#",
            "wss://trusted.example/runtime#fragment",
        ] {
            assert!(
                viewer_runtime_config_json(
                    DeploymentMode::HostedPublicJoin,
                    "0.0.0.0:5011",
                    Some(endpoint)
                )
                .is_err()
            );
        }
    }
    #[test]
    fn injected_json_is_unique_and_cannot_close_its_data_node() {
        let json = r#"{"viewerWsEndpoint":"wss://trusted.example/?x=</script>"}"#;
        let output = String::from_utf8(inject_config(b"<head></head>", json).unwrap()).unwrap();
        assert!(output.contains(r"\u003c/script>"));
        assert!(!output.contains("x=</script>"));
        assert_eq!(
            output
                .matches("id=\"oasis7-viewer-runtime-config\"")
                .count(),
            1
        );
        assert!(inject_config(output.as_bytes(), json).is_err());
    }
}
