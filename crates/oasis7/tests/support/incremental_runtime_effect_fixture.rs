use super::fixture;
use oasis7_distfs::incremental_page_store::PageIoBudget;
use oasis7_runtime::incremental_field_member::MemberRecordLimitsV1;

pub(crate) fn physical() -> PageIoBudget {
    PageIoBudget {
        reads_remaining: 1_250_000,
        ..fixture::physical()
    }
}

pub(crate) fn limits() -> MemberRecordLimitsV1 {
    MemberRecordLimitsV1 {
        semantic_bytes: 4096,
        payload_bytes: 165,
        record_bytes: 8192,
    }
}
