# Oasis7 model provider

This package contains the session-scoped BYOK consumer and an explicit legacy
simulator compatibility module. Both depend on the shared Harness contract and
HTTP/codec libraries, without a dependency on the `oasis7` server, Runtime, World,
or simulator.

The BYOK API is `connection`, `preset`, `protocol`, `transport`, `probe`, and
`harness_adapter::SessionModelProvider`. It uses trusted session/executor bindings,
configured provider presets, request permits and structured supplier tool history.
It has no environment credential fallback and never invokes the legacy transport.

`legacy` retains the existing Responses client and configuration API; root
reexports preserve its active simulator consumers. The sections below describe
that legacy compatibility path. Its configuration and retry behavior do not
configure or weaken the separate BYOK consumer.

## Native legacy completion bridge

The implementation of `LlmCompletionModelProvider<C>` lives in
`legacy_model_adapter` in this crate, next to the existing legacy
`LlmCompletionClient` and its transport errors. The previous umbrella path
`oasis7::simulator::LlmCompletionModelProvider` is a type-identical reexport.
The adapter forwards exactly one budget-limited call, keeps request digest,
ordered Decision/ModuleCall payloads, usage, and provider ID, and preserves the
prior retryability classification. It never constructs Runtime World state or
executes provider-suggested tools. Both direct-owner and legacy-host tests must
pass before retiring the compatibility export.

## Boundaries

`OpenAiChatCompletionClient` implements the Harness `ModelProvider` interface.
Each request supplies its model, complete prompts, caller tool schemas, request
digest, and inference budget. The response preserves the request digest, exact
caller tool names and JSON arguments, usage, and provider diagnostics. Undeclared
returned tools are rejected. A tool call is a proposal for the host to interpret;
it does not execute a tool, sign an action, validate a task fence, or grant world
authority.

The native task runner owns goal/context assembly and tool dispatch. The game
client owns authenticated service reads, canonical quotes, signed submission,
original-operation recovery, and durable client state. Runtime remains responsible
for admission, settlement, and authoritative feedback. This package does not
reconstruct a local World or turn an observation into permission to execute.

## Configuration and compatibility

`ModelProviderConfig` reads `config.toml` or the existing `OASIS7_LLM_*` environment
settings. Its shared resolver preserves the selected TOML profile/model-provider
precedence and normalized per-agent short/long goal overrides used by the legacy
configuration. The native host supplies the complete authorized active task goal;
configuration prompts do not replace that task authority. Configuration `Debug`
output omits credentials, endpoint, and prompt/goal content.

`CompletionClientConfig` allows the existing `LlmAgentConfig` to initialize the
same transport. Existing `LlmCompletionRequest` literals remain source-compatible.
`CompletionToolProfile::LegacySimulator` retains the fixed legacy tools and
legacy response mapping. `CompletionToolProfile::Caller` and the Harness adapter
use only the supplied tools and preserve their exact names, even when a name
matches a legacy alias. Selecting caller tools never silently enables the legacy
capabilities.

Transport timeout, Responses streaming, optional stream diagnostics, and the
legacy bounded concurrency retry share one implementation. A request with
`max_model_calls: Some(_)` disables the legacy outer retry, and a zero budget
fails before a network call. The Harness adapter also bounds the timeout by the
admitted latency budget and rejects responses exceeding the tool-call budget.
The host remains responsible for charging multiple physical calls across a turn
and persisting its overall run budget.

## Verification

Run from the repository root:

```sh
./scripts/cargo-dev.sh test -p oasis7_model_provider --all-targets
./scripts/cargo-dev.sh clippy -p oasis7_model_provider --all-targets -- -D warnings
```

The integration tests use a local fake HTTP Responses service to verify dynamic
tool schemas, exact typed results, request correlation, usage, legacy tool
compatibility, and budgeted versus unbudgeted retry behavior. Configuration tests
cover document loading, selected provider precedence, per-agent goals, and
invalid budgets. They make no paid provider calls.

The `workspace_support` CI profile runs both commands with all targets so the
`tests/config.rs` and `tests/transport.rs` integration tests are included in full
and targeted plans.
