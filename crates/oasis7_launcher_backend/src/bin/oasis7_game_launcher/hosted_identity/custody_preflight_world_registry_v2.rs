//! Separate fixed World substeps. A failed observation never undoes a durable
//! physical claim, and historical completion never permits another send.
use super::*;
use ed25519_dalek::Signer;
use oasis7_custody_core::native_binding::ManagedNativeSessionLifetimeV1;
use oasis7_custody_core::native_cleanup_v2::*;
const KIND: &str = "custody_preflight_world_v2";
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct WorldStage {
    api_version: u8,
    original: SignedAccountPreflightAccessV2,
    operation: NativePreflightWorldOperationV2,
    storage_key: String,
    configuration_digest: String,
    #[serde(deserialize_with = "required_nullable")]
    observed: Option<ManagedNativeSessionLifetimeV1>,
    #[serde(deserialize_with = "required_nullable")]
    completed: Option<SignedNativePreflightWorldReceiptV2>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct InputIndex {
    api_version: u8,
    original: SignedAccountPreflightAccessV2,
}
fn input_key(digest: &str) -> String {
    format!("custody_preflight_world_input_v2:{digest}")
}
/// Owned private capabilities, not serializable or caller-constructible.
pub(super) struct ClaimedWorldStage {
    key: String,
    original: SignedAccountPreflightAccessV2,
    operation: NativePreflightWorldOperationV2,
}
pub(super) struct ObservedWorldStage {
    claimed: ClaimedWorldStage,
    lifetime: ManagedNativeSessionLifetimeV1,
}
impl ObservedWorldStage {
    pub(super) fn original(&self) -> &SignedAccountPreflightAccessV2 {
        &self.claimed.original
    }
    pub(super) fn operation(&self) -> NativePreflightWorldOperationV2 {
        self.claimed.operation
    }
}
fn required_nullable<'de, D: serde::Deserializer<'de>, T: serde::Deserialize<'de>>(
    d: D,
) -> Result<Option<T>, D::Error> {
    Option::deserialize(d)
}
fn unavailable() -> StoreError {
    reject("preflight_world_unavailable")
}
fn stage_key(
    original: &SignedAccountPreflightAccessV2,
    operation: NativePreflightWorldOperationV2,
) -> Result<String, StoreError> {
    let digest = original.digest().map_err(|_| unavailable())?;
    let suffix = match operation {
        NativePreflightWorldOperationV2::PolicyOriginal => "policy",
        NativePreflightWorldOperationV2::AuthorizePolicy => "authorize",
    };
    Ok(format!("{KIND}:{digest}:{suffix}"))
}
pub(super) struct WorldStageScope<'a> {
    pub config: &'a CustodyConfig,
    pub identity: &'a super::super::config::IdentityConfig,
    pub storage_key: &'a str,
    pub configuration_digest: &'a str,
    pub pin: &'a AccountMachineIssuerPinV2,
    pub floor: &'a AccountMachineIssuerFloorV2,
    pub now: u64,
}
/// This transaction is committed before calling observe(), much less World.
pub(super) fn claim(
    tx: &mut IdentityTransaction<'_>,
    scope: &WorldStageScope<'_>,
    original: SignedAccountPreflightAccessV2,
    operation: NativePreflightWorldOperationV2,
) -> Result<ClaimedWorldStage, StoreError> {
    let WorldStageScope {
        config,
        identity,
        storage_key,
        configuration_digest,
        pin,
        floor,
        now,
    } = *scope;
    let floor = cleanup_registry::admit_issuer_floor(tx, pin, floor)?;
    original
        .verify_issuer(pin, &floor, now)
        .map_err(|_| unavailable())?;
    cleanup_consent::validate_saved_full_input(
        tx,
        config,
        identity,
        &original.certificate.input,
        &original
            .certificate
            .input
            .request
            .intent_digest()
            .map_err(|_| unavailable())?,
    )?;
    let index_key = input_key(&original.certificate.approved_input_digest);
    match tx.get::<InputIndex>(&index_key)? {
        Some(old) if old.value.api_version == 2 && old.value.original == original => {}
        Some(_) => return Err(unavailable()),
        None => tx.create(
            &index_key,
            "custody_preflight_world_input_v2",
            None,
            None,
            &InputIndex {
                api_version: 2,
                original: original.clone(),
            },
        )?,
    }
    let key = stage_key(&original, operation)?;
    if tx.get::<WorldStage>(&key)?.is_some() {
        return Err(unavailable());
    }
    let port = original
        .certificate
        .ports
        .iter()
        .find(|p| p.operation == operation)
        .ok_or_else(unavailable)?;
    cleanup_registry::claim_physical_attempt(
        tx,
        &original.certificate.issuer,
        &port.physical_attempt_id,
        &original.digest().map_err(|_| unavailable())?,
        match operation {
            NativePreflightWorldOperationV2::PolicyOriginal => "preflight-policy-world",
            NativePreflightWorldOperationV2::AuthorizePolicy => "preflight-authorize-world",
        },
    )?;
    tx.create(
        &key,
        KIND,
        None,
        None,
        &WorldStage {
            api_version: 2,
            original: original.clone(),
            operation,
            storage_key: storage_key.into(),
            configuration_digest: configuration_digest.into(),
            observed: None,
            completed: None,
        },
    )?;
    Ok(ClaimedWorldStage {
        key,
        original,
        operation,
    })
}
fn current(
    tx: &mut IdentityTransaction<'_>,
    config: &CustodyConfig,
    identity: &super::super::config::IdentityConfig,
    row: &WorldStage,
    now: u64,
) -> Result<ManagedNativeSessionLifetimeV1, StoreError> {
    cleanup_consent::validate_saved_full_input(
        tx,
        config,
        identity,
        &row.original.certificate.input,
        &row.original
            .certificate
            .input
            .request
            .intent_digest()
            .map_err(|_| unavailable())?,
    )?;
    let approval = tx
        .get::<ApprovalRecord>(&approval_key(
            &row.original.certificate.input.approval.proof.approval_id,
        ))?
        .ok_or_else(unavailable)?;
    // validate_saved_full_input above authenticates historical producer lineage;
    // this comparison independently binds the actual current deployment tuple.
    guarded_consent::require_guarded_consent_v2(
        tx,
        config,
        identity,
        &approval.value,
        guarded_consent::GuardedConsentUseV2::TermWideCleanup,
        Some(&row.configuration_digest),
    )?;
    let c = &row.original.certificate;
    if now < c.issued_at.0 || now >= c.expires_at.0 {
        return Err(unavailable());
    }
    let source = &c.input.approval;
    let request = ApprovalVerifyRequestV1 {
        purpose: VerificationPurposeV1::InspectOrigin,
        approval: source.clone(),
        proof_digest: source.digest().map_err(|_| unavailable())?,
        expected_audience: source.proof.context.audience.clone(),
        expected_intent_digest: source.proof.intent_digest.clone(),
        verify_nonce: c.certificate_id.clone(),
    };
    origin_observation::validate_verification_request(config, identity, &request)?;
    let origin = origin_observation::observe_origin_current(tx, config, identity, &request, now)?;
    if origin.origin_state != OriginStateV1::Active || origin.proof_state != ProofStateV1::Active {
        return Err(unavailable());
    }
    let life = managed_binding::observe_managed_lifetime(
        tx,
        &row.storage_key,
        config,
        identity,
        &c.certificate_id,
        now,
    )?;
    life.validate().map_err(|_| unavailable())?;
    let status = &life.status;
    let expected = &c.descriptor;
    if status.service_epoch != c.input.request.context.service_epoch
        || !status.active
        || status.frozen
        || status.pending
        || status.receipt != expected.native_binding
        || status.session_ref != expected.session.session_ref
        || status.generation != expected.session.native_binding_generation
    {
        return Err(unavailable());
    }
    Ok(life)
}
/// Fallible local current checks occur only after the separate durable claim.
pub(super) fn observe(
    tx: &mut IdentityTransaction<'_>,
    scope: &WorldStageScope<'_>,
    claimed: ClaimedWorldStage,
) -> Result<ObservedWorldStage, StoreError> {
    let WorldStageScope {
        config,
        identity,
        storage_key,
        configuration_digest,
        pin,
        floor,
        now,
    } = *scope;
    let row = tx
        .get::<WorldStage>(&claimed.key)?
        .ok_or_else(unavailable)?;
    let stage = &row.value;
    let floor = cleanup_registry::admit_issuer_floor(tx, pin, floor)?;
    stage
        .original
        .verify_issuer(pin, &floor, now)
        .map_err(|_| unavailable())?;
    if stage.api_version != 2
        || stage.original != claimed.original
        || stage.operation != claimed.operation
        || stage.storage_key != storage_key
        || stage.configuration_digest != configuration_digest
        || stage.observed.is_some()
        || stage.completed.is_some()
    {
        return Err(unavailable());
    }
    if claimed.operation == NativePreflightWorldOperationV2::AuthorizePolicy {
        let policy = tx
            .get::<WorldStage>(&stage_key(
                &claimed.original,
                NativePreflightWorldOperationV2::PolicyOriginal,
            )?)?
            .ok_or_else(unavailable)?
            .value;
        let receipt = policy.completed.as_ref().ok_or_else(unavailable)?;
        if policy.api_version != 2
            || policy.original != claimed.original
            || policy.observed.is_none()
            || policy.operation != NativePreflightWorldOperationV2::PolicyOriginal
        {
            return Err(unavailable());
        }
        receipt
            .verify_native(
                &claimed.original,
                &receipt.receipt.result_digest,
                &config.key_id,
                &hex::encode(config.key.verifying_key().to_bytes()),
            )
            .map_err(|_| unavailable())?;
    }
    let lifetime = current(tx, config, identity, stage, now)?;
    let mut stage = row.value;
    stage.observed = Some(lifetime.clone());
    tx.replace(&claimed.key, row.row_version, KIND, None, None, &stage)?;
    Ok(ObservedWorldStage { claimed, lifetime })
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
/// Actual result digest is computed by the typed policy/admission consumer.
/// The completion receipt is a fact, never permission for another World call.
pub(super) fn finish(
    tx: &mut IdentityTransaction<'_>,
    scope: &WorldStageScope<'_>,
    observed: ObservedWorldStage,
    result_digest: &str,
) -> Result<SignedNativePreflightWorldReceiptV2, StoreError> {
    let WorldStageScope {
        config,
        identity,
        configuration_digest,
        pin,
        floor,
        now,
        ..
    } = *scope;
    let row = tx
        .get::<WorldStage>(&observed.claimed.key)?
        .ok_or_else(unavailable)?;
    let stage = &row.value;
    let floor = cleanup_registry::admit_issuer_floor(tx, pin, floor)?;
    stage
        .original
        .verify_issuer(pin, &floor, now)
        .map_err(|_| unavailable())?;
    if stage.api_version != 2
        || stage.original != observed.claimed.original
        || stage.operation != observed.claimed.operation
        || stage.configuration_digest != configuration_digest
        || stage.observed.as_ref() != Some(&observed.lifetime)
        || stage.completed.is_some()
    {
        return Err(unavailable());
    }
    let live = current(tx, config, identity, stage, now)?;
    if !same_security(&observed.lifetime, &live) {
        return Err(unavailable());
    }
    let port = stage
        .original
        .certificate
        .ports
        .iter()
        .find(|p| p.operation == stage.operation)
        .ok_or_else(unavailable)?;
    let receipt = NativePreflightWorldReceiptV2 {
        api_version: 2,
        native_key_id: config.key_id.clone(),
        certificate_digest: stage.original.digest().map_err(|_| unavailable())?,
        operation: stage.operation,
        physical_attempt_id: port.physical_attempt_id.clone(),
        body_digest: port.body_digest.clone(),
        result_digest: result_digest.into(),
        completed_at: DecimalU64(now),
    };
    receipt
        .validate_relative(&stage.original, result_digest)
        .map_err(|_| unavailable())?;
    let signature = config
        .key
        .sign(&receipt.signing_bytes().map_err(|_| unavailable())?);
    let signed = SignedNativePreflightWorldReceiptV2 {
        receipt,
        signature_hex: hex::encode(signature.to_bytes()),
    };
    let mut stage = row.value;
    stage.completed = Some(signed.clone());
    tx.replace(
        &observed.claimed.key,
        row.row_version,
        KIND,
        None,
        None,
        &stage,
    )?;
    Ok(signed)
}

/// Third World read cannot precede both paid, completed fixed World stages.
/// Old profiles do not gain this stage or upgrade to the new profile.
pub(super) fn require_completed(
    tx: &mut IdentityTransaction<'_>,
    config: &CustodyConfig,
    pending: &SignedAccountConsumedPendingV2,
    pin: &AccountMachineIssuerPinV2,
    floor: &AccountMachineIssuerFloorV2,
) -> Result<(), StoreError> {
    let CustodyIntentV1::TestSessionV2 { plan, .. } = &pending.certificate.input.request.intent
    else {
        return Err(unavailable());
    };
    if plan.termination.cleanup_transport_policy != oasis7_custody_core::provisioning::CleanupTransportPolicyV2::TermWideFiveSecondOwnedWorld3V2 {
        return Ok(());
    }
    let index = tx
        .get::<InputIndex>(&input_key(&pending.certificate.approved_input_digest))?
        .ok_or_else(unavailable)?
        .value;
    let original = index.original;
    if index.api_version != 2
        || original.certificate.input != pending.certificate.input
        || original.certificate.descriptor.session != pending.certificate.participant_session
        || original.certificate.consumed_at != pending.certificate.consumed_at
    {
        return Err(unavailable());
    }
    // Historical proof verification, not refreshed current authorization.
    original
        .verify_issuer(pin, floor, original.certificate.issued_at.0)
        .map_err(|_| unavailable())?;
    for operation in [
        NativePreflightWorldOperationV2::PolicyOriginal,
        NativePreflightWorldOperationV2::AuthorizePolicy,
    ] {
        let stage = tx
            .get::<WorldStage>(&stage_key(&original, operation)?)?
            .ok_or_else(unavailable)?
            .value;
        let receipt = stage.completed.as_ref().ok_or_else(unavailable)?;
        if stage.api_version != 2
            || stage.original != original
            || stage.operation != operation
            || stage.observed.is_none()
        {
            return Err(unavailable());
        }
        receipt
            .verify_native(
                &original,
                &receipt.receipt.result_digest,
                &config.key_id,
                &hex::encode(config.key.verifying_key().to_bytes()),
            )
            .map_err(|_| unavailable())?;
    }
    Ok(())
}
