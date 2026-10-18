use crate::runtime;
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use oasis7_client_api::MainTokenTransfer;
use oasis7_simulator::{Action as SimulatorAction, ActionSubmitter};
use serde::{Deserialize, Serialize};

pub use oasis7_client_api::{
    MainTokenActionAuthProof, MainTokenActionAuthScheme, MainTokenActionParticipantSignature,
    VerifiedMainTokenActionAuth,
};

const CONSENSUS_ACTION_PAYLOAD_ENVELOPE_VERSION: u8 = 1;
const CONSENSUS_RECIPE_SUBMISSION_ENVELOPE_VERSION: u8 = 2;
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
pub struct ConsensusActionPayloadEnvelope {
    pub version: u8,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gameplay_submission_origin: Option<runtime::GameplaySubmissionOrigin>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth: Option<ConsensusActionAuthEnvelope>,
    pub body: ConsensusActionPayloadBody,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum ConsensusActionAuthEnvelope {
    MainTokenAction(MainTokenActionAuthProof),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MainTokenActionAuthError {
    InvalidRequest(String),
    InvalidSignature(String),
    AccountMismatch(String),
    UnsupportedAction(String),
}

impl std::fmt::Display for MainTokenActionAuthError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidRequest(message)
            | Self::InvalidSignature(message)
            | Self::AccountMismatch(message)
            | Self::UnsupportedAction(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for MainTokenActionAuthError {}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[expect(
    clippy::large_enum_variant,
    reason = "RuntimeAction has a stable serialized shape; boxing would change the public payload API."
)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum ConsensusActionPayloadBody {
    WorldServiceIntent {
        request:
            crate::world_service::SubmitIntentRequest<crate::world_service::WorldServicePayloadV1>,
    },
    RuntimeAction {
        action: runtime::Action,
    },
    SimulatorAction {
        action: SimulatorAction,
        #[serde(default)]
        submitter: ActionSubmitter,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize)]
struct MainTokenActionSigningEnvelope<'a> {
    version: u8,
    operation: &'static str,
    account_id: &'a str,
    public_key: &'a str,
    action: MainTokenActionSigningPayload<'a>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", content = "data")]
enum MainTokenActionSigningPayload<'a> {
    ClaimMainTokenVesting(ClaimMainTokenVestingSigningData<'a>),
    InitializeMainTokenGenesis(InitializeMainTokenGenesisSigningData<'a>),
    DistributeMainTokenTreasury(DistributeMainTokenTreasurySigningData<'a>),
    TopUpRestrictedStarterClaimLiveopsPool(TopUpRestrictedStarterClaimLiveopsPoolSigningData<'a>),
    UpdateRestrictedStarterClaimAdminRegistry(
        UpdateRestrictedStarterClaimAdminRegistrySigningData<'a>,
    ),
}

#[derive(Debug, Clone, PartialEq, Serialize)]
struct ClaimMainTokenVestingSigningData<'a> {
    bucket_id: &'a str,
    beneficiary: &'a str,
    nonce: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
struct InitializeMainTokenGenesisSigningData<'a> {
    allocations: &'a [runtime::MainTokenGenesisAllocationPlan],
}

#[derive(Debug, Clone, PartialEq, Serialize)]
struct DistributeMainTokenTreasurySigningData<'a> {
    proposal_id: runtime::ProposalId,
    distribution_id: &'a str,
    bucket_id: &'a str,
    distributions: &'a [runtime::MainTokenTreasuryDistribution],
}

#[derive(Debug, Clone, PartialEq, Serialize)]
struct TopUpRestrictedStarterClaimLiveopsPoolSigningData<'a> {
    controller_account_id: &'a str,
    top_up_id: &'a str,
    amount: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
struct UpdateRestrictedStarterClaimAdminRegistrySigningData<'a> {
    controller_account_id: &'a str,
    next_admin_account_ids: &'a [String],
}

impl ConsensusActionPayloadEnvelope {
    pub fn from_runtime_action(action: runtime::Action) -> Self {
        Self {
            version: CONSENSUS_ACTION_PAYLOAD_ENVELOPE_VERSION,
            gameplay_submission_origin: None,
            auth: None,
            body: ConsensusActionPayloadBody::RuntimeAction { action },
        }
    }

    /// Version two fails closed on legacy executors and binds the ingress identity.
    pub fn from_recipe_submission(
        action: runtime::Action,
        origin: runtime::GameplaySubmissionOrigin,
    ) -> Self {
        Self {
            version: CONSENSUS_RECIPE_SUBMISSION_ENVELOPE_VERSION,
            gameplay_submission_origin: Some(origin),
            auth: None,
            body: ConsensusActionPayloadBody::RuntimeAction { action },
        }
    }

    pub fn from_runtime_action_with_auth(
        action: runtime::Action,
        auth: ConsensusActionAuthEnvelope,
    ) -> Self {
        Self {
            version: CONSENSUS_ACTION_PAYLOAD_ENVELOPE_VERSION,
            gameplay_submission_origin: None,
            auth: Some(auth),
            body: ConsensusActionPayloadBody::RuntimeAction { action },
        }
    }

    pub fn from_simulator_action(action: SimulatorAction, submitter: ActionSubmitter) -> Self {
        Self {
            version: CONSENSUS_ACTION_PAYLOAD_ENVELOPE_VERSION,
            gameplay_submission_origin: None,
            auth: None,
            body: ConsensusActionPayloadBody::SimulatorAction { action, submitter },
        }
    }
}

pub use oasis7_agent_api::world_action::main_token_action_auth_required;

fn main_token_transfer_from_runtime_action(action: &runtime::Action) -> Option<MainTokenTransfer> {
    let runtime::Action::TransferMainToken {
        from_account_id,
        to_account_id,
        amount,
        nonce,
        asset_id,
        memo,
        chain_id,
        network_id,
        tx_version,
        tx_type,
        valid_until_unix_ms,
        max_fee,
        fee_asset_id,
        application_payload_hash,
        client_request_id,
    } = action
    else {
        return None;
    };
    Some(MainTokenTransfer {
        from_account_id: from_account_id.clone(),
        to_account_id: to_account_id.clone(),
        amount: *amount,
        nonce: *nonce,
        asset_id: asset_id.clone(),
        memo: memo.clone(),
        chain_id: chain_id.clone(),
        network_id: network_id.clone(),
        tx_version: *tx_version,
        tx_type: tx_type.clone(),
        valid_until_unix_ms: *valid_until_unix_ms,
        max_fee: *max_fee,
        fee_asset_id: fee_asset_id.clone(),
        application_payload_hash: application_payload_hash.clone(),
        client_request_id: client_request_id.clone(),
    })
}

fn map_client_signing_error(
    error: oasis7_client_api::ClientSigningError,
) -> MainTokenActionAuthError {
    match error {
        oasis7_client_api::ClientSigningError::InvalidRequest(message) => {
            MainTokenActionAuthError::InvalidRequest(message)
        }
        oasis7_client_api::ClientSigningError::InvalidSignature(message) => {
            MainTokenActionAuthError::InvalidSignature(message)
        }
        oasis7_client_api::ClientSigningError::AccountMismatch(message) => {
            MainTokenActionAuthError::AccountMismatch(message)
        }
    }
}

fn is_legacy_noncanonical_transfer_signing_account(
    action: &runtime::Action,
    account_id: &str,
    signer_public_key_hex: &str,
) -> bool {
    let runtime::Action::TransferMainToken {
        from_account_id, ..
    } = action
    else {
        return false;
    };
    let account_id = account_id.trim();
    const CANONICAL_ACCOUNT_PREFIX: &str = "oc:pk:";
    let canonical_prefix = CANONICAL_ACCOUNT_PREFIX;
    if account_id
        .get(..canonical_prefix.len())
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case(canonical_prefix))
    {
        return false;
    }
    account_id == from_account_id.trim()
        && account_id != runtime::main_token_account_id_from_node_public_key(signer_public_key_hex)
}

pub fn sign_main_token_runtime_action_auth(
    action: &runtime::Action,
    account_id: &str,
    signer_public_key_hex: &str,
    signer_private_key_hex: &str,
) -> Result<MainTokenActionAuthProof, MainTokenActionAuthError> {
    ensure_main_token_action_supported(action)?;
    if let Some(transfer) = main_token_transfer_from_runtime_action(action)
        && !is_legacy_noncanonical_transfer_signing_account(
            action,
            account_id,
            signer_public_key_hex,
        )
    {
        return oasis7_client_api::sign_main_token_transfer(
            &transfer,
            account_id,
            signer_public_key_hex,
            signer_private_key_hex,
        )
        .map_err(map_client_signing_error);
    }
    // Keep the old signing-only compatibility path for callers that use
    // player/agent account IDs instead of canonical oc:pk accounts. The
    // verifier still enforces canonical account binding.
    let account_id = normalize_required_field(account_id, "main token auth account_id")?;
    let public_key =
        normalize_public_key_field(signer_public_key_hex, "main token auth signer public key")?;
    let signing_key =
        signing_key_from_hex(signer_private_key_hex, "main token auth signer private key")?;
    verify_keypair_match(
        &signing_key,
        public_key.as_str(),
        "main token auth signer public key",
    )?;
    let payload =
        build_main_token_action_signing_payload(action, account_id.as_str(), public_key.as_str())?;
    let signature: Signature = signing_key.sign(payload.as_slice());
    Ok(MainTokenActionAuthProof {
        scheme: MainTokenActionAuthScheme::Ed25519,
        account_id,
        public_key: Some(public_key),
        signature: Some(format!(
            "{}{}",
            main_token_action_signature_prefix(action)?,
            hex::encode(signature.to_bytes())
        )),
        threshold: None,
        participant_signatures: Vec::new(),
    })
}

pub fn sign_threshold_main_token_runtime_action_auth(
    action: &runtime::Action,
    account_id: &str,
    threshold: u16,
    signer_keypairs: &[(&str, &str)],
) -> Result<MainTokenActionAuthProof, MainTokenActionAuthError> {
    ensure_main_token_action_supported(action)?;
    if threshold == 0 {
        return Err(MainTokenActionAuthError::InvalidRequest(
            "main token auth threshold must be > 0".to_string(),
        ));
    }
    let account_id = normalize_required_field(account_id, "main token auth account_id")?;
    if signer_keypairs.len() < threshold as usize {
        return Err(MainTokenActionAuthError::InvalidRequest(format!(
            "main token auth participant count below threshold: participants={} threshold={threshold}",
            signer_keypairs.len()
        )));
    }
    let mut participant_signatures = Vec::with_capacity(signer_keypairs.len());
    for (public_key_hex, private_key_hex) in signer_keypairs {
        let public_key =
            normalize_public_key_field(public_key_hex, "main token auth signer public key")?;
        let signing_key =
            signing_key_from_hex(private_key_hex, "main token auth signer private key")?;
        verify_keypair_match(
            &signing_key,
            public_key.as_str(),
            "main token auth signer public key",
        )?;
        let payload = build_main_token_action_signing_payload(
            action,
            account_id.as_str(),
            public_key.as_str(),
        )?;
        let signature: Signature = signing_key.sign(payload.as_slice());
        participant_signatures.push(MainTokenActionParticipantSignature {
            public_key,
            signature: format!(
                "{}{}",
                main_token_action_signature_prefix(action)?,
                hex::encode(signature.to_bytes())
            ),
        });
    }
    Ok(MainTokenActionAuthProof {
        scheme: MainTokenActionAuthScheme::ThresholdEd25519,
        account_id,
        public_key: None,
        signature: None,
        threshold: Some(threshold),
        participant_signatures,
    })
}

pub fn verify_main_token_runtime_action_auth(
    action: &runtime::Action,
    proof: &MainTokenActionAuthProof,
) -> Result<VerifiedMainTokenActionAuth, MainTokenActionAuthError> {
    ensure_main_token_action_supported(action)?;
    if let Some(transfer) = main_token_transfer_from_runtime_action(action) {
        // A1 owns the single-signer transfer proof. Threshold proofs remain
        // runtime-local because the client API deliberately does not model
        // participant signatures or threshold policy.
        if proof.scheme == MainTokenActionAuthScheme::Ed25519 {
            return oasis7_client_api::verify_main_token_transfer_signature(&transfer, proof)
                .map_err(map_client_signing_error);
        }
    }
    let account_id =
        normalize_required_field(proof.account_id.as_str(), "main token auth account_id")?;
    match proof.scheme {
        MainTokenActionAuthScheme::Ed25519 => {
            let public_key = normalize_public_key_field(
                proof.public_key.as_deref().ok_or_else(|| {
                    MainTokenActionAuthError::InvalidRequest(
                        "main token auth public_key is required for ed25519".to_string(),
                    )
                })?,
                "main token auth public key",
            )?;
            let payload = build_main_token_action_signing_payload(
                action,
                account_id.as_str(),
                public_key.as_str(),
            )?;
            validate_main_token_action_account_binding(
                action,
                account_id.as_str(),
                Some(public_key.as_str()),
            )?;
            verify_main_token_action_signature(
                action,
                public_key.as_str(),
                proof.signature.as_deref().ok_or_else(|| {
                    MainTokenActionAuthError::InvalidRequest(
                        "main token auth signature is required for ed25519".to_string(),
                    )
                })?,
                payload.as_slice(),
            )?;
            Ok(VerifiedMainTokenActionAuth {
                account_id,
                signer_public_keys: vec![public_key],
            })
        }
        MainTokenActionAuthScheme::ThresholdEd25519 => {
            let threshold = proof.threshold.ok_or_else(|| {
                MainTokenActionAuthError::InvalidRequest(
                    "main token auth threshold is required for threshold_ed25519".to_string(),
                )
            })?;
            if threshold == 0 {
                return Err(MainTokenActionAuthError::InvalidRequest(
                    "main token auth threshold must be > 0".to_string(),
                ));
            }
            validate_main_token_action_account_binding(action, account_id.as_str(), None)?;
            if proof.participant_signatures.len() < threshold as usize {
                return Err(MainTokenActionAuthError::InvalidRequest(format!(
                    "main token auth participant count below threshold: participants={} threshold={threshold}",
                    proof.participant_signatures.len()
                )));
            }
            let mut signer_public_keys = Vec::with_capacity(proof.participant_signatures.len());
            let mut seen = std::collections::BTreeSet::new();
            for participant in &proof.participant_signatures {
                let public_key = normalize_public_key_field(
                    participant.public_key.as_str(),
                    "main token auth participant public key",
                )?;
                if !seen.insert(public_key.clone()) {
                    return Err(MainTokenActionAuthError::InvalidRequest(format!(
                        "duplicate main token auth participant public key: {public_key}"
                    )));
                }
                let payload = build_main_token_action_signing_payload(
                    action,
                    account_id.as_str(),
                    public_key.as_str(),
                )?;
                verify_main_token_action_signature(
                    action,
                    public_key.as_str(),
                    participant.signature.as_str(),
                    payload.as_slice(),
                )?;
                signer_public_keys.push(public_key);
            }
            Ok(VerifiedMainTokenActionAuth {
                account_id,
                signer_public_keys,
            })
        }
    }
}

pub fn encode_consensus_action_payload(
    envelope: &ConsensusActionPayloadEnvelope,
) -> Result<Vec<u8>, String> {
    serde_cbor::to_vec(envelope)
        .map_err(|err| format!("encode consensus action payload envelope failed: {err}"))
}

pub fn decode_consensus_action_payload_envelope(
    payload_cbor: &[u8],
) -> Result<ConsensusActionPayloadEnvelope, String> {
    // A recognized metadata key must contain a typed identity; explicit null
    // cannot silently downgrade a new submitted envelope to legacy evidence.
    if serde_cbor::from_slice::<serde_cbor::Value>(payload_cbor)
        .ok()
        .is_some_and(|value| match value {
            serde_cbor::Value::Map(map) => {
                map.get(&serde_cbor::Value::Text(
                    "gameplay_submission_origin".to_string(),
                )) == Some(&serde_cbor::Value::Null)
            }
            _ => false,
        })
    {
        return Err("gameplay submission origin must not be null".to_string());
    }
    match serde_cbor::from_slice::<ConsensusActionPayloadEnvelope>(payload_cbor) {
        Ok(envelope) => {
            match (
                envelope.version,
                envelope.gameplay_submission_origin.as_ref(),
            ) {
                (CONSENSUS_ACTION_PAYLOAD_ENVELOPE_VERSION, None)
                | (CONSENSUS_RECIPE_SUBMISSION_ENVELOPE_VERSION, Some(_)) => {}
                _ => {
                    return Err(format!(
                        "unsupported consensus payload envelope version/origin combination {}",
                        envelope.version
                    ));
                }
            }
            if let Some(origin) = &envelope.gameplay_submission_origin {
                match &envelope.body {
                    ConsensusActionPayloadBody::RuntimeAction {
                        action:
                            runtime::Action::ScheduleRecipe {
                                requester_agent_id,
                                factory_id,
                                recipe_id,
                                ..
                            },
                    } if origin.matches_recipe(requester_agent_id, factory_id, recipe_id) => {}
                    _ => {
                        return Err(
                            "gameplay submission origin does not match ScheduleRecipe".to_string()
                        );
                    }
                }
            }
            Ok(envelope)
        }
        Err(envelope_err) => {
            let recognized_origin = serde_cbor::from_slice::<serde_cbor::Value>(payload_cbor)
                .ok()
                .is_some_and(|value| match value {
                    serde_cbor::Value::Map(map) => {
                        map.contains_key(&serde_cbor::Value::Text(
                            "gameplay_submission_origin".to_string(),
                        )) || map.get(&serde_cbor::Value::Text("version".to_string()))
                            == Some(&serde_cbor::Value::Integer(2))
                    }
                    _ => false,
                });
            if recognized_origin {
                return Err(format!(
                    "invalid gameplay submission origin envelope: {envelope_err}"
                ));
            }
            match serde_cbor::from_slice::<runtime::Action>(payload_cbor) {
                Ok(action) => Ok(ConsensusActionPayloadEnvelope::from_runtime_action(action)),
                Err(runtime_err) => Err(format!(
                    "decode consensus payload envelope failed ({envelope_err}); runtime fallback failed ({runtime_err})"
                )),
            }
        }
    }
}

pub fn decode_consensus_action_payload(
    payload_cbor: &[u8],
) -> Result<ConsensusActionPayloadBody, String> {
    decode_consensus_action_payload_envelope(payload_cbor).map(|envelope| envelope.body)
}

fn build_main_token_action_signing_payload(
    action: &runtime::Action,
    account_id: &str,
    public_key: &str,
) -> Result<Vec<u8>, MainTokenActionAuthError> {
    if let Some(transfer) = main_token_transfer_from_runtime_action(action) {
        return oasis7_client_api::build_main_token_transfer_signing_payload(
            &transfer, account_id, public_key,
        )
        .map_err(map_client_signing_error);
    }
    let envelope = MainTokenActionSigningEnvelope {
        version: MAIN_TOKEN_ACTION_AUTH_PAYLOAD_VERSION,
        operation: main_token_action_operation(action)?,
        account_id,
        public_key,
        action: build_main_token_action_signing_action(action)?,
    };
    serde_json::to_vec(&envelope).map_err(|err| {
        MainTokenActionAuthError::InvalidRequest(format!(
            "encode main token auth signing payload failed: {err}"
        ))
    })
}

fn build_main_token_action_signing_action(
    action: &runtime::Action,
) -> Result<MainTokenActionSigningPayload<'_>, MainTokenActionAuthError> {
    match action {
        runtime::Action::ClaimMainTokenVesting {
            bucket_id,
            beneficiary,
            nonce,
        } => Ok(MainTokenActionSigningPayload::ClaimMainTokenVesting(
            ClaimMainTokenVestingSigningData {
                bucket_id: bucket_id.as_str(),
                beneficiary: beneficiary.as_str(),
                nonce: *nonce,
            },
        )),
        runtime::Action::InitializeMainTokenGenesis { allocations } => {
            Ok(MainTokenActionSigningPayload::InitializeMainTokenGenesis(
                InitializeMainTokenGenesisSigningData {
                    allocations: allocations.as_slice(),
                },
            ))
        }
        runtime::Action::DistributeMainTokenTreasury {
            proposal_id,
            distribution_id,
            bucket_id,
            distributions,
        } => Ok(MainTokenActionSigningPayload::DistributeMainTokenTreasury(
            DistributeMainTokenTreasurySigningData {
                proposal_id: *proposal_id,
                distribution_id: distribution_id.as_str(),
                bucket_id: bucket_id.as_str(),
                distributions: distributions.as_slice(),
            },
        )),
        runtime::Action::TopUpRestrictedStarterClaimLiveopsPool {
            controller_account_id,
            top_up_id,
            amount,
        } => Ok(
            MainTokenActionSigningPayload::TopUpRestrictedStarterClaimLiveopsPool(
                TopUpRestrictedStarterClaimLiveopsPoolSigningData {
                    controller_account_id: controller_account_id.as_str(),
                    top_up_id: top_up_id.as_str(),
                    amount: *amount,
                },
            ),
        ),
        runtime::Action::UpdateRestrictedStarterClaimAdminRegistry {
            controller_account_id,
            next_admin_account_ids,
        } => Ok(
            MainTokenActionSigningPayload::UpdateRestrictedStarterClaimAdminRegistry(
                UpdateRestrictedStarterClaimAdminRegistrySigningData {
                    controller_account_id: controller_account_id.as_str(),
                    next_admin_account_ids: next_admin_account_ids.as_slice(),
                },
            ),
        ),
        _ => Err(MainTokenActionAuthError::UnsupportedAction(format!(
            "main token auth is not supported for action {action:?}"
        ))),
    }
}

fn validate_main_token_action_account_binding(
    action: &runtime::Action,
    account_id: &str,
    public_key: Option<&str>,
) -> Result<(), MainTokenActionAuthError> {
    match action {
        runtime::Action::TransferMainToken {
            from_account_id, ..
        } => {
            if account_id != from_account_id.trim() {
                return Err(MainTokenActionAuthError::AccountMismatch(format!(
                    "main token auth account_id does not match transfer from_account_id: expected={} actual={account_id}",
                    from_account_id.trim()
                )));
            }
            let public_key = public_key.ok_or_else(|| {
                MainTokenActionAuthError::InvalidRequest(
                    "main token auth public key is required for transfer binding".to_string(),
                )
            })?;
            let expected = runtime::main_token_account_id_from_node_public_key(public_key);
            if account_id != expected {
                return Err(MainTokenActionAuthError::AccountMismatch(format!(
                    "main token auth account_id does not match signer public key: expected={expected} actual={account_id}"
                )));
            }
        }
        runtime::Action::ClaimMainTokenVesting { beneficiary, .. } => {
            if account_id != beneficiary.trim() {
                return Err(MainTokenActionAuthError::AccountMismatch(format!(
                    "main token auth account_id does not match claim beneficiary: expected={} actual={account_id}",
                    beneficiary.trim()
                )));
            }
            if beneficiary.trim().starts_with("oc:pk:") {
                let public_key = public_key.ok_or_else(|| {
                    MainTokenActionAuthError::InvalidRequest(
                        "main token auth public key is required for oc:pk claim binding"
                            .to_string(),
                    )
                })?;
                let expected = runtime::main_token_account_id_from_node_public_key(public_key);
                if account_id != expected {
                    return Err(MainTokenActionAuthError::AccountMismatch(format!(
                        "main token auth account_id does not match signer public key: expected={expected} actual={account_id}"
                    )));
                }
            }
        }
        runtime::Action::InitializeMainTokenGenesis { .. }
        | runtime::Action::DistributeMainTokenTreasury { .. } => {}
        runtime::Action::TopUpRestrictedStarterClaimLiveopsPool {
            controller_account_id,
            ..
        } => {
            if account_id != controller_account_id.trim() {
                return Err(MainTokenActionAuthError::AccountMismatch(format!(
                    "main token auth account_id does not match restricted claim liveops pool top-up controller_account_id: expected={} actual={account_id}",
                    controller_account_id.trim()
                )));
            }
        }
        runtime::Action::UpdateRestrictedStarterClaimAdminRegistry {
            controller_account_id,
            ..
        } => {
            if account_id != controller_account_id.trim() {
                return Err(MainTokenActionAuthError::AccountMismatch(format!(
                    "main token auth account_id does not match restricted grant admin registry controller_account_id: expected={} actual={account_id}",
                    controller_account_id.trim()
                )));
            }
        }
        _ => {
            return Err(MainTokenActionAuthError::UnsupportedAction(format!(
                "main token auth is not supported for action {action:?}"
            )));
        }
    }
    Ok(())
}

fn verify_main_token_action_signature(
    action: &runtime::Action,
    public_key_hex: &str,
    signature: &str,
    signing_payload: &[u8],
) -> Result<(), MainTokenActionAuthError> {
    let public_key_bytes = decode_hex_array::<32>(public_key_hex, "main token auth public key")?;
    let prefix = main_token_action_signature_prefix(action)?;
    let signature_hex = signature.strip_prefix(prefix).ok_or_else(|| {
        MainTokenActionAuthError::InvalidSignature(format!(
            "main token auth signature is not {prefix}"
        ))
    })?;
    let signature_bytes = decode_hex_array::<64>(signature_hex, "main token auth signature")?;
    let verifying_key = VerifyingKey::from_bytes(&public_key_bytes).map_err(|err| {
        MainTokenActionAuthError::InvalidRequest(format!(
            "parse main token auth public key failed: {err}"
        ))
    })?;
    verifying_key
        .verify(signing_payload, &Signature::from_bytes(&signature_bytes))
        .map_err(|err| {
            MainTokenActionAuthError::InvalidSignature(format!(
                "verify main token auth signature failed: {err}"
            ))
        })
}

fn main_token_action_operation(
    action: &runtime::Action,
) -> Result<&'static str, MainTokenActionAuthError> {
    match action {
        runtime::Action::TransferMainToken { .. } => Ok("transfer_main_token"),
        runtime::Action::ClaimMainTokenVesting { .. } => Ok("claim_main_token_vesting"),
        runtime::Action::InitializeMainTokenGenesis { .. } => Ok("initialize_main_token_genesis"),
        runtime::Action::DistributeMainTokenTreasury { .. } => Ok("distribute_main_token_treasury"),
        runtime::Action::TopUpRestrictedStarterClaimLiveopsPool { .. } => {
            Ok("top_up_restricted_starter_claim_liveops_pool")
        }
        runtime::Action::UpdateRestrictedStarterClaimAdminRegistry { .. } => {
            Ok("update_restricted_starter_claim_admin_registry")
        }
        _ => Err(MainTokenActionAuthError::UnsupportedAction(format!(
            "main token auth is not supported for action {action:?}"
        ))),
    }
}

fn main_token_action_signature_prefix(
    action: &runtime::Action,
) -> Result<&'static str, MainTokenActionAuthError> {
    match action {
        runtime::Action::TransferMainToken {
            asset_id,
            memo,
            chain_id,
            network_id,
            tx_version,
            tx_type,
            valid_until_unix_ms,
            ..
        } => {
            let uses_v2_context = chain_id.is_some()
                || network_id.is_some()
                || tx_version.is_some()
                || tx_type.is_some()
                || valid_until_unix_ms.is_some()
                || asset_id
                    .as_deref()
                    .is_some_and(|value| value != "main_token")
                || memo.is_some();
            if uses_v2_context {
                Ok(MAIN_TOKEN_TRANSFER_AUTH_SIGNATURE_V2_PREFIX)
            } else {
                Ok(MAIN_TOKEN_TRANSFER_AUTH_SIGNATURE_V1_PREFIX)
            }
        }
        runtime::Action::ClaimMainTokenVesting { .. } => {
            Ok(MAIN_TOKEN_CLAIM_AUTH_SIGNATURE_V1_PREFIX)
        }
        runtime::Action::InitializeMainTokenGenesis { .. } => {
            Ok(MAIN_TOKEN_GENESIS_AUTH_SIGNATURE_V1_PREFIX)
        }
        runtime::Action::DistributeMainTokenTreasury { .. } => {
            Ok(MAIN_TOKEN_TREASURY_AUTH_SIGNATURE_V1_PREFIX)
        }
        runtime::Action::TopUpRestrictedStarterClaimLiveopsPool { .. } => {
            Ok(MAIN_TOKEN_RESTRICTED_CLAIM_LIVEOPS_POOL_TOP_UP_AUTH_SIGNATURE_V1_PREFIX)
        }
        runtime::Action::UpdateRestrictedStarterClaimAdminRegistry { .. } => {
            Ok(MAIN_TOKEN_RESTRICTED_GRANT_ADMIN_REGISTRY_AUTH_SIGNATURE_V1_PREFIX)
        }
        _ => Err(MainTokenActionAuthError::UnsupportedAction(format!(
            "main token auth is not supported for action {action:?}"
        ))),
    }
}

fn ensure_main_token_action_supported(
    action: &runtime::Action,
) -> Result<(), MainTokenActionAuthError> {
    main_token_action_operation(action).map(|_| ())
}

fn normalize_required_field(raw: &str, label: &str) -> Result<String, MainTokenActionAuthError> {
    let value = raw.trim();
    if value.is_empty() {
        return Err(MainTokenActionAuthError::InvalidRequest(format!(
            "{label} is empty"
        )));
    }
    Ok(value.to_string())
}

fn normalize_public_key_field(raw: &str, label: &str) -> Result<String, MainTokenActionAuthError> {
    let normalized = normalize_required_field(raw, label)?;
    let bytes = decode_hex_array::<32>(normalized.as_str(), label)?;
    Ok(hex::encode(bytes))
}

fn signing_key_from_hex(
    private_key_hex: &str,
    label: &str,
) -> Result<SigningKey, MainTokenActionAuthError> {
    let private_key_bytes = decode_hex_array::<32>(private_key_hex, label)?;
    Ok(SigningKey::from_bytes(&private_key_bytes))
}

fn verify_keypair_match(
    signing_key: &SigningKey,
    public_key_hex: &str,
    label: &str,
) -> Result<(), MainTokenActionAuthError> {
    let expected_public_key = hex::encode(signing_key.verifying_key().to_bytes());
    if expected_public_key != public_key_hex {
        return Err(MainTokenActionAuthError::InvalidRequest(format!(
            "{label} does not match private key: expected={expected_public_key} actual={public_key_hex}"
        )));
    }
    Ok(())
}

fn decode_hex_array<const N: usize>(
    raw: &str,
    label: &str,
) -> Result<[u8; N], MainTokenActionAuthError> {
    let bytes = hex::decode(raw).map_err(|err| {
        MainTokenActionAuthError::InvalidRequest(format!("decode {label} failed: {err}"))
    })?;
    if bytes.len() != N {
        return Err(MainTokenActionAuthError::InvalidRequest(format!(
            "{label} length mismatch: expected {N} bytes, got {}",
            bytes.len()
        )));
    }
    let mut fixed = [0_u8; N];
    fixed.copy_from_slice(bytes.as_slice());
    Ok(fixed)
}

#[cfg(test)]
mod compatibility_tests {
    use super::*;

    const BROWSER_V1_PRIVATE_KEY: &str =
        "c7a149783d4d97d4b36f6f97ae43eb71af7fe595b7f717d329c96be3e58fdc29";
    const BROWSER_V1_PUBLIC_KEY: &str =
        "fded5085f1e8099257b7bfb2346eb6bd4194c3351d8f97686b18cfcc5969e0a3";
    const BROWSER_V1_SIGNATURE: &str = "octransferauth:v1:9200ab505c80b5d27fb1a4e79624a083f2047669404d8c7d562e7d6b7da9154fcf7b85e96ab4c02bd5f2910845e4a7ce791d87398e0e3b9004bb022749d6830b";
    const V2_PRIVATE_KEY: &str = "1111111111111111111111111111111111111111111111111111111111111111";
    const V2_PUBLIC_KEY: &str = "d04ab232742bb4ab3a1368bd4615e4e6d0224ab71a016baf8520a332c9778737";
    const V2_TO_ACCOUNT: &str =
        "oc:pk:204040e364c10f2bec9c1fe500a1cd4c247c89d650a01ed7e82caba867877c21";
    const V2_SIGNATURE: &str = "octransferauth:v2:4c4a7f577c06ed0f1c07966b445de2b271201c779a3224fff4cd66d2ca413b1fbafa609c20023312abeba9a16cfbbc9418d03d2c810e5b3ae4aa20427cbaca0b";

    fn browser_v1_action() -> runtime::Action {
        runtime::Action::TransferMainToken {
            from_account_id: format!("oc:pk:{BROWSER_V1_PUBLIC_KEY}"),
            to_account_id: "oc:pk:1111111111111111111111111111111111111111111111111111111111111111"
                .to_string(),
            amount: 1,
            nonce: 1,
            asset_id: None,
            memo: None,
            chain_id: None,
            network_id: None,
            tx_version: None,
            tx_type: None,
            valid_until_unix_ms: None,
            max_fee: None,
            fee_asset_id: None,
            application_payload_hash: None,
            client_request_id: None,
        }
    }

    fn v2_action() -> runtime::Action {
        runtime::Action::TransferMainToken {
            from_account_id: format!("oc:pk:{V2_PUBLIC_KEY}"),
            to_account_id: V2_TO_ACCOUNT.to_string(),
            amount: 9,
            nonce: 4,
            asset_id: Some("main_token".to_string()),
            memo: Some("bridge:deposit:alpha".to_string()),
            chain_id: Some("oasis7-main".to_string()),
            network_id: Some("prod".to_string()),
            tx_version: Some(2),
            tx_type: Some("asset_transfer".to_string()),
            valid_until_unix_ms: Some(1_900_000_000_000),
            max_fee: Some(42),
            fee_asset_id: Some("main_token".to_string()),
            application_payload_hash: Some("sha256:payload-alpha".to_string()),
            client_request_id: Some("client-alpha".to_string()),
        }
    }

    #[test]
    fn legacy_runtime_wrapper_preserves_browser_v1_vector() {
        let action = browser_v1_action();
        let account_id = format!("oc:pk:{BROWSER_V1_PUBLIC_KEY}");
        let expected_payload = format!(
            r#"{{"version":1,"operation":"transfer_main_token","account_id":"{account_id}","public_key":"{BROWSER_V1_PUBLIC_KEY}","action":{{"type":"TransferMainToken","data":{{"from_account_id":"{account_id}","to_account_id":"oc:pk:1111111111111111111111111111111111111111111111111111111111111111","amount":1,"nonce":1}}}}}}"#,
        );
        let payload = build_main_token_action_signing_payload(
            &action,
            account_id.as_str(),
            BROWSER_V1_PUBLIC_KEY,
        )
        .expect("browser v1 payload");
        assert_eq!(payload, expected_payload.as_bytes());

        let proof = sign_main_token_runtime_action_auth(
            &action,
            account_id.as_str(),
            BROWSER_V1_PUBLIC_KEY,
            BROWSER_V1_PRIVATE_KEY,
        )
        .expect("browser v1 proof");
        assert_eq!(proof.signature.as_deref(), Some(BROWSER_V1_SIGNATURE));
        verify_main_token_runtime_action_auth(&action, &proof).expect("browser v1 verify");
    }

    #[test]
    fn legacy_runtime_wrapper_keeps_noncanonical_signing_accounts_compatible() {
        let action = runtime::Action::TransferMainToken {
            from_account_id: "player:alice".to_string(),
            to_account_id: "player:bob".to_string(),
            amount: 9,
            nonce: 8,
            asset_id: None,
            memo: None,
            chain_id: None,
            network_id: None,
            tx_version: None,
            tx_type: None,
            valid_until_unix_ms: None,
            max_fee: None,
            fee_asset_id: None,
            application_payload_hash: None,
            client_request_id: None,
        };
        let proof = sign_main_token_runtime_action_auth(
            &action,
            "player:alice",
            BROWSER_V1_PUBLIC_KEY,
            BROWSER_V1_PRIVATE_KEY,
        )
        .expect("legacy noncanonical transfer signer");
        assert_eq!(proof.account_id, "player:alice");
        assert_eq!(proof.public_key.as_deref(), Some(BROWSER_V1_PUBLIC_KEY));
        assert!(
            proof
                .signature
                .as_deref()
                .is_some_and(|signature| signature.starts_with("octransferauth:v1:"))
        );
    }

    #[test]
    fn canonical_transfer_signing_keeps_a1_account_binding() {
        let action = browser_v1_action();
        let account_id = format!("oc:pk:{BROWSER_V1_PUBLIC_KEY}");
        let error = sign_main_token_runtime_action_auth(
            &action,
            account_id.as_str(),
            V2_PUBLIC_KEY,
            V2_PRIVATE_KEY,
        )
        .expect_err("canonical account must stay A1-bound");
        assert!(matches!(
            error,
            MainTokenActionAuthError::AccountMismatch(_)
        ));
    }

    #[test]
    fn legacy_runtime_wrapper_preserves_contextual_v2_vector() {
        let action = v2_action();
        let account_id = format!("oc:pk:{V2_PUBLIC_KEY}");
        let expected_payload = format!(
            r#"{{"version":1,"operation":"transfer_main_token","account_id":"{account_id}","public_key":"{V2_PUBLIC_KEY}","action":{{"type":"TransferMainToken","data":{{"from_account_id":"{account_id}","to_account_id":"{V2_TO_ACCOUNT}","amount":9,"nonce":4,"asset_id":"main_token","memo":"bridge:deposit:alpha","chain_id":"oasis7-main","network_id":"prod","tx_version":2,"tx_type":"asset_transfer","valid_until_unix_ms":1900000000000,"max_fee":42,"fee_asset_id":"main_token","application_payload_hash":"sha256:payload-alpha","client_request_id":"client-alpha"}}}}}}"#,
        );
        let payload =
            build_main_token_action_signing_payload(&action, account_id.as_str(), V2_PUBLIC_KEY)
                .expect("v2 payload");
        assert_eq!(payload, expected_payload.as_bytes());

        let proof = sign_main_token_runtime_action_auth(
            &action,
            account_id.as_str(),
            V2_PUBLIC_KEY,
            V2_PRIVATE_KEY,
        )
        .expect("v2 proof");
        assert_eq!(proof.signature.as_deref(), Some(V2_SIGNATURE));
        verify_main_token_runtime_action_auth(&action, &proof).expect("v2 verify");
    }

    #[test]
    fn legacy_runtime_wrapper_keeps_threshold_transfer_verification_local() {
        let action = browser_v1_action();
        let account_id = format!("oc:pk:{BROWSER_V1_PUBLIC_KEY}");
        let proof = sign_threshold_main_token_runtime_action_auth(
            &action,
            account_id.as_str(),
            2,
            &[
                (BROWSER_V1_PUBLIC_KEY, BROWSER_V1_PRIVATE_KEY),
                (V2_PUBLIC_KEY, V2_PRIVATE_KEY),
            ],
        )
        .expect("threshold transfer proof");
        assert_eq!(proof.scheme, MainTokenActionAuthScheme::ThresholdEd25519);
        let error = verify_main_token_runtime_action_auth(&action, &proof)
            .expect_err("runtime threshold transfer binding remains explicit");
        assert!(matches!(
            error,
            MainTokenActionAuthError::InvalidRequest(message)
                if message == "main token auth public key is required for transfer binding"
        ));
    }
}

#[cfg(test)]
#[path = "consensus_action_payload_origin_tests.rs"]
mod origin_tests;
