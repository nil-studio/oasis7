//! Sealed operation adapters for the one proof predicate kernel. No public
//! verifier accepts an adapter; only Legacy and fixed bounded Normal implement it.
use super::*;
pub(super) trait QuorumValidationOps {
    fn validate_record(&mut self, r: &CanonicalRecordV1) -> Result<(), FormalError>;
    fn before_value<T: Serialize>(&mut self, value: &T) -> Result<(), FormalError>;
    fn before_context_clone(
        &mut self,
        r: &CanonicalRecordV1,
        a: &VerifiedAuthorityV1,
    ) -> Result<(), FormalError>;
    fn record_id(&mut self, r: &CanonicalRecordV1) -> Result<String, FormalError>;
    fn authority_id(&mut self, a: &AuthorityDefinitionV1) -> Result<String, FormalError>;
    fn instance_id(
        &mut self,
        i: &fencing::EndpointInstanceBindingV1,
    ) -> Result<String, FormalError>;
    fn hash<T: Serialize>(&mut self, domain: &str, value: &T) -> Result<String, FormalError>;
    fn verify<T: Serialize>(
        &mut self,
        domain: &str,
        value: &T,
        signature: &str,
        key: &str,
    ) -> Result<(), FormalError>;
    fn parent_continuity(
        &mut self,
        a: &VerifiedAuthorityV1,
        p: &VerifiedFormalCommitV1,
        n: u64,
    ) -> Result<(), FormalError>;
    fn frozen_certificate(
        &mut self,
        f: &FormalFrozenCertificateV1,
        a: &VerifiedAuthorityV1,
    ) -> Result<(), FormalError>;
    fn frozen_matches(
        &mut self,
        f: &FormalFrozenCertificateV1,
        r: &CanonicalRecordV1,
    ) -> Result<(), FormalError>;
    fn reservation(
        &mut self,
        f: &SignedFormalFrozenV1,
        a: &VerifiedAuthorityV1,
        p: &VerifiedFormalCommitV1,
    ) -> Result<(), FormalError>;
}
pub(super) struct LegacyQuorumOps;
impl QuorumValidationOps for LegacyQuorumOps {
    fn validate_record(&mut self, r: &CanonicalRecordV1) -> Result<(), FormalError> {
        r.validate()
    }
    fn before_value<T: Serialize>(&mut self, _: &T) -> Result<(), FormalError> {
        Ok(())
    }
    fn before_context_clone(
        &mut self,
        _: &CanonicalRecordV1,
        _: &VerifiedAuthorityV1,
    ) -> Result<(), FormalError> {
        Ok(())
    }
    fn record_id(&mut self, r: &CanonicalRecordV1) -> Result<String, FormalError> {
        r.record_id()
    }
    fn authority_id(&mut self, a: &AuthorityDefinitionV1) -> Result<String, FormalError> {
        a.authority_id()
    }
    fn instance_id(
        &mut self,
        i: &fencing::EndpointInstanceBindingV1,
    ) -> Result<String, FormalError> {
        i.digest().map_err(|e| invalid(e.to_string()))
    }
    fn hash<T: Serialize>(&mut self, d: &str, v: &T) -> Result<String, FormalError> {
        super::super::crypto::hash(d, v)
    }
    fn verify<T: Serialize>(
        &mut self,
        d: &str,
        v: &T,
        s: &str,
        k: &str,
    ) -> Result<(), FormalError> {
        super::super::crypto::verify(d, v, s, k)
    }
    fn parent_continuity(
        &mut self,
        a: &VerifiedAuthorityV1,
        p: &VerifiedFormalCommitV1,
        n: u64,
    ) -> Result<(), FormalError> {
        verify_normal_parent_continuity(a, p, n)
    }
    fn frozen_certificate(
        &mut self,
        f: &FormalFrozenCertificateV1,
        a: &VerifiedAuthorityV1,
    ) -> Result<(), FormalError> {
        f.verify(a.definition(), a.activation_ref()).map(|_| ())
    }
    fn frozen_matches(
        &mut self,
        f: &FormalFrozenCertificateV1,
        r: &CanonicalRecordV1,
    ) -> Result<(), FormalError> {
        f.matches_record(r)
    }
    fn reservation(
        &mut self,
        f: &SignedFormalFrozenV1,
        a: &VerifiedAuthorityV1,
        p: &VerifiedFormalCommitV1,
    ) -> Result<(), FormalError> {
        verify_formal_reservation(f, a, p).map(|_| ())
    }
}
