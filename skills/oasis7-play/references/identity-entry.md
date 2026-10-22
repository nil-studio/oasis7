# Trusted identity and owner control

For a new authorized subject, the [trusted cold-entry companion](cold-entry.md)
combines these same stages and preserves original operations. Existing explicit
commands below remain available for deliberate owner operations and recovery.

These commands belong to a trusted local launcher, separate from the model's
game tools. Cold-entry OpenClaw/Codex profiles require their own approved
runtime manifest, isolated environment and real acceptance; neither a valid
profile nor an account receipt proves complete cold entry.

Official and external clients use the same identity, participant, owner and
executor protocol. Establishing the world's admission trust is a deployment
operator responsibility: DevLocal may use its explicitly configured local
operator authority, while governed networks require their governance evidence.
Clients must not install either trust root, create governance history to log in,
select a weaker authority mode, or treat a local authorization as network finality.
Missing server trust remains a deployment blocker; it never authorizes bypassing
identity, owner signatures, quotas, delegation or current task fences.

The executable accepts `--identity-config <identity.json>` for `identity-discovery` (read-only, before unlock or vault creation), then explicit
`identity-init`, `identity-register --operation <id> --name <name>`,
`identity-login --operation <different-id>` and `identity-status --operation <id>`.
Choose stable IDs with `identity-operation-id` and retain them before sending.
The config pins HTTPS origin, service/environment and audience. Unix private
storage uses Argon2id/XChaCha; provide unlock only through trusted private stdin.
Never expose passwords, cookies, seeds, vaults or original requests to chat,
model tools, argv or environment. Missing unlock or permissions stops the stage.

`--entry-config <entry.json>` pins the identity client, instance epoch, world,
world-service public key and a separate owner state directory. Initialize it
with `entry-owner-init`. `entry-register --operation <entry-id>
--login-operation <login-id>` queries the original canonical registration before
submission. `entry-owner-view` with the same options then verifies current
canonical ownership. Account registration, world participant registration and
current owner observation are separate stages; a receipt does not grant the
next stage. Registration closure permits existing identities to log in and
recover original receipts. An authorized epoch transition keeps the stable
account/world binding and old originals; it never creates another Agent.

Explicit owner operations use `--participant-operation <entry-id>
--login-operation <login-id> --operation <stable-stage-id>`:

- `entry-task-create` creates or recovers the persistent task.
- `entry-starter-eligibility` reads current eligibility (omit `--operation`).
  `entry-starter-claim` uses actual starter rules and original claim; it cannot
  mint funds or bypass an unavailable treasury.
- `entry-goal-prepare --goal-file <private-json>` prepares complete private goal
  content; `entry-goal-publish --prepare-operation <prepare-id>` publishes its
  commitment. Neither the goal file nor its digest belongs in model output.
- `entry-executor-init --executor-directory <separate-private-dir>` (only this
  directory option) creates an independent executor key.
- `entry-executor-authorize --executor-public-key <key> --generation <n>
  --nonce <n>` authorizes the delegate; `entry-executor-bind` adds
  `--executor-public-key`, `--generation`, `--executor-id` to bind it to the
  task. The owner key cannot serve as delegate.
- `entry-executor-stop` stops the current task executor;
  `entry-executor-revoke --executor-public-key <key> --generation <n>
  --nonce <n>` revokes the selected signer delegation.

After stop/revoke, `entry-executor-lookup --original-file <private-original.json>`
uses the participant/login/operation options above to query an explicitly
provided original signed executor payload. It persists the original before
querying, permits a Suspended task, and never resubmits it or reopens old private
context. A newly authorized executor may likewise reconcile an explicitly
provided old signed original with current read authority. This does not share
private vaults or authorize the old key again.

These are explicit owner policy decisions. Ordinary game tools cannot call
these commands, change a goal or authorize themselves. `entry-status` and
`entry-owner-status` report historical progress, not current permission. After
an unknown reply, repeat the original stage and ID; never replace body, key,
nonce or operation to force progress. A consumed login whose session cookie
was lost requires an explicit fresh login; owner/executor permission must be
checked again.

A trusted launcher can run `broker-serve --profile <private-profile.json>`
with private unlock stdin. The model receives only the approved private Unix
socket and this Skill, using `broker-call --socket <socket>` with a bounded
JSON request on stdin: `request_id`, `resource_id`, `args` (native game command
and its exact options). Each resource pins an executor config and independent
vault, operation state and durable tool-request budget. Executor vaults, task operation state and budgets stay separate. The trusted
daemon may reuse the exact same login identity directory for A and B; account
revocation then affects both sessions, while resource delegation remains scoped. `broker-profile --profile <path>` validates configured
hashes and paths; it does not start or certify a Runtime. OS/container isolation
must prevent the model from reading daemon config, vaults, owner/identity state
or unlock. A same-user socket alone provides no such isolation. Tool-request
counts do not enforce provider/model fees or world costs; the Runtime profile
must separately disclose and enforce its approved cost scope.

The executor vault pins its original operation directory and tool-budget root.
After initialization, missing roots, missing key markers or budget counters
block recovery; restarting or renaming a resource cannot initialize fresh
originals or reset its budget. Restore/reconcile the approved state through the
trusted control surface; never replace an unknown operation automatically.
