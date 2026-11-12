use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use ed25519_dalek::{Signer, SigningKey};

use super::*;

fn temp_dir(prefix: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("duration")
        .as_nanos();
    std::env::temp_dir().join(format!("oasis7-feedback-{prefix}-{unique}"))
}

fn signing_key(seed: u8) -> SigningKey {
    let mut bytes = [0_u8; 32];
    bytes.fill(seed);
    SigningKey::from_bytes(&bytes)
}

fn public_key_hex(signing_key: &SigningKey) -> String {
    hex::encode(signing_key.verifying_key().to_bytes())
}

fn sign_signature_hex(payload: FeedbackSignedPayload<'_>, signing_key: &SigningKey) -> String {
    let bytes = to_canonical_cbor(&payload).expect("cbor");
    let signature = signing_key.sign(bytes.as_slice());
    hex::encode(signature.to_bytes())
}

fn now_plus(delta_ms: i64) -> i64 {
    now_unix_time_ms().saturating_add(delta_ms)
}

#[test]
fn feedback_create_append_tombstone_public_read_roundtrip() {
    let dir = temp_dir("roundtrip");
    let store = LocalCasStore::new(&dir);
    let feedback_store = FeedbackStore::new(store, FeedbackStoreConfig::default());
    let signing_key = signing_key(11);
    let actor_public_key_hex = public_key_hex(&signing_key);
    let created_at = now_plus(0);
    let expires_at = now_plus(60_000);

    let create_stub = FeedbackCreateRequest {
        feedback_id: "fb-001".to_string(),
        author_public_key_hex: actor_public_key_hex.clone(),
        submit_ip: "127.0.0.1".to_string(),
        category: "bug".to_string(),
        platform: "web".to_string(),
        game_version: "0.1.0".to_string(),
        content: "first feedback".to_string(),
        attachments: vec![],
        nonce: "n-001".to_string(),
        timestamp_ms: created_at,
        expires_at_ms: expires_at,
        signature_hex: String::new(),
    };
    let create_content_hash = feedback_create_content_hash(&create_stub).expect("content hash");
    let create_signature = sign_signature_hex(
        FeedbackSignedPayload::new(
            FeedbackActionKind::Create,
            create_stub.feedback_id.as_str(),
            create_stub.author_public_key_hex.as_str(),
            create_content_hash.as_str(),
            create_stub.nonce.as_str(),
            create_stub.timestamp_ms,
            create_stub.expires_at_ms,
        ),
        &signing_key,
    );
    let mut create_request = create_stub;
    create_request.signature_hex = create_signature;
    let create_receipt = feedback_store
        .submit_feedback(create_request)
        .expect("submit create");
    assert!(create_receipt.accepted);

    let append_content = "additional detail".to_string();
    let append_nonce = "n-002".to_string();
    let append_timestamp = now_plus(10);
    let append_expire = now_plus(60_000);
    let append_hash = blake3_hex(append_content.as_bytes());
    let append_signature = sign_signature_hex(
        FeedbackSignedPayload::new(
            FeedbackActionKind::Append,
            "fb-001",
            actor_public_key_hex.as_str(),
            append_hash.as_str(),
            append_nonce.as_str(),
            append_timestamp,
            append_expire,
        ),
        &signing_key,
    );
    let append_receipt = feedback_store
        .append_feedback(FeedbackAppendRequest {
            feedback_id: "fb-001".to_string(),
            actor_public_key_hex: actor_public_key_hex.clone(),
            submit_ip: "127.0.0.1".to_string(),
            content: append_content.clone(),
            nonce: append_nonce,
            timestamp_ms: append_timestamp,
            expires_at_ms: append_expire,
            signature_hex: append_signature,
        })
        .expect("append");
    assert_eq!(append_receipt.action, FeedbackActionKind::Append);

    let tombstone_reason = "duplicate report".to_string();
    let tombstone_nonce = "n-003".to_string();
    let tombstone_timestamp = now_plus(20);
    let tombstone_expire = now_plus(60_000);
    let tombstone_hash = blake3_hex(tombstone_reason.as_bytes());
    let tombstone_signature = sign_signature_hex(
        FeedbackSignedPayload::new(
            FeedbackActionKind::Tombstone,
            "fb-001",
            actor_public_key_hex.as_str(),
            tombstone_hash.as_str(),
            tombstone_nonce.as_str(),
            tombstone_timestamp,
            tombstone_expire,
        ),
        &signing_key,
    );
    let tombstone_receipt = feedback_store
        .tombstone_feedback(FeedbackTombstoneRequest {
            feedback_id: "fb-001".to_string(),
            actor_public_key_hex: actor_public_key_hex.clone(),
            submit_ip: "127.0.0.1".to_string(),
            reason: tombstone_reason.clone(),
            nonce: tombstone_nonce,
            timestamp_ms: tombstone_timestamp,
            expires_at_ms: tombstone_expire,
            signature_hex: tombstone_signature,
        })
        .expect("tombstone");
    assert_eq!(tombstone_receipt.action, FeedbackActionKind::Tombstone);

    let view = feedback_store
        .read_feedback_public("fb-001")
        .expect("read")
        .expect("exists");
    assert_eq!(view.feedback_id, "fb-001");
    assert_eq!(view.append_events.len(), 1);
    assert_eq!(view.append_events[0].content, append_content);
    assert!(view.tombstoned);
    assert_eq!(view.tombstone_reason, Some(tombstone_reason));

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn feedback_author_control_rejects_non_author_append() {
    let dir = temp_dir("author-control");
    let store = LocalCasStore::new(&dir);
    let feedback_store = FeedbackStore::new(store, FeedbackStoreConfig::default());
    let author = signing_key(5);
    let attacker = signing_key(9);
    let author_pubkey = public_key_hex(&author);
    let attacker_pubkey = public_key_hex(&attacker);
    let create_timestamp = now_plus(0);
    let create_expire = now_plus(60_000);
    let create_stub = FeedbackCreateRequest {
        feedback_id: "fb-author".to_string(),
        author_public_key_hex: author_pubkey.clone(),
        submit_ip: "127.0.0.2".to_string(),
        category: "ux".to_string(),
        platform: "desktop".to_string(),
        game_version: "0.1.0".to_string(),
        content: "owner feedback".to_string(),
        attachments: vec![],
        nonce: "n-auth-1".to_string(),
        timestamp_ms: create_timestamp,
        expires_at_ms: create_expire,
        signature_hex: String::new(),
    };
    let create_hash = feedback_create_content_hash(&create_stub).expect("hash");
    let create_signature = sign_signature_hex(
        FeedbackSignedPayload::new(
            FeedbackActionKind::Create,
            create_stub.feedback_id.as_str(),
            create_stub.author_public_key_hex.as_str(),
            create_hash.as_str(),
            create_stub.nonce.as_str(),
            create_stub.timestamp_ms,
            create_stub.expires_at_ms,
        ),
        &author,
    );
    let mut create_request = create_stub;
    create_request.signature_hex = create_signature;
    feedback_store
        .submit_feedback(create_request)
        .expect("submit create");

    let append_content = "attacker append";
    let append_hash = blake3_hex(append_content.as_bytes());
    let append_signature = sign_signature_hex(
        FeedbackSignedPayload::new(
            FeedbackActionKind::Append,
            "fb-author",
            attacker_pubkey.as_str(),
            append_hash.as_str(),
            "n-attacker-1",
            now_plus(10),
            now_plus(60_000),
        ),
        &attacker,
    );
    let append_result = feedback_store.append_feedback(FeedbackAppendRequest {
        feedback_id: "fb-author".to_string(),
        actor_public_key_hex: attacker_pubkey,
        submit_ip: "127.0.0.3".to_string(),
        content: append_content.to_string(),
        nonce: "n-attacker-1".to_string(),
        timestamp_ms: now_plus(10),
        expires_at_ms: now_plus(60_000),
        signature_hex: append_signature,
    });
    assert!(matches!(
        append_result,
        Err(WorldError::DistributedValidationFailed { .. })
    ));

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn feedback_replay_nonce_is_rejected() {
    let dir = temp_dir("replay");
    let store = LocalCasStore::new(&dir);
    let feedback_store = FeedbackStore::new(store, FeedbackStoreConfig::default());
    let author = signing_key(17);
    let author_pubkey = public_key_hex(&author);
    let timestamp = now_plus(0);
    let expires = now_plus(60_000);
    let nonce = "n-replay-1";

    let create_stub = FeedbackCreateRequest {
        feedback_id: "fb-replay".to_string(),
        author_public_key_hex: author_pubkey.clone(),
        submit_ip: "127.0.0.4".to_string(),
        category: "bug".to_string(),
        platform: "web".to_string(),
        game_version: "0.2.0".to_string(),
        content: "nonce replay test".to_string(),
        attachments: vec![],
        nonce: nonce.to_string(),
        timestamp_ms: timestamp,
        expires_at_ms: expires,
        signature_hex: String::new(),
    };
    let create_hash = feedback_create_content_hash(&create_stub).expect("hash");
    let create_signature = sign_signature_hex(
        FeedbackSignedPayload::new(
            FeedbackActionKind::Create,
            create_stub.feedback_id.as_str(),
            create_stub.author_public_key_hex.as_str(),
            create_hash.as_str(),
            create_stub.nonce.as_str(),
            create_stub.timestamp_ms,
            create_stub.expires_at_ms,
        ),
        &author,
    );
    let mut first_request = create_stub.clone();
    first_request.signature_hex = create_signature.clone();
    feedback_store
        .submit_feedback(first_request)
        .expect("first submit");

    let mut second_request = create_stub;
    second_request.feedback_id = "fb-replay-2".to_string();
    second_request.signature_hex = create_signature;
    let replay_result = feedback_store.submit_feedback(second_request);
    assert!(matches!(
        replay_result,
        Err(WorldError::DistributedValidationFailed { .. })
    ));

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn feedback_rate_limit_blocks_excessive_submissions() {
    let dir = temp_dir("rate-limit");
    let store = LocalCasStore::new(&dir);
    let config = FeedbackStoreConfig {
        max_actions_per_ip_window: 1,
        max_actions_per_pubkey_window: 1,
        ..FeedbackStoreConfig::default()
    };
    let feedback_store = FeedbackStore::new(store, config);
    let author = signing_key(23);
    let author_pubkey = public_key_hex(&author);

    let make_request = |feedback_id: &str, nonce: &str| {
        let timestamp = now_plus(0);
        let expires = now_plus(60_000);
        let stub = FeedbackCreateRequest {
            feedback_id: feedback_id.to_string(),
            author_public_key_hex: author_pubkey.clone(),
            submit_ip: "127.0.0.5".to_string(),
            category: "bug".to_string(),
            platform: "web".to_string(),
            game_version: "0.2.1".to_string(),
            content: format!("feedback {feedback_id}"),
            attachments: vec![],
            nonce: nonce.to_string(),
            timestamp_ms: timestamp,
            expires_at_ms: expires,
            signature_hex: String::new(),
        };
        let content_hash = feedback_create_content_hash(&stub).expect("hash");
        let signature = sign_signature_hex(
            FeedbackSignedPayload::new(
                FeedbackActionKind::Create,
                stub.feedback_id.as_str(),
                stub.author_public_key_hex.as_str(),
                content_hash.as_str(),
                stub.nonce.as_str(),
                stub.timestamp_ms,
                stub.expires_at_ms,
            ),
            &author,
        );
        FeedbackCreateRequest {
            signature_hex: signature,
            ..stub
        }
    };

    feedback_store
        .submit_feedback(make_request("fb-rate-1", "n-rate-1"))
        .expect("first submit");
    let second = feedback_store.submit_feedback(make_request("fb-rate-2", "n-rate-2"));
    assert!(matches!(
        second,
        Err(WorldError::DistributedValidationFailed { .. })
    ));

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn feedback_rate_limit_skips_stale_audit_paths_before_reading() {
    let dir = temp_dir("rate-limit-stale-skip");
    let store = LocalCasStore::new(&dir);
    let config = FeedbackStoreConfig {
        rate_limit_window_ms: 60_000,
        max_actions_per_ip_window: 1,
        max_actions_per_pubkey_window: 1,
        ..FeedbackStoreConfig::default()
    };
    let feedback_store = FeedbackStore::new(store, config);
    let author = signing_key(29);
    let author_pubkey = public_key_hex(&author);
    let stale_audit_path = feedback_audit_path(now_plus(-120_000), "stale-malformed");
    feedback_store
        .store
        .write_file(stale_audit_path.as_str(), b"not-json")
        .expect("write stale malformed audit");

    let timestamp = now_plus(0);
    let expires = now_plus(60_000);
    let stub = FeedbackCreateRequest {
        feedback_id: "fb-rate-stale-skip".to_string(),
        author_public_key_hex: author_pubkey.clone(),
        submit_ip: "127.0.0.6".to_string(),
        category: "bug".to_string(),
        platform: "web".to_string(),
        game_version: "0.2.2".to_string(),
        content: "stale audit should not be parsed".to_string(),
        attachments: vec![],
        nonce: "n-rate-stale-skip".to_string(),
        timestamp_ms: timestamp,
        expires_at_ms: expires,
        signature_hex: String::new(),
    };
    let content_hash = feedback_create_content_hash(&stub).expect("hash");
    let signature = sign_signature_hex(
        FeedbackSignedPayload::new(
            FeedbackActionKind::Create,
            stub.feedback_id.as_str(),
            stub.author_public_key_hex.as_str(),
            content_hash.as_str(),
            stub.nonce.as_str(),
            stub.timestamp_ms,
            stub.expires_at_ms,
        ),
        &author,
    );
    let receipt = feedback_store
        .submit_feedback(FeedbackCreateRequest {
            signature_hex: signature,
            ..stub
        })
        .expect("stale malformed audit is skipped before read");
    assert!(receipt.accepted);

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn feedback_replicated_ingest_is_idempotent_for_root_and_event() {
    let source_dir = temp_dir("replicated-source");
    let target_dir = temp_dir("replicated-target");
    let source = FeedbackStore::new(
        LocalCasStore::new(&source_dir),
        FeedbackStoreConfig::default(),
    );
    let target = FeedbackStore::new(
        LocalCasStore::new(&target_dir),
        FeedbackStoreConfig::default(),
    );
    let author = signing_key(31);
    let author_pubkey = public_key_hex(&author);
    let create_ts = now_plus(0);
    let create_expire = now_plus(60_000);
    let mut create_request = FeedbackCreateRequest {
        feedback_id: "fb-repl-1".to_string(),
        author_public_key_hex: author_pubkey.clone(),
        submit_ip: "127.0.0.8".to_string(),
        category: "bug".to_string(),
        platform: "web".to_string(),
        game_version: "0.3.0".to_string(),
        content: "root payload".to_string(),
        attachments: vec![],
        nonce: "n-repl-root".to_string(),
        timestamp_ms: create_ts,
        expires_at_ms: create_expire,
        signature_hex: String::new(),
    };
    create_request.signature_hex =
        sign_feedback_create_request(&create_request, &hex::encode(author.to_bytes()))
            .expect("sign create");
    let create_receipt = source.submit_feedback(create_request).expect("submit");
    let root_blob = source
        .blob_ref_for_receipt(&create_receipt)
        .expect("root blob ref")
        .expect("root blob");
    let root_bytes = source
        .store
        .read_file(root_blob.path.as_str())
        .expect("read root blob");
    let root_record: FeedbackRootRecord = serde_json::from_slice(&root_bytes).expect("root record");
    target
        .ingest_replicated_root_record(root_record.clone())
        .expect("ingest root");
    target
        .ingest_replicated_root_record(root_record)
        .expect("ingest root idempotent");

    let append_ts = now_plus(20);
    let append_expire = now_plus(60_000);
    let mut append_request = FeedbackAppendRequest {
        feedback_id: "fb-repl-1".to_string(),
        actor_public_key_hex: author_pubkey,
        submit_ip: "127.0.0.8".to_string(),
        content: "append payload".to_string(),
        nonce: "n-repl-append".to_string(),
        timestamp_ms: append_ts,
        expires_at_ms: append_expire,
        signature_hex: String::new(),
    };
    append_request.signature_hex =
        sign_feedback_append_request(&append_request, &hex::encode(author.to_bytes()))
            .expect("sign append");
    let append_receipt = source.append_feedback(append_request).expect("append");
    let append_blob = source
        .blob_ref_for_receipt(&append_receipt)
        .expect("append blob ref")
        .expect("append blob");
    let append_bytes = source
        .store
        .read_file(append_blob.path.as_str())
        .expect("read append blob");
    let append_record: FeedbackEventRecord =
        serde_json::from_slice(&append_bytes).expect("append record");
    target
        .ingest_replicated_event_record(append_record.clone())
        .expect("ingest append");
    target
        .ingest_replicated_event_record(append_record)
        .expect("ingest append idempotent");

    let view = target
        .read_feedback_public("fb-repl-1")
        .expect("read target")
        .expect("target view");
    assert_eq!(view.append_events.len(), 1);
    assert_eq!(view.append_events[0].content, "append payload");

    let _ = fs::remove_dir_all(source_dir);
    let _ = fs::remove_dir_all(target_dir);
}

#[test]
fn feedback_replicated_ingest_rejects_event_id_conflict() {
    let source_dir = temp_dir("replicated-conflict-source");
    let target_dir = temp_dir("replicated-conflict-target");
    let source = FeedbackStore::new(
        LocalCasStore::new(&source_dir),
        FeedbackStoreConfig::default(),
    );
    let target = FeedbackStore::new(
        LocalCasStore::new(&target_dir),
        FeedbackStoreConfig::default(),
    );
    let author = signing_key(33);
    let author_pubkey = public_key_hex(&author);

    let mut create_request = FeedbackCreateRequest {
        feedback_id: "fb-repl-2".to_string(),
        author_public_key_hex: author_pubkey.clone(),
        submit_ip: "127.0.0.9".to_string(),
        category: "bug".to_string(),
        platform: "web".to_string(),
        game_version: "0.3.1".to_string(),
        content: "root".to_string(),
        attachments: vec![],
        nonce: "n-repl2-root".to_string(),
        timestamp_ms: now_plus(0),
        expires_at_ms: now_plus(60_000),
        signature_hex: String::new(),
    };
    create_request.signature_hex =
        sign_feedback_create_request(&create_request, &hex::encode(author.to_bytes()))
            .expect("sign create");
    let create_receipt = source.submit_feedback(create_request).expect("submit");
    let root_blob = source
        .blob_ref_for_receipt(&create_receipt)
        .expect("root blob ref")
        .expect("root blob");
    let root_record: FeedbackRootRecord = serde_json::from_slice(
        source
            .store
            .read_file(root_blob.path.as_str())
            .expect("read root blob")
            .as_slice(),
    )
    .expect("root record");
    target
        .ingest_replicated_root_record(root_record)
        .expect("ingest root");

    let mut append_request = FeedbackAppendRequest {
        feedback_id: "fb-repl-2".to_string(),
        actor_public_key_hex: author_pubkey,
        submit_ip: "127.0.0.9".to_string(),
        content: "append one".to_string(),
        nonce: "n-repl2-append".to_string(),
        timestamp_ms: now_plus(10),
        expires_at_ms: now_plus(60_000),
        signature_hex: String::new(),
    };
    append_request.signature_hex =
        sign_feedback_append_request(&append_request, &hex::encode(author.to_bytes()))
            .expect("sign append");
    let append_receipt = source.append_feedback(append_request).expect("append");
    let append_blob = source
        .blob_ref_for_receipt(&append_receipt)
        .expect("append blob ref")
        .expect("append blob");
    let mut append_record: FeedbackEventRecord = serde_json::from_slice(
        source
            .store
            .read_file(append_blob.path.as_str())
            .expect("read append blob")
            .as_slice(),
    )
    .expect("append record");
    target
        .ingest_replicated_event_record(append_record.clone())
        .expect("ingest append");

    append_record.content = Some("tampered".to_string());
    let conflict = target.ingest_replicated_event_record(append_record);
    assert!(matches!(
        conflict,
        Err(WorldError::DistributedValidationFailed { .. })
    ));

    let _ = fs::remove_dir_all(source_dir);
    let _ = fs::remove_dir_all(target_dir);
}

#[test]
fn feedback_rate_limit_groups_equivalent_public_keys_after_reopen() {
    assert_feedback_rate_limit_identity_aliases(false);
}

#[test]
fn feedback_rate_limit_groups_equivalent_ip_addresses_after_reopen() {
    assert_feedback_rate_limit_identity_aliases(true);
}

fn assert_feedback_rate_limit_identity_aliases(check_ip: bool) {
    let dir = temp_dir("rate-limit-identity-aliases");
    let config = FeedbackStoreConfig {
        max_actions_per_ip_window: if check_ip { 1 } else { 100 },
        max_actions_per_pubkey_window: if check_ip { 100 } else { 1 },
        ..FeedbackStoreConfig::default()
    };
    let author = signing_key(23);
    let key = public_key_hex(&author);
    assert_ne!(key, key.to_uppercase());
    let make_request = |id: &str, public_key: String, ip: &str| {
        let mut request = FeedbackCreateRequest {
            feedback_id: id.to_string(),
            author_public_key_hex: public_key,
            submit_ip: ip.to_string(),
            category: "bug".to_string(),
            platform: "web".to_string(),
            game_version: "0.2.1".to_string(),
            content: "identity accounting regression".to_string(),
            attachments: vec![],
            nonce: id.to_string(),
            timestamp_ms: now_plus(0),
            expires_at_ms: now_plus(60_000),
            signature_hex: String::new(),
        };
        let hash = feedback_create_content_hash(&request).expect("hash");
        request.signature_hex = sign_signature_hex(
            FeedbackSignedPayload::new(
                FeedbackActionKind::Create,
                request.feedback_id.as_str(),
                request.author_public_key_hex.as_str(),
                hash.as_str(),
                request.nonce.as_str(),
                request.timestamp_ms,
                request.expires_at_ms,
            ),
            &author,
        );
        request
    };
    let first = make_request(
        "identity-first",
        key.to_uppercase(),
        " 2001:0db8:0:0:0:0:0:1 ",
    );
    let store = FeedbackStore::new(LocalCasStore::new(&dir), config.clone());
    store
        .submit_feedback(first.clone())
        .expect("first signed request");
    let root_before = store
        .store
        .read_file(&feedback_root_path("identity-first"))
        .expect("root");
    let audit_files = store.store.list_files().expect("files");
    let audit_before: Vec<_> = audit_files
        .iter()
        .filter(|file| file.path.starts_with(&feedback_audit_prefix()))
        .map(|file| {
            (
                file.path.clone(),
                store.store.read_file(&file.path).expect("audit"),
            )
        })
        .collect();
    assert_eq!(audit_before.len(), 1);
    drop(store);
    let reopened = FeedbackStore::new(LocalCasStore::new(&dir), config);
    let result = reopened.submit_feedback(make_request("identity-second", key, "2001:db8::1"));
    match result {
        Err(WorldError::DistributedValidationFailed { reason }) => {
            let expected = if check_ip {
                "rate limit exceeded for ip="
            } else {
                "rate limit exceeded for pubkey="
            };
            assert!(reason.starts_with(expected), "{reason}");
        }
        other => panic!("expected identity rate limit rejection, got {other:?}"),
    }
    assert_eq!(
        reopened
            .store
            .read_file(&feedback_root_path("identity-first"))
            .expect("root"),
        root_before
    );
    for (path, bytes) in audit_before {
        assert_eq!(reopened.store.read_file(&path).expect("unchanged audit"), bytes);
    }
    let root = reopened
        .read_feedback_root("identity-first")
        .expect("stored root");
    assert_eq!(root.author_public_key_hex, first.author_public_key_hex);
    assert_eq!(root.submit_ip, first.submit_ip);
    assert!(
        reopened
            .store
            .stat_file(&feedback_root_path("identity-second"))
            .expect("stat")
            .is_none()
    );
    fs::remove_dir_all(dir).expect("cleanup");
}

#[test]
fn feedback_rate_limit_keeps_distinct_identities_separate() {
    assert_ne!(
        rate_limit_ip_identity("2001:db8::1").expect("ip"),
        rate_limit_ip_identity("2001:db8::2").expect("ip")
    );
    assert_ne!(
        rate_limit_public_key_identity(&public_key_hex(&signing_key(23))).expect("key"),
        rate_limit_public_key_identity(&public_key_hex(&signing_key(24))).expect("key")
    );
}
