//! Protected immutable read-observation history. Never canonical committed,
//! fresh Current, prepared-ready, a retry token, or authoritative usage.
use super::*;
use oasis7_client_api::world_service::task_context_projection::{
    public_projection_digest_v2, verify_prepared_delivery_v2,
};
use oasis7_custody_core::native_execution_prepare_v2::{
    NativeExecutionPrepareGatedReplyV2, prepared_response_digest_v2,
};
const REPLY_KIND: &str = "custody_execution_prepare_reply_observation_v2";
const MAX_ARCHIVE_BYTES: usize = 64 * 1024;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PreparedReplyObservation {
    api_version: u8,
    // Commits exact request, authorization, SID-storage identity and config.
    claim_digest: String,
    configuration_digest: String,
    archived_at: DecimalU64,
    reply: NativeExecutionPrepareGatedReplyV2,
}
fn reply_key(request: &NativeExecutionPrepareRequestV2) -> Result<String, StoreError> {
    Ok(format!("{}:read-observation", execution_key(request)?))
}
fn bounded<T: Serialize>(value: &T) -> Result<Vec<u8>, StoreError> {
    let bytes = serde_json::to_vec(value).map_err(|_| unavailable())?;
    if bytes.is_empty() || bytes.len() > MAX_ARCHIVE_BYTES {
        return Err(unavailable());
    }
    Ok(bytes)
}
fn archive_bytes(observation: &PreparedReplyObservation) -> Result<Vec<u8>, StoreError> {
    // Metadata is fixed and bounded; the already persisted claim owns the full
    // original. Do not subtract wrapper overhead from a legal 64 KiB reply.
    if observation.api_version != 2
        || observation.claim_digest.len() != 64
        || observation.configuration_digest.len() != 64
        || !observation
            .claim_digest
            .bytes()
            .chain(observation.configuration_digest.bytes())
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(unavailable());
    }
    bounded(&observation.reply)?;
    #[derive(Serialize)]
    struct Metadata<'a> {
        api_version: u8,
        claim_digest: &'a str,
        configuration_digest: &'a str,
        archived_at: DecimalU64,
        reply: (),
    }
    let metadata = serde_json::to_vec(&Metadata {
        api_version: observation.api_version,
        claim_digest: &observation.claim_digest,
        configuration_digest: &observation.configuration_digest,
        archived_at: observation.archived_at,
        reply: (),
    })
    .map_err(|_| unavailable())?;
    let overhead = metadata
        .len()
        .checked_sub(b"null".len())
        .ok_or_else(unavailable)?;
    let maximum = MAX_ARCHIVE_BYTES
        .checked_add(overhead)
        .ok_or_else(unavailable)?;
    let bytes = serde_json::to_vec(observation).map_err(|_| unavailable())?;
    if bytes.len() > maximum {
        return Err(unavailable());
    }
    Ok(bytes)
}

fn verify_reply(
    tx: &mut IdentityTransaction<'_>,
    scope: &ExecutionScope<'_>,
    stored: &ExecutionClaim,
    reply: &NativeExecutionPrepareGatedReplyV2,
) -> Result<(), StoreError> {
    use oasis7_custody_core::approval::bytes_digest;
    let q = &stored.request;
    let receipt = &reply.reply.physical_receipt;
    reply.wire_bytes().map_err(|_| unavailable())?;
    bounded(reply)?;
    // The configured World and the original Native issuer are independently
    // verified. A signed projection alone cannot stand for the private body.
    verify_prepared_delivery_v2(
        &q.original.request,
        &reply.reply.world_response,
        &q.proof.certificate.world.service_public_key,
    )
    .map_err(|_| unavailable())?;
    let digest =
        prepared_response_digest_v2(&reply.reply.world_response).map_err(|_| unavailable())?;
    let public_digest = public_projection_digest_v2(&reply.reply.world_response.payload.projection)
        .map_err(|_| unavailable())?;
    let native_key = hex::encode(scope.config.key.verifying_key().to_bytes());
    receipt
        .verify_native(q, &scope.config.key_id, &native_key, &digest)
        .map_err(|_| unavailable())?;
    if receipt.receipt.completed_at.0 != scope.now
        || receipt.receipt.public_projection_digest != public_digest
    {
        return Err(unavailable());
    }
    let pin = scope.machine.execution_dispatch_pin()?;
    let floor = admit_issuer_floor(tx, pin, &scope.machine.issuer_floor)?;
    let mut previous = &stored.authorization;
    let current_body = serde_json::to_vec(&q.original.access.read).map_err(|_| unavailable())?;
    let prepare_body = serde_json::to_vec(&q.original.request).map_err(|_| unavailable())?;
    for ((claim, edge), body) in reply
        .world_claims
        .iter()
        .zip([
            ExecutionPreparePhysicalEdgeV2::NativeToWorldCurrent,
            ExecutionPreparePhysicalEdgeV2::NativeToWorldPrepare,
        ])
        .zip([current_body, prepare_body])
    {
        claim
            .verify_issuer(q, pin, &floor, scope.now)
            .map_err(|_| unavailable())?;
        if claim.claim.leaf.edge != edge
            || claim.claim.body_digest != bytes_digest(&body)
            || claim.claim.reservation_ordinal != previous.claim.reservation_ordinal
            || claim.claim.claim_ordinal.0 <= previous.claim.claim_ordinal.0
            || claim.claim.issued_at.0 > scope.now
        {
            return Err(unavailable());
        }
        previous = claim;
    }
    Ok(())
}

/// Called only with a non-reconstructible token from the original first claim.
/// The transaction commits Current verification and immutable history together;
/// on any failure it writes no reply or ready/admission/usage state.
pub(in super::super::super) fn finish_execution_prepare(
    tx: &mut IdentityTransaction<'_>,
    scope: &ExecutionScope<'_>,
    observed: ObservedExecutionPrepare,
    reply: NativeExecutionPrepareGatedReplyV2,
) -> Result<serde_json::Value, StoreError> {
    observed.require_dispatch_window(scope.now)?;
    let q = observed.request();
    let stored = tx
        .get::<ExecutionClaim>(&observed.claimed.key)?
        .ok_or_else(unavailable)?
        .value;
    let key = reply_key(q)?;
    if tx.get::<PreparedReplyObservation>(&key)?.is_some() {
        return Err(unavailable());
    }
    verify_reply(tx, scope, &stored, &reply)?;
    let claim_digest = blake3::hash(&serde_json::to_vec(&stored).map_err(|_| unavailable())?)
        .to_hex()
        .to_string();
    let mut observation = PreparedReplyObservation {
        api_version: 2,
        claim_digest,
        configuration_digest: scope.configuration_digest.into(),
        archived_at: DecimalU64(scope.now),
        reply,
    };
    archive_bytes(&observation)?;
    // Serialize before final Current: no fallible codec work after archive.
    let response = serde_json::to_value(&observation.reply).map_err(|_| unavailable())?;
    // All proof/codec work above can consume time. Re-read the actual Native
    // clock before the final local Current check, without retimestamping the
    // signed World/Native receipt or extending its original dispatch window.
    let final_now = super::super::super::super::now_ms();
    if final_now < scope.now {
        return Err(unavailable());
    }
    observed.require_dispatch_window(final_now)?;
    let final_scope = ExecutionScope {
        config: scope.config,
        identity: scope.identity,
        machine: scope.machine,
        storage_key: scope.storage_key,
        configuration_digest: scope.configuration_digest,
        now: final_now,
    };
    let (after, _) = current_execution_identity(tx, &final_scope, &observed.claimed)?;
    if !same_security(&observed.lifetime, &after) {
        return Err(unavailable());
    }
    // Only local bounded metadata changes after the final check. Historical
    // completed_at remains the original receipt's signed time.
    observation.archived_at = DecimalU64(final_now);
    tx.create(&key, REPLY_KIND, None, None, &observation)?;
    Ok(response)
}

#[cfg(test)]
#[path = "custody_execution_prepare_reply_archive_tests_v2.rs"]
mod tests;
