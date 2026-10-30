use super::*;
use crate::provisioning::ProvisionRoleKeyAlgorithmV1;
use serde::Serialize;

fn synthetic_scope(
    slot: ContextBodySlotKindV1,
    deadline: u64,
    maximum: bool,
) -> ContextBodyScopeV1 {
    let id = |short: &str, n| {
        if maximum {
            "s".repeat(n)
        } else {
            short.to_owned()
        }
    };
    ContextBodyScopeV1 {
        api_version: 1,
        identity_scope: RoleKeySlotScopeV1 {
            provision_id: id("synthetic-non-grant-provision", 128),
            operation_id: id("synthetic-non-grant-role", 128),
            context: EnvelopeContextV1 {
                schema_version: 1,
                identity: KeyIdentity {
                    environment: id("synthetic-storage-only", 256),
                    account_id: id("synthetic-non-grant-account", 256),
                    role: Role::Identity,
                    key_id: id("synthetic-non-grant-key", 128),
                    key_version: DecimalU64(1),
                },
                algorithm: EnvelopeAlgorithmV1::Aes256Gcm,
            },
            algorithm: ProvisionRoleKeyAlgorithmV1::Ed25519,
        },
        world: ApprovedWorldV1 {
            world_id: id("synthetic-non-grant-world", 256),
            genesis_digest: id("synthetic-non-grant-genesis", 256),
            branch_id: id("synthetic-non-grant-branch", 256),
            reorg_epoch: DecimalU64(u64::MAX),
            service_public_key: "ab".repeat(32),
        },
        session_id: id("synthetic-non-grant-session", 256),
        context_operation_id: id("synthetic-non-grant-context", 256),
        session_intent_digest: "cd".repeat(32),
        native_binding_digest: "ef".repeat(32),
        slot,
        version: DecimalU64(1),
        write_deadline: DecimalU64(deadline),
    }
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
#[derive(Serialize)]
struct Entry {
    name: &'static str,
    scope: ContextBodyScopeV1,
    scope_wire: String,
    actor_name: String,
    envelope_context: EnvelopeContextV1,
    envelope_context_wire: String,
    value_aad_hex: String,
    wrap_aad_hex: String,
}
fn entry(name: &'static str, scope: ContextBodyScopeV1) -> Entry {
    scope.validate().unwrap();
    let envelope_context = scope.envelope_context().unwrap();
    Entry {
        name,
        scope_wire: serde_json::to_string(&scope).unwrap(),
        actor_name: scope.derived_actor_name().unwrap(),
        envelope_context_wire: serde_json::to_string(&envelope_context).unwrap(),
        value_aad_hex: hex(&envelope_context.value_aad().unwrap()),
        wrap_aad_hex: hex(&envelope_context.wrap_aad(DecimalU64(1)).unwrap()),
        scope,
        envelope_context,
    }
}
#[test]
fn context_body_export_storage_scope_fixture() {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis();
    let deadline = u64::try_from(now).unwrap().checked_add(3_600_000).unwrap();
    let entries = vec![
        entry(
            "prepared",
            synthetic_scope(ContextBodySlotKindV1::Prepared, deadline, false),
        ),
        entry(
            "captured",
            synthetic_scope(ContextBodySlotKindV1::Captured, deadline, false),
        ),
        entry(
            "expired_prepared",
            synthetic_scope(ContextBodySlotKindV1::Prepared, 1, false),
        ),
        entry(
            "max_prepared",
            synthetic_scope(ContextBodySlotKindV1::Prepared, deadline, true),
        ),
    ];
    for e in &entries {
        let record = ContextBodyRecordV1 {
            scope: e.scope.clone(),
            envelope: KeyEnvelopeV1 {
                context: e.envelope_context.clone(),
                ciphertext: "ab".repeat(CONTEXT_BODY_MAX_BYTES + AES_GCM_TAG_BYTES),
                value_nonce: "01".repeat(AES_GCM_NONCE_BYTES),
                wrapped_dek: "cd".repeat(DEK_BYTES + AES_GCM_TAG_BYTES),
                wrap_nonce: "02".repeat(AES_GCM_NONCE_BYTES),
                wrapping_key_version: DecimalU64(1),
            },
        };
        record.validate().unwrap();
        assert!(record.wire_bytes().unwrap().len() <= CONTEXT_BODY_MAX_WIRE_BYTES);
    }
    let fixture =
        serde_json::json!({"label":"synthetic-storage-scope-not-authority", "entries": entries});
    if let Some(path) = std::env::var_os("OASIS7_CONTEXT_BODY_FIXTURE_OUTPUT") {
        let path = std::path::PathBuf::from(path);
        assert!(path.is_absolute() && path.starts_with("/tmp"));
        std::fs::write(path, serde_json::to_vec(&fixture).unwrap()).unwrap();
    }
}
