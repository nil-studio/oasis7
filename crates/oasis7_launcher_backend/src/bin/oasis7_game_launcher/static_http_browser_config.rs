//! Browser BYOK deployment gates and public trust pins for the Viewer.
//! The issuer seed stays zeroized here; only its public key reaches the config.

pub(super) fn configure_browser_game(config: String) -> Result<String, String> {
    match std::env::var("OASIS7_BROWSER_BYOK_ENABLED").as_deref() {
        Err(std::env::VarError::NotPresent) | Ok("0") => Ok(config),
        Ok("1") => {
            for flag in [
                "OASIS7_HOSTED_PARTICIPANT_ENTRY_ENABLED",
                "OASIS7_HOSTED_OWNER_CONTROL_ENABLED",
                "OASIS7_HOSTED_EXECUTOR_CONTROL_ENABLED",
            ] {
                if std::env::var(flag).as_deref() != Ok("1") {
                    return Err(format!("browser BYOK requires {flag}=1"));
                }
            }
            let trust = std::env::var("OASIS7_HOSTED_PARTICIPANT_ENTRY_TRUST")
                .map_err(|_| "browser BYOK requires participant trust")?;
            let config = super::super::viewer_runtime_config::enable_browser_game(&config, &trust)?;
            let mut value: serde_json::Value =
                serde_json::from_str(&config).map_err(|_| "invalid viewer config")?;
            let required =
                |name| std::env::var(name).map_err(|_| format!("browser BYOK requires {name}"));
            let epoch = required("OASIS7_HOSTED_IDENTITY_INSTANCE_EPOCH")?
                .parse::<u64>()
                .map_err(|_| "invalid browser identity epoch")?;
            if epoch == 0 {
                return Err("invalid browser identity epoch".into());
            }
            let issuer =
                zeroize::Zeroizing::new(required("OASIS7_HOSTED_PARTICIPANT_ISSUER_PRIVATE_KEY")?);
            let seed = zeroize::Zeroizing::new(
                hex::decode(issuer.as_str()).map_err(|_| "invalid browser issuer")?,
            );
            let seed: &[u8; 32] = seed
                .as_slice()
                .try_into()
                .map_err(|_| "invalid browser issuer")?;
            value["browserGameClient"]["participantIssuerPublicKey"] =
                serde_json::json!(hex::encode(
                    ed25519_dalek::SigningKey::from_bytes(seed)
                        .verifying_key()
                        .to_bytes()
                ));
            value["browserGameClient"]["identity"] = serde_json::json!({"service_id":required("OASIS7_HOSTED_IDENTITY_SERVICE_ID")?,"environment_id":required("OASIS7_HOSTED_IDENTITY_ENVIRONMENT")?,"instance_epoch":epoch.to_string()});
            serde_json::to_string(&value).map_err(|_| "invalid viewer config".into())
        }
        _ => Err("OASIS7_BROWSER_BYOK_ENABLED must be 0 or 1".into()),
    }
}
