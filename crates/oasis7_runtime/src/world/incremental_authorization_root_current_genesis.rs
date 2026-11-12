//! Actual World producer. No seven-map constructor or accepted-cache setter.
use super::*;
use crate::world::{
    World,
    incremental_authorization_root_projection::{
        AuthorityRevocationViewV1, AuthorizationRootProjectionV1,
    },
};

/// Original terminal validation stays before authorization serialization/hash.
/// Its single-record kernel is shared with TaskTerminalIndexV2::validate.
pub(super) fn validate_terminals(
    index: &oasis7_world_state::TaskTerminalIndexV2,
    limits: AuthorizationRootCacheLimitsV1,
    common: &mut FormalBoundedContextV1<'_>,
    class: MemoryClass,
) -> Result<(), AuthorizationRootCacheErrorV1> {
    common.consume_steps(1)?;
    let header_fee = common
        .memory()
        .reserve(std::mem::size_of::<String>() + 512, class)?;
    if let Err(error) = oasis7_world_state::validate_task_terminal_index_header_v2(index) {
        return Err(AuthorizationRootCacheErrorV1::TaskTerminal(
            header_fee.retain(error),
        ));
    }
    for (task_id, record) in &index.records {
        common.consume_steps(1)?;
        let shape = wire::measure(record, limits, common, class)?;
        // The fixed record kernel contains one request.validate and a fixed
        // list of scalar/fence equalities. CBOR bytes bound every original UTF8
        // byte and scalar; 64 pays those fixed validation/equality passes.
        let work = shape
            .bytes
            .checked_add(task_id.len() as u64)
            .and_then(|n| n.checked_mul(64))
            .and_then(|n| n.checked_add(4096))
            .ok_or_else(limited)?;
        common.charge(work, work)?;
        // Original predicate error strings remain owned and charged on error.
        // This is record-local scratch, not a reservation around whole-index
        // validation. Each record and each serializer poll the same Common stop.
        let scratch = usize::try_from(shape.bytes)
            .ok()
            .and_then(|n| n.checked_mul(6))
            .and_then(|n| n.checked_add(std::mem::size_of::<String>() + 4096))
            .ok_or_else(limited)?;
        let fee = common.memory().reserve(scratch, class)?;
        if let Err(error) = oasis7_world_state::validate_task_terminal_record_v2(task_id, record) {
            if error.capacity() > scratch - std::mem::size_of::<String>() {
                return Err(limited());
            }
            return Err(AuthorizationRootCacheErrorV1::TaskTerminal(
                fee.retain(error),
            ));
        }
        common.consume_steps(0)?;
    }
    Ok(())
}
impl World {
    /// Borrow the actual seven original owners. Output is only provisional
    /// canonical DATA: genuine seven-source joins and parent/tx binding remain
    /// mandatory before Root can install a qualified resident cache.
    pub(in crate::world) fn prepare_iws_authorization_projection_data_v1(
        &self,
        limits: AuthorizationRootCacheLimitsV1,
        common: &mut FormalBoundedContextV1<'_>,
        class: MemoryClass,
    ) -> Result<ProvisionalAuthorizationProjectionV1, AuthorizationRootCacheErrorV1> {
        limits.check()?;
        if let Some(index) = &self.capability_revocation_state.task_terminals_v2 {
            validate_terminals(index, limits, common, class)?;
        }
        // DecimalU64's fixed-size formatting and the maintained original serde
        // adapters are the only extra source-side temporaries. The bounded ABI
        // engine separately admits/sorts all map keys and pays every replay.
        let scalar_layout = limits
            .max_depth
            .checked_add(1)
            .and_then(|n| {
                n.checked_mul(
                    std::mem::size_of::<String>() + 39 + 1 + 2 * std::mem::align_of::<String>(),
                )
            })
            .ok_or_else(limited)?;
        let _scalar_scratch = common.memory().reserve(scalar_layout, class)?;
        let revocation = AuthorityRevocationViewV1(&self.capability_revocation_state);
        let original = AuthorizationRootProjectionV1 {
            grants: &self.capability_grants_v2,
            revocation: &revocation,
            invocation_contexts: &self.capability_invocation_contexts,
            budget_accounts: &self.capability_budget_accounts,
            nonce_records: &self.capability_nonce_records,
            receipts: &self.capability_authorization_receipts,
            effect_receipt_links: &self.capability_effect_receipt_links,
        };
        let raw = wire::encode(&original, limits, common, class)?;
        ProvisionalAuthorizationProjectionV1::read_hint_data(&raw, limits, common, class)
    }
}
