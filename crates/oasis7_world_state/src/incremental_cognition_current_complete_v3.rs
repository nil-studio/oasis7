//! Complete current Cognition only: no selected head, History or lease grant.
pub use crate::incremental_cognition_family::{
    CognitionCurrentCompleteWorkV3, CognitionCurrentPresenceV3,
    CompleteCognitionCurrentQualificationV3, qualify_complete_cognition_current_v3,
    readmit_complete_cognition_current_ordinary_v3,
};

pub(crate) use crate::incremental_cognition_family::accept_complete_cognition_current_verified_v3;

#[cfg(test)]
#[path = "incremental_cognition_current_complete_tests_v3.rs"]
mod tests;
