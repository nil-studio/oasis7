//! Synthetic role authority and KEK only; never compiled into deployment artifacts.
use crate::role_key_store::{RoleKeyCurrent, RoleKeyStore};
use ed25519_dalek::{Signer, SigningKey};
use oasis7_custody_core::{DecimalU64, http::Role, provisioning::*, role_key_slot::*};
use serde_json::{Value, json};
use wasm_bindgen::JsValue;
use worker::*;

const KEY: &str = "custody-role-key-slot-v1";
fn failure(_: impl std::fmt::Debug) -> Error {
    Error::RustError("role key fixture rejected".into())
}
fn scope() -> Result<RoleKeySlotScopeV1> {
    RoleKeySlotScopeV1::from_task(&ProvisionRoleKeyTaskV1 {
        operation_id: "synthetic-owner-key-task".into(),
        provision_id: "synthetic-provision".into(),
        environment: "staging".into(),
        custody_account_id: "synthetic-account".into(),
        role: Role::Owner,
        key_id: "synthetic-owner-key".into(),
        key_version: DecimalU64(1),
        algorithm: ProvisionRoleKeyAlgorithmV1::Ed25519,
        state: ProvisionRoleKeyTaskStateV1::Pending,
    })
    .map_err(failure)
}
fn record(state: &JsValue) -> Result<Value> {
    let rows = super::rows(
        state,
        &format!("SELECT payload FROM custody_records WHERE id='{KEY}'"),
    )?;
    let text = rows[0]["payload"]
        .as_str()
        .ok_or_else(|| failure("missing slot"))?;
    serde_json::from_str(text).map_err(failure)
}
pub async fn handle(state: JsValue, mut request: Request) -> Result<Response> {
    match run(state, &mut request).await {
        Ok(value) => Response::from_json(&value),
        Err(_) => Ok(Response::from_json(&json!({"rejected":true}))?.with_status(409)),
    }
}
async fn run(state: JsValue, request: &mut Request) -> Result<Value> {
    let path = request.path();
    if path == "/role-key/counts" {
        let tables = super::rows(&state, "SELECT name FROM sqlite_master WHERE type='table'")?;
        let has = |name| {
            tables
                .as_array()
                .is_some_and(|rows| rows.iter().any(|row| row["name"] == name))
        };
        let slots = if has("custody_records") {
            super::rows(&state, "SELECT COUNT(*) AS count FROM custody_records WHERE payload NOT LIKE '%\"slot\":null%'")?[0]["count"].clone()
        } else {
            json!(0)
        };
        let audits = if has("custody_audit") {
            super::rows(&state, "SELECT COUNT(*) AS count FROM custody_audit")?[0]["count"].clone()
        } else {
            json!(0)
        };
        return Ok(json!({"slots":slots,"audits":audits}));
    }
    if path == "/role-key/raw" {
        return super::rows(&state, "SELECT id,payload FROM custody_records");
    }
    if path == "/role-key/audit" {
        return super::rows(
            &state,
            "SELECT record_id,sequence,payload FROM custody_audit",
        );
    }
    if path == "/role-key/fault" {
        super::sql(
            &state,
            "CREATE TABLE custody_audit (record_id TEXT NOT NULL, sequence TEXT NOT NULL, payload TEXT NOT NULL CHECK(0), PRIMARY KEY(record_id,sequence))",
        )?;
        return Ok(json!({"ok":true}));
    }
    if path == "/role-key/clear-fault" {
        super::sql(&state, "DROP TABLE custody_audit")?;
        return Ok(json!({"ok":true}));
    }
    if path.starts_with("/role-key/corrupt-") {
        let mut saved = record(&state)?;
        // The store serializes its strict slot directly; this fixture never accepts arbitrary SQL.
        let slot = if saved.get("slot").is_some() {
            &mut saved["slot"]
        } else {
            &mut saved
        };
        match path.as_str() {
            "/role-key/corrupt-public" => slot["public_ref"]["public_key"] = json!("02".repeat(32)),
            "/role-key/corrupt-ciphertext" => {
                let text = slot["envelope"]["ciphertext"]
                    .as_str()
                    .ok_or_else(|| failure("ciphertext"))?;
                let changed = format!(
                    "{}{}",
                    if text.starts_with('0') { "1" } else { "0" },
                    &text[1..]
                );
                slot["envelope"]["ciphertext"] = json!(changed);
            }
            "/role-key/corrupt-schema" => slot["schema_version"] = json!(2),
            _ => return Err(failure("unknown corruption")),
        }
        let payload = serde_json::to_string(&saved)?.replace('\'', "''");
        super::sql(
            &state,
            &format!("UPDATE custody_records SET payload='{payload}' WHERE id='{KEY}'"),
        )?;
        return Ok(json!({"ok":true}));
    }
    if path == "/role-key/verify-material" {
        let saved = record(&state)?;
        let slot: RoleKeySlotV1 = serde_json::from_value(if saved.get("slot").is_some() {
            saved["slot"].clone()
        } else {
            saved
        })?;
        let bytes = crate::envelope_crypto::open(&slot.envelope, &scope()?.context, &[73; 32])
            .await
            .map_err(failure)?;
        let seed: &[u8; 32] = bytes.as_bytes().try_into().map_err(failure)?;
        let key = SigningKey::from_bytes(seed);
        let message = b"oasis7.synthetic.role-slot.persistence.v1";
        return Ok(
            json!({"public_key":hex::encode(key.verifying_key().to_bytes()),"message":message.as_slice(),"signature":hex::encode(key.sign(message).to_bytes())}),
        );
    }
    let mut expected = scope()?;
    if path.ends_with("-scope") {
        expected.operation_id = "other-task".into();
    }
    if path.ends_with("-role") {
        expected.context.identity.role = Role::Identity;
    }
    let reads = std::cell::Cell::new(0_u8);
    let authority = || {
        reads.set(reads.get() + 1);
        let mut value = RoleKeyCurrent {
            scope: expected.clone(),
            approved: true,
            frozen: false,
            cancelled: false,
            expires_at: DecimalU64(4_102_444_800_000),
            authority_revision: "ab".repeat(32),
        };
        if reads.get() > 1 {
            if path.ends_with("-expired") {
                value.expires_at = DecimalU64(1);
            }
            if path.ends_with("-frozen") {
                value.frozen = true;
            }
            if path.ends_with("-cancelled") {
                value.cancelled = true;
            }
            if path.ends_with("-revision") {
                value.authority_revision = "cd".repeat(32);
            }
            if path.ends_with("-unapproved") {
                value.approved = false;
            }
        }
        Ok(value)
    };
    let kek = [if path.ends_with("-wrong-kek") { 74 } else { 73 }; 32];
    let clock_reads = std::cell::Cell::new(0_u8);
    let clock = || {
        clock_reads.set(clock_reads.get() + 1);
        Ok(if path.ends_with("-clock") && clock_reads.get() > 1 {
            4_102_444_800_000
        } else {
            js_sys::Date::now() as u64
        })
    };
    let public = RoleKeyStore::new(state)
        .ensure_with_clock(expected.clone(), DecimalU64(1), &kek, authority, clock)
        .await
        .map_err(failure)?;
    serde_json::to_value(public).map_err(failure)
}
