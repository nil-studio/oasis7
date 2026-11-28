//! A response header may promote native state only when endpoint key custody
//! attests this exact RPC preimage under the independently pinned current floor.
use super::*;
use oasis7_proto::memory_budget::MemoryReservation;

// Caller-paid Query must account for both protected policy clones before
// either verifier can allocate. The fee outlives both checks; it is not an
// authorization token and does not change their original error ordering.
fn prepay_policy_clone_graph(
    provider_public_key: &str,
    bindings: &[String],
    ctx: &mut FormalBoundedContextV1<'_>,
) -> Result<MemoryReservation, FormalError> {
    ctx.consume_steps(1)?;
    let one = bindings.iter().try_fold(
        std::mem::size_of::<Vec<String>>()
            .checked_add(provider_public_key.len())
            .and_then(|n| n.checked_add(std::mem::size_of::<String>()))
            .and_then(|n| {
                bindings
                    .len()
                    .checked_mul(std::mem::size_of::<String>())
                    .and_then(|b| n.checked_add(b))
            })
            .ok_or_else(|| invalid("formal RPC policy clone size overflow"))?,
        |n, s| {
            n.checked_add(s.len())
                .ok_or_else(|| invalid("formal RPC policy clone size overflow"))
        },
    )?;
    let both = one
        .checked_mul(2)
        .ok_or_else(|| invalid("formal RPC policy clone size overflow"))?;
    let work_bytes =
        u64::try_from(both).map_err(|_| invalid("formal RPC policy clone work overflow"))?;
    ctx.charge(1, work_bytes)?;
    ctx.memory()
        .reserve(both, MemoryClass::Ordinary)
        .map_err(|_| invalid("formal RPC policy clone admission"))
}

pub(super) fn prepay_response_policy_clones(
    authority: &VerifiedAuthorityV1,
    ctx: &mut FormalBoundedContextV1<'_>,
) -> Result<MemoryReservation, FormalError> {
    prepay_policy_clone_graph(
        &authority.policy().fencing_provider_public_key,
        &authority.policy().authorized_fencing_resource_bindings,
        ctx,
    )
}

#[cfg(test)]
mod paid_policy_tests {
    use super::*;
    use oasis7_formal_protocol::controlled_authority::formal::bounded_context::FormalBoundedWorkV1;
    use oasis7_proto::memory_budget::MemoryBudget;

    #[test]
    fn caller_paid_policy_clones_hold_both_real_graphs_and_release() {
        let key = "ab".repeat(32);
        let bindings = vec!["resource-a".to_owned(), "resource-b".to_owned()];
        let one = std::mem::size_of::<Vec<String>>()
            + std::mem::size_of::<String>()
            + key.len()
            + bindings.len() * std::mem::size_of::<String>()
            + bindings.iter().map(String::len).sum::<usize>();
        let bytes = 2 * one;
        let work = FormalBoundedWorkV1::new(30, 1 << 20).unwrap();
        let pool = MemoryBudget::new(bytes, 0).unwrap();
        let mut stop = || false;
        let mut ctx = FormalBoundedContextV1::new(&pool, &work, &mut stop);
        let remaining = (work.remaining_steps(), work.remaining_read_bytes());
        let fee = prepay_policy_clone_graph(&key, &bindings, &mut ctx).unwrap();
        assert_eq!(pool.used(), bytes);
        assert!(work.remaining_steps() < remaining.0);
        assert!(work.remaining_read_bytes() < remaining.1);
        drop(fee);
        assert_eq!(pool.used(), 0);
        let too_small = MemoryBudget::new(bytes - 1, 0).unwrap();
        let error = prepay_policy_clone_graph(
            &key,
            &bindings,
            &mut FormalBoundedContextV1::new(&too_small, &work, &mut || false),
        )
        .unwrap_err();
        assert_eq!(error, invalid("formal RPC policy clone admission"));
        assert_eq!(too_small.used(), 0);
        assert_eq!(
            prepay_policy_clone_graph(
                &key,
                &bindings,
                &mut FormalBoundedContextV1::new(&pool, &work, &mut || true),
            )
            .unwrap_err(),
            invalid("bounded archive read cancelled")
        );
        assert_eq!(pool.used(), 0);
    }
}

pub(super) fn verify_response_custody(
    response: &Response,
    authority: &VerifiedAuthorityV1,
    role: EndpointRole,
    response_digest: &str,
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
    let signature = fencing::verify_protected_endpoint_signature(
        attestation,
        &request,
        instance,
        &fencing::TrustedFencingPolicyV1 {
            provider_public_key: authority.policy().fencing_provider_public_key.clone(),
            authorized_resource_bindings: authority
                .policy()
                .authorized_fencing_resource_bindings
                .clone(),
        },
    )
    .map_err(super::super::super::endpoint::fencing_error)?;
    if signature != response.signature
        || attestation.body.resource_binding != response.body.provider.statement.resource_binding
    {
        return Err(FormalError::FencingUnproven);
    }
    Ok(())
}
