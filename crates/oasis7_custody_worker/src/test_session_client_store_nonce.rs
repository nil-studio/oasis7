//! Two independent local commits within the same fixed Account RPC.
use super::*;
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use zeroize::Zeroizing;

impl OwnerProvisionApprovalStore {
    pub(crate) async fn issue_client_tool_nonce(
        &self,
        env: &Env,
        rpc: &TestSessionClientAdmissionRpc,
    ) -> Result<SessionChallengeV1> {
        self.client_proof_transaction(
            env,
            rpc,
            SessionClientOperation::ToolNonce,
            "test_session_tool_nonce_fee",
            |state, proof, time, _| {
                state
                    .charge_test_session_tool_nonce_http_attempt(
                        SessionProofRequest {
                            ingress_attempt_id: &rpc.ingress_attempt_id,
                            original_bytes: rpc.http.original.body.as_bytes(),
                            possession: proof,
                        },
                        time,
                    )
                    .map(|_| ())
                    .map_err(|_| rejected())
            },
        )
        .await?;

        // The fee is already durable. RNG, later authority or slot failures
        // cannot roll it back. No network call or private key is used here.
        let random =
            Zeroizing::new(crate::approval_crypto::verification_nonce().map_err(|_| rejected())?);
        let bytes = Zeroizing::new(hex::decode(random.as_str()).map_err(|_| rejected())?);
        let nonce = URL_SAFE_NO_PAD.encode(bytes.as_slice());
        self.client_proof_transaction(
            env,
            rpc,
            SessionClientOperation::ToolNonce,
            "test_session_tool_nonce_issued",
            |state, proof, time, _| {
                state
                    .issue_test_session_tool_nonce(
                        SessionProofRequest {
                            ingress_attempt_id: &rpc.ingress_attempt_id,
                            original_bytes: rpc.http.original.body.as_bytes(),
                            possession: proof,
                        },
                        &nonce,
                        time,
                    )
                    .map_err(|_| rejected())
            },
        )
        .await
    }
}
