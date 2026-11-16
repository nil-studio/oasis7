//! Legacy reload delegates to the same native checkpoint verifier. Cold
//! reconstruction may retain its real predecessor without re-reading proof.
use super::*;
impl FileFormalEndpoint {
    pub(super) fn reload_parent(
        &mut self,
        provider: &fencing::MonotonicProviderEvidenceV1,
    ) -> Result<(), FormalError> {
        self.reload_parent_with_predecessor(provider).map(|_| ())
    }
}
