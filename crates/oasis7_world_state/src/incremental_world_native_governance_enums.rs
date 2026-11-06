//! Closed original enum variants; even unit variants have a tagged child.
use super::*;
tagged_codec!(crate::governance::GovernanceValidatorAdmissionStatus,units{Applied=0,ApprovedCandidate=1,ProbationReady=2,Active=3,Revoked=4},tuples{},records{});
tagged_codec!(oasis7_agent_api::world_action::FinalityKeyRotationRoleV1,units{OldKeyAuthorization=0,NewKeyPossession=1},tuples{},records{});
