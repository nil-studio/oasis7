use std::fmt;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Condvar, Mutex};

use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use oasis7_distfs::{
    FeedbackAppendRequest, FeedbackCreateRequest, FeedbackMutationReceipt, FeedbackStore,
    FeedbackTombstoneRequest, LocalCasStore, build_feedback_announce_from_receipt,
};
use oasis7_proto::distributed_dht as proto_dht;
use oasis7_proto::world_error::WorldError as ProtoWorldError;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

use crate::consensus_support::reserve_action_payload_bytes;
pub(crate) use crate::node_runtime_state::RuntimeState;
use crate::runtime_util::now_unix_ms;
use crate::{
    NodeCommittedActionBatch, NodeCommittedActionBatchesHandle, NodeConfig, NodeConsensusAction,
    NodeConsensusProgressObserver, NodeError, NodeExecutionHook,
    NodeMainTokenControllerBindingConfig, NodeMainTokenControllerSignerPolicy,
    NodeReplicationNetworkHandle, NodeRuntime, REPLICATED_EXECUTION_INPUT_ACTION_ID,
    REPLICATED_EXECUTION_INPUT_SUBMITTER, compute_consensus_action_root,
    validate_consensus_action_root, validate_replicated_execution_input_actions,
};

#[path = "node_bft_action_validation.rs"]
mod bft_action_validation;
#[path = "node_runtime_gameplay_origin.rs"]
mod gameplay_origin;

const CONSENSUS_ACTION_PAYLOAD_ENVELOPE_VERSION: u8 = 1;
const MAIN_TOKEN_ACTION_AUTH_PAYLOAD_VERSION: u8 = 1;
const MAIN_TOKEN_TRANSFER_AUTH_SIGNATURE_V1_PREFIX: &str = "octransferauth:v1:";
const MAIN_TOKEN_TRANSFER_AUTH_SIGNATURE_V2_PREFIX: &str = "octransferauth:v2:";
const MAIN_TOKEN_CLAIM_AUTH_SIGNATURE_V1_PREFIX: &str = "occlaimauth:v1:";
const MAIN_TOKEN_GENESIS_AUTH_SIGNATURE_V1_PREFIX: &str = "ocgenesisauth:v1:";
const MAIN_TOKEN_TREASURY_AUTH_SIGNATURE_V1_PREFIX: &str = "octreasuryauth:v1:";
const MAIN_TOKEN_RESTRICTED_CLAIM_LIVEOPS_POOL_TOP_UP_AUTH_SIGNATURE_V1_PREFIX: &str =
    "ocrestrictedclaimliveopspoolauth:v1:";
const MAIN_TOKEN_RESTRICTED_GRANT_ADMIN_REGISTRY_AUTH_SIGNATURE_V1_PREFIX: &str =
    "ocrestrictedgrantadminauth:v1:";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct LocalConsensusActionPayloadEnvelope {
    version: u8,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    gameplay_submission_origin:
        Option<oasis7_proto::gameplay_submission_origin::GameplaySubmissionOrigin>,
    #[serde(default)]
    auth: Option<LocalConsensusActionAuthEnvelope>,
    body: LocalConsensusActionPayloadBody,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
enum LocalConsensusActionAuthEnvelope {
    MainTokenAction(LocalMainTokenActionAuthProof),
    GovernanceAction(JsonValue),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum LocalMainTokenActionAuthScheme {
    Ed25519,
    ThresholdEd25519,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct LocalMainTokenActionParticipantSignature {
    public_key: String,
    signature: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct LocalMainTokenActionAuthProof {
    scheme: LocalMainTokenActionAuthScheme,
    account_id: String,
    #[serde(default)]
    public_key: Option<String>,
    #[serde(default)]
    signature: Option<String>,
    #[serde(default)]
    threshold: Option<u16>,
    #[serde(default)]
    participant_signatures: Vec<LocalMainTokenActionParticipantSignature>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
enum LocalConsensusActionPayloadBody {
    RuntimeAction {
        action: JsonValue,
    },
    SimulatorAction {
        action: JsonValue,
        #[serde(default)]
        submitter: JsonValue,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct LocalMainTokenActionSigningEnvelope<'a> {
    version: u8,
    operation: &'static str,
    account_id: &'a str,
    public_key: &'a str,
    action: LocalMainTokenActionSigningPayload<'a>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "type", content = "data")]
enum LocalMainTokenActionSigningPayload<'a> {
    TransferMainToken(LocalTransferMainTokenSigningData<'a>),
    ClaimMainTokenVesting(LocalClaimMainTokenVestingSigningData<'a>),
    InitializeMainTokenGenesis(LocalInitializeMainTokenGenesisSigningData),
    DistributeMainTokenTreasury(LocalDistributeMainTokenTreasurySigningData),
    TopUpRestrictedStarterClaimLiveopsPool(
        LocalTopUpRestrictedStarterClaimLiveopsPoolSigningData<'a>,
    ),
    UpdateRestrictedStarterClaimAdminRegistry(
        LocalUpdateRestrictedStarterClaimAdminRegistrySigningData,
    ),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct LocalTransferMainTokenSigningData<'a> {
    from_account_id: &'a str,
    to_account_id: &'a str,
    amount: u64,
    nonce: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    asset_id: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    memo: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    chain_id: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    network_id: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tx_version: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tx_type: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    valid_until_unix_ms: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_fee: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    fee_asset_id: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    application_payload_hash: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    client_request_id: Option<&'a str>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct LocalClaimMainTokenVestingSigningData<'a> {
    bucket_id: &'a str,
    beneficiary: &'a str,
    nonce: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct LocalInitializeMainTokenGenesisSigningData {
    allocations: Vec<LocalMainTokenGenesisAllocationPlan>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct LocalDistributeMainTokenTreasurySigningData {
    proposal_id: u64,
    distribution_id: String,
    bucket_id: String,
    distributions: Vec<LocalMainTokenTreasuryDistribution>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct LocalTopUpRestrictedStarterClaimLiveopsPoolSigningData<'a> {
    controller_account_id: &'a str,
    top_up_id: &'a str,
    amount: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct LocalUpdateRestrictedStarterClaimAdminRegistrySigningData {
    controller_account_id: String,
    next_admin_account_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct LocalMainTokenGenesisAllocationPlan {
    bucket_id: String,
    ratio_bps: u32,
    recipient: String,
    cliff_epochs: u64,
    linear_unlock_epochs: u64,
    start_epoch: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct LocalMainTokenTreasuryDistribution {
    account_id: String,
    amount: u64,
}

impl fmt::Debug for NodeRuntime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NodeRuntime")
            .field("config", &self.config)
            .field(
                "has_replication_network",
                &self.replication_network.is_some(),
            )
            .field(
                "replication_network_consensus_enabled",
                &self.replication_network_consensus_enabled,
            )
            .field("has_feedback_store", &self.feedback_store.is_some())
            .field("has_execution_hook", &self.execution_hook.is_some())
            .field(
                "has_consensus_progress_observer",
                &(self.consensus_progress_observer.is_some()
                    || self.consensus_progress_observer_dispatcher.is_some()),
            )
            .field("running", &self.running.load(Ordering::SeqCst))
            .finish()
    }
}

impl NodeRuntime {
    /// Shared BFT proposal admission for runtime methods and worker threads.
    /// The worker must use the same installed execution hook as the runtime so
    /// governance preflight cannot drift between ingress paths.
    pub fn new(config: NodeConfig) -> Self {
        let feedback_store = config.feedback_p2p.as_ref().and_then(|feedback_config| {
            config.replication.as_ref().map(|replication_config| {
                Arc::new(FeedbackStore::new(
                    LocalCasStore::new(replication_config.root_dir.join("store")),
                    feedback_config.store.clone(),
                ))
            })
        });
        Self {
            config,
            bft_config: None,
            bft_v2_config: None,
            local_execution_bootstrap: None,
            local_execution_genesis_bootstrap: None,
            replication_network: None,
            replication_network_consensus_enabled: true,
            gossip_endpoint: None,
            feedback_store,
            pending_feedback_announces: Arc::new(Mutex::new(Vec::new())),
            execution_hook: None,
            consensus_progress_observer: None,
            consensus_progress_observer_dispatcher: None,
            pending_consensus_actions: Arc::new(Mutex::new(Vec::new())),
            pending_consensus_action_queue_bytes: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            committed_action_batches: Arc::new((Mutex::new(Vec::new()), Condvar::new())),
            running: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            replica_maintenance_dht: None,
            state: Arc::new(Mutex::new(RuntimeState::default())),
            stop_tx: None,
            worker: None,
            #[cfg(test)]
            fail_next_worker_spawn: false,
            #[cfg(test)]
            fail_next_observer_worker_spawn: false,
        }
    }

    pub fn with_replication_network(mut self, network: NodeReplicationNetworkHandle) -> Self {
        self.replication_network = Some(network);
        self
    }

    pub fn with_replication_network_consensus_enabled(mut self, enabled: bool) -> Self {
        self.replication_network_consensus_enabled = enabled;
        self
    }

    pub fn with_execution_hook<T>(mut self, hook: T) -> Self
    where
        T: NodeExecutionHook + 'static,
    {
        self.execution_hook = Some(Arc::new(Mutex::new(Box::new(hook))));
        self
    }

    /// Drive the existing hook's bounded maintenance without resuming execution.
    pub fn advance_stopped_storage_maintenance(&mut self) -> Result<bool, String> {
        if self.running.load(Ordering::SeqCst) || self.worker.is_some() {
            return Err("stopped storage maintenance requires a fully stopped Node".into());
        }
        self.execution_hook
            .as_ref()
            .ok_or("stopped storage maintenance requires an execution hook")?
            .lock()
            .map_err(|_| "execution hook lock poisoned during stopped storage maintenance")?
            .advance_stopped_storage_maintenance()
    }

    /// Seed an explicit local execution boundary before consensus starts.
    /// This is reserved for a caller that has persisted and independently
    /// validated the corresponding runtime world; ordinary nodes remain at
    /// the genesis height.
    pub fn with_local_execution_bootstrap(
        mut self,
        bootstrap: crate::NodeExecutionBootstrap,
    ) -> Self {
        self.local_execution_bootstrap = Some(bootstrap);
        self
    }

    /// Seed a fresh local Node from a hook-issued, qualified InitialActivation
    /// capability. Ordinary callers and hooks cannot use the positive-height
    /// bootstrap API to bypass this genesis-specific boundary.
    pub fn with_local_execution_genesis_bootstrap(
        mut self,
        bootstrap: crate::NodeExecutionGenesisBootstrap,
    ) -> Self {
        self.local_execution_genesis_bootstrap = Some(bootstrap);
        self
    }

    pub fn with_consensus_progress_observer<T>(mut self, observer: T) -> Self
    where
        T: NodeConsensusProgressObserver + 'static,
    {
        self.consensus_progress_observer = Some(Box::new(observer));
        self
    }

    pub fn with_replica_maintenance_dht(
        mut self,
        dht: Arc<dyn proto_dht::DistributedDht<ProtoWorldError> + Send + Sync>,
    ) -> Self {
        self.replica_maintenance_dht = Some(dht);
        self
    }

    pub fn config(&self) -> &NodeConfig {
        &self.config
    }

    pub fn submit_consensus_action_payload(
        &self,
        action_id: u64,
        payload_cbor: Vec<u8>,
    ) -> Result<(), NodeError> {
        self.submit_consensus_action_payload_as_player(
            self.config.player_id.clone(),
            action_id,
            payload_cbor,
        )
    }

    pub fn submit_consensus_action_payload_as_player(
        &self,
        player_id: impl Into<String>,
        action_id: u64,
        payload_cbor: Vec<u8>,
    ) -> Result<(), NodeError> {
        let player_id = player_id.into();
        let player_id = player_id.trim();
        if player_id.is_empty() {
            return Err(NodeError::Consensus {
                reason: "submitter player_id cannot be empty".to_string(),
            });
        }
        if player_id != self.config.player_id {
            return Err(NodeError::Consensus {
                reason: format!(
                    "submitter player_id mismatch expected={} actual={}",
                    self.config.player_id, player_id
                ),
            });
        }
        if action_id == REPLICATED_EXECUTION_INPUT_ACTION_ID
            || player_id == REPLICATED_EXECUTION_INPUT_SUBMITTER
        {
            return Err(NodeError::Consensus {
                reason: "reserved replicated execution input identity is not available to player actions"
                    .to_string(),
            });
        }
        if payload_cbor.len() > self.config.max_consensus_action_payload_bytes {
            return Err(NodeError::Consensus {
                reason: format!(
                    "consensus action payload too large: bytes={} limit={}",
                    payload_cbor.len(),
                    self.config.max_consensus_action_payload_bytes
                ),
            });
        }
        if self.bft_config.is_some() {
            self.validate_bft_payload_admission(&payload_cbor)?;
        }
        if self.bft_v2_config.is_some() {
            self.validate_bft_v2_payload_admission(&payload_cbor)?;
        }
        validate_consensus_action_payload_auth(&self.config, payload_cbor.as_slice())?;
        // BFT V2 admission above is stateful: its execution hook validates the
        // signed governance action against the exact verified V2 parent cursor.
        // The legacy preflight uses the ordinary governance path and can reject
        // a valid V2 transition under a different, non-BFT parent projection.
        if self.bft_v2_config.is_none() {
            governance_auth::preflight(self, payload_cbor.as_slice())?;
        }
        let action = NodeConsensusAction::from_payload(
            action_id,
            self.config.player_id.clone(),
            payload_cbor,
        )
        .map_err(|err| NodeError::Consensus { reason: err.reason })?;
        let mut pending = self
            .pending_consensus_actions
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if (self.bft_config.is_some() || self.bft_v2_config.is_some())
            && bft_action_validation::reject_conflicting_queue_id(&pending, &action)?
        {
            return Ok(());
        }
        if pending.len() >= self.config.max_pending_consensus_actions {
            return Err(NodeError::Consensus {
                reason: format!(
                    "pending consensus actions queue saturated: len={} limit={}",
                    pending.len(),
                    self.config.max_pending_consensus_actions
                ),
            });
        }
        reserve_action_payload_bytes(
            &self.pending_consensus_action_queue_bytes,
            self.config.max_pending_consensus_action_queue_bytes,
            action.payload_cbor.len(),
        )?;
        pending.push(action);
        Ok(())
    }

    pub fn submit_feedback(
        &self,
        request: FeedbackCreateRequest,
    ) -> Result<FeedbackMutationReceipt, NodeError> {
        let store = self.require_feedback_store()?;
        let receipt = store
            .submit_feedback(request)
            .map_err(node_feedback_error)?;
        self.enqueue_feedback_announce(store, &receipt)?;
        Ok(receipt)
    }

    pub fn append_feedback(
        &self,
        request: FeedbackAppendRequest,
    ) -> Result<FeedbackMutationReceipt, NodeError> {
        let store = self.require_feedback_store()?;
        let receipt = store
            .append_feedback(request)
            .map_err(node_feedback_error)?;
        self.enqueue_feedback_announce(store, &receipt)?;
        Ok(receipt)
    }

    pub fn tombstone_feedback(
        &self,
        request: FeedbackTombstoneRequest,
    ) -> Result<FeedbackMutationReceipt, NodeError> {
        let store = self.require_feedback_store()?;
        let receipt = store
            .tombstone_feedback(request)
            .map_err(node_feedback_error)?;
        self.enqueue_feedback_announce(store, &receipt)?;
        Ok(receipt)
    }

    pub fn drain_committed_action_batches(&self) -> Vec<NodeCommittedActionBatch> {
        let (committed_lock, _) = &*self.committed_action_batches;
        let mut committed = committed_lock
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        std::mem::take(&mut *committed)
    }

    pub fn committed_action_batches_handle(&self) -> NodeCommittedActionBatchesHandle {
        NodeCommittedActionBatchesHandle {
            state: Arc::clone(&self.committed_action_batches),
        }
    }

    fn require_feedback_store(&self) -> Result<&Arc<FeedbackStore>, NodeError> {
        self.feedback_store
            .as_ref()
            .ok_or_else(|| NodeError::Replication {
                reason: "feedback_p2p is not configured".to_string(),
            })
    }

    fn enqueue_feedback_announce(
        &self,
        store: &FeedbackStore,
        receipt: &FeedbackMutationReceipt,
    ) -> Result<(), NodeError> {
        let now_ms = now_unix_ms();
        let announce = build_feedback_announce_from_receipt(
            store,
            self.config.world_id.as_str(),
            receipt,
            now_ms,
        )
        .map_err(node_feedback_error)?;
        let max_pending = self
            .config
            .feedback_p2p
            .as_ref()
            .map(|config| {
                config
                    .max_outgoing_announces_per_tick
                    .max(1)
                    .saturating_mul(64)
            })
            .unwrap_or(64);
        let mut pending = self
            .pending_feedback_announces
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if pending.len() >= max_pending {
            return Err(NodeError::Replication {
                reason: format!(
                    "feedback announce queue saturated: len={} limit={}",
                    pending.len(),
                    max_pending
                ),
            });
        }
        pending.push(announce);
        Ok(())
    }
}

fn decode_local_consensus_action_payload_envelope(
    payload_cbor: &[u8],
) -> Result<LocalConsensusActionPayloadEnvelope, String> {
    gameplay_origin::validate_metadata_presence(payload_cbor)?;
    match serde_cbor::from_slice::<LocalConsensusActionPayloadEnvelope>(payload_cbor) {
        Ok(envelope) => {
            gameplay_origin::validate_envelope(&envelope)?;
            Ok(envelope)
        }
        Err(error) if gameplay_origin::recognized_metadata(payload_cbor) => {
            Err(format!("invalid gameplay origin envelope: {error}"))
        }
        Err(_) => Ok(LocalConsensusActionPayloadEnvelope {
            version: CONSENSUS_ACTION_PAYLOAD_ENVELOPE_VERSION,
            gameplay_submission_origin: None,
            auth: None,
            body: LocalConsensusActionPayloadBody::RuntimeAction {
                action: serde_cbor::from_slice::<JsonValue>(payload_cbor)
                    .map_err(|err| format!("legacy runtime action decode failed: {err}"))?,
            },
        }),
    }
}

fn local_main_token_action_auth_required(action: &JsonValue) -> bool {
    matches!(
        local_runtime_action_kind(action),
        Some("TransferMainToken")
            | Some("ClaimMainTokenVesting")
            | Some("InitializeMainTokenGenesis")
            | Some("DistributeMainTokenTreasury")
            | Some("TopUpRestrictedStarterClaimLiveopsPool")
            | Some("UpdateRestrictedStarterClaimAdminRegistry")
    )
}

fn local_runtime_action_kind(action: &JsonValue) -> Option<&str> {
    action.get("type").and_then(JsonValue::as_str)
}

fn local_runtime_action_data(
    action: &JsonValue,
) -> Result<&serde_json::Map<String, JsonValue>, String> {
    action
        .get("data")
        .and_then(JsonValue::as_object)
        .ok_or_else(|| "runtime action missing object data".to_string())
}

fn verify_local_main_token_action_auth(
    action: &JsonValue,
    proof: &LocalMainTokenActionAuthProof,
    controller_binding: &NodeMainTokenControllerBindingConfig,
) -> Result<(), String> {
    let account_id =
        normalize_required_field(proof.account_id.as_str(), "main token auth account_id")?;
    match proof.scheme {
        LocalMainTokenActionAuthScheme::Ed25519 => {
            let public_key = normalize_public_key_field(
                proof.public_key.as_deref().ok_or_else(|| {
                    "main token auth public_key is required for ed25519".to_string()
                })?,
                "main token auth public key",
            )?;
            let payload = build_local_main_token_action_signing_payload(
                action,
                account_id.as_str(),
                public_key.as_str(),
            )?;
            validate_local_main_token_action_account_binding(
                action,
                account_id.as_str(),
                Some(public_key.as_str()),
                controller_binding,
            )?;
            enforce_local_controller_signer_policy(
                action,
                account_id.as_str(),
                controller_binding,
                1,
                &[public_key.as_str()],
            )?;
            verify_local_main_token_action_signature(
                action,
                public_key.as_str(),
                proof.signature.as_deref().ok_or_else(|| {
                    "main token auth signature is required for ed25519".to_string()
                })?,
                payload.as_slice(),
            )
        }
        LocalMainTokenActionAuthScheme::ThresholdEd25519 => {
            let threshold = proof.threshold.ok_or_else(|| {
                "main token auth threshold is required for threshold_ed25519".to_string()
            })?;
            if threshold == 0 {
                return Err("main token auth threshold must be > 0".to_string());
            }
            validate_local_main_token_action_account_binding(
                action,
                account_id.as_str(),
                None,
                controller_binding,
            )?;
            if proof.participant_signatures.len() < threshold as usize {
                return Err(format!(
                    "main token auth participant count below threshold: participants={} threshold={threshold}",
                    proof.participant_signatures.len()
                ));
            }
            let mut signer_public_keys = Vec::with_capacity(proof.participant_signatures.len());
            let mut seen = std::collections::BTreeSet::new();
            for participant in &proof.participant_signatures {
                let public_key = normalize_public_key_field(
                    participant.public_key.as_str(),
                    "main token auth participant public key",
                )?;
                if !seen.insert(public_key.clone()) {
                    return Err(format!(
                        "duplicate main token auth participant public key: {public_key}"
                    ));
                }
                let payload = build_local_main_token_action_signing_payload(
                    action,
                    account_id.as_str(),
                    public_key.as_str(),
                )?;
                verify_local_main_token_action_signature(
                    action,
                    public_key.as_str(),
                    participant.signature.as_str(),
                    payload.as_slice(),
                )?;
                signer_public_keys.push(public_key);
            }
            let signer_refs = signer_public_keys
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>();
            enforce_local_controller_signer_policy(
                action,
                account_id.as_str(),
                controller_binding,
                threshold,
                signer_refs.as_slice(),
            )
        }
    }
}

fn build_local_main_token_action_signing_payload(
    action: &JsonValue,
    account_id: &str,
    public_key: &str,
) -> Result<Vec<u8>, String> {
    let envelope = LocalMainTokenActionSigningEnvelope {
        version: MAIN_TOKEN_ACTION_AUTH_PAYLOAD_VERSION,
        operation: local_main_token_action_operation(action)?,
        account_id,
        public_key,
        action: build_local_main_token_action_signing_action(action)?,
    };
    serde_json::to_vec(&envelope)
        .map_err(|err| format!("encode main token auth signing payload failed: {err}"))
}

fn build_local_main_token_action_signing_action(
    action: &JsonValue,
) -> Result<LocalMainTokenActionSigningPayload<'_>, String> {
    let action_kind = local_runtime_action_kind(action)
        .ok_or_else(|| "runtime action missing type".to_string())?;
    let data = local_runtime_action_data(action)?;
    match action_kind {
        "TransferMainToken" => Ok(LocalMainTokenActionSigningPayload::TransferMainToken(
            LocalTransferMainTokenSigningData {
                from_account_id: data
                    .get("from_account_id")
                    .and_then(JsonValue::as_str)
                    .ok_or_else(|| "transfer action missing from_account_id".to_string())?,
                to_account_id: data
                    .get("to_account_id")
                    .and_then(JsonValue::as_str)
                    .ok_or_else(|| "transfer action missing to_account_id".to_string())?,
                amount: data
                    .get("amount")
                    .and_then(JsonValue::as_u64)
                    .ok_or_else(|| "transfer action missing amount".to_string())?,
                nonce: data
                    .get("nonce")
                    .and_then(JsonValue::as_u64)
                    .ok_or_else(|| "transfer action missing nonce".to_string())?,
                asset_id: data.get("asset_id").and_then(JsonValue::as_str),
                memo: data.get("memo").and_then(JsonValue::as_str),
                chain_id: data.get("chain_id").and_then(JsonValue::as_str),
                network_id: data.get("network_id").and_then(JsonValue::as_str),
                tx_version: data.get("tx_version").and_then(JsonValue::as_u64),
                tx_type: data.get("tx_type").and_then(JsonValue::as_str),
                valid_until_unix_ms: data.get("valid_until_unix_ms").and_then(JsonValue::as_i64),
                max_fee: data.get("max_fee").and_then(JsonValue::as_u64),
                fee_asset_id: data.get("fee_asset_id").and_then(JsonValue::as_str),
                application_payload_hash: data
                    .get("application_payload_hash")
                    .and_then(JsonValue::as_str),
                client_request_id: data.get("client_request_id").and_then(JsonValue::as_str),
            },
        )),
        "ClaimMainTokenVesting" => Ok(LocalMainTokenActionSigningPayload::ClaimMainTokenVesting(
            LocalClaimMainTokenVestingSigningData {
                bucket_id: data
                    .get("bucket_id")
                    .and_then(JsonValue::as_str)
                    .ok_or_else(|| "claim action missing bucket_id".to_string())?,
                beneficiary: data
                    .get("beneficiary")
                    .and_then(JsonValue::as_str)
                    .ok_or_else(|| "claim action missing beneficiary".to_string())?,
                nonce: data
                    .get("nonce")
                    .and_then(JsonValue::as_u64)
                    .ok_or_else(|| "claim action missing nonce".to_string())?,
            },
        )),
        "InitializeMainTokenGenesis" => Ok(
            LocalMainTokenActionSigningPayload::InitializeMainTokenGenesis(
                LocalInitializeMainTokenGenesisSigningData {
                    allocations: serde_json::from_value(
                        data.get("allocations")
                            .cloned()
                            .ok_or_else(|| "genesis action missing allocations".to_string())?,
                    )
                    .map_err(|err| format!("genesis action allocations decode failed: {err}"))?,
                },
            ),
        ),
        "DistributeMainTokenTreasury" => Ok(
            LocalMainTokenActionSigningPayload::DistributeMainTokenTreasury(
                LocalDistributeMainTokenTreasurySigningData {
                    proposal_id: data
                        .get("proposal_id")
                        .and_then(JsonValue::as_u64)
                        .ok_or_else(|| "treasury action missing proposal_id".to_string())?,
                    distribution_id: data
                        .get("distribution_id")
                        .and_then(JsonValue::as_str)
                        .ok_or_else(|| "treasury action missing distribution_id".to_string())?
                        .to_string(),
                    bucket_id: data
                        .get("bucket_id")
                        .and_then(JsonValue::as_str)
                        .ok_or_else(|| "treasury action missing bucket_id".to_string())?
                        .to_string(),
                    distributions: serde_json::from_value(
                        data.get("distributions")
                            .cloned()
                            .ok_or_else(|| "treasury action missing distributions".to_string())?,
                    )
                    .map_err(|err| format!("treasury action distributions decode failed: {err}"))?,
                },
            ),
        ),
        "TopUpRestrictedStarterClaimLiveopsPool" => Ok(
            LocalMainTokenActionSigningPayload::TopUpRestrictedStarterClaimLiveopsPool(
                LocalTopUpRestrictedStarterClaimLiveopsPoolSigningData {
                    controller_account_id: data
                        .get("controller_account_id")
                        .and_then(JsonValue::as_str)
                        .ok_or_else(|| {
                            "restricted claim liveops pool top-up action missing controller_account_id"
                                .to_string()
                        })?,
                    top_up_id: data
                        .get("top_up_id")
                        .and_then(JsonValue::as_str)
                        .ok_or_else(|| {
                            "restricted claim liveops pool top-up action missing top_up_id"
                                .to_string()
                        })?,
                    amount: data
                        .get("amount")
                        .and_then(JsonValue::as_u64)
                        .ok_or_else(|| {
                            "restricted claim liveops pool top-up action missing amount"
                                .to_string()
                        })?,
                },
            ),
        ),
        "UpdateRestrictedStarterClaimAdminRegistry" => Ok(
            LocalMainTokenActionSigningPayload::UpdateRestrictedStarterClaimAdminRegistry(
                LocalUpdateRestrictedStarterClaimAdminRegistrySigningData {
                    controller_account_id: data
                        .get("controller_account_id")
                        .and_then(JsonValue::as_str)
                        .ok_or_else(|| {
                            "restricted claim admin registry action missing controller_account_id"
                                .to_string()
                        })?
                        .to_string(),
                    next_admin_account_ids: serde_json::from_value(
                        data.get("next_admin_account_ids")
                            .cloned()
                            .ok_or_else(|| {
                                "restricted claim admin registry action missing next_admin_account_ids"
                                    .to_string()
                            })?,
                    )
                    .map_err(|err| {
                        format!(
                            "restricted claim admin registry next_admin_account_ids decode failed: {err}"
                        )
                    })?,
                },
            ),
        ),
        other => Err(format!(
            "main token auth is not supported for action {other}"
        )),
    }
}

fn validate_local_main_token_action_account_binding(
    action: &JsonValue,
    account_id: &str,
    public_key: Option<&str>,
    controller_binding: &NodeMainTokenControllerBindingConfig,
) -> Result<(), String> {
    let action_kind = local_runtime_action_kind(action).unwrap_or("unknown");
    let data = local_runtime_action_data(action)?;
    match action_kind {
        "TransferMainToken" => {
            let from_account_id = data
                .get("from_account_id")
                .and_then(JsonValue::as_str)
                .ok_or_else(|| "transfer action missing from_account_id".to_string())?
                .trim();
            if account_id != from_account_id {
                return Err(format!(
                    "main token auth account_id does not match transfer from_account_id: expected={from_account_id} actual={account_id}"
                ));
            }
            let public_key = public_key.ok_or_else(|| {
                "main token auth public key is required for transfer binding".to_string()
            })?;
            let expected = main_token_account_id_from_public_key(public_key);
            if account_id != expected {
                return Err(format!(
                    "main token auth account_id does not match signer public key: expected={expected} actual={account_id}"
                ));
            }
        }
        "ClaimMainTokenVesting" => {
            let beneficiary = data
                .get("beneficiary")
                .and_then(JsonValue::as_str)
                .ok_or_else(|| "claim action missing beneficiary".to_string())?
                .trim();
            if account_id != beneficiary {
                return Err(format!(
                    "main token auth account_id does not match claim beneficiary: expected={beneficiary} actual={account_id}"
                ));
            }
            if beneficiary.starts_with("oc:pk:") {
                let public_key = public_key.ok_or_else(|| {
                    "main token auth public key is required for oc:pk claim binding".to_string()
                })?;
                let expected = main_token_account_id_from_public_key(public_key);
                if account_id != expected {
                    return Err(format!(
                        "main token auth account_id does not match signer public key: expected={expected} actual={account_id}"
                    ));
                }
            }
        }
        "InitializeMainTokenGenesis" => {
            let expected = controller_binding.genesis_controller_account_id.trim();
            if account_id != expected {
                return Err(format!(
                    "main token auth account_id does not match genesis controller slot: expected={expected} actual={account_id}"
                ));
            }
        }
        "DistributeMainTokenTreasury" => {
            let bucket_id = data
                .get("bucket_id")
                .and_then(JsonValue::as_str)
                .ok_or_else(|| "treasury action missing bucket_id".to_string())?
                .trim();
            let expected = controller_binding
                .treasury_bucket_controller_slots
                .get(bucket_id)
                .map(String::as_str)
                .ok_or_else(|| {
                    format!(
                        "main token treasury controller slot is not configured for bucket {bucket_id}"
                    )
                })?;
            if account_id != expected {
                return Err(format!(
                    "main token auth account_id does not match treasury controller slot: bucket={bucket_id} expected={expected} actual={account_id}"
                ));
            }
        }
        "TopUpRestrictedStarterClaimLiveopsPool" => {
            let controller_account_id = data
                .get("controller_account_id")
                .and_then(JsonValue::as_str)
                .ok_or_else(|| {
                    "restricted claim liveops pool top-up action missing controller_account_id"
                        .to_string()
                })?
                .trim();
            if account_id != controller_account_id {
                return Err(format!(
                    "main token auth account_id does not match restricted claim liveops pool top-up controller_account_id: expected={controller_account_id} actual={account_id}"
                ));
            }
            let expected = controller_binding
                .treasury_bucket_controller_slots
                .get("ecosystem_pool")
                .map(String::as_str)
                .ok_or_else(|| {
                    "restricted claim liveops pool top-up controller slot is not configured for bucket ecosystem_pool".to_string()
                })?;
            if account_id != expected {
                return Err(format!(
                    "main token auth account_id does not match restricted claim liveops pool top-up controller slot: expected={expected} actual={account_id}"
                ));
            }
        }
        "UpdateRestrictedStarterClaimAdminRegistry" => {
            let controller_account_id = data
                .get("controller_account_id")
                .and_then(JsonValue::as_str)
                .ok_or_else(|| {
                    "restricted claim admin registry action missing controller_account_id"
                        .to_string()
                })?
                .trim();
            if account_id != controller_account_id {
                return Err(format!(
                    "main token auth account_id does not match restricted claim admin registry controller_account_id: expected={controller_account_id} actual={account_id}"
                ));
            }
            let expected = controller_binding
                .treasury_bucket_controller_slots
                .get("ecosystem_pool")
                .map(String::as_str)
                .ok_or_else(|| {
                    "restricted claim admin registry controller slot is not configured for bucket ecosystem_pool".to_string()
                })?;
            if account_id != expected {
                return Err(format!(
                    "main token auth account_id does not match restricted claim admin registry controller slot: expected={expected} actual={account_id}"
                ));
            }
        }
        other => {
            return Err(format!(
                "main token auth is not supported for action {other}"
            ));
        }
    }
    Ok(())
}

fn enforce_local_controller_signer_policy(
    action: &JsonValue,
    account_id: &str,
    controller_binding: &NodeMainTokenControllerBindingConfig,
    proof_threshold: u16,
    signer_public_keys: &[&str],
) -> Result<(), String> {
    let Some(policy) =
        local_controller_signer_policy_for_action(action, account_id, controller_binding)?
    else {
        return Ok(());
    };
    if policy.allowed_public_keys.is_empty() {
        return Err(format!(
            "main token controller signer allowlist is empty: controller_account_id={account_id}"
        ));
    }
    if policy.threshold != proof_threshold {
        return Err(format!(
            "main token controller signer threshold mismatch: controller_account_id={account_id} expected={} actual={proof_threshold}",
            policy.threshold
        ));
    }
    let mut unique = std::collections::BTreeSet::new();
    for public_key in signer_public_keys {
        if !unique.insert((*public_key).to_string()) {
            continue;
        }
        if !policy.allowed_public_keys.contains(*public_key) {
            return Err(format!(
                "main token controller signer is not allowlisted: controller_account_id={account_id} public_key={public_key}"
            ));
        }
    }
    if unique.len() < usize::from(policy.threshold) {
        return Err(format!(
            "main token controller signer threshold not met: controller_account_id={account_id} unique_signers={} threshold={}",
            unique.len(),
            policy.threshold
        ));
    }
    Ok(())
}

fn local_controller_signer_policy_for_action<'a>(
    action: &JsonValue,
    account_id: &str,
    controller_binding: &'a NodeMainTokenControllerBindingConfig,
) -> Result<Option<&'a NodeMainTokenControllerSignerPolicy>, String> {
    match local_runtime_action_kind(action) {
        Some("InitializeMainTokenGenesis")
        | Some("DistributeMainTokenTreasury")
        | Some("TopUpRestrictedStarterClaimLiveopsPool")
        | Some("UpdateRestrictedStarterClaimAdminRegistry") => {
            controller_binding
                .controller_signer_policies
                .get(account_id)
                .ok_or_else(|| {
                    format!(
                        "main token controller signer policy is not configured: controller_account_id={account_id}"
                    )
                })
                .map(Some)
        }
        Some("TransferMainToken") | Some("ClaimMainTokenVesting") => Ok(None),
        Some(other) => Err(format!("main token auth is not supported for action {other}")),
        None => Err("runtime action missing type".to_string()),
    }
}

fn verify_local_main_token_action_signature(
    action: &JsonValue,
    public_key_hex: &str,
    signature: &str,
    signing_payload: &[u8],
) -> Result<(), String> {
    let public_key_bytes = decode_hex_array::<32>(public_key_hex, "main token auth public key")?;
    let prefix = local_main_token_action_signature_prefix(action)?;
    let signature_hex = signature
        .strip_prefix(prefix)
        .ok_or_else(|| format!("main token auth signature is not {prefix}"))?;
    let signature_bytes = decode_hex_array::<64>(signature_hex, "main token auth signature")?;
    let verifying_key = VerifyingKey::from_bytes(&public_key_bytes)
        .map_err(|err| format!("parse main token auth public key failed: {err}"))?;
    verifying_key
        .verify(signing_payload, &Signature::from_bytes(&signature_bytes))
        .map_err(|err| format!("verify main token auth signature failed: {err}"))
}

fn local_main_token_action_operation(action: &JsonValue) -> Result<&'static str, String> {
    match local_runtime_action_kind(action) {
        Some("TransferMainToken") => Ok("transfer_main_token"),
        Some("ClaimMainTokenVesting") => Ok("claim_main_token_vesting"),
        Some("InitializeMainTokenGenesis") => Ok("initialize_main_token_genesis"),
        Some("DistributeMainTokenTreasury") => Ok("distribute_main_token_treasury"),
        Some("TopUpRestrictedStarterClaimLiveopsPool") => {
            Ok("top_up_restricted_starter_claim_liveops_pool")
        }
        Some("UpdateRestrictedStarterClaimAdminRegistry") => {
            Ok("update_restricted_starter_claim_admin_registry")
        }
        Some(other) => Err(format!(
            "main token auth is not supported for action {other}"
        )),
        None => Err("runtime action missing type".to_string()),
    }
}

fn local_main_token_action_signature_prefix(action: &JsonValue) -> Result<&'static str, String> {
    match local_runtime_action_kind(action) {
        Some("TransferMainToken") => {
            let data = local_runtime_action_data(action)?;
            let uses_v2_context = data.get("chain_id").is_some()
                || data.get("network_id").is_some()
                || data.get("tx_version").is_some()
                || data.get("tx_type").is_some()
                || data.get("valid_until_unix_ms").is_some()
                || data
                    .get("asset_id")
                    .and_then(JsonValue::as_str)
                    .is_some_and(|value| value != "main_token")
                || data.get("memo").is_some();
            if uses_v2_context {
                Ok(MAIN_TOKEN_TRANSFER_AUTH_SIGNATURE_V2_PREFIX)
            } else {
                Ok(MAIN_TOKEN_TRANSFER_AUTH_SIGNATURE_V1_PREFIX)
            }
        }
        Some("ClaimMainTokenVesting") => Ok(MAIN_TOKEN_CLAIM_AUTH_SIGNATURE_V1_PREFIX),
        Some("InitializeMainTokenGenesis") => Ok(MAIN_TOKEN_GENESIS_AUTH_SIGNATURE_V1_PREFIX),
        Some("DistributeMainTokenTreasury") => Ok(MAIN_TOKEN_TREASURY_AUTH_SIGNATURE_V1_PREFIX),
        Some("TopUpRestrictedStarterClaimLiveopsPool") => {
            Ok(MAIN_TOKEN_RESTRICTED_CLAIM_LIVEOPS_POOL_TOP_UP_AUTH_SIGNATURE_V1_PREFIX)
        }
        Some("UpdateRestrictedStarterClaimAdminRegistry") => {
            Ok(MAIN_TOKEN_RESTRICTED_GRANT_ADMIN_REGISTRY_AUTH_SIGNATURE_V1_PREFIX)
        }
        Some(other) => Err(format!(
            "main token auth is not supported for action {other}"
        )),
        None => Err("runtime action missing type".to_string()),
    }
}

fn normalize_required_field(raw: &str, label: &str) -> Result<String, String> {
    let value = raw.trim();
    if value.is_empty() {
        return Err(format!("{label} is empty"));
    }
    Ok(value.to_string())
}

fn normalize_public_key_field(raw: &str, label: &str) -> Result<String, String> {
    let normalized = normalize_required_field(raw, label)?;
    let bytes = decode_hex_array::<32>(normalized.as_str(), label)?;
    Ok(hex::encode(bytes))
}

fn decode_hex_array<const N: usize>(raw: &str, label: &str) -> Result<[u8; N], String> {
    let bytes = hex::decode(raw).map_err(|err| format!("decode {label} failed: {err}"))?;
    if bytes.len() != N {
        return Err(format!(
            "{label} length mismatch: expected {N} bytes, got {}",
            bytes.len()
        ));
    }
    let mut fixed = [0_u8; N];
    fixed.copy_from_slice(bytes.as_slice());
    Ok(fixed)
}

fn main_token_account_id_from_public_key(public_key_hex: &str) -> String {
    format!("oc:pk:{}", public_key_hex.trim().to_ascii_lowercase())
}

fn node_feedback_error(err: ProtoWorldError) -> NodeError {
    NodeError::Replication {
        reason: format!("feedback operation failed: {err:?}"),
    }
}

#[cfg(test)]
#[path = "node_runtime_governance_auth_tests.rs"]
mod governance_auth_tests;

#[path = "node_runtime_governance_auth.rs"]
mod governance_auth;

pub(crate) use governance_auth::preflight_with_execution_hook;
pub(crate) use governance_auth::retain_for_proposal as retain_governance_action_for_proposal;
pub(crate) use governance_auth::validate_consensus_action_payload_auth;
