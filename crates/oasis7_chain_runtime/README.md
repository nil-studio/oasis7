# Chain runtime host

The library owns the native chain host: execution bridge, formal runtime,
status and submission services, lifecycle operations, and reward worker.
`src/bin/oasis7_chain_runtime.rs` only initializes tracing, collects arguments,
calls `run_cli`, and maps its result to the process exit status.

`execution_config` is available on native targets without default features.
The host API requires a native target and `node-libp2p`, matching the binary's
feature boundary. `wasmtime` keeps its existing execution-engine meaning.

Library callers can parse `RuntimeConfig::parse(args)`, inspect the resolved
`RuntimeConfig::paths()`, then call `run(config)`. Parsing and running return the
existing string errors. `run_cli(args)` also dispatches identity/world commands
and help, preserving CLI diagnostics while returning errors to its caller.
Callers own tracing initialization and process termination. Running the host
still installs its existing signal handler and owns its runtime workers; this
API does not promise concurrent independent hosts in one process.

`src/host.rs` declares the implementation modules in `src/host/`. It is included
at the library root to preserve internal crate paths and exact test identities,
including child-process test dispatch. Host tests now belong to the library:

```sh
env -u RUSTC_WRAPPER cargo test -p oasis7_chain_runtime --lib --features test_tier_required
```

Moving implementation to a library lets entry-point-only edits reuse its build
artifact. It does not establish a cold-build speedup. Check suspected repeated
serialization code generation with a fixed toolchain, feature set and profile,
for example (with `cargo-llvm-lines` installed):

```sh
./scripts/cargo-dev.sh llvm-lines -p oasis7_world_service --lib --no-default-features --profile dev --filter 'Snapshot|Journal|WorldState|WorldServiceProjection'
```

LLVM line counts locate generated-code candidates; they are not compilation
time measurements. Compare the same format and concrete types across producer
crates before centralizing codecs. Such changes must preserve encoded bytes,
hashes and error semantics.
