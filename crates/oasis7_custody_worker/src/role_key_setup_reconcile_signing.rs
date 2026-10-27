//! Owner-only reads for a separately paid setup reconciliation. No write domain.
use super::*;
use ed25519_dalek::Signer;
use oasis7_client_api::world_service as ws;
use oasis7_custody_core::provisioning::{
    OwnerSetupReconcilePermitV1, OwnerSetupReconcileReadCandidateV1,
};

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct SetupReconcileSigningCurrent {
    pub role: RoleKeyCurrent,
    pub reconcile_id: String,
    pub attempt_id: String,
    pub permit_digest: String,
}
pub(crate) fn reconcile_permit_digest(permit: &OwnerSetupReconcilePermitV1) -> Result<String> {
    let bytes = zeroize::Zeroizing::new(
        oasis7_wasm_abi::encode_canonical_cbor(&(
            "oasis7.custody.owner-setup-reconcile-read-signing.v1",
            permit,
        ))
        .map_err(|_| rejected())?,
    );
    Ok(oasis7_custody_core::approval::bytes_digest(&bytes))
}
fn sign<T: Clone + serde::Serialize>(
    domain: &str,
    request: &T,
    key: &str,
    signing: &SigningKey,
) -> Result<ws::SignedReadRequest<T>> {
    let bytes =
        zeroize::Zeroizing::new(ws::signing_bytes(domain, request).map_err(|_| rejected())?);
    Ok(ws::SignedReadRequest {
        request: request.clone(),
        subject_public_key: key.into(),
        signature_hex: hex::encode(signing.sign(&bytes).to_bytes()),
    })
}
impl RoleKeyStore {
    pub(crate) async fn sign_setup_reconcile<F, P, U>(
        &self,
        key: super::ParticipantSigningKey<'_>,
        permit: &OwnerSetupReconcilePermitV1,
        current: F,
        publish: P,
    ) -> Result<OwnerSetupReconcileReadCandidateV1>
    where
        F: Fn() -> Result<SetupReconcileSigningCurrent>,
        P: FnOnce(OwnerSetupReconcileReadCandidateV1) -> U,
        U: Future<Output = Result<OwnerSetupReconcileReadCandidateV1>>,
    {
        let before = current()?;
        before.role.check(key.scope, now()?)?;
        if COMPILED_ROLE != Role::Owner
            || before.reconcile_id != permit.reconcile_id
            || before.attempt_id != permit.attempt_id
            || before.permit_digest != reconcile_permit_digest(permit)?
            || permit.reconcile_id.is_empty()
            || permit.attempt_id.is_empty()
        {
            return Err(rejected());
        }
        let saved = read(&self.state, KEY)?;
        let slot = decode(&saved)?.slot.ok_or_else(rejected)?;
        slot.validate_for(key.scope).map_err(|_| rejected())?;
        if slot.envelope.wrapping_key_version != key.wrapping_version {
            return Err(rejected());
        }
        let seed = envelope_crypto::open(&slot.envelope, &key.scope.context, key.kek)
            .await
            .map_err(|_| rejected())?;
        if public(seed.as_bytes())? != slot.public_ref.public_key
            || current()? != before
            || read(&self.state, KEY)? != saved
        {
            return Err(rejected());
        }
        before.role.check(key.scope, now()?)?;
        let secret: &[u8; 32] = seed.as_bytes().try_into().map_err(|_| rejected())?;
        let signing = SigningKey::from_bytes(secret);
        let mut observation = permit.participant_access.clone();
        observation.read = sign(
            ws::VIEW_JSON_PATH,
            &permit.observation,
            &slot.public_ref.public_key,
            &signing,
        )?;
        observation.response_profile =
            Some(ws::participant_discovery::ParticipantOwnerViewProfileV1::OwnerSetupV1);
        let candidate = OwnerSetupReconcileReadCandidateV1 {
            observation,
            goal: permit
                .final_goal
                .as_ref()
                .map(|goal| {
                    sign(
                        ws::private_goal_read::GOAL_CURRENT_PATH,
                        goal,
                        &slot.public_ref.public_key,
                        &signing,
                    )
                })
                .transpose()?,
        };
        let committed = publish(candidate.clone()).await?;
        before.role.check(key.scope, now()?)?;
        if committed != candidate || current()? != before || read(&self.state, KEY)? != saved {
            return Err(rejected());
        }
        Ok(committed)
    }
}
