//! Real transport-v2 reader. One peer, nonce, grant and absolute read deadline.
use super::super::wire::{CHUNK, MAX_PACKAGE};
use super::batch_wire::*;
use super::*;
use oasis7_distfs::protected_bft_v3::{
    BftPrivateRetentionAdmissionInputReadV1, BftPrivateRetentionAdmissionInputReaderV1,
    BftSignerErrorV3,
};
pub struct NodeBftLibp2pAdmissionInputReaderV2 {
    network: Arc<Libp2pNetwork>,
    serving_peer: libp2p::PeerId,
    credentials: Arc<dyn NodeBftAdmissionInputBatchCredentialsV2>,
    buffers: Arc<crate::NodeBftPrivateTransportBuffersV1>,
}
impl NodeBftLibp2pAdmissionInputReaderV2 {
    pub fn new(
        network: Arc<Libp2pNetwork>,
        serving_peer: String,
        credentials: Arc<dyn NodeBftAdmissionInputBatchCredentialsV2>,
        buffers: Arc<crate::NodeBftPrivateTransportBuffersV1>,
    ) -> Result<Self, String> {
        let serving_peer = serving_peer
            .parse::<libp2p::PeerId>()
            .map_err(|_| "admission v2 serving Noise peer")?;
        if serving_peer == network.peer_id() {
            return Err("admission v2 serving peer is self".into());
        }
        Ok(Self {
            network,
            serving_peer,
            credentials,
            buffers,
        })
    }
    fn request(
        &self,
        read: BatchReadV2,
        deadline: Instant,
        original_grant: &mut Option<BftPrivateRetentionAdmissionReadGrantV1>,
    ) -> Result<BatchResponseV2, String> {
        if Instant::now() >= deadline {
            return Err("admission v2 original deadline expired".into());
        }
        let authorization = self.credentials.authorize(&read, deadline)?;
        if let Some(original) = original_grant {
            if original != &authorization.grant {
                return Err("admission v2 original issuer permission changed".into());
            }
        } else {
            *original_grant = Some(authorization.grant.clone());
        }
        let request = BatchRequestV2 {
            domain: DOMAIN_V2.into(),
            read,
            authorization,
        };
        request.verify_subject()?;
        let encoded = request.encode()?;
        // Retries reuse these exact bytes/PoP; no credential call or nonce reset.
        let reply = super::input_client::request_original_until(deadline, |remaining| {
            self.network.request_authenticated_to_peer_with_timeout(
                PROTOCOL_V2,
                &encoded,
                self.serving_peer,
                remaining,
            )
        })?;
        if Instant::now() >= deadline {
            return Err("admission v2 original deadline expired".into());
        }
        BatchResponseV2::decode_for(&reply, &request)
    }
    fn read(
        &self,
        reference: &InputRef,
        deadline: Instant,
    ) -> Result<BftPrivateRetentionAdmissionInputReadV1, String> {
        reference
            .validate()
            .map_err(|_| "admission v2 original reference")?;
        if Instant::now() >= deadline {
            return Err("admission v2 original deadline expired".into());
        }
        let hold = self.buffers.inbound(2 * MAX_PACKAGE)?;
        let mut nonce = [0u8; 32];
        getrandom::fill(&mut nonce).map_err(|_| "admission v2 CSPRNG unavailable")?;
        let mut read = BatchReadV2 {
            reference: reference.clone(),
            requester_peer_id: self.network.peer_id().to_string(),
            serving_peer_id: self.serving_peer.to_string(),
            nonce: hex::encode(nonce),
            operation: BatchOperationV2::Manifest,
        };
        let mut grant = None;
        self.request(read.clone(), deadline, &mut grant)?; // v2 only; no V1 fallback
        let total =
            usize::try_from(reference.object.bytes).map_err(|_| "admission v2 object length")?;
        let mut bytes = Vec::new();
        bytes
            .try_reserve_exact(total)
            .map_err(|_| "admission v2 original allocation")?;
        while bytes.len() < total {
            let offset_bytes = bytes.len() as u64;
            let remaining = (reference.object.bytes - offset_bytes).div_ceil(u64::from(CHUNK));
            let part_count = u8::try_from(remaining.min(u64::from(MAX_PARTS_V2)))
                .map_err(|_| "admission v2 batch count")?;
            read.operation = BatchOperationV2::Batch {
                offset_bytes,
                part_count,
            };
            {
                let response = self.request(read.clone(), deadline, &mut grant)?;
                for part in &response.parts {
                    bytes.extend_from_slice(part.bytes.as_slice());
                }
                // response/parts drop before another batch or whole-object decode.
            }
        }
        if Instant::now() >= deadline {
            return Err("admission v2 original deadline expired".into());
        }
        reference
            .object
            .validate_bytes(&bytes)
            .map_err(|_| "admission v2 whole original digest differs")?;
        let input = Input::from_canonical_bytes(&bytes)
            .map_err(|_| "admission v2 canonical original rejected")?;
        reference
            .validate_input(&input)
            .map_err(|_| "admission v2 decoded original differs")?;
        drop(bytes);
        if Instant::now() >= deadline {
            return Err("admission v2 original deadline expired".into());
        }
        Ok(BftPrivateRetentionAdmissionInputReadV1::new(
            Arc::new(input),
            Box::new(hold),
        ))
    }
}
impl BftPrivateRetentionAdmissionInputReaderV1 for NodeBftLibp2pAdmissionInputReaderV2 {
    fn read_original(
        &self,
        reference: &InputRef,
        deadline: Instant,
    ) -> Result<BftPrivateRetentionAdmissionInputReadV1, BftSignerErrorV3> {
        self.read(reference, deadline).map_err(BftSignerErrorV3)
    }
}
