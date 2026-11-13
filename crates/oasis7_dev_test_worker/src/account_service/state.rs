use super::config::Config;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub method: String,
    pub grant_ref: Option<String>,
    pub session_ref: Option<String>,
    pub expires_at_unix: Option<u64>,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct High {
    version: u64,
    digest: String,
    active: bool,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Session {
    caller: String,
    service_id: String,
    grant_ref: String,
    digest: String,
    issued: u64,
    expires: u64,
    closed: bool,
}
#[derive(Clone, Deserialize, Serialize, Default)]
#[serde(deny_unknown_fields)]
pub struct State {
    version: u32,
    generation: u64,
    policy_digest: String,
    service_id: String,
    max_calls: u64,
    expires: u64,
    retired: BTreeSet<String>,
    grants: BTreeMap<String, High>,
    usage: BTreeMap<String, u64>,
    initialized: BTreeSet<String>,
    sessions: BTreeMap<String, Session>,
}
impl State {
    pub fn load(raw: &str) -> Result<Self, &'static str> {
        if raw.is_empty() {
            return Ok(Self::default());
        }
        let s: Self = oasis7_agent_api::game::parse_bounded_json(raw.as_bytes(), 65536)
            .map_err(|_| "account_state_unknown")?;
        if s.version != 1
            || s.generation == 0
            || !super::config::bounded(&s.service_id, 128)
            || !super::config::key(&s.policy_digest)
            || s.max_calls == 0
            || s.max_calls > 10000
            || s.expires == 0
            || s.retired.len() > 32
            || s.grants.len() > 128
            || s.usage.len() > 128
            || s.sessions.len() > 64
            || s.initialized != s.usage.keys().cloned().collect()
            || s.grants.iter().any(|(k, h)| {
                !super::config::bounded(k, 128) || h.version == 0 || !super::config::key(&h.digest)
            })
            || s.usage.keys().any(|k| !super::config::key(k))
            || s.usage.values().any(|n| *n > 10000)
            || s.sessions.iter().any(|(id, v)| {
                !super::config::key(id)
                    || v.caller.is_empty()
                    || v.expires <= v.issued
                    || !super::config::key(&v.digest)
            })
        {
            return Err("account_state_unknown");
        }
        Ok(s)
    }
    pub fn encode(&self) -> Result<String, &'static str> {
        let raw = serde_json::to_string(self).map_err(|_| "account_state_unknown")?;
        Self::load(&raw)?;
        Ok(raw)
    }
    pub fn reconcile(&mut self, c: &Config, digest: &str) -> Result<(), &'static str> {
        if c.generation < self.generation
            || (c.generation == self.generation && digest != self.policy_digest)
        {
            return Err("account_policy_retired");
        }
        if c.generation == self.generation {
            return Ok(());
        }
        if self.service_id == c.service_id {
            if c.max_calls > self.max_calls || c.expires_at_unix > self.expires {
                return Err("account_budget_expansion_denied");
            }
        } else {
            if self.retired.contains(&c.service_id) {
                return Err("account_service_retired");
            }
            if !self.service_id.is_empty() {
                self.retired.insert(self.service_id.clone());
            }
        }
        let mut active = BTreeSet::new();
        for g in &c.grants {
            let digest = c.contract(g)?;
            if self.grants.get(&g.grant_ref).is_some_and(|old| {
                g.authorization_version < old.version
                    || (g.authorization_version == old.version
                        && (!old.active || digest != old.digest))
            }) {
                return Err("account_grant_retired");
            }
            self.grants.insert(
                g.grant_ref.clone(),
                High {
                    version: g.authorization_version,
                    digest,
                    active: true,
                },
            );
            active.insert(g.grant_ref.clone());
        }
        for (reference, h) in &mut self.grants {
            if !active.contains(reference) {
                h.active = false;
            }
        }
        self.version = 1;
        self.generation = c.generation;
        self.policy_digest = digest.into();
        self.service_id = c.service_id.clone();
        self.max_calls = c.max_calls;
        self.expires = c.expires_at_unix;
        self.encode()?;
        Ok(())
    }
    fn usage_key(c: &Config, caller: &str) -> String {
        blake3::hash(format!("{}:{}:{}", c.service_id.len(), c.service_id, caller).as_bytes())
            .to_hex()
            .to_string()
    }
    fn charge(&mut self, c: &Config, caller: &str) -> Result<(), &'static str> {
        let key = Self::usage_key(c, caller);
        if self.initialized.contains(&key) && !self.usage.contains_key(&key) {
            return Err("account_state_unknown");
        }
        self.initialized.insert(key.clone());
        let count = self.usage.entry(key).or_default();
        if *count >= c.max_calls {
            return Err("call_limit_exhausted");
        }
        *count += 1;
        Ok(())
    }
    fn grant<'a>(
        c: &'a Config,
        caller: &str,
        reference: &str,
        now: u64,
    ) -> Result<&'a super::config::Grant, &'static str> {
        c.grants
            .iter()
            .find(|g| {
                g.grant_ref == reference
                    && g.caller_id == caller
                    && g.enabled
                    && now < g.expires_at_unix
                    && !c.accounts[&g.account_ref].disabled
            })
            .ok_or("grant_unavailable")
    }
    fn account(c: &Config, g: &super::config::Grant) -> Value {
        let a = &c.accounts[&g.account_ref];
        json!({"grant_ref":g.grant_ref,"account_ref":g.account_ref,"account_id":a.account_id,"alias":a.alias,"provider":a.provider,"environment":a.environment,"credential_version":a.credential_version})
    }
    pub fn dispatch(
        &mut self,
        c: &Config,
        caller: &str,
        r: &Request,
        now: u64,
        nonce: &str,
        material: impl Fn(&str) -> bool,
    ) -> Result<Value, &'static str> {
        let mut next = self.clone();
        let value = next.apply(c, caller, r, now, nonce, material)?;
        next.encode()?;
        *self = next;
        Ok(
            json!({"result":value,"usage":{"used":self.usage.get(&Self::usage_key(c,caller)).copied().unwrap_or(0),"limit":c.max_calls}}),
        )
    }
    fn apply(
        &mut self,
        c: &Config,
        caller: &str,
        r: &Request,
        now: u64,
        nonce: &str,
        material: impl Fn(&str) -> bool,
    ) -> Result<Value, &'static str> {
        if now >= c.expires_at_unix {
            return Err("unauthorized");
        }
        match r.method.as_str() {
            "accounts" => {
                if r.grant_ref.is_some() || r.session_ref.is_some() || r.expires_at_unix.is_some() {
                    return Err("invalid_arguments");
                }
                let accounts: Vec<_> = c
                    .grants
                    .iter()
                    .filter(|g| Self::grant(c, caller, &g.grant_ref, now).is_ok())
                    .map(|g| Self::account(c, g))
                    .collect();
                self.charge(c, caller)?;
                Ok(json!({"accounts":accounts}))
            }
            "open" => {
                if r.session_ref.is_some() {
                    return Err("invalid_arguments");
                }
                let g = Self::grant(
                    c,
                    caller,
                    r.grant_ref.as_deref().ok_or("grant_unavailable")?,
                    now,
                )?;
                let expires = r.expires_at_unix.ok_or("invalid_expiry")?;
                let credential = c
                    .credentials
                    .iter()
                    .find(|v| v.caller_id == caller)
                    .ok_or("unauthorized")?;
                if expires <= now
                    || expires > g.expires_at_unix
                    || expires > credential.expires_at_unix
                {
                    return Err("invalid_expiry");
                }
                if !material(&c.accounts[&g.account_ref].credential_secret) {
                    return Err("credential_unavailable");
                }
                if self.sessions.len() >= 64 {
                    return Err("session_capacity_exhausted");
                }
                if !super::config::key(nonce) || self.sessions.contains_key(nonce) {
                    return Err("session_unavailable");
                }
                self.charge(c, caller)?;
                self.sessions.insert(
                    nonce.into(),
                    Session {
                        caller: caller.into(),
                        service_id: c.service_id.clone(),
                        grant_ref: g.grant_ref.clone(),
                        digest: c.contract(g)?,
                        issued: now,
                        expires,
                        closed: false,
                    },
                );
                Ok(
                    json!({"session_ref":nonce,"account":Self::account(c,g),"expires_at_unix":expires}),
                )
            }
            "describe" | "close" => {
                if r.grant_ref.is_some() || r.expires_at_unix.is_some() {
                    return Err("invalid_arguments");
                }
                let reference = r.session_ref.as_deref().ok_or("session_unavailable")?;
                let s = self.sessions.get(reference).ok_or("session_unavailable")?;
                if s.caller != caller
                    || s.service_id != c.service_id
                    || s.closed
                    || now < s.issued
                    || now >= s.expires
                {
                    return Err("session_unavailable");
                }
                let g = Self::grant(c, caller, &s.grant_ref, now)?;
                if s.digest != c.contract(g)? {
                    return Err("session_scope_denied");
                }
                if r.method == "close" {
                    self.sessions
                        .get_mut(reference)
                        .ok_or("session_unavailable")?
                        .closed = true;
                    Ok(json!({"closed":true}))
                } else {
                    let value = Self::account(c, g);
                    self.charge(c, caller)?;
                    Ok(value)
                }
            }
            _ => Err("method_unavailable"),
        }
    }
}
