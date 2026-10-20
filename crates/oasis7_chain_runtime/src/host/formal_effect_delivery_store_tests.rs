use super::*;
use oasis7_distfs::controlled_authority::formal::FormalWorldIdentityV1;
use oasis7_runtime::{EffectIntent, EffectOrigin};
use serde_json::json;
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT_ROOT: AtomicU64 = AtomicU64::new(0);

struct PrivateTestRoot(PathBuf);

impl PrivateTestRoot {
    fn new() -> Self {
        use std::os::unix::fs::PermissionsExt;
        let parent = std::env::temp_dir()
            .canonicalize()
            .expect("canonical temporary directory");
        let path = parent.join(format!(
            "oasis7-effect-store-{}-{}",
            std::process::id(),
            NEXT_ROOT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).expect("create private test root");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700))
            .expect("set private test root mode");
        Self(path)
    }

    fn store_path(&self) -> PathBuf {
        self.0.join("state")
    }
}

impl Drop for PrivateTestRoot {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn obligation() -> QualifiedEffectObligationV1 {
    let world = FormalWorldIdentityV1 {
        world_id: "effect-store-world".into(),
        chain_id: "effect-store-chain".into(),
        genesis_digest: "a".repeat(64),
    };
    let intent = EffectIntent {
        birth_era: Some(2),
        intent_id: "effect-store-2".into(),
        kind: "send_message".into(),
        params: json!({
            "schema_version": 1,
            "target_id": "receiver.test",
            "receiver_contract_digest": "b".repeat(64),
            "payload": {"message": "stable"}
        }),
        cap_ref: "cap-store".into(),
        origin: EffectOrigin::System,
    };
    super::super::formal_effect_outbox::test_only_qualified_obligation(
        world,
        super::super::formal_effect_outbox::QualifiedEffectCommitRefV1 {
            position: 1,
            record_id: "c".repeat(64),
            active_authority_id: "effect-store-authority".into(),
            finalizing_authority_id: "effect-store-authority".into(),
        },
        intent,
    )
    .expect("test-only qualified source fixture")
}

#[test]
fn formal_effect_store_recovers_durable_attempt_and_requires_query_proof_before_retry() {
    let root = PrivateTestRoot::new();
    let effect = obligation();
    let request_digest = "d".repeat(64);
    let query_digest = "e".repeat(64);
    let identity_key = delivery_identity_key(&effect.identity).unwrap();
    {
        let store = FormalEffectDeliveryStore::open(&root.store_path()).unwrap();
        assert_eq!(
            store.observe(&effect).unwrap().phase,
            DeliveryPhaseV1::Ready
        );
        assert!(matches!(
            store.begin_attempt(&effect, &request_digest).unwrap().phase,
            DeliveryPhaseV1::AttemptStarted { attempts: 1, .. }
        ));
    }

    let store = FormalEffectDeliveryStore::open(&root.store_path()).unwrap();
    assert!(matches!(
        store.read(&effect.identity).unwrap().unwrap().phase,
        DeliveryPhaseV1::AttemptStarted { attempts: 1, .. }
    ));
    assert!(matches!(
        store
            .begin_query(&effect, &query_digest, &request_digest)
            .unwrap()
            .phase,
        DeliveryPhaseV1::QueryStarted { queries: 1, .. }
    ));
    assert!(matches!(
        store
            .authorize_retry(&effect, b"complete receiver-signed NotFound".to_vec())
            .unwrap()
            .phase,
        DeliveryPhaseV1::RetryAuthorized { attempts: 1, .. }
    ));
    assert!(matches!(
        store.begin_attempt(&effect, &request_digest).unwrap().phase,
        DeliveryPhaseV1::AttemptStarted { attempts: 2, .. }
    ));
    store
        .mark_delivered(
            &effect,
            b"signed receiver receipt".to_vec(),
            &request_digest,
        )
        .unwrap();
    assert!(
        store
            .read(&effect.identity)
            .unwrap()
            .unwrap()
            .phase
            .is_terminal()
    );
    assert!(
        root.store_path()
            .join(format!("{identity_key}.cbor"))
            .is_file()
    );
}

#[test]
fn formal_effect_store_rejects_identity_payload_rewrite_and_poison_fails_closed() {
    let root = PrivateTestRoot::new();
    let store = FormalEffectDeliveryStore::open(&root.store_path()).unwrap();
    let first = obligation();
    store.observe(&first).unwrap();
    let mut conflicting_intent = first.intent.clone();
    conflicting_intent.params["payload"]["message"] = json!("rewritten");
    let conflicting = super::super::formal_effect_outbox::test_only_qualified_obligation(
        first.identity.world.clone(),
        first.committed_at.clone(),
        conflicting_intent,
    )
    .unwrap();
    assert!(store.observe(&conflicting).is_err());
    assert!(store.read(&first.identity).is_err());
}

#[test]
fn formal_effect_store_keeps_first_commit_when_same_qualified_effect_reappears() {
    let root = PrivateTestRoot::new();
    let store = FormalEffectDeliveryStore::open(&root.store_path()).unwrap();
    let first = obligation();
    store.observe(&first).unwrap();
    let later = super::super::formal_effect_outbox::test_only_qualified_obligation(
        first.identity.world.clone(),
        super::super::formal_effect_outbox::QualifiedEffectCommitRefV1 {
            position: 2,
            record_id: "d".repeat(64),
            active_authority_id: "effect-store-authority".into(),
            finalizing_authority_id: "effect-store-authority".into(),
        },
        first.intent.clone(),
    )
    .unwrap();
    let record = store.observe(&later).unwrap();
    assert_eq!(record.obligation.committed_at.position, 1);
    assert_eq!(record.obligation.committed_at.record_id, "c".repeat(64));
}

#[test]
fn formal_effect_store_rejects_record_phase_tampering_without_matching_checksum() {
    let root = PrivateTestRoot::new();
    let effect = obligation();
    let store = FormalEffectDeliveryStore::open(&root.store_path()).unwrap();
    store.observe(&effect).unwrap();
    let record = store.read(&effect.identity).unwrap().unwrap();
    let key = delivery_identity_key(&effect.identity).unwrap();
    let path = root.store_path().join(format!("{key}.cbor"));
    let mut bytes = fs::read(&path).unwrap();
    let checksum = record.record_checksum.as_bytes();
    let start = bytes
        .windows(checksum.len())
        .position(|window| window == checksum)
        .expect("serialized record contains its checksum");
    bytes[start] = if bytes[start] == b'0' { b'1' } else { b'0' };
    fs::write(&path, bytes).unwrap();
    let error = store.read(&effect.identity).unwrap_err();
    assert!(error.contains("checksum mismatch"), "{error}");
}

#[test]
fn formal_effect_store_rejects_mutated_fields_even_when_source_marker_is_reused() {
    let root = PrivateTestRoot::new();
    let store = FormalEffectDeliveryStore::open(&root.store_path()).unwrap();
    let first = obligation();
    store.observe(&first).unwrap();
    let mut altered = first.clone();
    altered.target_id = "attacker.target".into();
    assert!(store.observe(&altered).is_err());
}

#[test]
fn formal_effect_store_exclusive_lock_rejects_second_dispatcher() {
    let root = PrivateTestRoot::new();
    let first = FormalEffectDeliveryStore::open(&root.store_path()).unwrap();
    assert!(FormalEffectDeliveryStore::open(&root.store_path()).is_err());
    drop(first);
    assert!(FormalEffectDeliveryStore::open(&root.store_path()).is_ok());
}
