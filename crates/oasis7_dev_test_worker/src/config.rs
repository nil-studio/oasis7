//! Private operator configuration. Never serialize this structure into tool replies.
use oasis7_agent_api::game::GameTaskFenceV1;
use oasis7_custody_core::dev_test::Grant;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use subtle::ConstantTimeEq;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Config {
    pub generation: u64,
    pub service_grant_id: String,
    pub origin: String,
    pub expires_at_unix: u64,
    pub max_calls: u64,
    pub timeout_ms: u64,
    pub max_request_bytes: usize,
    pub credentials: Vec<Credential>,
    pub grants: Vec<Grant>,
    #[serde(default)]
    pub history_grants: Vec<String>,
    pub accounts: BTreeMap<String, Account>,
    pub resources: BTreeMap<String, Resource>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Credential {
    pub caller_id: String,
    pub token_secret: String,
    pub expires_at_unix: u64,
    pub revoked: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Account {
    pub alias: String,
    pub account_id: String,
    pub environment: String,
    pub identity_public_key: String,
    pub disabled: bool,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Resource {
    pub account_ref: String,
    pub endpoint: String,
    pub service_public_key: String,
    pub material_secret: String,
    pub fence: GameTaskFenceV1,
}
fn bounded(value: &str, max: usize) -> bool {
    !value.is_empty() && value.len() <= max && !value.chars().any(char::is_control)
}
fn key(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn secret_name(value: &str) -> bool {
    value.starts_with("DEV_TEST_")
        && value.len() <= 128
        && value
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
}
fn https_origin(value: &str) -> bool {
    let Some(authority) = value.strip_prefix("https://") else {
        return false;
    };
    let (host, port) = authority
        .split_once(':')
        .map_or((authority, None), |(h, p)| (h, Some(p)));
    !host.is_empty()
        && host.len() <= 253
        && host.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label
                    .bytes()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
        })
        && port.is_none_or(|p| {
            !p.is_empty()
                && p.bytes().all(|c| c.is_ascii_digit())
                && p.parse::<u16>().is_ok_and(|n| n != 0)
        })
}

impl Config {
    pub fn parse(raw: &str) -> Result<Self, &'static str> {
        if raw.len() > 60 * 1024 {
            return Err("configuration_unavailable");
        }
        let cfg: Self = oasis7_agent_api::game::parse_bounded_json(raw.as_bytes(), 60 * 1024)
            .map_err(|_| "configuration_unavailable")?;
        if cfg.generation == 0
            || !bounded(&cfg.service_grant_id, 128)
            || !https_origin(&cfg.origin)
            || cfg.expires_at_unix == 0
            || cfg.max_calls == 0
            || cfg.max_calls > 10000
            || cfg.timeout_ms == 0
            || cfg.timeout_ms > 30000
            || cfg.max_request_bytes == 0
            || cfg.max_request_bytes > 65536
            || cfg.credentials.is_empty()
            || cfg.credentials.len() > 32
            || cfg.grants.is_empty()
            || cfg.grants.len() > 128
            || cfg.accounts.len() > 64
            || cfg.resources.len() > 128
        {
            return Err("configuration_unavailable");
        }
        if cfg.history_grants.len() > 32
            || cfg
                .history_grants
                .iter()
                .any(|id| !bounded(id, 128) || id == &cfg.service_grant_id)
            || cfg
                .history_grants
                .iter()
                .collect::<std::collections::BTreeSet<_>>()
                .len()
                != cfg.history_grants.len()
        {
            return Err("configuration_unavailable");
        }
        let mut callers = std::collections::BTreeSet::new();
        let mut tokens = std::collections::BTreeSet::new();
        for c in &cfg.credentials {
            if !bounded(&c.caller_id, 128)
                || !secret_name(&c.token_secret)
                || c.expires_at_unix > cfg.expires_at_unix
                || !callers.insert(&c.caller_id)
                || !tokens.insert(&c.token_secret)
            {
                return Err("configuration_unavailable");
            }
        }
        let mut refs = std::collections::BTreeSet::new();
        for g in &cfg.grants {
            let account = cfg
                .accounts
                .get(&g.binding.account_ref)
                .ok_or("configuration_unavailable")?;
            let resource = cfg
                .resources
                .get(&g.binding.resource)
                .ok_or("configuration_unavailable")?;
            resource
                .fence
                .validate()
                .map_err(|_| "configuration_unavailable")?;
            let environment: (String, String) = serde_json::from_str(&account.environment)
                .map_err(|_| "configuration_unavailable")?;
            if g.grant_id != cfg.service_grant_id
                || !callers.contains(&g.caller_id)
                || !refs.insert(&g.grant_ref)
                || g.expires_at_unix > cfg.expires_at_unix
                || g.binding.authorization_version == 0
                || g.binding.environment != account.environment
                || environment.0.is_empty()
                || environment.1.is_empty()
                || resource.account_ref != g.binding.account_ref
                || !https_origin(&resource.endpoint)
                || !key(&resource.service_public_key)
                || !secret_name(&resource.material_secret)
                || !key(&account.identity_public_key)
                || resource
                    .fence
                    .executor_binding
                    .as_ref()
                    .is_none_or(|b| b.executor_public_key == account.identity_public_key)
                || !bounded(&account.account_id, 128)
                || !bounded(&account.alias, 128)
                || g.binding.world != resource.fence.world_id
                || g.binding.genesis_digest != resource.fence.genesis_digest
                || g.binding.agent_id != resource.fence.agent_id
                || g.methods.is_empty()
                || g.methods.iter().any(|m| {
                    ![
                        "describe",
                        "task",
                        "capabilities",
                        "context",
                        "move",
                        "lookup",
                    ]
                    .contains(&m.as_str())
                })
            {
                return Err("configuration_unavailable");
            }
        }
        for g in &cfg.grants {
            let r = &cfg.resources[&g.binding.resource];
            let digest =
                blake3::hash(&serde_json::to_vec(r).map_err(|_| "configuration_unavailable")?)
                    .to_hex()
                    .to_string();
            if g.binding.executor_config_digest != digest {
                return Err("resource_configuration_changed_new_session_required");
            }
        }
        Ok(cfg)
    }
    pub fn authenticate(
        &self,
        token: &str,
        now: u64,
        secret: impl Fn(&str) -> Option<String>,
    ) -> Result<&str, &'static str> {
        if now >= self.expires_at_unix || !key(token) {
            return Err("unauthorized");
        }
        self.credentials
            .iter()
            .find(|c| {
                !c.revoked
                    && now < c.expires_at_unix
                    && secret(&c.token_secret)
                        .filter(|s| key(s))
                        .is_some_and(|s| bool::from(s.as_bytes().ct_eq(token.as_bytes())))
            })
            .map(|c| c.caller_id.as_str())
            .ok_or("unauthorized")
    }
}

/// Account material comes only from an operator-provisioned Worker Secret.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Material {
    pub executor_seed: String,
    pub identity_cookie: String,
}
impl Material {
    pub fn parse(raw: &str) -> Result<Self, &'static str> {
        if raw.len() > 5120 {
            return Err("private_material_unavailable");
        }
        let value: Self = oasis7_agent_api::game::parse_bounded_json(raw.as_bytes(), 5120)
            .map_err(|_| "private_material_unavailable")?;
        if !key(&value.executor_seed)
            || !value.identity_cookie.starts_with("__Host-oasis7_sid=")
            || !bounded(&value.identity_cookie, 4096)
            || value.identity_cookie.contains(';')
        {
            return Err("private_material_unavailable");
        }
        Ok(value)
    }
}
impl Drop for Material {
    fn drop(&mut self) {
        use zeroize::Zeroize;
        self.executor_seed.zeroize();
        self.identity_cookie.zeroize();
    }
}
