use super::super::wire::{decode_frame, encode_frame};
use super::input_wire::*;
use super::*;
use oasis7_distfs::protected_bft_v3::{
    BftPrivateRetentionAdmissionInputReadV1, BftPrivateRetentionAdmissionInputReaderV1,
    BftSignerErrorV3,
};

pub struct NodeBftLibp2pAdmissionInputReaderV1 {
    network: Arc<Libp2pNetwork>,
    serving_peer: libp2p::PeerId,
    credentials: Arc<dyn NodeBftAdmissionInputReadCredentialsV1>,
    buffers: Arc<crate::NodeBftPrivateTransportBuffersV1>,
}
impl NodeBftLibp2pAdmissionInputReaderV1 {
    pub fn new(
        network: Arc<Libp2pNetwork>,
        serving_peer: String,
        credentials: Arc<dyn NodeBftAdmissionInputReadCredentialsV1>,
        buffers: Arc<crate::NodeBftPrivateTransportBuffersV1>,
    ) -> Result<Self, String> {
        let serving_peer = serving_peer
            .parse::<libp2p::PeerId>()
            .map_err(|_| "admission serving Noise peer identity")?;
        if serving_peer == network.peer_id() {
            return Err("admission serving peer is self".into());
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
        read: NodeBftAdmissionInputReadRequestV1,
        chunk_index: Option<u32>,
        deadline: Instant,
        original_grant: &mut Option<BftPrivateRetentionAdmissionReadGrantV1>,
    ) -> Result<Response, String> {
        if Instant::now() >= deadline {
            return Err("admission original deadline expired".into());
        }
        let authorization = self.credentials.authorize(&read, deadline)?;
        if let Some(original) = original_grant {
            if original != &authorization.grant {
                return Err("admission original issuer permission changed".into());
            }
        } else {
            *original_grant = Some(authorization.grant.clone());
        }
        let request = Request {
            domain: DOMAIN.into(),
            read,
            authorization,
            chunk_index,
        };
        request.validate()?;
        let bytes = encode_frame(&request)?;
        let reply = request_original_until(deadline, |remaining| {
            self.network.request_authenticated_to_peer_with_timeout(
                PROTOCOL,
                &bytes,
                self.serving_peer,
                remaining,
            )
        })?;
        if Instant::now() >= deadline {
            return Err("admission original deadline expired".into());
        }
        let response: Response = decode_frame(&reply)?;
        if response.domain != DOMAIN
            || response.request_digest != request.digest()?
            || response.reference != request.read.reference
            || response.chunk_index != chunk_index
        {
            return Err("admission original response binding differs".into());
        }
        Ok(response)
    }
    fn read(
        &self,
        reference: &InputRef,
        deadline: Instant,
    ) -> Result<BftPrivateRetentionAdmissionInputReadV1, String> {
        reference
            .validate()
            .map_err(|_| "admission original reference rejected")?;
        if Instant::now() >= deadline {
            return Err("admission original deadline expired".into());
        }
        // Keep the complete encoded+decoded peak permit through independent
        // verifier/CAS/readback. The Reader result has no consuming DTO escape.
        let hold = self.buffers.inbound(2 * super::super::wire::MAX_PACKAGE)?;
        let mut random = [0u8; 32];
        getrandom::fill(&mut random).map_err(|_| "admission request CSPRNG unavailable")?;
        let mut read = NodeBftAdmissionInputReadRequestV1 {
            reference: reference.clone(),
            requester_peer_id: self.network.peer_id().to_string(),
            serving_peer_id: self.serving_peer.to_string(),
            nonce: hex::encode(random),
            operation: BftCandidateReadOperationV3::Manifest,
        };
        let mut original_grant = None;
        let manifest = self.request(read.clone(), None, deadline, &mut original_grant)?;
        if manifest.chunk_hex.is_some() {
            return Err("admission manifest contains chunk bytes".into());
        }
        read.operation = package_operation(reference)?;
        let total = usize::try_from(reference.object.bytes)
            .map_err(|_| "admission original byte length")?;
        let mut bytes = Vec::new();
        bytes
            .try_reserve_exact(total)
            .map_err(|_| "admission receive allocation rejected")?;
        let count = match read.operation {
            BftCandidateReadOperationV3::Package { total_chunks, .. } => total_chunks,
            _ => return Err("admission original layout missing".into()),
        };
        for index in 0..count {
            let response =
                self.request(read.clone(), Some(index), deadline, &mut original_grant)?;
            let (_, length) = read.operation.chunk_range(index)?;
            let encoded = response
                .chunk_hex
                .ok_or("admission original chunk absent")?;
            if encoded.len() != length as usize * 2 {
                return Err("admission chunk original byte length differs".into());
            }
            let chunk = hex::decode(encoded).map_err(|_| "admission chunk encoding rejected")?;
            bytes.extend_from_slice(&chunk);
        }
        if Instant::now() >= deadline {
            return Err("admission original deadline expired".into());
        }
        reference
            .object
            .validate_bytes(&bytes)
            .map_err(|_| "admission original byte digest differs")?;
        let input = Input::from_canonical_bytes(&bytes)
            .map_err(|_| "admission canonical original rejected")?;
        reference
            .validate_input(&input)
            .map_err(|_| "admission decoded original differs")?;
        drop(bytes);
        if Instant::now() >= deadline {
            return Err("admission original deadline expired".into());
        }
        Ok(BftPrivateRetentionAdmissionInputReadV1::new(
            Arc::new(input),
            Box::new(hold),
        ))
    }
}
impl BftPrivateRetentionAdmissionInputReaderV1 for NodeBftLibp2pAdmissionInputReaderV1 {
    fn read_original(
        &self,
        reference: &InputRef,
        deadline: Instant,
    ) -> Result<BftPrivateRetentionAdmissionInputReadV1, BftSignerErrorV3> {
        self.read(reference, deadline).map_err(BftSignerErrorV3)
    }
}

pub(super) fn request_original_until(
    deadline: Instant,
    mut request: impl FnMut(std::time::Duration) -> Result<Vec<u8>, WorldError>,
) -> Result<Vec<u8>, String> {
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err("admission original deadline expired".into());
        }
        match request(remaining) {
            Ok(bytes) => return Ok(bytes),
            Err(WorldError::NetworkRequestFailed {
                code: oasis7_proto::distributed::DistributedErrorCode::ErrRateLimited,
                retryable: true,
                ..
            }) => {
                // Retry only this no-effect encrypted read, using the exact
                // serialized request and original absolute operation deadline.
                std::thread::sleep(
                    std::time::Duration::from_millis(50)
                        .min(deadline.saturating_duration_since(Instant::now())),
                );
            }
            Err(_) => return Err("admission encrypted original request failed".into()),
        }
    }
}
#[cfg(test)]
mod retry_tests {
    use super::*;
    use std::time::Duration;
    #[test]
    fn admission_read_shared_rate_limit_keeps_original_deadline_and_request() {
        let deadline = Instant::now() + Duration::from_secs(1);
        let mut attempts = 0;
        let mut previous = Duration::from_secs(2);
        let original = b"same signed request nonce and operation";
        let result = request_original_until(deadline, |remaining| {
            assert!(remaining < previous);
            previous = remaining;
            attempts += 1;
            if attempts == 1 {
                Err(WorldError::NetworkRequestFailed {
                    code: oasis7_proto::distributed::DistributedErrorCode::ErrRateLimited,
                    message: "shared quota".into(),
                    retryable: true,
                })
            } else {
                Ok(original.to_vec())
            }
        })
        .unwrap();
        assert_eq!(result, original);
        assert_eq!(attempts, 2);
        assert!(Instant::now() < deadline);
    }
    #[test]
    fn admission_read_does_not_retry_unknown_or_extend_expired_deadline() {
        let mut calls = 0;
        assert!(
            request_original_until(Instant::now(), |_| {
                calls += 1;
                Ok(vec![])
            })
            .is_err()
        );
        assert_eq!(calls, 0);
        assert!(
            request_original_until(Instant::now() + Duration::from_secs(1), |_| {
                calls += 1;
                Err(WorldError::NetworkRequestFailed {
                    code: oasis7_proto::distributed::DistributedErrorCode::ErrBadRequest,
                    message: "unknown original".into(),
                    retryable: false,
                })
            })
            .is_err()
        );
        assert_eq!(calls, 1);
    }
}
