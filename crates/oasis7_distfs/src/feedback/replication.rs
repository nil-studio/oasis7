use serde::{Deserialize, Serialize};

use super::*;
use crate::FileMetadata;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeedbackBlobRef {
    pub path: String,
    pub content_hash: String,
    pub size_bytes: u64,
    pub actor_public_key_hex: String,
}

impl FeedbackStore {
    pub fn ingest_replicated_root_record(
        &self,
        root_record: FeedbackRootRecord,
    ) -> Result<FeedbackMutationReceipt, WorldError> {
        let feedback_id = root_record.feedback_id.clone();
        let actor_public_key_hex = root_record.author_public_key_hex.clone();
        let submit_ip = root_record.submit_ip.clone();
        let timestamp_ms = root_record.timestamp_ms;
        self.mutate_with_audit(
            FeedbackActionKind::Create,
            feedback_id.as_str(),
            actor_public_key_hex.as_str(),
            submit_ip.as_str(),
            timestamp_ms,
            |_| self.ingest_replicated_root_record_inner(root_record),
        )
    }

    pub fn ingest_replicated_event_record(
        &self,
        event_record: FeedbackEventRecord,
    ) -> Result<FeedbackMutationReceipt, WorldError> {
        let feedback_id = event_record.feedback_id.clone();
        let actor_public_key_hex = event_record.actor_public_key_hex.clone();
        let submit_ip = event_record.submit_ip.clone();
        let timestamp_ms = event_record.timestamp_ms;
        self.mutate_with_audit(
            event_record.action,
            feedback_id.as_str(),
            actor_public_key_hex.as_str(),
            submit_ip.as_str(),
            timestamp_ms,
            |_| self.ingest_replicated_event_record_inner(event_record),
        )
    }

    pub fn blob_ref_for_receipt(
        &self,
        receipt: &FeedbackMutationReceipt,
    ) -> Result<Option<FeedbackBlobRef>, WorldError> {
        match receipt.action {
            FeedbackActionKind::Create => {
                let path = feedback_root_path(receipt.feedback_id.as_str());
                let Some(metadata) = self.store.stat_file(path.as_str())? else {
                    return Ok(None);
                };
                let root = self.read_feedback_root(receipt.feedback_id.as_str())?;
                Ok(Some(FeedbackBlobRef {
                    path,
                    content_hash: metadata.content_hash,
                    size_bytes: metadata.size_bytes,
                    actor_public_key_hex: root.author_public_key_hex,
                }))
            }
            FeedbackActionKind::Append | FeedbackActionKind::Tombstone => {
                let metadata = self.find_event_file_metadata_with_record(
                    receipt.feedback_id.as_str(),
                    receipt.event_id.as_str(),
                )?;
                let Some((metadata, event_record)) = metadata else {
                    return Ok(None);
                };
                Ok(Some(FeedbackBlobRef {
                    path: metadata.path,
                    content_hash: metadata.content_hash,
                    size_bytes: metadata.size_bytes,
                    actor_public_key_hex: event_record.actor_public_key_hex,
                }))
            }
        }
    }

    fn ingest_replicated_root_record_inner(
        &self,
        root_record: FeedbackRootRecord,
    ) -> Result<FeedbackMutationReceipt, WorldError> {
        let request = FeedbackCreateRequest {
            feedback_id: root_record.feedback_id.clone(),
            author_public_key_hex: root_record.author_public_key_hex.clone(),
            submit_ip: root_record.submit_ip.clone(),
            category: root_record.category.clone(),
            platform: root_record.platform.clone(),
            game_version: root_record.game_version.clone(),
            content: root_record.content.clone(),
            attachments: root_record.attachments.clone(),
            nonce: root_record.nonce.clone(),
            timestamp_ms: root_record.timestamp_ms,
            expires_at_ms: root_record.expires_at_ms,
            signature_hex: root_record.signature_hex.clone(),
        };
        validate_feedback_create_request(&request, &self.config)?;
        validate_signed_timestamp_range_only(request.timestamp_ms, request.expires_at_ms)?;
        let content_hash = feedback_create_content_hash(&request)?;
        verify_signed_request(
            FeedbackSignedPayload::new(
                FeedbackActionKind::Create,
                request.feedback_id.as_str(),
                request.author_public_key_hex.as_str(),
                content_hash.as_str(),
                request.nonce.as_str(),
                request.timestamp_ms,
                request.expires_at_ms,
            ),
            request.signature_hex.as_str(),
        )?;

        let root_path = feedback_root_path(root_record.feedback_id.as_str());
        if self.store.stat_file(root_path.as_str())?.is_some() {
            let existing = self.read_feedback_root(root_record.feedback_id.as_str())?;
            if existing == root_record {
                let root_bytes = serde_json::to_vec(&root_record)?;
                return Ok(FeedbackMutationReceipt {
                    feedback_id: root_record.feedback_id,
                    action: FeedbackActionKind::Create,
                    event_id: blake3_hex(root_bytes.as_slice()),
                    audit_id: String::new(),
                    accepted: true,
                    created_at_ms: root_record.created_at_ms,
                });
            }
            return Err(WorldError::DistributedValidationFailed {
                reason: format!(
                    "replicated root conflict for feedback_id={}",
                    root_record.feedback_id
                ),
            });
        }

        self.claim_nonce_idempotent(
            root_record.author_public_key_hex.as_str(),
            root_record.nonce.as_str(),
            root_record.created_at_ms,
        )?;
        let root_bytes = serde_json::to_vec(&root_record)?;
        self.store
            .write_file(root_path.as_str(), root_bytes.as_slice())?;
        Ok(FeedbackMutationReceipt {
            feedback_id: root_record.feedback_id,
            action: FeedbackActionKind::Create,
            event_id: blake3_hex(root_bytes.as_slice()),
            audit_id: String::new(),
            accepted: true,
            created_at_ms: root_record.created_at_ms,
        })
    }

    fn ingest_replicated_event_record_inner(
        &self,
        event_record: FeedbackEventRecord,
    ) -> Result<FeedbackMutationReceipt, WorldError> {
        if event_record.action == FeedbackActionKind::Create {
            return Err(WorldError::DistributedValidationFailed {
                reason: "replicated event action create is invalid".to_string(),
            });
        }
        validate_feedback_event_record(&event_record, &self.config)?;
        validate_signed_timestamp_range_only(
            event_record.timestamp_ms,
            event_record.expires_at_ms,
        )?;
        let root = self.read_feedback_root(event_record.feedback_id.as_str())?;
        ensure_author_matches(
            root.author_public_key_hex.as_str(),
            event_record.actor_public_key_hex.as_str(),
        )?;
        let existing_events = self.read_feedback_events(event_record.feedback_id.as_str())?;
        if let Some(existing) = existing_events
            .iter()
            .find(|event| event.event_id == event_record.event_id)
        {
            if existing == &event_record {
                return Ok(FeedbackMutationReceipt {
                    feedback_id: event_record.feedback_id,
                    action: event_record.action,
                    event_id: event_record.event_id,
                    audit_id: String::new(),
                    accepted: true,
                    created_at_ms: event_record.created_at_ms,
                });
            }
            return Err(WorldError::DistributedValidationFailed {
                reason: format!(
                    "replicated event conflict feedback_id={} event_id={}",
                    event_record.feedback_id, event_record.event_id
                ),
            });
        }
        if event_record.action == FeedbackActionKind::Append {
            ensure_feedback_not_tombstoned(existing_events.as_slice())?;
        }

        let expected_event_id = compute_feedback_event_id(&event_record)?;
        if expected_event_id != event_record.event_id {
            return Err(WorldError::DistributedValidationFailed {
                reason: format!(
                    "replicated event_id mismatch expected={} actual={}",
                    expected_event_id, event_record.event_id
                ),
            });
        }
        let signed_content_hash = match event_record.action {
            FeedbackActionKind::Append => blake3_hex(
                event_record
                    .content
                    .as_deref()
                    .unwrap_or_default()
                    .as_bytes(),
            ),
            FeedbackActionKind::Tombstone => blake3_hex(
                event_record
                    .reason
                    .as_deref()
                    .unwrap_or_default()
                    .as_bytes(),
            ),
            FeedbackActionKind::Create => String::new(),
        };
        verify_signed_request(
            FeedbackSignedPayload::new(
                event_record.action,
                event_record.feedback_id.as_str(),
                event_record.actor_public_key_hex.as_str(),
                signed_content_hash.as_str(),
                event_record.nonce.as_str(),
                event_record.timestamp_ms,
                event_record.expires_at_ms,
            ),
            event_record.signature_hex.as_str(),
        )?;
        // Exact record retries returned above. Unsigned metadata can change the
        // event ID without changing the signed request; that is not a new append.
        if existing_events.iter().any(|existing| {
            existing.actor_public_key_hex == event_record.actor_public_key_hex
                && existing.nonce == event_record.nonce
        }) {
            return Err(WorldError::DistributedValidationFailed {
                reason: format!(
                    "replicated event nonce conflict feedback_id={} nonce={}",
                    event_record.feedback_id, event_record.nonce
                ),
            });
        }
        self.claim_nonce_idempotent(
            event_record.actor_public_key_hex.as_str(),
            event_record.nonce.as_str(),
            event_record.created_at_ms,
        )?;

        let event_path = feedback_event_path(
            event_record.feedback_id.as_str(),
            event_record.created_at_ms,
            event_record.event_id.as_str(),
        );
        let event_json = serde_json::to_vec(&event_record)?;
        self.store
            .write_file(event_path.as_str(), event_json.as_slice())?;
        Ok(FeedbackMutationReceipt {
            feedback_id: event_record.feedback_id,
            action: event_record.action,
            event_id: event_record.event_id,
            audit_id: String::new(),
            accepted: true,
            created_at_ms: event_record.created_at_ms,
        })
    }

    pub(crate) fn find_event_file_metadata_with_record(
        &self,
        feedback_id: &str,
        event_id: &str,
    ) -> Result<Option<(FileMetadata, FeedbackEventRecord)>, WorldError> {
        let files = self.store.list_files()?;
        let prefix = feedback_events_prefix(feedback_id);
        for file in files {
            if !file.path.starts_with(prefix.as_str()) {
                continue;
            }
            let bytes = self.store.read_file(file.path.as_str())?;
            let event: FeedbackEventRecord = serde_json::from_slice(&bytes)?;
            if event.event_id == event_id {
                return Ok(Some((file, event)));
            }
        }
        Ok(None)
    }

    fn claim_nonce_idempotent(
        &self,
        public_key_hex: &str,
        nonce: &str,
        now_ms: i64,
    ) -> Result<(), WorldError> {
        validate_token(public_key_hex, "public_key_hex")?;
        validate_token(nonce, "nonce")?;
        let nonce_path = feedback_nonce_path(public_key_hex, nonce);
        if self.store.stat_file(nonce_path.as_str())?.is_some() {
            let existing_bytes = self.store.read_file(nonce_path.as_str())?;
            let existing: FeedbackNonceRecord = serde_json::from_slice(&existing_bytes)?;
            if existing.public_key_hex == public_key_hex && existing.nonce == nonce {
                return Ok(());
            }
            return Err(WorldError::DistributedValidationFailed {
                reason: format!(
                    "nonce conflict for pubkey={} nonce={} in replicated ingest",
                    public_key_hex, nonce
                ),
            });
        }
        let nonce_record = FeedbackNonceRecord {
            public_key_hex: public_key_hex.to_string(),
            nonce: nonce.to_string(),
            claimed_at_ms: now_ms,
        };
        let nonce_bytes = serde_json::to_vec(&nonce_record)?;
        self.store
            .write_file(nonce_path.as_str(), nonce_bytes.as_slice())?;
        Ok(())
    }
}

fn validate_feedback_event_record(
    event_record: &FeedbackEventRecord,
    config: &FeedbackStoreConfig,
) -> Result<(), WorldError> {
    validate_token(event_record.feedback_id.as_str(), "feedback_id")?;
    validate_token(event_record.event_id.as_str(), "event_id")?;
    validate_public_key_hex(event_record.actor_public_key_hex.as_str())?;
    validate_ip(event_record.submit_ip.as_str())?;
    validate_token(event_record.nonce.as_str(), "nonce")?;
    validate_signature_hex(event_record.signature_hex.as_str())?;
    match event_record.action {
        FeedbackActionKind::Append => {
            let Some(content) = event_record.content.as_deref() else {
                return Err(WorldError::DistributedValidationFailed {
                    reason: "replicated append event missing content".to_string(),
                });
            };
            if event_record.reason.is_some() {
                return Err(WorldError::DistributedValidationFailed {
                    reason: "replicated append event must not carry reason".to_string(),
                });
            }
            validate_feedback_content(
                content,
                "replicated_append_content",
                config.max_content_bytes,
            )
        }
        FeedbackActionKind::Tombstone => {
            let Some(reason) = event_record.reason.as_deref() else {
                return Err(WorldError::DistributedValidationFailed {
                    reason: "replicated tombstone event missing reason".to_string(),
                });
            };
            if event_record.content.is_some() {
                return Err(WorldError::DistributedValidationFailed {
                    reason: "replicated tombstone event must not carry content".to_string(),
                });
            }
            validate_feedback_content(
                reason,
                "replicated_tombstone_reason",
                config.max_content_bytes,
            )
        }
        FeedbackActionKind::Create => Err(WorldError::DistributedValidationFailed {
            reason: "replicated event action create is invalid".to_string(),
        }),
    }
}

fn validate_signed_timestamp_range_only(
    timestamp_ms: i64,
    expires_at_ms: i64,
) -> Result<(), WorldError> {
    if expires_at_ms < timestamp_ms {
        return Err(WorldError::DistributedValidationFailed {
            reason: format!(
                "invalid signed request timestamp range: timestamp_ms={} expires_at_ms={}",
                timestamp_ms, expires_at_ms
            ),
        });
    }
    Ok(())
}

fn compute_feedback_event_id(event: &FeedbackEventRecord) -> Result<String, WorldError> {
    let payload = FeedbackEventHashPayload {
        version: FEEDBACK_SIGNATURE_PAYLOAD_VERSION,
        action: event.action.as_str(),
        feedback_id: event.feedback_id.as_str(),
        actor_public_key_hex: event.actor_public_key_hex.as_str(),
        submit_ip: event.submit_ip.as_str(),
        content: event.content.as_deref(),
        reason: event.reason.as_deref(),
        nonce: event.nonce.as_str(),
        timestamp_ms: event.timestamp_ms,
        expires_at_ms: event.expires_at_ms,
        signature_hex: event.signature_hex.as_str(),
        created_at_ms: event.created_at_ms,
    };
    let bytes = to_canonical_cbor(&payload)?;
    Ok(blake3_hex(bytes.as_slice()))
}
