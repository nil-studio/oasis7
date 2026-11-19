//! The original journal-retention resource setting from the real typed cold
//! Host startup configuration. It is not an accepted-state qualification.
use super::*;
pub(in crate::world) fn cold_configuration_retention_v1(
    configuration: &crate::world::incremental_normal_host_cold_sources::NormalHostColdRuntimeConfigurationV1,
    common: &mut oasis7_formal_protocol::controlled_authority::formal::bounded_context::FormalBoundedContextV1<'_>,
) -> Result<
    ReservedValue<WorldJournalRetentionV1>,
    oasis7_formal_protocol::controlled_authority::formal::FormalError,
> {
    configuration.validate_context(common)?;
    common.charge(1, std::mem::size_of::<WorldJournalRetentionV1>() as u64)?;
    let fee = common
        .memory()
        .reserve(
            std::mem::size_of::<WorldJournalRetentionV1>(),
            MemoryClass::Ordinary,
        )
        .map_err(|_| {
            oasis7_formal_protocol::controlled_authority::formal::FormalError::Invalid(
                "cold journal retention admission".into(),
            )
        })?;
    Ok(fee.retain(WorldJournalRetentionV1 {
        max_events: configuration.limits().max_journal_events.max(1),
        class: MemoryClass::Ordinary,
    }))
}
