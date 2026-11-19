//! Provenance for the original native provisioning path. This does not grant
//! acceptance or claim that existing capture/registration use Common budgets.
use oasis7_distfs::controlled_authority::formal::coordinator::FormalDurabilityEndpoint;
use oasis7_formal_protocol::{
    controlled_authority::formal::bounded_context::FormalBoundedContextV1,
    incremental_artifacts::OriginalArtifactRefV1, native_environment::NativeExecutionEnvironmentV1,
};
use oasis7_proto::memory_budget::ReservedValue;

/// Real pre-approval capture breaks the C0/native/RootSet bootstrap cycle. It
/// has no endpoint registration, accepted-source or worker execution authority.
pub(crate) struct CapturedGenesisNativeV1 {
    descriptor: NativeExecutionEnvironmentV1,
}
impl CapturedGenesisNativeV1 {
    pub(crate) fn capture() -> Result<Self, String> {
        Ok(Self {
            descriptor: crate::native_environment_capture::capture(|_, _| Ok(()))?,
        })
    }
    pub(super) fn descriptor(&self) -> &NativeExecutionEnvironmentV1 {
        &self.descriptor
    }
    pub(crate) fn original_digest_bounded(
        &self,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<[u8; 32], super::IwsGenesisTransferErrorV1> {
        super::business_originals::execution_inputs::native_digest(&self.descriptor, common)
    }
    pub(crate) fn stage<P: FormalDurabilityEndpoint, R: FormalDurabilityEndpoint>(
        &self,
        primary: &mut P,
        replica: &mut R,
    ) -> Result<StagedGenesisNativeV1, String> {
        let descriptor = super::super::formal_native_environment::stage_current(primary, replica)?;
        if descriptor != self.descriptor {
            // A changed real environment leaves no selectable GEN owner. Both
            // original verification jobs must be released; report both cleanup
            // results if either endpoint cannot discharge that obligation.
            let id = descriptor.digest()?;
            let left = primary.release_native_environment_verification(&id);
            let right = replica.release_native_environment_verification(&id);
            return Err(format!(
                "GEN native environment changed after approval; primary release={left:?}, replica release={right:?}"
            ));
        }
        Ok(StagedGenesisNativeV1 { descriptor })
    }
    #[cfg(test)]
    pub(super) fn descriptor_fixture(descriptor: NativeExecutionEnvironmentV1) -> Self {
        // Codec tests only, never a shipping native/worker proof. No test path
        // constructs StagedGenesisNativeV1 or bypasses production endpoint gates.
        Self { descriptor }
    }
}

/// Only actual pinned capture followed by both endpoints' original native
/// registration can construct this production DATA owner. No caller digest,
/// executable path, boolean or public descriptor constructor substitutes for it.
pub(crate) struct StagedGenesisNativeV1 {
    descriptor: NativeExecutionEnvironmentV1,
}

impl StagedGenesisNativeV1 {
    pub(in super::super) fn validate_original(
        &self,
        original: (OriginalArtifactRefV1, &ReservedValue<Vec<u8>>),
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<(), super::IwsGenesisTransferErrorV1> {
        super::business_originals::execution_inputs::validate_native_original(
            &self.descriptor,
            original,
            common,
        )
    }
}
