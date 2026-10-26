//! Role-local encrypted immutable slot component. No production caller or authority is installed.
//! A trusted reader must establish real source, approval and fence state before integration.
use crate::envelope_crypto;
use ed25519_dalek::SigningKey;
use js_sys::{Array, Reflect};
use oasis7_custody_core::{
    DecimalU64,
    http::Role,
    provisioning::PublicRoleKey,
    role_key_slot::{RoleKeySlotScopeV1, RoleKeySlotV1},
};
use serde::{Deserialize, Serialize};
use wasm_bindgen::{JsCast, prelude::*};
use web_sys::Crypto;
use zeroize::Zeroizing;

const KEY: &str = "custody-role-key-slot-v1";
const MAX_BYTES: usize = 32 * 1024;
type Result<T> = std::result::Result<T, JsValue>;

#[cfg(feature = "identity")]
const COMPILED_ROLE: Role = Role::Identity;
#[cfg(feature = "owner")]
const COMPILED_ROLE: Role = Role::Owner;
#[cfg(feature = "executor")]
const COMPILED_ROLE: Role = Role::Executor;

/// Trusted Rust-only current reads. This is not a deserializable grant or a
/// substitute for the production source/ACL/fence readers, which are not wired.
#[derive(Clone, PartialEq, Eq)]
pub struct RoleKeyCurrent {
    pub scope: RoleKeySlotScopeV1,
    pub approved: bool,
    pub frozen: bool,
    pub cancelled: bool,
    pub expires_at: DecimalU64,
    /// Canonical digest of the trusted approval/source/fence revision.
    pub authority_revision: String,
}
impl RoleKeyCurrent {
    fn check(&self, scope: &RoleKeySlotScopeV1, now: u64) -> Result<()> {
        scope.validate().map_err(|_| rejected())?;
        if scope.context.identity.role != COMPILED_ROLE
            || self.scope != *scope
            || !self.approved
            || self.frozen
            || self.cancelled
            || self.expires_at.0 <= now
            || self.authority_revision.len() != 64
            || !self
                .authority_revision
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(rejected());
        }
        Ok(())
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Snapshot {
    schema_version: u8,
    slot: Option<RoleKeySlotV1>,
    audit_sequence: DecimalU64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Inventory {
    records: Vec<Row>,
    table_names: Vec<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Row {
    id: String,
    payload: String,
}
fn rejected() -> JsValue {
    JsValue::from_str("role key slot rejected")
}
fn decode(raw: &str) -> Result<Snapshot> {
    if raw.len() > MAX_BYTES {
        return Err(rejected());
    }
    let saved: Snapshot = serde_json::from_str(raw).map_err(|_| rejected())?;
    if saved.schema_version != 1
        || !matches!(
            (&saved.slot, saved.audit_sequence.0),
            (None, 0) | (Some(_), 1)
        )
    {
        return Err(rejected());
    }
    if let Some(slot) = &saved.slot {
        slot.validate().map_err(|_| rejected())?;
    }
    Ok(saved)
}
fn encode(saved: &Snapshot) -> Result<String> {
    let value = serde_json::to_string(saved).map_err(|_| rejected())?;
    if value.len() > MAX_BYTES {
        return Err(rejected());
    }
    Ok(value)
}
fn inventory(raw: &str) -> Result<()> {
    let inventory: Inventory = serde_json::from_str(raw).map_err(|_| rejected())?;
    if inventory.records.len() != 1
        || inventory.records[0].id != KEY
        || inventory.records[0].payload.len() > MAX_BYTES
        || inventory.table_names.iter().any(|name| {
            !matches!(
                name.as_str(),
                "table:custody_records"
                    | "table:custody_audit"
                    | "table:sqlite_sequence"
                    | "table:_cf_METADATA"
            )
        })
    {
        return Err(rejected());
    }
    Ok(())
}
fn now() -> Result<u64> {
    let value = js_sys::Date::now();
    if !value.is_finite() || value < 0.0 || value.fract() != 0.0 || value > 9_007_199_254_740_991.0
    {
        return Err(rejected());
    }
    Ok(value as u64)
}
fn public(seed: &[u8]) -> Result<String> {
    let seed: &[u8; 32] = seed.try_into().map_err(|_| rejected())?;
    let key = SigningKey::from_bytes(seed);
    Ok(hex::encode(key.verifying_key().to_bytes()))
}
fn random_seed() -> Result<Zeroizing<[u8; 32]>> {
    let crypto: Crypto = Reflect::get(&js_sys::global(), &"crypto".into())
        .map_err(|_| rejected())?
        .dyn_into()
        .map_err(|_| rejected())?;
    let mut seed = Zeroizing::new([0; 32]);
    crypto
        .get_random_values_with_u8_array(seed.as_mut_slice())
        .map_err(|_| rejected())?;
    Ok(seed)
}
#[wasm_bindgen(module = "/platform/sql.mjs")]
extern "C" {
    #[wasm_bindgen(catch, js_name = readTextRecord)]
    fn read(state: &JsValue, key: &str) -> Result<String>;
    #[wasm_bindgen(catch, js_name = accountTransition)]
    fn update(
        state: &JsValue,
        key: &str,
        initial: &str,
        transition: &js_sys::Function,
    ) -> Result<String>;
}

pub struct RoleKeyStore {
    state: JsValue,
}
impl RoleKeyStore {
    /// This component's DO is permanently dedicated to one exact role/task scope.
    /// Construction performs no SQL writes and reads no production KEK.
    pub fn new(state: JsValue) -> Self {
        Self { state }
    }
    pub async fn ensure<F>(
        &self,
        scope: RoleKeySlotScopeV1,
        wrapping_version: DecimalU64,
        kek: &[u8; 32],
        current: F,
    ) -> Result<PublicRoleKey>
    where
        F: Fn() -> Result<RoleKeyCurrent>,
    {
        self.ensure_with_clock(scope, wrapping_version, kek, current, now)
            .await
    }
    pub async fn ensure_with_clock<F, C>(
        &self,
        scope: RoleKeySlotScopeV1,
        wrapping_version: DecimalU64,
        kek: &[u8; 32],
        current: F,
        clock: C,
    ) -> Result<PublicRoleKey>
    where
        F: Fn() -> Result<RoleKeyCurrent>,
        C: Fn() -> Result<u64>,
    {
        if wrapping_version.0 == 0 {
            return Err(rejected());
        }
        let initial = encode(&Snapshot {
            schema_version: 1,
            slot: None,
            audit_sequence: DecimalU64(0),
        })?;
        // A losing candidate must authenticate the winning original before returning it.
        for _ in 0..3 {
            let before = current()?;
            before.check(&scope, clock()?)?;
            let observed = match read(&self.state, KEY) {
                Ok(raw) => decode(&raw)?.slot,
                // A missing row/table is expected on first use. Any real SQL failure
                // still fails the final transaction; it can never produce a replacement.
                Err(_) => None,
            };
            let candidate = if let Some(slot) = &observed {
                slot.validate_for(&scope).map_err(|_| rejected())?;
                if slot.envelope.wrapping_key_version != wrapping_version {
                    return Err(rejected());
                }
                let seed = envelope_crypto::open(&slot.envelope, &scope.context, kek)
                    .await
                    .map_err(|_| rejected())?;
                if public(seed.as_bytes())? != slot.public_ref.public_key {
                    return Err(rejected());
                }
                slot.clone()
            } else {
                let seed = random_seed()?;
                let public_ref = PublicRoleKey {
                    key_id: scope.context.identity.key_id.clone(),
                    key_version: scope.context.identity.key_version,
                    public_key: public(seed.as_slice())?,
                };
                let envelope =
                    envelope_crypto::seal(&scope.context, wrapping_version, kek, seed.as_slice())
                        .await
                        .map_err(|_| rejected())?;
                RoleKeySlotV1::new(scope.clone(), public_ref, envelope).map_err(|_| rejected())?
            };
            let mut returned = None;
            let mut retry = false;
            let mut callback = |old: String, raw_inventory: String| -> Result<Array> {
                inventory(&raw_inventory)?;
                let final_current = current()?;
                final_current.check(&scope, clock()?)?;
                if final_current != before {
                    return Err(rejected());
                }
                let saved = decode(&old)?;
                let result = Array::new();
                if let Some(original) = saved.slot {
                    original.validate_for(&scope).map_err(|_| rejected())?;
                    if observed.as_ref() == Some(&original) {
                        returned = Some(original.public_ref);
                    } else {
                        // No write and no unverified public return. Retry outside SQL
                        // to authenticate this exact original and its seed/public binding.
                        retry = true;
                    }
                    for value in [old.as_str(), "", ""] {
                        result.push(&value.into());
                    }
                } else {
                    if observed.is_some() {
                        return Err(rejected());
                    }
                    let audit = serde_json::to_string(&(
                        DecimalU64(1),
                        "role_key_created",
                        &scope,
                        &candidate.public_ref,
                    ))
                    .map_err(|_| rejected())?;
                    let next = encode(&Snapshot {
                        schema_version: 1,
                        slot: Some(candidate.clone()),
                        audit_sequence: DecimalU64(1),
                    })?;
                    result.push(&next.into());
                    result.push(&"1".into());
                    result.push(&audit.into());
                    returned = Some(candidate.public_ref.clone());
                }
                Ok(result)
            };
            let closure = Closure::borrow_mut(&mut callback);
            update(&self.state, KEY, &initial, closure.as_ref().unchecked_ref())?;
            drop(closure);
            if let Some(public_ref) = returned {
                return Ok(public_ref);
            }
            if !retry {
                return Err(rejected());
            }
        }
        Err(rejected())
    }
}
