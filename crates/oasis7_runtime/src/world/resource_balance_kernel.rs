//! Shared global resource arithmetic used by materialized and root execution.
use crate::WorldError;
use oasis7_agent_api::world_action::ResourceKind;
pub(super) fn checked_next(
    kind: ResourceKind,
    current: i64,
    amount: i64,
) -> Result<i64, WorldError> {
    current
        .checked_add(amount)
        .ok_or_else(|| WorldError::ResourceBalanceInvalid {
            reason: format!(
                "resource balance overflow: kind={kind:?} current={current} delta={amount}"
            ),
        })
}
