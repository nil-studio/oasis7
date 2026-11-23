//! Authenticated typed writer RPC. The service owns the writer secret; a Runtime
//! holds only its separate client credential and an independently pinned key.
use super::*;
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

pub(super) fn check_deadline(deadline: Instant) -> Result<(), FormalError> {
    if Instant::now() >= deadline {
        return Err(FormalError::Io(
            "writer custody operation deadline expired".into(),
        ));
    }
    Ok(())
}

fn validate_address_timeout(address: SocketAddr, timeout: Duration) -> Result<(), FormalError> {
    if !address.ip().is_loopback() || timeout.is_zero() || timeout > Duration::from_secs(60) {
        return Err(invalid(
            "writer custody requires loopback tunnel termination and timeout in (0,60s]",
        ));
    }
    Ok(())
}

struct RemoteWriter {
    address: SocketAddr,
    timeout: Duration,
    authority_id: String,
    writer_public_key: String,
    credential: SigningKey,
    revision: AtomicU64,
}

#[derive(Clone)]
pub struct RemoteWriterCustodyClientV1(Arc<RemoteWriter>);
impl RemoteWriterCustodyClientV1 {
    /// Construction performs no network or storage operation. All trust pins
    /// come from the caller's verified authority and deployment configuration.
    pub fn new(
        address: SocketAddr,
        timeout: Duration,
        authority_id: String,
        writer_public_key: String,
        credential: SigningKey,
        minimum_revision: u64,
    ) -> Result<Self, FormalError> {
        validate_address_timeout(address, timeout)?;
        hash_hex(&authority_id)?;
        public_key(&writer_public_key)?;
        if writer_public_key == hex::encode(credential.verifying_key().to_bytes()) {
            return Err(invalid(
                "writer custody client credential must not be the writer key",
            ));
        }
        Ok(Self(Arc::new(RemoteWriter {
            address,
            timeout,
            authority_id,
            writer_public_key,
            credential,
            revision: AtomicU64::new(minimum_revision),
        })))
    }
    fn call(
        &self,
        command: WriterCustodyCommandV1,
        deadline: Instant,
    ) -> Result<(WriterCustodyValueV1, u64), FormalError> {
        let deadline = deadline.min(Instant::now() + self.0.timeout);
        check_deadline(deadline)?;
        let body = WriterCustodyRequestBodyV1 {
            schema_version: 1,
            authority_id: self.0.authority_id.clone(),
            challenge: fencing::fresh_challenge().map_err(|e| FormalError::Io(e.to_string()))?,
            expected_revision: Some(self.0.revision.load(Ordering::Acquire)),
            credential_public_key: hex::encode(self.0.credential.verifying_key().to_bytes()),
            command,
        };
        let request = SignedWriterCustodyRequestV1 {
            signature_hex: hex::encode(
                self.0
                    .credential
                    .sign(&writer_custody_request_signing_bytes(&body)?)
                    .to_bytes(),
            ),
            body,
        };
        check_deadline(deadline)?;
        // One attempt and one absolute deadline. Lost replies remain unknown;
        // reconciliation reuses the original plan or seal identity.
        let response: SignedWriterCustodyResponseV1 =
            transport::writer_custody_exchange(&self.0.address, &request, deadline)?;
        verify_writer_custody_response(&response, &request, &self.0.writer_public_key)?;
        check_deadline(deadline)?;
        let revision = response.body.revision;
        let observed = self.0.revision.fetch_max(revision, Ordering::AcqRel);
        if revision < observed {
            return Err(FormalError::Rollback(
                "writer custody response precedes an authenticated revision".into(),
            ));
        }
        Ok((response.body.result?, revision))
    }
    fn signature(
        &self,
        command: WriterCustodyCommandV1,
        preimage: Vec<u8>,
        deadline: Instant,
    ) -> Result<String, FormalError> {
        let (value, _) = self.call(command, deadline)?;
        let WriterCustodyValueV1::Signature(signature) = value else {
            return Err(invalid("writer custody signature response type"));
        };
        verify_with_preimage::<_, FormalError>(
            &signature,
            &self.0.writer_public_key,
            || Ok(preimage),
            |_| check_deadline(deadline),
        )?;
        Ok(signature)
    }
    pub fn query(&self) -> Result<WriterCustodySnapshotV1, FormalError> {
        let (value, revision) = self.call(
            WriterCustodyCommandV1::Query,
            Instant::now() + self.0.timeout,
        )?;
        let WriterCustodyValueV1::State(state) = value else {
            return Err(invalid("writer custody state response type"));
        };
        if state.writer_public_key != self.0.writer_public_key
            || state.revision != revision
            || (state.drained_digest.is_some() && state.plan_digest.is_none())
            || (state.seal_digest.is_some() && state.drained_digest.is_none())
        {
            return Err(invalid("writer custody state identity/revision binding"));
        }
        for digest in [
            &state.plan_digest,
            &state.drained_digest,
            &state.seal_digest,
        ]
        .into_iter()
        .flatten()
        {
            hash_hex(digest)?;
        }
        Ok(state)
    }
    pub fn prepare_recovery_plan(
        &self,
        plan: &SignedRecoveryPlanV1,
        fixed: &RecoveryTailWitnessV1,
        fixed_material: Option<&ClosedRecord>,
        pending: &RecoveryPendingHandoffV1,
        authority: &VerifiedAuthorityV1,
    ) -> Result<SignedWriterRecoveryPlanAckV1, FormalError> {
        self.verify_authority(authority)?;
        plan.verify(authority)?;
        match (fixed, fixed_material) {
            (RecoveryTailWitnessV1::Empty, None) => {}
            (
                RecoveryTailWitnessV1::Frozen(frozen)
                | RecoveryTailWitnessV1::Prepared { frozen, .. }
                | RecoveryTailWitnessV1::Decision { frozen, .. },
                Some(material),
            ) => verify_frozen_material(&frozen.reservation.body, material)?,
            _ => {
                return Err(invalid(
                    "writer plan must retain the exact original Frozen closure",
                ));
            }
        }
        if pending.root()? != plan.body.pending_handoff_root
            || recovery_requests_commitment(fixed)? != plan.body.original_requests_commitment
        {
            return Err(invalid("writer custody original pending handoff root"));
        }
        let (value, revision) = self.call(
            WriterCustodyCommandV1::InstallPlan {
                plan: Box::new(plan.clone()),
                fixed: Box::new(fixed.clone()),
                fixed_material: fixed_material.cloned().map(Box::new),
                pending: Box::new(pending.clone()),
            },
            Instant::now() + self.0.timeout,
        )?;
        let WriterCustodyValueV1::Plan(ack) = value else {
            return Err(invalid("writer custody plan response type"));
        };
        ack.verify(plan, authority)?;
        if ack.body.revision > revision {
            return Err(invalid(
                "writer acknowledgement exceeds authenticated durable revision",
            ));
        }
        Ok(*ack)
    }
    pub fn complete_drain(
        &self,
        plan: &SignedRecoveryPlanV1,
        drained: &RecoveryDrainedProofV1,
        authority: &VerifiedAuthorityV1,
    ) -> Result<RecoveryDrainedProofV1, FormalError> {
        self.verify_authority(authority)?;
        drained.verify(plan, authority)?;
        let (value, _) = self.call(
            WriterCustodyCommandV1::DrainComplete(Box::new(drained.clone())),
            Instant::now() + self.0.timeout,
        )?;
        let WriterCustodyValueV1::Drained(original) = value else {
            return Err(invalid("writer custody drained response type"));
        };
        if original.as_ref() != drained {
            return Err(invalid("writer custody changed the original drained proof"));
        }
        original.verify(plan, authority)?;
        Ok(*original)
    }
    pub fn seal_epoch(
        &self,
        plan: &SignedRecoveryPlanV1,
        drained: &RecoveryDrainedProofV1,
        authority: &VerifiedAuthorityV1,
    ) -> Result<SignedWriterEpochSealV1, FormalError> {
        self.verify_authority(authority)?;
        let cut = drained.verify(plan, authority)?;
        let (value, revision) = self.call(
            WriterCustodyCommandV1::Seal {
                plan_digest: plan.digest()?,
                cut,
            },
            Instant::now() + self.0.timeout,
        )?;
        let WriterCustodyValueV1::Seal(seal) = value else {
            return Err(invalid("writer custody seal response type"));
        };
        seal.verify(plan, authority)?;
        if &seal.drained != drained {
            return Err(invalid(
                "writer epoch seal changed the original drained proof",
            ));
        }
        if seal.body.revision > revision {
            return Err(invalid(
                "writer seal exceeds authenticated durable revision",
            ));
        }
        Ok(*seal)
    }
    fn verify_authority(&self, authority: &VerifiedAuthorityV1) -> Result<(), FormalError> {
        if authority.authority_id()? != self.0.authority_id
            || authority.definition().writer_public_key != self.0.writer_public_key
        {
            return Err(invalid("writer custody command verified authority binding"));
        }
        Ok(())
    }
}
impl FormalWriterSignerV1 for RemoteWriterCustodyClientV1 {
    fn public_key_hex(&self) -> String {
        self.0.writer_public_key.clone()
    }
    fn sign_frozen(&self, body: &FormalFrozenBodyV1) -> Result<String, FormalError> {
        let deadline = Instant::now() + self.0.timeout;
        self.signature(
            WriterCustodyCommandV1::Frozen(Box::new(body.clone())),
            frozen_signing_bytes(body)?,
            deadline,
        )
    }
    fn sign_candidate(
        &self,
        record: &CanonicalRecordV1,
        frozen: Option<&FormalFrozenCertificateV1>,
    ) -> Result<String, FormalError> {
        let deadline = Instant::now() + self.0.timeout;
        self.signature(
            WriterCustodyCommandV1::Candidate {
                record: Box::new(record.clone()),
                frozen: frozen.cloned().map(Box::new),
            },
            canonical_candidate_signing_bytes(record)?,
            deadline,
        )
    }
    fn sign_endpoint_request(
        &self,
        request: &WriterEndpointRequestV1,
    ) -> Result<String, FormalError> {
        self.sign_endpoint_request_before(request, Instant::now() + self.0.timeout)
    }
    fn sign_endpoint_request_before(
        &self,
        request: &WriterEndpointRequestV1,
        deadline: Instant,
    ) -> Result<String, FormalError> {
        check_deadline(deadline)?;
        self.signature(
            WriterCustodyCommandV1::Endpoint(Box::new(request.clone())),
            request.signing_bytes()?,
            deadline,
        )
    }
}

/// Implementations authenticate the complete request and perform phase checks,
/// durable transitions, and final signatures under the same exclusive lease.
pub trait WriterCustodyServiceV1 {
    fn handle_signed(
        &mut self,
        request: SignedWriterCustodyRequestV1,
    ) -> Result<SignedWriterCustodyResponseV1, FormalError>;
}
pub struct WriterCustodyServerV1<S> {
    service: S,
    timeout: Duration,
}
impl<S: WriterCustodyServiceV1> WriterCustodyServerV1<S> {
    pub fn new(service: S, timeout: Duration) -> Result<Self, FormalError> {
        if timeout.is_zero() || timeout > Duration::from_secs(60) {
            return Err(invalid("writer custody timeout in (0,60s]"));
        }
        Ok(Self { service, timeout })
    }
    pub fn serve_one(&mut self, listener: &TcpListener) -> Result<(), FormalError> {
        let address = listener
            .local_addr()
            .map_err(|e| FormalError::Io(e.to_string()))?;
        validate_address_timeout(address, self.timeout)?;
        let (stream, _) = listener
            .accept()
            .map_err(|e| FormalError::Io(e.to_string()))?;
        self.serve_connection(stream)
    }
    pub fn serve_connection(&mut self, stream: TcpStream) -> Result<(), FormalError> {
        let peer = stream
            .peer_addr()
            .map_err(|e| FormalError::Io(e.to_string()))?;
        validate_address_timeout(peer, self.timeout)?;
        transport::writer_custody_serve_connection(stream, self.timeout, |request| {
            self.service.handle_signed(request)
        })
    }
}
