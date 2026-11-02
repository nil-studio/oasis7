//! Dedicated paid Source inspection. The first Native claim commits before
//! protected Current checks; history cannot recreate its continuation.
use super::*;
use oasis7_custody_core::execution_pending_source_inspect_v2::PendingSourceNativeRequestV2;
const SOURCE_KIND: &str = "custody_pending_source_inspect_v2";
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceClaim {
    api_version: u8,
    request: PendingSourceNativeRequestV2,
    configuration_digest: String,
}
pub(in super::super) struct ClaimedPendingSourceInspect {
    key: String,
    request: PendingSourceNativeRequestV2,
}
pub(in super::super) struct SourceInspectScope<'a> {
    pub config: &'a CustodyConfig,
    pub identity: &'a super::super::super::config::IdentityConfig,
    pub machine: &'a super::super::cleanup_config::MachineConfig,
    pub configuration_digest: &'a str,
    pub now: u64,
}
fn source_key(q: &PendingSourceNativeRequestV2) -> String {
    // The physical ID is already a complete closed master/leaf commitment.
    format!(
        "{SOURCE_KIND}:{}",
        q.leaf.claim.dispatch.binding.native_attempt_id
    )
}
pub(in super::super) fn claim_pending_source_inspect(
    tx: &mut IdentityTransaction<'_>,
    scope: &SourceInspectScope<'_>,
    request: PendingSourceNativeRequestV2,
) -> Result<ClaimedPendingSourceInspect, StoreError> {
    request.wire_bytes().map_err(|_| unavailable())?;
    tx.machine_storage_head().ok_or_else(unavailable)?;
    let pin = scope.machine.source_inspect_pin()?;
    let floor = admit_issuer_floor(tx, pin, &scope.machine.issuer_floor)?;
    request
        .leaf
        .verify(pin, &floor, &pin.native_audience_origin, scope.now)
        .map_err(|_| unavailable())?;
    let key = source_key(&request);
    if tx.get::<SourceClaim>(&key)?.is_some() {
        return Err(unavailable());
    }
    let b = &request.leaf.claim.dispatch.binding;
    claim_physical_attempt(
        tx,
        &b.issuer,
        &b.native_attempt_id,
        &request.leaf.digest().map_err(|_| unavailable())?,
        "execution-pending-source-native",
    )?;
    tx.create(
        &key,
        SOURCE_KIND,
        None,
        None,
        &SourceClaim {
            api_version: 2,
            request: request.clone(),
            configuration_digest: scope.configuration_digest.into(),
        },
    )?;
    Ok(ClaimedPendingSourceInspect { key, request })
}
pub(in super::super) fn current_pending_source_inspect(
    tx: &mut IdentityTransaction<'_>,
    scope: &SourceInspectScope<'_>,
    claimed: &ClaimedPendingSourceInspect,
) -> Result<(ManagedNativeSessionLifetimeV1, u64, ApprovalVerificationV1), StoreError> {
    let stored = tx
        .get::<SourceClaim>(&claimed.key)?
        .ok_or_else(unavailable)?
        .value;
    if stored.api_version != 2
        || stored.request != claimed.request
        || stored.configuration_digest != scope.configuration_digest
    {
        return Err(unavailable());
    }
    let pin = scope.machine.source_inspect_pin()?;
    let floor = admit_issuer_floor(tx, pin, &scope.machine.issuer_floor)?;
    claimed
        .request
        .leaf
        .verify(pin, &floor, &pin.native_audience_origin, scope.now)
        .map_err(|_| unavailable())?;
    let c = &claimed.request.leaf.claim.dispatch.binding;
    let index = tx
        .get::<ReferenceIndex>(&reference_key(&c.opaque_registration_ref))?
        .ok_or_else(unavailable)?
        .value;
    if index.api_version != 2 {
        return Err(unavailable());
    }
    let row = tx
        .get::<Registration>(&index.registration_key)?
        .ok_or_else(unavailable)?
        .value;
    let Phase::Active {
        registration,
        certificate,
        ack,
    } = &row.phase
    else {
        return Err(unavailable());
    };
    if row.api_version != 2
        || row.configuration_digest != scope.configuration_digest
        || record_key(&row.pending)? != index.registration_key
        || certificate.certificate.opaque_registration_ref != c.opaque_registration_ref
        || certificate.digest().map_err(|_| unavailable())? != c.active_certificate_digest
        || certificate.certificate.session_id != c.session_id
        || certificate.certificate.approved_input_digest != c.approved_input_digest
        || row.pending.certificate.participant_session != c.session
        || row
            .pending
            .certificate
            .native_lifetime
            .lifetime
            .status
            .receipt
            != c.native_binding
    {
        return Err(unavailable());
    }
    let active_floor =
        admit_issuer_floor(tx, &scope.machine.active_pin, &scope.machine.issuer_floor)?;
    certificate
        .verify_issuer(
            &row.pending,
            &scope.machine.active_pin,
            &active_floor,
            scope.now,
        )
        .map_err(|_| unavailable())?;
    let native_public = hex::encode(scope.config.key.verifying_key().to_bytes());
    // Registration/Pending are historical. Their original short dispatch
    // windows are not a second expiry for the independently Active session.
    registration
        .verify_native(
            &row.pending,
            &scope.config.key_id,
            &native_public,
            &scope.identity.namespace.key(),
            scope.identity.rp.identity_instance_epoch,
            registration.ack.issued_at.0,
        )
        .map_err(|_| unavailable())?;
    ack.verify_native(
        &row.pending,
        certificate,
        registration,
        &scope.config.key_id,
        &native_public,
    )
    .map_err(|_| unavailable())?;
    super::super::cleanup_consent::validate_saved_full_input(
        tx,
        scope.config,
        scope.identity,
        &row.pending.certificate.input,
        &row.pending.certificate.intent_digest,
    )?;
    let saved = tx
        .get::<ApprovalRecord>(&approval_key(
            &row.pending.certificate.input.approval.proof.approval_id,
        ))?
        .ok_or_else(unavailable)?;
    super::super::guarded_consent::require_guarded_consent_v2(
        tx,
        scope.config,
        scope.identity,
        &saved.value,
        super::super::guarded_consent::GuardedConsentUseV2::ExecutionPendingSourceInspect,
        Some(scope.configuration_digest),
    )?;
    let observed = observe(
        tx,
        &row.managed_storage_key,
        scope.config,
        scope.identity,
        &row.pending,
        &c.source_verify_nonce,
        scope.now,
    )?;
    if !same_security(&row.initial_lifetime, &observed)
        || observed.status.session_ref != c.native_session_ref
        || observed.status.generation != c.native_binding_generation
        || observed.status.receipt != c.native_binding
    {
        return Err(unavailable());
    }
    let input = &row.pending.certificate.input;
    let origin_request = ApprovalVerifyRequestV1 {
        approval: input.approval.clone(),
        proof_digest: input.approval.digest().map_err(|_| unavailable())?,
        expected_audience: input.request.context.audience.clone(),
        expected_intent_digest: row.pending.certificate.intent_digest.clone(),
        purpose: VerificationPurposeV1::InspectOrigin,
        verify_nonce: c.source_verify_nonce.clone(),
    };
    if origin_request != claimed.request.request {
        return Err(unavailable());
    }
    super::super::origin_observation::validate_verification_request(
        scope.config,
        scope.identity,
        &origin_request,
    )?;
    let origin = super::super::origin_observation::observe_origin_current(
        tx,
        scope.config,
        scope.identity,
        &origin_request,
        scope.now,
    )?;
    if origin.origin_state != OriginStateV1::Active
        || origin.manager_security_version != input.approval.proof.manager_security_version
        || origin.manager_policy_version != input.approval.proof.manager_policy_version
        || origin.origin_credential_version != input.approval.proof.origin_credential_version
        || origin.credential_version != input.approval.proof.passkey_credential_version
        || scope.now >= origin.source_session_expires_at.0
        || scope.now >= origin.source_session_idle_expires_at.0
    {
        return Err(unavailable());
    }
    tx.machine_storage_head().ok_or_else(unavailable)?;
    let dispatch_until = c
        .expires_at
        .0
        .min(claimed.request.leaf.claim.expires_at.0)
        .min(observed.status.expires_at.0)
        .min(observed.session_expires_at.0)
        .min(observed.session_idle_expires_at.0)
        .min(origin.source_session_expires_at.0)
        .min(origin.source_session_idle_expires_at.0);
    if scope.now >= dispatch_until {
        return Err(unavailable());
    }
    Ok((observed, dispatch_until, origin))
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceReply {
    api_version: u8,
    request: PendingSourceNativeRequestV2,
    configuration_digest: String,
    archived_at: DecimalU64,
    reply:
        oasis7_custody_core::execution_pending_source_result_v2::SignedPendingSourceInspectResultV2,
}
/// The original first-commit token is consumed. No replay/cold getter returns
/// either this token or a fresh result; the archived reply is historical only.
pub(in super::super) fn finish_pending_source_inspect(
    tx: &mut IdentityTransaction<'_>,
    scope: &SourceInspectScope<'_>,
    claimed: ClaimedPendingSourceInspect,
) -> Result<serde_json::Value, StoreError> {
    use ed25519_dalek::Signer;
    use oasis7_custody_core::execution_pending_source_result_v2::*;
    let key = format!("{}:reply", claimed.key);
    if tx.get::<SourceReply>(&key)?.is_some() {
        return Err(unavailable());
    }
    let (before, until, fact) = current_pending_source_inspect(tx, scope, &claimed)?;
    let origin_expires = fact.source_session_expires_at.0;
    let origin_idle_expires = fact.source_session_idle_expires_at.0;
    let completed_at = crate::hosted_identity::now_ms();
    if completed_at < scope.now || completed_at >= until {
        return Err(unavailable());
    }

    let inner = SignedApprovalVerificationV1 {
        signature: URL_SAFE_NO_PAD.encode(
            scope
                .config
                .key
                .sign(&fact.signing_bytes().map_err(|_| unavailable())?)
                .to_bytes(),
        ),
        verification: fact,
    };
    let result = PendingSourceInspectResultV2 {
        api_version: 2,
        key_id: scope.config.key_id.clone(),
        dispatch_digest: claimed
            .request
            .leaf
            .claim
            .dispatch
            .digest()
            .map_err(|_| unavailable())?,
        native_leaf_digest: claimed.request.leaf.digest().map_err(|_| unavailable())?,
        native_request_digest: claimed.request.digest().map_err(|_| unavailable())?,
        inner,
        completed_at: DecimalU64(completed_at),
    };
    result
        .check_request(&claimed.request)
        .map_err(|_| unavailable())?;
    let reply = SignedPendingSourceInspectResultV2 {
        signature_hex: hex::encode(
            scope
                .config
                .key
                .sign(
                    &SignedPendingSourceInspectResultV2::signing_bytes(&result)
                        .map_err(|_| unavailable())?,
                )
                .to_bytes(),
        ),
        result,
    };
    reply
        .verify_for(
            &claimed.request,
            &scope.config.key_id,
            &hex::encode(scope.config.key.verifying_key().to_bytes()),
        )
        .map_err(|_| unavailable())?;
    reply.wire_bytes().map_err(|_| unavailable())?;
    let response = serde_json::to_value(&reply).map_err(|_| unavailable())?;
    // Actual clock after signing/codec work, without rewriting the signed time.
    let final_now = crate::hosted_identity::now_ms();
    if final_now < completed_at || final_now >= until {
        return Err(unavailable());
    }
    let final_scope = SourceInspectScope {
        config: scope.config,
        identity: scope.identity,
        machine: scope.machine,
        configuration_digest: scope.configuration_digest,
        now: final_now,
    };
    let (after, final_until, after_origin) =
        current_pending_source_inspect(tx, &final_scope, &claimed)?;
    if !same_security(&before, &after)
        || final_now >= final_until
        || after_origin.source_session_expires_at.0 < origin_expires
        || after_origin.source_session_idle_expires_at.0 < origin_idle_expires
    {
        return Err(unavailable());
    }
    tx.create(
        &key,
        "custody_pending_source_inspect_reply_v2",
        None,
        None,
        &SourceReply {
            api_version: 2,
            request: claimed.request,
            configuration_digest: scope.configuration_digest.into(),
            archived_at: DecimalU64(final_now),
            reply,
        },
    )?;
    Ok(response)
}
