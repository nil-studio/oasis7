//! Pure TestSession timing primitives. Time values are injected Unix seconds.
use crate::DecimalU64;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionWindow {
    pub approved_at: DecimalU64,
    pub write_deadline: DecimalU64,
    pub read_until: DecimalU64,
    pub cleanup_until: DecimalU64,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionError {
    InvalidWindow,
    Overflow,
    Closed,
    TooSoon,
    Exhausted,
    InvalidId,
}

impl SessionWindow {
    pub fn validate(&self) -> Result<(), SessionError> {
        let approval_max = self
            .approved_at
            .0
            .checked_add(1800)
            .ok_or(SessionError::Overflow)?;
        let read_max = self
            .write_deadline
            .0
            .checked_add(1800)
            .ok_or(SessionError::Overflow)?;
        let cleanup_max = self
            .write_deadline
            .0
            .checked_add(300)
            .ok_or(SessionError::Overflow)?;
        if self.write_deadline.0 <= self.approved_at.0
            || self.write_deadline.0 > approval_max
            || self.read_until.0 < self.write_deadline.0
            || self.read_until.0 > read_max
            || self.cleanup_until.0 < self.write_deadline.0
            || self.cleanup_until.0 > cleanup_max
        {
            return Err(SessionError::InvalidWindow);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "RawSessionLifecycle")]
pub struct SessionLifecycle {
    window: SessionWindow,
    lease_until: DecimalU64,
    last_heartbeat_at: Option<DecimalU64>,
    heartbeats: BTreeMap<String, DecimalU64>,
    closing: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawSessionLifecycle {
    window: SessionWindow,
    lease_until: DecimalU64,
    last_heartbeat_at: Option<DecimalU64>,
    heartbeats: BTreeMap<String, DecimalU64>,
    closing: bool,
}

impl TryFrom<RawSessionLifecycle> for SessionLifecycle {
    type Error = &'static str;
    fn try_from(raw: RawSessionLifecycle) -> Result<Self, Self::Error> {
        raw.window
            .validate()
            .map_err(|_| "invalid persisted session window")?;
        let start = raw.window.approved_at.0;
        let deadline = raw.window.write_deadline.0;
        let initial = start
            .checked_add(90)
            .ok_or("session time overflow")?
            .min(deadline);
        let expected = match raw.last_heartbeat_at {
            None if raw.heartbeats.is_empty() => initial,
            Some(last)
                if !raw.heartbeats.is_empty() && last.0 >= start + 30 && last.0 < deadline =>
            {
                last.0
                    .checked_add(90)
                    .ok_or("session time overflow")?
                    .min(deadline)
            }
            _ => return Err("inconsistent heartbeat history"),
        };
        let earliest_heartbeat_lease = start
            .checked_add(120)
            .ok_or("session time overflow")?
            .min(deadline);
        if raw.lease_until.0 != expected
            || raw.heartbeats.len() > 64
            || raw.heartbeats.iter().any(|(id, lease)| {
                id.is_empty() || lease.0 < earliest_heartbeat_lease || lease.0 > expected
            })
            || (!raw.heartbeats.is_empty() && !raw.heartbeats.values().any(|v| v.0 == expected))
        {
            return Err("invalid persisted session lease");
        }
        Ok(Self {
            window: raw.window,
            lease_until: raw.lease_until,
            last_heartbeat_at: raw.last_heartbeat_at,
            heartbeats: raw.heartbeats,
            closing: raw.closing,
        })
    }
}

impl SessionLifecycle {
    pub fn new(window: SessionWindow) -> Result<Self, SessionError> {
        window.validate()?;
        let lease = window
            .approved_at
            .0
            .checked_add(90)
            .ok_or(SessionError::Overflow)?
            .min(window.write_deadline.0);
        Ok(Self {
            window,
            lease_until: DecimalU64(lease),
            last_heartbeat_at: None,
            heartbeats: BTreeMap::new(),
            closing: false,
        })
    }
    pub fn is_closing(&self) -> bool {
        self.closing
    }
    pub fn lease_until(&self) -> u64 {
        self.lease_until.0
    }
    pub fn close(&mut self) {
        self.closing = true;
    }
    /// Every write admission and claim must call this, even if alarms are delayed.
    pub fn check_write(&mut self, now: u64) -> Result<(), SessionError> {
        if now < self.window.approved_at.0 {
            return Err(SessionError::InvalidWindow);
        }
        if now >= self.lease_until.0 || now >= self.window.write_deadline.0 {
            self.close();
        }
        if self.closing {
            Err(SessionError::Closed)
        } else {
            Ok(())
        }
    }
    pub fn heartbeat(&mut self, id: &str, now: u64) -> Result<u64, SessionError> {
        if id.is_empty() {
            return Err(SessionError::InvalidId);
        }
        // A retry returns its original acknowledgement, including after closing.
        // It never changes the current lease or reopens the session.
        if let Some(original) = self.heartbeats.get(id) {
            return Ok(original.0);
        }
        self.check_write(now)?;
        if self.heartbeats.len() >= 64 {
            self.close();
            return Err(SessionError::Exhausted);
        }
        let previous = self.last_heartbeat_at.unwrap_or(self.window.approved_at).0;
        if now < previous.checked_add(30).ok_or(SessionError::Overflow)? {
            return Err(SessionError::TooSoon);
        }
        let lease = now
            .checked_add(90)
            .ok_or(SessionError::Overflow)?
            .min(self.window.write_deadline.0);
        self.lease_until = DecimalU64(lease);
        self.last_heartbeat_at = Some(DecimalU64(now));
        self.heartbeats.insert(id.to_owned(), DecimalU64(lease));
        Ok(lease)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn window() -> SessionWindow {
        SessionWindow {
            approved_at: DecimalU64(100),
            write_deadline: DecimalU64(1900),
            read_until: DecimalU64(3700),
            cleanup_until: DecimalU64(2200),
        }
    }
    #[test]
    fn deadlines_are_checked_and_bounded() {
        window().validate().unwrap();
        let mut invalid = window();
        invalid.write_deadline = DecimalU64(1901);
        assert_eq!(invalid.validate(), Err(SessionError::InvalidWindow));
        invalid = window();
        invalid.read_until = DecimalU64(3701);
        assert_eq!(invalid.validate(), Err(SessionError::InvalidWindow));
        invalid = window();
        invalid.cleanup_until = DecimalU64(2201);
        assert_eq!(invalid.validate(), Err(SessionError::InvalidWindow));
        invalid.approved_at = DecimalU64(u64::MAX);
        assert_eq!(invalid.validate(), Err(SessionError::Overflow));
    }
    #[test]
    fn heartbeat_retry_never_extends_and_expiry_never_reopens() {
        let mut session = SessionLifecycle::new(window()).unwrap();
        assert_eq!(session.heartbeat("a", 129), Err(SessionError::TooSoon));
        assert_eq!(session.heartbeat("a", 130), Ok(220));
        assert_eq!(session.heartbeat("a", 160), Ok(220));
        assert_eq!(session.lease_until(), 220);
        assert_eq!(session.heartbeat("b", 220), Err(SessionError::Closed));
        assert!(session.is_closing());
        assert_eq!(session.heartbeat("a", 230), Ok(220));
        assert_eq!(session.lease_until(), 220);
        assert_eq!(session.check_write(150), Err(SessionError::Closed));
    }
    #[test]
    fn lease_is_clamped_to_absolute_deadline() {
        let mut session = SessionLifecycle::new(window()).unwrap();
        for index in 1..60 {
            session
                .heartbeat(&index.to_string(), 100 + index * 30)
                .unwrap();
        }
        assert_eq!(session.lease_until(), 1900);
        assert_eq!(session.check_write(1900), Err(SessionError::Closed));
    }
}
