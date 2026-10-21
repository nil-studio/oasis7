use super::*;
use std::io::Read;

impl IdentityClient {
    /// Non-secret preflight before vault initialization; discovery grants nothing.
    pub fn discover_entry(config: &IdentityClientConfig) -> Result<Value, String> {
        http::validate_config(config)?;
        let client = reqwest::blocking::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(10))
            .connect_timeout(Duration::from_secs(3))
            .build()
            .map_err(|_| "identity discovery unavailable")?;
        let response = client
            .get(format!(
                "{}{IDENTITY_PREFIX}/entry/discovery",
                config.endpoint
            ))
            .send()
            .map_err(|_| "identity discovery unavailable")?;
        if response.status() != reqwest::StatusCode::OK {
            return Err("identity discovery unavailable".into());
        }
        let mut bytes = Vec::new();
        response
            .take(65537)
            .read_to_end(&mut bytes)
            .map_err(|_| "identity discovery unavailable")?;
        if bytes.len() > 65536 {
            return Err("identity discovery exceeds bound".into());
        }
        let value: Value =
            oasis7_agent_api::game::parse_request(&bytes).map_err(|_| "invalid entry discovery")?;
        if value["api_version"] != 2
            || value["ok"] != true
            || value["schema_revision"] != 1
            || value["identity"]["service_id"] != config.identity_service_id
            || value["identity"]["environment_id"] != config.environment_id
            || !value["phase_capabilities"].is_object()
            || !value["runtime_profiles"].is_array()
        {
            return Err("entry discovery trust mismatch".into());
        }
        Ok(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{io::Write, net::TcpListener, thread};
    #[test]
    fn discovery_precedes_vault_and_rejects_different_identity_authority() {
        for wrong in [false, true] {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let directory = std::env::temp_dir().join(format!(
                "o7-discovery-{}-{}",
                std::process::id(),
                listener.local_addr().unwrap().port()
            ));
            assert!(!directory.exists());
            let config = IdentityClientConfig {
                endpoint: format!("http://{}", listener.local_addr().unwrap()),
                identity_service_id: "identity".into(),
                environment_id: "test".into(),
                audience: "https://game.invalid".into(),
                state_directory: directory.clone(),
                test_loopback: true,
            };
            let server = thread::spawn(move || {
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(3)))
                    .unwrap();
                let mut b = [0; 4096];
                let n = stream.read(&mut b).unwrap();
                assert!(
                    String::from_utf8_lossy(&b[..n])
                        .starts_with("GET /api/public/hosted-account/v2/entry/discovery ")
                );
                let body = serde_json::to_vec(&serde_json::json!({"api_version":2,"ok":true,"schema_revision":1,
                    "identity":{"service_id":if wrong {"other"} else {"identity"},"environment_id":"test"},
                    "phase_capabilities":{"complete_cold_entry":false},"runtime_profiles":[]})).unwrap();
                write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                )
                .unwrap();
                stream.write_all(&body).unwrap();
            });
            assert_eq!(IdentityClient::discover_entry(&config).is_ok(), !wrong);
            server.join().unwrap();
            assert!(!directory.exists());
        }
    }
}
