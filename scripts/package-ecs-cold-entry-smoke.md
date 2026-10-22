# Actual-service cold entry integration smoke

`package-ecs-cold-entry-smoke.py` is a Linux-only, bounded integration driver. It runs
actual fixture setup, Chain Runtime, Launcher IdentityService and Game Client
binaries. It does not implement a world handler, issue signed replies, precreate
participants or start a model Runtime. Passing this test is not OpenClaw/Codex
Runtime playtest evidence.

For the concrete fresh DevLocal fixture, build the example from the same source
revision as the ECS bundle and run on Linux:

```sh
env -u RUSTC_WRAPPER cargo build -p oasis7_game_client --example cold_entry_probe
python3 scripts/package-ecs-cold-entry-smoke.py \
  --bundle /absolute/verified/ecs-release \
  --probe-binary /absolute/target/debug/examples/cold_entry_probe \
  --evidence-directory /absolute/private/new-evidence \
  --timeout-seconds 600
```

This built-in fixture provisions a fresh node identity, initializes a genuine
preview world, starts actual services to create the identity SQLite database,
stops all writers, exports the consistent identity snapshot, and invokes the
DevLocal `prepare`/`finalize --prepared` commands. It then starts the actual
services with that authorization. The authorized second startup removes the
bootstrap replication listener: DevLocal authority requires the isolated
single-node path and rejects even an explicit loopback replication listener.
The real initializer seeds one unrelated Agent. Its offline prepare output must
contain exactly integer counts `agents: 1`, `tasks: 0`, `goals: 0`, `grants: 0`.
Fresh discovery must match the world, service and identity pins and explicitly
enable all four entry phases. The new cold-entry subject's orchestration,
identity, owner and executor directories must still be absent before any client
vault is initialized. Failed readiness attempts cannot retain a success state.
Discovery readiness is bounded by 60 attempts at one-second intervals and the existing overall
deadline; first policy installation may need a real world slot to commit.
The probe verifies the new participant resource through SDK reads and commits one move to `(1,0,0)` with
one durable original operation; reruns query the same action. Counts in the
canonical probe are scoped to the newly registered participant, not a global
population audit.

The fixture also enables the formal loopback private goal/context service with
`OASIS7_PRIVATE_GOAL_DIR` and `OASIS7_PRIVATE_GOAL_KEY_FILE`. Its independent
0700 namespace is outside the preview world's public world/records/CAS roots;
the key is a fresh, exclusive 0600 file containing exactly 32 random bytes.
This configures encrypted storage only: no goal, task, provider or signed response
is created in advance. Without both settings those private routes stay disabled,
and a public gateway's unknown response cannot count as goal preparation.

Alternatively, run using a private fixture file assembled for the exact built revision:

```sh
python3 scripts/package-ecs-cold-entry-smoke.py \
  --fixture /absolute/private/fixture.json \
  --evidence-directory /absolute/private/new-evidence \
  --timeout-seconds 300
```

The evidence directory must not exist. Its logs can contain private service
responses, so preserve its 0700 permissions. An ephemeral random owner unlock is
sent only on the Game Client child stdin; it is never placed in argv, environment,
logs or a report. The script retains the original vaults until operator cleanup.
Do not use this fixture against an existing world or production identity store.

The fixture is a 0600 JSON object with these fields:

| Field | Meaning |
| --- | --- |
| `client_binary` | Absolute actual `oasis7_game_client` binary path |
| `identity_upstream` | `["127.0.0.1", port]` for the actual Launcher HTTP server |
| `setup_commands` | Ordered argv arrays using the real offline initialization commands |
| `server_commands` | Actual long-running Chain and Launcher argv arrays |
| `environment` | Optional isolated fixture environment for the child processes |
| `private_files` | Optional relative filename to JSON content map for setup config files |
| `plan_template` | Actual owner-approved `cold-entry init` plan |
| `initial_probe` | Actual read-only SDK probe command proving the zero-resource baseline |
| `canonical_probe` | Actual SDK integration command verifying task/context and one committed action |
| `binding_response_path` | `/api/public/hosted-account/v2/entry-owner/control`; body must be an actual executor bind submit |

String values expand `${root}` to the fresh evidence root and `${origin}` to the
local HTTPS origin. Paths in the cold-entry plan must be absolute independent
subdirectories under `${root}`. Setup must use the formal DevLocal prepare,
identity snapshot export and finalize commands, with fixture-only signing keys;
it must never edit snapshots to create Agent/task/grant state. Configure the
Launcher exact allowed origin and RP host `localhost`, trusted loopback proxy,
actual world trust and enabled entry phases. The caller supplies port choices;
occupied ports must cause service startup to fail rather than attach to another
world. Always use fresh storage paths.

The initial probe must emit exactly one JSON object with `agents`, `tasks`,
`goals`, and `grants` all zero. It should also verify identity/world pins and
service readiness. The canonical probe receives the same private unlock on stdin and must emit `canonical_verified: true`,
`agents: 1`, `tasks: 1`, `goals: 1`, `active_executors: 1`,
`sdk_task_context_verified: true`, `action_committed: true` and the actual
`participant_operation_id`, `executor_operation_id`, `task_id`, and `agent_id`.
These commands must query and validate actual canonical services. They may not
print constants or issue test-authority signatures. The final probe runs twice:
its action must use one durable original operation, reconcile its committed
receipt and avoid creating a second action on retry.
The SDK's bounded submit polling can return an uncommitted outcome at a slot
boundary. The probe may continue read-only lookup of that same signed original
within its own shared wall-clock budget; this neither extends the context's
logical lease nor permits a replacement operation. Only a verified committed
result advances the probe. Terminal rejection, expiry or unavailable history
must fail with fixed outcome categories rather than an ambiguous success claim.

The forwarding server terminates actual temporary CA TLS and forwards each
request unchanged to the actual HTTP service. It discards one successful actual
machine-registration completion response and one successful actual binding
response. It then restarts the Game Client command with the same vault and
operation identities. The flow must reach complete, prove both losses occurred,
and retain the canonical identities after another completed-workflow restart.
No synthetic success response is produced.

Both initial entry and the completed-workflow restart require an explicit
`complete` stage with `confirmed` state; a zero exit code with an unknown state
does not pass. Both use the same original command and recovery rules below.
Recovery is bounded by the existing overall deadline, rather than a fixed number
of rapid CLI restarts. Pending and unknown results retain the original vaults and
operations and use bounded backoff. The trusted forwarding server observes actual
HTTP 429 responses and their delta-seconds `Retry-After`; the next attempt must
wait at least that long. Missing or invalid values use the identity service's
60-second cooldown. A cooldown beyond the remaining deadline fails without a
new request. Non-429 headers cannot control this wait. Explicit blocked or
action-required results fail for operator inspection; the driver never supplies
fresh-login consent. An HTTP 202 is an unknown result, not evidence of a prepared
goal or committed operation. Service limits and world slot timing remain unchanged.

TLS verification remains enabled. The temporary CA is passed only through the
child `SSL_CERT_FILE` environment, supported by the Linux native-certificate
loader used by reqwest. macOS and Windows are rejected because their platform
trust loaders cannot be assumed to honor this isolated trust configuration.
System certificate stores are never changed. `openssl` and Python 3 are needed.

The tool has an overall deadline (30–900 seconds), a 60-second per-command bound,
request/response size bounds and bounded termination of tracked process groups.
Cleanup skips already reaped leader PIDs because their numeric IDs may have
been reused. CI job containment or an operator-owned systemd cgroup must contain
unexpected orphan descendants; verify that containment separately. The receipt
does not certify complete descendant cleanup. A
`receipt.json` records success only after all actual verification commands succeed.
On failure it contains only fixed diagnostic enums, exit code, attempt count and
the two response-loss flags; arbitrary CLI text and private response material are
excluded. Failure receipt write errors must not prevent process cleanup.
Fixture source revision, actual binary digests and canonical proof details belong
in the probe evidence, not an unverified claim in this driver. CI selection is
owned by the main task; the script does not alter workflow or test selection.

The ECS packaging workflow compiles this example separately with the `ecs`
profile from the same checkout as the four delivery binaries. It transfers the
probe as a separate staging artifact, verifies the assembled bundle on Linux,
and only publishes the production package after smoke success. The probe binary
is not included in the production bundle. Only `receipt.json` is uploaded as
public verification evidence; private fixture stores, keys, goals and process
logs remain in the ephemeral runner workspace.
The sanitized receipt is also uploaded after a failed smoke; package publication
continues to require smoke success.
