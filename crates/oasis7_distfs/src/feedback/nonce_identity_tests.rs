use super::*;

fn signed_create(
    author: &SigningKey,
    key_text: String,
    id: &str,
    nonce: &str,
) -> FeedbackCreateRequest {
    let mut request = FeedbackCreateRequest {
        feedback_id: id.into(),
        author_public_key_hex: key_text,
        submit_ip: "127.0.0.1".into(),
        category: "bug".into(),
        platform: "test".into(),
        game_version: "test".into(),
        content: "synthetic nonce identity regression".into(),
        attachments: vec![],
        nonce: nonce.into(),
        timestamp_ms: now_plus(0),
        expires_at_ms: now_plus(60_000),
        signature_hex: String::new(),
    };
    let hash = feedback_create_content_hash(&request).unwrap();
    request.signature_hex = sign_signature_hex(
        FeedbackSignedPayload::new(
            FeedbackActionKind::Create,
            &request.feedback_id,
            &request.author_public_key_hex,
            &hash,
            &request.nonce,
            request.timestamp_ms,
            request.expires_at_ms,
        ),
        author,
    );
    request
}

#[test]
fn equivalent_key_reuses_nonce_only_with_a_new_valid_signature() {
    let dir = temp_dir("nonce-key-alias");
    let author = signing_key(41);
    let key = public_key_hex(&author);
    let upper = key.to_ascii_uppercase();
    assert_ne!(key, upper);
    let store = FeedbackStore::new(LocalCasStore::new(&dir), FeedbackStoreConfig::default());
    let original = signed_create(&author, key.clone(), "nonce-original", "same-nonce");
    store.submit_feedback(original.clone()).unwrap();
    drop(store);
    let store = FeedbackStore::new(LocalCasStore::new(&dir), FeedbackStoreConfig::default());
    let alias = signed_create(&author, upper.clone(), "nonce-alias", "same-nonce");
    assert_ne!(original.signature_hex, alias.signature_hex);
    let hash = feedback_create_content_hash(&alias).unwrap();
    verify_signed_request(
        FeedbackSignedPayload::new(
            FeedbackActionKind::Create,
            &alias.feedback_id,
            &alias.author_public_key_hex,
            &hash,
            &alias.nonce,
            alias.timestamp_ms,
            alias.expires_at_ms,
        ),
        &alias.signature_hex,
    )
    .expect("alias uses a real newly signed payload, not a broken signature");
    assert!(
        matches!(store.submit_feedback(alias),
        Err(WorldError::DistributedValidationFailed { reason })
        if reason.starts_with("replay nonce detected")),
        "equivalent key identity must share consumed nonces after reopen"
    );
    assert!(store.read_feedback_public("nonce-alias").unwrap().is_none());
    store
        .submit_feedback(signed_create(&author, upper, "nonce-fresh", "fresh-nonce"))
        .unwrap();
    let other = signing_key(42);
    store
        .submit_feedback(signed_create(
            &other,
            public_key_hex(&other),
            "nonce-other",
            "same-nonce",
        ))
        .unwrap();
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn historical_mixed_case_nonce_is_preserved_and_replicated_root_retry_still_recovers() {
    let dir = temp_dir("historical-nonce-alias");
    let source_dir = temp_dir("nonce-root-source");
    let author = signing_key(43);
    let lower = public_key_hex(&author);
    let mixed: String = lower
        .chars()
        .enumerate()
        .map(|(index, ch)| {
            if index % 2 == 0 {
                ch.to_ascii_uppercase()
            } else {
                ch
            }
        })
        .collect();
    assert_ne!(mixed, lower);
    assert_ne!(mixed, lower.to_ascii_uppercase());
    let source = FeedbackStore::new(
        LocalCasStore::new(&source_dir),
        FeedbackStoreConfig::default(),
    );
    source
        .submit_feedback(signed_create(
            &author,
            mixed.clone(),
            "replicated-root",
            "historical-nonce",
        ))
        .unwrap();
    let root = source.read_feedback_root("replicated-root").unwrap();
    let target = FeedbackStore::new(LocalCasStore::new(&dir), FeedbackStoreConfig::default());
    // Preexisting raw-path record, including the nonce-write/root-write crash window.
    let path = feedback_nonce_path(&mixed, "historical-nonce");
    let bytes = serde_json::to_vec(&FeedbackNonceRecord {
        public_key_hex: mixed.clone(),
        nonce: "historical-nonce".into(),
        claimed_at_ms: root.created_at_ms,
    })
    .unwrap();
    target.store.write_file(&path, &bytes).unwrap();
    drop(target);
    let target = FeedbackStore::new(LocalCasStore::new(&dir), FeedbackStoreConfig::default());
    assert!(
        matches!(target.submit_feedback(signed_create(&author, lower, "legacy-alias", "historical-nonce")),
        Err(WorldError::DistributedValidationFailed { reason }) if reason.starts_with("replay nonce detected"))
    );
    assert!(
        target
            .read_feedback_public("legacy-alias")
            .unwrap()
            .is_none()
    );
    target
        .ingest_replicated_root_record(root.clone())
        .expect("resume original replicated root after nonce persisted");
    target
        .ingest_replicated_root_record(root.clone())
        .expect("exact root retry");
    assert_eq!(target.read_feedback_root("replicated-root").unwrap(), root);
    assert_eq!(target.store.read_file(&path).unwrap(), bytes);
    let _ = fs::remove_dir_all(dir);
    let _ = fs::remove_dir_all(source_dir);
}
