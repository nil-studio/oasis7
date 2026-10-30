# Fresh WorldService Preview Bootstrap

- Owner role: `blockchain_ops_engineer`
- Review roles: `runtime_engineer`, `qa_engineer`
- Scope: a new, isolated DevLocal technical preview; never recovery of an existing world or formal authority activation.
- Authorities: [P2P identity/profile boundaries](../../p2p/prd.md#p2p-authority-profiles), [hosted player isolation](../../p2p/blockchain/hosted-player-access-operator-runbook.md), [design](../../p2p/design.md#des-p2p-fresh-preview-bootstrap).

## Inputs and trust

The operator independently fixes the exact world/node IDs, a protected node identity directory, a fresh absolute destination and a loopback WorldService endpoint. Generate an explicit 32-byte genesis initialization nonce once and retain it privately with the operation inputs. It is not a simulation seed, token, password or formal issuer authorization.

Use the existing `provision-identity --config-dir <absolute-private-directory> --node-id <exact-node-id>` only when creating a new node identity. When preserving a node identity, supply its existing, validated key configuration. The key directory must be owned by the invoking user with mode 0700; `node-keypair.toml` is a regular, non-symlink 0600 file. Generate and run on the same machine so the node signing seed need not be exported.

The initializer consumes these explicit inputs:

```bash
oasis7_chain_runtime initialize-preview-world \
  --root <fresh-absolute-root> \
  --world-id <new-world-id> \
  --node-id <exact-node-id> \
  --key-dir <absolute-private-key-directory> \
  --status-bind 127.0.0.1:5121 \
  --genesis-nonce <retained-64-hex-initialization-nonce>
```

Do not use an old world's ID or overwrite a partial old root. Stop/fence any old writer before switching the runtime to a new initialized root; keep its data and configuration separately. A new world is not a rollback or repair of old lineage.

### Explicit local provider preview

Trace refs: O7-REQ-0bb14512-622e-48fc-879f-bd9f00127b39. For an isolated provider transport/playtest world, add all of:

```text
--profile devlocal-provider-v1
--provider-wasm <absolute-canonical-suite-WASM>
--provider-metadata <absolute-canonical-suite-build-metadata>
--provider-agent-id <existing-canonical-preset-agent>
--provider-owner-binding <new-local-owner-binding>
--provider-finality-block-hash <explicit-DevLocal-blake3:64-hex-marker>
--provider-allowance <1..128-fixed-units>
```

The local marker is an explicit DevLocal authority input, not a formal BFT proof. Retain the canonical suite's provenance and verify its applicability to the tested source; this initializer verifies Runtime module admission and input hashes, not the builder's trust. Only this explicit fresh profile selects the existing DevLocal release security policy at creation; the default public preset remains hardened and Runtime still rejects provider provisioning under production policy. The module must be installed before the immutable cognition binding. Runtime creates the finite grant and invocation, ordinary `ClaimStarterOc` establishes the canonical owner and bounded starter funds, and the owner signs the executor delegation. No W3 325-unit funding is added. Incorrect Agent/owner inputs reject initialization.

The profile also writes private `provider-authority.json`, `owner.key`, `executor.key`, and `agent-viewer.env`. The last file selects `agent:<id>` reads and generation-1 executor writes; keep it private and supply the authority file through Viewer's existing `--provider-bootstrap-authority` input. It contains no model credentials. Start the chain with the initialized execution paths and original node identity; do not request a second local-provider initialization over this root. The default `private/viewer.env` remains public-read only. Grant expiry is real: this initializer neither renews grants on rerun nor removes normal Runtime validation. Provider/Viewer Play/Pause/Resume and any BYOK calls require a separate actual run.

All profile inputs, artifact hashes and generated public keys are committed by genesis and the signed receipt. A profile/input/artifact mismatch rejects a rerun without writing. A root initialized by an older source is not automatically upgraded; its original source commitment remains authoritative.

## Generated materials

The command creates the actual canonical Runtime initial state and journal, binds the new world identity, and publishes real bounded node/execution commits into CAS. The initial commit must use the provisioned node's derived consensus signer and matching validator public-key binding; verify its actual signature separately from the service-signed initialization receipt. Its domain-separated genesis digest binds versioned initial material, explicit initialization inputs and the derived service public key. Retained initial bytes permit recomputation; an arbitrary genesis label or a hash of a directory path is insufficient.

Public material contains the world identity, initialization receipt, genesis material commitments, service public-key pin and immutable initial publication references. The service key is the node's derived feedback-submit signing identity, not an arbitrary node public key. Independent public-read credentials and Viewer configuration remain in private 0600 files; public receipts and CAS must not contain signing seeds.

The layout is `world/`, `records/`, `store/`, `replication/` and `state.json`, with `initialization-receipt.json`, `viewer.public.json` and `genesis/initial-artifact.json` as public material. Retain `replication/replication_commit_messages/` for the signed initial commit; its payload hashes must match the pinned execution record. The receipt authenticates a fixed-layout `genesis/initial-publication/` archive containing the original execution record, signed envelope, and exact snapshot, journal and world-head-proof bytes. That archive keeps the initial proof verifiable after historical CAS retention collects those objects; it does not replace live CAS validation of the current head. `private/public-reader.key` and `private/viewer.env` contain private consumer material. The receipt is signed under its explicit preview-initialization domain; initializer source/schema commitments bind the original CLI version. A future mismatched initializer rejects a rerun instead of migrating or resealing the old root implicitly.

The root is staged in a private sibling on the same filesystem, synchronized, then published with an operating-system atomic no-replace operation. Existing targets are verified against their original receipt, identity, signer, retained genesis bytes, archived initial proof and live current CAS/record/proof; they are not rewritten or silently repaired. A legacy root without the authenticated archive fails closed and requires a verified backup; the initializer does not reconstruct, rewrite or recertify its original proof. Conflicting or incomplete roots fail closed.

## Starting and checking the consumer

Use the generated layout for the chain execution-world, records, CAS and execution-state paths, and keep the same node identity configuration. Do not let default `--runtime-root` resolution select a different store. Inject the private Viewer configuration through the service manager; do not paste it into logs, a PR, screenshots or shell arguments.

The consumer fixes `OASIS7_WORLD_SERVICE_ENDPOINT`, `OASIS7_WORLD_SERVICE_PUBLIC_KEY`, `OASIS7_WORLD_SERVICE_WORLD_ID`, `OASIS7_WORLD_SERVICE_GENESIS_DIGEST`, `OASIS7_WORLD_SERVICE_SCOPE` and `OASIS7_WORLD_SERVICE_READ_PRIVATE_KEY`. Public scope only permits reading. Agent writes require their separate signer, valid owner/executor delegation and generation; initialization does not synthesize those grants.

Measure a real signed read on the deployment machine, including snapshot restoration and response verification. The default HTTP request timeout is 1500 ms. For slower preview hosts, set `OASIS7_WORLD_SERVICE_TIMEOUT_MS` explicitly in the consumer's service environment; accepted values are integer milliseconds from 100 through 30000, and invalid values reject startup. The launcher inherits this setting into the Viewer. The hosted identity adapter also uses it as one total loopback RPC budget for Hello and the runtime fence query or session release; its default remains 2000 ms. Verify account admission as well as Viewer reads, since their caller budgets are separate. A 10000 ms budget can accommodate reads taking several seconds; verify the measured latency rather than treating a larger timeout as proof of health. Viewer periodic refresh uses one total job budget equal to this timeout, bounded between 2 and 30 seconds; its single worker and single queued job remain bounded. Other caller deadlines may impose a shorter budget. This setting does not extend registration grants, task contexts, or signed request deadlines, and does not alter identity, signature or response-size validation.

Verify on the deployment machine that the original receipt/genesis commitments still match, the node opens the recorded CAS and commit proof, and a signed WorldService read validates against the independently pinned service key and world. Verify the same after a controlled restart. Loading JSON or reaching a status port alone does not establish this chain.

## Regression and acceptance boundaries

Test fresh initialization and actual pinned read, matching rerun after state advancement, conflicting world/node/key/nonce, missing or modified genesis/receipt/CAS/proof, invalid key permissions and symlinks, and competing target publication. Failed initialization must not overwrite existing worlds, publish partial success or repair corrupt inputs.

Record the actual source and binaries used. Cross-platform generation is not presumed compatible: relevant schema/hash/persistence code must be equivalent and the target machine must actually open and verify the pin.

These materials prove isolated preview initialization only. Pending cognition binding and bounded local commits do not certify formal finality, activation/profile history, distributed consensus, independent failure-domain durability, production minting or invitation readiness. Complete account/world-control and applicable gameplay acceptance separately before expanding the player entry; deployment observations belong in the delivery issue.

## Explicit offline historical material rehydration

Stop the original writer. Keep the original root, signed initial-publication archive, commit envelopes,
records and CAS intact. `rehydrate-preview-history` acquires the same world-writer exclusion as the
node; a running writer is a hard refusal. Supply the independent service public-key pin, an absent
scratch path outside the source root, and finite operator budgets. It reads no signing seed and makes
no network or model call.

```sh
oasis7_chain_runtime rehydrate-preview-history \
  --storage-profile dev_local --root /absolute/original-preview \
  --scratch /absolute/private-scratch/history-17 \
  --world-id exact-world --node-id exact-node --service-public-key INDEPENDENT_PUBLIC_PIN \
  --height 17 --max-replay-heights 13 --max-replay-bytes 67108864 --deadline-ms 60000 \
  --max-store-bytes OPERATOR_CAS_BYTE_CAP --max-new-bytes OPERATOR_NEW_BYTE_CAP \
  --max-store-objects OPERATOR_OBJECT_CAP --max-history-cuts 4
```

The numeric replay values above illustrate one original h4 to h17 cut; they are not defaults or formal
BFT limits. The history cache has a separate implementation bound of 64 cuts. The store cap counts
physical CAS blob entries, including orphan and failed publication temporaries, with new objects
reserved at their conservative raw size. It is not a cap on the whole world root. Pin metadata is
separately bounded by the cut count and the 16 KiB per-shard reader limit. The command never prunes
existing material to fit a budget. If an explicit DevLocal participant context is genuinely required,
provide its independently fixed public pin through `--operator-public-key`; snapshots cannot select it.

Only after exact original replay and complete proof/effect comparison does publication copy missing
hash-identical historical bytes into original CAS and publish an independent exact pin. A partial
failure may retain unreferenced verified bytes or a private publication temporary; retries count those
bytes/objects against the same store cap. An incomplete pin-namespace temporary blocks subsequent
publication rather than allowing an unbounded series of new metadata files. Investigate such errors;
do not delete unknown material or treat a failed command as a completed read.

The old compact record, current head, world state, nonce and model ledger remain unchanged. Historical
queries read the original proof, exact pin and CAS through the strict restore path; they never run
replay or repair pins. Missing closure remains `HistoryUnavailable`, and corruption fails closed.
The history pin also protects its original record/proof from later cooperative retention. Retain the
scratch until the result has been checked; no implicit cleanup or formal/network finality claim is made.
