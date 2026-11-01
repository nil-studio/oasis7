//! Child of the existing protected registry. Execution has its own fixed paid
//! certificate and once claims; no Cleanup/World3 transport permit is borrowed.
use super::*;
use oasis7_custody_core::native_execution_prepare_v2::{
    ExecutionPreparePhysicalEdgeV2, NativeExecutionPrepareRequestV2,
};
const EXECUTION_KIND: &str = "custody_execution_prepare_gated_v2";
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ExecutionClaim {
    api_version: u8,
    request: NativeExecutionPrepareRequestV2,
    storage_key: String,
    configuration_digest: String,
    authorization: oasis7_custody_core::execution_dispatch_gate_v2::SignedExecutionDispatchClaimV2,
}
/// First-commit-only token. A stored row/history cannot recreate this value.
pub(in super::super) struct ClaimedExecutionPrepare {
    key: String,
    request: NativeExecutionPrepareRequestV2,
}
pub(in super::super) struct ObservedExecutionPrepare {
    claimed: ClaimedExecutionPrepare,
    lifetime: ManagedNativeSessionLifetimeV1,
    dispatch_until: u64,
}
impl ObservedExecutionPrepare {
    pub(in super::super) fn request(&self) -> &NativeExecutionPrepareRequestV2 {
        &self.claimed.request
    }
    pub(in super::super) fn require_dispatch_window(&self, now: u64) -> Result<(), StoreError> {
        if now >= self.dispatch_until {
            return Err(unavailable());
        }
        Ok(())
    }
}
fn execution_key(request: &NativeExecutionPrepareRequestV2) -> Result<String, StoreError> {
    let c = &request.proof.certificate;
    let bytes = serde_json::to_vec(&(
        &c.issuer.namespace,
        &c.issuer.service_id,
        &c.issuer.environment,
        &c.session_id,
        &c.stage_operation_id,
    ))
    .map_err(|_| unavailable())?;
    Ok(format!(
        "{EXECUTION_KIND}:{}",
        blake3::hash(&bytes).to_hex()
    ))
}
pub(in super::super) struct ExecutionScope<'a> {
    pub config: &'a CustodyConfig,
    pub identity: &'a super::super::super::config::IdentityConfig,
    pub machine: &'a super::super::cleanup_config::MachineConfig,
    pub storage_key: &'a str,
    pub configuration_digest: &'a str,
    pub now: u64,
}
/// Commit this transaction before the fallible observation and all World I/O.
pub(in super::super) fn claim_execution_prepare(
    tx: &mut IdentityTransaction<'_>,
    scope: &ExecutionScope<'_>,
    request: NativeExecutionPrepareRequestV2,
    authorization: oasis7_custody_core::execution_dispatch_gate_v2::SignedExecutionDispatchClaimV2,
) -> Result<ClaimedExecutionPrepare, StoreError> {
    request.wire_bytes().map_err(|_| unavailable())?;
    let pin = scope.machine.execution_prepare_pin()?;
    let floor = admit_issuer_floor(tx, pin, &scope.machine.issuer_floor)?;
    request
        .proof
        .verify_issuer(pin, &floor, scope.now)
        .map_err(|_| unavailable())?;
    let dispatch_pin = scope.machine.execution_dispatch_pin()?;
    let dispatch_floor = admit_issuer_floor(tx, dispatch_pin, &scope.machine.issuer_floor)?;
    authorization
        .verify_issuer(&request, dispatch_pin, &dispatch_floor, scope.now)
        .map_err(|_| unavailable())?;
    if authorization.claim.leaf.edge != ExecutionPreparePhysicalEdgeV2::IdentityActorToNative
        || authorization.claim.body_digest
            != oasis7_custody_core::approval::bytes_digest(
                &request.wire_bytes().map_err(|_| unavailable())?,
            )
    {
        return Err(unavailable());
    }
    let key = execution_key(&request)?;
    if tx.get::<ExecutionClaim>(&key)?.is_some() {
        return Err(unavailable());
    }
    let digest = request.proof.digest().map_err(|_| unavailable())?;
    for (edge, label) in [
        (
            ExecutionPreparePhysicalEdgeV2::IdentityActorToNative,
            "execution-prepare-native",
        ),
        (
            ExecutionPreparePhysicalEdgeV2::NativeToWorldCurrent,
            "execution-prepare-world-current",
        ),
        (
            ExecutionPreparePhysicalEdgeV2::NativeToWorldPrepare,
            "execution-prepare-world-prepare",
        ),
    ] {
        let port = request
            .proof
            .certificate
            .ports
            .iter()
            .find(|p| p.edge == edge)
            .ok_or_else(unavailable)?;
        claim_physical_attempt(
            tx,
            &request.proof.certificate.issuer,
            &port.attempt_id,
            &digest,
            label,
        )?;
    }
    tx.create(
        &key,
        EXECUTION_KIND,
        None,
        None,
        &ExecutionClaim {
            api_version: 2,
            request: request.clone(),
            storage_key: scope.storage_key.into(),
            configuration_digest: scope.configuration_digest.into(),
            authorization,
        },
    )?;
    Ok(ClaimedExecutionPrepare { key, request })
}
fn current_execution_identity(
    tx: &mut IdentityTransaction<'_>,
    scope: &ExecutionScope<'_>,
    claimed: &ClaimedExecutionPrepare,
) -> Result<(ManagedNativeSessionLifetimeV1, u64), StoreError> {
    let stored = tx
        .get::<ExecutionClaim>(&claimed.key)?
        .ok_or_else(unavailable)?
        .value;
    if stored.api_version != 2
        || stored.request != claimed.request
        || stored.storage_key != scope.storage_key
        || stored.configuration_digest != scope.configuration_digest
    {
        return Err(unavailable());
    }
    let dispatch_pin = scope.machine.execution_dispatch_pin()?;
    let dispatch_floor = admit_issuer_floor(tx, dispatch_pin, &scope.machine.issuer_floor)?;
    stored
        .authorization
        .verify_issuer(&claimed.request, dispatch_pin, &dispatch_floor, scope.now)
        .map_err(|_| unavailable())?;
    let c = &claimed.request.proof.certificate;
    let pin = scope.machine.execution_prepare_pin()?;
    let floor = admit_issuer_floor(tx, pin, &scope.machine.issuer_floor)?;
    claimed
        .request
        .proof
        .verify_issuer(pin, &floor, scope.now)
        .map_err(|_| unavailable())?;
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
        || row.managed_storage_key != scope.storage_key
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
        super::super::guarded_consent::GuardedConsentUseV2::TermWideCleanup,
        Some(scope.configuration_digest),
    )?;
    let observed = observe(
        tx,
        scope.storage_key,
        scope.config,
        scope.identity,
        &row.pending,
        &c.source_nonce,
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
        verify_nonce: c.source_nonce.clone(),
    };
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
        .min(stored.authorization.claim.expires_at.0)
        .min(observed.status.expires_at.0)
        .min(observed.session_expires_at.0)
        .min(observed.session_idle_expires_at.0)
        .min(origin.source_session_expires_at.0)
        .min(origin.source_session_idle_expires_at.0);
    if scope.now >= dispatch_until {
        return Err(unavailable());
    }
    Ok((observed, dispatch_until))
}
pub(in super::super) fn observe_execution_prepare(
    tx: &mut IdentityTransaction<'_>,
    scope: &ExecutionScope<'_>,
    claimed: ClaimedExecutionPrepare,
) -> Result<ObservedExecutionPrepare, StoreError> {
    let (lifetime, dispatch_until) = current_execution_identity(tx, scope, &claimed)?;
    Ok(ObservedExecutionPrepare {
        claimed,
        lifetime,
        dispatch_until,
    })
}
/// Reenter the same real registry after owned World work; never retimestamp or
/// recreate a failed/unknown claim. Returning raw delivery does not admit it.
pub(in super::super) fn finish_execution_prepare(
    tx: &mut IdentityTransaction<'_>,
    scope: &ExecutionScope<'_>,
    observed: ObservedExecutionPrepare,
) -> Result<(), StoreError> {
    observed.require_dispatch_window(scope.now)?;
    let (after, _) = current_execution_identity(tx, scope, &observed.claimed)?;
    if !same_security(&observed.lifetime, &after) {
        return Err(unavailable());
    }
    Ok(())
}

pub(in super::super) fn recheck_execution_prepare(
    tx: &mut IdentityTransaction<'_>,
    scope: &ExecutionScope<'_>,
    observed: &ObservedExecutionPrepare,
) -> Result<(), StoreError> {
    observed.require_dispatch_window(scope.now)?;
    let (current, until) = current_execution_identity(tx, scope, &observed.claimed)?;
    if !same_security(&observed.lifetime, &current) || scope.now >= until {
        return Err(unavailable());
    }
    Ok(())
}

pub(in super::super) fn claim_execution_world_callback(
    tx: &mut IdentityTransaction<'_>,
    scope: &ExecutionScope<'_>,
    observed: &ObservedExecutionPrepare,
    previous: &oasis7_custody_core::execution_dispatch_gate_v2::SignedExecutionDispatchClaimV2,
    edge: ExecutionPreparePhysicalEdgeV2,
    body_digest: String,
) -> Result<
    oasis7_custody_core::execution_dispatch_gate_v2::NativeExecutionDispatchCallbackV2,
    StoreError,
> {
    use ed25519_dalek::Signer;
    use oasis7_custody_core::execution_dispatch_gate_v2::*;
    recheck_execution_prepare(tx, scope, observed)?;
    let request = observed.request();
    let pin = scope.machine.execution_dispatch_pin()?;
    let floor = admit_issuer_floor(tx, pin, &scope.machine.issuer_floor)?;
    previous
        .verify_issuer(request, pin, &floor, scope.now)
        .map_err(|_| unavailable())?;
    let previous_edge = match edge {
        ExecutionPreparePhysicalEdgeV2::NativeToWorldCurrent => {
            ExecutionPreparePhysicalEdgeV2::IdentityActorToNative
        }
        ExecutionPreparePhysicalEdgeV2::NativeToWorldPrepare => {
            ExecutionPreparePhysicalEdgeV2::NativeToWorldCurrent
        }
        _ => return Err(unavailable()),
    };
    if previous.claim.leaf.edge != previous_edge {
        return Err(unavailable());
    }
    let inventory = execution_reentry_inventory(request).map_err(|_| unavailable())?;
    let ids: Vec<_> = inventory
        .into_iter()
        .filter(|p| p.edge == edge)
        .map(|p| p.port.attempt_id)
        .collect();
    let query = NativeExecutionReentryQueryV2 {
        api_version: 2,
        native_key_id: scope.config.key_id.clone(),
        identity_origin: scope.machine.execution_identity_origin()?.into(),
        native_proof_digest: request.proof.digest().map_err(|_| unavailable())?,
        leaf: request
            .proof
            .certificate
            .ports
            .iter()
            .find(|p| p.edge == edge)
            .ok_or_else(unavailable)?
            .clone(),
        body_digest,
        callback_attempt_ids: ids.try_into().map_err(|_| unavailable())?,
        nonce: native_reentry_nonce(request, edge).map_err(|_| unavailable())?,
        issued_at: DecimalU64(scope.now),
        expires_at: DecimalU64(observed.dispatch_until.min(previous.claim.expires_at.0)),
    };
    let bytes = query.signing_bytes(request).map_err(|_| unavailable())?;
    let signed = SignedNativeExecutionReentryQueryV2 {
        signature_hex: hex::encode(scope.config.key.sign(&bytes).to_bytes()),
        query,
    };
    let q = NativeExecutionDispatchCallbackV2 {
        api_version: 2,
        request: Box::new(request.clone()),
        previous: previous.clone(),
        query: signed,
    };
    let bytes = q.wire_bytes().map_err(|_| unavailable())?;
    let digest = blake3::hash(&bytes).to_hex().to_string();
    let attempt = &q.query.query.callback_attempt_ids[0];
    claim_physical_attempt(
        tx,
        &request.proof.certificate.issuer,
        attempt,
        &digest,
        "execution-world-reentry",
    )?;
    tx.create(
        &format!("custody_execution_world_reentry_v2:{attempt}"),
        "custody_execution_world_reentry_v2",
        None,
        None,
        &q,
    )?;
    Ok(q)
}
