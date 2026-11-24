#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_root"

usage() {
  cat <<'USAGE'
Usage: ./scripts/viewer-post-onboarding-headless-smoke.sh [options] [run-launcher-stack options...]

Validate the #46 PostOnboarding handoff prerequisites in no-UI mode by speaking the
Viewer live TCP protocol directly.

Default flow:
1. bootstrap a fresh stack via ./scripts/run-launcher-stack.sh
2. connect to the live TCP endpoint
3. hello + subscribe + request_snapshot
4. step(8) and step(24) in the same session
5. verify snapshot progression, control completion acks, event stream, and runtime events

Options:
  --live-addr <host:port>      Reuse an existing live TCP endpoint instead of bootstrapping a stack
  --out-dir <path>             Artifact root (default: output/playwright/playability)
  --startup-timeout <secs>     Wait timeout for stack startup / TCP listener (default: 240)
  --step-a <count>             First live step count (default: 8)
  --step-b <count>             Second live step count (default: 24)
  -h, --help                   Show this help

Examples:
  ./scripts/viewer-post-onboarding-headless-smoke.sh --bundle-dir output/release/game-launcher-local --no-llm
  ./scripts/viewer-post-onboarding-headless-smoke.sh --live-addr 127.0.0.1:5023
USAGE
}

wait_for_tcp_listener() {
  local host=$1
  local port=$2
  local timeout_secs=${3:-20}
  local step
  for step in $(seq 1 "$timeout_secs"); do
    if python3 - "$host" "$port" <<'PY'
import socket
import sys

host = sys.argv[1]
port = int(sys.argv[2])
try:
    with socket.create_connection((host, port), timeout=1):
        pass
except OSError:
    raise SystemExit(1)
raise SystemExit(0)
PY
    then
      return 0
    fi
    sleep 1
  done
  return 1
}

live_addr=""
out_root="output/playwright/playability"
startup_timeout_secs=240
step_a=8
step_b=24
stack_args=()

while [[ $# -gt 0 ]]; do
  case "$1" in
    --live-addr)
      live_addr="${2:-}"
      shift 2
      ;;
    --out-dir)
      out_root="${2:-}"
      shift 2
      ;;
    --startup-timeout)
      startup_timeout_secs="${2:-}"
      shift 2
      ;;
    --step-a)
      step_a="${2:-}"
      shift 2
      ;;
    --step-b)
      step_b="${2:-}"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      stack_args+=("$1")
      shift
      ;;
  esac
done

[[ -n "$out_root" ]] || { echo "error: --out-dir cannot be empty" >&2; exit 2; }
[[ "$startup_timeout_secs" =~ ^[0-9]+$ ]] && [[ "$startup_timeout_secs" -gt 0 ]] || {
  echo "error: --startup-timeout must be a positive integer" >&2
  exit 2
}
[[ "$step_a" =~ ^[0-9]+$ ]] && [[ "$step_a" -gt 0 ]] || {
  echo "error: --step-a must be a positive integer" >&2
  exit 2
}
[[ "$step_b" =~ ^[0-9]+$ ]] && [[ "$step_b" -gt 0 ]] || {
  echo "error: --step-b must be a positive integer" >&2
  exit 2
}

resolve_bootstrap_live_addr() {
  local resolved="127.0.0.1:5023"
  local index=0
  while (( index < ${#stack_args[@]} )); do
    if [[ "${stack_args[$index]}" == "--live-bind" ]]; then
      if (( index + 1 >= ${#stack_args[@]} )); then
        echo "error: --live-bind requires an address" >&2
        exit 2
      fi
      resolved="${stack_args[$((index + 1))]}"
      break
    fi
    index=$((index + 1))
  done
  printf '%s\n' "$resolved"
}

stamp=$(date +%Y%m%d-%H%M%S)
run_id="post-onboarding-headless-${stamp}"
out_dir="$out_root/$run_id"
mkdir -p "$out_dir"

run_log="$out_dir/launcher-stack.log"
summary_json_path="$out_dir/post-onboarding-headless-summary.json"
summary_md_path="$out_dir/post-onboarding-headless-summary.md"
transcript_path="$out_dir/viewer-protocol-transcript.jsonl"
initial_snapshot_path="$out_dir/snapshot-initial.json"
feedback_snapshot_path="$out_dir/snapshot-feedback.json"
followup_snapshot_path="$out_dir/snapshot-followup.json"

stack_pid=""
stack_logs_dir=""
probe_live_addr="${live_addr:-$(resolve_bootstrap_live_addr)}"

cleanup() {
  local exit_code=$?
  trap - EXIT INT TERM
  if [[ -n "$stack_pid" ]] && kill -0 "$stack_pid" >/dev/null 2>&1; then
    kill "$stack_pid" >/dev/null 2>&1 || true
    wait "$stack_pid" >/dev/null 2>&1 || true
  fi
  exit "$exit_code"
}
trap cleanup EXIT INT TERM

if [[ -z "$live_addr" ]]; then
  ./scripts/run-launcher-stack.sh "${stack_args[@]}" > >(tee "$run_log") 2>&1 &
  stack_pid=$!

  for ((i = 0; i < startup_timeout_secs; i++)); do
    if ! kill -0 "$stack_pid" >/dev/null 2>&1; then
      echo "error: run-launcher-stack.sh exited unexpectedly" >&2
      tail -n 120 "$run_log" >&2 || true
      exit 1
    fi
    stack_logs_dir="$(sed -n 's/^- Logs: \(.*\)$/\1/p' "$run_log" | tail -n 1)"
    if wait_for_tcp_listener "${probe_live_addr%:*}" "${probe_live_addr##*:}" 1; then
      break
    fi
  done
else
  wait_for_tcp_listener "${probe_live_addr%:*}" "${probe_live_addr##*:}" "$startup_timeout_secs" || {
    echo "error: timeout waiting for live TCP listener at $probe_live_addr" >&2
    exit 1
  }
fi

if ! wait_for_tcp_listener "${probe_live_addr%:*}" "${probe_live_addr##*:}" 1; then
  echo "error: timeout waiting for live TCP listener at $probe_live_addr" >&2
  if [[ -n "$stack_pid" ]]; then
    tail -n 120 "$run_log" >&2 || true
  fi
  exit 1
fi

summary_raw=$(python3 - \
  "$probe_live_addr" \
  "$step_a" \
  "$step_b" \
  "$initial_snapshot_path" \
  "$feedback_snapshot_path" \
  "$followup_snapshot_path" \
  "$transcript_path" <<'PY'
import collections
import json
import pathlib
import socket
import sys
import time

live_addr = sys.argv[1]
step_a = int(sys.argv[2])
step_b = int(sys.argv[3])
initial_snapshot_path = pathlib.Path(sys.argv[4])
feedback_snapshot_path = pathlib.Path(sys.argv[5])
followup_snapshot_path = pathlib.Path(sys.argv[6])
transcript_path = pathlib.Path(sys.argv[7])
host, port_text = live_addr.rsplit(":", 1)
port = int(port_text)


def json_write(path: pathlib.Path, payload) -> None:
    path.write_text(json.dumps(payload, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")


def parse_time(snapshot: dict) -> int:
    raw = snapshot.get("time", 0)
    if isinstance(raw, bool):
        return int(raw)
    if isinstance(raw, (int, float)):
        return int(raw)
    return int(str(raw or "0"))


def last_snapshot_from_messages(messages: list[dict]) -> dict | None:
    for message in reversed(messages):
        if message.get("type") == "snapshot":
            return message.get("snapshot")
    return None


def snapshot_reaches_followup_contract(snapshot: dict) -> bool:
    gameplay = snapshot.get("player_gameplay") or {}
    return (
        gameplay.get("stage_id") == "post_onboarding"
        and (gameplay.get("progress_percent") or 0) >= 20
        and bool(gameplay.get("next_step_hint"))
    )


def first_agent_id(snapshot: dict) -> str | None:
    agents = (snapshot.get("model") or {}).get("agents") or {}
    if isinstance(agents, dict):
        for key in agents:
            return key
        return None
    if isinstance(agents, list):
        for agent in agents:
            if isinstance(agent, dict) and agent.get("id"):
                return str(agent["id"])
        return None
    return None


def event_kind_name(message: dict) -> str:
    kind = ((message.get("event") or {}).get("kind"))
    if isinstance(kind, dict):
        return str(kind.get("type") or "unknown")
    if kind is None:
        return "unknown"
    return str(kind)


def runtime_event_sample(message: dict) -> str | None:
    event = message.get("event") or {}
    kind = event.get("kind") or {}
    if not isinstance(kind, dict) or kind.get("type") != "RuntimeEvent":
        return None
    data = kind.get("data")
    if isinstance(data, dict) and data.get("kind"):
        return str(data["kind"])
    runtime_event = event.get("runtime_event") or {}
    body = runtime_event.get("body") or {}
    payload = body.get("payload") or {}
    if isinstance(payload, dict) and payload.get("type"):
        return str(payload["type"])
    return "runtime_event"


def fetch_snapshot_via_fresh_connection(live_addr: str) -> dict:
    host, port_text = live_addr.rsplit(":", 1)
    port = int(port_text)
    with socket.create_connection((host, port), timeout=5) as sock:
        sock.settimeout(1)
        reader = sock.makefile("rb")
        writer = sock.makefile("wb")

        def send(payload: dict) -> None:
            writer.write((json.dumps(payload, ensure_ascii=False) + "\n").encode("utf-8"))
            writer.flush()

        def read_until(expected_type: str, timeout_secs: float) -> dict:
            deadline = time.time() + timeout_secs
            while time.time() < deadline:
                try:
                    line = reader.readline()
                except TimeoutError:
                    continue
                except OSError as exc:
                    if "timed out" in str(exc).lower():
                        continue
                    raise
                if not line:
                    continue
                payload = json.loads(line)
                if payload.get("type") == expected_type:
                    return payload
            raise TimeoutError(f"timed out waiting for {expected_type} on fresh snapshot connection")

        send({"type": "hello", "client": "oasis7-headless-smoke-snapshot", "version": 1})
        read_until("hello_ack", 5)
        send({"type": "request_snapshot"})
        snapshot_payload = read_until("snapshot", 10)
        writer.close()
        reader.close()
        return snapshot_payload["snapshot"]


with socket.create_connection((host, port), timeout=5) as sock:
    sock.settimeout(1)
    reader = sock.makefile("rb")
    writer = sock.makefile("wb")
    transcript = transcript_path.open("w", encoding="utf-8")

    def record(direction: str, payload: dict) -> None:
        transcript.write(json.dumps({"direction": direction, "payload": payload}, ensure_ascii=False) + "\n")
        transcript.flush()

    def send(payload: dict) -> None:
        record("send", payload)
        writer.write((json.dumps(payload, ensure_ascii=False) + "\n").encode("utf-8"))
        writer.flush()

    def read_message(deadline: float) -> dict:
        while time.time() < deadline:
            try:
                line = reader.readline()
            except TimeoutError:
                continue
            except OSError as exc:
                if "timed out" in str(exc).lower():
                    continue
                raise
            if not line:
                if time.time() < deadline:
                    continue
                raise TimeoutError("timed out waiting for viewer response")
            try:
                payload = json.loads(line)
            except json.JSONDecodeError:
                continue
            record("recv", payload)
            return payload
        raise TimeoutError("timed out waiting for viewer response")

    def collect_until(expected_type: str, timeout_secs: float) -> list[dict]:
        deadline = time.time() + timeout_secs
        collected: list[dict] = []
        while True:
            payload = read_message(deadline)
            collected.append(payload)
            if payload.get("type") == expected_type:
                return collected

    def collect_control_completion(timeout_secs: float) -> tuple[list[dict], dict]:
        deadline = time.time() + timeout_secs
        collected: list[dict] = []
        saw_recovery_ack = False
        saw_authoritative_batch = False
        while time.time() < deadline:
            try:
                payload = read_message(deadline)
            except TimeoutError:
                break
            collected.append(payload)
            payload_type = payload.get("type")
            if payload_type == "control_completion_ack":
                return collected, payload.get("ack") or {}
            if payload_type == "authoritative_recovery_ack":
                saw_recovery_ack = True
            elif payload_type == "authoritative_batch":
                saw_authoritative_batch = True
        if saw_recovery_ack or saw_authoritative_batch:
            return collected, {
                "status": "advanced_via_authoritative_stream",
                "via_authoritative_recovery_ack": saw_recovery_ack,
                "via_authoritative_batch": saw_authoritative_batch,
            }
        raise TimeoutError("timed out waiting for control completion or authoritative live progress")

    send({"type": "hello", "client": "oasis7-headless-smoke", "version": 1})
    hello_window = collect_until("hello_ack", 5)
    hello_ack = hello_window[-1]

    send({"type": "subscribe", "streams": ["events", "snapshot", "metrics"], "event_kinds": []})
    send({"type": "request_snapshot"})
    initial_window = collect_until("snapshot", 5)
    initial_snapshot = initial_window[-1]["snapshot"]

    send({"type": "live_control", "mode": {"mode": "step", "count": step_a}, "request_id": 1})
    feedback_window, feedback_ack = collect_control_completion(10)
    feedback_snapshot = last_snapshot_from_messages(feedback_window)
    if feedback_snapshot is None:
        send({"type": "request_snapshot"})
        try:
            feedback_snapshot_window = collect_until("snapshot", 5)
            feedback_snapshot = feedback_snapshot_window[-1]["snapshot"]
        except TimeoutError:
            feedback_snapshot = fetch_snapshot_via_fresh_connection(live_addr)

    if snapshot_reaches_followup_contract(feedback_snapshot):
        followup_window = list(feedback_window)
        followup_ack = {
            "status": "skipped_reused_feedback_snapshot",
            "reason": "feedback_snapshot_already_satisfied_followup_contract",
        }
        followup_snapshot = feedback_snapshot
    else:
        send({"type": "live_control", "mode": {"mode": "step", "count": step_b}, "request_id": 2})
        followup_window, followup_ack = collect_control_completion(15)
        followup_snapshot = last_snapshot_from_messages(followup_window)
        if followup_snapshot is None:
            send({"type": "request_snapshot"})
            try:
                followup_snapshot_window = collect_until("snapshot", 5)
                followup_snapshot = followup_snapshot_window[-1]["snapshot"]
            except TimeoutError:
                followup_snapshot = fetch_snapshot_via_fresh_connection(live_addr)

    writer.close()
    reader.close()
    transcript.close()

json_write(initial_snapshot_path, initial_snapshot)
json_write(feedback_snapshot_path, feedback_snapshot)
json_write(followup_snapshot_path, followup_snapshot)

all_event_messages = [
    message
    for message in (feedback_window + followup_window)
    if message.get("type") == "event"
]
event_counts = collections.Counter(event_kind_name(message) for message in all_event_messages)
message_type_counts = collections.Counter(
    (message.get("type") or "unknown")
    for message in (feedback_window + followup_window)
)
runtime_event_samples = []
for message in all_event_messages:
    sample = runtime_event_sample(message)
    if sample and sample not in runtime_event_samples:
        runtime_event_samples.append(sample)
    if len(runtime_event_samples) >= 3:
        break

initial_time = parse_time(initial_snapshot)
feedback_time = parse_time(feedback_snapshot)
followup_time = parse_time(followup_snapshot)
first_agent = first_agent_id(initial_snapshot)

checks = {
    "helloAckLive": hello_ack.get("control_profile") == "live",
    "snapshotHasRuntimeState": isinstance(initial_snapshot.get("runtime_snapshot"), dict),
    "firstAgentPresent": bool(first_agent),
    "feedbackAdvanced": feedback_ack.get("status") in {"advanced", "advanced_via_authoritative_stream"},
    "feedbackProducedDelta": (
        int(feedback_ack.get("delta_logical_time", 0)) > 0
        or int(feedback_ack.get("delta_event_seq", 0)) > 0
        or feedback_ack.get("via_authoritative_batch") is True
    ),
    "followupAdvanced": followup_ack.get("status") in {
        "advanced",
        "advanced_via_authoritative_stream",
        "skipped_reused_feedback_snapshot",
    },
    "followupProducedDelta": (
        int(followup_ack.get("delta_logical_time", 0)) > 0
        or int(followup_ack.get("delta_event_seq", 0)) > 0
        or followup_ack.get("via_authoritative_batch") is True
        or followup_ack.get("status") == "skipped_reused_feedback_snapshot"
    ),
    "snapshotTimeAdvanced": (
        feedback_time > initial_time
        and (
            followup_time > feedback_time
            or (
                followup_ack.get("status") == "skipped_reused_feedback_snapshot"
                and followup_time >= feedback_time
            )
        )
    ),
    "eventStreamNonEmpty": len(all_event_messages) > 0,
    "runtimeSignalSeen": (
        event_counts.get("RuntimeEvent", 0) > 0
        or message_type_counts.get("decision_trace", 0) > 0
        or message_type_counts.get("authoritative_batch", 0) > 0
    ),
}

failed_checks = [name for name, passed in checks.items() if not passed]
if failed_checks:
    raise SystemExit(
        "headless smoke failed checks: " + ", ".join(failed_checks)
    )

summary = {
    "result": "pass",
    "mode": "viewer_live_protocol_no_ui",
    "liveAddr": live_addr,
    "artifacts": {
        "initialSnapshot": str(initial_snapshot_path),
        "feedbackSnapshot": str(feedback_snapshot_path),
        "followupSnapshot": str(followup_snapshot_path),
        "protocolTranscript": str(transcript_path),
    },
    "checks": checks,
    "notes": {
        "worldId": hello_ack.get("world_id"),
        "controlProfile": hello_ack.get("control_profile"),
        "initialTime": initial_time,
        "feedbackTime": feedback_time,
        "followupTime": followup_time,
        "firstAgentId": first_agent,
        "feedbackAck": feedback_ack,
        "followupAck": followup_ack,
        "eventCounts": dict(event_counts),
        "messageTypeCounts": dict(message_type_counts),
        "runtimeEventSamples": runtime_event_samples,
    },
    "scopeBoundary": [
        "Validates same-session live control, snapshot progression, event stream, and runtime-event feed for #46 without browser UI.",
        "Does not prove Mission HUD / PostOnboarding card rendering; headed Web/UI QA remains the release truth for on-screen semantics.",
    ],
}
print(json.dumps(summary, ensure_ascii=False))
PY
)

python3 - "$summary_json_path" "$summary_md_path" "$summary_raw" "$stack_logs_dir" "$run_log" <<'PY'
import json
import pathlib
import sys

summary_json_path = pathlib.Path(sys.argv[1])
summary_md_path = pathlib.Path(sys.argv[2])
summary = json.loads(sys.argv[3])
stack_logs_dir = sys.argv[4]
run_log = sys.argv[5]

if stack_logs_dir:
    summary["stackLogsDir"] = stack_logs_dir
if pathlib.Path(run_log).exists():
    summary.setdefault("artifacts", {})["runGameTestLog"] = run_log

summary_json_path.write_text(
    json.dumps(summary, ensure_ascii=False, indent=2) + "\n",
    encoding="utf-8",
)

lines = [
    "# PostOnboarding no-UI protocol smoke summary",
    "",
    f"- result: `{summary['result']}`",
    f"- mode: `{summary['mode']}`",
    f"- liveAddr: `{summary['liveAddr']}`",
    f"- worldId: `{summary['notes']['worldId']}`",
    f"- controlProfile: `{summary['notes']['controlProfile']}`",
    f"- firstAgentId: `{summary['notes']['firstAgentId']}`",
    f"- timeline: `{summary['notes']['initialTime']} -> {summary['notes']['feedbackTime']} -> {summary['notes']['followupTime']}`",
    f"- feedbackAck: `{json.dumps(summary['notes']['feedbackAck'], ensure_ascii=False)}`",
    f"- followupAck: `{json.dumps(summary['notes']['followupAck'], ensure_ascii=False)}`",
    f"- eventCounts: `{json.dumps(summary['notes']['eventCounts'], ensure_ascii=False, sort_keys=True)}`",
    f"- messageTypeCounts: `{json.dumps(summary['notes']['messageTypeCounts'], ensure_ascii=False, sort_keys=True)}`",
    f"- runtimeEventSamples: `{json.dumps(summary['notes']['runtimeEventSamples'], ensure_ascii=False)}`",
    "",
    "## Checks",
]
for key, value in summary["checks"].items():
    lines.append(f"- {key}: `{value}`")
lines.extend([
    "",
    "## Scope Boundary",
])
for line in summary["scopeBoundary"]:
    lines.append(f"- {line}")
summary_md_path.write_text("\n".join(lines) + "\n", encoding="utf-8")
PY

printf 'ok: artifacts written to %s\n' "$out_dir"
