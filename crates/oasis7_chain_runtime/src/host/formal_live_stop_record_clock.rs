//! New Stop route clock only. Never converts an uncertain UTC sample to zero.
use std::time::{Instant, SystemTime, UNIX_EPOCH};
#[derive(Default)]
pub(super) struct StopRecordClock {
    last: Option<(u64, Instant)>,
    failed: bool,
}
impl StopRecordClock {
    pub(super) fn sample(&mut self) -> Result<u64, String> {
        let before = Instant::now();
        let wall = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| ())
            .and_then(|duration| u64::try_from(duration.as_millis()).map_err(|_| ()));
        let after = Instant::now();
        self.observe_sample(wall, before, after)
    }
    fn observe_sample(
        &mut self,
        wall: Result<u64, ()>,
        before: Instant,
        after: Instant,
    ) -> Result<u64, String> {
        let result = (|| {
            if self.failed || after.checked_duration_since(before).is_none() {
                return Err(());
            }
            let now = wall?;
            if now == 0 {
                return Err(());
            }
            if let Some((previous, previous_after)) = self.last {
                // Bracket the UTC syscall. Using the prior AFTER and current
                // BEFORE avoids counting sampling latency as elapsed UTC.
                let elapsed = before.checked_duration_since(previous_after).ok_or(())?;
                let elapsed = u64::try_from(elapsed.as_millis()).map_err(|_| ())?;
                let floor = previous.checked_add(elapsed).ok_or(())?;
                if now < floor {
                    return Err(());
                }
            }
            self.last = Some((now, after));
            Ok(now)
        })();
        if result.is_err() {
            self.failed = true;
        }
        result.map_err(|()| "Stop observation UTC is uncertain or regressed".to_owned())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    #[test]
    fn stop_record_clock_rejects_zero_error_rollback_and_frozen_utc_without_rearming() {
        let start = Instant::now();
        for bad in [Ok(0), Err(())] {
            let mut clock = StopRecordClock::default();
            assert!(clock.observe_sample(bad, start, start).is_err());
            assert!(clock.observe_sample(Ok(2000), start, start).is_err());
        }
        let mut clock = StopRecordClock::default();
        assert_eq!(clock.observe_sample(Ok(2000), start, start).unwrap(), 2000);
        let later = start + Duration::from_millis(2);
        assert_eq!(clock.observe_sample(Ok(2002), later, later).unwrap(), 2002);
        assert!(clock.observe_sample(Ok(2001), later, later).is_err());
        assert!(clock.observe_sample(Ok(3000), later, later).is_err());
        let mut frozen = StopRecordClock::default();
        frozen.observe_sample(Ok(2000), start, start).unwrap();
        assert!(frozen.observe_sample(Ok(2000), later, later).is_err());
        assert!(frozen.observe_sample(Ok(3000), later, later).is_err());
    }
}
