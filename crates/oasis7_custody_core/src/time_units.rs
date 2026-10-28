//! Explicit time units. No implicit conversion between approval milliseconds
//! and the legacy lifecycle/RFC whole seconds.
use crate::DecimalU64;
use serde::{Deserialize, Serialize};
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct UnixSeconds(DecimalU64);
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct UnixMillis(DecimalU64);
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimeUnitError {
    Overflow,
    NoWholeSecondWindow,
}
impl UnixSeconds {
    pub const fn new(value: u64) -> Self {
        Self(DecimalU64(value))
    }
    pub const fn get(self) -> u64 {
        self.0.0
    }
    pub fn checked_millis(self) -> Result<UnixMillis, TimeUnitError> {
        self.get()
            .checked_mul(1000)
            .map(UnixMillis::new)
            .ok_or(TimeUnitError::Overflow)
    }
}
impl UnixMillis {
    pub const fn new(value: u64) -> Self {
        Self(DecimalU64(value))
    }
    pub const fn get(self) -> u64 {
        self.0.0
    }
    pub const fn floor_seconds(self) -> UnixSeconds {
        UnixSeconds::new(self.get() / 1000)
    }
}
/// Expiry rounds down, never extending a millisecond authority deadline.
/// Less than a complete second remaining cannot issue a new HTTP proof.
pub fn http_proof_window(
    now: UnixMillis,
    deadline: UnixMillis,
) -> Result<(UnixSeconds, UnixSeconds), TimeUnitError> {
    if deadline
        .get()
        .checked_sub(now.get())
        .is_none_or(|remaining| remaining < 1000)
    {
        return Err(TimeUnitError::NoWholeSecondWindow);
    }
    let created = now.floor_seconds();
    let expires = UnixSeconds::new(
        created
            .get()
            .checked_add(30)
            .ok_or(TimeUnitError::Overflow)?
            .min(deadline.floor_seconds().get()),
    );
    if expires.get() <= created.get()
        || expires
            .checked_millis()?
            .get()
            .checked_sub(now.get())
            .is_none_or(|remaining| remaining < 1000)
    {
        return Err(TimeUnitError::NoWholeSecondWindow);
    }
    Ok((created, expires))
}

impl core::fmt::Display for TimeUnitError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "invalid time-unit conversion: {self:?}")
    }
}
impl std::error::Error for TimeUnitError {}
