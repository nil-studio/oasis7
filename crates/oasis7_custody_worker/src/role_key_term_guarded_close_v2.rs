//! Independent Close purpose, sharing only the permanent dual-purpose inventory.
use super::*;
use oasis7_agent_api::game::{GAME_TASK_CLEANUP_DOMAIN_V2, GameTaskRevokeAndCloseV2};
use oasis7_client_api::world_service::SignedReadRequest;
use oasis7_custody_core::term_guarded_close_signer_v2::SignedTermGuardedCloseSignerPermitV2;
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Claim {
    pub(super) id: String,
    pub(super) permit: SignedTermGuardedCloseSignerPermitV2,
    pub(super) claimed_at: DecimalU64,
    #[serde(deserialize_with = "required")]
    pub(super) signed_at: Option<DecimalU64>,
    #[serde(deserialize_with = "required")]
    pub(super) original: Option<SignedReadRequest<GameTaskRevokeAndCloseV2>>,
}
fn id(permit: &SignedTermGuardedCloseSignerPermitV2) -> Result<String> {
    let p = &permit.payload;
    oasis7_wasm_abi::canonical_hash(&(
        "oasis7.custody.term-owner-signer-once.v2",
        &p.issuer.namespace,
        &p.issuer.service_id,
        &p.issuer.environment,
        &p.paid_attempt_id,
        &p.recipient_path,
        &p.owner_scope,
    ))
    .map_err(|_| rejected())
}
fn check_original(c: &Claim) -> Result<()> {
    match (&c.original, c.signed_at) {
        (None, None) => Ok(()),
        (Some(original), Some(at)) => {
            original.request.validate().map_err(|_| rejected())?;
            if original.request != c.permit.payload.original
                || original.subject_public_key != c.permit.payload.owner_key.public_key
                || at.0 < c.claimed_at.0
                || at.0 >= c.permit.payload.expires_at.0
            {
                return Err(rejected());
            }
            oasis7_client_api::world_service::private_goal::verify_owner_signature_strict(
                GAME_TASK_CLEANUP_DOMAIN_V2,
                &original.request,
                &original.subject_public_key,
                &original.signature_hex,
            )
            .map_err(|_| rejected())
        }
        _ => Err(rejected()),
    }
}

pub(super) fn check_claim(
    c: &Claim,
    slot: &RoleKeySlotV1,
    pin: &AccountMachineIssuerPinV2,
    floor: &AccountMachineIssuerFloorV2,
) -> Result<()> {
    if c.id != id(&c.permit)? {
        return Err(rejected());
    }
    c.permit
        .verify(pin, floor, &slot.scope, &slot.public_ref, c.claimed_at.0)
        .map_err(|_| rejected())?;
    check_original(c)
}
impl RoleKeyStore {
    pub(crate) async fn sign_term_guarded_close_v2(
        &self,
        env: &Env,
        permit: SignedTermGuardedCloseSignerPermitV2,
        config: &TermGuardedSignerConfigurationV2,
    ) -> Result<SignedReadRequest<GameTaskRevokeAndCloseV2>> {
        self.sign_term_guarded_close_with_loan_v2(permit, config, || load_material(env))
            .await
    }
    #[cfg(feature = "approval-harness")]
    pub(crate) async fn test_sign_term_guarded_close_with_loan_v2<K>(
        &self,
        permit: SignedTermGuardedCloseSignerPermitV2,
        config: &TermGuardedSignerConfigurationV2,
        loan: K,
    ) -> Result<SignedReadRequest<GameTaskRevokeAndCloseV2>>
    where
        K: FnOnce() -> Result<(DecimalU64, [u8; 32])>,
    {
        self.sign_term_guarded_close_with_loan_v2(permit, config, || {
            let (wrapping_version, kek) = loan()?;
            Ok(TermSignerKeyMaterialV2 {
                wrapping_version,
                kek: Zeroizing::new(kek),
            })
        })
        .await
    }
    async fn sign_term_guarded_close_with_loan_v2<K>(
        &self,
        permit: SignedTermGuardedCloseSignerPermitV2,
        config: &TermGuardedSignerConfigurationV2,
        loan: K,
    ) -> Result<SignedReadRequest<GameTaskRevokeAndCloseV2>>
    where
        K: FnOnce() -> Result<TermSignerKeyMaterialV2>,
    {
        let claim_id = id(&permit)?;
        let (pin, pinned_floor) = config.close.as_ref().ok_or_else(rejected)?;
        let mut callback = |raw: String| -> Result<String> {
            let checked = check_inventory(&raw, config)?;
            let at = clock()?;
            if at < checked.clock_floor
                || checked.claims.len() + checked.close_claims.len() >= config.maximum()
                || checked
                    .claims
                    .iter()
                    .any(|c| c.permit.payload.paid_attempt_id == permit.payload.paid_attempt_id)
                || checked
                    .lineage
                    .as_ref()
                    .is_some_and(|v| Some(v) != permit.payload.account_lineage.as_ref())
                || checked.close_claims.iter().any(|c| c.id == claim_id)
                || checked
                    .qualifications
                    .iter()
                    .any(|c| c.request.paid_attempt_id == permit.payload.paid_attempt_id)
            {
                return Err(rejected());
            }
            permit
                .verify(
                    pin,
                    pinned_floor,
                    &checked.slot.scope,
                    &checked.slot.public_ref,
                    at,
                )
                .map_err(|_| rejected())?;
            json(&Claim {
                id: claim_id.clone(),
                permit: permit.clone(),
                claimed_at: DecimalU64(at),
                signed_at: None,
                original: None,
            })
        };
        let function = Closure::borrow_mut(&mut callback);
        // The FFI returns only after SQL has durably committed the unique claim.
        let raw = claim_once(&self.state, function.as_ref().unchecked_ref())?;
        drop(function);
        let slot = decode(&raw)?.slot.ok_or_else(rejected)?;
        permit
            .verify(pin, pinned_floor, &slot.scope, &slot.public_ref, clock()?)
            .map_err(|_| rejected())?;
        // Secret access is deliberately after the durable claim above.
        let material = loan()?;
        if slot.envelope.wrapping_key_version != material.wrapping_version {
            return Err(rejected());
        }
        let seed = envelope_crypto::open(
            &slot.envelope,
            &slot.scope.context,
            material.kek.as_slice().try_into().map_err(|_| rejected())?,
        )
        .await
        .map_err(|_| rejected())?;
        if public(seed.as_bytes())? != slot.public_ref.public_key {
            return Err(rejected());
        }
        let before_sign = clock()?;
        let current = check_inventory(&read_state(&self.state)?, config)?;
        if current.slot != slot
            || before_sign < current.clock_floor
            || !current
                .close_claims
                .iter()
                .any(|c| c.id == claim_id && c.permit == permit && c.original.is_none())
        {
            return Err(rejected());
        }
        permit
            .verify(
                pin,
                pinned_floor,
                &slot.scope,
                &slot.public_ref,
                before_sign,
            )
            .map_err(|_| rejected())?;
        let signing = SigningKey::from_bytes(seed.as_bytes().try_into().map_err(|_| rejected())?);
        let bytes = Zeroizing::new(
            oasis7_client_api::world_service::signing_bytes(
                GAME_TASK_CLEANUP_DOMAIN_V2,
                &permit.payload.original,
            )
            .map_err(|_| rejected())?,
        );
        let original = SignedReadRequest {
            request: permit.payload.original.clone(),
            subject_public_key: slot.public_ref.public_key.clone(),
            signature_hex: hex::encode(signing.sign(&bytes).to_bytes()),
        };
        let saved_original = original.clone();
        let mut finish = |raw: String| -> Result<String> {
            let checked = check_inventory(&raw, config)?;
            if checked.slot != slot || clock()? < checked.clock_floor {
                return Err(rejected());
            }
            let mut claim = checked
                .close_claims
                .into_iter()
                .find(|c| c.id == claim_id)
                .ok_or_else(rejected)?;
            if claim.permit != permit || claim.original.is_some() {
                return Err(rejected());
            }
            claim.signed_at = Some(DecimalU64(before_sign));
            claim.original = Some(saved_original.clone());
            check_original(&claim)?;
            json(&claim)
        };
        let function = Closure::borrow_mut(&mut finish);
        record_original(&self.state, &claim_id, function.as_ref().unchecked_ref())?;
        drop(function);
        Ok(original)
    }
}
