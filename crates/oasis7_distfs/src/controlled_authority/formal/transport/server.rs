use super::*;
use std::net::TcpListener;
use std::time::Duration;
pub struct FormalEndpointServer {
    endpoint: FileFormalEndpoint,
    timeout: Duration,
}
impl FormalEndpointServer {
    pub fn new(endpoint: FileFormalEndpoint, timeout: Duration) -> Result<Self, FormalError> {
        if timeout.is_zero() || timeout > Duration::from_secs(60) {
            return Err(invalid("formal RPC timeout in (0,60s]"));
        }
        Ok(Self { endpoint, timeout })
    }
    #[cfg(test)]
    pub(crate) fn publish_qualified_for_test(
        &mut self,
        proof: FormalQualifiedCommitProofV1,
    ) -> Result<FormalHeadRefV1, FormalError> {
        self.endpoint.publish_qualified(proof)
    }
    pub fn serve_one(&mut self, listener: &TcpListener) -> Result<(), FormalError> {
        wire::address(listener.local_addr().map_err(wire::io)?)?;
        let (mut stream, peer) = listener.accept().map_err(wire::io)?;
        wire::address(peer)?;
        wire::configure(&stream, self.timeout)?;
        let request: Request = wire::read(&mut stream)?;
        let body = &request.body;
        hash_hex(&body.challenge)?;
        hash_hex(&body.minimum.record_id)?;
        let authority = self.endpoint.authority();
        let instance = match self.endpoint.role() {
            EndpointRole::Primary => &authority.definition().primary.instance,
            EndpointRole::Replica => &authority.definition().replica.instance,
        };
        if body.schema_version != 1
            || body.world != authority.definition().world
            || body.authority_id != authority.authority_id()?
            || body.instance_digest
                != instance
                    .digest()
                    .map_err(super::super::endpoint::fencing_error)?
        {
            return Err(invalid("formal RPC exact pinned authority/instance"));
        }
        let request_key = if let Some(grant) = &body.archive_read_grant {
            if !matches!(
                &body.command,
                Command::Head
                    | Command::ArchiveManifest
                    | Command::ArchiveObject(_)
                    | Command::BusinessIndexPath { .. }
                    | Command::ArchiveChunk { .. }
            ) {
                return Err(invalid("sealed archive delegation cannot mutate"));
            }
            grant.verify(authority, self.endpoint.role())?;
            grant.body.reader_public_key.clone()
        } else {
            authority.definition().writer_public_key.clone()
        };
        verify(REQUEST_DOMAIN, body, &request.signature, &request_key)?;
        if let Some(grant) = &body.archive_read_grant {
            self.endpoint
                .verify_sealed_archive_grant_scope(grant, &body.minimum)?;
        }
        let request_digest = hash("oasis7.formal.endpoint.rpc.request_digest.v1", body)?;
        let response = match &body.command {
            Command::Head
            | Command::ArchiveChunk { .. }
            | Command::ArchiveObject(_)
            | Command::BusinessIndexPath { .. } => self.endpoint.sign_formal_rpc_read_response(
                request_digest,
                body.challenge.clone(),
                body.minimum.clone(),
                &body.command,
            )?,
            _ => {
                self.endpoint.verify_minimum(&body.minimum)?;
                let result = self.execute(body.command.clone());
                self.endpoint.sign_formal_rpc_response(
                    request_digest,
                    body.challenge.clone(),
                    body.minimum.clone(),
                    result,
                )?
            }
        };
        wire::write(&mut stream, &response)
    }
    fn execute(&mut self, command: Command) -> Result<Value, FormalError> {
        match command {
            Command::Head => {
                self.endpoint.head()?;
                Ok(Value::Complete)
            }
            Command::Query(request) => self
                .endpoint
                .query(&request)
                .map(|q| Value::Query(Box::new(q))),
            Command::Freeze(input, material) => self
                .endpoint
                .freeze(*input, *material)
                .map(|r| Value::Frozen(Box::new(r))),
            Command::CompleteFrozen(cert) => {
                self.endpoint.complete_frozen(*cert)?;
                Ok(Value::Complete)
            }
            Command::InstallRecoveryFrozen(cert, material) => {
                self.endpoint.install_recovery_frozen(*cert, *material)?;
                Ok(Value::Complete)
            }
            Command::Prepare(candidate) => self
                .endpoint
                .prepare(*candidate)
                .map(|r| Value::Prepare(Box::new(r))),
            Command::Decide(request, primary, replica) => self
                .endpoint
                .decide(&request, *primary, *replica)
                .map(|r| Value::Final(Box::new(r))),
            Command::InstallProof(proof) => self
                .endpoint
                .install_proof(*proof)
                .map(|r| Value::ProofComplete(Box::new(r))),
            Command::PublishQualified(proof) => {
                self.endpoint.publish_qualified(*proof)?;
                Ok(Value::Complete)
            }
            Command::StageNativeEnvironment(descriptor, chunks) => {
                self.endpoint
                    .stage_native_environment(&descriptor, &chunks)?;
                Ok(Value::Complete)
            }
            Command::VerifyNativeEnvironment(descriptor) => self
                .endpoint
                .advance_native_environment_verification(&descriptor)
                .map(Value::NativeEnvironment),
            Command::VerifyPublishedClosure {
                request,
                candidate_digest,
            } => self
                .endpoint
                .verify_published_closure(&request, &candidate_digest)
                .map(Value::PublishedClosure),
            Command::ReleasePublishedClosure { candidate_digest } => {
                self.endpoint.release_published_closure(&candidate_digest)?;
                Ok(Value::Complete)
            }
            Command::ArchiveManifest => self
                .endpoint
                .archive_manifest()
                .map(|m| Value::ArchiveManifest(Box::new(m))),
            Command::ArchiveChunk { id, offset, limit } => self
                .endpoint
                .archive_chunk(&id, offset, limit)
                .map(Value::ArchiveChunk),
            Command::ArchiveObject(id) => {
                self.endpoint.archive_object(&id).map(Value::ArchiveObject)
            }
            Command::Seal {
                operation,
                next_epoch,
                checkpoint,
            } => self
                .endpoint
                .seal_epoch(&operation, next_epoch, &checkpoint)
                .map(|s| Value::Seal(Box::new(s))),
            Command::SealDurableSet {
                transition,
                checkpoint,
            } => {
                let verified = self
                    .endpoint
                    .verify_durable_set_transition_at_head(&transition)?;
                self.endpoint
                    .seal_durable_set(&verified, &checkpoint)
                    .map(|seal| Value::DurableSetSeal(Some(Box::new(seal))))
            }
            Command::BusinessIndexPath { .. } => {
                Err(invalid("business index path requires guarded read"))
            }
            Command::QueryDurableSet(transition) => {
                let verified = self
                    .endpoint
                    .verify_durable_set_transition_at_head(&transition)?;
                self.endpoint
                    .query_durable_set(&verified)
                    .map(|seal| Value::DurableSetSeal(seal.map(Box::new)))
            }
        }
    }
}
