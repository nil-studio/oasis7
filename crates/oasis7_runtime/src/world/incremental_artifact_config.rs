//! Borrow original live Runtime configuration; no deployment or policy grant.
use super::{ReleaseSecurityPolicy, World};
use oasis7_formal_protocol::{
    incremental_execution::ExecutionWriteV1,
    incremental_state::StatePageWriter,
    incremental_value::{ValueNodeReader, ValueTreeError},
};
use oasis7_proto::memory_budget::ReservedValue;
use oasis7_world_state::{
    incremental_field_member::{MemberRecordError, MemberRecordWriteContext},
    incremental_runtime_artifact_config::*,
};
#[cfg(test)]
#[path = "incremental_artifact_config_physical_tests.rs"]
mod physical_tests;

impl World {
    pub fn prepare_incremental_artifact_config_genesis_v1<W, C, S>(
        &self,
        context: &mut MemberRecordWriteContext<'_, W, C>,
        work: &mut ArtifactConfigWriteWorkV1,
        sink: &mut S,
    ) -> Result<(), MemberRecordError>
    where
        W: StatePageWriter + ValueNodeReader,
        C: FnMut() -> bool,
        S: FnMut(ReservedValue<ExecutionWriteV1>) -> Result<(), ValueTreeError>,
    {
        let ReleaseSecurityPolicy {
            allow_builtin_manifest_fallback,
            allow_identity_hash_signature,
            allow_local_finality_signing,
            allow_runtime_source_compile,
        } = &self.release_security_policy;
        prepare_artifact_config_genesis_v1(
            &ArtifactConfigSourceRefV1 {
                manifest: &self.manifest,
                registry: &self.module_registry,
                limits: &self.module_limits_max,
                security: ArtifactSecurityPolicyRefV1 {
                    allow_builtin_manifest_fallback: *allow_builtin_manifest_fallback,
                    allow_identity_hash_signature: *allow_identity_hash_signature,
                    allow_local_finality_signing: *allow_local_finality_signing,
                    allow_runtime_source_compile: *allow_runtime_source_compile,
                },
            },
            context,
            work,
            sink,
        )
    }
}
