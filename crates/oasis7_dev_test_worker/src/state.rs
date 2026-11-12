//! Ledger and initialization marker commit together before any native network send.
use crate::config::Config;
#[cfg(target_arch = "wasm32")]
use crate::store;
use oasis7_custody_core::dev_test::{Ledger, Original, usage_key};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::JsValue;
#[derive(Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Meta {
    version: u32,
    generation: u64,
    policy_digest: String,
    active_grant_id: String,
    retired_grant_ids: BTreeSet<String>,
    initialized: BTreeSet<String>,
    grant_versions: BTreeMap<String, u64>,
    active_contracts: BTreeMap<String, String>,
}
fn unavailable<T>(_: T) -> &'static str {
    "persistent_state_unknown"
}
fn decode_meta(raw: &str) -> Result<Meta, &'static str> {
    if raw.is_empty() {
        return Ok(Meta {
            version: 1,
            ..Meta::default()
        });
    }
    let meta: Meta = serde_json::from_str(raw).map_err(unavailable)?;
    if meta.version != 1
        || meta.initialized.len() > 4096
        || meta.grant_versions.len() > 4096
        || meta.active_contracts.len() > 128
        || meta.retired_grant_ids.len() > 4096
    {
        return Err("persistent_state_unknown");
    }
    Ok(meta)
}
fn contract(
    config: &Config,
    grant: &oasis7_custody_core::dev_test::Grant,
) -> Result<String, &'static str> {
    let credential = config
        .credentials
        .iter()
        .find(|c| c.caller_id == grant.caller_id)
        .ok_or("persistent_state_unknown")?;
    let account = config
        .accounts
        .get(&grant.binding.account_ref)
        .ok_or("persistent_state_unknown")?;
    let bytes = serde_json::to_vec(&(
        grant,
        config.max_calls,
        config.expires_at_unix,
        &config.history_grants,
        (
            &credential.caller_id,
            &credential.token_secret,
            credential.expires_at_unix,
            credential.revoked,
        ),
        (
            &account.account_id,
            &account.environment,
            &account.identity_public_key,
            account.disabled,
        ),
    ))
    .map_err(unavailable)?;
    Ok(blake3::hash(&bytes).to_hex().to_string())
}
/// Pure transition: callers persist the complete result with compare-and-swap.
fn reconcile_meta(raw: &str, config: &Config, digest: &str) -> Result<String, &'static str> {
    let mut meta = decode_meta(raw)?;
    if config.generation < meta.generation
        || (config.generation == meta.generation && digest != meta.policy_digest)
    {
        return Err("policy_generation_retired");
    }
    if config.generation == meta.generation {
        return Ok(raw.into());
    }
    if meta.active_grant_id != config.service_grant_id {
        if meta.retired_grant_ids.contains(&config.service_grant_id) {
            return Err("restored_authorization_permanently_retired");
        }
        if !meta.active_grant_id.is_empty() {
            meta.retired_grant_ids.insert(meta.active_grant_id);
        }
        meta.active_grant_id = config.service_grant_id.clone();
    }
    let mut active = BTreeMap::new();
    for grant in &config.grants {
        let key = grant.grant_ref.clone();
        let version = grant.binding.authorization_version;
        let fingerprint = contract(config, grant)?;
        if let Some(high) = meta.grant_versions.get(&key)
            && (version < *high
                || (version == *high && meta.active_contracts.get(&key) != Some(&fingerprint)))
        {
            return Err("grant_generation_retired");
        }
        if active.insert(key.clone(), fingerprint).is_some() {
            return Err("persistent_state_unknown");
        }
        meta.grant_versions.insert(key, version);
    }
    meta.active_contracts = active;
    meta.generation = config.generation;
    meta.policy_digest = digest.into();
    // Bound retained tombstones too: capacity never silently evicts history.
    let encoded = serde_json::to_string(&meta).map_err(unavailable)?;
    decode_meta(&encoded)?;
    Ok(encoded)
}
fn decode_ledger(meta: &Meta, key: &str, raw: &str) -> Result<Ledger, &'static str> {
    if raw.is_empty() {
        if meta.initialized.contains(key) {
            return Err("usage_record_missing_unknown");
        }
        return Ok(Ledger {
            version: 1,
            ..Ledger::default()
        });
    }
    if !meta.initialized.contains(key) {
        return Err("persistent_state_unknown");
    }
    let ledger: Ledger = serde_json::from_str(raw).map_err(unavailable)?;
    if ledger.version != 1 || ledger.operations.len() > 4096 {
        return Err("unsupported_usage_record");
    }
    Ok(ledger)
}
/// Query-only history reads never initialize, debit, or rewrite retired usage.
fn history_original(
    meta: &Meta,
    config: &Config,
    caller: &str,
    id: &str,
    mut load: impl FnMut(&str) -> Result<String, &'static str>,
) -> Result<Option<Original>, &'static str> {
    if !oasis7_custody_core::dev_test::valid_id(id) {
        return Err("invalid_request_id");
    }
    let mut found: Option<(Vec<u8>, Original)> = None;
    for grant in &config.history_grants {
        if grant == &config.service_grant_id || !meta.retired_grant_ids.contains(grant) {
            return Err("history_authorization_denied");
        }
        let key = usage_key(grant, caller);
        if !meta.initialized.contains(&key) {
            return Err("historical_usage_unknown");
        }
        let ledger = decode_ledger(meta, &key, &load(&key)?)?;
        if let Some(original) = ledger.operations.get(id) {
            let identity = serde_json::to_vec(&(
                &original.body_digest,
                &original.scope,
                &original.arguments,
                &original.native_context,
                &original.native_kind,
                &original.native_action,
                &original.native_context_identity,
            ))
            .map_err(unavailable)?;
            if found.as_ref().is_some_and(|(prior, _)| prior != &identity) {
                return Err("historical_original_ambiguous");
            }
            let mut original = original.clone();
            original.historical_lookup_only = true;
            found = Some((identity, original));
        }
    }
    Ok(found.map(|(_, original)| original))
}
#[cfg(target_arch = "wasm32")]
pub(crate) struct State<'a> {
    pub raw: &'a JsValue,
    pub config: &'a Config,
    pub policy_digest: &'a str,
}
#[cfg(target_arch = "wasm32")]
impl State<'_> {
    fn meta(&self) -> Result<(String, Meta), &'static str> {
        let raw = store::load(self.raw, "policy-meta").map_err(unavailable)?;
        let meta = decode_meta(&raw)?;
        Ok((raw, meta))
    }
    /// Called only after caller authentication. A policy generation cannot roll back.
    pub fn reconcile(&self) -> Result<(), &'static str> {
        let (raw, _) = self.meta()?;
        let next = reconcile_meta(&raw, self.config, self.policy_digest)?;
        if next == raw {
            return Ok(());
        }
        store::commit(self.raw, "policy-meta", &raw, &next).map_err(unavailable)
    }
    pub fn transact<T>(
        &self,
        caller: &str,
        update: impl FnOnce(&mut Ledger) -> Result<T, &'static str>,
    ) -> Result<T, &'static str> {
        let (meta_raw, mut meta) = self.meta()?;
        if meta.generation != self.config.generation
            || meta.policy_digest != self.policy_digest
            || meta.active_grant_id != self.config.service_grant_id
        {
            return Err("policy_generation_retired");
        }
        let key = usage_key(&self.config.service_grant_id, caller);
        let raw = store::load(self.raw, &key).map_err(|_| "persistent_state_unknown")?;
        let mut ledger = decode_ledger(&meta, &key, &raw)?;
        let result = update(&mut ledger)?;
        meta.initialized.insert(key.clone());
        if meta.initialized.len() > 4096 || ledger.operations.len() > 4096 {
            return Err("persistent_state_capacity_exhausted");
        }
        store::commit_ledger(
            self.raw,
            &meta_raw,
            &serde_json::to_string(&meta).map_err(unavailable)?,
            &key,
            &raw,
            &serde_json::to_string(&ledger).map_err(unavailable)?,
        )
        .map_err(|_| "persistent_state_unknown")?;
        Ok(result)
    }
    pub fn history(&self, caller: &str, id: &str) -> Result<Option<Original>, &'static str> {
        let (_, meta) = self.meta()?;
        if meta.generation != self.config.generation
            || meta.policy_digest != self.policy_digest
            || meta.active_grant_id != self.config.service_grant_id
        {
            return Err("policy_generation_retired");
        }
        history_original(&meta, self.config, caller, id, |key| {
            store::load(self.raw, key).map_err(unavailable)
        })
    }
    pub fn charge_read(&self, caller: &str) -> Result<(), &'static str> {
        self.transact(caller, |ledger| {
            if ledger.used >= self.config.max_calls {
                return Err("call_limit_exhausted");
            }
            ledger.used += 1;
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn config() -> Config {
        serde_json::from_value(serde_json::json!({
            "generation":1,"service_grant_id":"service","origin":"https://example.test",
            "expires_at_unix":1000,"max_calls":10,"timeout_ms":1000,"max_request_bytes":4096,
            "credentials":[{"caller_id":"caller","token_secret":"DEV_TEST_TOKEN","expires_at_unix":1000,"revoked":false}],
            "accounts":{"account":{"alias":"A","account_id":"identity","environment":"env","identity_public_key":"key","disabled":false}},
            "resources":{},"grants":[{"grant_ref":"grant","grant_id":"service","caller_id":"caller",
              "binding":{"account_ref":"account","environment":"env","world":"world","genesis_digest":"genesis","agent_id":"agent","resource":"resource","executor_config_digest":"digest","authorization_version":1},
              "expires_at_unix":900,"enabled":true,"methods":["lookup"],"move_targets":[]}]
        })).unwrap()
    }
    #[test]
    fn policy_generation_and_same_version_contract_are_monotonic() {
        let mut c = config();
        let initial = reconcile_meta("", &c, "one").unwrap();
        assert_eq!(reconcile_meta(&initial, &c, "one").unwrap(), initial);
        assert_eq!(
            reconcile_meta(&initial, &c, "different").unwrap_err(),
            "policy_generation_retired"
        );
        c.generation = 2;
        assert!(reconcile_meta(&initial, &c, "two").is_ok());
        for field in 0..7 {
            let mut changed = config();
            changed.generation = 2;
            match field {
                0 => changed.grants[0].methods.push("move".into()),
                1 => changed.grants[0].expires_at_unix += 1,
                2 => changed.grants[0].binding.agent_id = "other".into(),
                3 => changed.grants[0].move_targets.push([1, 2, 3]),
                4 => changed.credentials[0].revoked = true,
                5 => changed.max_calls += 1,
                _ => changed.accounts.get_mut("account").unwrap().disabled = true,
            }
            assert_eq!(
                reconcile_meta(&initial, &changed, "changed").unwrap_err(),
                "grant_generation_retired"
            );
            changed.grants[0].binding.authorization_version = 2;
            assert!(reconcile_meta(&initial, &changed, "changed").is_ok());
        }
    }
    #[test]
    fn removed_readded_grant_requires_strictly_new_version_even_with_new_caller() {
        let mut c = config();
        let first = reconcile_meta("", &c, "one").unwrap();
        c.generation = 2;
        c.grants.clear();
        let removed = reconcile_meta(&first, &c, "two").unwrap();
        c = config();
        c.generation = 3;
        assert_eq!(
            reconcile_meta(&removed, &c, "three").unwrap_err(),
            "grant_generation_retired"
        );
        c.grants[0].caller_id = "new-caller".into();
        c.credentials[0].caller_id = "new-caller".into();
        assert_eq!(
            reconcile_meta(&removed, &c, "three").unwrap_err(),
            "grant_generation_retired"
        );
        c.grants[0].binding.authorization_version = 2;
        let restored = reconcile_meta(&removed, &c, "three").unwrap();
        c.generation = 4;
        c.grants[0].binding.authorization_version = 1;
        assert_eq!(
            reconcile_meta(&restored, &c, "four").unwrap_err(),
            "grant_generation_retired"
        );
    }
    #[test]
    fn retired_service_grant_never_reactivates() {
        let mut c = config();
        let first = reconcile_meta("", &c, "one").unwrap();
        c.generation = 2;
        c.service_grant_id = "new".into();
        c.grants[0].grant_id = "new".into();
        c.grants[0].binding.authorization_version = 2;
        let second = reconcile_meta(&first, &c, "two").unwrap();
        c.generation = 3;
        c.service_grant_id = "service".into();
        assert_eq!(
            reconcile_meta(&second, &c, "three").unwrap_err(),
            "restored_authorization_permanently_retired"
        );
    }
    #[test]
    fn deleted_ledger_or_marker_is_unknown_not_fresh_budget() {
        let mut meta = decode_meta("").unwrap();
        assert_eq!(decode_ledger(&meta, "usage", "").unwrap().used, 0);
        let encoded = serde_json::to_string(&Ledger {
            version: 1,
            used: 7,
            ..Ledger::default()
        })
        .unwrap();
        assert!(decode_ledger(&meta, "usage", &encoded).is_err());
        meta.initialized.insert("usage".into());
        assert_eq!(decode_ledger(&meta, "usage", &encoded).unwrap().used, 7);
        for _ in 0..2 {
            assert!(decode_ledger(&meta, "usage", "").is_err());
        }
        assert!(decode_meta("{}").is_err());
    }
    #[test]
    fn explicit_retired_history_is_query_only_and_never_resets_usage() {
        use oasis7_custody_core::dev_test::{Arguments, Call, Scope};
        let mut c = config();
        c.history_grants = vec!["retired".into()];
        let key = usage_key("retired", "caller");
        let mut meta = decode_meta("").unwrap();
        meta.retired_grant_ids.insert("retired".into());
        meta.initialized.insert(key.clone());
        let scope = Scope {
            account_ref: "a".into(),
            environment: "e".into(),
            world: "w".into(),
            genesis_digest: "g".into(),
            agent: "agent".into(),
            resource: "r".into(),
            executor: "x".into(),
            fence_digest: "f".into(),
        };
        let mut ledger = Ledger {
            version: 1,
            ..Ledger::default()
        };
        ledger
            .admit(
                "retired",
                "caller",
                &scope,
                &Call {
                    request_id: "original".into(),
                    arguments: Arguments::Context,
                },
                10,
                |_| Ok(None),
            )
            .unwrap();
        let raw = serde_json::to_string(&ledger).unwrap();
        let original = history_original(&meta, &c, "caller", "original", |k| {
            assert_eq!(k, key);
            Ok(raw.clone())
        })
        .unwrap()
        .unwrap();
        assert!(original.historical_lookup_only);
        assert_eq!(ledger.used, 1);
        assert!(
            history_original(&meta, &c, "other", "original", |_| panic!(
                "missing marker must not load"
            ))
            .is_err()
        );
        assert!(history_original(&meta, &c, "caller", "original", |_| Ok(String::new())).is_err());
        meta.retired_grant_ids.clear();
        assert!(
            history_original(&meta, &c, "caller", "original", |_| panic!(
                "unretired must not load"
            ))
            .is_err()
        );
        c.history_grants.clear();
        assert!(
            history_original(&meta, &c, "caller", "original", |_| panic!(
                "no allowlist must not load"
            ))
            .unwrap()
            .is_none()
        );
    }
    #[test]
    fn ambiguous_historical_originals_and_same_version_history_expansion_are_rejected() {
        use oasis7_custody_core::dev_test::{Arguments, Call, Scope};
        let mut c = config();
        let initial = reconcile_meta("", &c, "one").unwrap();
        c.generation = 2;
        c.history_grants = vec!["old-a".into(), "old-b".into()];
        assert_eq!(
            reconcile_meta(&initial, &c, "two").unwrap_err(),
            "grant_generation_retired"
        );
        let mut meta = decode_meta("").unwrap();
        for g in &c.history_grants {
            meta.retired_grant_ids.insert(g.clone());
            meta.initialized.insert(usage_key(g, "caller"));
        }
        let scope = Scope {
            account_ref: "a".into(),
            environment: "e".into(),
            world: "w".into(),
            genesis_digest: "g".into(),
            agent: "agent".into(),
            resource: "r".into(),
            executor: "x".into(),
            fence_digest: "f".into(),
        };
        let mut ledger = Ledger {
            version: 1,
            ..Ledger::default()
        };
        ledger
            .admit(
                "old",
                "caller",
                &scope,
                &Call {
                    request_id: "rid".into(),
                    arguments: Arguments::Context,
                },
                10,
                |_| Ok(None),
            )
            .unwrap();
        let first = serde_json::to_string(&ledger).unwrap();
        ledger.operations.get_mut("rid").unwrap().scope.agent = "other".into();
        let second = serde_json::to_string(&ledger).unwrap();
        assert_eq!(
            history_original(&meta, &c, "caller", "rid", |key| Ok(
                if key == usage_key("old-a", "caller") {
                    first.clone()
                } else {
                    second.clone()
                }
            ))
            .unwrap_err(),
            "historical_original_ambiguous"
        );
    }
}
