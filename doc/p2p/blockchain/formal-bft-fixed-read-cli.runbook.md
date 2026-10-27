# Formal BFT fixed H+1 read CLI

Trace refs: O7-REQ-0bb14512-622e-48fc-879f-bd9f00127b39, O7-REQ-792f38f5-82d4-47bc-8b1f-e4d4568e1b46; contract: [des-p2p-formal-bft-read-handoff-v1](../design.md#des-p2p-formal-bft-read-handoff-v1).

This runbook covers the first fixed-cut operator CLI for the formal BFT read handoff. It installs one externally pinned H+1 certificate into a local ledger, then optionally serves that exact already-installed cut over loopback. It does not enable formal writes, validator signing, automatic head following, private reads, production signer custody, or the complete #4975 profile.

## Trust and input files

Use Unix and absolute file paths. The config and trust files must be regular files owned by the current user with mode `0600`. The local ledger must already exist and meet the formal driver's private-directory requirements. The CAS directory must already exist, contain the referenced H material and index nodes, and share no ancestor/descendant path with the ledger. Paths containing symlinks are rejected. H material and certificates are bounded, no-follow reads; their signatures and content addresses are still independently verified.

`formal-read-trust.json` is a `FormalReadTrustConfigV1` JSON object with exactly these top-level members:

```json
{
  "initial_trust": {
    "issuer_public_key": "<externally provisioned Ed25519 public key>",
    "world": {
      "world_id": "<world>",
      "chain_id": "<chain>",
      "genesis_digest": "<digest>"
    },
    "initial_state_root": "<digest>",
    "governing_manifest_digest": "<digest>",
    "recovery_policy_digest": "<digest>"
  },
  "checkpoint": {
    "schema_version": 1,
    "initial_certificate": "<complete SignedInitialActivationV1 JSON object>",
    "active_authority": "<complete AuthorityDefinitionV1 JSON object>",
    "recovery_policy": "<complete RecoveryPolicyV1 JSON object>",
    "first_normal_position": null,
    "recovery_cut": null,
    "retained_record_id": null,
    "head_record": "<complete CanonicalRecordV1 JSON object for H>",
    "head_finalizing_authority_id": "<verified authority id>"
  },
  "checkpoint_digest": "<independently pinned typed checkpoint digest>"
}
```

The quoted values in the `checkpoint` skeleton stand for complete JSON objects, not strings in a usable file. The file must contain the full typed checkpoint fields and values. The digest is checked by `FormalReadTrustConfigV1::verified_anchor`; never copy it from a service response. Formal `world.genesis_digest` and BFT target `scope.genesis_hash` are distinct typed fields and must each match their authority source.

`read.json` is a strict `BftReadConfigV1`; unknown fields are rejected. Its `target` is the complete static `BftProfileTargetV1`. It must use profile `bft`, profile version `1`, wire version `2`, and tick ruleset `formal-position-equals-runtime-tick-v1`. `scope` contains `world_id`, `chain_id`, and a 64-character lowercase-hex `genesis_hash`. Validators are sorted by unique `validator_id`; each has a 64-hex Ed25519 `public_key_hex` and positive integer `voting_power`. The current schedule uses `mapping_ruleset="epoch-multiply-minus-two-v1"` and `profile_id="bft"`.

```json
{
  "schema_version": 1,
  "trust_config_path": "/srv/oasis7/formal-read-trust.json",
  "target": {
    "schema_version": 1,
    "profile": "bft",
    "profile_version": 1,
    "wire_version": 2,
    "scope": {
      "world_id": "<world>",
      "chain_id": "<chain>",
      "genesis_hash": "<64-lowercase-hex>"
    },
    "authority_epoch": 2,
    "initial_validators": [
      {
        "validator_id": "validator-a",
        "public_key_hex": "<64-hex>",
        "voting_power": 1
      }
    ],
    "schedule_policy": {
      "epoch_length_ticks": 100,
      "mapping_ruleset": "epoch-multiply-minus-two-v1",
      "profile_id": "bft"
    },
    "tick_mapping_ruleset": "formal-position-equals-runtime-tick-v1"
  },
  "exact_target": {
    "height": 43,
    "block_id": "<64-lowercase-hex>"
  },
  "handoff_material_path": "/srv/oasis7/handoff-h-closed-record.cbor",
  "cas_directory": "/srv/oasis7/cas",
  "ledger_directory": "/srv/oasis7/formal-bft-ledger",
  "bind": "127.0.0.1:8645",
  "service_signing_key_path": "/srv/oasis7/read-service-key.hex"
}
```

The illustrative target values must be replaced with the exact target committed by H. `exact_target.height` must equal H position plus one, and its `block_id` is an independently provisioned 64-hex exact pin. The pin is checked before opening the ledger; it is never inferred from the certificate.

`handoff_material_path` is one canonical CBOR-encoded `ClosedRecord` containing H's full closure. It is not JSON, a digest-only record, a partial object set, or data copied from a response. The reader caps it at 64 MiB, rejects trailing data and requires byte-for-byte canonical re-encoding. Business-index CAS nodes are fetched through a bounded range read, capped at 4 MiB + 128 bytes per node, then checked against their bare BLAKE3 content-address digest.

Use a dedicated CAS for this fixed read installation and serving window. Concurrent local maintenance is supported only when every accessor uses the [CAS retention protocol](../distfs/distfs-production-hardening.design.md#des-dh-local-retention-coordination) on a filesystem with reliable OS locks. Exclude old processes and direct filesystem pruning; never remove or replace `.retention.lock` while the CAS is in use. GC may win before a candidate is pinned: install then fails without publishing a commit, and a retry must retain the original QC and operation identity. Successfully pinned material is protected against cooperating local GC. These commands do not start GC or establish complete history retention or independent reconstruction eligibility.

The `--certificate` file is one canonical CBOR-encoded `BftCommitCertificateV2`, capped at 2 MiB. Its top-level fields are `proposal` and `precommits`; `proposal` contains `header`, `block`, and optional `valid_round_evidence`. Its block's computed block ID and height must both match `exact_target`. Do not manually edit or reconstruct the QC, and do not write values learned from it back into the config.

`service_signing_key_path` is needed only by `serve`: an owner-owned regular file with mode `0600`, containing a 32-byte Ed25519 secret as 64 ASCII hexadecimal characters (surrounding whitespace is accepted by the loader). Keep the key out of JSON, arguments, shell history, and logs. The `bind` must be a literal loopback socket address with a nonzero port, such as `127.0.0.1:8645` or `[::1]:8645`.

## Install and serve

```bash
oasis7_chain_runtime bft-read install \
  --config /srv/oasis7/read.json \
  --certificate /srv/oasis7/handoff-h-plus-1.cbor

oasis7_chain_runtime bft-read serve \
  --config /srv/oasis7/read.json
```

`install` verifies and durably installs the already-existing, exactly pinned H+1 QC by replaying H and executing H+1. It does not sign votes, submit blocks, or create a certificate. Success prints a bounded JSON receipt for the durable local install. `serve` reopens the existing ledger without initialization, requires the exact configured cut, and serves public reads only. No current authorized World is connected in this CLI, so private scopes fail closed. The loopback listener is not an access-control boundary or evidence of independent custody.
