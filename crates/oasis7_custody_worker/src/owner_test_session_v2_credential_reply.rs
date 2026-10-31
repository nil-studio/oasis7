//! Only the committed once winner returns freshly generated tokens.
use super::*;
use crate::{
    provision_approval_store::native::client::ClientLiveCurrent, test_session_client_response::*,
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use oasis7_custody_core::{client_enrollment::ClientCredentialClaimRequestV1, provisioning::*};
use zeroize::Zeroizing;

pub(super) async fn claim(
    env: &Env,
    store: &OwnerProvisionApprovalStore,
    rpc: &TestSessionClientAdmissionRpc,
    current: ClientLiveCurrent,
) -> Result<String> {
    let request: ClientCredentialClaimRequestV1 = serde_json::from_str(&rpc.http.original.body)?;
    request.validate().map_err(|_| denied())?;
    let mut tokens = Vec::new();
    let mut allocations = Vec::new();
    for purpose in &request.purposes {
        let purpose = match purpose {
            oasis7_custody_core::client_enrollment::TestSessionPurposeV1::Execute => {
                SessionPurposeV1::Execution
            }
            oasis7_custody_core::client_enrollment::TestSessionPurposeV1::Read => {
                SessionPurposeV1::ReconcileRead
            }
            _ => return Err(denied()),
        };
        let random =
            Zeroizing::new(crate::approval_crypto::verification_nonce().map_err(|_| denied())?);
        let raw = Zeroizing::new(hex::decode(random.as_str()).map_err(|_| denied())?);
        if raw.len() != 32 {
            return Err(denied());
        }
        let token = Zeroizing::new(URL_SAFE_NO_PAD.encode(raw.as_slice()));
        if token.len() != 43 {
            return Err(denied());
        }
        let digest =
            hex::encode(oasis7_custody_core::http_pop::token_digest(&token).map_err(|_| denied())?);
        allocations.push(SessionCredentialAllocation { purpose, digest });
        tokens.push((purpose, token));
    }
    let outcome = store
        .complete_credential_v2_claim(env, rpc, &allocations, || Ok(current.clone()))
        .await
        .map_err(|_| denied())?;
    if !outcome.newly_claimed {
        return json(ClientCredentialReply::Historical {
            api_version: 2,
            session_id: rpc.http.selector.session_id.clone(),
            request_id: request.request_id,
            credentials: outcome
                .roots
                .into_iter()
                .map(|(purpose, generation)| HistoricalClientCredential {
                    purpose,
                    generation,
                })
                .collect(),
        });
    }
    if outcome.roots.len() != tokens.len() {
        return Err(denied());
    }
    let mut credentials = Vec::new();
    for (purpose, token) in tokens {
        let generation = outcome
            .roots
            .iter()
            .find(|(p, _)| *p == purpose)
            .map(|(_, g)| *g)
            .ok_or_else(denied)?;
        credentials.push(ClaimedClientCredential {
            purpose,
            generation,
            token: PrivateClientToken::new(token),
        });
    }
    json(ClientCredentialReply::Claimed {
        api_version: 2,
        session_id: rpc.http.selector.session_id.clone(),
        request_id: request.request_id,
        credentials,
    })
}
