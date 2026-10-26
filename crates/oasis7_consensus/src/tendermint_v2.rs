//! Dynamic-set BFT V2 consensus primitives. V2 has separate wire/signing
//! domains and never reinterprets V1 histories.

mod engine;
mod priority_wire;
mod proposer;
mod signing_journal;
mod validator_set;
mod verified_commit;

pub use engine::{
    BFT_MAX_CACHED_PROPOSAL_BYTES_V2, BFT_MAX_CACHED_VOTES_V2, BFT_MAX_EQUIVOCATIONS_V2,
    BFT_MAX_PROPOSAL_EQUIVOCATIONS_V2, BftConsensusActionV2, BftConsensusMachineV2,
    BftProposalIntentV2, BftStepV2, BftVoteIntentV2, verify_commit_certificate_v2,
};
pub use proposer::{BftProposerPriorityV2, BftProposerV2};
pub use signing_journal::{
    BFT_MAX_SIGNING_JOURNAL_BYTES_V2, BftSigningJournalEventV2, BftSigningJournalV2,
    complete_reserved_bft_proposal_v2, complete_reserved_bft_vote_v2,
    replay_bft_signing_journal_v2, replay_bft_signing_journal_with_effects_v2,
    verify_bft_signing_journal_transition_v2, verify_reserved_bft_commit_transition_v2,
};
pub use validator_set::{
    BFT_EPOCH_MAPPING_RULESET_V2, BFT_HEIGHT_SCHEDULE_RULESET_V2, BFT_MAX_ACTIONS_BYTES_V2,
    BFT_MAX_ROUNDS_PER_HEIGHT_V2, BFT_MAX_VALIDATORS_PER_TRANSITION_V2,
    BFT_VALIDATOR_CHANGE_RULESET_V2, BftSchedulePolicyV2, BftValidatorSetV2,
};
pub use verified_commit::{
    BftHeightProposerScheduleV2, VerifiedBftCommitCertificateV2, verify_commit_certificate_token_v2,
};
