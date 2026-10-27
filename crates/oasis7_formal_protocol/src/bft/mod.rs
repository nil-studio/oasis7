//! Pure, offline-verifiable BFT V2 proof contracts.
//!
//! This module contains protocol validation only. It does not execute blocks,
//! persist consensus state, or depend on local storage services.

mod certificate;
mod formal_handoff;
pub mod priority_wire;
mod proposer;
mod validator_set;
mod verified_commit;

pub use certificate::verify_commit_certificate_v2;
pub use formal_handoff::{
    BFT_FORMAL_EXECUTION_COMMITMENT_MAX_BYTES_V1, BFT_FORMAL_EXECUTION_DOMAIN_V1,
    BFT_FORMAL_PREFIX_MAX_BYTES_V1, BFT_FORMAL_PREFIX_MAX_HEIGHTS_V1, BFT_FORMAL_PROFILE_V1,
    BFT_FORMAL_PROFILE_VERSION_V1, BFT_FORMAL_TICK_MAPPING_RULESET_V1, BftProfileTargetV1,
    FormalBftCertifiedEntryV1, FormalBftCertifiedPrefixV1, FormalBftExecutionCommitmentV1,
    VerifiedBftHandoffTargetV1, VerifiedFormalBftPrefixV1, verify_formal_bft_handoff_v1,
};
pub use proposer::{BftProposerPriorityV2, BftProposerV2};
pub use validator_set::{
    BFT_EPOCH_MAPPING_RULESET_V2, BFT_HEIGHT_SCHEDULE_RULESET_V2, BFT_MAX_ACTIONS_BYTES_V2,
    BFT_MAX_ROUNDS_PER_HEIGHT_V2, BFT_MAX_VALIDATORS_PER_TRANSITION_V2,
    BFT_VALIDATOR_CHANGE_RULESET_V2, BftSchedulePolicyV2, BftValidatorSetV2,
};
pub use verified_commit::{
    BftHeightProposerScheduleV2, VerifiedBftCommitCertificateV2, verify_commit_certificate_token_v2,
};
