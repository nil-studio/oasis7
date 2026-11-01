//! Owner-only fixed Resource final domains; no raw byte signing or legacy result.
use super::*;
use crate::owner_setup_rpc::OwnerSetupResourceSigningBundleV2 as OwnerResourceSigningBundleV2;
use ed25519_dalek::Signer;
use oasis7_client_api::world_service::{self as ws, resource_basis as resource};
use oasis7_custody_core::provisioning::OwnerSetupResourceCandidateV2;
use serde::Serialize;
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct OwnerResourceSigningCurrentV2 {
    pub role: RoleKeyCurrent,
    pub setup_id: String,
    pub bundle_digest: String,
}
pub(crate) fn owner_resource_bundle_digest_v2(
    bundle: &OwnerResourceSigningBundleV2,
) -> Result<String> {
    let raw = zeroize::Zeroizing::new(
        oasis7_wasm_abi::encode_canonical_cbor(&(
            "oasis7.custody.owner-resource-signing-bundle.v2",
            bundle,
        ))
        .map_err(|_| rejected())?,
    );
    Ok(oasis7_custody_core::approval::bytes_digest(&raw))
}
fn signed<T: Clone + Serialize>(
    path: &str,
    request: &T,
    key: &str,
    signing: &SigningKey,
) -> Result<ws::SignedReadRequest<T>> {
    let bytes = zeroize::Zeroizing::new(ws::signing_bytes(path, request).map_err(|_| rejected())?);
    Ok(ws::SignedReadRequest {
        request: request.clone(),
        subject_public_key: key.into(),
        signature_hex: hex::encode(signing.sign(&bytes).to_bytes()),
    })
}
impl RoleKeyStore {
    pub(crate) async fn sign_owner_resource_final_v2<F, P, U>(
        &self,
        key: ParticipantSigningKey<'_>,
        bundle: &OwnerResourceSigningBundleV2,
        current: F,
        publish: P,
    ) -> Result<OwnerSetupResourceCandidateV2>
    where
        F: Fn() -> Result<OwnerResourceSigningCurrentV2>,
        P: FnOnce(OwnerSetupResourceCandidateV2) -> U,
        U: Future<Output = Result<OwnerSetupResourceCandidateV2>>,
    {
        let before = current()?;
        before.role.check(key.scope, now()?)?;
        bundle.request.validate().map_err(|_| rejected())?;
        if COMPILED_ROLE != Role::Owner
            || before.setup_id != bundle.request.setup_id
            || before.bundle_digest != owner_resource_bundle_digest_v2(bundle)?
            || bundle.owner_public_key != bundle.request.expected_fence.owner_public_key
            || now()? >= bundle.valid_until.0
        {
            return Err(rejected());
        }
        let saved = read(&self.state, KEY)?;
        let slot = decode(&saved)?.slot.ok_or_else(rejected)?;
        slot.validate_for(key.scope).map_err(|_| rejected())?;
        if slot.envelope.wrapping_key_version != key.wrapping_version
            || slot.public_ref.public_key != bundle.owner_public_key
        {
            return Err(rejected());
        }
        let seed = envelope_crypto::open(&slot.envelope, &key.scope.context, key.kek)
            .await
            .map_err(|_| rejected())?;
        if public(seed.as_bytes())? != slot.public_ref.public_key
            || current()? != before
            || read(&self.state, KEY)? != saved
            || now()? >= bundle.valid_until.0
        {
            return Err(rejected());
        }
        before.role.check(key.scope, now()?)?;
        let secret: &[u8; 32] = seed.as_bytes().try_into().map_err(|_| rejected())?;
        let signing = SigningKey::from_bytes(secret);
        let mut observation = bundle.participant_access.clone();
        observation.read = signed(
            ws::VIEW_JSON_PATH,
            &bundle.observation,
            &slot.public_ref.public_key,
            &signing,
        )?;
        observation.response_profile =
            Some(ws::participant_discovery::ParticipantOwnerViewProfileV1::OwnerSetupV1);
        let original = signed(
            resource::RESOURCE_GOAL_PATH_V2,
            &bundle.request,
            &slot.public_ref.public_key,
            &signing,
        )?
        .into();
        let candidate = OwnerSetupResourceCandidateV2 {
            original,
            observation,
        };
        let committed = publish(candidate.clone()).await?;
        before.role.check(key.scope, now()?)?;
        if committed != candidate
            || current()? != before
            || read(&self.state, KEY)? != saved
            || now()? >= bundle.valid_until.0
        {
            return Err(rejected());
        }
        Ok(committed)
    }
}
