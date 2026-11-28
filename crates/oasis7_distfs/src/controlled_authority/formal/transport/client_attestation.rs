//! A response header may promote native state only when endpoint key custody
//! attests this exact RPC preimage under the independently pinned current floor.
use super::*;

fn with_policy_clone<T>(
    key: &str,
    bindings: &[String],
    paid: Option<&mut FormalBoundedContextV1<'_>>,
    verify: impl FnOnce(&fencing::TrustedFencingPolicyV1) -> Result<T, FormalError>,
) -> Result<T, FormalError> {
    let fee = if let Some(ctx) = paid {
        let overflow = || invalid("formal RPC policy clone size overflow");
        let mut bytes = std::mem::size_of::<fencing::TrustedFencingPolicyV1>()
            .checked_add(key.len())
            .and_then(|n| {
                bindings
                    .len()
                    .checked_mul(std::mem::size_of::<String>())
                    .and_then(|b| n.checked_add(b))
            })
            .ok_or_else(overflow)?;
        ctx.charge(1, u64::try_from(bytes).map_err(|_| overflow())?)?;
        for binding in bindings {
            ctx.charge(1, u64::try_from(binding.len()).map_err(|_| overflow())?)?;
            bytes = bytes.checked_add(binding.len()).ok_or_else(overflow)?;
        }
        Some(
            ctx.memory()
                .reserve(bytes, MemoryClass::Ordinary)
                .map_err(|_| invalid("formal RPC policy clone admission"))?,
        )
    } else {
        None
    };
    // The ordinary fee remains live throughout the actual clone and verifier.
    // The two original verifiers run sequentially, so their fees do not overlap.
    let policy = fencing::TrustedFencingPolicyV1 {
        provider_public_key: key.to_owned(),
        authorized_resource_bindings: bindings.to_vec(),
    };
    if fee.is_some()
        && (policy.provider_public_key.capacity() != key.len()
            || policy.authorized_resource_bindings.capacity() != bindings.len()
            || policy
                .authorized_resource_bindings
                .iter()
                .zip(bindings)
                .any(|(copy, original)| copy.capacity() != original.len()))
    {
        return Err(invalid("formal RPC policy clone capacity"));
    }
    verify(&policy)
}

pub(super) fn with_response_policy<T>(
    authority: &VerifiedAuthorityV1,
    paid: Option<&mut FormalBoundedContextV1<'_>>,
    verify: impl FnOnce(&fencing::TrustedFencingPolicyV1) -> Result<T, FormalError>,
) -> Result<T, FormalError> {
    with_policy_clone(
        &authority.policy().fencing_provider_public_key,
        &authority.policy().authorized_fencing_resource_bindings,
        paid,
        verify,
    )
}

pub(super) fn verify_response_custody(
    response: &Response,
    authority: &VerifiedAuthorityV1,
    role: EndpointRole,
    response_digest: &str,
    paid: Option<&mut FormalBoundedContextV1<'_>>,
) -> Result<(), FormalError> {
    let instance = match role {
        EndpointRole::Primary => &authority.definition().primary.instance,
        EndpointRole::Replica => &authority.definition().replica.instance,
    };
    let attestation = response
        .protected_signature
        .as_ref()
        .ok_or(FormalError::FencingUnproven)?;
    let request = fencing::ProtectedEndpointSigningRequestV1 {
        schema_version: 1,
        instance_digest: instance
            .digest()
            .map_err(super::super::super::endpoint::fencing_error)?,
        checkpoint: response.body.checkpoint.clone(),
        challenge: attestation.body.challenge.clone(),
        purpose: fencing::ProtectedEndpointSignaturePurposeV1::RpcResponse,
        preimage: bytes(RESPONSE_DOMAIN, &response_digest)?,
    };
    with_response_policy(authority, paid, |policy| {
        let signature =
            fencing::verify_protected_endpoint_signature(attestation, &request, instance, policy)
                .map_err(super::super::super::endpoint::fencing_error)?;
        if signature != response.signature
            || attestation.body.resource_binding != response.body.provider.statement.resource_binding
        {
            return Err(FormalError::FencingUnproven);
        }
        Ok(())
    })
}

#[cfg(test)]
mod paid_policy_tests {
    use super::*;
    use oasis7_formal_protocol::controlled_authority::formal::bounded_context::FormalBoundedWorkV1;
    use oasis7_proto::memory_budget::MemoryBudget;

    #[test]
    fn caller_paid_policy_clones_hold_real_graphs_and_release() {
        let key = "ab".repeat(32);
        let bindings = vec!["resource-a".to_owned(), "resource-b".to_owned()];
        let bytes = std::mem::size_of::<fencing::TrustedFencingPolicyV1>()
            + key.len()
            + bindings.len() * std::mem::size_of::<String>()
            + bindings.iter().map(String::len).sum::<usize>();
        let work = FormalBoundedWorkV1::new(30, 1 << 20).unwrap();
        let pool = MemoryBudget::new(bytes, 0).unwrap();
        let mut stop = || false;
        let mut ctx = FormalBoundedContextV1::new(&pool, &work, &mut stop);
        let remaining = (work.remaining_steps(), work.remaining_read_bytes());
        for _ in 0..2 {
            with_policy_clone(&key, &bindings, Some(&mut ctx), |policy| {
                assert_eq!(pool.used(), bytes);
                assert_eq!(policy.provider_public_key, key);
                assert_eq!(policy.authorized_resource_bindings, bindings);
                Ok(())
            })
            .unwrap();
            assert_eq!(pool.used(), 0);
        }
        assert!(work.remaining_steps() < remaining.0);
        assert!(work.remaining_read_bytes() < remaining.1);
        let too_small = MemoryBudget::new(bytes - 1, 0).unwrap();
        let mut ctx = FormalBoundedContextV1::new(&too_small, &work, &mut stop);
        let error = with_policy_clone::<()>(&key, &bindings, Some(&mut ctx), |_| {
            panic!("unpaid verifier ran")
        })
        .unwrap_err();
        assert_eq!(error, invalid("formal RPC policy clone admission"));
        assert_eq!(too_small.used(), 0);
        let mut checks = 0;
        let mut cancel = || {
            checks += 1;
            checks > 1
        };
        let mut ctx = FormalBoundedContextV1::new(&pool, &work, &mut cancel);
        let result: Result<(), FormalError> =
            with_policy_clone(&key, &bindings, Some(&mut ctx), |_| {
                panic!("cancelled verifier ran")
            });
        assert_eq!(result.unwrap_err(), invalid("bounded archive read cancelled"));
        assert_eq!(pool.used(), 0);
    }
}
