//! Permanent paid HTTP claims precede fallible managed-current checks.
//! Tokens are process-local and consumed once while the Native mutex is owned.
use super::*;
pub(in super::super) struct PaidRegistration {
    pub(super) request: NativeReadAndRegisterRequestV2,
    pub(super) pending: SignedAccountConsumedPendingV2,
}
pub(in super::super) struct PaidActivation {
    pub(super) request: NativeActivateRequestV2,
}
pub(in super::super) fn claim_registration_native(
    tx: &mut IdentityTransaction<'_>,
    config: &CustodyConfig,
    identity: &super::super::super::config::IdentityConfig,
    request: NativeReadAndRegisterRequestV2,
    issuer: &AccountMachineIssuerPinV2,
    floor: &AccountMachineIssuerFloorV2,
    now: u64,
) -> Result<PaidRegistration, StoreError> {
    let admitted = admit_issuer_floor(tx, issuer, floor)?;
    let pending = super::super::cleanup_consent::expand_saved_pending_reference(
        tx,
        config,
        identity,
        &request.pending,
        issuer,
        &admitted,
        now,
    )?;
    request.validate().map_err(|_| unavailable())?;
    tx.machine_storage_head().ok_or_else(unavailable)?;
    if tx.get::<Registration>(&record_key(&pending)?)?.is_some() {
        return Err(unavailable());
    }
    claim_physical_attempt(
        tx,
        &pending.certificate.issuer,
        &pending
            .certificate
            .preflight_read_claim
            .native_transport_attempt_id,
        &pending.digest().map_err(|_| unavailable())?,
        "registration_native",
    )?;
    Ok(PaidRegistration { request, pending })
}
pub(in super::super) fn claim_activation_native(
    tx: &mut IdentityTransaction<'_>,
    configuration_digest: &str,
    request: NativeActivateRequestV2,
    issuer: &AccountMachineIssuerPinV2,
    floor: &AccountMachineIssuerFloorV2,
    now: u64,
) -> Result<PaidActivation, StoreError> {
    request.validate_shape().map_err(|_| unavailable())?;
    let index = tx
        .get::<ReferenceIndex>(&reference_key(&request.opaque_registration_ref))?
        .ok_or_else(unavailable)?
        .value;
    let row = tx
        .get::<Registration>(&index.registration_key)?
        .ok_or_else(unavailable)?
        .value;
    let Phase::Registered(registration) = &row.phase else {
        return Err(unavailable());
    };
    if index.api_version != 2
        || row.api_version != 2
        || row.configuration_digest != configuration_digest
        || record_key(&row.pending)? != index.registration_key
        || registration.ack.opaque_registration_ref != request.opaque_registration_ref
    {
        return Err(unavailable());
    }
    let admitted = admit_issuer_floor(tx, issuer, floor)?;
    request
        .active
        .verify_issuer(&row.pending, issuer, &admitted, now)
        .map_err(|_| unavailable())?;
    request
        .active
        .validate_registration(&row.pending, registration)
        .map_err(|_| unavailable())?;
    claim_physical_attempt(
        tx,
        &request.active.certificate.issuer,
        &request
            .active
            .certificate
            .activation_transport
            .native_transport_attempt_id,
        &request.active.digest().map_err(|_| unavailable())?,
        "activation_native",
    )?;
    Ok(PaidActivation { request })
}
