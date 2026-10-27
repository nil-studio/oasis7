//! Owner-only closed two-domain signing. The Account commits the candidate before publication.
use super::*;
use ed25519_dalek::Signer;
use oasis7_agent_api::game::GAME_PARTICIPANT_REGISTER_DOMAIN;
use oasis7_client_api::world_service::{self as ws, SignedReadRequest};
use oasis7_custody_core::provisioning::{ParticipantDescriptorV1, ParticipantSignBundleV1};
pub(crate) struct ParticipantSigningKey<'a> {
    pub scope: &'a RoleKeySlotScopeV1,
    pub wrapping_version: DecimalU64,
    pub kek: &'a [u8; 32],
}
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct ParticipantSigningCurrent {
    pub role: RoleKeyCurrent,
    pub participant_permit_id: String,
    pub bundle_digest: String,
}
pub(crate) fn participant_bundle_digest(descriptor: &ParticipantDescriptorV1) -> Result<String> {
    let bytes = oasis7_wasm_abi::encode_canonical_cbor(&(
        "oasis7.custody.participant-fixed-signing.v1",
        &descriptor.owner_scope,
        &descriptor.owner_key,
        &descriptor.register,
        &descriptor.owner_read,
    ))
    .map_err(|_| rejected())?;
    Ok(oasis7_custody_core::approval::bytes_digest(&bytes))
}
impl RoleKeyStore {
    pub(crate) async fn sign_participant_bundle<F, P, U>(
        &self,
        key: ParticipantSigningKey<'_>,
        descriptor: &ParticipantDescriptorV1,
        current: F,
        publish: P,
    ) -> Result<ParticipantSignBundleV1>
    where
        F: Fn() -> Result<ParticipantSigningCurrent>,
        P: FnOnce(ParticipantSignBundleV1) -> U,
        U: Future<Output = Result<ParticipantSignBundleV1>>,
    {
        let before = current()?;
        before.role.check(key.scope, now()?)?;
        if COMPILED_ROLE != Role::Owner
            || descriptor.owner_scope != *key.scope
            || before.participant_permit_id.is_empty()
            || before.bundle_digest != participant_bundle_digest(descriptor)?
            || descriptor.register.owner_public_key != descriptor.owner_key.public_key
            || descriptor.register.owner_key_version != descriptor.owner_key.key_version
        {
            return Err(rejected());
        }
        let original = read(&self.state, KEY)?;
        let slot = decode(&original)?.slot.ok_or_else(rejected)?;
        slot.validate_for(key.scope).map_err(|_| rejected())?;
        if slot.public_ref != descriptor.owner_key
            || slot.envelope.wrapping_key_version != key.wrapping_version
        {
            return Err(rejected());
        }
        let seed = envelope_crypto::open(&slot.envelope, &key.scope.context, key.kek)
            .await
            .map_err(|_| rejected())?;
        if public(seed.as_bytes())? != descriptor.owner_key.public_key
            || current()? != before
            || read(&self.state, KEY)? != original
        {
            return Err(rejected());
        }
        before.role.check(key.scope, now()?)?;
        let secret: &[u8; 32] = seed.as_bytes().try_into().map_err(|_| rejected())?;
        let signing = SigningKey::from_bytes(secret);
        let candidate = ParticipantSignBundleV1 {
            owner: SignedReadRequest {
                request: descriptor.register.clone(),
                subject_public_key: descriptor.owner_key.public_key.clone(),
                signature_hex: hex::encode(
                    signing
                        .sign(
                            &ws::signing_bytes(
                                GAME_PARTICIPANT_REGISTER_DOMAIN,
                                &descriptor.register,
                            )
                            .map_err(|_| rejected())?,
                        )
                        .to_bytes(),
                ),
            },
            read: SignedReadRequest {
                request: descriptor.owner_read.clone(),
                subject_public_key: descriptor.owner_key.public_key.clone(),
                signature_hex: hex::encode(
                    signing
                        .sign(
                            &ws::signing_bytes(ws::VIEW_JSON_PATH, &descriptor.owner_read)
                                .map_err(|_| rejected())?,
                        )
                        .to_bytes(),
                ),
            },
        };
        // This is the third, prepaid signing oracle. It verifies and commits the
        // exact two-domain candidate with current source/Fence in one SQL step.
        let committed = publish(candidate.clone()).await?;
        before.role.check(key.scope, now()?)?;
        if committed != candidate || current()? != before || read(&self.state, KEY)? != original {
            return Err(rejected());
        }
        Ok(committed)
    }
}
