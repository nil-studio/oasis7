//! Test-binary-only interruption after one endpoint has published the original proof.
#![cfg(all(test, unix))]
use oasis7_distfs::controlled_authority::{
    formal::{
        coordinator::FormalDurabilityEndpoint, endpoint::*, transport::RemoteFormalEndpoint, *,
    },
    replicated_protocol::{ClosedRecord, EndpointRole},
};
pub(crate) struct CrashBoundaryFormalEndpoint {
    inner: RemoteFormalEndpoint,
    role: EndpointRole,
}
impl CrashBoundaryFormalEndpoint {
    pub(crate) fn new(inner: RemoteFormalEndpoint, role: EndpointRole) -> Self {
        Self { inner, role }
    }
}
impl FormalDurabilityEndpoint for CrashBoundaryFormalEndpoint {
    fn release_published_closure(&mut self, digest: &str) -> Result<(), FormalError> {
        use crate::execution_bridge_real_tests::real_execution_bridge::formal_crash_test_boundary as boundary;
        let phase = std::env::var("OASIS7_FORMAL_TEST_CRASH_PHASE").unwrap_or_default();
        if self.role == EndpointRole::Replica
            && matches!(
                phase.as_str(),
                "release_replica_temporarily_unavailable" | "release_replica_reply_lost"
            )
            && boundary::release_target_matches(digest).map_err(FormalError::Invalid)?
        {
            if phase == "release_replica_reply_lost" {
                // The actual RPC reaches R and releases its kernel pin. Discard
                // its successful response, leaving the writer with an Io error.
                self.inner.release_published_closure(digest)?;
            }
            boundary::release_transport_failure(digest, &phase).map_err(FormalError::Invalid)?;
            return Err(FormalError::Io(if phase == "release_replica_reply_lost" {
                "test discarded successful replica release response; remote remains alive".into()
            } else {
                "test transport unavailable before replica release; remote remains alive".into()
            }));
        }
        self.inner.release_published_closure(digest)
    }

    fn stage_native_environment(
        &mut self,
        descriptor: &native_environment::NativeExecutionEnvironmentV1,
        chunks: &[NativeEnvironmentChunkBytesV1],
    ) -> Result<(), FormalError> {
        self.inner.stage_native_environment(descriptor, chunks)
    }
    fn advance_native_environment_verification(
        &mut self,
        descriptor: &native_environment::NativeExecutionEnvironmentV1,
    ) -> Result<NativeEnvironmentVerificationProgressV1, FormalError> {
        self.inner
            .advance_native_environment_verification(descriptor)
    }

    fn release_native_environment_verification(
        &mut self,
        descriptor_digest: &str,
    ) -> Result<(), FormalError> {
        self.inner
            .release_native_environment_verification(descriptor_digest)
    }

    fn verify_published_closure(
        &mut self,
        request: &str,
        digest: &str,
    ) -> Result<
        oasis7_distfs::controlled_authority::formal::coordinator::FormalPublishedClosureReceiptV1,
        FormalError,
    > {
        crate::execution_bridge_real_tests::real_execution_bridge::formal_crash_test_boundary::forbid_release_reverification(digest).map_err(FormalError::Invalid)?;
        self.inner.verify_published_closure(request, digest)
    }
    fn install_recovery_frozen(
        &mut self,
        certificate: FormalFrozenCertificateV1,
        material: ClosedRecord,
    ) -> Result<(), FormalError> {
        self.inner.install_recovery_frozen(certificate, material)
    }
    fn head(&mut self) -> Result<FormalHeadRefV1, FormalError> {
        self.inner.head()
    }
    fn query(&mut self, request: &str) -> Result<FormalQueryOutcomeV1, FormalError> {
        self.inner.query(request)
    }
    fn freeze(
        &mut self,
        input: SignedFormalFrozenV1,
        material: ClosedRecord,
    ) -> Result<FormalFrozenReceiptV1, FormalError> {
        self.inner.freeze(input, material)
    }
    fn complete_frozen(
        &mut self,
        certificate: FormalFrozenCertificateV1,
    ) -> Result<(), FormalError> {
        self.inner.complete_frozen(certificate)
    }
    fn prepare(
        &mut self,
        candidate: FormalCandidateMaterialV1,
    ) -> Result<FormalPrepareReceiptV1, FormalError> {
        let receipt = self.inner.prepare(candidate)?;
        if self.role == EndpointRole::Replica {
            crate::execution_bridge_real_tests::real_execution_bridge::formal_crash_test_boundary::stop_after("replica_prepared")
                .map_err(FormalError::Invalid)?;
        }
        Ok(receipt)
    }
    fn decide(
        &mut self,
        request: &str,
        primary: FormalPrepareReceiptV1,
        replica: FormalPrepareReceiptV1,
    ) -> Result<FormalFinalReceiptV1, FormalError> {
        self.inner.decide(request, primary, replica)
    }
    fn install_proof(
        &mut self,
        decision: FormalDecisionProofV1,
    ) -> Result<FormalProofCompleteReceiptV1, FormalError> {
        self.inner.install_proof(decision)
    }
    fn publish_qualified(
        &mut self,
        proof: FormalQualifiedCommitProofV1,
    ) -> Result<FormalHeadRefV1, FormalError> {
        let head = self.inner.publish_qualified(proof)?;
        if self.role == EndpointRole::Primary {
            crate::execution_bridge_real_tests::real_execution_bridge::formal_crash_test_boundary::stop_after("primary_published")
                .map_err(FormalError::Invalid)?;
        }
        Ok(head)
    }
}
