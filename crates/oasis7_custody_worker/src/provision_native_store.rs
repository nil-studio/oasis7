//! Account-local native stage transitions. All authority/config inputs are Rust-owned.
use super::*;
#[path = "provision_native_history_store.rs"]
pub mod history;
#[path = "owner_setup_store.rs"]
pub mod owner_setup;
#[path = "provision_participant_store.rs"]
pub mod participant;
#[path = "provision_native_recovery_store.rs"]
pub mod recovery;
use crate::approval_store::ExactCache;
use oasis7_custody_core::{
    approval::{ApprovalVerifyRequestV1, SignedApprovalVerificationV1},
    approval_admission::{ApprovalAdmissionError, ApprovalSignatureVerifier, ExactSignatureChecks},
    native_binding::{
        NativeBindingPinV1, SignedNativeBindingChallengeV1, SignedOwnerNativeBindingStatementV1,
    },
    provisioning::{
        ManagedLoginPermitViewV1, ManagedLoginTaskV1, NativeStageOriginalV1,
        NativeStagePermitViewV1, NativeStageRequestV1, NativeStageResultV1, NativeStageScope,
        PersistedNativeSessionAttestation,
    },
};

/// Fixed deployment pins; never deserialized from a caller or a native response.
#[derive(Clone)]
pub struct NativeStageScopeCurrent {
    pub http_origin: String,
    pub native_root_key_id: String,
    pub native_root_public_key: String,
    pub owner_binding_pin: NativeBindingPinV1,
}
impl NativeStageScopeCurrent {
    fn as_core(&self) -> NativeStageScope<'_> {
        NativeStageScope {
            http_origin: &self.http_origin,
            native_root_key_id: &self.native_root_key_id,
            native_root_public_key: &self.native_root_public_key,
            owner_binding_pin: &self.owner_binding_pin,
        }
    }
}
/// Target authority is the real Account epoch (0, then activation 1). The original
/// Provision proof retains its signed epoch 0; current source observations are separate.
#[derive(Clone)]
pub struct NativeStageCurrent {
    pub provision: ProvisionApprovalCurrent,
    pub scope: NativeStageScopeCurrent,
}
struct NativeExactCache(Vec<ExactCache>);
impl ApprovalSignatureVerifier for NativeExactCache {
    fn verify(
        &mut self,
        checks: &ExactSignatureChecks,
    ) -> std::result::Result<(), ApprovalAdmissionError> {
        for cache in &mut self.0 {
            if cache.verify(checks).is_ok() {
                return Ok(());
            }
        }
        Err(ApprovalAdmissionError::SignatureRejected)
    }
}
/// Match public historical fact plus the exact private persistence attestation.
/// An absent result is pending, never historical success; corrupted/unknown originals reject.
fn historical_result(
    original: Option<&NativeStageResultV1>,
    generation: Option<DecimalU64>,
    requested: &NativeStageResultV1,
    session: Option<&PersistedNativeSessionAttestation<'_>>,
) -> Result<Option<NativeStageResultV1>> {
    let Some(original) = original else {
        return Ok(None);
    };
    if original != requested {
        return Err(rejected());
    }
    match original {
        NativeStageResultV1::SessionStatus(signed) => {
            let session = session.ok_or_else(rejected)?;
            if generation.is_none_or(|v| v.0 == 0 || v.0 != session.private_sid_generation)
                || session.session_ref != signed.status.session_ref
                || session.status_digest != signed.status.digest().map_err(|_| rejected())?
            {
                return Err(rejected());
            }
        }
        _ if session.is_some() || generation.is_some() => return Err(rejected()),
        _ => {}
    }
    Ok(Some(original.clone()))
}
impl OwnerProvisionApprovalStore {
    /// Exact persisted original only; this does not authorize an outbound request.
    pub fn native_stage_original(
        &self,
        request: &NativeStageRequestV1,
    ) -> Result<NativeStageOriginalV1> {
        request.validate().map_err(|_| rejected())?;
        self.snapshot()?
            .state
            .native_stages()
            .get(&request.operation_id)
            .filter(|saved| &saved.request == request)
            .cloned()
            .ok_or_else(rejected)
    }
    /// Historical statement recovery never signs again or changes its original deadline.
    pub fn native_binding_statement(
        &self,
        request: &NativeStageRequestV1,
    ) -> Result<SignedOwnerNativeBindingStatementV1> {
        self.native_stage_original(request)?
            .binding_statement
            .ok_or_else(rejected)
    }
    /// Read an already registered B1 statement from this exact Account/plan/native
    /// registration for B2 receipt lookup. This does not re-sign or authorize B2.
    pub fn native_binding_statement_archive(
        &self,
        request: &NativeStageRequestV1,
        binding_operation_id: &str,
    ) -> Result<SignedOwnerNativeBindingStatementV1> {
        let current = self.native_stage_original(request)?;
        let saved = self.snapshot()?;
        let draft = saved.state.draft();
        let mut found = None;
        for original in saved.state.native_stages().values() {
            let Some(signed) = &original.binding_statement else {
                continue;
            };
            let statement = &signed.statement;
            if statement.binding_operation_id != binding_operation_id {
                continue;
            }
            let registration = &current.descriptor.registration;
            if original.descriptor.registration != *registration
                || original.descriptor.identity_key != current.descriptor.identity_key
                || statement.environment != draft.entry.environment
                || statement.custody_account_id != request.custody_account_id
                || statement.provision_id != request.provision_id
                || statement.approved_plan_digest != draft.entry.plan_digest
                || statement.login_key_ref != current.descriptor.identity_key.key_id
                || statement.challenge.identity_namespace != registration.identity_namespace
                || statement.challenge.identity_instance_epoch
                    != registration.identity_instance_epoch
                || statement.challenge.registration_operation_id
                    != registration.registration_operation_id
                || statement.challenge.registration_request_digest
                    != registration.digest().map_err(|_| rejected())?
                || statement.challenge.login_public_key != registration.login_public_key
            {
                return Err(rejected());
            }
            let known_root_challenge = original
                .binding_challenge
                .as_ref()
                .or_else(|| {
                    saved
                        .state
                        .native_stages()
                        .values()
                        .find_map(|stage| match &stage.result {
                            Some(NativeStageResultV1::RegistrationChallenge(challenge))
                                if challenge.challenge == statement.challenge =>
                            {
                                Some(challenge)
                            }
                            _ => None,
                        })
                })
                .ok_or_else(rejected)?;
            if known_root_challenge.challenge != statement.challenge
                || known_root_challenge.validate().is_err()
                || signed.validate().is_err()
            {
                return Err(rejected());
            }
            if found.as_ref().is_some_and(|old| old != signed) {
                return Err(rejected());
            }
            found = Some(signed.clone());
        }
        found.ok_or_else(rejected)
    }
    /// Tightening reservation only: charge and persist before remote Current/RF I/O.
    pub fn reserve_native_stage(
        &self,
        request: NativeStageRequestV1,
        inspection_id: &str,
        scope: &NativeStageScopeCurrent,
    ) -> Result<NativeStageOriginalV1> {
        let nonce = crate::approval_crypto::verification_nonce().map_err(|_| rejected())?;
        let id = request.operation_id.clone();
        self.local_reservation("native_stage_reserved", &id, |state, time| {
            state
                .reserve_native_stage(
                    request.clone(),
                    inspection_id,
                    &nonce,
                    &scope.as_core(),
                    time,
                )
                .map_err(|_| rejected())
        })
    }
    fn native_step<F, R, T>(
        &self,
        current: &F,
        event: &str,
        reference: &str,
        mut transition: T,
    ) -> Result<R>
    where
        F: Fn() -> Result<NativeStageCurrent>,
        T: FnMut(
            &mut AccountProvisionApprovalState,
            &NativeStageScope<'_>,
            &ProvisionApprovalContext<'_>,
            u64,
        ) -> Result<R>,
    {
        let initial = read(&self.state, KEY)?;
        let mut returned = None;
        let mut callback = |old: String, inventory: String| -> Result<Array> {
            #[cfg(feature = "approval-harness")]
            let _performance = super::performance::callback(event, &old, &self.state);
            let (mut next, previous) = self.reader.transition(&old)?;
            let trusted = current()?;
            let time = now()?;
            let result = transition(
                &mut next.state,
                &trusted.scope.as_core(),
                &trusted.provision.as_core(),
                time,
            )?;
            let array = if next.state == previous {
                validate_inventory(&inventory, next.state.draft(), false)?;
                unchanged(&old)
            } else {
                validate_inventory(&inventory, next.state.draft(), true)?;
                next.audit_sequence =
                    DecimalU64(next.audit_sequence.0.checked_add(1).ok_or_else(rejected)?);
                let audit = serde_json::to_string(&(
                    next.audit_sequence,
                    event,
                    reference,
                    trusted.provision.approval.authority.service_epoch,
                ))
                .map_err(|_| rejected())?;
                let array = Array::new();
                array.push(&encode(&next)?.into());
                array.push(&next.audit_sequence.0.to_string().into());
                array.push(&audit.into());
                array
            };
            returned = Some(result);
            Ok(array)
        };
        let closure = Closure::borrow_mut(&mut callback);
        update(&self.state, KEY, &initial, closure.as_ref().unchecked_ref())?;
        drop(closure);
        returned.ok_or_else(rejected)
    }
    async fn native_verified<F, R, C, T>(
        &self,
        current: &F,
        event: &str,
        reference: &str,
        checks: C,
        mut transition: T,
    ) -> Result<R>
    where
        F: Fn() -> Result<NativeStageCurrent>,
        C: Fn(
            &AccountProvisionApprovalState,
            &NativeStageScope<'_>,
            &ProvisionApprovalContext<'_>,
            u64,
        ) -> Result<Vec<ExactSignatureChecks>>,
        T: FnMut(
            &mut AccountProvisionApprovalState,
            &NativeStageScope<'_>,
            &ProvisionApprovalContext<'_>,
            u64,
            &mut NativeExactCache,
        ) -> Result<R>,
    {
        let snapshot = self.snapshot()?;
        let trusted = current()?;
        let checks_before = checks(
            &snapshot.state,
            &trusted.scope.as_core(),
            &trusted.provision.as_core(),
            now()?,
        )?;
        let mut verified = Vec::with_capacity(checks_before.len());
        for exact in checks_before {
            verified.push(verify(exact).await?);
        }
        let mut cache = NativeExactCache(verified);
        self.native_step(current, event, reference, |state, scope, context, time| {
            for exact in checks(state, scope, context, time)? {
                cache.verify(&exact).map_err(|_| rejected())?;
            }
            transition(state, scope, context, time, &mut cache)
        })
    }
    pub fn claim_native_origin_inspection<F>(
        &self,
        id: &str,
        current: F,
    ) -> Result<ApprovalVerifyRequestV1>
    where
        F: Fn() -> Result<NativeStageCurrent>,
    {
        self.native_step(
            &current,
            "native_source_claimed",
            id,
            |state, _, context, time| {
                state
                    .claim_native_origin_inspection(id, context, time)
                    .map_err(|_| rejected())
            },
        )
    }
    pub async fn complete_native_origin_inspection<F>(
        &self,
        id: &str,
        signed: SignedApprovalVerificationV1,
        current: F,
    ) -> Result<()>
    where
        F: Fn() -> Result<NativeStageCurrent>,
    {
        self.native_verified(
            &current,
            "native_source_verified",
            id,
            |state, _, context, time| {
                Ok(vec![
                    state
                        .native_origin_result_checks(id, &signed, context, time)
                        .map_err(|_| rejected())?,
                ])
            },
            |state, _, context, time, cache| {
                state
                    .complete_native_origin_inspection(id, signed.clone(), context, time, cache)
                    .map_err(|_| rejected())
            },
        )
        .await
    }
    pub async fn claim_native_stage<F>(
        &self,
        id: &str,
        current: F,
    ) -> Result<NativeStagePermitViewV1>
    where
        F: Fn() -> Result<NativeStageCurrent>,
    {
        self.native_verified(
            &current,
            "native_stage_claimed",
            id,
            |state, scope, context, time| {
                Ok(vec![
                    state
                        .native_stage_checks(id, scope, context, time)
                        .map_err(|_| rejected())?,
                ])
            },
            |state, scope, context, time, cache| {
                state
                    .claim_native_stage(id, scope, context, time, cache)
                    .map_err(|_| rejected())
            },
        )
        .await
    }
    pub fn reserve_native_stage_read(&self, id: &str) -> Result<u8> {
        self.local_reservation("native_oracle_reserved", id, |state, time| {
            state
                .reserve_native_stage_read(id, time)
                .map_err(|_| rejected())
        })
    }
    pub async fn authorize_native_stage_read<F>(
        &self,
        id: &str,
        ticket: u8,
        current: F,
    ) -> Result<NativeStagePermitViewV1>
    where
        F: Fn() -> Result<NativeStageCurrent>,
    {
        self.native_verified(
            &current,
            "native_oracle_authorized",
            id,
            |state, scope, context, time| {
                Ok(vec![
                    state
                        .native_stage_checks(id, scope, context, time)
                        .map_err(|_| rejected())?,
                ])
            },
            |state, scope, context, time, cache| {
                state
                    .authorize_native_stage_read(id, ticket, scope, context, time, cache)
                    .map_err(|_| rejected())
            },
        )
        .await
    }
    pub async fn record_native_binding_challenge<F>(
        &self,
        id: &str,
        signed: SignedNativeBindingChallengeV1,
        current: F,
    ) -> Result<()>
    where
        F: Fn() -> Result<NativeStageCurrent>,
    {
        self.native_verified(
            &current,
            "native_binding_challenge_recorded",
            id,
            |state, scope, context, time| {
                Ok(vec![
                    state
                        .native_binding_challenge_checks(id, &signed, scope, context, time)
                        .map_err(|_| rejected())?,
                ])
            },
            |state, scope, context, time, cache| {
                state
                    .record_native_binding_challenge(
                        id,
                        signed.clone(),
                        scope,
                        context,
                        time,
                        cache,
                    )
                    .map_err(|_| rejected())
            },
        )
        .await
    }
    pub async fn register_native_binding_statement<F>(
        &self,
        id: &str,
        signed: SignedOwnerNativeBindingStatementV1,
        current: F,
    ) -> Result<SignedOwnerNativeBindingStatementV1>
    where
        F: Fn() -> Result<NativeStageCurrent>,
    {
        if self
            .snapshot()?
            .state
            .native_stages()
            .get(id)
            .and_then(|v| v.binding_statement.as_ref())
            .is_some_and(|v| v == &signed)
        {
            return self.local_reservation("native_statement_original_read", id, |state, _| {
                state
                    .native_stages()
                    .get(id)
                    .and_then(|v| v.binding_statement.clone())
                    .filter(|v| v == &signed)
                    .ok_or_else(rejected)
            });
        }
        self.native_verified(
            &current,
            "native_binding_statement_registered",
            id,
            |state, scope, context, time| {
                Ok(vec![
                    state
                        .native_binding_statement_checks(id, &signed, scope, context, time)
                        .map_err(|_| rejected())?,
                ])
            },
            |state, scope, context, time, cache| {
                state
                    .register_native_binding_statement(
                        id,
                        signed.clone(),
                        scope,
                        context,
                        time,
                        cache,
                    )
                    .map_err(|_| rejected())?;
                state
                    .native_stages()
                    .get(id)
                    .and_then(|v| v.binding_statement.clone())
                    .ok_or_else(rejected)
            },
        )
        .await
    }
    /// Recover only an exact previously committed fact; never constructs Current or renews a grant.
    pub fn recover_native_result(
        &self,
        id: &str,
        result: &NativeStageResultV1,
        session: Option<&PersistedNativeSessionAttestation<'_>>,
    ) -> Result<Option<NativeStageResultV1>> {
        let saved = self.snapshot()?;
        let original = saved.state.native_stages().get(id).ok_or_else(rejected)?;
        if historical_result(
            original.result.as_ref(),
            original.private_sid_generation,
            result,
            session,
        )?
        .is_none()
        {
            return Ok(None);
        }
        self.local_reservation("native_result_original_read", id, |state, _| {
            let original = state.native_stages().get(id).ok_or_else(rejected)?;
            historical_result(
                original.result.as_ref(),
                original.private_sid_generation,
                result,
                session,
            )?
            .map(Some)
            .ok_or_else(rejected)
        })
    }
    pub fn recover_managed_login_result(
        &self,
        id: &str,
        result: &NativeStageResultV1,
        session: Option<&PersistedNativeSessionAttestation<'_>>,
    ) -> Result<Option<NativeStageResultV1>> {
        let saved = self.snapshot()?;
        let original = saved.state.managed_login_task(id).ok_or_else(rejected)?;
        if historical_result(
            original.result.as_ref(),
            original.private_sid_generation,
            result,
            session,
        )?
        .is_none()
        {
            return Ok(None);
        }
        self.local_reservation("native_result_original_read", id, |state, _| {
            let original = state.managed_login_task(id).ok_or_else(rejected)?;
            historical_result(
                original.result.as_ref(),
                original.private_sid_generation,
                result,
                session,
            )?
            .map(Some)
            .ok_or_else(rejected)
        })
    }
    /// A charged current Binding attempt may recover only an Account-known old
    /// statement's root-signed receipt. The core factory owns B2/B1 reconciliation.
    pub async fn recover_native_binding_receipt<F>(
        &self,
        id: &str,
        receipt: oasis7_custody_core::native_binding::SignedNativeBindingReceiptV1,
        current: F,
    ) -> Result<NativeStageResultV1>
    where
        F: Fn() -> Result<NativeStageCurrent>,
    {
        let result = NativeStageResultV1::BindingReceipt(receipt.clone());
        if let Some(original) = self.recover_native_result(id, &result, None)? {
            return Ok(original);
        }
        self.native_verified(
            &current,
            "native_binding_receipt_recovered",
            id,
            |state, scope, context, time| {
                Ok(vec![
                    state
                        .native_binding_recovery_checks(id, &receipt, scope, context, time)
                        .map_err(|_| rejected())?,
                ])
            },
            |state, scope, context, time, cache| {
                state
                    .recover_native_binding_receipt(
                        id,
                        receipt.clone(),
                        scope,
                        context,
                        time,
                        cache,
                    )
                    .map_err(|_| rejected())
            },
        )
        .await
    }
    pub async fn complete_native_stage<F>(
        &self,
        id: &str,
        result: NativeStageResultV1,
        session: Option<&PersistedNativeSessionAttestation<'_>>,
        current: F,
    ) -> Result<NativeStageResultV1>
    where
        F: Fn() -> Result<NativeStageCurrent>,
    {
        if let Some(original) = self.recover_native_result(id, &result, session)? {
            return Ok(original);
        }
        self.native_verified(
            &current,
            "native_stage_result_recorded",
            id,
            |state, scope, context, time| {
                Ok(vec![
                    match &result {
                        NativeStageResultV1::ExplicitLoginRequired => {
                            state.native_stage_checks(id, scope, context, time)
                        }
                        _ => state.native_result_checks(id, &result, scope, context, time),
                    }
                    .map_err(|_| rejected())?,
                ])
            },
            |state, scope, context, time, cache| {
                state
                    .complete_native_stage(id, result.clone(), scope, context, time, session, cache)
                    .map_err(|_| rejected())
            },
        )
        .await
    }
    pub async fn begin_managed_login<F>(
        &self,
        input: ApprovalAdmissionInput,
        current: F,
    ) -> Result<ApprovalAdmissionRecord>
    where
        F: Fn() -> Result<NativeStageCurrent>,
    {
        let snapshot = self.snapshot()?;
        let nonce =
            if let Some(original) = snapshot.state.managed_login_original(&input.operation_id) {
                if original.input() != &input {
                    return Err(rejected());
                }
                original.verify_nonce().to_owned()
            } else {
                crate::approval_crypto::verification_nonce().map_err(|_| rejected())?
            };
        let id = input.operation_id.clone();
        self.native_verified(
            &current,
            "managed_login_pending",
            &id,
            |state, _, context, time| {
                Ok(vec![
                    state
                        .begin_managed_login_checks(&input, &nonce, context, time)
                        .map_err(|_| rejected())?,
                ])
            },
            |state, _, context, time, cache| {
                state
                    .begin_managed_login(input.clone(), nonce.clone(), context, time, cache)
                    .map_err(|_| rejected())
            },
        )
        .await
    }
    pub async fn commit_managed_login<F>(
        &self,
        id: &str,
        signed: SignedApprovalVerificationV1,
        current: F,
    ) -> Result<ManagedLoginTaskV1>
    where
        F: Fn() -> Result<NativeStageCurrent>,
    {
        let nonce = crate::approval_crypto::verification_nonce().map_err(|_| rejected())?;
        self.native_verified(
            &current,
            "managed_login_consumed",
            id,
            |state, _, context, time| {
                Ok(vec![
                    state
                        .commit_managed_login_checks(id, &signed, context, time)
                        .map_err(|_| rejected())?,
                ])
            },
            |state, _, context, time, cache| {
                state
                    .commit_managed_login(id, signed.clone(), context, time, &nonce, cache)
                    .map_err(|_| rejected())
            },
        )
        .await
    }
    pub async fn claim_managed_login<F>(&self, id: &str, current: F) -> Result<ManagedLoginTaskV1>
    where
        F: Fn() -> Result<NativeStageCurrent>,
    {
        self.native_verified(
            &current,
            "managed_login_claimed",
            id,
            |state, scope, context, time| {
                Ok(vec![
                    state
                        .managed_login_dispatch_checks(id, scope, context, time)
                        .map_err(|_| rejected())?,
                ])
            },
            |state, scope, context, time, cache| {
                state
                    .claim_managed_login(id, scope, context, time, cache)
                    .map_err(|_| rejected())
            },
        )
        .await
    }
    pub fn reserve_managed_login_read(&self, id: &str) -> Result<u8> {
        self.local_reservation("managed_login_oracle_reserved", id, |state, time| {
            state
                .reserve_managed_login_read(id, time)
                .map_err(|_| rejected())
        })
    }
    pub async fn authorize_managed_login_read<F>(
        &self,
        id: &str,
        ticket: u8,
        current: F,
    ) -> Result<ManagedLoginPermitViewV1>
    where
        F: Fn() -> Result<NativeStageCurrent>,
    {
        self.native_verified(
            &current,
            "managed_login_oracle_authorized",
            id,
            |state, scope, context, time| {
                Ok(vec![
                    state
                        .managed_login_dispatch_checks(id, scope, context, time)
                        .map_err(|_| rejected())?,
                ])
            },
            |state, scope, context, time, cache| {
                state
                    .authorize_managed_login_read(id, ticket, scope, context, time, cache)
                    .map_err(|_| rejected())
            },
        )
        .await
    }
    pub async fn complete_managed_login<F>(
        &self,
        id: &str,
        result: NativeStageResultV1,
        session: Option<&PersistedNativeSessionAttestation<'_>>,
        current: F,
    ) -> Result<NativeStageResultV1>
    where
        F: Fn() -> Result<NativeStageCurrent>,
    {
        if let Some(original) = self.recover_managed_login_result(id, &result, session)? {
            return Ok(original);
        }
        self.native_verified(
            &current,
            "managed_login_result_recorded",
            id,
            |state, scope, context, time| {
                Ok(vec![
                    match &result {
                        NativeStageResultV1::SessionStatus(v) => {
                            state.managed_login_result_checks(id, v, scope, context, time)
                        }
                        NativeStageResultV1::ExplicitLoginRequired => {
                            state.managed_login_dispatch_checks(id, scope, context, time)
                        }
                        _ => return Err(rejected()),
                    }
                    .map_err(|_| rejected())?,
                ])
            },
            |state, scope, context, time, cache| {
                state
                    .complete_managed_login(
                        id,
                        result.clone(),
                        scope,
                        context,
                        time,
                        session,
                        cache,
                    )
                    .map_err(|_| rejected())
            },
        )
        .await
    }
}

#[path = "owner_setup_reconcile_store.rs"]
pub mod setup_reconcile;

#[path = "test_session_store.rs"]
pub(crate) mod test_session;
#[path = "test_session_client_store.rs"]
pub(crate) mod client;
