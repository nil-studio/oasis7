//! Persist the genuine GEN projection as untrusted cold-test DATA. The normal
//! cold consumer must still authenticate all seven sources and the real head.
use crate::world::incremental_authorization_root_current::{
    AuthorizationRootCacheErrorV1, ProvisionalAuthorizationProjectionV1,
};
use oasis7_formal_protocol::controlled_authority::formal::bounded_context::FormalBoundedContextV1;

impl ProvisionalAuthorizationProjectionV1 {
    /// Fixture-only access to the same paid canonical writer used internally.
    /// Neither the bytes nor this provisional owner qualify accepted Current.
    pub fn write_hint_for_fixture_v1(
        &self,
        out: &mut impl std::io::Write,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<u64, AuthorizationRootCacheErrorV1> {
        self.write_hint(out, common)
    }
}
