//! Immutable sessions. References do not authenticate callers.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Binding {
    pub account_ref: String,
    pub environment: String,
    pub world: String,
    pub genesis_digest: String,
    pub agent_id: String,
    pub resource: String,
    pub executor_config_digest: String,
    pub authorization_version: u64,
}
#[derive(Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Grant {
    pub grant_ref: String,
    pub grant_id: String,
    pub caller_id: String,
    pub binding: Binding,
    pub expires_at_unix: u64,
    pub enabled: bool,
    pub methods: Vec<String>,
    #[serde(default)]
    pub move_targets: Vec<[i64; 3]>,
}
struct Session {
    grant_ref: String,
    grant_id: String,
    caller_id: String,
    binding: Binding,
    expires: u64,
    methods: Vec<String>,
    move_targets: Vec<[i64; 3]>,
}
pub struct Sessions {
    epoch: String,
    next: u64,
    sessions: BTreeMap<String, Session>,
}
impl Sessions {
    pub fn with_epoch(epoch: String) -> Result<Self, String> {
        if epoch.len() != 64
            || !epoch
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err("session_entropy_unavailable".into());
        }
        Ok(Self {
            epoch,
            next: 0,
            sessions: BTreeMap::new(),
        })
    }
    pub fn epoch(&self) -> &str {
        &self.epoch
    }
    /// Metadata only: consumers must still call authorize with current policy.
    pub fn policy_reference(&self, reference: &str) -> Result<(&str, u64), String> {
        let session = self.sessions.get(reference).ok_or("session_unavailable")?;
        Ok((&session.grant_ref, session.expires))
    }
    pub fn open(
        &mut self,
        caller: &str,
        grant: &Grant,
        expires: u64,
        now: u64,
    ) -> Result<String, String> {
        if caller != grant.caller_id
            || !grant.enabled
            || expires <= now
            || expires > grant.expires_at_unix
            || grant.methods.is_empty()
            || grant.methods.iter().any(|m| {
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
            return Err("session_scope_denied".into());
        }
        self.next = self
            .next
            .checked_add(1)
            .ok_or("session_capacity_exhausted")?;
        let reference = format!("{}-{}", self.epoch, self.next);
        self.sessions.insert(
            reference.clone(),
            Session {
                grant_ref: grant.grant_ref.clone(),
                grant_id: grant.grant_id.clone(),
                caller_id: caller.into(),
                binding: grant.binding.clone(),
                expires,
                methods: grant.methods.clone(),
                move_targets: grant.move_targets.clone(),
            },
        );
        Ok(reference)
    }
    pub fn authorize(
        &self,
        reference: &str,
        caller: &str,
        current: &Grant,
        method: &str,
        now: u64,
    ) -> Result<&Binding, String> {
        let session = self.sessions.get(reference).ok_or("session_unavailable")?;
        if caller != session.caller_id
            || caller != current.caller_id
            || session.grant_ref != current.grant_ref
            || session.grant_id != current.grant_id
            || session.binding != current.binding
            || session.move_targets != current.move_targets
            || !current.enabled
            || now >= session.expires
            || now >= current.expires_at_unix
            || !session.methods.iter().any(|m| m == method)
            || !current.methods.iter().any(|m| m == method)
        {
            return Err("session_scope_denied".into());
        }
        Ok(&session.binding)
    }
    pub fn close(&mut self, reference: &str, caller: &str) -> Result<(), String> {
        if self
            .sessions
            .get(reference)
            .is_none_or(|s| s.caller_id != caller)
        {
            return Err("session_unavailable".into());
        }
        self.sessions.remove(reference);
        Ok(())
    }
}
