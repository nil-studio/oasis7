# Signed-read protocol consumer

This independent development-time consumer imports generated TypeScript types,
compiles a typed use of discovery/projection/error and operation response types,
and validates the real handler fixtures with locked Ajv (JSON Schema 2020-12).
The fixture authority is the Rust handler test in `oasis7_viewer_runtime`; this
consumer does not generate fixtures from the schema. Its independent negative
values test canonical and exact u64 bounds, known nested task fields, positive
integer narrowing, nullable/omitted fixed commits, and request/response unknown
field policies. Opaque proof bytes remain outside this consumer; the Rust parser,
signing and handler tests cover duplicate keys, authentication and rejections.

From this directory, run `npm ci --ignore-scripts`, then `npm test`. Set `PYTHON`
to the profile's locked Python interpreter when `python3` is a different version.
The test first runs the generator's read-only full-set integrity/drift check, then
compiles and executes the consumer. No generator or npm tooling enters the service
runtime. There is no previous TS release: this is the initial consumer baseline,
with existing Rust wire golden tests remaining independent compatibility evidence.

Raw World Service request/proof fields retain numeric u64 wire schemas. JavaScript
`number` and `JSON.parse` cannot represent their complete domain exactly (including
numeric schema maxima); this sample makes no lossless numeric codec/validation
claim and does not sign requests. Exact full-domain checks here concern DecimalU64
strings. Projection `data` stays unknown JSON; the consumer checks fixture response
byte limits, while the handler owns recursive conversion and runtime bounds.
