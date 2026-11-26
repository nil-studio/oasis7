//! Institutions bounded context: verified governance membership projections.
//!
//! Only read-only, provenance-binding views are moved in this increment.
//! Governance mutations still belong to the root World transaction until
//! the #6136 commitment mapping and #6141 staged-install acceptance close.

mod governance_candidate_projection;
mod governance_membership;
mod governance_projection;
mod governance_quote;

pub use governance_candidate_projection::WorldGovernanceCandidateRegistryProjectionV2;
pub use governance_membership::{
    RuntimeMembershipApplicationView, WorldGovernanceFinalityRegistryProjectionV1,
    WorldGovernanceMembershipView,
};
