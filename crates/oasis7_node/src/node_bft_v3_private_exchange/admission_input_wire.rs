use super::super::wire::{CHUNK, digest, encode, hash};
use super::*;
pub(super) const PROTOCOL: &str = "/oasis7/bft/admission-input/1.0.0";
pub(super) const DOMAIN: &str = "oasis7.bft.admission-input.exchange.v1";
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Request {
    pub domain: String,
    pub read: NodeBftAdmissionInputReadRequestV1,
    pub authorization: NodeBftAdmissionInputAuthorizationV1,
    pub chunk_index: Option<u32>,
}
impl Request {
    pub fn validate(&self) -> Result<(), String> {
        if self.domain != DOMAIN {
            return Err("admission request domain".into());
        }
        self.read
            .reference
            .validate()
            .map_err(|_| "admission reference rejected")?;
        hash(&self.read.nonce)?;
        for peer in [&self.read.requester_peer_id, &self.read.serving_peer_id] {
            peer.parse::<libp2p::PeerId>()
                .map_err(|_| "admission Noise peer identity")?;
        }
        match (&self.read.operation, self.chunk_index) {
            (BftCandidateReadOperationV3::Manifest, None) => Ok(()),
            (operation, Some(index)) if operation == &package_operation(&self.read.reference)? => {
                operation.chunk_range(index).map(|_| ())
            }
            _ => Err("admission operation differs from original layout".into()),
        }
    }
    pub fn digest(&self) -> Result<String, String> {
        Ok(digest(
            b"oasis7.bft.admission-input.request.v1",
            &encode(self)?,
        ))
    }
    pub fn base(&self) -> Result<String, String> {
        Ok(digest(
            b"oasis7.bft.admission-input.original.v1",
            &encode(&(
                &self.read.reference,
                &self.read.requester_peer_id,
                &self.read.serving_peer_id,
                &self.read.nonce,
            ))?,
        ))
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Response {
    pub domain: String,
    pub request_digest: String,
    pub reference: InputRef,
    pub chunk_index: Option<u32>,
    pub chunk_hex: Option<String>,
}
pub(super) fn package_operation(
    reference: &InputRef,
) -> Result<BftCandidateReadOperationV3, String> {
    let total_bytes = reference.object.bytes;
    let operation = BftCandidateReadOperationV3::Package {
        package_digest: reference.object.digest.clone(),
        total_bytes,
        chunk_size: CHUNK,
        total_chunks: u32::try_from(total_bytes.div_ceil(u64::from(CHUNK)))
            .map_err(|_| "admission chunk count overflow")?,
    };
    operation.validate()?;
    Ok(operation)
}
