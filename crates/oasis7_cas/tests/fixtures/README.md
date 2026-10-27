# Frozen pre-upgrade Zstandard frames

The stream frame was generated once with `zstd 0.13.3`,
`zstd-safe 7.2.4`, and `zstd-sys 2.0.16+zstd.1.5.7`, with the Rust
`zstd` default features disabled. The generator linked the existing local
`libzstd-f17426188dfeaca7.rlib`; its dep-info identifies those registry
versions and its Cargo fingerprint records `features: []`.
No old dependency is run or downloaded by the tests.

Exact generation semantics (Rust):

```rust
let raw: Vec<u8> = (0..65536)
    .map(|i| ((i % 251) ^ ((i / 4096) % 17)) as u8).collect();
let stream = zstd::stream::encode_all(raw.as_slice(), 3).unwrap();
let bulk = zstd::bulk::compress(&raw, 3).unwrap();
```

`stream` is this directory's `zstd-0.13.3-level3-stream.zst` (2283 bytes).
`bulk` is in `oasis7_distfs/tests/fixtures` (2284 bytes), alongside a frozen
format-v1 recipe: one span `(raw_digest_bytes, 0, 65536)`, one compressed
frame with the stored/raw BLAKE3 below, version/chunker 1 and length 65536.
The recipe uses the pre-existing ordered serde-CBOR schema and a BLAKE3
checksum over its CBOR body. It is frozen independently of the reader.
Tests authenticate these historical bytes and logical IDs; they do not
require a newer encoder to reproduce identical compressed bytes.

BLAKE3:

- raw: `0f227458a96cb2b18610d9624d5750390613bf9e84107e46ff48902b5b31be88`
- stream: `6d1849629b97438a53509b0b532d86b3c08d53438fccd1704a2d55847806781e`
- bulk: `7436b6db87568eb99f55de4c9aa7eb79af33f6825126299f096a992850dbe65c`

SHA-256:

- stream: `6f0dd3c02309ef53349113572caa0c9e2a1cf33f61f441b60ae068fd2405e352`
- bulk: `6e24814f79cdda6fd6cbaca93b921207c27069fd8d8a944e8e0d5c35db0347d4`
- recipe (346 bytes): `25c17a4f9f1697185eee7e4fb4ddbccc359712b02a15721a2ab8f3dd787d004b`
