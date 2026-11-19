//! Preserve the precise failure category when Current warm source admission fails.
use super::*;

pub(super) fn source_error(
    error: crate::world::incremental_authorization_root_current::WarmError,
) -> BodyMutationError {
    use crate::world::incremental_authorization_root_current::{SourceError, WarmError};
    match error {
        WarmError::Body(e) => e,
        WarmError::Source(SourceError::Execution(e)) => e.into(),
        WarmError::Source(SourceError::Overlay(e)) => overlay_error(e),
        WarmError::Source(other) => {
            #[derive(Debug)]
            struct Cause(SourceError);
            impl std::fmt::Display for Cause {
                fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                    write!(f, "{:?}", self.0)
                }
            }
            impl std::error::Error for Cause {}
            BodyMutationError::Original(Box::new(Cause(other)))
        }
    }
}
