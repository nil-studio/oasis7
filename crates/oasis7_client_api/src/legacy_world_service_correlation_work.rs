//! The only concrete browser adapter; JSON fallback is exactly the original
//! Gameplay parse-success boundary, never a resource/crypto-error fallback.
use super::*;
use crate::world_service::{PaidServiceProtocolRefusalV1, ServiceAuthorityWorkErrorV1 as Error};
use crate::world_service_correlation::work::{PaidLegacyIdentityDataV1, Phase};
use oasis7_wasm_abi::CapabilityCanonicalWorkV1;
fn proto<C>(error: oasis7_proto::player_auth::PlayerAuthWorkErrorV1<C>) -> Error<C> {
    match error {
        oasis7_proto::player_auth::PlayerAuthWorkErrorV1::Work(e) => Error::Work(e),
        oasis7_proto::player_auth::PlayerAuthWorkErrorV1::Protocol(e) => {
            Error::Protocol(PaidServiceProtocolRefusalV1 {
                message: e.message,
                fee: e.fee,
            })
        }
    }
}
pub(crate) fn derive_browser<A: CapabilityCanonicalWorkV1>(
    payload: &WorldServicePayloadV1,
    source: &A,
) -> Result<PaidLegacyIdentityDataV1<A::Charge>, Error<A::Charge>> {
    let mut phase = Phase::new(source)?;
    let value = match payload {
        WorldServicePayloadV1::GameplayJson(bytes) => {
            if let Ok(request) =
                phase.decode_json::<oasis7_proto::viewer::GameplayActionRequest>(bytes)?
            {
                let Some(proof) = request.auth.as_ref() else {
                    return phase.fail("missing gameplay proof");
                };
                let verified =
                    oasis7_proto::player_auth::verify_gameplay_action_auth_proof_with_work_v1(
                        &request, proof, source,
                    )
                    .map_err(proto)?;
                (
                    phase.copy(&verified.verified().public_key)?,
                    "gameplay",
                    "player_auth",
                    phase.decimal(verified.verified().nonce)?,
                )
            } else {
                let command =
                    match phase.decode_json::<oasis7_proto::viewer::CollectDataCommand>(bytes)? {
                        Ok(v) => v,
                        Err(e) => return Err(phase.owned_error(e.to_string())?),
                    };
                let oasis7_proto::viewer::CollectDataCommand::Submit { request } = &command else {
                    return phase.fail("unsupported collect_data operation");
                };
                let Some(proof) = request.auth.as_ref() else {
                    return phase.fail("missing collect_data proof");
                };
                let verified =
                    oasis7_proto::player_auth::verify_collect_data_auth_proof_with_work_v1(
                        &command, proof, source,
                    )
                    .map_err(proto)?;
                (
                    phase.copy(&verified.verified().public_key)?,
                    "collect_data",
                    "collect_data_submit",
                    phase.decimal(verified.verified().nonce)?,
                )
            }
        }
        WorldServicePayloadV1::AgentChat(request) => {
            let Some(proof) = request.auth.as_ref() else {
                return phase.fail("missing canonical chat proof");
            };
            let verified = oasis7_proto::player_auth::verify_agent_chat_auth_proof_with_work_v1(
                request, proof, source,
            )
            .map_err(proto)?;
            (
                phase.copy(&verified.verified().public_key)?,
                "agent_chat",
                "canonical_owner_chat",
                phase.decimal(verified.verified().nonce)?,
            )
        }
        _ => return phase.fail("legacy verifier received non-browser payload"),
    };
    Ok(phase.finish_legacy(value))
}
