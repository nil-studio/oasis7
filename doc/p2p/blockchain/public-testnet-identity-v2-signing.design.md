# Public-testnet identity-receipt v2 signing design

Status: **OFFLINE IMPLEMENTED / LIVE ADMISSION NOT PROVISIONED**

- Task: GitHub Issue `3607`, UID `task_174f0a5a87394012b071171cc4a52372`
- Frozen design context (historical): HEAD `3fc05dec53312617a1d7c795c10548c22784b4f8`
- Implementation state: the offline signing tool, four-command sidecar bridge,
  v2 evidence-map planner gate, and five-map aggregator are implemented in the
  current task overlay. They are not production provisioning or release
  evidence.
- Scope: define the dedicated identity-v2 signing, assembly, verification, and
  trust-config contract and its implemented file/CLI bridge; this document does
  not create keys, call a provider, provision trust anchors, or authorize live
  deployment.
- Authority: [`public-testnet-governed-bootstrap.runbook.md`](public-testnet-governed-bootstrap.runbook.md)
  and [`public-testnet-governance-trust-root-provisioning.md`](public-testnet-governance-trust-root-provisioning.md).

## 1. Decision and non-goals

Use a dedicated identity-receipt-v2 domain with a detached Ed25519 signature
produced by an explicitly approved external-custody provider, followed by an
independent verifier. The provider and verifier consume the same immutable
canonical payload bytes. Private key material stays outside the repository,
sidecar, planner, adapter, logs, and ordinary CLI arguments.

This new authority/tooling contract does not reuse an existing domain key.
The runtime `identity-receipt` command remains a raw metadata producer;
the rebuild-proof signer/verifier, node-consensus/feedback signer, finality
signers, and rollback signer are not identity-v2 signers. The current fixture
keys in `scripts/fixtures/oasis7-governance-root.v1.json` remain fixture-only
and must never be selected as production custody.

The offline executable path is implemented, but no live provider is configured.
Until the approved provider, trust configuration, independently pinned registry,
and independent verifier exist, production `current_admission` remains
`capability_blocked`; a callback, copied signature, or `verified: true` field
cannot satisfy admission.

This proposal does not supersede the active runbook's existing-root requirement.
The preferred authority migration is a dedicated identity-v2 profile authorized
under that existing governance root, not an automatic new root or reuse of a
node-consensus key. If that root cannot authorize the profile, a separate human
approved governance-authority change is required before implementation can be
activated. Tool development alone grants no signer or trust-root authority.

## 2. Options considered

| Option | Result | Reason |
| --- | --- | --- |
| Reuse runtime feedback/rebuild-proof signer and verifier | Reject | Its Ed25519 key and payload domain are node-consensus/rebuild-proof scoped. It cannot attest identity-v2 context or the exact raw-v1 byte stream. |
| Make the sidecar call OpenSSL or an offline callback directly | Reject | A generic detached primitive does not establish signer custody, allowlist, rotation/revocation, or an independent verifier receipt. Test callbacks also cannot be deployment evidence. |
| Dedicated prepare/sign/assemble/verify tools with external custody | Recommend | Separates deterministic payload construction, key custody, envelope assembly, and independent admission verification. It gives the adapter an auditable raw-byte and context bridge without placing secrets in this repository. |

## 3. Trust and authority boundary

The proposed deployment-bound regular file is:

`/operator/truth/identity-v2-trust-config.json`

Its repository-owned schema fixture, if separately approved, should live at a
future `scripts/fixtures/oasis7-identity-v2-trust-config.v1.json`; no such
production configuration is created by this slice. The existing
`/operator/truth/governance-root.json` provisioning and regular-file rules
remain applicable to the root that authorizes this profile.

The trust config has exact fields:

```json
{
  "schema_version": "oasis7.identity_v2_trust_config.v1",
  "network_id": "<governed-network-id>",
  "trust_root_id": "<governed-root-id>",
  "verifier_id": "governed-receipt-verifier",
  "algorithm": "ed25519",
  "rotation_epoch": "<governed-rotation-epoch>",
  "allowlist": [
    {
      "signer_id": "<dedicated-identity-v2-signer-id>",
      "public_key_ref": "<operator-managed-public-key-reference>",
      "public_key_sha256": "<64-lower-hex>",
      "status": "active",
      "valid_from": "<RFC3339 UTC>",
      "valid_until": "<RFC3339 UTC>"
    }
  ],
  "revocations": []
}
```

`public_key_ref` points only to public material. The verifier checks the trust
config and every ancestor for regular-file/no-symlink policy, ownership, mode,
exact schema, duplicate entries, and the deployment-truth file digest. Root,
network, verifier, algorithm, and rotation epoch are not caller-selectable.

The current-admission verifier resolves the code-owned path above and checks its
exact file SHA-256 against an independently installed deployment-authority pin,
itself anchored by the approved governance root. The caller cannot supply or
change that pin through a plan, template, manifest, environment override, or
verification receipt. A differing `--trust-config` path is rejected; the option
is an assertion of the configured path, not authority selection. Missing pins
or unapproved profile authorization fail closed. Recording a digest after
verification is evidence, not an independent trust anchor.

An active signer is accepted only when its validity interval covers the signed
receipt's `issued_at` and the signer is not revoked at that capture window.
Rotation adds a new signer identity and records the old signer as retired or
revoked; it never silently replaces a public key under the same identity.
Historical verification may use a retired key only when the trust ledger proves
that it was active at the receipt's capture window. Revocation reason, issuer,
effective time, and replacement identity are auditable public metadata.

Verification has two disjoint modes: `historical_audit` and `current_admission`.
Historical acceptance emits `historical_only=true, apply_authorized=false` and
cannot be consumed by destructive apply. Current admission rechecks the trusted
current effective signer status and rejects retired or revoked keys even when
they were active at issuance. It also requires the envelope, context, provider
attestation, and pinned trust config to carry the same current `rotation_epoch`;
an unknown or stale epoch is a blocker. Historical mode remains forensic-only
and requires separate historical-ledger proof at its consuming boundary.
Mode, evaluation time, trust-config digest and authorization result are bound
into the verifier receipt; apply independently revalidates current authority.

The production signer is an external custody/HSM/KMS authority selected by
blockchain-ops/governance. It attests signer identity, algorithm, and SHA-256
of the exact payload bytes. The CLI accepts no private-key path, secret, seed,
token, or provider credential.

`provider-ref` is a non-secret provider ID from a governance-approved registry,
not an endpoint or command. A pinned registry maps it to the permitted signer,
public-key digest, algorithm, authenticated transport and custody adapter digest.
Credentials are supplied exclusively by the operator's external custody runtime;
no CLI argument can override endpoint, command, trust anchor or authentication.
The authenticated provider receipt is `oasis7.identity_v2_provider_attestation.v2`
with the exact fields `schema_version`, `network_id`, `provider_id`, `request_id`,
`signer_id`, `public_key_sha256`, `algorithm`, `canonical_payload_sha256`,
`signature_sha256`, `context_digest`, `rotation_epoch`, `capture_window_id`,
`issued_at`, `expires_at`, `task_uid`, `head_oid`, `proof_ref`, and `proof`.
The nested proof is exactly
`oasis7.identity_v2_provider_authentication_proof.v1` with only
`schema_version`, `algorithm`, `claims_sha256`, and `signature_hex`. Its
domain-separated canonical claims are
`oasis7.identity_v2_provider_authentication_claims.v1` under
`oasis7.identity-v2-provider-authentication/v1`; they bind the provider,
request, signer/key, payload/signature digests, context, task/HEAD, rotation,
window, issue/expiry times, and proof reference. The provider signs
`OASIS7-IDENTITY-V2-PROVIDER-AUTH\0` plus those claims with the same public key
resolved from the pinned registry. The independently pinned provider identity
must authenticate this receipt; exact payload, signature, proof, and request
bindings are checked again by the independent verifier. `req-v2:<64 lowercase
hex>` is generated as a CSPRNG request challenge and `proof-v1:<64 lowercase
hex>` is a bounded reference; format checks do not prove entropy or prevent
covert data. The offline implementation has no provider-side durable one-shot
replay authority, so freshness/uniqueness are binding checks rather than proof
of provider consumption. A self-reported JSON claim is not custody attestation;
absence of the external authority still blocks production signing.

## 4. Canonical payload and context

The signed payload is a finalized JSON object with exactly these fields:

```json
{
  "domain_separator": "oasis7.identity_receipt.v2/signature/v1",
  "schema_version": "oasis7.identity_receipt.v2",
  "signer_id": "<allowlisted-id>",
  "verifier_id": "governed-receipt-verifier",
  "trust_root_id": "<governed-root-id>",
  "network_id": "oasis7-public-testnet-governed-20260606",
  "task_uid": "<task-uid>",
  "head_oid": "<frozen-head-oid>",
  "frozen_head_oid": "<frozen-head-oid>",
  "plan_digest": "<pre-receipt-plan-intent-sha256>",
  "context_digest": "<canonical-context-sha256>",
  "capture_window_id": "<window-id>",
  "rotation_epoch": "<rotation-epoch>",
  "issued_at": "<RFC3339 UTC>",
  "expires_at": "<RFC3339 UTC>",
  "node_id": "<governed-node-id>",
  "peer_id": "<runtime-peer-id>",
  "key_sha256": "<64-lower-hex>",
  "key_size_bytes": 1,
  "key_mode": "0600",
  "key_uid": 0,
  "key_gid": 0,
  "signed_payload_sha256": "<sha256-of-exact-raw-v1-bytes>"
}
```

`plan_digest` is the digest of a separate, pre-receipt
`oasis7.clean_room_plan_intent.v2` document binding governed task UID,
frozen HEAD, capture window, network/adapter action, managed node names/roles,
and canonical reset-surface identifiers. It excludes identity receipts,
signatures, verification outputs, and final-plan receipt digests. Its canonical
JSON (UTF-8, sorted keys, compact separators, finite values) is hashed once;
the final receipt-bearing plan is never fed back into `plan_digest`.

The exact context schema is `oasis7.identity_v2_context.v1`: fields are
`schema_version`, `network_id`, `task_uid`, `head_oid`, `capture_window_id`,
`capture_start`, `capture_end`, `rotation_epoch`, `issued_at`, `expires_at`.
All are non-empty strings; timestamps use UTC `YYYY-MM-DDTHH:MM:SSZ`, HEAD is
40 lowercase hexadecimal characters, and timestamp ordering is checked.
The exact plan-intent fields are `schema_version` (the value above),
`context_digest`, `adapter_action`, `peer_registry_sha256`,
`peer_registry_epoch`, and `nodes`. Each node has only `node_name`,
`node_id`, `peer_id`, `role`, and `reset_surface_ids`; identities are strings,
surfaces are unique strings, nodes are sorted by unique `node_name`, and surface
IDs are sorted. Node set, roles, action and surfaces must equal independently
approved deployment truth, not values invented by a receipt.
The intent always contains all five canonical managed nodes, including when
preparing one node's envelope. The digest and epoch must match the current
independently pinned managed-peer registry, whose shared loader is
`scripts/p2p-public-testnet-peer-registry.py`. These fields enter the canonical
intent digest and are therefore signed through the payload's `plan_digest`.
`peer_registry_epoch` is the registry's `registry_epoch`, distinct from the
context's signing-key `rotation_epoch`. Registry byte-digest or epoch changes
invalidate old intent, evidence-map, plan and resume admission. Current
validation rejects intent v1; it never adds missing fields or upgrades old
signed evidence. The registry pin remains unprovisioned, as specified by the
[deployment contract](./public-testnet-governance-trust-root-provisioning.md#managed-peer-registry-not-provisioned).
Both documents reject duplicate/unknown fields and noncanonical serialization;
their digests use the same sorted-key compact JSON encoding as the payload.
`context_digest` binds the exact canonical context and is signed. Shared fields
must agree across context, intent, raw identity and payload. Neither input may
contain receipt, payload, signature, final-plan or verification-output fields.

The one-way dependency is: canonical context -> context digest -> plan intent
-> plan-intent digest -> payload -> signature/envelope -> verification receipt
-> final plan. Raw-v1 bytes independently feed the payload's raw digest. No
downstream digest is allowed in an upstream input, including final-plan hashes.

The payload bytes are exactly:

```text
ASCII("OASIS7-IDENTITY-RECEIPT-V2\0")
+ UTF-8(JSON object above, ensure_ascii=true, sort_keys=true,
        separators=(",", ":"))
```

The signer signs those bytes with Ed25519. `canonical_payload_sha256` in the
prepare manifest is SHA-256 of the complete byte sequence above. The existing
wire field `signed_payload_sha256` retains its runbook meaning: SHA-256 of the
exact raw `oasis7.identity_receipt.v1` bytes, including whitespace and the
host-specific `key_path`. The v2 envelope does not expose `key_path`; the raw
digest binds it without making it a direct admission field.

Signed fields are every field in the payload object, including signer,
verifier, root, governed network ID, task/head/plan, freshness, node/peer, key tuple, and raw-byte
digest. `signature_hex`, `canonical_digest`, `authenticated`, and `verified`
are not signed. `signature_hex` is the lowercase 128-hex Ed25519 result;
`canonical_digest` is SHA-256 of canonical envelope JSON omitting only
`canonical_digest`, `authenticated`, and `verified` (and includes the
signature). It is an integrity aid, not cryptographic verification.

The template supplies only governed signed fields. It must not supply a
signature, digest, or verification verdict. If a compatibility reader sees
those fields in an older template, it must reject non-empty/non-default values
and regenerate them; it may not copy them into the final receipt.

## 5. Strict validation rules

- Parse UTF-8 JSON with duplicate-key rejection; reject BOM, non-finite
  numbers, unknown/missing fields, wrong types, malformed IDs, and
  non-canonical casing/serialization. Never normalize after payload selection.
- Require `head_oid == frozen_head_oid`, exact task UID, pre-receipt plan
  digest, capture window, rotation epoch, and trust-config signer/verifier/root.
- Require `issued_at < expires_at`, explicit UTC timestamps inside capture
  bounds, and an unexpired interval with configured clock-skew allowance.
- Require exact raw-v1 schema/fields and non-empty regular non-symlink bytes;
  hash before parse/projection. Match node/peer and the complete key tuple
  across raw v1, template, payload, and deployment inventory. Never reconstruct
  or open `key_path` during admission.
- Require the managed node set exactly once; reject duplicate nodes/peers or
  raw-map entries and cross-paired node/template/context files.
- Check provider payload digest, signer, algorithm, and signature shape, then
  independently verify Ed25519 over exact bytes. Recompute every digest and
  binding before setting either verdict true; never trust copied flags/signature.

## 6. Executable contract (offline implementation; live authority not provisioned)

The implemented dedicated executable is
`scripts/p2p-public-testnet-identity-v2-signing-tool.py`. It has four
file-oriented subcommands and uses only regular files plus an
operator-approved provider bridge. The exact parser contract is:

```bash
python3 scripts/p2p-public-testnet-identity-v2-signing-tool.py prepare \
  --raw-v1 <raw-v1.bin> --template <unsigned-template.json> \
  --context <context.json> --plan-intent <plan-intent.json> \
  --trust-config /operator/truth/identity-v2-trust-config.json \
  --provider-registry /operator/truth/identity-v2-provider-registry.json \
  --payload-out <payload.bin> --manifest-out <prepare.json>

python3 scripts/p2p-public-testnet-identity-v2-signing-tool.py sign \
  --payload <payload.bin> --manifest <prepare.json> \
  --provider-registry /operator/truth/identity-v2-provider-registry.json \
  --provider-ref <approved-custody-ref> \
  --signature-out <signature.hex> --attestation-out <sign-attestation.json>

python3 scripts/p2p-public-testnet-identity-v2-signing-tool.py assemble \
  --payload <payload.bin> --manifest <prepare.json> --signature <signature.hex> \
  --attestation <sign-attestation.json> \
  --provider-registry /operator/truth/identity-v2-provider-registry.json \
  --out <unsigned-envelope.json>

python3 scripts/p2p-public-testnet-identity-v2-signing-tool.py verify \
  --mode current_admission \
  --envelope <unsigned-envelope.json> \
  --attestation <provider-attestation-v2.json> --raw-v1 <raw-v1.bin> \
  --context <context.json> --plan-intent <plan-intent.json> \
  --trust-config /operator/truth/identity-v2-trust-config.json \
  --provider-registry /operator/truth/identity-v2-provider-registry.json \
  --out <verified-envelope.json> --verification-out <verification.json>
```

`prepare` is deterministic and emits exact payload bytes plus raw/canonical
digests and sizes, template/context digests, all task/head/plan/window/rotation
bindings, signer/verifier/root IDs, and algorithm. `sign` asks only the approved
provider; it has no private-key option and fails closed without a provider.
`assemble` accepts no field overrides, checks the attestation, attaches only
the signature, and sets verdicts false. `verify` ignores incoming verdicts,
reconstructs and byte-compares the payload, verifies trust and Ed25519, then
writes freshly computed verdicts and an independent receipt. Only unsigned
verdict/digest fields may change after `prepare`.

The provider registry's `verifier.executable_path` is the authoritative
independent verifier selection; it must be a distinct executable from this
signing tool, the sidecar, planner, adapter, and provider adapters. `verify`
invokes that exact executable with the fixed file-oriented `verify` argument
shape in a private temporary output directory, after validating the pinned
registry and local bindings. A non-zero exit, missing/noncanonical output, stale
receipt, or any mismatch between the verifier's output pair and the local
recomputation fails closed before either caller output is written. Only the
canonical output pair returned by the pinned registry verifier is promoted to
the requested paths. The sidecar's `--verifier-tool` is an assertion of this
registry-selected path, never an alternate verifier choice.

The handwritten Ed25519 verifier decodes both public-key and `R` points only
when they are canonical, non-identity points in the prime-order subgroup;
small-order keys or forged `R=identity,S=0` signatures are rejected.

All trust-config, provider-registry, public-key, custody-adapter, and
independent-verifier files are deployment-owned regular non-symlink artifacts.
The operator-local owner must be the account running admission. Artifacts must
not be writable by group/other accounts. Ancestors must be owned by root or
the admission account and obey the same write-bit rule except for root-owned
sticky directories. Ancestor symlinks are rejected except for
the fixed `/var` and `/tmp` system aliases used on macOS. Public
metadata and public keys are not secrets: owner-readable `0644` is permitted
when those write bits are absent. Adapter/verifier artifacts additionally
require owner execute. Authority reads use an `O_NOFOLLOW` descriptor with
pre/post `fstat` identity checks; the adapter/verifier digest is rechecked
around its subprocess before outputs are promoted. The sidecar captures the
registry-selected verifier's initial protected file identity, then checks that
same identity, protected metadata, and pinned digest immediately before and
after verifier execution, before promoting evidence. These protected reads and
pre/post checks do not guarantee atomic execution against a hostile same-owner
process that replaces and restores an executable between checks. Public files
do not require `0600` modes under this contract.

Before any derived-output preflight, each output path is rejected if it aliases
an input, authority artifact, another output, or a code-owned deployment path.
Existing derived outputs are retained on validation failure and replaced only
by the final atomic write after the command succeeds.

`assemble` opens the payload as a regular non-symlink file and requires its
exact bytes, size and SHA-256 to match the prepare manifest, canonical payload
reconstruction and authenticated provider receipt. It verifies the detached
signature against these same bytes before emitting an envelope. Any mismatch
aborts without an output; a manifest alone cannot stand in for the payload.

The verification receipt records raw-v1, canonical-payload, and envelope
digests, signer/public-key and trust-config/registry digests, verifier executable
digest, provider `proof_ref` and `proof_claims_sha256`, capture context, and
result. It contains no private provider output.

## 7. Context, retention, and raw-byte executable bridging

The single-node sidecar now accepts explicit `--context`, `--plan-intent`,
`--trust-config`, `--provider-registry`, `--provider-ref`, `--signer-tool`,
`--verifier-tool`, `--evidence-map-out`, and `--evidence-dir` arguments. In
bridge mode these seams are all required, the sidecar passes the
original `--raw-v1` unchanged, and it orchestrates the four fixed commands in a
private temporary directory. Missing tool, provider, trust config, registry,
or verifier receipt remains a capability blocker.

When `--evidence-dir` is supplied, the sidecar atomically promotes a
transaction-unique directory containing the exact context and plan-intent
files plus these seven per-node artifacts: `raw_v1`, `prepare_manifest`,
`payload`, `provider_attestation`, `unsigned_envelope`, `signed_envelope`, and
`verification`. The evidence root and transaction directory are mode `0700`;
retained files are owner-only mode `0600`, regular non-symlink files with
symlink-free ancestors. Each descriptor records only `path`, `sha256`, and
`size_bytes`; staging writes and parent-directory fsync precede promotion, and
an incomplete private stage is cleaned up on failure. The provider-attestation
object has an exact field allowlist, but the value/content sensitivity of its
detached authentication proof remains a provider/runtime diagnosis. Credentials,
private keys, and raw command output must not be placed in retained evidence.

The planner and adapter consume one exact v2 evidence map. A sidecar emits a
one-node map; `scripts/p2p-public-testnet-identity-v2-evidence-aggregate.py`
accepts exactly five such maps, requires one entry for each canonical node in
the fixed order `storage-205`, `sequencer-204`, `linux-lan-observer`,
`windows-observer`, `macos-observer`, and re-runs the canonical planner
validation before atomically writing the aggregate. The map shape is:

```json
{
  "schema_version": "oasis7.identity_v2_evidence_map.v2",
  "network_id": "oasis7-public-testnet-governed-20260606",
  "task_uid": "<task-uid>",
  "head_oid": "<frozen-head-oid>",
  "context": {"path": "<context.json>", "sha256": "<64-lower-hex>", "size_bytes": 1},
  "plan_intent": {"path": "<plan-intent.json>", "sha256": "<64-lower-hex>", "size_bytes": 1},
  "entries": [
    {
      "node_name": "<managed-node-name>",
      "node_id": "<governed-node-id>",
      "peer_id": "<expected-peer-id>",
      "raw_v1": {"path": "<raw-v1>", "sha256": "<64-lower-hex>", "size_bytes": 1},
      "prepare_manifest": {"path": "<prepare.json>", "sha256": "<64-lower-hex>", "size_bytes": 1},
      "payload": {"path": "<payload.bin>", "sha256": "<64-lower-hex>", "size_bytes": 1},
      "provider_attestation": {"path": "<provider-attestation.json>", "sha256": "<64-lower-hex>", "size_bytes": 1},
      "unsigned_envelope": {"path": "<unsigned-envelope.json>", "sha256": "<64-lower-hex>", "size_bytes": 1},
      "signed_envelope": {"path": "<verified-envelope.json>", "sha256": "<64-lower-hex>", "size_bytes": 1},
      "verification": {"path": "<verification.json>", "sha256": "<64-lower-hex>", "size_bytes": 1}
    }
  ]
}
```

The map contains the exact managed node set once. The planner validates every
descriptor, context/intent binding, seven-artifact closure, node/peer registry
binding, and current-admission envelope/verification receipt. It never derives
bytes from `key_path`, synthesizes JSON, or pairs a receipt by list position.
The map path is transport-only; the verifier hashes and validates the opened
bytes first. The canonical network binding is
`oasis7-public-testnet-governed-20260606`; it is not caller-selectable.

The planner CLI requires `--identity-v2-evidence-map <verified-map.json>`.
Every adapter invocation carrying identity-v2 inputs, including an `--apply`
invocation, must carry the exact frozen map with
`--identity-v2-mode current_admission`; the gate checks both before execute,
provenance verification, transaction locking, or provider transport. This
current map path remains validation-only and does not grant apply authority. A
`historical_audit` map is forensic only and is rejected at this boundary.
Current offline implementation does not provision the deployment anchors or
grant pair-executor production v2 input; the existing legacy
`--identity-receipts` path must not be repurposed.

## 8. Clean-room semantic receipt contract

Truth/probe integrity is a separate signing domain from identity-v2. An
identity envelope or `authenticated`/`verified` assertion does not authorize
package, genesis, world, checkpoint or fresh-root metadata. The planner's
`canonical_semantic_receipt_payload` defines these exact kind/schema pairs:

| kind | receipt schema |
| --- | --- |
| `package` | `oasis7.package_provenance.v1` |
| `genesis` | `oasis7.genesis_binding.v1` |
| `world` | `oasis7.world_binding.v1` |
| `checkpoint` | `oasis7.checkpoint_binding.v1` |
| `fresh_root_probe` | `oasis7.fresh_root_probe_receipt.v1` |
| `validator_verify_output` | `oasis7.validator_verify_output.v1` |

Signed material starts with ASCII `OASIS7-CLEAN-ROOM-SEMANTIC-RECEIPT-V1`
followed by one NUL byte, then UTF-8 JSON encoded with `ensure_ascii=true`,
sorted keys, compact `,`/`:` separators and `allow_nan=false`. Its exact fields
are `schema_version=oasis7.clean_room_semantic_payload.v1`, `kind`,
`receipt_schema`, `network_id`, `signer_id`, `verifier_id`, `trust_root_id`, and
`payload`. Network and verifier/root identities must match code-owned
governance authority. Detached truth/probe payloads exclude only their
`receipt` member. Inline validator-output payloads exclude only
`schema_version`, `authenticated`, `verified`, `signer_id`, `verifier_id`,
`trust_root_id`, `signed_payload_sha256`, `signature_hex`, and
`canonical_digest`. The outer probe includes both complete signed validator
outputs. Checkpoint path, size and hash are signed semantic fields; they
cannot be changed while retaining the old receipt. Truth must already equal
its canonical projection; normalization cannot silently alter signed values.

`signed_payload_sha256` hashes those exact prefixed bytes, `signature_hex` is
their strict Ed25519 signature, and `canonical_digest` must match the
repository's canonical receipt digest. Each validation rereads the pinned live
governance root, checks its root/network identities and requires exactly one
allowlisted Ed25519 key for the signer, with matching public-key SHA-256.
The bounded 128-entry cache covers only verification of the exact public-key,
material and signature bytes; it does not cache trust or current authorization.

Before apply, the adapter also sends each of the four truth receipts, two
validator outputs and outer probe to the independent verifier. Its binding
contains `kind`, `task_uid`, `frozen_head_oid`, `plan_digest`,
`consumer_impact_record`, and `semantic_payload_hex` containing the exact
signed bytes, not a caller-asserted payload digest. Transport projection retains
the nested verifier/root identity fields. Legacy generic receipts without this
binding/signature fail closed and must be regenerated by a separately
provisioned authorized signer; neither fields nor signatures are synthesized
to migrate old evidence.

This contract authenticates metadata, not the existence or contents of deployed
checkpoint files, provider custody, successful restore or live fleet health.
Those still require independent operational evidence and the existing recovery
gates. Reusing the crypto primitive does not reuse identity-v2 signer authority;
governance-root approval, signer custody and production provisioning remain
required and capability blocked. Fixture signatures are not deployment proof.

## 9. Test and evidence contract

Offline tests cover deterministic repeated preparation; duplicate/unknown JSON;
malformed types/timestamps; plan-intent/final-plan cycle attempts; changed raw
or payload bytes; changed template/context; signer/public-key mismatch; wrong
domain; cross-node/peer/window pairs; stale/future receipts; revoked/retired
keys; copied signatures; and copied verdict flags. An end-to-end test may use
an ephemeral in-memory key/provider to prove byte identity across all four
steps, but it must be excluded from deployed allowlists and readiness evidence.

Production evidence requires custody/provider attestation; trust-config,
public-key, signer/verifier executable, and exact raw-v1 digests; prepare/sign/
verify manifests; independent verification receipt; all context bindings; and
an audit showing no secret emission. The final plan may reference these
digests, but cannot alter the signed payload or pre-receipt plan-intent digest.

Synthetic fixtures, fake transport, test-only module-cache wrappers, and
temporary harness receipts are orphan/harness evidence only. They demonstrate
validation behavior but do not prove provider custody, trust-config/registry/
verifier pinning, node health, deployment, or release readiness and must not be
promoted into production evidence.

This design has no tick, replay, checkpoint, or recovery-state mutation. Its
determinism guarantee is limited to canonical byte production and exact
admission pairing. Runtime integration, custody ceremony, trust-root approval,
and real provider evidence remain residual risks requiring blockchain-ops,
runtime, QA, and TPM follow-up.

## 审读基线与当前差距

- Owner role：`runtime_engineer`、`blockchain_ops_engineer`；验证由 `qa_engineer` 独立核对。
- 本次文档/源码审读基线：`4e94163a3dff9ba69f6959b406ca3fcedf8aa097`，日期 `2026-10-15`。当前实现说明与历史阶段记录分别保留；本次不执行签名、部署、恢复或构建，不推导新的运行完成态。
- 产品关系只承接下表明确列出的局部义务；功能整体仍为部分覆盖，现有产品实现状态与原验收层级不变。

## 上游、目标与边界补充

<a id="identity-v2-role-isolation"></a>
### 身份证明与共识权限隔离

本专题仅承接 DCS-003 中的 identity-v2 signer/verifier 域隔离子范围。原 §1/3/6/8 的独立 authority、固定 verifier 与语义签名域共同限制 sidecar、planner、adapter 和服务节点：它们只能验证/传送材料，不因签名或 verified 标志取得 validator finality 或世界写入权。身份回执不能成为共识证书；批准 provider 也不能跳过治理 activation。完整服务角色、验证者转换及运行暴露面的组合验收仍由 P2P 主设计与 QA 持有。

## 状态、事务与持久化边界

原 §6 的 prepare/sign/assemble/verify 是文件处理阶段，assemble 的 verdict 为 false；只有独立 verifier 输出与本地重算一致才可发布输出对，仍不意味着世界 applied。验证失败保留已有派生产物；捕获的发布失败按 pair 恢复机制处置，恢复失败保留 backup 并要求 reconciliation，不保证跨两个输出路径的 crash-atomic visibility。输出不能 alias 输入/authority/code-owned 路径。原 §7 的私有 staging、七工件闭包与 atomic promotion 管理证据生命周期；原 §8 的 metadata authentication 不证明 checkpoint 文件存在、restore 成功或 fleet health。本设计没有 tick/replay/recovery-state 写入权。

## 兼容、部署与风险

原始 v1 receipt 原字节进入 digest，但 legacy identity-receipts 不得当作 v2 map；historical_audit 只能取证，不能 current_admission/apply。旧签名不能通过重算字段迁移。真实 custody、root/profile 批准、registry pinning 与独立 verifier 未 provisioned 时保持 capability_blocked；同 owner 替换 executable 的竞态限制保留。没有本次 live 验收，测试 provider 与 fixture 不能进入 deployed allowlist。

### 2.1 需求承接与分配表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 具体 obligation 与适用条件 | 本设计条款（path#anchor） | 外部 owner / dependency | 明确排除或未覆盖范围 |
| --- | --- | --- | --- | --- |
| [REQ-DCS-003](../../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#req-dcs-003)；Trace refs: O7-REQ-103d7ad7-33f2-41b8-a741-a9c4b383b4c3 | 身份-v2 的材料生产者/验证者不能取得共识 authority。 | [身份域隔离](#identity-v2-role-isolation) | P2P / governance root owner / QA | 只覆盖 signer/verifier 域边界，不证明全部服务角色或 BFT 隔离。 |
| [runbook identity-v2 preparation](public-testnet-governed-bootstrap.runbook.md#identity-v2-evidence-preparation-capability-gated) | 按已批准 authority 生产、校验 exact raw/context/intent 字节与完整 evidence map；缺能力 fail closed。 | [offline executable contract](#6-executable-contract-offline-implementation-live-authority-not-provisioned) | deployment authority / provider custody owner | 产品适用性：N/A；理由：本行是 operator 文件与 CLI 验证合同，不定义玩家准入；范围：prepare/sign/assemble/verify。真实 provisioning 与 apply 授权未覆盖。 |

### 11.1 验证映射表

以下是验证要求与已有入口的对应，不是本次执行结果。源码/测试定义只提供静态核对；实际验收须在同候选、适用 feature/config 与环境下运行，并保留命令、提交、输出、失败样例和未覆盖范围。文档检查通过不证明运行、部署或 release readiness。

| 上游 requirement / acceptance | 本设计条款 | 验证目标 / oracle | 验证方法 / 精确来源 | 证据产物 | 未覆盖边界 / residual risk |
| --- | --- | --- | --- | --- | --- |
| [REQ-DCS-003](../../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#req-dcs-003)；Trace refs: O7-REQ-103d7ad7-33f2-41b8-a741-a9c4b383b4c3 | [身份域隔离](#identity-v2-role-isolation) | 错误域、替换 verifier、复制 verdict 不得生成可准入结果。 | 审读并执行 [signing tool regression](../../../scripts/p2p-public-testnet-identity-v2-signing-tool.test.py) 中 registry-selected verifier、retired/revoked key 与 copied verdict 拒绝用例。 | 对应候选的命令、退出码、断言输出与环境；本次未运行此技术验收。 | 只覆盖 signer/verifier 域边界，不证明全部服务角色或 BFT 隔离。 |
| [runbook identity-v2 preparation](public-testnet-governed-bootstrap.runbook.md#identity-v2-evidence-preparation-capability-gated) | [offline executable contract](#6-executable-contract-offline-implementation-live-authority-not-provisioned) | 精确字节与 authority pin 一致；验证失败不覆盖已有输出；捕获的 publication failure 尝试恢复，恢复失败保留 backup，不保证崩溃原子性；fixture 不产生 live 证据。 | 执行 [signing tool tests](../../../scripts/p2p-public-testnet-identity-v2-signing-tool.test.py) 的 deterministic prepare、unchanged payload、output alias、publication failure 和 verifier mismatch 断言。 | 对应候选的命令、退出码、断言输出与环境；本次未运行此技术验收。 | 产品适用性：N/A；理由：本行是 operator 文件与 CLI 验证合同，不定义玩家准入；范围：prepare/sign/assemble/verify。真实 provisioning 与 apply 授权未覆盖。 |
