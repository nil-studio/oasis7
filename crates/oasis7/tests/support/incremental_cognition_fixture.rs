use oasis7_distfs::incremental_page_store::{PageIoBudget, PageStoreError};
use oasis7_formal_protocol::incremental_value::ValueTreeError;

pub(crate) fn physical() -> PageIoBudget {
    PageIoBudget {
        reads_remaining: 2_500_000,
        read_bytes_remaining: 256 << 20,
        writes_remaining: 100_000,
        write_bytes_remaining: 256 << 20,
    }
}

pub(crate) fn source(all_optional: bool) -> serde_json::Value {
    let mut value = serde_json::json!({"schema_version":"cognition-persistence.v1", "cognition_journal":{"schema_version":"cognition-journal.v1","head_seq":u64::MAX,"head_digest":"原digest","events":[]}, "responses":[],"commit_records":[],"receipt_registry":[],"receipt_lineage_registry":[],"feedback_outbox":{"":true},"idempotency_index":{},"staged_actions":{},"scheduler_state":{},"continuations":[null,""],"recovery":null,"continuation_contexts":{},"reorg_invalidation_epoch":0});
    if all_optional {
        value["cognition_economy"] =
            serde_json::to_value(oasis7_runtime::CognitionEconomyStateV1::default()).unwrap();
        value["cognition_economy"]["balances"] =
            serde_json::json!({"":{}, "账户":{"unit":{"available":u64::MAX,"reserved":0}}});
        value["cognition_economy"]["provisioned_balances"] = serde_json::json!({"原binding":{}});
        value["cognition_economy"]["provision_head_seq"] = serde_json::json!(42);
        value["cognition_economy"]["head_seq"] = serde_json::json!(u64::MAX);
        value["cognition_economy"]["head_digest"] = serde_json::json!("原economic-digest");
        value["cognition_economy"]["provision_head_digest"] = serde_json::json!("");
        value["agent_delegation"] = serde_json::json!({"grants":{"outer-id":{"grant_id":"embedded-id"}},"spent":{"":u64::MAX},"decisions":{},"receipts":{},"controls":{}});
        value["continuation_evaluations"] = serde_json::json!({"n":{"tick":u64::MAX}});
    }
    value
}

pub(crate) fn state_error(error: PageStoreError) -> ValueTreeError {
    match error {
        PageStoreError::Memory(error) => ValueTreeError::Memory(error),
        PageStoreError::ResourceLimited(error) => ValueTreeError::ResourceLimited(error),
        PageStoreError::Cancelled => ValueTreeError::Cancelled,
        error => ValueTreeError::Io(std::io::Error::other(error)),
    }
}
