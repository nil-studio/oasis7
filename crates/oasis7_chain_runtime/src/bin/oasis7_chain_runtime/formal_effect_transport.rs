//! Signed, bounded loopback transport for qualified external effects.
//!
//! The transport never accepts a URL from World data. A route is an explicit
//! operator contract, and each request is bound to a qualified world-local
//! intent identity. Ambiguous delivery is reconciled by a signed receiver
//! query; it is never blindly posted again.

use super::formal_effect_delivery_store::{DeliveryPhaseV1, FormalEffectDeliveryStore};
use super::formal_effect_outbox::{
    ExternalEffectIdentityV1, QualifiedEffectObligationV1, delivery_identity_key, valid_digest,
    validate_qualified_effect_obligation,
};
use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::{
    collections::BTreeSet,
    io::{Read, Write},
    net::{SocketAddr, TcpStream},
    time::{Duration, Instant},
};

const CONTRACT_DOMAIN: &str = "oasis7.formal.effect.receiver_contract.v1";
const REQUEST_DOMAIN: &str = "oasis7.formal.effect.receiver_request.v1";
const EFFECT_REQUEST_DOMAIN: &str = "oasis7.formal.effect.receiver_effect.v1";
const RESPONSE_DOMAIN: &str = "oasis7.formal.effect.receiver_response.v1";
const MAX_ROUTES: usize = 256;
const MAX_AUTHORIZED_SENDERS: usize = 16;
const MAX_HTTP_HEADER_BYTES: usize = 16 * 1024;
const MAX_HTTP_BODY_BYTES: usize = 1024 * 1024;
const MAX_SIGNED_REPLY_BYTES: usize = 256 * 1024;
const MAX_ID_BYTES: usize = 128;

#[cfg(all(test, unix))]
#[path = "formal_effect_transport_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "formal_effect_transport_test_support.rs"]
pub(crate) mod test_support;

/// Fixed, operator-approved local receiver contract. The authorized sender
/// set is part of the signed route digest, allowing an explicitly approved
/// predecessor/successor pair without changing an already committed intent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct FormalEffectReceiverContractV1 {
    pub(super) schema_version: u16,
    pub(super) target_id: String,
    pub(super) receiver_id: String,
    pub(super) address: SocketAddr,
    pub(super) authorized_sender_public_keys_hex: Vec<String>,
    pub(super) receiver_public_key_hex: String,
    /// Safe automatic delivery requires both this guarantee and complete
    /// signed queries. A false value permits observation only.
    pub(super) idempotent_post: bool,
    pub(super) complete_signed_query: bool,
    /// The receiver contract promises this signed outcome means no effect
    /// happened. Without it, a rejection remains ambiguous.
    pub(super) signed_rejection_proves_no_effect: bool,
    pub(super) timeout_ms: u64,
}

impl FormalEffectReceiverContractV1 {
    pub(super) fn validate(&self) -> Result<(), String> {
        if self.schema_version != 1
            || !valid_id(&self.target_id)
            || !valid_id(&self.receiver_id)
            || !self.address.ip().is_loopback()
            || self.address.port() == 0
            || !(1..=60_000).contains(&self.timeout_ms)
            || self.authorized_sender_public_keys_hex.is_empty()
            || self.authorized_sender_public_keys_hex.len() > MAX_AUTHORIZED_SENDERS
        {
            return Err("invalid formal effect receiver contract fields".into());
        }
        if self
            .authorized_sender_public_keys_hex
            .windows(2)
            .any(|pair| pair[0] >= pair[1])
        {
            return Err("authorized effect sender keys must be sorted and unique".into());
        }
        for key in &self.authorized_sender_public_keys_hex {
            decode_verifying_key(key)?;
        }
        decode_verifying_key(&self.receiver_public_key_hex)?;
        Ok(())
    }

    pub(super) fn canonical_digest(&self) -> Result<String, String> {
        self.validate()?;
        let bytes = serde_cbor::to_vec(&(
            CONTRACT_DOMAIN,
            self.schema_version,
            &self.target_id,
            &self.receiver_id,
            self.address,
            &self.authorized_sender_public_keys_hex,
            &self.receiver_public_key_hex,
            self.idempotent_post,
            self.complete_signed_query,
            self.signed_rejection_proves_no_effect,
            self.timeout_ms,
        ))
        .map_err(|error| format!("encode receiver contract failed: {error}"))?;
        Ok(hex::encode(blake3::hash(&bytes).as_bytes()))
    }
}

pub(super) fn validate_receiver_contracts(
    contracts: &[FormalEffectReceiverContractV1],
) -> Result<(), String> {
    if contracts.len() > MAX_ROUTES {
        return Err("formal effect receiver route count exceeds bound".into());
    }
    let mut identities = BTreeSet::new();
    for contract in contracts {
        contract.validate()?;
        let digest = contract.canonical_digest()?;
        if !identities.insert((contract.target_id.clone(), digest)) {
            return Err("duplicate formal effect receiver route".into());
        }
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct EffectDeliveryReceiptV1 {
    pub(super) identity: ExternalEffectIdentityV1,
    pub(super) payload_digest: String,
    pub(super) receiver_id: String,
    pub(super) receipt_payload: JsonValue,
    /// The complete independently signed receiver response. This is not a
    /// World receipt and cannot advance or mutate the committed World.
    pub(super) signed_response: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) enum EffectDeliveryDriveResultV1 {
    Blocked { reason: String },
    Pending { reason: String },
    Unknown { reason: String },
    Delivered(EffectDeliveryReceiptV1),
    RejectedNoEffect { signed_response: Vec<u8> },
    AlreadyTerminal,
}

#[derive(Clone)]
pub(super) struct FormalEffectDeliveryTransport {
    contracts: Vec<FormalEffectReceiverContractV1>,
    sender: SigningKey,
    store: std::sync::Arc<FormalEffectDeliveryStore>,
}

impl FormalEffectDeliveryTransport {
    pub(super) fn new(
        contracts: Vec<FormalEffectReceiverContractV1>,
        sender: SigningKey,
        store: std::sync::Arc<FormalEffectDeliveryStore>,
    ) -> Result<Self, String> {
        validate_receiver_contracts(&contracts)?;
        Ok(Self {
            contracts,
            sender,
            store,
        })
    }

    /// Drive one obligation from a caller-supplied qualified outbox. The
    /// runtime worker must construct that outbox from the exact current or
    /// replayed qualified record immediately before this call.
    pub(super) fn drive_qualified_obligation(
        &self,
        obligation: &QualifiedEffectObligationV1,
    ) -> EffectDeliveryDriveResultV1 {
        if let Err(reason) = validate_qualified_effect_obligation(obligation) {
            return EffectDeliveryDriveResultV1::Blocked { reason };
        }
        let contract = match self.contracts.iter().find(|contract| {
            contract.target_id == obligation.target_id
                && contract
                    .canonical_digest()
                    .is_ok_and(|digest| digest == obligation.receiver_contract_digest)
        }) {
            Some(contract) => contract,
            None => {
                return EffectDeliveryDriveResultV1::Blocked {
                    reason: "no configured receiver exactly matches committed target and contract digest".into(),
                };
            }
        };
        if !contract
            .authorized_sender_public_keys_hex
            .iter()
            .any(|allowed| allowed == &hex::encode(self.sender.verifying_key().to_bytes()))
        {
            return EffectDeliveryDriveResultV1::Blocked {
                reason:
                    "this node signing key is not authorized by the committed receiver contract"
                        .into(),
            };
        }
        let contract_digest = match contract.canonical_digest() {
            Ok(digest) => digest,
            Err(reason) => return EffectDeliveryDriveResultV1::Blocked { reason },
        };
        let record = match self.store.observe(obligation) {
            Ok(record) => record,
            Err(reason) => return EffectDeliveryDriveResultV1::Unknown { reason },
        };
        if record.phase.is_terminal() {
            return match verify_terminal_delivery_record(contract, obligation, &record.phase) {
                Ok(()) => EffectDeliveryDriveResultV1::AlreadyTerminal,
                Err(reason) => EffectDeliveryDriveResultV1::Unknown { reason },
            };
        }
        let effect_request =
            match build_effect_request(obligation, contract, &contract_digest, &self.sender) {
                Ok(request) => request,
                Err(reason) => return EffectDeliveryDriveResultV1::Blocked { reason },
            };
        let effect_request_digest = effect_request.body.effect_request_digest.clone();
        let rpc_request_digest =
            match request_digest(&effect_request.body, &effect_request.sender_public_key_hex) {
                Ok(digest) => digest,
                Err(reason) => return EffectDeliveryDriveResultV1::Blocked { reason },
            };
        match record.phase {
            DeliveryPhaseV1::Ready => {
                if !(contract.idempotent_post && contract.complete_signed_query) {
                    return EffectDeliveryDriveResultV1::Blocked {
                        reason: "automatic delivery requires idempotent POST and complete signed query guarantees".into(),
                    };
                }
                self.reconcile_attempt(
                    contract,
                    obligation,
                    &effect_request,
                    &rpc_request_digest,
                    &effect_request_digest,
                    &contract_digest,
                )
            }
            DeliveryPhaseV1::RetryAuthorized {
                effect_request_digest: authorized_digest,
                query_digest,
                signed_query_not_found,
                ..
            } => {
                if !(contract.idempotent_post && contract.complete_signed_query)
                    || authorized_digest != effect_request_digest
                {
                    return EffectDeliveryDriveResultV1::Unknown {
                        reason: "durable retry authorization does not match the idempotent request"
                            .into(),
                    };
                }
                if let Err(reason) = verify_retry_authorization(
                    contract,
                    obligation,
                    &signed_query_not_found,
                    &query_digest,
                    &effect_request_digest,
                ) {
                    return EffectDeliveryDriveResultV1::Unknown { reason };
                }
                self.post_effect(
                    contract,
                    &effect_request,
                    &rpc_request_digest,
                    &effect_request_digest,
                    obligation,
                )
            }
            DeliveryPhaseV1::AttemptStarted { .. }
            | DeliveryPhaseV1::Unknown { .. }
            | DeliveryPhaseV1::QueryStarted { .. } => self.reconcile_attempt(
                contract,
                obligation,
                &effect_request,
                &rpc_request_digest,
                &effect_request_digest,
                &contract_digest,
            ),
            DeliveryPhaseV1::Delivered { .. } | DeliveryPhaseV1::RejectedNoEffect { .. } => {
                EffectDeliveryDriveResultV1::Unknown {
                    reason: "terminal delivery record bypassed signature validation".into(),
                }
            }
        }
    }

    fn post_effect(
        &self,
        contract: &FormalEffectReceiverContractV1,
        request: &SignedEffectRequestV1,
        rpc_request_digest: &str,
        effect_request_digest: &str,
        obligation: &QualifiedEffectObligationV1,
    ) -> EffectDeliveryDriveResultV1 {
        if !(contract.idempotent_post && contract.complete_signed_query) {
            return EffectDeliveryDriveResultV1::Blocked {
                reason: "unsafe receiver contract cannot automatically post an effect".into(),
            };
        }
        let request_bytes = match encode_bounded_body(request, MAX_HTTP_BODY_BYTES) {
            Ok(bytes) => bytes,
            Err(reason) => return EffectDeliveryDriveResultV1::Blocked { reason },
        };
        if let Err(reason) = self.store.begin_attempt(obligation, effect_request_digest) {
            return EffectDeliveryDriveResultV1::Unknown { reason };
        }
        let raw = match http_post_bytes(contract, "/v1/effects/deliver", &request_bytes) {
            Ok(raw) => raw,
            Err(reason) => {
                let _ = self.store.mark_unknown(obligation, &reason);
                return EffectDeliveryDriveResultV1::Unknown { reason };
            }
        };
        let signed: SignedEffectResponseV1 = match decode_bounded_reply(&raw) {
            Ok(response) => response,
            Err(reason) => {
                let _ = self.store.mark_unknown(obligation, &reason);
                return EffectDeliveryDriveResultV1::Unknown { reason };
            }
        };
        let response = match verify_response(
            contract,
            &signed,
            &obligation.identity,
            "deliver",
            rpc_request_digest,
            effect_request_digest,
        ) {
            Ok(response) => response,
            Err(reason) => {
                let _ = self.store.mark_unknown(obligation, &reason);
                return EffectDeliveryDriveResultV1::Unknown { reason };
            }
        };
        self.apply_delivery_response(
            contract,
            obligation,
            effect_request_digest,
            &signed,
            (*response).clone(),
        )
    }

    fn reconcile_attempt(
        &self,
        contract: &FormalEffectReceiverContractV1,
        obligation: &QualifiedEffectObligationV1,
        effect_request: &SignedEffectRequestV1,
        effect_rpc_request_digest: &str,
        effect_request_digest: &str,
        contract_digest: &str,
    ) -> EffectDeliveryDriveResultV1 {
        if !contract.complete_signed_query {
            let reason = "receiver contract cannot completely query an attempted identity";
            let _ = self.store.mark_unknown(obligation, reason);
            return EffectDeliveryDriveResultV1::Unknown {
                reason: reason.into(),
            };
        }
        let delivery_key = match delivery_identity_key(&obligation.identity) {
            Ok(key) => key,
            Err(reason) => return EffectDeliveryDriveResultV1::Unknown { reason },
        };
        let query_body = EffectRequestBodyV1 {
            schema_version: 1,
            receiver_id: contract.receiver_id.clone(),
            contract_digest: contract_digest.to_owned(),
            operation: "query".into(),
            identity: obligation.identity.clone(),
            target_id: obligation.target_id.clone(),
            delivery_key,
            payload_digest: obligation.payload_digest.clone(),
            effect_request_digest: effect_request_digest.to_owned(),
            intent: None,
            committed_at: None,
        };
        let query = match sign_request(query_body, &self.sender) {
            Ok(query) => query,
            Err(reason) => return EffectDeliveryDriveResultV1::Unknown { reason },
        };
        let query_digest = match request_digest(&query.body, &query.sender_public_key_hex) {
            Ok(digest) => digest,
            Err(reason) => return EffectDeliveryDriveResultV1::Unknown { reason },
        };
        if let Err(reason) =
            self.store
                .begin_query(obligation, &query_digest, effect_request_digest)
        {
            return EffectDeliveryDriveResultV1::Unknown { reason };
        }
        let raw = match http_post_cbor(contract, "/v1/effects/query", &query, MAX_HTTP_BODY_BYTES) {
            Ok(raw) => raw,
            Err(reason) => {
                let _ = self.store.mark_unknown(obligation, &reason);
                return EffectDeliveryDriveResultV1::Unknown { reason };
            }
        };
        let signed: SignedEffectResponseV1 = match decode_bounded_reply(&raw) {
            Ok(response) => response,
            Err(reason) => {
                let _ = self.store.mark_unknown(obligation, &reason);
                return EffectDeliveryDriveResultV1::Unknown { reason };
            }
        };
        let response = match verify_response(
            contract,
            &signed,
            &obligation.identity,
            "query",
            &query_digest,
            effect_request_digest,
        ) {
            Ok(response) => response,
            Err(reason) => {
                let _ = self.store.mark_unknown(obligation, &reason);
                return EffectDeliveryDriveResultV1::Unknown { reason };
            }
        };
        match response.outcome {
            EffectResponseOutcomeV1::NotFound { complete: true } => {
                if !contract.idempotent_post {
                    let reason = "signed query proves absence but receiver does not guarantee idempotent POST";
                    let _ = self.store.mark_unknown(obligation, reason);
                    return EffectDeliveryDriveResultV1::Unknown {
                        reason: reason.into(),
                    };
                }
                let raw_proof = match serde_cbor::to_vec(&signed) {
                    Ok(bytes) => bytes,
                    Err(error) => {
                        return EffectDeliveryDriveResultV1::Unknown {
                            reason: format!("encode signed query proof failed: {error}"),
                        };
                    }
                };
                if let Err(reason) = self.store.authorize_retry(obligation, raw_proof) {
                    return EffectDeliveryDriveResultV1::Unknown { reason };
                }
                self.post_effect(
                    contract,
                    effect_request,
                    effect_rpc_request_digest,
                    effect_request_digest,
                    obligation,
                )
            }
            EffectResponseOutcomeV1::NotFound { complete: false } => {
                let reason = "receiver returned an incomplete absence proof";
                let _ = self.store.mark_unknown(obligation, reason);
                EffectDeliveryDriveResultV1::Unknown {
                    reason: reason.into(),
                }
            }
            _ => self.apply_delivery_response(
                contract,
                obligation,
                effect_request_digest,
                &signed,
                (*response).clone(),
            ),
        }
    }

    fn apply_delivery_response(
        &self,
        contract: &FormalEffectReceiverContractV1,
        obligation: &QualifiedEffectObligationV1,
        effect_request_digest: &str,
        signed: &SignedEffectResponseV1,
        response: EffectResponseBodyV1,
    ) -> EffectDeliveryDriveResultV1 {
        match response.outcome {
            EffectResponseOutcomeV1::Delivered { receipt_payload } => {
                let signed_response = match serde_cbor::to_vec(signed) {
                    Ok(bytes) => bytes,
                    Err(error) => {
                        return EffectDeliveryDriveResultV1::Unknown {
                            reason: format!("encode signed delivery receipt failed: {error}"),
                        };
                    }
                };
                if let Err(reason) = self.store.mark_delivered(
                    obligation,
                    signed_response.clone(),
                    effect_request_digest,
                ) {
                    return EffectDeliveryDriveResultV1::Unknown { reason };
                }
                EffectDeliveryDriveResultV1::Delivered(EffectDeliveryReceiptV1 {
                    identity: obligation.identity.clone(),
                    payload_digest: obligation.payload_digest.clone(),
                    receiver_id: contract.receiver_id.clone(),
                    receipt_payload,
                    signed_response,
                })
            }
            EffectResponseOutcomeV1::RejectedNoEffect { .. }
                if contract.signed_rejection_proves_no_effect =>
            {
                let signed_response = match serde_cbor::to_vec(signed) {
                    Ok(bytes) => bytes,
                    Err(error) => {
                        return EffectDeliveryDriveResultV1::Unknown {
                            reason: format!("encode signed rejection failed: {error}"),
                        };
                    }
                };
                match self.store.mark_rejected_no_effect(
                    obligation,
                    signed_response.clone(),
                    effect_request_digest,
                ) {
                    Ok(_) => EffectDeliveryDriveResultV1::RejectedNoEffect { signed_response },
                    Err(reason) => EffectDeliveryDriveResultV1::Unknown { reason },
                }
            }
            EffectResponseOutcomeV1::Pending => EffectDeliveryDriveResultV1::Pending {
                reason: "receiver reports the identity is still pending".into(),
            },
            EffectResponseOutcomeV1::NotFound { .. } => EffectDeliveryDriveResultV1::Unknown {
                reason: "query did not produce an actionable complete result".into(),
            },
            EffectResponseOutcomeV1::RejectedNoEffect { .. }
            | EffectResponseOutcomeV1::Rejected { .. } => EffectDeliveryDriveResultV1::Unknown {
                reason: "receiver rejection is not contractually proven to have no effect".into(),
            },
        }
    }
}

fn verify_terminal_delivery_record(
    contract: &FormalEffectReceiverContractV1,
    obligation: &QualifiedEffectObligationV1,
    phase: &DeliveryPhaseV1,
) -> Result<(), String> {
    let stable_digest = stable_effect_request_digest(obligation, &contract.canonical_digest()?)?;
    let (phase_digest, signed_bytes, accepted_outcome) = match phase {
        DeliveryPhaseV1::Delivered {
            effect_request_digest,
            signed_receipt,
            ..
        } => (effect_request_digest.as_str(), signed_receipt, "delivered"),
        DeliveryPhaseV1::RejectedNoEffect {
            effect_request_digest,
            signed_rejection,
            ..
        } => {
            if !contract.signed_rejection_proves_no_effect {
                return Err("receiver contract does not authorize no-effect rejection".into());
            }
            (
                effect_request_digest.as_str(),
                signed_rejection,
                "rejected_no_effect",
            )
        }
        _ => return Err("delivery record is not terminal".into()),
    };
    if phase_digest != stable_digest {
        return Err("terminal effect record stable digest differs from qualified intent".into());
    }
    let signed = decode_bounded_reply(signed_bytes)?;
    if !matches!(signed.body.operation.as_str(), "deliver" | "query")
        || !valid_digest(&signed.body.response_to_digest)
    {
        return Err("terminal effect receipt has invalid operation or RPC correlation".into());
    }
    verify_response(
        contract,
        &signed,
        &obligation.identity,
        &signed.body.operation,
        &signed.body.response_to_digest,
        &stable_digest,
    )?;
    let outcome_matches = matches!(
        (accepted_outcome, &signed.body.outcome),
        ("delivered", EffectResponseOutcomeV1::Delivered { .. })
            | (
                "rejected_no_effect",
                EffectResponseOutcomeV1::RejectedNoEffect { .. }
            )
    );
    if !outcome_matches {
        return Err("terminal effect record outcome differs from signed receiver response".into());
    }
    Ok(())
}

fn verify_retry_authorization(
    contract: &FormalEffectReceiverContractV1,
    obligation: &QualifiedEffectObligationV1,
    signed_bytes: &[u8],
    query_digest: &str,
    stable_digest: &str,
) -> Result<(), String> {
    let signed = decode_bounded_reply(signed_bytes)?;
    let response = verify_response(
        contract,
        &signed,
        &obligation.identity,
        "query",
        query_digest,
        stable_digest,
    )?;
    match response.outcome {
        EffectResponseOutcomeV1::NotFound { complete: true } => Ok(()),
        _ => Err("durable retry proof is not a complete signed NotFound for this query".into()),
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct EffectRequestBodyV1 {
    schema_version: u16,
    receiver_id: String,
    contract_digest: String,
    operation: String,
    identity: ExternalEffectIdentityV1,
    target_id: String,
    delivery_key: String,
    payload_digest: String,
    effect_request_digest: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    intent: Option<oasis7_runtime::EffectIntent>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    committed_at: Option<super::formal_effect_outbox::QualifiedEffectCommitRefV1>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SignedEffectRequestV1 {
    body: EffectRequestBodyV1,
    sender_public_key_hex: String,
    signature_hex: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct EffectResponseBodyV1 {
    schema_version: u16,
    receiver_id: String,
    contract_digest: String,
    operation: String,
    response_to_digest: String,
    effect_request_digest: String,
    identity: ExternalEffectIdentityV1,
    outcome: EffectResponseOutcomeV1,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "snake_case", deny_unknown_fields)]
enum EffectResponseOutcomeV1 {
    Delivered { receipt_payload: JsonValue },
    NotFound { complete: bool },
    Pending,
    RejectedNoEffect { reason: String },
    Rejected { reason: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SignedEffectResponseV1 {
    body: EffectResponseBodyV1,
    signature_hex: String,
}

fn build_effect_request(
    obligation: &QualifiedEffectObligationV1,
    contract: &FormalEffectReceiverContractV1,
    contract_digest: &str,
    sender: &SigningKey,
) -> Result<SignedEffectRequestV1, String> {
    let delivery_key = delivery_identity_key(&obligation.identity)?;
    let effect_request_digest = stable_effect_request_digest(obligation, contract_digest)?;
    let body = EffectRequestBodyV1 {
        schema_version: 1,
        receiver_id: contract.receiver_id.clone(),
        contract_digest: contract_digest.to_owned(),
        operation: "deliver".into(),
        identity: obligation.identity.clone(),
        target_id: obligation.target_id.clone(),
        delivery_key,
        payload_digest: obligation.payload_digest.clone(),
        effect_request_digest,
        intent: Some(obligation.intent.clone()),
        committed_at: Some(obligation.committed_at.clone()),
    };
    sign_request(body, sender)
}

/// Stable receiver idempotency identity. It survives a different qualified
/// head, sidecar loss, and an authorized predecessor/successor sender change.
fn stable_effect_request_digest(
    obligation: &QualifiedEffectObligationV1,
    contract_digest: &str,
) -> Result<String, String> {
    stable_effect_request_digest_fields(
        &obligation.identity,
        &obligation.intent,
        &obligation.payload_digest,
        &obligation.target_id,
        contract_digest,
    )
}

fn stable_effect_request_digest_fields(
    identity: &ExternalEffectIdentityV1,
    intent: &oasis7_runtime::EffectIntent,
    payload_digest: &str,
    target_id: &str,
    contract_digest: &str,
) -> Result<String, String> {
    let bytes = serde_cbor::to_vec(&(
        EFFECT_REQUEST_DOMAIN,
        identity,
        intent,
        payload_digest,
        target_id,
        contract_digest,
    ))
    .map_err(|error| format!("encode stable effect request digest failed: {error}"))?;
    Ok(hex::encode(blake3::hash(&bytes).as_bytes()))
}

/// Per-RPC reply correlation digest. Unlike the stable effect identity, this
/// intentionally binds the current operation, provenance and sender key.
fn request_digest(
    body: &EffectRequestBodyV1,
    sender_public_key_hex: &str,
) -> Result<String, String> {
    let bytes = serde_cbor::to_vec(&(
        REQUEST_DOMAIN,
        body.schema_version,
        &body.receiver_id,
        &body.contract_digest,
        &body.operation,
        &body.identity,
        &body.target_id,
        &body.delivery_key,
        &body.payload_digest,
        &body.effect_request_digest,
        &body.intent,
        &body.committed_at,
        sender_public_key_hex,
    ))
    .map_err(|error| format!("encode effect request digest failed: {error}"))?;
    Ok(hex::encode(blake3::hash(&bytes).as_bytes()))
}

fn sign_request(
    body: EffectRequestBodyV1,
    sender: &SigningKey,
) -> Result<SignedEffectRequestV1, String> {
    let bytes = serde_cbor::to_vec(&(REQUEST_DOMAIN, &body))
        .map_err(|error| format!("encode signed effect request failed: {error}"))?;
    Ok(SignedEffectRequestV1 {
        body,
        sender_public_key_hex: hex::encode(sender.verifying_key().to_bytes()),
        signature_hex: hex::encode(sender.sign(&bytes).to_bytes()),
    })
}

fn verify_response<'a>(
    contract: &FormalEffectReceiverContractV1,
    signed: &'a SignedEffectResponseV1,
    expected_identity: &ExternalEffectIdentityV1,
    expected_operation: &str,
    expected_response_to: &str,
    expected_effect_request_digest: &str,
) -> Result<&'a EffectResponseBodyV1, String> {
    if signed.body.schema_version != 1
        || signed.body.receiver_id != contract.receiver_id
        || signed.body.contract_digest != contract.canonical_digest()?
        || signed.body.operation != expected_operation
        || signed.body.response_to_digest != expected_response_to
        || signed.body.effect_request_digest != expected_effect_request_digest
        || &signed.body.identity != expected_identity
    {
        return Err("signed effect response does not match request identity".into());
    }
    let signature_bytes = serde_cbor::to_vec(&(RESPONSE_DOMAIN, &signed.body))
        .map_err(|error| format!("encode signed effect response failed: {error}"))?;
    let signature = decode_signature(&signed.signature_hex)?;
    decode_verifying_key(&contract.receiver_public_key_hex)?
        .verify_strict(&signature_bytes, &signature)
        .map_err(|_| "effect receiver signature invalid".to_owned())?;
    Ok(&signed.body)
}

fn decode_bounded_reply(raw: &[u8]) -> Result<SignedEffectResponseV1, String> {
    if raw.is_empty() || raw.len() > MAX_SIGNED_REPLY_BYTES {
        return Err("signed effect response is empty or exceeds bound".into());
    }
    serde_cbor::from_slice(raw).map_err(|error| format!("decode signed effect response: {error}"))
}

fn http_post_cbor<T: Serialize>(
    contract: &FormalEffectReceiverContractV1,
    path: &str,
    value: &T,
    max_body_bytes: usize,
) -> Result<Vec<u8>, String> {
    let body = encode_bounded_body(value, max_body_bytes)?;
    http_post_bytes(contract, path, &body)
}

fn encode_bounded_body<T: Serialize>(value: &T, max_body_bytes: usize) -> Result<Vec<u8>, String> {
    let body = serde_cbor::to_vec(value)
        .map_err(|error| format!("encode effect HTTP request failed: {error}"))?;
    if body.is_empty() || body.len() > max_body_bytes || body.len() > MAX_HTTP_BODY_BYTES {
        return Err("effect HTTP request exceeds body bound".into());
    }
    Ok(body)
}

fn http_post_bytes(
    contract: &FormalEffectReceiverContractV1,
    path: &str,
    body: &[u8],
) -> Result<Vec<u8>, String> {
    if !contract.address.ip().is_loopback() {
        return Err("effect transport refuses a non-loopback receiver".into());
    }
    if body.is_empty() || body.len() > MAX_HTTP_BODY_BYTES {
        return Err("effect HTTP request exceeds body bound".into());
    }
    let timeout = Duration::from_millis(contract.timeout_ms);
    let deadline = Instant::now() + timeout;
    let mut stream = TcpStream::connect_timeout(&contract.address, timeout)
        .map_err(|error| format!("connect effect receiver failed: {error}"))?;
    let header = format!(
        "POST {path} HTTP/1.1\r\nHost: {}\r\nContent-Type: application/cbor\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        contract.address,
        body.len()
    );
    write_all_before(&mut stream, header.as_bytes(), deadline)?;
    write_all_before(&mut stream, body, deadline)?;
    read_http_response(&mut stream, deadline)
}

fn read_http_response(stream: &mut TcpStream, deadline: Instant) -> Result<Vec<u8>, String> {
    let mut header = Vec::with_capacity(1024);
    let mut byte = [0u8; 1];
    loop {
        if header.len() >= MAX_HTTP_HEADER_BYTES {
            return Err("effect HTTP response headers exceed bound".into());
        }
        read_exact_before(stream, &mut byte, deadline)
            .map_err(|error| format!("read effect HTTP response headers failed: {error}"))?;
        header.push(byte[0]);
        if header.ends_with(b"\r\n\r\n") {
            break;
        }
    }
    let header_text =
        std::str::from_utf8(&header).map_err(|_| "effect HTTP response headers are not ASCII")?;
    let mut lines = header_text.split("\r\n");
    let status = lines.next().ok_or("effect HTTP response status missing")?;
    if status != "HTTP/1.1 200 OK" && status != "HTTP/1.0 200 OK" {
        return Err("effect HTTP receiver returned a non-success status".into());
    }
    let mut content_length = None;
    let mut transfer_encoding = false;
    for line in lines {
        if line.is_empty() {
            continue;
        }
        let (name, value) = line
            .split_once(':')
            .ok_or("malformed effect HTTP response header")?;
        if name.eq_ignore_ascii_case("content-length") {
            if content_length.is_some() {
                return Err("duplicate effect HTTP Content-Length".into());
            }
            content_length = Some(
                value
                    .trim()
                    .parse::<usize>()
                    .map_err(|_| "invalid effect HTTP Content-Length")?,
            );
        } else if name.eq_ignore_ascii_case("transfer-encoding") {
            transfer_encoding = true;
        }
    }
    if transfer_encoding {
        return Err("chunked effect HTTP responses are unsupported".into());
    }
    let length = content_length.ok_or("effect HTTP Content-Length is required")?;
    if length == 0 || length > MAX_SIGNED_REPLY_BYTES || length > MAX_HTTP_BODY_BYTES {
        return Err("effect HTTP response body exceeds bound".into());
    }
    let mut body = vec![0u8; length];
    read_exact_before(stream, &mut body, deadline)
        .map_err(|error| format!("read effect HTTP response body failed: {error}"))?;
    Ok(body)
}

fn write_all_before(
    stream: &mut TcpStream,
    mut bytes: &[u8],
    deadline: Instant,
) -> Result<(), String> {
    while !bytes.is_empty() {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err("effect receiver operation deadline expired while writing".into());
        }
        stream
            .set_write_timeout(Some(remaining))
            .map_err(|error| format!("configure effect write deadline failed: {error}"))?;
        let written = stream
            .write(bytes)
            .map_err(|error| format!("write effect receiver request failed: {error}"))?;
        if written == 0 {
            return Err("effect receiver closed while writing".into());
        }
        bytes = &bytes[written..];
    }
    Ok(())
}

fn read_exact_before(
    stream: &mut TcpStream,
    mut bytes: &mut [u8],
    deadline: Instant,
) -> Result<(), String> {
    while !bytes.is_empty() {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err("effect receiver operation deadline expired while reading".into());
        }
        stream
            .set_read_timeout(Some(remaining))
            .map_err(|error| format!("configure effect read deadline failed: {error}"))?;
        let read = stream
            .read(bytes)
            .map_err(|error| format!("read effect receiver response failed: {error}"))?;
        if read == 0 {
            return Err("effect receiver closed before response completed".into());
        }
        bytes = &mut bytes[read..];
    }
    Ok(())
}

fn decode_verifying_key(hex_value: &str) -> Result<VerifyingKey, String> {
    if hex_value.len() != 64 || !hex_value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("effect receiver contract public key must be 32-byte hex".into());
    }
    let bytes: [u8; 32] = hex::decode(hex_value)
        .map_err(|_| "invalid effect receiver contract public key")?
        .try_into()
        .map_err(|_| "invalid effect receiver contract public key length")?;
    if hex::encode(bytes) != hex_value {
        return Err("effect receiver public keys must use lowercase canonical hex".into());
    }
    VerifyingKey::from_bytes(&bytes).map_err(|_| "invalid effect receiver Ed25519 key".into())
}

fn decode_signature(hex_value: &str) -> Result<Signature, String> {
    if hex_value.len() != 128 || !hex_value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("effect receiver signature must be 64-byte hex".into());
    }
    let bytes: [u8; 64] = hex::decode(hex_value)
        .map_err(|_| "invalid effect receiver signature")?
        .try_into()
        .map_err(|_| "invalid effect receiver signature length")?;
    Ok(Signature::from_bytes(&bytes))
}

fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_ID_BYTES
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"._-".contains(&byte)
        })
}
