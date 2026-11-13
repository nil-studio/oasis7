use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use subtle::ConstantTimeEq;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub version: u32,
    pub generation: u64,
    pub service_id: String,
    pub origin: String,
    pub expires_at_unix: u64,
    pub max_calls: u64,
    pub credentials: Vec<Credential>,
    pub accounts: BTreeMap<String, Account>,
    pub grants: Vec<Grant>,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Credential {
    pub caller_id: String,
    pub token_secret: String,
    pub expires_at_unix: u64,
    pub revoked: bool,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Account {
    pub alias: String,
    pub account_id: String,
    pub provider: String,
    pub environment: String,
    pub credential_secret: String,
    pub credential_version: u64,
    pub disabled: bool,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Grant {
    pub grant_ref: String,
    pub caller_id: String,
    pub account_ref: String,
    pub authorization_version: u64,
    pub expires_at_unix: u64,
    pub enabled: bool,
}
pub fn bounded(s: &str, max: usize) -> bool {
    !s.is_empty() && s.len() <= max && !s.chars().any(char::is_control)
}
pub fn key(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}
fn secret(s: &str) -> bool {
    s != "ACCOUNT_SERVICE_POLICY"
        && s.starts_with("ACCOUNT_SERVICE_")
        && s.len() <= 128
        && s.bytes()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == b'_')
}
impl Config {
    pub fn parse(raw: &str) -> Result<Self, &'static str> {
        let c: Self = oasis7_agent_api::game::parse_bounded_json(raw.as_bytes(), 5120)
            .map_err(|_| "account_configuration_unavailable")?;
        let url = worker::Url::parse(&c.origin).map_err(|_| "account_configuration_unavailable")?;
        if c.version != 1
            || c.generation == 0
            || !bounded(&c.service_id, 128)
            || c.expires_at_unix == 0
            || c.max_calls == 0
            || c.max_calls > 10000
            || url.scheme() != "https"
            || url.host_str().is_none()
            || url.origin().ascii_serialization() != c.origin
            || !url.username().is_empty()
            || url.password().is_some()
            || c.credentials.is_empty()
            || c.credentials.len() > 16
            || c.accounts.len() > 32
            || c.grants.len() > 64
        {
            return Err("account_configuration_unavailable");
        }
        let mut callers = BTreeSet::new();
        let mut tokens = BTreeSet::new();
        for x in &c.credentials {
            if !bounded(&x.caller_id, 128)
                || !secret(&x.token_secret)
                || x.expires_at_unix == 0
                || x.expires_at_unix > c.expires_at_unix
                || !callers.insert(&x.caller_id)
                || !tokens.insert(&x.token_secret)
            {
                return Err("account_configuration_unavailable");
            }
        }
        for (reference, a) in &c.accounts {
            if !bounded(reference, 128)
                || !bounded(&a.alias, 128)
                || !bounded(&a.account_id, 128)
                || !bounded(&a.provider, 128)
                || !bounded(&a.environment, 128)
                || !secret(&a.credential_secret)
                || tokens.contains(&a.credential_secret)
                || a.credential_version == 0
            {
                return Err("account_configuration_unavailable");
            }
        }
        let mut refs = BTreeSet::new();
        for g in &c.grants {
            if !bounded(&g.grant_ref, 128)
                || !callers.contains(&g.caller_id)
                || !c.accounts.contains_key(&g.account_ref)
                || !refs.insert(&g.grant_ref)
                || g.authorization_version == 0
                || g.expires_at_unix == 0
                || g.expires_at_unix > c.expires_at_unix
            {
                return Err("account_configuration_unavailable");
            }
        }
        Ok(c)
    }
    pub fn authenticate<'a>(
        &'a self,
        token: &str,
        now: u64,
        secret: impl Fn(&str) -> Option<String>,
    ) -> Result<&'a str, &'static str> {
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
    pub fn contract(&self, g: &Grant) -> Result<String, &'static str> {
        let a = &self.accounts[&g.account_ref];
        let c = self
            .credentials
            .iter()
            .find(|c| c.caller_id == g.caller_id)
            .ok_or("account_configuration_unavailable")?;
        let raw = serde_json::to_vec(&(
            g,
            c,
            (
                &a.account_id,
                &a.provider,
                &a.environment,
                &a.credential_secret,
                a.credential_version,
                a.disabled,
            ),
        ))
        .map_err(|_| "account_configuration_unavailable")?;
        Ok(blake3::hash(&raw).to_hex().to_string())
    }
}
