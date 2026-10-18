# Legacy Scheduler migration fixture

These 19 records were generated from the original Runtime `cognition_economy.rs`
and `cognition_wake.rs` declarations and pure methods at `cff23171d16b98a90bc4620be32aa515ab12409a`, saved before
migration. The temporary generator instantiated the existing generic wire schema
with those original concrete types, then signed with fixed private key `11` repeated
32 times. It captured exact signed JSON, scheduler canonical signing bytes, payload
digest, correlation, canonical result JSON and request-key digest before decoding
through the newly published aliases. No replacement DTO is used by the consumer.

Coverage: reserve/quote with expiry, settle, release, continuation proposal and its
optional defaults, budget, resume, terminal/replan handoff, every continuation status,
provider prefix/failure. Native task negative tests remain in client_api's wire tests.
The external `oasis7_game_client/tests/legacy_profile.rs` integration target checks
these records using only ordinary API/proto/Game Client dependencies, and compiles
actual transport Submit/Lookup with the published concrete profile.

The legacy result receipt remains opaque Runtime output for scheduler operations;
the pure client validates its signed operation/correlation and committed identity.
Current World permission, balance, admission and state-transition checks remain
server responsibilities. Fixed bytes are regression data, not an authorization grant.
