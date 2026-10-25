//! Fixed-validator-set Tendermint-style consensus primitives.
//!
//! This module is intentionally separate from the legacy PoS engine. Its
//! commit certificate is not a legacy commit receipt or a formal profile
//! proof.

mod engine;
mod proposer;
mod signing_journal;
mod validator_set;
mod verified_commit;
mod vote_tally;

pub use engine::{
    BFT_MAX_ROUNDS_PER_HEIGHT_V1, BftConsensusActionV1, BftConsensusMachineV1, BftProposalIntentV1,
    BftStepV1, BftTimeoutsV1, BftVoteIntentV1, verify_commit_certificate_v1,
};
pub use proposer::{BftProposerPriorityV1, BftProposerV1};
pub use signing_journal::{
    BFT_MAX_SIGNING_JOURNAL_BYTES_V1, BftSigningJournalEventV1, BftSigningJournalV1,
    complete_reserved_bft_proposal_v1, complete_reserved_bft_vote_v1,
    replay_bft_signing_journal_v1, verify_bft_signing_journal_transition_v1,
};
pub use validator_set::{
    BFT_HEIGHT_SCHEDULE_RULESET_V2, BFT_MAX_VALIDATORS_V1, BftValidatorSetV1, BftValidatorV1,
};
pub use verified_commit::{
    BftHeightProposerScheduleV1, VerifiedBftCommitCertificateV1, verify_commit_certificate_token_v1,
};
