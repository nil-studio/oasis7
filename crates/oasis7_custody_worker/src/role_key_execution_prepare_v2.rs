//! Exact Prepare domains, after a committed independent SQLite physical once.
use super::execution_once_v2::ClaimedExecutionPrepareV2;
use super::*;
use ed25519_dalek::Signer;
use oasis7_client_api::world_service::{
    self as ws, entry_executor::EntryExecutorAccessV1, task_context::TASK_CONTEXT_PREPARE_PATH,
    task_context_projection::TaskContextPrepareAttestationRequestV2,
};
use oasis7_custody_core::{
    provisioning::{
        ExecutionPrepareSignedOriginalV2, ExecutionPrepareSignerOperationV2,
        ExecutionSigningActionV2, VerifiedExecutionPreparePermitV2,
    },
    role_key_slot::RoleKeySlotScopeV1,
};
/// Trusted private reader output for this exact Session/stage, not the old
/// provisioning-only RoleKeyCurrent. No Deserialize or caller boolean profile.
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct ExecutionPrepareSigningCurrentV2 {
    pub role: RoleKeyCurrent,
    pub session_id: String,
    pub approved_input_digest: String,
    pub action_request_id: String,
    pub stage_operation_id: String,
    pub source_operation_id: String,
    pub source_nonce: String,
    pub credential_generation: DecimalU64,
    pub bundle_digest: String,
    pub service_proof_digest: String,
}
impl ExecutionPrepareSigningCurrentV2 {
    fn check(
        &self,
        verified: &VerifiedExecutionPreparePermitV2,
        scope: &RoleKeySlotScopeV1,
        at: u64,
    ) -> Result<()> {
        self.role.check(scope, at)?;
        let proof = &verified.proof().permit;
        if self.session_id != proof.session_id
            || self.approved_input_digest != proof.approved_input_digest
            || self.action_request_id != proof.action_request_id
            || self.stage_operation_id != proof.stage_operation_id
            || self.source_operation_id != proof.source_operation_id
            || self.source_nonce != proof.source_nonce
            || self.credential_generation != proof.credential_generation
            || self.bundle_digest != proof.bundle_digest
            || self.service_proof_digest != verified.proof_digest().map_err(|_| rejected())?
            || at < proof.issued_at.get()
            || at >= proof.expires_at.get()
        {
            return Err(rejected());
        }
        Ok(())
    }
}
pub(crate) struct ExecutionPrepareKeyV2<'a> {
    pub scope: &'a RoleKeySlotScopeV1,
    pub wrapping_version: DecimalU64,
    pub kek: &'a [u8; 32],
}
fn sign<T: Clone + Serialize>(
    domain: &str,
    request: &T,
    public: &str,
    signing: &SigningKey,
) -> Result<ws::SignedReadRequest<T>> {
    let bytes = Zeroizing::new(ws::signing_bytes(domain, request).map_err(|_| rejected())?);
    Ok(ws::SignedReadRequest {
        request: request.clone(),
        subject_public_key: public.into(),
        signature_hex: hex::encode(signing.sign(&bytes).to_bytes()),
    })
}
impl RoleKeyStore {
    /// The caller must obtain `verified` by actual configured Account pin/floor
    /// verification. A wire proof, caller Bundle, or billing receipt cannot
    /// construct it. Current is the existing private Rust reader, not serde.
    #[cfg(feature = "execution-signer-harness")]
    pub(crate) async fn sign_execution_prepare_once_v2<F>(
        &self,
        key: ExecutionPrepareKeyV2<'_>,
        verified: VerifiedExecutionPreparePermitV2,
        current: F,
    ) -> Result<ExecutionPrepareSignedOriginalV2>
    where
        F: Fn() -> Result<ExecutionPrepareSigningCurrentV2>,
    {
        self.sign_execution_prepare_with_clock_v2(key, verified, current, now)
            .await
    }
    #[cfg(feature = "execution-signer-harness")]
    async fn sign_execution_prepare_with_clock_v2<F, C>(
        &self,
        key: ExecutionPrepareKeyV2<'_>,
        verified: VerifiedExecutionPreparePermitV2,
        current: F,
        clock: C,
    ) -> Result<ExecutionPrepareSignedOriginalV2>
    where
        F: Fn() -> Result<ExecutionPrepareSigningCurrentV2>,
        C: Fn() -> Result<u64>,
    {
        if COMPILED_ROLE != Role::Executor
            || &verified.proof().permit.recipient_scope != key.scope
            || verified.proof().permit.operation != ExecutionPrepareSignerOperationV2::SignPrepare
        {
            return Err(rejected());
        }
        // No decrypt, signature, publication, or implicit state creation before
        // this standalone SQL transaction commits. Failure below never refunds.
        let claimed = self.claim_execution_prepare_v2(verified, clock()?)?;
        let before = current()?;
        before.check(&claimed.verified, key.scope, clock()?)?;
        self.sign_claimed_prepare_v2(key, claimed, before, current, clock)
            .await
    }
    async fn sign_claimed_prepare_v2<F, C>(
        &self,
        key: ExecutionPrepareKeyV2<'_>,
        claimed: ClaimedExecutionPrepareV2,
        before: ExecutionPrepareSigningCurrentV2,
        current: F,
        clock: C,
    ) -> Result<ExecutionPrepareSignedOriginalV2>
    where
        F: Fn() -> Result<ExecutionPrepareSigningCurrentV2>,
        C: Fn() -> Result<u64>,
    {
        let proof = &claimed.verified.proof().permit;
        let bundle = claimed.verified.bundle();
        let raw = read(&self.state, KEY)?;
        if raw != claimed.immutable_slot {
            return Err(rejected());
        }
        let slot = decode(&raw)?.slot.ok_or_else(rejected)?;
        slot.validate_for(key.scope).map_err(|_| rejected())?;
        if slot.public_ref != proof.recipient_key
            || slot.envelope.wrapping_key_version != key.wrapping_version
            || clock()? >= proof.expires_at.get()
        {
            return Err(rejected());
        }
        let seed = envelope_crypto::open(&slot.envelope, &key.scope.context, key.kek)
            .await
            .map_err(|_| rejected())?;
        let after_open = current()?;
        after_open.check(&claimed.verified, key.scope, clock()?)?;
        if after_open != before
            || clock()? >= proof.expires_at.get()
            || read(&self.state, KEY)? != claimed.immutable_slot
        {
            return Err(rejected());
        }
        let seed: &[u8; 32] = seed.as_bytes().try_into().map_err(|_| rejected())?;
        let signing = SigningKey::from_bytes(seed);
        let public = hex::encode(signing.verifying_key().to_bytes());
        if public != proof.recipient_key.public_key {
            return Err(rejected());
        }
        let ExecutionSigningActionV2::Prepare {
            original,
            observation_nonce,
        } = bundle.action.as_ref()
        else {
            return Err(rejected());
        };
        let output = ExecutionPrepareSignedOriginalV2 {
            access: EntryExecutorAccessV1 {
                read: sign(ws::VIEW_JSON_PATH, &bundle.observation, &public, &signing)?,
                fence: bundle.fence.clone(),
            },
            request: TaskContextPrepareAttestationRequestV2 {
                original: sign(
                    TASK_CONTEXT_PREPARE_PATH,
                    original.as_ref(),
                    &public,
                    &signing,
                )?,
                observation_nonce: observation_nonce.clone(),
            },
        };
        bundle
            .verify_prepare_original(&output)
            .map_err(|_| rejected())?;
        let before_publish = current()?;
        before_publish.check(&claimed.verified, key.scope, clock()?)?;
        if before_publish != before
            || clock()? >= proof.expires_at.get()
            || read(&self.state, KEY)? != claimed.immutable_slot
        {
            return Err(rejected());
        }
        // A local immutable public-original record is distinct from later paid
        // Account publication. Unknown failure here cannot re-open this claim.
        self.publish_execution_prepare_v2(&claimed, &output)?;
        let after_publish = current()?;
        after_publish.check(&claimed.verified, key.scope, clock()?)?;
        if after_publish != before
            || clock()? >= proof.expires_at.get()
            || read(&self.state, KEY)? != claimed.immutable_slot
        {
            return Err(rejected());
        }
        Ok(output)
    }
}

#[cfg(feature = "execution-signer-harness")]
impl RoleKeyStore {
    pub(crate) async fn execution_prepare_historical_harness_v2<F, C>(
        &self,
        key: ExecutionPrepareKeyV2<'_>,
        verified: VerifiedExecutionPreparePermitV2,
        current: F,
        clock: C,
    ) -> Result<ExecutionPrepareSignedOriginalV2>
    where
        F: Fn() -> Result<ExecutionPrepareSigningCurrentV2>,
        C: Fn() -> Result<u64>,
    {
        self.sign_execution_prepare_with_clock_v2(key, verified, current, clock)
            .await
    }
}

impl RoleKeyStore {
    /// Production sealed-grant consumer. Current here means the exact short
    /// Account-attested original window, not a new remote Native/World read.
    /// Only strict pinned verification creates these Rust values.
    pub(crate) async fn sign_execution_prepare_sealed_v2(
        &self,
        env: &worker::Env,
        verified: VerifiedExecutionPreparePermitV2,
    ) -> Result<ExecutionPrepareSignedOriginalV2> {
        let claimed = self.claim_execution_prepare_v2(verified, now()?)?;
        let proof = claimed.verified.proof().clone();
        let bundle = claimed.verified.bundle().clone();
        let scope = proof.permit.recipient_scope.clone();
        let current = || execution_prepare_sealed_current_v2(env, &proof, &bundle, now()?);
        let before = current()?;
        // Reading even the KEK is below the already committed physical claim.
        let raw = Zeroizing::new(
            env.secret("CUSTODY_ROLE_KEY_KEK")
                .map_err(|_| rejected())?
                .to_string(),
        );
        if raw.len() != 64
            || !raw
                .bytes()
                .all(|v| v.is_ascii_digit() || (b'a'..=b'f').contains(&v))
        {
            return Err(rejected());
        }
        let mut kek = Zeroizing::new([0_u8; 32]);
        hex::decode_to_slice(raw.as_bytes(), kek.as_mut_slice()).map_err(|_| rejected())?;
        let wrapping_version = serde_json::from_value::<DecimalU64>(
            env.var("CUSTODY_ROLE_WRAPPING_KEY_VERSION")
                .map_err(|_| rejected())?
                .to_string()
                .into(),
        )
        .map_err(|_| rejected())?;
        self.sign_claimed_prepare_v2(
            ExecutionPrepareKeyV2 {
                scope: &scope,
                wrapping_version,
                kek: &kek,
            },
            claimed,
            before,
            current,
            now,
        )
        .await
    }
}

/// Creates only the exact short-lived issuer-attested tuple, never a caller
/// Current, a fresh remote observation or a World/Native dispatch grant.
pub(crate) fn execution_prepare_sealed_current_v2(
    env: &worker::Env,
    proof: &oasis7_custody_core::provisioning::SignedAccountExecutionPreparePermitV2,
    bundle: &oasis7_custody_core::provisioning::ExecutionSigningBundleV2,
    at: u64,
) -> Result<ExecutionPrepareSigningCurrentV2> {
    let scope = &proof.permit.recipient_scope;
    let public = &proof.permit.recipient_key;
    let verified = ExecutionIssuerConfigurationV2::load(env)?.verify(
        proof.clone(),
        bundle.clone(),
        oasis7_custody_core::provisioning::ExecutionPrepareRecipientV2 {
            scope,
            key: public,
            operation: ExecutionPrepareSignerOperationV2::SignPrepare,
        },
        at,
    )?;
    let p = &verified.proof().permit;
    let revision = verified.proof_digest().map_err(|_| rejected())?;
    Ok(ExecutionPrepareSigningCurrentV2 {
        role: RoleKeyCurrent {
            scope: scope.clone(),
            approved: true,
            frozen: false,
            cancelled: false,
            expires_at: DecimalU64(p.expires_at.get()),
            authority_revision: revision.clone(),
        },
        session_id: p.session_id.clone(),
        approved_input_digest: p.approved_input_digest.clone(),
        action_request_id: p.action_request_id.clone(),
        stage_operation_id: p.stage_operation_id.clone(),
        source_operation_id: p.source_operation_id.clone(),
        source_nonce: p.source_nonce.clone(),
        credential_generation: p.credential_generation,
        bundle_digest: p.bundle_digest.clone(),
        service_proof_digest: revision,
    })
}
