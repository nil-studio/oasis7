//! Fixed Owner VIEW_JSON read for a consumed TestSession preflight. No write domain.
use super::*;
use crate::owner_test_session_rpc::TestSessionSignerPermit;
use ed25519_dalek::Signer;
use oasis7_client_api::world_service as ws;
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct TestSessionSigningCurrent {
    pub role: RoleKeyCurrent,
    pub session_id: String,
    pub permit_digest: String,
}
pub(crate) fn test_session_signing_digest(view: &TestSessionSignerPermit) -> Result<String> {
    let bytes = Zeroizing::new(
        oasis7_wasm_abi::encode_canonical_cbor(&(
            "oasis7.custody.test-session-owner-read-signing.v1",
            view,
        ))
        .map_err(|_| rejected())?,
    );
    Ok(oasis7_custody_core::approval::bytes_digest(&bytes))
}
impl RoleKeyStore {
    pub(crate) async fn sign_test_session_read<F, P, U>(
        &self,
        key: ParticipantSigningKey<'_>,
        view: &TestSessionSignerPermit,
        current: F,
        publish: P,
    ) -> Result<ws::participant_discovery::ParticipantOwnerViewRequestV1>
    where
        F: Fn() -> Result<TestSessionSigningCurrent>,
        P: FnOnce(ws::participant_discovery::ParticipantOwnerViewRequestV1) -> U,
        U: Future<Output = Result<ws::participant_discovery::ParticipantOwnerViewRequestV1>>,
    {
        let before = current()?;
        before.role.check(key.scope, now()?)?;
        let permit = &view.permit;
        if COMPILED_ROLE != Role::Owner
            || permit.ticket != 4
            || before.session_id != permit.descriptor.session_id
            || before.permit_digest != test_session_signing_digest(view)?
            || view.signing.read != permit.descriptor.owner_read
        {
            return Err(rejected());
        }
        let saved = read(&self.state, KEY)?;
        let slot = decode(&saved)?.slot.ok_or_else(rejected)?;
        slot.validate_for(key.scope).map_err(|_| rejected())?;
        if slot.envelope.wrapping_key_version != key.wrapping_version
            || slot.public_ref != permit.descriptor.owner_key
        {
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
        let bytes = Zeroizing::new(
            ws::signing_bytes(ws::VIEW_JSON_PATH, &view.signing.read).map_err(|_| rejected())?,
        );
        let candidate = ws::participant_discovery::ParticipantOwnerViewRequestV1 {
            owner: view.signing.owner.clone(),
            admission: view.signing.admission.clone(),
            read: ws::SignedReadRequest {
                request: view.signing.read.clone(),
                subject_public_key: slot.public_ref.public_key.clone(),
                signature_hex: hex::encode(signing.sign(&bytes).to_bytes()),
            },
            response_profile: Some(
                ws::participant_discovery::ParticipantOwnerViewProfileV1::OwnerSetupV1,
            ),
        };
        let committed = publish(candidate.clone()).await?;
        before.role.check(key.scope, now()?)?;
        if committed != candidate || current()? != before || read(&self.state, KEY)? != saved {
            return Err(rejected());
        }
        Ok(committed)
    }
}
