# Public Testnet Guarded Faucet Operator Runbook (2026-07-04)

Lifecycle: current reusable operator procedure with a dated blocked observation. Package/install/key isolation/upstream/guarded-claim/verification/recovery steps remain procedural; the 2026-07-04 HTTP 000 and `faucet_guard_ready=block` values are historical and say nothing about current endpoint health. This runbook never proves the faucet is open or the lane is ready.

## Scope
- Task: GitHub issue #1868 / `task_17e9c8cd02014f938b29689a44cbfb89`
- Lane: `faucet_guard_ready`
- Target endpoint: `http://39.104.204.172:6681/`
- Service binary: `oasis7_testnet_faucet serve`
- Current verdict: recovery path documented; live lane remains `block` until fresh endpoint and guarded claim evidence pass.

This runbook restores the repo-owned operator path for the governed `public_testnet` faucet. It does not by itself prove that the public faucet is open.

## Current Blocker
The previous faucet evidence in `doc/testing/evidence/p2p-public-testnet-faucet-service-2026-05-19.md` showed a guarded service on port `6681` with:

- amount: `1000000`
- cooldown: `3600` seconds
- claim target format: `oc:pk:<64-hex>`
- endpoints: `/`, `/healthz`, `/claim`

The fresh 2026-07-04 recheck failed to connect to `http://39.104.204.172:6681/` and returned HTTP `000`, so `faucet_guard_ready` remains blocked.

## Package
Build a service bundle from the repo root:

```bash
./scripts/public-testnet-faucet/package-public-testnet-faucet.sh --profile release
```

The package includes:

- `oasis7_testnet_faucet`
- `scripts/public-testnet-faucet/start-public-testnet-faucet.sh`
- `systemd/oasis7-public-testnet-faucet.service`
- `examples/public-testnet-faucet.env.example`
- `RUNBOOK.md`
- `HELP.txt`
- `BUILDINFO`
- `SHA256SUMS`

Use `--profile dev --out-dir <dir> --archive <path>` only for local packaging smoke tests.

## Host Install
On the sequencer/public faucet host:

```bash
sudo install -d -m 0755 -o oasis7-testnet -g oasis7-testnet /opt/oasis7/public-testnet-faucet/releases
sudo install -d -m 0750 -o oasis7-testnet -g oasis7-testnet /etc/oasis7/public-testnet-faucet

release_id="$(date -u +%Y%m%dT%H%M%SZ)"
sudo install -d -m 0755 -o oasis7-testnet -g oasis7-testnet "/opt/oasis7/public-testnet-faucet/releases/${release_id}"
sudo tar -C "/opt/oasis7/public-testnet-faucet/releases/${release_id}" -xzf public-testnet-faucet-service-<host>.tar.gz
sudo ln -sfn "/opt/oasis7/public-testnet-faucet/releases/${release_id}" /opt/oasis7/public-testnet-faucet/current
```

Create `/etc/oasis7/public-testnet-faucet.env` from `examples/public-testnet-faucet.env.example` and keep private key material under `/etc/oasis7/public-testnet-faucet/`, outside the release directory.

Expected public-testnet env values:

```bash
OASIS7_PUBLIC_TESTNET_FAUCET_ROOT=/opt/oasis7/public-testnet-faucet/current
OASIS7_PUBLIC_TESTNET_FAUCET_LISTEN=0.0.0.0:6681
OASIS7_PUBLIC_TESTNET_FAUCET_UPSTREAM=http://127.0.0.1:6631
OASIS7_PUBLIC_TESTNET_FAUCET_AMOUNT=1000000
OASIS7_PUBLIC_TESTNET_FAUCET_COOLDOWN_SECS=3600
OASIS7_PUBLIC_TESTNET_FAUCET_REQUEST_TIMEOUT_SECS=10
```

## Start Or Restart
Install and start the systemd service:

```bash
sudo install -m 0644 \
  /opt/oasis7/public-testnet-faucet/current/systemd/oasis7-public-testnet-faucet.service \
  /etc/systemd/system/oasis7-public-testnet-faucet.service
sudo systemctl daemon-reload
sudo systemctl enable oasis7-public-testnet-faucet.service
sudo systemctl restart oasis7-public-testnet-faucet.service
sudo systemctl status --no-pager oasis7-public-testnet-faucet.service
```

The unit expects the local sequencer/API service to be reachable at `OASIS7_PUBLIC_TESTNET_FAUCET_UPSTREAM`. If the upstream is unhealthy, the faucet may start but claims must remain blocked until upstream-backed claim submission is verified.

The current faucet cooldown guard is stored in process memory. A service restart clears in-memory IP/account cooldown records, so this guard is sufficient for testnet faucet readiness only when paired with operator monitoring and the documented amount/cooldown limits. Do not describe it as persistent anti-abuse storage.

The service is plain HTTP. If stronger public-surface protection is required, put TLS, reverse-proxy rate limiting, access logs, or WAF controls in front of the service and record that as separate evidence.

## Readiness Verification
From outside the host:

```bash
curl --connect-timeout 8 --max-time 20 -sS http://39.104.204.172:6681/
curl --connect-timeout 8 --max-time 20 -sS http://39.104.204.172:6681/healthz
```

The root response must report the expected faucet account, `amount`, `cooldown_secs`, and `claim_path`.

`/healthz` only proves the faucet process is alive. It does not prove upstream health, faucet account balance, or claim submission viability, so `/healthz` alone cannot pass `faucet_guard_ready`.

Only after root and health pass, test a guarded claim with a fresh test account:

```bash
curl --connect-timeout 8 --max-time 20 -sS \
  -H 'content-type: application/json' \
  -d '{"account_id":"oc:pk:<64-hex-test-account>"}' \
  http://39.104.204.172:6681/claim
```

For `faucet_guard_ready=pass`, evidence must include:

- fresh public `/` and `/healthz` responses
- a successful guarded `/claim` response for a fresh test account
- a repeated claim or documented cooldown evidence proving account/IP guard behavior
- an upstream or explorer/chain-state sample showing that the claim was committed or that the faucet account state changed as expected
- the service env boundary: upstream URL, amount, cooldown, key-file usage, and no private key disclosure

## Claim Boundary
Allowed after this runbook lands:

- `repo-owned public_testnet faucet recovery package/runbook exists`
- `faucet_guard_ready recovery path is documented`
- `faucet_guard_ready remains blocked until live endpoint and guarded claim evidence pass`

Denied until fresh live verification passes:

- `public faucet is open`
- `faucet_guard_ready pass`
- `ready_for_live_candidate`
- `live public testnet is already online`
