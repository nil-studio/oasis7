//! Borrowed call inputs for bounded endpoint publication. These references
//! neither own a provider FD nor grant publication or execution authority.
use super::FormalArchiveReadWorkV1;
use crate::controlled_authority::fencing::{
    RemoteMonotonicEndpointProvider, TrustedFencingPolicyV1,
};
use oasis7_proto::memory_budget::MemoryBudget;

#[derive(Clone, Copy)]
pub struct FormalEndpointPublicationRefsV1<'a> {
    pub provider: &'a RemoteMonotonicEndpointProvider,
    pub policy: &'a TrustedFencingPolicyV1,
    pub memory: &'a MemoryBudget,
    pub work: &'a FormalArchiveReadWorkV1,
}

#[derive(Clone, Copy)]
pub struct FormalEndpointPairPublicationRefsV1<'a> {
    pub primary_provider: &'a RemoteMonotonicEndpointProvider,
    pub replica_provider: &'a RemoteMonotonicEndpointProvider,
    pub primary_policy: &'a TrustedFencingPolicyV1,
    pub replica_policy: &'a TrustedFencingPolicyV1,
    pub memory: &'a MemoryBudget,
    pub work: &'a FormalArchiveReadWorkV1,
}
