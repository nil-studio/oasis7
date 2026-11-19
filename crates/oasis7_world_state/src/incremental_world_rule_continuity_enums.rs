//! Frozen continuity variants; unit variants remain distinct from Option::None.
use super::*;
tagged_codec!(crate::world_rules::OrganizationAvailabilityEvidenceStatusV1,units {Unknown = 0,Certified = 1},tuples {},records {});
tagged_codec!(crate::world_rules::OrganizationActivityKindV1,units {FulfilledDelivery = 0,ExternalServiceSettlement = 1,ExternalAtomicExchangeSettlement = 2,NonCircularProductionDelivery = 3,AuthorizedBusinessDecision = 4},tuples {},records {});
tagged_codec!(crate::world_rules::OrganizationContinuityPhaseV1,units {AwaitingNotice = 0,Notice = 1,Protection = 2,CandidateCollection = 3,Protected = 4,Disposition = 5,Resolved = 6,Cancelled = 7},tuples {},records {});
tagged_codec!(crate::world_rules::OrganizationCaseOriginV1,units {Dormancy = 0,VoluntaryDissolution = 1,VoluntaryReorganization = 2},tuples {},records {});
tagged_codec!(crate::world_rules::OrganizationDispositionStageV1,units {None = 0,Frozen = 1,ExistingContracts = 2,CustodyReturn = 3,ClaimsAndCosts = 4,BusinessTransfer = 5,Residual = 6,Complete = 7},tuples {},records {});
tagged_codec!(oasis7_agent_api::world_action::OrganizationRemedyKindV1,units {Reclaim = 0,Appeal = 1},tuples {},records {});
tagged_codec!(oasis7_agent_api::world_action::OrganizationRemedyOutcomeV1,units {Rejected = 0,Withdrawn = 1,Supported = 2,PartiallySupported = 3},tuples {},records {});
