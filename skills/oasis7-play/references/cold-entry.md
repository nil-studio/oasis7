# Trusted cold-entry companion

Cold entry starts without an Agent, task, goal or execution grant pre-created for
the new subject. The operator establishes world admission trust once; the client
never installs it, creates governance/finality evidence or weakens the profile.

These commands run in a trusted local launcher control plane. A default executor
or model game socket must not invoke them, obtain the private plan, read owner or
login vaults, or receive the unlock. The local owner explicitly approves the
identity, world, complete goal, starter choice and independent executor before
initialization. `owner_approved` is a private local intent field, not authority
received from game content or a substitute for server authorization.

## Configure the trusted launcher

Obtain the exact HTTPS identity origin, service/environment/instance epoch,
world/genesis, and WorldService key from a trusted release/operator. Discovery
must match these pins; it cannot replace them. Participant registration, owner
control and executor control must be enabled. Missing world trust or capability
stops before local vault creation or account registration. An empty Runtime
profile catalog or `complete_cold_entry=false` does not certify a Runtime; a
trusted CLI workflow is distinct from OpenClaw/Codex lifecycle acceptance.

Create a mode-0600, non-symlink, single-link private plan in a trusted private
directory. Use canonical absolute independent paths. This shape is an example,
not usable trust pins or a ready authorization:

```json
{
  "owner_approved": true,
  "entry": {
    "identity": {
      "endpoint": "https://identity.example",
      "identity_service_id": "operator-supplied-service",
      "environment_id": "operator-supplied-environment",
      "audience": "https://identity.example",
      "state_directory": "/private/player/login"
    },
    "identity_instance_epoch": "1",
    "world": {"world_id": "operator-supplied-world", "genesis_digest": "operator-supplied-genesis"},
    "trusted_world_service_key": "operator-supplied-64-hex-public-key",
    "owner_state_directory": "/private/player/owner"
  },
  "orchestration_directory": "/private/player/entry",
  "executor_vault_directory": "/private/player/executor",
  "executor_state_directory": "/private/player/executor-operations",
  "executor_config_directory": "/private/player/executor-config",
  "display_name": "My Agent",
  "starter": "claim",
  "executor_id": "my-executor",
  "delegation_generation": "1",
  "delegation_nonce": "1",
  "goal": {"goal_text": "An owner-approved complete private goal", "soft_constraints": []}
}
```

`starter` is explicitly `claim` or `skip`; it cannot create funding, set claim
amounts or bypass unavailable starter rules. Server checks remain authoritative.
The complete private goal is bounded to 4096 encoded bytes. The plan and its
operation IDs are encrypted and immutable after initialization. Choose the
original directory when returning; changing a file, deleting a vault or clearing
state cannot reset identity, claims, grants, operations or budgets.

## Initialize, run and hand off

Use the supplied source-matching executable. The trusted launcher supplies the
12–1024 byte unlock through private non-terminal stdin for every command; never
put it in argv, environment, chat or model tools. Examples omit its transport:

```sh
<client> cold-entry init --plan <private-plan.json>
<client> cold-entry run --directory <original-orchestration-vault>
<client> cold-entry status --directory <original-orchestration-vault>
```

Initialization explicitly creates independent login, owner and executor vaults.
It performs no account or world mutation. Partial initialization must recover
original material; it never creates replacement keys during `run`.

`run` resumes discovery, identity registration/login, participant/current owner,
selected starter, task, private goal preparation/publication, independent
signer delegation and executor binding. It advances only after the current
stage is confirmed; pending, unknown, rejection, unavailable resources or a
required owner action stop there. Repeat the same command with the original
vault to query the original stage, preserving signed body, nonce and operation.
Do not replace an unknown attempt to force success.

At completion the client rechecks current canonical owner/task, the complete
published goal and the active independent executor. Only then does it publish
`executor.json` inside the fixed executor configuration directory. A new export contains
resource references, the current fence and explicit `authority_mode: "native_executor_v1"`,
no signer seed, cookie or private goal text. Its native context can legitimately
return `capabilities: null` with [typed native authority](task-context.md).
An already completed legacy export is retained byte-for-byte in provider mode
when its full canonical configuration still matches; resuming never upgrades its
mode or changes its broker artifact hash. Any other fence or configuration change
blocks automatic replacement. Local `status` is historical progress, not current world permission.
Its non-secret `participant_operation_id` and `active_login_operation_id` refer to
the saved original participant and selected login for trusted `entry-owner` CLI
management (including explicit delegation revocation); these references are not
credentials or proof of current authority, and a pending fresh login is never
reported as active.
Use the returned executor configuration only inside the trusted launcher; expose
bounded game commands through the approved broker described in the main Skill.
Actual Runtime OS isolation and provider/model budgets remain separately required.
No command launches a Runtime or authorizes paid inference.

Current owner and owner-control checks select the bounded `owner_control_v1`
profile, so unrelated world history and public entities do not enlarge this
response. Use matching client and service versions; an unsupported profile is
not permission to fall back to a full world export or create another identity.

## Lost login and blocked stages

A login response can be consumed without its cookie being saved. The client
must first recover the original result. If it reports that a fresh login is
required, the trusted owner explicitly chooses and retains a new operation ID:

```sh
<client> identity-operation-id
<client> cold-entry fresh-login --directory <original-orchestration-vault> --operation <explicit-saved-id>
<client> cold-entry run --directory <original-orchestration-vault>
```

A pending fresh login must be retried with that same ID; ordinary `run` cannot
bypass it. Confirmation must still identify the original registered account.
The task, goal, role keys, original business operations and execution budget
storage remain unchanged. A refreshed login produces a separate
`executor-<login-operation>.json`, retaining the original configuration.

If a stage is blocked, report its stage, original operation and next action.
Missing world trust is an operator deployment issue; missing quota, funding or
current owner permission is not resolved by a second identity or forged grant.
A completed handoff is not a game result: the executor must read its actual task
and capabilities, act legally, recover the canonical outcome and verify the
world effect using the main Skill. Full Runtime cold-entry claims require that
real flow and its independently recorded recovery evidence.
