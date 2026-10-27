//! Owner-only fixed setup domains. Candidate publication grants preflight reads, not effects.
use super::*;
use ed25519_dalek::Signer;
use oasis7_agent_api::game::{
    GAME_EXECUTOR_CONTROL_DOMAIN, GAME_GOAL_PREPARE_DOMAIN, GAME_GOAL_PUBLISH_DOMAIN,
    GAME_TASK_CREATE_DOMAIN,
};
use oasis7_client_api::{
    world_service as ws,
    world_service_wire::{AuthenticatedLookup, WorldServicePayloadV1},
};
use oasis7_custody_core::provisioning::{
    OwnerSetupCandidateV1, OwnerSetupOriginalV1, OwnerSetupSigningBundleV1,
    OwnerSetupSigningRequestV1,
};

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct SetupSigningCurrent {
    pub role: RoleKeyCurrent,
    pub setup_id: String,
    pub bundle_digest: String,
}
pub(crate) fn setup_bundle_digest(bundle: &OwnerSetupSigningBundleV1) -> Result<String> {
    let bytes = zeroize::Zeroizing::new(
        oasis7_wasm_abi::encode_canonical_cbor(&("oasis7.custody.owner-setup-signing.v1", bundle))
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
fn candidate(
    bundle: &OwnerSetupSigningBundleV1,
    world: &oasis7_custody_core::approval::ApprovedWorldV1,
    deadline: u64,
    key: &str,
    signing: &SigningKey,
) -> Result<OwnerSetupCandidateV1> {
    let mut observation = bundle.participant_access.clone();
    observation.read = sign(ws::VIEW_JSON_PATH, &bundle.observation, key, signing)?;
    observation.response_profile =
        Some(ws::participant_discovery::ParticipantOwnerViewProfileV1::OwnerSetupV1);
    let payload = match &bundle.action {
        OwnerSetupSigningRequestV1::CreateTask(r) => Some(WorldServicePayloadV1::GameTaskCreate(
            sign(GAME_TASK_CREATE_DOMAIN, r, key, signing)?,
        )),
        OwnerSetupSigningRequestV1::PublishGoal(r) => Some(WorldServicePayloadV1::GameGoalPublish(
            sign(GAME_GOAL_PUBLISH_DOMAIN, r, key, signing)?,
        )),
        OwnerSetupSigningRequestV1::Delegation(r) => Some(WorldServicePayloadV1::Delegation(sign(
            "delegation",
            r,
            key,
            signing,
        )?)),
        OwnerSetupSigningRequestV1::Bind(r) => Some(WorldServicePayloadV1::GameExecutorControl(
            sign(GAME_EXECUTOR_CONTROL_DOMAIN, r, key, signing)?,
        )),
        _ => None,
    };
    let original = if let Some(payload) = payload {
        let correlation = oasis7_client_api::world_service_correlation::derive_correlation(
            ws::WorldIdentity {
                world_id: world.world_id.clone(),
                genesis_digest: world.genesis_digest.clone(),
            },
            &payload,
        )
        .map_err(|_| rejected())?;
        let lookup = AuthenticatedLookup {
            request: ws::LookupIntentRequest {
                contract_version: ws::WORLD_SERVICE_CONTRACT_VERSION,
                key: correlation.key.clone(),
            },
            original: payload.clone(),
        };
        OwnerSetupOriginalV1::Canonical {
            submit: Box::new(ws::SubmitIntentRequest {
                contract_version: ws::WORLD_SERVICE_CONTRACT_VERSION,
                correlation,
                deadline_unix_ms: Some(deadline),
                signed_payload: payload,
            }),
            lookup: Box::new(lookup),
        }
    } else {
        match &bundle.action {
            OwnerSetupSigningRequestV1::PrepareGoal(r) => {
                let proof = sign(GAME_GOAL_PREPARE_DOMAIN, r, key, signing)?;
                OwnerSetupOriginalV1::PrepareGoal(Box::new(ws::private_goal::SignedGoalPrepareV1 {
                    request: proof.request,
                    subject_public_key: proof.subject_public_key,
                    signature_hex: proof.signature_hex,
                }))
            }
            OwnerSetupSigningRequestV1::FinalObservation { owner, goal } => {
                if owner.as_ref() != &bundle.observation {
                    return Err(rejected());
                }
                OwnerSetupOriginalV1::FinalObservation {
                    owner: Box::new(observation.clone()),
                    goal: Box::new(sign(
                        ws::private_goal_read::GOAL_CURRENT_PATH,
                        goal,
                        key,
                        signing,
                    )?),
                }
            }
            _ => return Err(rejected()),
        }
    };
    Ok(OwnerSetupCandidateV1 {
        original,
        observation,
    })
}
impl RoleKeyStore {
    pub(crate) async fn sign_setup<F, P, U>(
        &self,
        key: super::ParticipantSigningKey<'_>,
        bundle: &OwnerSetupSigningBundleV1,
        world: &oasis7_custody_core::approval::ApprovedWorldV1,
        deadline: u64,
        current: F,
        publish: P,
    ) -> Result<OwnerSetupCandidateV1>
    where
        F: Fn() -> Result<SetupSigningCurrent>,
        P: FnOnce(OwnerSetupCandidateV1) -> U,
        U: Future<Output = Result<OwnerSetupCandidateV1>>,
    {
        let before = current()?;
        before.role.check(key.scope, now()?)?;
        if COMPILED_ROLE != Role::Owner
            || before.setup_id.is_empty()
            || before.bundle_digest != setup_bundle_digest(bundle)?
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
        let original = candidate(
            bundle,
            world,
            deadline,
            &slot.public_ref.public_key,
            &SigningKey::from_bytes(secret),
        )?;
        let committed = publish(original.clone()).await?;
        before.role.check(key.scope, now()?)?;
        if committed != original || current()? != before || read(&self.state, KEY)? != saved {
            return Err(rejected());
        }
        Ok(committed)
    }
}
