use std::collections::BTreeMap;
use std::net::IpAddr;
use std::str::FromStr;
use std::time::{SystemTime, UNIX_EPOCH};

use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};

use super::{FileStore, LocalCasStore, WorldError, blake3_hex, to_canonical_cbor, validate_hash};

const FEEDBACK_NAMESPACE: &str = "feedback";
const FEEDBACK_RECORDS_DIR: &str = "records";
const FEEDBACK_EVENTS_DIR: &str = "events";
const FEEDBACK_NONCES_DIR: &str = "nonces";
const FEEDBACK_AUDIT_DIR: &str = "audit";
const FEEDBACK_SIGNATURE_PAYLOAD_VERSION: u8 = 1;
const MAX_TOKEN_LEN: usize = 128;

mod signing;
pub use signing::{
    public_key_hex_from_signing_key_hex, sign_feedback_append_request,
    sign_feedback_create_request, sign_feedback_tombstone_request,
};
mod replication;
pub use replication::FeedbackBlobRef;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeedbackAttachment {
    pub content_hash: String,
    pub size_bytes: u64,
    pub mime_type: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeedbackCreateRequest {
    pub feedback_id: String,
    pub author_public_key_hex: String,
    pub submit_ip: String,
    pub category: String,
    pub platform: String,
    pub game_version: String,
    pub content: String,
    #[serde(default)]
    pub attachments: Vec<FeedbackAttachment>,
    pub nonce: String,
    pub timestamp_ms: i64,
    pub expires_at_ms: i64,
    pub signature_hex: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeedbackAppendRequest {
    pub feedback_id: String,
    pub actor_public_key_hex: String,
    pub submit_ip: String,
    pub content: String,
    pub nonce: String,
    pub timestamp_ms: i64,
    pub expires_at_ms: i64,
    pub signature_hex: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeedbackTombstoneRequest {
    pub feedback_id: String,
    pub actor_public_key_hex: String,
    pub submit_ip: String,
    pub reason: String,
    pub nonce: String,
    pub timestamp_ms: i64,
    pub expires_at_ms: i64,
    pub signature_hex: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FeedbackActionKind {
    Create,
    Append,
    Tombstone,
}

impl FeedbackActionKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Create => "create",
            Self::Append => "append",
            Self::Tombstone => "tombstone",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeedbackRootRecord {
    pub feedback_id: String,
    pub author_public_key_hex: String,
    pub submit_ip: String,
    pub category: String,
    pub platform: String,
    pub game_version: String,
    pub content: String,
    #[serde(default)]
    pub attachments: Vec<FeedbackAttachment>,
    pub nonce: String,
    pub timestamp_ms: i64,
    pub expires_at_ms: i64,
    pub signature_hex: String,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeedbackEventRecord {
    pub feedback_id: String,
    pub event_id: String,
    pub action: FeedbackActionKind,
    pub actor_public_key_hex: String,
    pub submit_ip: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    pub nonce: String,
    pub timestamp_ms: i64,
    pub expires_at_ms: i64,
    pub signature_hex: String,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeedbackAuditRecord {
    pub audit_id: String,
    pub action: FeedbackActionKind,
    pub feedback_id: String,
    pub actor_public_key_hex: String,
    pub submit_ip: String,
    pub timestamp_ms: i64,
    pub accepted: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeedbackPublicView {
    pub feedback_id: String,
    pub author_public_key_hex: String,
    pub category: String,
    pub platform: String,
    pub game_version: String,
    pub content: String,
    #[serde(default)]
    pub attachments: Vec<FeedbackAttachment>,
    #[serde(default)]
    pub append_events: Vec<FeedbackPublicAppendEvent>,
    pub tombstoned: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tombstone_reason: Option<String>,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeedbackPublicAppendEvent {
    pub event_id: String,
    pub content: String,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeedbackMutationReceipt {
    pub feedback_id: String,
    pub action: FeedbackActionKind,
    pub event_id: String,
    pub audit_id: String,
    pub accepted: bool,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeedbackStoreConfig {
    pub rate_limit_window_ms: i64,
    pub max_actions_per_ip_window: u32,
    pub max_actions_per_pubkey_window: u32,
    pub max_clock_skew_ms: i64,
    pub max_content_bytes: usize,
    pub max_attachments: usize,
    pub max_attachment_bytes: u64,
}

impl Default for FeedbackStoreConfig {
    fn default() -> Self {
        Self {
            rate_limit_window_ms: 60_000,
            max_actions_per_ip_window: 20,
            max_actions_per_pubkey_window: 10,
            max_clock_skew_ms: 300_000,
            max_content_bytes: 4 * 1024,
            max_attachments: 8,
            max_attachment_bytes: 32 * 1024 * 1024,
        }
    }
}

#[derive(Debug, Clone)]
pub struct FeedbackStore {
    store: LocalCasStore,
    config: FeedbackStoreConfig,
}

impl FeedbackStore {
    pub fn new(store: LocalCasStore, config: FeedbackStoreConfig) -> Self {
        Self { store, config }
    }

    pub fn submit_feedback(
        &self,
        request: FeedbackCreateRequest,
    ) -> Result<FeedbackMutationReceipt, WorldError> {
        let feedback_id = request.feedback_id.clone();
        let actor_public_key_hex = request.author_public_key_hex.clone();
        let submit_ip = request.submit_ip.clone();
        let timestamp_ms = request.timestamp_ms;
        self.mutate_with_audit(
            FeedbackActionKind::Create,
            feedback_id.as_str(),
            actor_public_key_hex.as_str(),
            submit_ip.as_str(),
            timestamp_ms,
            |now_ms| self.submit_feedback_inner(request, now_ms),
        )
    }

    pub fn append_feedback(
        &self,
        request: FeedbackAppendRequest,
    ) -> Result<FeedbackMutationReceipt, WorldError> {
        let feedback_id = request.feedback_id.clone();
        let actor_public_key_hex = request.actor_public_key_hex.clone();
        let submit_ip = request.submit_ip.clone();
        let timestamp_ms = request.timestamp_ms;
        self.mutate_with_audit(
            FeedbackActionKind::Append,
            feedback_id.as_str(),
            actor_public_key_hex.as_str(),
            submit_ip.as_str(),
            timestamp_ms,
            |now_ms| self.append_feedback_inner(request, now_ms),
        )
    }

    pub fn tombstone_feedback(
        &self,
        request: FeedbackTombstoneRequest,
    ) -> Result<FeedbackMutationReceipt, WorldError> {
        let feedback_id = request.feedback_id.clone();
        let actor_public_key_hex = request.actor_public_key_hex.clone();
        let submit_ip = request.submit_ip.clone();
        let timestamp_ms = request.timestamp_ms;
        self.mutate_with_audit(
            FeedbackActionKind::Tombstone,
            feedback_id.as_str(),
            actor_public_key_hex.as_str(),
            submit_ip.as_str(),
            timestamp_ms,
            |now_ms| self.tombstone_feedback_inner(request, now_ms),
        )
    }

    pub fn read_feedback_public(
        &self,
        feedback_id: &str,
    ) -> Result<Option<FeedbackPublicView>, WorldError> {
        validate_token(feedback_id, "feedback_id")?;
        let root_path = feedback_root_path(feedback_id);
        let Some(_) = self.store.stat_file(root_path.as_str())? else {
            return Ok(None);
        };
        let root = self.read_feedback_root(feedback_id)?;
        let mut events = self.read_feedback_events(feedback_id)?;
        sort_feedback_events(&mut events);
        Ok(Some(to_public_view(&root, &events)))
    }

    pub fn list_feedback_public(&self) -> Result<Vec<FeedbackPublicView>, WorldError> {
        let files = self.store.list_files()?;
        let mut roots = Vec::new();
        for file in files {
            if !is_feedback_root_path(file.path.as_str()) {
                continue;
            }
            let feedback_id = extract_feedback_id_from_root_path(file.path.as_str())?;
            roots.push(self.read_feedback_root(feedback_id.as_str())?);
        }
        let mut views = Vec::new();
        for root in roots {
            let mut events = self.read_feedback_events(root.feedback_id.as_str())?;
            sort_feedback_events(&mut events);
            views.push(to_public_view(&root, &events));
        }
        views.sort_by_key(|view| std::cmp::Reverse(view.created_at_ms));
        Ok(views)
    }

    fn submit_feedback_inner(
        &self,
        request: FeedbackCreateRequest,
        now_ms: i64,
    ) -> Result<FeedbackMutationReceipt, WorldError> {
        validate_feedback_create_request(&request, &self.config)?;
        validate_signed_request_timestamps(
            request.timestamp_ms,
            request.expires_at_ms,
            now_ms,
            self.config.max_clock_skew_ms,
        )?;
        self.enforce_rate_limit(
            request.submit_ip.as_str(),
            request.author_public_key_hex.as_str(),
            now_ms,
        )?;

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
        self.claim_nonce(
            request.author_public_key_hex.as_str(),
            request.nonce.as_str(),
            now_ms,
        )?;

        let root_path = feedback_root_path(request.feedback_id.as_str());
        if self.store.stat_file(root_path.as_str())?.is_some() {
            return Err(WorldError::DistributedValidationFailed {
                reason: format!("feedback_id already exists: {}", request.feedback_id),
            });
        }

        let root_record = FeedbackRootRecord {
            feedback_id: request.feedback_id.clone(),
            author_public_key_hex: request.author_public_key_hex,
            submit_ip: request.submit_ip,
            category: request.category,
            platform: request.platform,
            game_version: request.game_version,
            content: request.content,
            attachments: request.attachments,
            nonce: request.nonce,
            timestamp_ms: request.timestamp_ms,
            expires_at_ms: request.expires_at_ms,
            signature_hex: request.signature_hex,
            created_at_ms: now_ms,
        };
        let root_bytes = serde_json::to_vec(&root_record)?;
        self.store
            .write_file(root_path.as_str(), root_bytes.as_slice())?;

        let event_id = blake3_hex(root_bytes.as_slice());
        Ok(FeedbackMutationReceipt {
            feedback_id: root_record.feedback_id,
            action: FeedbackActionKind::Create,
            event_id,
            audit_id: String::new(),
            accepted: true,
            created_at_ms: now_ms,
        })
    }

    fn append_feedback_inner(
        &self,
        request: FeedbackAppendRequest,
        now_ms: i64,
    ) -> Result<FeedbackMutationReceipt, WorldError> {
        validate_feedback_append_request(&request, &self.config)?;
        validate_signed_request_timestamps(
            request.timestamp_ms,
            request.expires_at_ms,
            now_ms,
            self.config.max_clock_skew_ms,
        )?;
        self.enforce_rate_limit(
            request.submit_ip.as_str(),
            request.actor_public_key_hex.as_str(),
            now_ms,
        )?;

        let root = self.read_feedback_root(request.feedback_id.as_str())?;
        ensure_author_matches(
            root.author_public_key_hex.as_str(),
            request.actor_public_key_hex.as_str(),
        )?;
        ensure_feedback_not_tombstoned(
            self.read_feedback_events(request.feedback_id.as_str())?
                .as_slice(),
        )?;
        let content_hash = blake3_hex(request.content.as_bytes());
        verify_signed_request(
            FeedbackSignedPayload::new(
                FeedbackActionKind::Append,
                request.feedback_id.as_str(),
                request.actor_public_key_hex.as_str(),
                content_hash.as_str(),
                request.nonce.as_str(),
                request.timestamp_ms,
                request.expires_at_ms,
            ),
            request.signature_hex.as_str(),
        )?;
        self.claim_nonce(
            request.actor_public_key_hex.as_str(),
            request.nonce.as_str(),
            now_ms,
        )?;

        let event_id = self.write_event(FeedbackEventRecord {
            feedback_id: request.feedback_id.clone(),
            event_id: String::new(),
            action: FeedbackActionKind::Append,
            actor_public_key_hex: request.actor_public_key_hex,
            submit_ip: request.submit_ip,
            content: Some(request.content),
            reason: None,
            nonce: request.nonce,
            timestamp_ms: request.timestamp_ms,
            expires_at_ms: request.expires_at_ms,
            signature_hex: request.signature_hex,
            created_at_ms: now_ms,
        })?;
        Ok(FeedbackMutationReceipt {
            feedback_id: request.feedback_id,
            action: FeedbackActionKind::Append,
            event_id,
            audit_id: String::new(),
            accepted: true,
            created_at_ms: now_ms,
        })
    }

    fn tombstone_feedback_inner(
        &self,
        request: FeedbackTombstoneRequest,
        now_ms: i64,
    ) -> Result<FeedbackMutationReceipt, WorldError> {
        validate_feedback_tombstone_request(&request, &self.config)?;
        validate_signed_request_timestamps(
            request.timestamp_ms,
            request.expires_at_ms,
            now_ms,
            self.config.max_clock_skew_ms,
        )?;
        self.enforce_rate_limit(
            request.submit_ip.as_str(),
            request.actor_public_key_hex.as_str(),
            now_ms,
        )?;

        let root = self.read_feedback_root(request.feedback_id.as_str())?;
        ensure_author_matches(
            root.author_public_key_hex.as_str(),
            request.actor_public_key_hex.as_str(),
        )?;
        ensure_feedback_not_tombstoned(
            self.read_feedback_events(request.feedback_id.as_str())?
                .as_slice(),
        )?;
        let reason_hash = blake3_hex(request.reason.as_bytes());
        verify_signed_request(
            FeedbackSignedPayload::new(
                FeedbackActionKind::Tombstone,
                request.feedback_id.as_str(),
                request.actor_public_key_hex.as_str(),
                reason_hash.as_str(),
                request.nonce.as_str(),
                request.timestamp_ms,
                request.expires_at_ms,
            ),
            request.signature_hex.as_str(),
        )?;
        self.claim_nonce(
            request.actor_public_key_hex.as_str(),
            request.nonce.as_str(),
            now_ms,
        )?;

        let event_id = self.write_event(FeedbackEventRecord {
            feedback_id: request.feedback_id.clone(),
            event_id: String::new(),
            action: FeedbackActionKind::Tombstone,
            actor_public_key_hex: request.actor_public_key_hex,
            submit_ip: request.submit_ip,
            content: None,
            reason: Some(request.reason),
            nonce: request.nonce,
            timestamp_ms: request.timestamp_ms,
            expires_at_ms: request.expires_at_ms,
            signature_hex: request.signature_hex,
            created_at_ms: now_ms,
        })?;
        Ok(FeedbackMutationReceipt {
            feedback_id: request.feedback_id,
            action: FeedbackActionKind::Tombstone,
            event_id,
            audit_id: String::new(),
            accepted: true,
            created_at_ms: now_ms,
        })
    }

    fn mutate_with_audit<F>(
        &self,
        action: FeedbackActionKind,
        feedback_id: &str,
        actor_public_key_hex: &str,
        submit_ip: &str,
        timestamp_ms: i64,
        mutate: F,
    ) -> Result<FeedbackMutationReceipt, WorldError>
    where
        F: FnOnce(i64) -> Result<FeedbackMutationReceipt, WorldError>,
    {
        let now_ms = now_unix_time_ms();
        let result = mutate(now_ms);
        let (accepted, reason) = match &result {
            Ok(_) => (true, None),
            Err(error) => (false, Some(format!("{error:?}"))),
        };
        let audit_id = self.write_audit_record(FeedbackAuditRecord {
            audit_id: String::new(),
            action,
            feedback_id: feedback_id.to_string(),
            actor_public_key_hex: actor_public_key_hex.to_string(),
            submit_ip: submit_ip.to_string(),
            timestamp_ms: timestamp_ms.max(now_ms),
            accepted,
            reason,
        })?;
        match result {
            Ok(mut receipt) => {
                receipt.audit_id = audit_id;
                Ok(receipt)
            }
            Err(error) => Err(error),
        }
    }

    fn write_event(&self, mut event: FeedbackEventRecord) -> Result<String, WorldError> {
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
        let event_bytes = to_canonical_cbor(&payload)?;
        let event_id = blake3_hex(event_bytes.as_slice());
        event.event_id = event_id.clone();
        let event_path = feedback_event_path(
            event.feedback_id.as_str(),
            event.created_at_ms,
            event.event_id.as_str(),
        );
        if self.store.stat_file(event_path.as_str())?.is_some() {
            return Err(WorldError::DistributedValidationFailed {
                reason: format!("feedback event already exists: {event_path}"),
            });
        }
        let event_json = serde_json::to_vec(&event)?;
        self.store
            .write_file(event_path.as_str(), event_json.as_slice())?;
        Ok(event_id)
    }

    fn write_audit_record(&self, mut record: FeedbackAuditRecord) -> Result<String, WorldError> {
        let payload = FeedbackAuditHashPayload {
            version: FEEDBACK_SIGNATURE_PAYLOAD_VERSION,
            action: record.action.as_str(),
            feedback_id: record.feedback_id.as_str(),
            actor_public_key_hex: record.actor_public_key_hex.as_str(),
            submit_ip: record.submit_ip.as_str(),
            timestamp_ms: record.timestamp_ms,
            accepted: record.accepted,
            reason: record.reason.as_deref(),
        };
        let payload_bytes = to_canonical_cbor(&payload)?;
        let audit_id = blake3_hex(payload_bytes.as_slice());
        record.audit_id = audit_id.clone();
        let audit_path = feedback_audit_path(record.timestamp_ms, audit_id.as_str());
        let audit_json = serde_json::to_vec(&record)?;
        self.store
            .write_file(audit_path.as_str(), audit_json.as_slice())?;
        Ok(audit_id)
    }

    fn read_feedback_root(&self, feedback_id: &str) -> Result<FeedbackRootRecord, WorldError> {
        let root_path = feedback_root_path(feedback_id);
        let bytes = self.store.read_file(root_path.as_str())?;
        Ok(serde_json::from_slice(&bytes)?)
    }

    fn read_feedback_events(
        &self,
        feedback_id: &str,
    ) -> Result<Vec<FeedbackEventRecord>, WorldError> {
        let files = self.store.list_files()?;
        let prefix = feedback_events_prefix(feedback_id);
        let mut events = Vec::new();
        for file in files {
            if !file.path.starts_with(prefix.as_str()) {
                continue;
            }
            let bytes = self.store.read_file(file.path.as_str())?;
            let event: FeedbackEventRecord = serde_json::from_slice(&bytes)?;
            events.push(event);
        }
        Ok(events)
    }

    fn claim_nonce(
        &self,
        public_key_hex: &str,
        nonce: &str,
        now_ms: i64,
    ) -> Result<(), WorldError> {
        validate_token(public_key_hex, "public_key_hex")?;
        validate_token(nonce, "nonce")?;
        let nonce_path = feedback_nonce_path(public_key_hex, nonce);
        if self.store.stat_file(nonce_path.as_str())?.is_some() {
            return Err(WorldError::DistributedValidationFailed {
                reason: format!("replay nonce detected for pubkey={public_key_hex} nonce={nonce}"),
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

    fn enforce_rate_limit(
        &self,
        submit_ip: &str,
        actor_public_key_hex: &str,
        now_ms: i64,
    ) -> Result<(), WorldError> {
        validate_ip(submit_ip)?;
        validate_token(actor_public_key_hex, "actor_public_key_hex")?;
        let counts = self.collect_recent_accepted_audit_counts(now_ms)?;
        let ip_identity = rate_limit_ip_identity(submit_ip)?;
        let public_key_identity = rate_limit_public_key_identity(actor_public_key_hex)?;
        let ip_count = counts.ip_counts.get(&ip_identity).copied().unwrap_or(0);
        if ip_count >= self.config.max_actions_per_ip_window {
            return Err(WorldError::DistributedValidationFailed {
                reason: format!(
                    "rate limit exceeded for ip={} count={} window_ms={}",
                    submit_ip, ip_count, self.config.rate_limit_window_ms
                ),
            });
        }
        let pubkey_count = counts
            .pubkey_counts
            .get(&public_key_identity)
            .copied()
            .unwrap_or(0);
        if pubkey_count >= self.config.max_actions_per_pubkey_window {
            return Err(WorldError::DistributedValidationFailed {
                reason: format!(
                    "rate limit exceeded for pubkey={} count={} window_ms={}",
                    actor_public_key_hex, pubkey_count, self.config.rate_limit_window_ms
                ),
            });
        }
        Ok(())
    }

    fn collect_recent_accepted_audit_counts(
        &self,
        now_ms: i64,
    ) -> Result<FeedbackRateLimitCounts, WorldError> {
        let files = self.store.list_files()?;
        let mut counts = FeedbackRateLimitCounts::default();
        let min_timestamp = now_ms.saturating_sub(self.config.rate_limit_window_ms.max(0));
        for file in files {
            if !file.path.starts_with(feedback_audit_prefix().as_str()) {
                continue;
            }
            if feedback_audit_path_timestamp_ms(file.path.as_str())
                .is_some_and(|timestamp_ms| timestamp_ms < min_timestamp)
            {
                continue;
            }
            let bytes = self.store.read_file(file.path.as_str())?;
            let record: FeedbackAuditRecord = serde_json::from_slice(&bytes)?;
            if !record.accepted {
                continue;
            }
            if record.timestamp_ms < min_timestamp {
                continue;
            }
            let ip_identity = rate_limit_ip_identity(record.submit_ip.as_str())?;
            let public_key_identity =
                rate_limit_public_key_identity(record.actor_public_key_hex.as_str())?;
            increment_counter(&mut counts.ip_counts, ip_identity.as_str());
            increment_counter(
                &mut counts.pubkey_counts,
                public_key_identity.as_str(),
            );
        }
        Ok(counts)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct FeedbackSignedPayload<'a> {
    version: u8,
    action: &'a str,
    feedback_id: &'a str,
    actor_public_key_hex: &'a str,
    content_hash: &'a str,
    nonce: &'a str,
    timestamp_ms: i64,
    expires_at_ms: i64,
}

impl<'a> FeedbackSignedPayload<'a> {
    fn new(
        action: FeedbackActionKind,
        feedback_id: &'a str,
        actor_public_key_hex: &'a str,
        content_hash: &'a str,
        nonce: &'a str,
        timestamp_ms: i64,
        expires_at_ms: i64,
    ) -> Self {
        Self {
            version: FEEDBACK_SIGNATURE_PAYLOAD_VERSION,
            action: action.as_str(),
            feedback_id,
            actor_public_key_hex,
            content_hash,
            nonce,
            timestamp_ms,
            expires_at_ms,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct FeedbackEventHashPayload<'a> {
    version: u8,
    action: &'a str,
    feedback_id: &'a str,
    actor_public_key_hex: &'a str,
    submit_ip: &'a str,
    content: Option<&'a str>,
    reason: Option<&'a str>,
    nonce: &'a str,
    timestamp_ms: i64,
    expires_at_ms: i64,
    signature_hex: &'a str,
    created_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct FeedbackAuditHashPayload<'a> {
    version: u8,
    action: &'a str,
    feedback_id: &'a str,
    actor_public_key_hex: &'a str,
    submit_ip: &'a str,
    timestamp_ms: i64,
    accepted: bool,
    reason: Option<&'a str>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct FeedbackNonceRecord {
    public_key_hex: String,
    nonce: String,
    claimed_at_ms: i64,
}

#[derive(Debug, Default)]
struct FeedbackRateLimitCounts {
    ip_counts: BTreeMap<String, u32>,
    pubkey_counts: BTreeMap<String, u32>,
}

fn validate_feedback_create_request(
    request: &FeedbackCreateRequest,
    config: &FeedbackStoreConfig,
) -> Result<(), WorldError> {
    validate_token(request.feedback_id.as_str(), "feedback_id")?;
    validate_public_key_hex(request.author_public_key_hex.as_str())?;
    validate_ip(request.submit_ip.as_str())?;
    validate_token(request.category.as_str(), "category")?;
    validate_token(request.platform.as_str(), "platform")?;
    validate_token(request.game_version.as_str(), "game_version")?;
    validate_token(request.nonce.as_str(), "nonce")?;
    validate_signature_hex(request.signature_hex.as_str())?;
    validate_feedback_content(
        request.content.as_str(),
        "content",
        config.max_content_bytes,
    )?;
    validate_attachments(request.attachments.as_slice(), config)?;
    Ok(())
}

fn validate_feedback_append_request(
    request: &FeedbackAppendRequest,
    config: &FeedbackStoreConfig,
) -> Result<(), WorldError> {
    validate_token(request.feedback_id.as_str(), "feedback_id")?;
    validate_public_key_hex(request.actor_public_key_hex.as_str())?;
    validate_ip(request.submit_ip.as_str())?;
    validate_token(request.nonce.as_str(), "nonce")?;
    validate_signature_hex(request.signature_hex.as_str())?;
    validate_feedback_content(
        request.content.as_str(),
        "append_content",
        config.max_content_bytes,
    )?;
    Ok(())
}

fn validate_feedback_tombstone_request(
    request: &FeedbackTombstoneRequest,
    config: &FeedbackStoreConfig,
) -> Result<(), WorldError> {
    validate_token(request.feedback_id.as_str(), "feedback_id")?;
    validate_public_key_hex(request.actor_public_key_hex.as_str())?;
    validate_ip(request.submit_ip.as_str())?;
    validate_token(request.nonce.as_str(), "nonce")?;
    validate_signature_hex(request.signature_hex.as_str())?;
    validate_feedback_content(
        request.reason.as_str(),
        "tombstone_reason",
        config.max_content_bytes,
    )?;
    Ok(())
}

fn validate_attachments(
    attachments: &[FeedbackAttachment],
    config: &FeedbackStoreConfig,
) -> Result<(), WorldError> {
    if attachments.len() > config.max_attachments {
        return Err(WorldError::DistributedValidationFailed {
            reason: format!(
                "attachments exceed max count: actual={} max={}",
                attachments.len(),
                config.max_attachments
            ),
        });
    }
    for attachment in attachments {
        validate_hash(attachment.content_hash.as_str())?;
        if attachment.size_bytes > config.max_attachment_bytes {
            return Err(WorldError::DistributedValidationFailed {
                reason: format!(
                    "attachment size exceeds max: hash={} actual={} max={}",
                    attachment.content_hash, attachment.size_bytes, config.max_attachment_bytes
                ),
            });
        }
        validate_token(attachment.mime_type.as_str(), "attachment_mime_type")?;
    }
    Ok(())
}

fn validate_signed_request_timestamps(
    timestamp_ms: i64,
    expires_at_ms: i64,
    now_ms: i64,
    max_clock_skew_ms: i64,
) -> Result<(), WorldError> {
    if expires_at_ms < timestamp_ms {
        return Err(WorldError::DistributedValidationFailed {
            reason: format!(
                "invalid signed request timestamp range: timestamp_ms={} expires_at_ms={}",
                timestamp_ms, expires_at_ms
            ),
        });
    }
    if now_ms > expires_at_ms {
        return Err(WorldError::DistributedValidationFailed {
            reason: format!(
                "signed request expired: now_ms={} expires_at_ms={}",
                now_ms, expires_at_ms
            ),
        });
    }
    let clock_drift = now_ms.saturating_sub(timestamp_ms).abs();
    if clock_drift > max_clock_skew_ms {
        return Err(WorldError::DistributedValidationFailed {
            reason: format!(
                "signed request clock skew too large: timestamp_ms={} now_ms={} max_clock_skew_ms={}",
                timestamp_ms, now_ms, max_clock_skew_ms
            ),
        });
    }
    Ok(())
}

fn verify_signed_request(
    payload: FeedbackSignedPayload<'_>,
    signature_hex: &str,
) -> Result<(), WorldError> {
    validate_public_key_hex(payload.actor_public_key_hex)?;
    validate_signature_hex(signature_hex)?;
    let payload_bytes = to_canonical_cbor(&payload)?;
    let public_key_bytes = decode_hex_array::<32>(
        payload.actor_public_key_hex,
        "feedback actor_public_key_hex",
    )?;
    let signature_bytes = decode_hex_array::<64>(signature_hex, "feedback signature_hex")?;
    let verifying_key = VerifyingKey::from_bytes(&public_key_bytes).map_err(|err| {
        WorldError::DistributedValidationFailed {
            reason: format!("invalid feedback public key bytes: {err}"),
        }
    })?;
    let signature = Signature::from_bytes(&signature_bytes);
    verifying_key
        .verify(payload_bytes.as_slice(), &signature)
        .map_err(|err| WorldError::DistributedValidationFailed {
            reason: format!("feedback signature verify failed: {err}"),
        })
}

fn decode_hex_array<const N: usize>(value: &str, label: &str) -> Result<[u8; N], WorldError> {
    let decoded = hex::decode(value).map_err(|error| WorldError::DistributedValidationFailed {
        reason: format!("invalid hex for {label}: {error}"),
    })?;
    if decoded.len() != N {
        return Err(WorldError::DistributedValidationFailed {
            reason: format!(
                "invalid length for {label}: expected={} actual={}",
                N,
                decoded.len()
            ),
        });
    }
    let mut bytes = [0_u8; N];
    bytes.copy_from_slice(decoded.as_slice());
    Ok(bytes)
}

fn feedback_create_content_hash(request: &FeedbackCreateRequest) -> Result<String, WorldError> {
    let payload = FeedbackCreateHashPayload {
        version: FEEDBACK_SIGNATURE_PAYLOAD_VERSION,
        feedback_id: request.feedback_id.as_str(),
        category: request.category.as_str(),
        platform: request.platform.as_str(),
        game_version: request.game_version.as_str(),
        content: request.content.as_str(),
        attachments: request.attachments.as_slice(),
    };
    let payload_bytes = to_canonical_cbor(&payload)?;
    Ok(blake3_hex(payload_bytes.as_slice()))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct FeedbackCreateHashPayload<'a> {
    version: u8,
    feedback_id: &'a str,
    category: &'a str,
    platform: &'a str,
    game_version: &'a str,
    content: &'a str,
    attachments: &'a [FeedbackAttachment],
}

fn ensure_author_matches(expected: &str, actual: &str) -> Result<(), WorldError> {
    if expected != actual {
        return Err(WorldError::DistributedValidationFailed {
            reason: format!(
                "feedback actor pubkey mismatch: expected={} actual={}",
                expected, actual
            ),
        });
    }
    Ok(())
}

fn ensure_feedback_not_tombstoned(events: &[FeedbackEventRecord]) -> Result<(), WorldError> {
    if events
        .iter()
        .any(|event| event.action == FeedbackActionKind::Tombstone)
    {
        return Err(WorldError::DistributedValidationFailed {
            reason: "feedback already tombstoned".to_string(),
        });
    }
    Ok(())
}

fn to_public_view(root: &FeedbackRootRecord, events: &[FeedbackEventRecord]) -> FeedbackPublicView {
    let mut append_events = Vec::new();
    let mut tombstone_reason = None;
    let mut updated_at_ms = root.created_at_ms;
    for event in events {
        updated_at_ms = updated_at_ms.max(event.created_at_ms);
        match event.action {
            FeedbackActionKind::Append => {
                if let Some(content) = event.content.as_deref() {
                    append_events.push(FeedbackPublicAppendEvent {
                        event_id: event.event_id.clone(),
                        content: content.to_string(),
                        created_at_ms: event.created_at_ms,
                    });
                }
            }
            FeedbackActionKind::Tombstone => {
                tombstone_reason = event.reason.clone();
            }
            FeedbackActionKind::Create => {}
        }
    }
    FeedbackPublicView {
        feedback_id: root.feedback_id.clone(),
        author_public_key_hex: root.author_public_key_hex.clone(),
        category: root.category.clone(),
        platform: root.platform.clone(),
        game_version: root.game_version.clone(),
        content: root.content.clone(),
        attachments: root.attachments.clone(),
        append_events,
        tombstoned: tombstone_reason.is_some(),
        tombstone_reason,
        created_at_ms: root.created_at_ms,
        updated_at_ms,
    }
}

fn sort_feedback_events(events: &mut [FeedbackEventRecord]) {
    events.sort_by(|left, right| {
        left.created_at_ms
            .cmp(&right.created_at_ms)
            .then_with(|| left.event_id.cmp(&right.event_id))
    });
}

fn validate_feedback_content(
    content: &str,
    label: &str,
    max_bytes: usize,
) -> Result<(), WorldError> {
    if content.trim().is_empty() {
        return Err(WorldError::DistributedValidationFailed {
            reason: format!("{label} cannot be empty"),
        });
    }
    if content.len() > max_bytes {
        return Err(WorldError::DistributedValidationFailed {
            reason: format!(
                "{label} exceeds max bytes: actual={} max={}",
                content.len(),
                max_bytes
            ),
        });
    }
    Ok(())
}

fn validate_token(value: &str, field: &str) -> Result<(), WorldError> {
    let normalized = value.trim();
    if normalized.is_empty() {
        return Err(WorldError::DistributedValidationFailed {
            reason: format!("{field} cannot be empty"),
        });
    }
    if normalized.len() > MAX_TOKEN_LEN {
        return Err(WorldError::DistributedValidationFailed {
            reason: format!(
                "{field} exceeds max length: actual={} max={}",
                normalized.len(),
                MAX_TOKEN_LEN
            ),
        });
    }
    if normalized.contains('/') || normalized.contains('\\') {
        return Err(WorldError::DistributedValidationFailed {
            reason: format!("{field} cannot contain path separators"),
        });
    }
    Ok(())
}

// Canonicalize only accounting identities, not signed payloads or stored records.
// Apply the same mapping when reading historical audit entries and checking requests.
fn rate_limit_ip_identity(value: &str) -> Result<String, WorldError> {
    IpAddr::from_str(value.trim())
        .map(|address| address.to_string())
        .map_err(|error| WorldError::DistributedValidationFailed {
            reason: format!("submit_ip is invalid ip address: {error}"),
        })
}

fn rate_limit_public_key_identity(value: &str) -> Result<String, WorldError> {
    decode_hex_array::<32>(value.trim(), "feedback public key").map(hex::encode)
}

fn validate_ip(value: &str) -> Result<(), WorldError> {
    let normalized = value.trim();
    IpAddr::from_str(normalized).map_err(|error| WorldError::DistributedValidationFailed {
        reason: format!("submit_ip is invalid ip address: {error}"),
    })?;
    Ok(())
}

fn validate_public_key_hex(value: &str) -> Result<(), WorldError> {
    let normalized = value.trim();
    decode_hex_array::<32>(normalized, "feedback public key")?;
    Ok(())
}

fn validate_signature_hex(value: &str) -> Result<(), WorldError> {
    let normalized = value.trim();
    decode_hex_array::<64>(normalized, "feedback signature")?;
    Ok(())
}

fn feedback_root_path(feedback_id: &str) -> String {
    format!("{FEEDBACK_NAMESPACE}/{FEEDBACK_RECORDS_DIR}/{feedback_id}/root.json")
}

fn feedback_events_prefix(feedback_id: &str) -> String {
    format!("{FEEDBACK_NAMESPACE}/{FEEDBACK_RECORDS_DIR}/{feedback_id}/{FEEDBACK_EVENTS_DIR}/")
}

fn feedback_event_path(feedback_id: &str, created_at_ms: i64, event_id: &str) -> String {
    format!(
        "{}{:020}-{}.json",
        feedback_events_prefix(feedback_id),
        created_at_ms.max(0),
        event_id
    )
}

fn feedback_nonce_path(public_key_hex: &str, nonce: &str) -> String {
    format!("{FEEDBACK_NAMESPACE}/{FEEDBACK_NONCES_DIR}/{public_key_hex}/{nonce}.json")
}

fn feedback_audit_prefix() -> String {
    format!("{FEEDBACK_NAMESPACE}/{FEEDBACK_AUDIT_DIR}/")
}

fn feedback_audit_path(timestamp_ms: i64, audit_id: &str) -> String {
    format!(
        "{}/{:020}-{}.json",
        feedback_audit_prefix().trim_end_matches('/'),
        timestamp_ms.max(0),
        audit_id
    )
}

fn feedback_audit_path_timestamp_ms(path: &str) -> Option<i64> {
    let prefix = feedback_audit_prefix();
    let rest = path.strip_prefix(prefix.as_str())?;
    let timestamp = rest.get(..20)?;
    if !timestamp.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    if rest.as_bytes().get(20).is_none_or(|byte| *byte != b'-') {
        return None;
    }
    timestamp.parse().ok()
}

fn is_feedback_root_path(path: &str) -> bool {
    path.starts_with(format!("{FEEDBACK_NAMESPACE}/{FEEDBACK_RECORDS_DIR}/").as_str())
        && path.ends_with("/root.json")
}

fn extract_feedback_id_from_root_path(path: &str) -> Result<String, WorldError> {
    let prefix = format!("{FEEDBACK_NAMESPACE}/{FEEDBACK_RECORDS_DIR}/");
    let suffix = "/root.json";
    if !path.starts_with(prefix.as_str()) || !path.ends_with(suffix) {
        return Err(WorldError::DistributedValidationFailed {
            reason: format!("invalid feedback root path: {path}"),
        });
    }
    let inner = &path[prefix.len()..path.len() - suffix.len()];
    validate_token(inner, "feedback_id")?;
    Ok(inner.to_string())
}

fn increment_counter(map: &mut BTreeMap<String, u32>, key: &str) {
    let entry = map.entry(key.to_string()).or_insert(0);
    *entry = entry.saturating_add(1);
}

fn now_unix_time_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| i64::try_from(duration.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}
#[cfg(test)]
mod tests;
