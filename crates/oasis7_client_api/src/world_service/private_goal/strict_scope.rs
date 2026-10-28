//! Opaque crypto-only scope. No authority, Current, validator result or time cache.
use super::PreparedOwnerSignature;
#[derive(Default, Clone, Copy, serde::Serialize)]
pub struct StrictOwnerSignatureSummary {
    pub calls: u64,
    pub successes: u64,
    pub failures: u64,
    pub repeated_successes: u64,
    pub tracked_tuples: u64,
    pub overflow_tuples: u64,
    pub direct_calls: u64,
    pub nested_calls: u64,
    pub nested_repeats: u64,
    pub strict_attempts: u64,
    pub reuse_hits: u64,
}
#[derive(Default)]
pub struct StrictOwnerSignatureScope {
    entries: Vec<PreparedOwnerSignature>,
    bytes: usize,
    summary: StrictOwnerSignatureSummary,
}
#[derive(Clone, Copy)]
enum Mode {
    DirectReuse,
    NestedReuse,
}
impl StrictOwnerSignatureScope {
    pub const MAX_ENTRIES: usize = 256;
    pub const MAX_BYTES: usize = 2 * 1024 * 1024;
    pub fn summary(&self) -> StrictOwnerSignatureSummary {
        self.summary
    }
    /// The existing Core direct boundary may reuse a prior actual exact success.
    /// There is no public insertion or caller-chosen verification backend.
    pub fn verify_prepared_reusing(
        &mut self,
        prepared: PreparedOwnerSignature,
    ) -> Result<(), String> {
        self.verify(prepared, Mode::DirectReuse)
    }
    // Only the fixed nested SDK goal chain uses this entrypoint; exact crypto
    // successes share the same bounded scope as the existing Core direct path.
    pub(super) fn verify_prepared_nested(
        &mut self,
        prepared: PreparedOwnerSignature,
    ) -> Result<(), String> {
        self.verify(prepared, Mode::NestedReuse)
    }
    fn verify(&mut self, prepared: PreparedOwnerSignature, mode: Mode) -> Result<(), String> {
        self.summary.calls += 1;
        let repeated = self.entries.iter().any(|old| exact(old, &prepared));
        match mode {
            Mode::DirectReuse => self.summary.direct_calls += 1,
            Mode::NestedReuse => self.summary.nested_calls += 1,
        }
        if repeated {
            self.summary.repeated_successes += 1;
            self.summary.successes += 1;
            self.summary.reuse_hits += 1;
            if matches!(mode, Mode::NestedReuse) {
                self.summary.nested_repeats += 1;
            }
            return Ok(());
        }
        self.summary.strict_attempts += 1;
        if let Err(error) = prepared.verify_strict() {
            self.summary.failures += 1;
            return Err(error);
        }
        self.summary.successes += 1;
        let bytes = prepared.canonical_bytes().len()
            + prepared.domain().len()
            + prepared.public_key_hex().len()
            + prepared.signature_hex().len()
            + prepared.profile().len();
        if self.entries.len() >= Self::MAX_ENTRIES
            || bytes > Self::MAX_BYTES.saturating_sub(self.bytes)
        {
            self.summary.overflow_tuples += 1;
            return Ok(());
        }
        self.bytes += bytes;
        self.entries.push(prepared);
        self.summary.tracked_tuples += 1;
        Ok(())
    }
}
fn exact(a: &PreparedOwnerSignature, b: &PreparedOwnerSignature) -> bool {
    a.profile() == b.profile()
        && a.domain() == b.domain()
        && a.public_key_hex() == b.public_key_hex()
        && a.signature_hex() == b.signature_hex()
        && a.canonical_bytes() == b.canonical_bytes()
}
#[cfg(test)]
#[path = "strict_scope_tests.rs"]
mod tests;
