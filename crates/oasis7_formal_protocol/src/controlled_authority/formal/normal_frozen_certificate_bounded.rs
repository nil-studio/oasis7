//! Fixed paid certificate verification; predicates remain in frozen's one kernel.
use super::bounded_context::FormalBoundedContextV1;
use super::crypto::invalid;
use super::fencing::EndpointInstanceBindingV1;
use super::normal_frozen_bounded::{Frame, frame, wire};
use super::*;
use oasis7_proto::memory_budget::{MemoryClass, ReservedValue};

struct PaidDigest(ReservedValue<String>);
impl AsRef<str> for PaidDigest {
    fn as_ref(&self) -> &str {
        self.0.as_str()
    }
}

struct Ops<'a, 'c> {
    ctx: &'a mut FormalBoundedContextV1<'c>,
}
impl super::frozen::CertificateValidationOps for Ops<'_, '_> {
    type Digest = PaidDigest;
    type Preimage = Frame;
    fn before_validation(
        &mut self,
        certificate: &FormalFrozenCertificateV1,
        authority: &AuthorityDefinitionV1,
    ) -> Result<(), FormalError> {
        let certificate = wire(certificate, self.ctx)?;
        let authority = wire(authority, self.ctx)?;
        let bytes = certificate
            .len()
            .checked_add(authority.len())
            .ok_or_else(|| invalid("Frozen certificate validation layout"))?;
        // Fixed body validators have only bounded field/role inventories; all
        // payload byte traversals are covered by the unique measured encoding.
        self.ctx.charge(
            u64::try_from(bytes).map_err(|_| invalid("Frozen certificate validation length"))?,
            bytes as u64,
        )
    }
    fn authority_id(
        &mut self,
        authority: &AuthorityDefinitionV1,
    ) -> Result<Self::Digest, FormalError> {
        normal_frozen_hash_bounded(super::record::AUTHORITY_ID_DOMAIN_V1, authority, self.ctx)
            .map(PaidDigest)
    }
    fn reservation_id(&mut self, body: &FormalFrozenBodyV1) -> Result<Self::Digest, FormalError> {
        body.validate()?;
        normal_frozen_hash_bounded("oasis7.formal.frozen_identity.v1", body, self.ctx)
            .map(PaidDigest)
    }
    fn instance_id(
        &mut self,
        instance: &EndpointInstanceBindingV1,
    ) -> Result<Self::Digest, FormalError> {
        let measured = wire(instance, self.ctx)?;
        self.ctx
            .charge(measured.len() as u64, measured.len() as u64)?;
        instance
            .validate()
            .map_err(|error| invalid(error.to_string()))?;
        let fee = self
            .ctx
            .memory()
            .reserve(64 + std::mem::size_of::<String>(), MemoryClass::Ordinary)
            .map_err(|_| invalid("Frozen instance digest admission"))?;
        let mut hash = blake3::Hasher::new();
        for chunk in measured.chunks(8192) {
            self.ctx.charge(1, chunk.len() as u64)?;
            hash.update(chunk);
        }
        self.ctx.charge(1, 64)?;
        let digest = hash.finalize().to_hex().to_string();
        if digest.capacity() != 64 {
            return Err(invalid("Frozen instance digest capacity"));
        }
        Ok(PaidDigest(fee.retain(digest)))
    }
    fn reservation_preimage(&mut self, body: &FormalFrozenBodyV1) -> Result<Frame, FormalError> {
        frame("oasis7.formal.frozen_reservation.sign.v1", body, self.ctx)
    }
    fn receipt_preimage(&mut self, body: &FormalFrozenReceiptBodyV1) -> Result<Frame, FormalError> {
        frame("oasis7.formal.frozen_receipt.sign.v1", body, self.ctx)
    }
    fn crypto_work(&mut self, bytes: usize) -> Result<(), FormalError> {
        self.ctx.charge(1, bytes as u64)
    }
}
pub fn verify_normal_frozen_certificate_bounded(
    certificate: &FormalFrozenCertificateV1,
    authority: &AuthorityDefinitionV1,
    activation_ref: &str,
    ctx: &mut FormalBoundedContextV1<'_>,
) -> Result<ReservedValue<String>, FormalError> {
    ctx.consume_steps(1)?;
    let _phase = ctx
        .memory()
        .reserve(1 << 20, MemoryClass::Ordinary)
        .map_err(|_| invalid("Frozen certificate phase admission"))?;
    super::frozen::verify_frozen_certificate_with(
        certificate,
        authority,
        activation_ref,
        &mut Ops { ctx },
    )
    .map(|digest| digest.0)
}

#[cfg(test)]
mod tests {
    use super::super::bounded_context::FormalBoundedWorkV1;
    use super::*;
    use oasis7_proto::memory_budget::MemoryBudget;
    #[test]
    fn actual_schema4_dual_receipt_matches_original_and_rejects_role_swap() {
        let setup = super::super::test_fixtures::incremental::setup();
        let certificate = super::super::test_fixtures::incremental::frozen(&setup);
        let expected = certificate
            .verify(
                setup.authority.definition(),
                setup.authority.activation_ref(),
            )
            .unwrap();
        let memory = MemoryBudget::new(32 << 20, 4 << 20).unwrap();
        let work = FormalBoundedWorkV1::new(10_000_000, 128 << 20).unwrap();
        let mut stop = || false;
        let paid = verify_normal_frozen_certificate_bounded(
            &certificate,
            setup.authority.definition(),
            setup.authority.activation_ref(),
            &mut FormalBoundedContextV1::new(&memory, &work, &mut stop),
        )
        .unwrap();
        assert_eq!(*paid, expected);
        drop(paid);
        let mut wrong = certificate;
        std::mem::swap(&mut wrong.primary, &mut wrong.replica);
        let old = wrong
            .verify(
                setup.authority.definition(),
                setup.authority.activation_ref(),
            )
            .unwrap_err()
            .to_string();
        let error = verify_normal_frozen_certificate_bounded(
            &wrong,
            setup.authority.definition(),
            setup.authority.activation_ref(),
            &mut FormalBoundedContextV1::new(&memory, &work, &mut stop),
        )
        .unwrap_err();
        assert_eq!(error.to_string(), old);
    }
    #[test]
    fn actual_schema4_dual_receipt_cancel_and_zero_pool_do_not_verify() {
        let setup = super::super::test_fixtures::incremental::setup();
        let certificate = super::super::test_fixtures::incremental::frozen(&setup);
        let memory = MemoryBudget::new(0, 0).unwrap();
        let work = FormalBoundedWorkV1::new(10_000_000, 128 << 20).unwrap();
        let mut stop = || false;
        assert!(
            verify_normal_frozen_certificate_bounded(
                &certificate,
                setup.authority.definition(),
                setup.authority.activation_ref(),
                &mut FormalBoundedContextV1::new(&memory, &work, &mut stop)
            )
            .is_err()
        );
        let memory = MemoryBudget::new(32 << 20, 4 << 20).unwrap();
        let mut stop = || true;
        assert!(
            verify_normal_frozen_certificate_bounded(
                &certificate,
                setup.authority.definition(),
                setup.authority.activation_ref(),
                &mut FormalBoundedContextV1::new(&memory, &work, &mut stop)
            )
            .is_err()
        );
    }
}
