//! Public selection data only. No ref, private lookup or input-existence oracle.
use super::super::wire::{decode_frame, digest, encode, encode_frame, hash};
use super::*;
pub(super) const HINT_DOMAIN: &str = "oasis7.bft.admission-input.current-view.v1";
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NodeBftAdmissionInputCurrentViewV1 {
    pub binding_digest: String,
    pub scope: oasis7_proto::distributed_bft::v2::BftScopeV2,
    pub schedule_digest: String,
    pub generation: u64,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct HintRequest {
    pub domain: String,
    pub requester_peer_id: String,
    pub serving_peer_id: String,
    pub nonce: String,
}
impl HintRequest {
    pub fn validate(&self) -> Result<(), String> {
        if self.domain != HINT_DOMAIN {
            return Err("admission hint domain rejected".into());
        }
        hash(&self.nonce)?;
        for peer in [&self.requester_peer_id, &self.serving_peer_id] {
            peer.parse::<libp2p::PeerId>()
                .map_err(|_| "admission hint Noise identity rejected")?;
        }
        Ok(())
    }
    pub fn digest(&self) -> Result<String, String> {
        Ok(digest(HINT_DOMAIN.as_bytes(), &encode(self)?))
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct HintResponse {
    pub domain: String,
    pub request_digest: String,
    pub view: NodeBftAdmissionInputCurrentViewV1,
}
impl NodeBftAdmissionInputCurrentViewV1 {
    pub fn read_from(
        network: &Libp2pNetwork,
        serving_peer: &str,
        original_nonce: &str,
        deadline: Instant,
    ) -> Result<Self, String> {
        hash(original_nonce)?;
        let serving_peer = serving_peer
            .parse::<libp2p::PeerId>()
            .map_err(|_| "admission hint serving peer rejected")?;
        let request = HintRequest {
            domain: HINT_DOMAIN.into(),
            requester_peer_id: network.peer_id().to_string(),
            serving_peer_id: serving_peer.to_string(),
            nonce: digest(
                b"oasis7.bft.admission-input.hint-nonce.v1",
                original_nonce.as_bytes(),
            ),
        };
        request.validate()?;
        let encoded = encode_frame(&request)?;
        let bytes = super::input_client::request_original_until(deadline, |remaining| {
            network.request_authenticated_to_peer_with_timeout(
                super::input_wire::PROTOCOL,
                &encoded,
                serving_peer,
                remaining,
            )
        })?;
        let response: HintResponse = decode_frame(&bytes)?;
        if Instant::now() >= deadline
            || response.domain != HINT_DOMAIN
            || response.request_digest != request.digest()?
        {
            return Err("admission hint original request/deadline differs".into());
        }
        hash(&response.view.binding_digest)?;
        hash(&response.view.schedule_digest)?;
        response.view.scope.validate()?;
        Ok(response.view)
    }
}
