use super::*;

#[cfg(unix)]
pub(crate) fn launch_operation_owner_with_fixed_trust(
    config: &Path,
    operation: &str,
    request: &str,
    config_digest: &str,
    expected_trust: &str,
    timeout: Duration,
) -> Result<FormalRuntimeOwnerSnapshotV1, String> {
    owner::launch_operation_with_fixed_trust(
        config,
        operation,
        request,
        config_digest,
        Some(expected_trust),
        timeout,
    )
}
#[cfg(unix)]
pub(crate) fn query_operation_owner_with_fixed_trust(
    config: &Path,
    operation: &str,
    request: &str,
    config_digest: &str,
    incarnation: Option<&str>,
    expected_trust: &str,
) -> Result<Option<FormalRuntimeOwnerSnapshotV1>, String> {
    owner::query_operation_with_fixed_trust(
        config,
        operation,
        request,
        config_digest,
        incarnation,
        expected_trust,
    )
}
#[cfg(unix)]
pub(crate) fn run_operation_owner_worker_with_fixed_trust(
    config: &Path,
    operation: &str,
    request: &str,
    config_digest: &str,
    expected_trust: Option<&str>,
) -> Result<(), String> {
    owner::run_operation_worker_with_fixed_trust(
        config,
        operation,
        request,
        config_digest,
        expected_trust,
    )
}
