use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::RuntimeState;

pub(crate) fn lock_state<'a>(
    state: &'a Arc<Mutex<RuntimeState>>,
) -> std::sync::MutexGuard<'a, RuntimeState> {
    state
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

pub(crate) fn now_unix_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(duration_millis_to_i64_saturating)
        .unwrap_or(0)
}

pub(crate) fn with_execution_hook<T>(
    execution_hook: &mut Option<&mut dyn crate::NodeExecutionHook>,
    f: impl FnOnce(Option<&mut dyn crate::NodeExecutionHook>) -> T,
) -> T {
    match execution_hook {
        Some(hook) => f(Some(&mut **hook)),
        None => f(None),
    }
}

pub(crate) fn millis_until_next_logical_tick(
    now_ms: i64,
    genesis_unix_ms: i64,
    slot_duration_ms: u64,
    ticks_per_slot: u64,
) -> Option<u64> {
    if slot_duration_ms == 0 || ticks_per_slot == 0 {
        return None;
    }
    if now_ms < genesis_unix_ms {
        return Some((genesis_unix_ms - now_ms) as u64);
    }

    let elapsed_ms = (now_ms - genesis_unix_ms) as u128;
    let ticks_per_slot_u128 = ticks_per_slot as u128;
    let slot_duration_ms_u128 = slot_duration_ms as u128;
    let current_tick = elapsed_ms
        .checked_mul(ticks_per_slot_u128)?
        .checked_div(slot_duration_ms_u128)?;
    let next_tick = current_tick.checked_add(1)?;
    let next_tick_numerator = next_tick.checked_mul(slot_duration_ms_u128)?;
    let next_elapsed_ms = next_tick_numerator
        .checked_add(ticks_per_slot_u128.saturating_sub(1))?
        .checked_div(ticks_per_slot_u128)?;
    if next_elapsed_ms > i64::MAX as u128 {
        return None;
    }
    let next_tick_at_ms = genesis_unix_ms.checked_add(next_elapsed_ms as i64)?;
    if next_tick_at_ms <= now_ms {
        return Some(1);
    }
    Some((next_tick_at_ms - now_ms) as u64)
}

fn duration_millis_to_i64_saturating(duration: Duration) -> i64 {
    i64::try_from(duration.as_millis()).unwrap_or(i64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duration_millis_to_i64_saturating_clamps_on_overflow() {
        assert_eq!(
            duration_millis_to_i64_saturating(Duration::from_secs(u64::MAX)),
            i64::MAX
        );
    }

    #[test]
    fn millis_until_next_logical_tick_returns_slot_phase_boundary_delta() {
        let wait = millis_until_next_logical_tick(10_000, 10_000, 12_000, 10)
            .expect("wait must be computable");
        assert_eq!(wait, 1_200);

        let wait = millis_until_next_logical_tick(10_150, 10_000, 12_000, 10)
            .expect("wait must be computable");
        assert_eq!(wait, 1_050);
    }

    #[test]
    fn millis_until_next_logical_tick_handles_time_before_genesis() {
        let wait = millis_until_next_logical_tick(900, 1_000, 12_000, 10)
            .expect("wait must be computable");
        assert_eq!(wait, 100);
    }

    #[test]
    fn millis_until_next_logical_tick_rejects_invalid_parameters() {
        assert!(millis_until_next_logical_tick(0, 0, 0, 10).is_none());
        assert!(millis_until_next_logical_tick(0, 0, 12_000, 0).is_none());
    }
}
