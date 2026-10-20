use std::fs;
use std::path::{Path, PathBuf};

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use oasis7::network_tier_manifest::LoadedNetworkTierManifest;
use oasis7_node::{NodeSnapshot, ReplicationNetworkDebugSnapshot};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::execution_bridge::ExecutionCheckpointStatusEvidence;
use super::feedback_submit_api::FeedbackSubmitSigner;

const MAX_CONNECTED_PEER_IDS: usize = 64;
const MAX_ERROR_BYTES: usize = 512;
const MAX_REBUILD_PROOF_FILE_BYTES: u64 = 512 * 1024;
const REBUILD_PROOF_SCHEMA: &str = "oasis7.rebuild_proof.v1";

#[derive(Debug, Serialize)]
pub(super) struct RebuildStatusResponse {
    pub(super) schema_version: &'static str,
    pub(super) observed_at_unix_ms: i64,
    pub(super) ok: bool,
    pub(super) liveness: RebuildLiveness,
    pub(super) readiness: RebuildReadiness,
    pub(super) heights: RebuildHeights,
    pub(super) network_head: RebuildNetworkHead,
    pub(super) checkpoint: Option<RebuildCheckpoint>,
    pub(super) local_peer_id: String,
    pub(super) connected_peers: Vec<String>,
    pub(super) connected_peer_count: usize,
    pub(super) proof: RebuildProofEnvelope,
}

#[derive(Debug, Serialize)]
pub(super) struct RebuildLiveness {
    pub(super) running: bool,
    pub(super) last_error: Option<String>,
}

#[derive(Debug, Serialize)]
pub(super) struct RebuildReadiness {
    pub(super) status: &'static str,
    pub(super) failed_gates: Vec<String>,
}

#[derive(Debug, Serialize)]
pub(super) struct RebuildHeights {
    pub(super) committed_height: u64,
    pub(super) network_committed_height: u64,
    pub(super) last_execution_height: u64,
}

#[derive(Debug, Serialize)]
pub(super) struct RebuildNetworkHead {
    pub(super) source: String,
    pub(super) decision: String,
    pub(super) height: Option<u64>,
    pub(super) block_hash: Option<String>,
    pub(super) execution_block_hash: Option<String>,
    pub(super) execution_state_root: Option<String>,
    pub(super) observed_peer_count: usize,
    pub(super) fresh_peer_count: usize,
}

#[derive(Debug, Serialize)]
pub(super) struct RebuildCheckpoint {
    pub(super) schema_version: u32,
    pub(super) checkpoint_id: String,
    pub(super) world_id: String,
    pub(super) height: u64,
    pub(super) execution_block_hash: String,
    pub(super) execution_state_root: String,
    pub(super) manifest_hash: String,
}

#[derive(Debug, Serialize)]
pub(super) struct RebuildProofEnvelope {
    pub(super) schema_version: &'static str,
    pub(super) signer_id: String,
    pub(super) signer_public_key_hex: String,
    pub(super) signed_payload_sha256: String,
    pub(super) signature_hex: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RebuildStatusWire {
    schema_version: String,
    observed_at_unix_ms: i64,
    ok: bool,
    liveness: RebuildLivenessWire,
    readiness: RebuildReadinessWire,
    heights: RebuildHeightsWire,
    network_head: RebuildNetworkHeadWire,
    checkpoint: Option<RebuildCheckpointWire>,
    local_peer_id: String,
    connected_peers: Vec<String>,
    connected_peer_count: usize,
    proof: RebuildProofEnvelopeWire,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RebuildLivenessWire {
    running: bool,
    last_error: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RebuildReadinessWire {
    status: String,
    failed_gates: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RebuildHeightsWire {
    committed_height: u64,
    network_committed_height: u64,
    last_execution_height: u64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RebuildNetworkHeadWire {
    source: String,
    decision: String,
    height: Option<u64>,
    block_hash: Option<String>,
    execution_block_hash: Option<String>,
    execution_state_root: Option<String>,
    observed_peer_count: usize,
    fresh_peer_count: usize,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RebuildCheckpointWire {
    schema_version: u32,
    checkpoint_id: String,
    world_id: String,
    height: u64,
    execution_block_hash: String,
    execution_state_root: String,
    manifest_hash: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RebuildProofEnvelopeWire {
    schema_version: String,
    signer_id: String,
    signer_public_key_hex: String,
    signed_payload_sha256: String,
    signature_hex: String,
}

#[derive(Debug, Serialize)]
pub(super) struct RebuildProofVerificationReceipt {
    schema_version: &'static str,
    proof_schema_version: &'static str,
    signer_id: String,
    signer_public_key_hex: String,
    signed_payload_sha256: String,
    pub(super) local_peer_id: String,
    pub(super) proof_sha256: String,
    verified: bool,
}

#[derive(Debug, Serialize)]
struct RebuildProofClaims<'a> {
    schema_version: &'static str,
    observed_at_unix_ms: i64,
    node_id: &'a str,
    world_id: &'a str,
    ok: bool,
    liveness: &'a RebuildLiveness,
    readiness: &'a RebuildReadiness,
    heights: &'a RebuildHeights,
    network_head: &'a RebuildNetworkHead,
    checkpoint: Option<&'a RebuildCheckpoint>,
    local_peer_id: &'a str,
    connected_peers: &'a [String],
    connected_peer_count: usize,
}

pub(super) fn build_rebuild_status_with_signer(
    snapshot: NodeSnapshot,
    network: ReplicationNetworkDebugSnapshot,
    checkpoint: Option<ExecutionCheckpointStatusEvidence>,
    observed_at_unix_ms: i64,
    manifest: Option<&LoadedNetworkTierManifest>,
    signer: &FeedbackSubmitSigner,
) -> Result<RebuildStatusResponse, String> {
    let network_head =
        super::status_payload::build_network_head_status(&snapshot, observed_at_unix_ms, manifest);
    let checkpoint = checkpoint.map(|evidence| RebuildCheckpoint {
        schema_version: evidence.schema_version,
        checkpoint_id: evidence.checkpoint_id,
        world_id: evidence.world_id,
        height: evidence.height,
        execution_block_hash: evidence.execution_block_hash,
        execution_state_root: evidence.execution_state_root,
        manifest_hash: evidence.manifest_hash,
    });
    let mut failed_gates = Vec::new();
    if !snapshot.running {
        failed_gates.push("runtime_not_running".to_string());
    }
    if snapshot.last_error.is_some() {
        failed_gates.push("runtime_last_error".to_string());
    }
    if snapshot.replication_enabled && network_head.decision != "ready" {
        failed_gates.push("network_head_not_ready".to_string());
    }
    if snapshot.consensus.committed_height == 0 {
        failed_gates.push("committed_height_zero".to_string());
    }
    if snapshot.consensus.last_execution_height == 0 {
        failed_gates.push("execution_height_zero".to_string());
    }
    match checkpoint.as_ref() {
        None => failed_gates.push("checkpoint_unavailable".to_string()),
        Some(checkpoint) if !checkpoint_matches_runtime_heads(&snapshot, checkpoint) => {
            failed_gates.push("checkpoint_head_mismatch".to_string());
        }
        Some(_) => {}
    }
    let readiness_status = if failed_gates.is_empty() {
        "ready"
    } else {
        "not_ready"
    };
    let connected_peer_count = network.connected_peers.len();
    let connected_peers = network
        .connected_peers
        .into_iter()
        .take(MAX_CONNECTED_PEER_IDS)
        .collect::<Vec<_>>();
    let last_error = snapshot
        .last_error
        .map(|error| error.chars().take(MAX_ERROR_BYTES).collect::<String>());
    let mut response = RebuildStatusResponse {
        schema_version: "oasis7.rebuild_status.v1",
        observed_at_unix_ms,
        ok: readiness_status == "ready",
        liveness: RebuildLiveness {
            running: snapshot.running,
            last_error,
        },
        readiness: RebuildReadiness {
            status: readiness_status,
            failed_gates,
        },
        heights: RebuildHeights {
            committed_height: snapshot.consensus.committed_height,
            network_committed_height: snapshot.consensus.network_committed_height,
            last_execution_height: snapshot.consensus.last_execution_height,
        },
        network_head: RebuildNetworkHead {
            source: network_head.source,
            decision: network_head.decision,
            height: network_head.height,
            block_hash: network_head.block_hash,
            execution_block_hash: network_head.execution_block_hash,
            execution_state_root: network_head.execution_state_root,
            observed_peer_count: network_head.observed_peer_count,
            fresh_peer_count: network_head.fresh_peer_count,
        },
        checkpoint,
        local_peer_id: network.local_peer_id,
        connected_peers,
        connected_peer_count,
        proof: RebuildProofEnvelope {
            schema_version: REBUILD_PROOF_SCHEMA,
            signer_id: snapshot.node_id.clone(),
            signer_public_key_hex: String::new(),
            signed_payload_sha256: String::new(),
            signature_hex: String::new(),
        },
    };
    response.proof = sign_rebuild_proof(&response, signer)?;
    Ok(response)
}

fn checkpoint_matches_runtime_heads(
    snapshot: &NodeSnapshot,
    checkpoint: &RebuildCheckpoint,
) -> bool {
    if checkpoint.world_id != snapshot.world_id
        || checkpoint.height > snapshot.consensus.committed_height
        || checkpoint.height > snapshot.consensus.last_execution_height
    {
        return false;
    }
    if snapshot.replication_enabled
        && snapshot.consensus.network_committed_height > 0
        && checkpoint.height > snapshot.consensus.network_committed_height
    {
        return false;
    }
    if checkpoint.height == snapshot.consensus.last_execution_height {
        snapshot.consensus.last_execution_block_hash.as_deref()
            == Some(checkpoint.execution_block_hash.as_str())
            && snapshot.consensus.last_execution_state_root.as_deref()
                == Some(checkpoint.execution_state_root.as_str())
    } else {
        true
    }
}

fn proof_claims(response: &RebuildStatusResponse) -> RebuildProofClaims<'_> {
    RebuildProofClaims {
        schema_version: response.schema_version,
        observed_at_unix_ms: response.observed_at_unix_ms,
        node_id: response.proof.signer_id.as_str(),
        world_id: response
            .checkpoint
            .as_ref()
            .map(|checkpoint| checkpoint.world_id.as_str())
            .unwrap_or_default(),
        ok: response.ok,
        liveness: &response.liveness,
        readiness: &response.readiness,
        heights: &response.heights,
        network_head: &response.network_head,
        checkpoint: response.checkpoint.as_ref(),
        local_peer_id: response.local_peer_id.as_str(),
        connected_peers: response.connected_peers.as_slice(),
        connected_peer_count: response.connected_peer_count,
    }
}

fn proof_claim_bytes(response: &RebuildStatusResponse) -> Result<Vec<u8>, String> {
    serde_json::to_vec(&proof_claims(response))
        .map_err(|err| format!("serialize rebuild proof claims failed: {err}"))
}

fn sign_rebuild_proof(
    response: &RebuildStatusResponse,
    signer: &FeedbackSubmitSigner,
) -> Result<RebuildProofEnvelope, String> {
    let private_bytes = hex::decode(signer.private_key_hex.as_str())
        .map_err(|_| "rebuild proof signer private key is not valid hex".to_string())?;
    let private_array: [u8; 32] = private_bytes
        .try_into()
        .map_err(|_| "rebuild proof signer private key must be 32 bytes".to_string())?;
    let signing_key = SigningKey::from_bytes(&private_array);
    let public_key_hex = hex::encode(signing_key.verifying_key().to_bytes());
    if public_key_hex != signer.public_key_hex.to_ascii_lowercase() {
        return Err("rebuild proof signer public key does not match private key".to_string());
    }
    let claims = proof_claim_bytes(response)?;
    let digest = Sha256::digest(claims.as_slice());
    let signature = signing_key.sign(claims.as_slice());
    Ok(RebuildProofEnvelope {
        schema_version: REBUILD_PROOF_SCHEMA,
        signer_id: response.proof.signer_id.clone(),
        signer_public_key_hex: public_key_hex,
        signed_payload_sha256: hex::encode(digest),
        signature_hex: hex::encode(signature.to_bytes()),
    })
}

pub(super) fn verify_rebuild_proof(response: &RebuildStatusResponse) -> Result<(), String> {
    if response.proof.schema_version != REBUILD_PROOF_SCHEMA {
        return Err("unsupported rebuild proof schema".to_string());
    }
    let public_bytes = hex::decode(response.proof.signer_public_key_hex.as_str())
        .map_err(|_| "rebuild proof public key is not valid hex".to_string())?;
    let public_array: [u8; 32] = public_bytes
        .try_into()
        .map_err(|_| "rebuild proof public key must be 32 bytes".to_string())?;
    let signature_bytes = hex::decode(response.proof.signature_hex.as_str())
        .map_err(|_| "rebuild proof signature is not valid hex".to_string())?;
    let signature_array: [u8; 64] = signature_bytes
        .try_into()
        .map_err(|_| "rebuild proof signature must be 64 bytes".to_string())?;
    let claims = proof_claim_bytes(response)?;
    let digest = Sha256::digest(claims.as_slice());
    if response.proof.signed_payload_sha256 != hex::encode(digest) {
        return Err("rebuild proof payload digest mismatch".to_string());
    }
    VerifyingKey::from_bytes(&public_array)
        .map_err(|err| format!("rebuild proof public key is invalid: {err}"))?
        .verify(claims.as_slice(), &Signature::from_bytes(&signature_array))
        .map_err(|err| format!("rebuild proof signature verification failed: {err}"))
}

/// Verify a bounded proof file independently of the HTTP producer. The
/// expected signer values are deployment-truth inputs; they are never taken
/// from the proof itself.
pub(super) fn verify_rebuild_proof_file(
    path: &Path,
    trusted_signer_id: &str,
    trusted_public_key_hex: &str,
) -> Result<RebuildProofVerificationReceipt, String> {
    if trusted_signer_id.trim().is_empty() {
        return Err("trusted signer id cannot be empty".to_string());
    }
    let trusted_public_key_hex = trusted_public_key_hex.trim().to_ascii_lowercase();
    if trusted_public_key_hex.len() != 64
        || !trusted_public_key_hex
            .chars()
            .all(|ch| ch.is_ascii_hexdigit())
    {
        return Err("trusted signer public key must be 32-byte hex".to_string());
    }
    let metadata = fs::metadata(path).map_err(|err| {
        format!(
            "read rebuild proof metadata {} failed: {err}",
            path.display()
        )
    })?;
    if !metadata.is_file() {
        return Err(format!(
            "rebuild proof path {} is not a file",
            path.display()
        ));
    }
    if metadata.len() > MAX_REBUILD_PROOF_FILE_BYTES {
        return Err(format!(
            "rebuild proof file {} exceeds {} bytes",
            path.display(),
            MAX_REBUILD_PROOF_FILE_BYTES
        ));
    }
    let bytes = fs::read(path)
        .map_err(|err| format!("read rebuild proof {} failed: {err}", path.display()))?;
    if bytes.len() as u64 > MAX_REBUILD_PROOF_FILE_BYTES {
        return Err(format!(
            "rebuild proof file {} exceeds {} bytes",
            path.display(),
            MAX_REBUILD_PROOF_FILE_BYTES
        ));
    }
    let wire: RebuildStatusWire = serde_json::from_slice(bytes.as_slice())
        .map_err(|err| format!("parse rebuild proof {} failed: {err}", path.display()))?;
    let response = wire.into_response()?;
    if response.proof.signer_id != trusted_signer_id.trim() {
        return Err(format!(
            "trusted signer id mismatch: expected {}, got {}",
            trusted_signer_id.trim(),
            response.proof.signer_id
        ));
    }
    if response.proof.signer_public_key_hex != trusted_public_key_hex {
        return Err("trusted signer public key mismatch".to_string());
    }
    verify_rebuild_proof(&response)?;
    Ok(RebuildProofVerificationReceipt {
        schema_version: "oasis7.rebuild_proof_verification.v1",
        proof_schema_version: REBUILD_PROOF_SCHEMA,
        signer_id: response.proof.signer_id,
        signer_public_key_hex: response.proof.signer_public_key_hex,
        signed_payload_sha256: response.proof.signed_payload_sha256,
        local_peer_id: response.local_peer_id,
        proof_sha256: hex::encode(Sha256::digest(bytes.as_slice())),
        verified: true,
    })
}

pub(super) fn run_verify<'a>(mut args: impl Iterator<Item = &'a str>) -> Result<(), String> {
    let mut proof_path = None;
    let mut trusted_signer_id = None;
    let mut trusted_public_key_hex = None;
    while let Some(arg) = args.next() {
        match arg {
            "--proof" => proof_path = Some(required_verify_value(&mut args, "--proof")?),
            "--trusted-signer-id" => {
                trusted_signer_id = Some(required_verify_value(&mut args, "--trusted-signer-id")?)
            }
            "--trusted-signer-public-key-hex" => {
                trusted_public_key_hex = Some(required_verify_value(
                    &mut args,
                    "--trusted-signer-public-key-hex",
                )?)
            }
            "-h" | "--help" => return Err(verify_help()),
            _ => return Err(format!("unknown verify-rebuild-proof option: {arg}")),
        }
    }
    let proof_path = PathBuf::from(
        proof_path.ok_or_else(|| "verify-rebuild-proof requires --proof".to_string())?,
    );
    let trusted_signer_id = trusted_signer_id
        .ok_or_else(|| "verify-rebuild-proof requires --trusted-signer-id".to_string())?;
    let trusted_public_key_hex = trusted_public_key_hex.ok_or_else(|| {
        "verify-rebuild-proof requires --trusted-signer-public-key-hex".to_string()
    })?;
    let receipt = verify_rebuild_proof_file(
        proof_path.as_path(),
        trusted_signer_id.as_str(),
        trusted_public_key_hex.as_str(),
    )?;
    println!(
        "{}",
        serde_json::to_string(&receipt)
            .map_err(|err| format!("serialize rebuild proof verification receipt failed: {err}"))?
    );
    Ok(())
}

fn required_verify_value<'a>(
    args: &mut impl Iterator<Item = &'a str>,
    option: &str,
) -> Result<String, String> {
    args.next()
        .map(str::to_owned)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| format!("{option} requires a non-empty value"))
}

fn verify_help() -> String {
    "Usage: oasis7_chain_runtime verify-rebuild-proof --proof <path> --trusted-signer-id <id> --trusted-signer-public-key-hex <64-hex>".to_string()
}

impl RebuildStatusWire {
    fn into_response(self) -> Result<RebuildStatusResponse, String> {
        if self.schema_version != "oasis7.rebuild_status.v1" {
            return Err("unsupported rebuild status schema".to_string());
        }
        if self.proof.schema_version != REBUILD_PROOF_SCHEMA {
            return Err("unsupported rebuild proof schema".to_string());
        }
        if self.connected_peers.len() > MAX_CONNECTED_PEER_IDS {
            return Err("rebuild proof connected peer list exceeds bound".to_string());
        }
        let readiness_status = match self.readiness.status.as_str() {
            "ready" => "ready",
            "not_ready" => "not_ready",
            _ => return Err("rebuild proof readiness status is invalid".to_string()),
        };
        if self.ok != (readiness_status == "ready") {
            return Err("rebuild proof ok/readiness status mismatch".to_string());
        }
        if self.connected_peer_count < self.connected_peers.len() {
            return Err("rebuild proof connected peer count is below list length".to_string());
        }
        Ok(RebuildStatusResponse {
            schema_version: "oasis7.rebuild_status.v1",
            observed_at_unix_ms: self.observed_at_unix_ms,
            ok: self.ok,
            liveness: RebuildLiveness {
                running: self.liveness.running,
                last_error: self.liveness.last_error,
            },
            readiness: RebuildReadiness {
                status: readiness_status,
                failed_gates: self.readiness.failed_gates,
            },
            heights: RebuildHeights {
                committed_height: self.heights.committed_height,
                network_committed_height: self.heights.network_committed_height,
                last_execution_height: self.heights.last_execution_height,
            },
            network_head: RebuildNetworkHead {
                source: self.network_head.source,
                decision: self.network_head.decision,
                height: self.network_head.height,
                block_hash: self.network_head.block_hash,
                execution_block_hash: self.network_head.execution_block_hash,
                execution_state_root: self.network_head.execution_state_root,
                observed_peer_count: self.network_head.observed_peer_count,
                fresh_peer_count: self.network_head.fresh_peer_count,
            },
            checkpoint: self.checkpoint.map(|checkpoint| RebuildCheckpoint {
                schema_version: checkpoint.schema_version,
                checkpoint_id: checkpoint.checkpoint_id,
                world_id: checkpoint.world_id,
                height: checkpoint.height,
                execution_block_hash: checkpoint.execution_block_hash,
                execution_state_root: checkpoint.execution_state_root,
                manifest_hash: checkpoint.manifest_hash,
            }),
            local_peer_id: self.local_peer_id,
            connected_peers: self.connected_peers,
            connected_peer_count: self.connected_peer_count,
            proof: RebuildProofEnvelope {
                schema_version: REBUILD_PROOF_SCHEMA,
                signer_id: self.proof.signer_id,
                signer_public_key_hex: self.proof.signer_public_key_hex,
                signed_payload_sha256: self.proof.signed_payload_sha256,
                signature_hex: self.proof.signature_hex,
            },
        })
    }
}
