//! Direct V3 Native-local facts after the separately prepaid Actor and Native proofs.
//! This consumer never reads Cloud private generations or signs World originals.
use super::*;
use oasis7_custody_core::native_binding::SignedManagedNativeSessionLifetimeV1;
/// Once is durably committed before any fallible local current observation.
/// No Clone/serde/debug: an HTTP replay cannot reconstruct this owned token.
pub(in super::super) struct PreparedTermDirectObservationV3 {
    request: NativeTermDirectFrameV3,
}
pub(in super::super) fn claim_term_direct_registered_v3(
    tx: &mut IdentityTransaction<'_>,
    config: &CustodyConfig,
    identity: &super::super::super::config::IdentityConfig,
    machine: &super::super::cleanup_config::MachineConfig,
    configuration_digest: &str,
    request: NativeTermDirectFrameV3,
    now: u64,
) -> Result<PreparedTermDirectObservationV3, StoreError> {
    let body = &request.body;
    body.validate().map_err(|_| unavailable())?;
    let reference = &request.actor.binding.opaque_registration_ref;
    let index = tx
        .get::<ReferenceIndex>(&reference_key(reference))?
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
        || row.configuration_digest != configuration_digest
        || record_key(&row.pending)? != index.registration_key
        || ack.ack.opaque_registration_ref != *reference
    {
        return Err(unavailable());
    }
    let actor_pin = machine
        .term_direct_actor_pin
        .as_ref()
        .ok_or_else(unavailable)?;
    let native_pin = machine
        .term_direct_native_pin
        .as_ref()
        .ok_or_else(unavailable)?;
    if actor_pin.public_key != native_pin.public_key {
        return Err(unavailable());
    }
    let floor = admit_issuer_floor(tx, native_pin, &machine.issuer_floor)?;
    // Validate the exact stored ceremony and its current configured lineage.
    super::super::cleanup_consent::validate_saved_full_input(
        tx,
        config,
        identity,
        &row.pending.certificate.input,
        &row.pending.certificate.intent_digest,
    )?;
    let saved = tx
        .get::<super::super::ApprovalRecord>(&super::super::approval_key(
            &row.pending.certificate.input.approval.proof.approval_id,
        ))?
        .ok_or_else(unavailable)?
        .value;
    super::super::guarded_consent::require_guarded_consent_v2(
        tx,
        config,
        identity,
        &saved,
        super::super::guarded_consent::GuardedConsentUseV2::TermWideCleanup,
        Some(configuration_digest),
    )?;
    certificate
        .verify_issuer(&row.pending, &machine.active_pin, &floor, now)
        .map_err(|_| unavailable())?;
    request
        .actor
        .binding
        .validate_relative(&request.body, &row.pending, certificate, registration)
        .map_err(|_| unavailable())?;
    request
        .verify_at_native_recipient(actor_pin, native_pin, &floor, now)
        .map_err(|_| unavailable())?;
    claim_physical_attempt(
        tx,
        &request.actor.binding.issuer,
        &request.actor.binding.native_attempt_id,
        &request.digest().map_err(|_| unavailable())?,
        "term_direct_current_native_v3",
    )?;
    Ok(PreparedTermDirectObservationV3 { request })
}

pub(in super::super) fn observe_term_direct_registered_v3(
    tx: &mut IdentityTransaction<'_>,
    config: &CustodyConfig,
    identity: &super::super::super::config::IdentityConfig,
    machine: &super::super::cleanup_config::MachineConfig,
    configuration_digest: &str,
    prepared: PreparedTermDirectObservationV3,
    now: u64,
) -> Result<SignedNativeTermDirectFactV3, StoreError> {
    let request = prepared.request;
    let body = &request.body;
    body.validate().map_err(|_| unavailable())?;
    let reference = &request.actor.binding.opaque_registration_ref;
    let index = tx
        .get::<ReferenceIndex>(&reference_key(reference))?
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
        || row.configuration_digest != configuration_digest
        || record_key(&row.pending)? != index.registration_key
        || ack.ack.opaque_registration_ref != *reference
    {
        return Err(unavailable());
    }
    let actor_pin = machine
        .term_direct_actor_pin
        .as_ref()
        .ok_or_else(unavailable)?;
    let native_pin = machine
        .term_direct_native_pin
        .as_ref()
        .ok_or_else(unavailable)?;
    if actor_pin.public_key != native_pin.public_key {
        return Err(unavailable());
    }
    let floor = admit_issuer_floor(tx, native_pin, &machine.issuer_floor)?;
    // Validate the exact stored ceremony and its current configured lineage.
    super::super::cleanup_consent::validate_saved_full_input(
        tx,
        config,
        identity,
        &row.pending.certificate.input,
        &row.pending.certificate.intent_digest,
    )?;
    let saved = tx
        .get::<super::super::ApprovalRecord>(&super::super::approval_key(
            &row.pending.certificate.input.approval.proof.approval_id,
        ))?
        .ok_or_else(unavailable)?
        .value;
    super::super::guarded_consent::require_guarded_consent_v2(
        tx,
        config,
        identity,
        &saved,
        super::super::guarded_consent::GuardedConsentUseV2::TermWideCleanup,
        Some(configuration_digest),
    )?;
    certificate
        .verify_issuer(&row.pending, &machine.active_pin, &floor, now)
        .map_err(|_| unavailable())?;
    request
        .actor
        .binding
        .validate_relative(&request.body, &row.pending, certificate, registration)
        .map_err(|_| unavailable())?;
    request
        .verify_at_native_recipient(actor_pin, native_pin, &floor, now)
        .map_err(|_| unavailable())?;
    let observed = observe(
        tx,
        &row.managed_storage_key,
        config,
        identity,
        &row.pending,
        &body.lifetime_nonce,
        now,
    )?;
    if !same_security(&row.initial_lifetime, &observed) {
        return Err(unavailable());
    }
    let lifetime = SignedManagedNativeSessionLifetimeV1 {
        signature: URL_SAFE_NO_PAD.encode(
            config
                .key
                .sign(&observed.signing_bytes().map_err(|_| unavailable())?)
                .to_bytes(),
        ),
        key_id: config.key_id.clone(),
        lifetime: observed,
    };
    let input = &row.pending.certificate.input;
    let origin_request = ApprovalVerifyRequestV1 {
        approval: input.approval.clone(),
        proof_digest: input.approval.digest().map_err(|_| unavailable())?,
        expected_audience: input.request.context.audience.clone(),
        expected_intent_digest: row.pending.certificate.intent_digest.clone(),
        purpose: VerificationPurposeV1::InspectOrigin,
        verify_nonce: body.origin_nonce.clone(),
    };
    super::super::origin_observation::validate_verification_request(
        config,
        identity,
        &origin_request,
    )?;
    let origin = super::super::origin_observation::observe_origin_current(
        tx,
        config,
        identity,
        &origin_request,
        now,
    )?;
    let origin = SignedApprovalVerificationV1 {
        signature: URL_SAFE_NO_PAD.encode(
            config
                .key
                .sign(&origin.signing_bytes().map_err(|_| unavailable())?)
                .to_bytes(),
        ),
        verification: origin,
    };
    let expires = request
        .actor
        .binding
        .expires_at
        .0
        .min(lifetime.lifetime.status.expires_at.0)
        .min(lifetime.lifetime.session_expires_at.0)
        .min(lifetime.lifetime.session_idle_expires_at.0)
        .min(origin.verification.source_session_expires_at.0)
        .min(origin.verification.source_session_idle_expires_at.0);
    let fact = NativeTermCurrentFactV2 {
        api_version: 2,
        native_key_id: config.key_id.clone(),
        identity_namespace: identity.namespace.key(),
        identity_epoch: DecimalU64(identity.rp.identity_instance_epoch),
        custody_account_id: input.request.context.custody_account_id.clone(),
        session_id: body.session_id.clone(),
        approved_input_digest: row.pending.certificate.approved_input_digest.clone(),
        active_certificate_digest: certificate.digest().map_err(|_| unavailable())?,
        opaque_registration_ref: reference.clone(),
        owned_invocation_id: body.owned_invocation_id.clone(),
        stop_operation_id: body.stop_operation_id.clone(),
        close_operation_id: body.close_operation_id.clone(),
        source_operation_id: body.source_operation_id.clone(),
        native_transport_attempt_id: request.actor.binding.native_attempt_id.clone(),
        native_request_digest: request.digest().map_err(|_| unavailable())?,
        nonce: body.nonce.clone(),
        origin_nonce: body.origin_nonce.clone(),
        lifetime_nonce: body.lifetime_nonce.clone(),
        storage_head: tx.machine_storage_head().ok_or_else(unavailable)?.clone(),
        registry_cursor: ack.ack.registry_cursor,
        lifetime,
        origin,
        observation_started_at: request.actor.binding.observation_started_at,
        issued_at: DecimalU64(now),
        expires_at: DecimalU64(expires),
    };
    let fact = NativeTermDirectFactV3 {
        api_version: 3,
        frame_digest: request.digest().map_err(|_| unavailable())?,
        local: fact,
    };
    fact.validate_frame(&request).map_err(|_| unavailable())?;
    fact.validate_approval(&row.pending, certificate, registration)
        .map_err(|_| unavailable())?;
    let signature_hex = hex::encode(
        config
            .key
            .sign(&fact.signing_bytes().map_err(|_| unavailable())?)
            .to_bytes(),
    );
    Ok(SignedNativeTermDirectFactV3 {
        fact,
        signature_hex,
    })
}
