use super::fixture::physical;
use oasis7_distfs::incremental_page_store::PageIoBudget;
use oasis7_formal_protocol::{
    incremental_state::StateScanBudget, incremental_value::ValueTreeError,
};
use oasis7_runtime::WorldState;

pub(crate) fn state_error(
    error: oasis7_distfs::incremental_page_store::PageStoreError,
) -> ValueTreeError {
    use oasis7_distfs::incremental_page_store::PageStoreError as E;
    match error {
        E::Memory(e) => ValueTreeError::Memory(e),
        E::ResourceLimited(reason) => ValueTreeError::ResourceLimited(reason),
        E::Cancelled => ValueTreeError::Cancelled,
        E::Io(e) => ValueTreeError::Io(e),
        E::Missing(_) => ValueTreeError::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "factory authority physical page missing",
        )),
        E::Corrupt(reason) => ValueTreeError::Invalid(reason),
        E::UnsupportedPlatform => ValueTreeError::Invalid("factory authority platform"),
    }
}

pub(crate) fn physical_scope() -> PageIoBudget {
    let mut io = physical();
    // This one functional session includes four independent full child proofs
    // and every literal column query. The initial 1M-operation session reached
    // Raw/Ordinary/empty/AuthorityRevision and failed with the precise physical
    // read allowance. Keep the byte bound; this is no performance threshold.
    io.reads_remaining = 2_000_000;
    io
}

pub(crate) fn scan() -> StateScanBudget {
    StateScanBudget {
        nodes_remaining: 100_000,
        read_bytes_remaining: 64 << 20,
        leaves_remaining: 1000,
    }
}

pub(crate) fn source() -> WorldState {
    let mut source = WorldState::default();
    source.factory_site_authorities.insert(" outer-site\0界 ".into(),serde_json::from_value(serde_json::json!({
        "site_id":" inner-site\0界 ","location_id":"","owner_agent_id":"owner","authorized_agent_ids":["z","","z"],
        "chunk_ready":false,"active":true,"authority_revision":u64::MAX,"registered_at":17})).unwrap());
    for (outer, module) in [
        ("none", None),
        ("empty", Some("")),
        ("text", Some(" raw-module\0界 ")),
    ] {
        source.factory_construction_power_profiles.insert(outer.into(),serde_json::from_value(serde_json::json!({
            "factory_id":"different-inner-id","factory_kind":"","source_module_id":module,"electricity_amount":i64::MIN,
            "mode":"start_only_sink","authority_revision":u64::MAX,"active":false})).unwrap());
    }
    source
}
