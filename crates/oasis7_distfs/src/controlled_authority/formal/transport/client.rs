use super::super::coordinator::FormalDurabilityEndpoint;
use super::*;
use std::net::{SocketAddr, TcpStream};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
#[path = "client_attestation.rs"]
mod attestation;
#[path = "client_native.rs"]
mod native;
#[path = "client_native_context.rs"]
mod native_context;
#[path = "client_native_history.rs"]
mod native_history;
pub(crate) use native_context::VerifiedNativeRpcContextV1;
pub struct RemoteFormalEndpoint {
    address: SocketAddr,
    timeout: Duration,
    operation_deadline: Option<Instant>,
    authority: VerifiedAuthorityV1,
    role: EndpointRole,
    key: SigningKey,
    minimum: FormalHeadRefV1,
    last_checkpoint: Option<fencing::EndpointCheckpointBindingV1>,
    last_native_context: Option<VerifiedNativeRpcContextV1>,
    archive_read_grant: Option<SignedArchiveReadGrantV1>,
    #[cfg(test)]
    last_native_restore_source_reads: usize,
}
impl RemoteFormalEndpoint {
    pub fn new(
        address: SocketAddr,
        timeout: Duration,
        authority: VerifiedAuthorityV1,
        role: EndpointRole,
        key: SigningKey,
        minimum: FormalHeadRefV1,
    ) -> Result<Self, FormalError> {
        Self::new_inner(address, timeout, authority, role, key, minimum, None)
    }
    pub fn new_archive_reader(
        address: SocketAddr,
        timeout: Duration,
        authority: VerifiedAuthorityV1,
        role: EndpointRole,
        key: SigningKey,
        grant: SignedArchiveReadGrantV1,
        target_instance_digest: &str,
    ) -> Result<Self, FormalError> {
        grant.verify(&authority, role)?;
        if grant.body.target_instance_digest != target_instance_digest {
            return Err(invalid("sealed archive grant target instance"));
        }
        let minimum = grant.body.sealed_cut.clone();
        Self::new_inner(address, timeout, authority, role, key, minimum, Some(grant))
    }
    fn new_inner(
        address: SocketAddr,
        timeout: Duration,
        authority: VerifiedAuthorityV1,
        role: EndpointRole,
        key: SigningKey,
        minimum: FormalHeadRefV1,
        archive_read_grant: Option<SignedArchiveReadGrantV1>,
    ) -> Result<Self, FormalError> {
        wire::address(address)?;
        if timeout.is_zero() || timeout > Duration::from_secs(60) {
            return Err(invalid("formal RPC timeout in (0,60s]"));
        }
        authority.definition().verify_policy(authority.policy())?;
        hash_hex(&minimum.record_id)?;
        let expected_key = archive_read_grant
            .as_ref()
            .map_or(authority.definition().writer_public_key.as_str(), |grant| {
                grant.body.reader_public_key.as_str()
            });
        if expected_key != hex::encode(key.verifying_key().to_bytes()) {
            return Err(invalid(
                "formal RPC writer key does not match verified active authority",
            ));
        }
        if minimum.position == 0 && minimum != authority.definition().world.genesis_head()? {
            return Err(invalid("formal RPC pinned genesis minimum"));
        }
        Ok(Self {
            address,
            timeout,
            operation_deadline: None,
            authority,
            role,
            key,
            minimum,
            last_checkpoint: None,
            last_native_context: None,
            archive_read_grant,
            #[cfg(test)]
            last_native_restore_source_reads: 0,
        })
    }
    /// Bound a multi-RPC operation by one absolute deadline. The configured
    /// RPC window is still the maximum; an external caller cannot extend it.
    pub fn set_operation_deadline(&mut self, deadline: Instant) {
        let configured = Instant::now() + self.timeout;
        self.operation_deadline = Some(deadline.min(configured));
    }
    fn remaining_timeout(&self, deadline: Instant) -> Result<Duration, FormalError> {
        deadline
            .checked_duration_since(Instant::now())
            .filter(|time| !time.is_zero())
            .map(|time| time.min(self.timeout))
            .ok_or_else(|| FormalError::Io("formal RPC operation deadline expired".into()))
    }
    fn instance(&self) -> &fencing::EndpointInstanceBindingV1 {
        match self.role {
            EndpointRole::Primary => &self.authority.definition().primary.instance,
            EndpointRole::Replica => &self.authority.definition().replica.instance,
        }
    }
    fn call(&mut self, command: Command) -> Result<(Value, FormalHeadRefV1), FormalError> {
        let body = RequestBody {
            schema_version: 1,
            world: self.authority.definition().world.clone(),
            authority_id: self.authority.authority_id()?,
            instance_digest: self
                .instance()
                .digest()
                .map_err(super::super::endpoint::fencing_error)?,
            challenge: fencing::fresh_challenge().map_err(wire::io)?,
            minimum: self.minimum.clone(),
            command,
            archive_read_grant: self.archive_read_grant.clone(),
        };
        let digest = hash("oasis7.formal.endpoint.rpc.request_digest.v1", &body)?;
        let request = Request {
            signature: hex::encode(self.key.sign(&bytes(REQUEST_DOMAIN, &body)?).to_bytes()),
            body,
        };
        let deadline = self
            .operation_deadline
            .unwrap_or_else(|| Instant::now() + self.timeout);
        let mut stream =
            TcpStream::connect_timeout(&self.address, self.remaining_timeout(deadline)?)
                .map_err(wire::io)?;
        wire::configure(&stream, self.remaining_timeout(deadline)?)?;
        wire::write_before_deadline(&mut stream, &request, deadline)?;
        let response: Response = wire::read_before_deadline(&mut stream, deadline)?;
        verify(
            RESPONSE_DOMAIN,
            &hash(
                "oasis7.formal.endpoint.rpc.response_body.v1",
                &response.body,
            )?,
            &response.signature,
            &self.instance().endpoint_public_key,
        )?;
        attestation::verify_response_custody(&response, &self.authority, self.role)?;
        let body = response.body;
        if body.schema_version != 1
            || body.request_digest != digest
            || body.challenge != request.body.challenge
            || body.world != request.body.world
            || body.authority_id != request.body.authority_id
            || body.instance_digest != request.body.instance_digest
            || body.minimum != request.body.minimum
            || body.head.position < self.minimum.position
            || (body.head.position == self.minimum.position
                && body.head.record_id != self.minimum.record_id)
        {
            return Err(FormalError::Rollback(
                "formal RPC freshness/authority/canonical minimum".into(),
            ));
        }
        hash_hex(&body.head.record_id)?;
        super::super::endpoint::verify_formal_rpc_checkpoint(&body, &self.authority, self.role)?;
        fencing::verify_monotonic_checkpoint_evidence(
            &body.provider,
            self.instance(),
            &body.checkpoint,
            &fencing::TrustedFencingPolicyV1 {
                provider_public_key: self.authority.policy().fencing_provider_public_key.clone(),
                authorized_resource_bindings: self
                    .authority
                    .policy()
                    .authorized_fencing_resource_bindings
                    .clone(),
            },
        )
        .map_err(super::super::endpoint::fencing_error)?;
        if body.checkpoint.head.position < body.head.position
            || body.checkpoint.head.position > body.head.position.saturating_add(1)
            || (!body.checkpoint.head.qualified
                && Some(body.checkpoint.head.position) != body.head.position.checked_add(1))
            || (body.checkpoint.head.qualified
                && (body.checkpoint.head.position != body.head.position
                    || body.checkpoint.head.decision_hash != body.head.record_id))
        {
            return Err(invalid("formal RPC provider/head checkpoint mismatch"));
        }
        let native_context =
            native_context::context_after_authenticated_call(&body, &self.authority, self.role)?;
        self.remaining_timeout(deadline)?;
        self.last_native_context = Some(native_context);
        self.last_checkpoint = Some(body.checkpoint);
        self.minimum = body.head.clone();
        Ok((body.result?, body.head))
    }
    fn complete(&mut self, command: Command) -> Result<(), FormalError> {
        match self.call(command)?.0 {
            Value::Complete => Ok(()),
            _ => Err(invalid("formal RPC response type")),
        }
    }
    pub fn checkpoint(&mut self) -> Result<fencing::EndpointCheckpointBindingV1, FormalError> {
        self.call(Command::Head)?;
        self.last_checkpoint
            .clone()
            .ok_or_else(|| invalid("formal RPC missing checkpoint attestation"))
    }
    pub fn archive_manifest(&mut self) -> Result<SignedFormalArchiveTransferV1, FormalError> {
        match self.call(Command::ArchiveManifest)?.0 {
            Value::ArchiveManifest(m) => Ok(*m),
            _ => Err(invalid("formal RPC response type")),
        }
    }
    pub fn archive_object(&mut self, id: &str) -> Result<Option<Vec<u8>>, FormalError> {
        self.fetch_hash_object(id)
    }
    /// Fetch immutable path bytes only. The caller supplies its independently
    /// qualified descriptor; this RPC does not qualify a root or result.
    pub fn business_index_path(
        &mut self,
        root: &super::super::business_index::BusinessIndexRootV1,
        key_digest: &str,
    ) -> Result<super::super::business_index::BusinessIndexPathV1, FormalError> {
        root.validate().map_err(invalid)?;
        hash_hex(key_digest)?;
        match self
            .call(Command::BusinessIndexPath {
                root: root.clone(),
                key_digest: key_digest.to_owned(),
            })?
            .0
        {
            Value::BusinessIndexPath(path) => {
                path.verify(root, key_digest).map_err(invalid)?;
                Ok(*path)
            }
            _ => Err(invalid("formal RPC response type")),
        }
    }

    pub fn seal_epoch(
        &mut self,
        operation: String,
        next_epoch: u64,
        checkpoint: fencing::EndpointCheckpointBindingV1,
    ) -> Result<fencing::EpochSealReceiptV1, FormalError> {
        match self
            .call(Command::Seal {
                operation,
                next_epoch,
                checkpoint,
            })?
            .0
        {
            Value::Seal(seal) => Ok(*seal),
            _ => Err(invalid("formal RPC response type")),
        }
    }
    pub fn into_archive_source(self) -> Arc<dyn FormalArchiveSourceV1> {
        Arc::new(RemoteArchiveSource(Mutex::new(self)))
    }
    pub fn seal_durable_set(
        &mut self,
        transition: SignedNormalDurableSetTransitionV1,
        checkpoint: fencing::EndpointCheckpointBindingV1,
    ) -> Result<fencing::EpochSealReceiptV1, FormalError> {
        match self
            .call(Command::SealDurableSet {
                transition: Box::new(transition),
                checkpoint,
            })?
            .0
        {
            Value::DurableSetSeal(Some(seal)) => Ok(*seal),
            _ => Err(invalid("formal RPC durable set seal response type")),
        }
    }
    pub fn query_durable_set(
        &mut self,
        transition: SignedNormalDurableSetTransitionV1,
    ) -> Result<Option<fencing::EpochSealReceiptV1>, FormalError> {
        match self.call(Command::QueryDurableSet(Box::new(transition)))?.0 {
            Value::DurableSetSeal(seal) => Ok(seal.map(|seal| *seal)),
            _ => Err(invalid("formal RPC durable set query response type")),
        }
    }
}
struct RemoteArchiveSource(Mutex<RemoteFormalEndpoint>);
impl FormalArchiveSourceV1 for RemoteArchiveSource {
    fn read_range(
        &self,
        id: &str,
        offset: u64,
        limit: u32,
    ) -> Result<Option<FormalArchiveChunkV1>, FormalError> {
        self.0
            .lock()
            .map_err(|_| invalid("formal archive RPC mutex poisoned"))?
            .archive_chunk(id, offset, limit)
    }
    fn read_object(&self, id: &str) -> Result<Option<Vec<u8>>, FormalError> {
        self.0
            .lock()
            .map_err(|_| invalid("formal archive RPC mutex poisoned"))?
            .archive_object(id)
    }
}
impl FormalDurabilityEndpoint for RemoteFormalEndpoint {
    fn stage_native_environment(
        &mut self,
        descriptor: &super::super::native_environment::NativeExecutionEnvironmentV1,
        chunks: &[super::super::NativeEnvironmentChunkBytesV1],
    ) -> Result<(), FormalError> {
        descriptor.validate().map_err(invalid)?;
        if chunks.is_empty()
            || chunks.len() > 8
            || chunks.iter().map(|chunk| chunk.bytes.len()).sum::<usize>() > 8 * 1024 * 1024
        {
            return Err(invalid("native stage batch budget"));
        }
        match self
            .call(Command::StageNativeEnvironment(
                Box::new(descriptor.clone()),
                chunks.to_vec(),
            ))?
            .0
        {
            Value::Complete => Ok(()),
            _ => Err(invalid("native stage response type")),
        }
    }
    fn advance_native_environment_verification(
        &mut self,
        descriptor: &super::super::native_environment::NativeExecutionEnvironmentV1,
    ) -> Result<super::super::NativeEnvironmentVerificationProgressV1, FormalError> {
        let id = descriptor.digest().map_err(invalid)?;
        match self
            .call(Command::VerifyNativeEnvironment(Box::new(
                descriptor.clone(),
            )))?
            .0
        {
            Value::NativeEnvironment(progress) if progress.descriptor_digest == id => Ok(progress),
            _ => Err(invalid("native verification response identity/type")),
        }
    }

    fn release_published_closure(&mut self, candidate_digest: &str) -> Result<(), FormalError> {
        match self
            .call(Command::ReleasePublishedClosure {
                candidate_digest: candidate_digest.into(),
            })?
            .0
        {
            Value::Complete => Ok(()),
            _ => Err(invalid("formal closure release response type")),
        }
    }
    fn verify_published_closure(
        &mut self,
        request: &str,
        candidate_digest: &str,
    ) -> Result<super::super::coordinator::FormalPublishedClosureReceiptV1, FormalError> {
        match self
            .call(Command::VerifyPublishedClosure {
                request: request.into(),
                candidate_digest: candidate_digest.into(),
            })?
            .0
        {
            Value::PublishedClosure(receipt) if receipt.candidate_digest == candidate_digest => {
                Ok(receipt)
            }
            _ => Err(invalid("formal independent closure response identity/type")),
        }
    }
    fn head(&mut self) -> Result<FormalHeadRefV1, FormalError> {
        let (value, head) = self.call(Command::Head)?;
        if !matches!(value, Value::Complete) {
            return Err(invalid("formal RPC response type"));
        }
        Ok(head)
    }
    fn query(&mut self, request: &str) -> Result<FormalQueryOutcomeV1, FormalError> {
        match self.call(Command::Query(request.into()))?.0 {
            Value::Query(q) => Ok(*q),
            _ => Err(invalid("formal RPC response type")),
        }
    }
    fn freeze(
        &mut self,
        input: SignedFormalFrozenV1,
        material: ClosedRecord,
    ) -> Result<FormalFrozenReceiptV1, FormalError> {
        match self
            .call(Command::Freeze(Box::new(input), Box::new(material)))?
            .0
        {
            Value::Frozen(r) => Ok(*r),
            _ => Err(invalid("formal RPC response type")),
        }
    }
    fn complete_frozen(
        &mut self,
        certificate: FormalFrozenCertificateV1,
    ) -> Result<(), FormalError> {
        self.complete(Command::CompleteFrozen(Box::new(certificate)))
    }
    fn install_recovery_frozen(
        &mut self,
        certificate: FormalFrozenCertificateV1,
        material: ClosedRecord,
    ) -> Result<(), FormalError> {
        self.complete(Command::InstallRecoveryFrozen(
            Box::new(certificate),
            Box::new(material),
        ))
    }
    fn prepare(
        &mut self,
        candidate: FormalCandidateMaterialV1,
    ) -> Result<FormalPrepareReceiptV1, FormalError> {
        match self.call(Command::Prepare(Box::new(candidate)))?.0 {
            Value::Prepare(r) => Ok(*r),
            _ => Err(invalid("formal RPC response type")),
        }
    }
    fn decide(
        &mut self,
        request: &str,
        primary: FormalPrepareReceiptV1,
        replica: FormalPrepareReceiptV1,
    ) -> Result<FormalFinalReceiptV1, FormalError> {
        match self
            .call(Command::Decide(
                request.into(),
                Box::new(primary),
                Box::new(replica),
            ))?
            .0
        {
            Value::Final(r) => Ok(*r),
            _ => Err(invalid("formal RPC response type")),
        }
    }
    fn install_proof(
        &mut self,
        decision: FormalDecisionProofV1,
    ) -> Result<FormalProofCompleteReceiptV1, FormalError> {
        match self.call(Command::InstallProof(Box::new(decision)))?.0 {
            Value::ProofComplete(r) => Ok(*r),
            _ => Err(invalid("formal RPC response type")),
        }
    }
    fn publish_qualified(
        &mut self,
        proof: FormalQualifiedCommitProofV1,
    ) -> Result<FormalHeadRefV1, FormalError> {
        let (value, head) = self.call(Command::PublishQualified(Box::new(proof)))?;
        if !matches!(value, Value::Complete) {
            return Err(invalid("formal RPC response type"));
        }
        Ok(head)
    }
}

#[path = "client_committed_history.rs"]
mod committed_history;
