//! Durable registration state. A claimed read is never replayed or cached as
//! another successful HTTP; its original outcome may remain unknown.
use super::*;
use oasis7_custody_core::native_binding::ManagedNativeSessionLifetimeV1;
use oasis7_custody_core::native_cleanup_v2::*;
#[path = "custody_cleanup_v2_registry_journal.rs"]
mod journal;
use journal::{admit_issuer_floor, claim_physical_attempt};
#[path = "custody_cleanup_v2_registry_current.rs"]
mod current;
pub(super) use current::{claim_current_registered, observe_current_registered};
const KIND: &str = "custody_cleanup_registration_v2";
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReferenceIndex {
    api_version: u8,
    registration_key: String,
}
fn reference_key(reference: &str) -> String {
    format!(
        "custody_cleanup_reference_v2:{}",
        blake3::hash(reference.as_bytes()).to_hex()
    )
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Registration {
    api_version: u8,
    pending: SignedAccountConsumedPendingV2,
    managed_storage_key: String,
    configuration_digest: String,
    initial_lifetime: ManagedNativeSessionLifetimeV1,
    request: NativeReadAndRegisterRequestV2,
    phase: Phase,
}
#[derive(Serialize, Deserialize)]
#[serde(tag = "type", content = "value", deny_unknown_fields)]
enum Phase {
    ReadClaimed,
    Registered(Box<SignedNativeRegistrationAckV2>),
    Active {
        registration: Box<SignedNativeRegistrationAckV2>,
        certificate: Box<SignedAccountActiveThreeRootsV2>,
        ack: Box<SignedNativeActivationAckV2>,
    },
}
/// Private owned token, deliberately neither Serialize nor Clone nor Debug.
/// It authorizes only completion of the already claimed exact World read.
pub(super) struct PreparedRegistration {
    key: String,
    pending_digest: String,
    request: NativeReadAndRegisterRequestV2,
}
impl PreparedRegistration {
    pub(super) fn owner_read(
        &self,
    ) -> &oasis7_client_api::world_service::participant_discovery::ParticipantOwnerViewRequestV1
    {
        &self.request.owner_read
    }
}
fn unavailable() -> StoreError {
    StoreError::Rejected("cleanup_registration_unavailable".into())
}
fn record_key(pending: &SignedAccountConsumedPendingV2) -> Result<String, StoreError> {
    Ok(format!(
        "{KIND}:{}",
        pending.digest().map_err(|_| unavailable())?
    ))
}
fn same_security(a: &ManagedNativeSessionLifetimeV1, b: &ManagedNativeSessionLifetimeV1) -> bool {
    let a_status = &a.status;
    let b_status = &b.status;
    a_status.receipt == b_status.receipt
        && a_status.service_epoch == b_status.service_epoch
        && a_status.native_account_security_epoch == b_status.native_account_security_epoch
        && a_status.native_credential_version == b_status.native_credential_version
        && a_status.session_ref == b_status.session_ref
        && a_status.generation == b_status.generation
        && a_status.active == b_status.active
        && a_status.frozen == b_status.frozen
        && a_status.pending == b_status.pending
        && a.session_expires_at == b.session_expires_at
        && a.session_idle_expires_at == b.session_idle_expires_at
}
fn observe(
    tx: &mut IdentityTransaction<'_>,
    storage_key: &str,
    config: &CustodyConfig,
    identity: &super::super::config::IdentityConfig,
    pending: &SignedAccountConsumedPendingV2,
    nonce: &str,
    now: u64,
) -> Result<ManagedNativeSessionLifetimeV1, StoreError> {
    let observed =
        managed_binding::observe_managed_lifetime(tx, storage_key, config, identity, nonce, now)?;
    observed.validate().map_err(|_| unavailable())?;
    let expected = &pending.certificate.native_lifetime.lifetime;
    if !observed.status.active
        || observed.status.frozen
        || observed.status.pending
        || !same_security(expected, &observed)
        || observed.status.session_ref != pending.certificate.participant_session.session_ref
    {
        return Err(unavailable());
    }
    Ok(observed)
}
/// Caller is the browser-authenticated explicit ReadAndRegister adapter. Its
/// storage key must come from the existing Cookie/Origin/CSRF session wrapper.
/// All checks and the permanent once row commit before the owned World send.
pub(super) fn claim(
    tx: &mut IdentityTransaction<'_>,
    config: &CustodyConfig,
    identity: &super::super::config::IdentityConfig,
    storage_key: &str,
    configuration_digest: &str,
    paid: PaidRegistration,
    issuer: &AccountMachineIssuerPinV2,
    floor: &AccountMachineIssuerFloorV2,
    now: u64,
) -> Result<PreparedRegistration, StoreError> {
    let request = paid.request;
    let admitted_floor = admit_issuer_floor(tx, issuer, floor)?;
    let pending = super::cleanup_consent::expand_saved_pending_reference(
        tx,
        config,
        identity,
        &request.pending,
        issuer,
        &admitted_floor,
        now,
    )?;
    request
        .validate_relative(&pending)
        .map_err(|_| unavailable())?;
    let approval = tx
        .get::<ApprovalRecord>(&approval_key(
            &pending.certificate.input.approval.proof.approval_id,
        ))?
        .ok_or_else(unavailable)?;
    super::guarded_consent::require_guarded_consent_v2(
        tx,
        config,
        identity,
        &approval.value,
        super::guarded_consent::GuardedConsentUseV2::TermWideCleanup,
        Some(configuration_digest),
    )?;
    if pending != paid.pending {
        return Err(unavailable());
    }
    if configuration_digest.len() != 64
        || !configuration_digest.bytes().all(|b| b.is_ascii_hexdigit())
    {
        return Err(unavailable());
    }
    let lifetime = observe(
        tx,
        storage_key,
        config,
        identity,
        &pending,
        &request.registration_nonce,
        now,
    )?;
    // A configured external journal must already be participating in this
    // transaction (including the explicit first-registration initializer).
    tx.machine_storage_head().ok_or_else(unavailable)?;
    let key = record_key(&pending)?;
    if tx.get::<Registration>(&key)?.is_some() {
        return Err(unavailable());
    }
    let pending_digest = pending.digest().map_err(|_| unavailable())?;
    let ports = &pending.certificate.preflight_read_claim;
    claim_physical_attempt(
        tx,
        &pending.certificate.issuer,
        &ports.world_transport_attempt_id,
        &pending_digest,
        "registration_world",
    )?;
    let native_account_id = lifetime.status.receipt.native_account_id.clone();
    tx.create(
        &key,
        KIND,
        Some(&native_account_id),
        None,
        &Registration {
            api_version: 2,
            pending,
            managed_storage_key: storage_key.into(),
            configuration_digest: configuration_digest.into(),
            initial_lifetime: lifetime,
            request: request.clone(),
            phase: Phase::ReadClaimed,
        },
    )?;
    Ok(PreparedRegistration {
        key,
        pending_digest,
        request,
    })
}
/// Must run after the single owned World send, under the re-acquired Native
/// transaction and its current configuration. It never sends another request.
pub(super) fn finish(
    tx: &mut IdentityTransaction<'_>,
    config: &CustodyConfig,
    identity: &super::super::config::IdentityConfig,
    configuration_digest: &str,
    prepared: PreparedRegistration,
    response: oasis7_client_api::world_service::SignedServiceResponse<
        oasis7_client_api::world_service::participant_discovery::OwnerSetupObservationV1,
    >,
    now: u64,
) -> Result<NativeReadAndRegisterReplyV2, StoreError> {
    let saved = tx
        .get::<Registration>(&prepared.key)?
        .ok_or_else(unavailable)?;
    let row = saved.value;
    if row.api_version != 2
        || !matches!(row.phase, Phase::ReadClaimed)
        || row.configuration_digest != configuration_digest
        || row.request != prepared.request
        || row.pending.digest().map_err(|_| unavailable())? != prepared.pending_digest
        || now >= row.pending.certificate.expires_at.0
    {
        return Err(unavailable());
    }
    let current = observe(
        tx,
        &row.managed_storage_key,
        config,
        identity,
        &row.pending,
        &row.request.registration_nonce,
        now,
    )?;
    if !same_security(&row.initial_lifetime, &current) {
        return Err(unavailable());
    }
    let ack = NativeRegistrationAckV2 {
        api_version: 2,
        native_key_id: config.key_id.clone(),
        identity_namespace: identity.namespace.key(),
        identity_epoch: DecimalU64(identity.rp.identity_instance_epoch),
        session_id: row.pending.certificate.input.operation_id.clone(),
        approved_input_digest: row.pending.certificate.approved_input_digest.clone(),
        pending_certificate_digest: prepared.pending_digest,
        owner_read_response_digest: registration_response_digest(&response)
            .map_err(|_| unavailable())?,
        opaque_registration_ref: random_token(),
        managed_session_ref: current.status.session_ref.clone(),
        managed_binding_generation: current.status.generation,
        registration_nonce: row.request.registration_nonce.clone(),
        storage_head: tx.machine_storage_head().ok_or_else(unavailable)?.clone(),
        registry_cursor: DecimalU64(1),
        issued_at: DecimalU64(now),
        expires_at: row.pending.certificate.expires_at,
    };
    let signature_hex = hex::encode(
        config
            .key
            .sign(&ack.signing_bytes(&row.pending).map_err(|_| unavailable())?)
            .to_bytes(),
    );
    let signed = SignedNativeRegistrationAckV2 { ack, signature_hex };
    let reply = NativeReadAndRegisterReplyV2 {
        api_version: 2,
        world_response: response,
        registration_ack: signed.clone(),
    };
    reply
        .validate_relative(&row.request, &row.pending)
        .map_err(|_| unavailable())?;
    reply
        .verify_world(&row.request, &row.pending)
        .map_err(|_| unavailable())?;
    if serde_json::to_vec(&reply).map_err(|_| unavailable())?.len() > 65_536 {
        return Err(unavailable());
    }
    tx.create(
        &reference_key(&reply.registration_ack.ack.opaque_registration_ref),
        "custody_cleanup_reference_v2",
        None,
        None,
        &ReferenceIndex {
            api_version: 2,
            registration_key: prepared.key.clone(),
        },
    )?;
    let account = current.status.receipt.native_account_id.clone();
    let next = Registration {
        phase: Phase::Registered(Box::new(signed)),
        ..row
    };
    tx.replace(
        &prepared.key,
        saved.row_version,
        KIND,
        Some(&account),
        None,
        &next,
    )?;
    Ok(reply)
}

/// A separate Account-issued certificate proves three actual roots and one
/// prepaid activation HTTP. The Native transaction consumes it once; it does
/// not infer Active from the earlier human approval or Pending registration.
pub(super) fn activate(
    tx: &mut IdentityTransaction<'_>,
    config: &CustodyConfig,
    identity: &super::super::config::IdentityConfig,
    configuration_digest: &str,
    pending: &SignedAccountConsumedPendingV2,
    active: SignedAccountActiveThreeRootsV2,
    issuer: &AccountMachineIssuerPinV2,
    floor: &AccountMachineIssuerFloorV2,
    now: u64,
) -> Result<SignedNativeActivationAckV2, StoreError> {
    let admitted_floor = admit_issuer_floor(tx, issuer, floor)?;
    active
        .verify_issuer(pending, issuer, &admitted_floor, now)
        .map_err(|_| unavailable())?;
    let key = record_key(pending)?;
    let saved = tx.get::<Registration>(&key)?.ok_or_else(unavailable)?;
    let row = saved.value;
    let Phase::Registered(registration) = &row.phase else {
        return Err(unavailable());
    };
    if row.api_version != 2
        || row.pending != *pending
        || row.configuration_digest != configuration_digest
    {
        return Err(unavailable());
    }
    active
        .validate_registration(pending, registration)
        .map_err(|_| unavailable())?;
    let current = observe(
        tx,
        &row.managed_storage_key,
        config,
        identity,
        pending,
        &active.certificate.activation_transport.activation_nonce,
        now,
    )?;
    if !same_security(&row.initial_lifetime, &current) {
        return Err(unavailable());
    }
    let head = tx.machine_storage_head().ok_or_else(unavailable)?.clone();
    let a = &active.certificate;
    let ack = NativeActivationAckV2 {
        api_version: 2,
        native_key_id: config.key_id.clone(),
        identity_namespace: identity.namespace.key(),
        identity_epoch: DecimalU64(identity.rp.identity_instance_epoch),
        session_id: a.session_id.clone(),
        approved_input_digest: a.approved_input_digest.clone(),
        opaque_registration_ref: a.opaque_registration_ref.clone(),
        pending_certificate_digest: pending.digest().map_err(|_| unavailable())?,
        active_certificate_digest: active.digest().map_err(|_| unavailable())?,
        root_birth_digest: a.root_birth_digest.clone(),
        native_transport_attempt_id: a.activation_transport.native_transport_attempt_id.clone(),
        activation_nonce: a.activation_transport.activation_nonce.clone(),
        storage_head: head,
        registry_cursor: DecimalU64(
            registration
                .ack
                .registry_cursor
                .0
                .checked_add(1)
                .ok_or_else(unavailable)?,
        ),
        activated_at: DecimalU64(now),
    };
    ack.validate_relative(pending, &active, registration)
        .map_err(|_| unavailable())?;
    let bytes = ack
        .signing_bytes(pending, &active, registration)
        .map_err(|_| unavailable())?;
    let signed = SignedNativeActivationAckV2 {
        signature_hex: hex::encode(config.key.sign(&bytes).to_bytes()),
        ack,
    };
    let account = current.status.receipt.native_account_id;
    let next = Registration {
        phase: Phase::Active {
            registration: registration.clone(),
            certificate: Box::new(active),
            ack: Box::new(signed.clone()),
        },
        ..row
    };
    tx.replace(&key, saved.row_version, KIND, Some(&account), None, &next)?;
    Ok(signed)
}

#[cfg(test)]
#[path = "custody_cleanup_v2_registry_tests.rs"]
mod tests;

pub(super) fn activate_registered(
    tx: &mut IdentityTransaction<'_>,
    config: &CustodyConfig,
    identity: &super::super::config::IdentityConfig,
    configuration_digest: &str,
    paid: PaidActivation,
    issuer: &AccountMachineIssuerPinV2,
    floor: &AccountMachineIssuerFloorV2,
    now: u64,
) -> Result<SignedNativeActivationAckV2, StoreError> {
    let request = paid.request;
    request.validate_shape().map_err(|_| unavailable())?;
    let index = tx
        .get::<ReferenceIndex>(&reference_key(&request.opaque_registration_ref))?
        .ok_or_else(unavailable)?
        .value;
    if index.api_version != 2 {
        return Err(unavailable());
    }
    let row = tx
        .get::<Registration>(&index.registration_key)?
        .ok_or_else(unavailable)?
        .value;
    if record_key(&row.pending)? != index.registration_key {
        return Err(unavailable());
    }
    activate(
        tx,
        config,
        identity,
        configuration_digest,
        &row.pending,
        request.active,
        issuer,
        floor,
        now,
    )
}

#[path = "custody_cleanup_v2_registry_term_current.rs"]
mod term_current;
pub(super) use term_current::{claim_term_current_registered, observe_term_current_registered};

#[path = "custody_cleanup_v2_registry_paid_prepare.rs"]
mod paid_prepare;
pub(super) use paid_prepare::{
    PaidActivation, PaidRegistration, claim_activation_native, claim_registration_native,
};
