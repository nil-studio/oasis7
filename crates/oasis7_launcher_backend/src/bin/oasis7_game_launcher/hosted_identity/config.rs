use super::model::{IdentityNamespace, RpConfig};
use std::{env, path::PathBuf, time::Duration};

pub(super) fn runtime_control_timeout() -> Result<Duration, String> {
    match env::var("OASIS7_WORLD_SERVICE_TIMEOUT_MS") {
        Ok(value) => value
            .bytes()
            .all(|byte| byte.is_ascii_digit())
            .then(|| value.parse::<u64>().ok())
            .flatten()
            .filter(|value| (100..=30_000).contains(value))
            .map(Duration::from_millis)
            .ok_or_else(|| {
                "OASIS7_WORLD_SERVICE_TIMEOUT_MS must be an integer from 100 to 30000".into()
            }),
        Err(env::VarError::NotPresent) => Ok(Duration::from_secs(2)),
        Err(_) => Err("OASIS7_WORLD_SERVICE_TIMEOUT_MS is not valid Unicode".into()),
    }
}

pub(crate) struct IdentityConfig {
    pub namespace: IdentityNamespace,
    pub rp: RpConfig,
    pub sqlite_path: PathBuf,
    pub registration_enabled: bool,
    pub machine_registration_enabled: bool,
    pub session_key: [u8; 32],
    pub world_scope: String,
    pub runtime_live_bind: String,
    pub tablestore: Option<super::store::tablestore::TablestoreConfig>,
    pub trusted_proxy_loopback: bool,
}

impl IdentityConfig {
    pub fn from_env() -> Result<Option<Self>, String> {
        if !oasis7_client_api::viewer_auth::hosted_identity_v2_enabled_checked()? {
            const ROLLBACK_BOUNDARY: &str = "identity v2 storage prevents legacy startup; keep identity v2 enabled and close registration instead";
            if let Some(path) =
                env::var_os("OASIS7_HOSTED_IDENTITY_SQLITE_PATH").filter(|path| !path.is_empty())
            {
                match PathBuf::from(path).try_exists() {
                    Ok(false) => {}
                    Ok(true) | Err(_) => return Err(ROLLBACK_BOUNDARY.into()),
                }
            }
            if env::var_os("OASIS7_HOSTED_IDENTITY_TABLE_V2").is_some_and(|table| {
                table
                    .to_str()
                    .map(|value| !value.trim().is_empty())
                    .unwrap_or(true)
            }) {
                return Err(ROLLBACK_BOUNDARY.into());
            }
            return Ok(None);
        }
        runtime_control_timeout()?;
        fn required(key: &str) -> Result<String, String> {
            env::var(key)
                .ok()
                .filter(|v| !v.trim().is_empty())
                .ok_or_else(|| format!("{key} is required for identity v2"))
        }
        let backend =
            env::var("OASIS7_HOSTED_ACCOUNT_STORE_BACKEND").unwrap_or_else(|_| "file".into());
        let tablestore = match backend.as_str() {
            "file" => None,
            "tablestore" => Some(super::store::tablestore::TablestoreConfig {
                endpoint: required("OASIS7_HOSTED_ACCOUNT_TABLESTORE_ENDPOINT")?,
                instance: required("OASIS7_HOSTED_IDENTITY_TABLESTORE_INSTANCE")?,
                access_key_id: required("OASIS7_HOSTED_ACCOUNT_TABLESTORE_AK_ID")?,
                access_key_secret: required("OASIS7_HOSTED_ACCOUNT_TABLESTORE_AK_SECRET")?,
                sts_token: env::var("OASIS7_HOSTED_ACCOUNT_TABLESTORE_STS_TOKEN").ok(),
                table: required("OASIS7_HOSTED_IDENTITY_TABLE_V2")?,
                initialize: env::var("OASIS7_HOSTED_IDENTITY_TABLE_V2_INITIALIZE")
                    .ok()
                    .as_deref()
                    == Some("1"),
            }),
            _ => return Err("unsupported identity store backend".into()),
        };
        if env::var("OASIS7_HOSTED_NATIVE_KEY_ENABLED").ok().as_deref() == Some("1") {
            return Err("native identity keys are not enabled in P0".into());
        }
        let rp_id = required("OASIS7_HOSTED_WEBAUTHN_RP_ID")?;
        let allowed_origins: Vec<String> = required("OASIS7_HOSTED_WEBAUTHN_ORIGINS")?
            .split(',')
            .map(|v| v.trim().to_owned())
            .collect();
        for origin in &allowed_origins {
            let url = url::Url::parse(origin).map_err(|_| "invalid identity origin")?;
            if url.origin().ascii_serialization() != *origin
                || url.scheme() != "https"
                || !url.username().is_empty()
                || url.password().is_some()
            {
                return Err("identity origins must be exact HTTPS origins".into());
            }
            if url.host_str() != Some(rp_id.as_str()) {
                return Err("identity RP must equal the configured origin host".into());
            }
        }
        let session_key = hex::decode(required("OASIS7_HOSTED_IDENTITY_SESSION_KEY")?)
            .map_err(|_| "invalid identity session key")?
            .try_into()
            .map_err(|_| "identity session key must contain 32 bytes")?;
        let epoch = required("OASIS7_HOSTED_IDENTITY_INSTANCE_EPOCH")?
            .parse::<u64>()
            .map_err(|_| "invalid identity instance epoch")?;
        if epoch == 0 {
            return Err("identity instance epoch must be positive".into());
        }
        Ok(Some(Self {
            namespace: IdentityNamespace {
                identity_service_id: required("OASIS7_HOSTED_IDENTITY_SERVICE_ID")?,
                environment_id: required("OASIS7_HOSTED_IDENTITY_ENVIRONMENT")?,
            },
            rp: RpConfig {
                rp_id,
                allowed_origins,
                version: 1,
                identity_instance_epoch: epoch,
            },
            sqlite_path: if tablestore.is_none() {
                PathBuf::from(required("OASIS7_HOSTED_IDENTITY_SQLITE_PATH")?)
            } else {
                PathBuf::new()
            },
            tablestore,
            trusted_proxy_loopback: env::var("OASIS7_HOSTED_IDENTITY_TRUST_LOOPBACK_PROXY")
                .ok()
                .as_deref()
                == Some("1"),
            registration_enabled: env::var("OASIS7_HOSTED_ACCOUNT_REGISTRATION_ENABLED")
                .ok()
                .as_deref()
                == Some("1"),
            machine_registration_enabled: match env::var(
                "OASIS7_HOSTED_MACHINE_REGISTRATION_ENABLED",
            ) {
                Err(env::VarError::NotPresent) => false,
                Ok(value) if value == "0" => false,
                Ok(value) if value == "1" => true,
                _ => return Err("OASIS7_HOSTED_MACHINE_REGISTRATION_ENABLED must be 0 or 1".into()),
            },
            session_key,
            world_scope: required("OASIS7_HOSTED_WORLD_IDENTITY_SCOPE")?,
            runtime_live_bind: env::var("OASIS7_HOSTED_RUNTIME_LIVE_BIND").unwrap_or_default(),
        }))
    }
}

#[cfg(test)]
#[path = "config_tests.rs"]
mod config_tests;
