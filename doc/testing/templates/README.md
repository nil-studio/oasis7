# Testing templates and examples

This directory contains reusable schemas and deliberately non-live examples. It is a control surface, not an evidence archive.

## Claim boundary

- A template, blank field, sample `pass`, checklist, or example lane never proves that a test ran or passed.
- `*.example.json`, `*.example.txt`, and `*.template.tsv` values are non-live inputs. Addresses under `.example.invalid`, sample genesis, bootstrap peers, manifests, lane rows, account values, and endpoints must be replaced and validated for the current execution window.
- `mainnet`, `public_testnet`, legacy-network, incident, release, proof, playability, and hosted-world filenames describe the target schema only. They do not establish network health, readiness, public availability, incident closure, player validation, settlement, or release approval.
- Completed evidence belongs in the governed evidence/task sink and must bind commands, inputs, outputs, owners, time window, and current authority; do not edit a template in place and cite it as a result.

## Scope and records

Use a template only for the test, review, release or incident scope that calls for it. Its fields do not create a universal PR checklist, new task identity, permission to operate, or approval to publish. Existing task references may be reused; ordinary changes may use the actual PR. Required proof, same-window identity, safety constraints and domain-specific release decisions remain mandatory within their applicable scope. Preserve completed runs separately from blank templates; later outcomes do not overwrite earlier failures.

## Active proof and public_testnet templates

The following active schemas remain reusable operator/QA evidence templates (non-live schemas only; filling them requires fresh same-window execution evidence):

- [Chain proof evidence v1 template](chain-proof-evidence-v1-template.md)
- [External verifier light-client-lite v1 template](external-verifier-light-client-lite-v1-template.md)
- [State / resource / receipt proof evidence v1 template](state-resource-receipt-proof-evidence-v1-template.md)
- [Validator finality proof evidence v1 template](validator-finality-proof-evidence-v1-template.md)
- [Public testnet rehearsal record template](public-testnet-rehearsal-template.md)
- [Public testnet exit review template](public-testnet-exit-review-template.md)

## Lifecycle

A zero text-reference count is not sufficient for deletion because schemas may be copied by operator or external workflows. Retirement requires a reviewed successor, semantic absorption, repaired callers, domain-owner approval, and QA/repository-health verification.
