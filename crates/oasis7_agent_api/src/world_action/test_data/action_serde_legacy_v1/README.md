# Legacy Action Serde fixtures

These pure-data fixtures were captured from `Action`'s derived `Serialize`
implementation at source commit `163fffef9102121b04f0016652d14b05a15c79ec`.
The provenance manifest records the exact old `action.rs` SHA-256 and each
fixture's byte length and SHA-256. The compatibility test rechecks those hashes.

The base set contains default-valued fixtures for all 90 Action variants. The
second set contains four optional-present cases for `TransferMainToken`,
`ClaimStarterOc`, `TransferMaterial`, and `GrantMetaProgress`. These values are
serialization fixtures only; they are not valid gameplay commands.

For each set the JSON, ordinary CBOR, packed CBOR, and public Serde callback
trace are retained. Tests deserialize the frozen JSON with the current
`Deserialize` implementation, then compare re-encoded bytes and the callback
trace with the frozen legacy outputs. No legacy serializer implementation or
fixture constructor is included in the crate.
