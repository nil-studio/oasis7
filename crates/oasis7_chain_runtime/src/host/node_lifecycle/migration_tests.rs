use super::{
    COPY_METADATA_RESERVE_PER_ENTRY, COPY_SPACE_RESERVE_BYTES, MAX_COPY_BYTES,
    MigrationCapacityOutcomeV1, capacity_outcome,
};

#[test]
fn migration_capacity_preview_uses_aggregate_budget_and_reserve() {
    let source_bytes = 20 * 1024 * 1024;
    let entries = 3;
    let required =
        source_bytes + COPY_SPACE_RESERVE_BYTES + entries * COPY_METADATA_RESERVE_PER_ENTRY;
    assert_eq!(
        capacity_outcome(source_bytes, entries, required),
        MigrationCapacityOutcomeV1::Ready
    );
    assert_eq!(
        capacity_outcome(source_bytes, entries, required - 1),
        MigrationCapacityOutcomeV1::CapacityInsufficient
    );
    assert_eq!(
        capacity_outcome(MAX_COPY_BYTES + 1, entries, u64::MAX),
        MigrationCapacityOutcomeV1::SourceSizeLimitExceeded
    );
}
