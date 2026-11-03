//! Explicit v2 selector only. No Ref, private lookup or grant is returned.
use super::super::wire::{decode_frame, digest, encode, encode_frame, hash};
use super::batch_wire::PROTOCOL_V2;
use super::*;
pub(super) const HINT_V2: &str = "oasis7.bft.admission-input.current-view.v2";
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct HintRequestV2 {
    pub domain: String,
    pub requester_peer_id: String,
    pub serving_peer_id: String,
    pub nonce: String,
}
impl HintRequestV2 {
    pub fn validate(&self) -> Result<(), String> {
        if self.domain != HINT_V2 {
            return Err("admission v2 selector domain unsupported".into());
        }
        hash(&self.nonce)?;
        for peer in [&self.requester_peer_id, &self.serving_peer_id] {
            if peer.len() > 128 {
                return Err("admission v2 selector peer bound".into());
            }
            peer.parse::<libp2p::PeerId>()
                .map_err(|_| "admission v2 selector Noise peer")?;
        }
        if self.requester_peer_id == self.serving_peer_id {
            return Err("admission v2 selector endpoints equal".into());
        }
        Ok(())
    }
    pub fn digest(&self) -> Result<String, String> {
        self.validate()?;
        Ok(digest(HINT_V2.as_bytes(), &encode(self)?))
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct HintResponseV2 {
    pub domain: String,
    pub request_digest: String,
    pub view: NodeBftAdmissionInputCurrentViewV1,
}
impl NodeBftAdmissionInputCurrentViewV1 {
    /// The v2 namespace fixes binary parts, count<=3 and the original 16KiB
    /// layout. Unsupported is returned; there is no V1 retry/negotiation branch.
    pub fn read_from_v2(
        network: &Libp2pNetwork,
        serving_peer: &str,
        original_nonce: &str,
        deadline: Instant,
    ) -> Result<Self, String> {
        hash(original_nonce)?;
        let serving_peer = serving_peer
            .parse::<libp2p::PeerId>()
            .map_err(|_| "admission v2 selector serving peer")?;
        let request = HintRequestV2 {
            domain: HINT_V2.into(),
            requester_peer_id: network.peer_id().to_string(),
            serving_peer_id: serving_peer.to_string(),
            // Same derived nonce as v1: a version switch cannot obtain a second
            // deadline from the shared per-peer nonce ledger.
            nonce: digest(
                b"oasis7.bft.admission-input.hint-nonce.v1",
                original_nonce.as_bytes(),
            ),
        };
        request.validate()?;
        let bytes = encode_frame(&request)?;
        let reply = super::input_client::request_original_until(deadline, |remaining| {
            network.request_authenticated_to_peer_with_timeout(
                PROTOCOL_V2,
                &bytes,
                serving_peer,
                remaining,
            )
        })?;
        let response: HintResponseV2 = decode_frame(&reply)?;
        if Instant::now() >= deadline
            || response.domain != HINT_V2
            || response.request_digest != request.digest()?
        {
            return Err("admission v2 selector reply unsupported or changed".into());
        }
        hash(&response.view.binding_digest)?;
        hash(&response.view.schedule_digest)?;
        response.view.scope.validate()?;
        Ok(response.view)
    }
}
