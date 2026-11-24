#!/usr/bin/env bash

wh_require_git_worktree() {
  if ! git rev-parse --is-inside-work-tree >/dev/null 2>&1; then
    echo "error: worktree harness must run inside a git worktree" >&2
    exit 1
  fi
}

wh_repo_root() {
  git rev-parse --show-toplevel
}

wh_git_head() {
  git rev-parse HEAD
}

wh_worktree_path() {
  pwd -P
}

wh_git_common_dir() {
  local repo_root common_dir
  repo_root=$(git rev-parse --show-toplevel) || return 1
  common_dir=$(git rev-parse --git-common-dir) || return 1
  if [[ "$common_dir" == /* ]]; then
    (cd "$common_dir" && pwd -P)
  else
    (cd "$repo_root/$common_dir" && pwd -P)
  fi
}

wh_worktree_id() {
  python3 - "$(wh_worktree_path)" <<'PY'
import hashlib
import pathlib
import sys

path = pathlib.Path(sys.argv[1]).resolve()
digest = hashlib.sha256(str(path).encode("utf-8")).hexdigest()[:8]
print(f"wt-{digest}")
PY
}

wh_harness_root() {
  local repo_root=$1
  local worktree_id=$2
  # Tests may opt into an explicitly marked disposable root; production keeps
  # the worktree-scoped output path below and never consults this override.
  if [[ -n "${OASIS7_HARNESS_TEST_ROOT:-}" ]]; then
    local test_root=${OASIS7_HARNESS_TEST_ROOT}
    local test_root_parent test_root_marker marker_value
    [[ "$test_root" == /* ]] || {
      echo "error: OASIS7_HARNESS_TEST_ROOT must be an absolute path" >&2
      return 1
    }
    test_root_parent="$(dirname "$test_root")"
    test_root_parent="$(cd "$test_root_parent" 2>/dev/null && pwd -P)" || {
      echo "error: OASIS7_HARNESS_TEST_ROOT parent does not exist: $test_root_parent" >&2
      return 1
    }
    test_root="$test_root_parent/$(basename "$test_root")"
    [[ "$(basename "$test_root")" == "harness" ]] || {
      echo "error: OASIS7_HARNESS_TEST_ROOT must name a disposable harness child" >&2
      return 1
    }
    test_root_marker="$test_root_parent/.oasis7-harness-test-root"
    [[ -f "$test_root_marker" ]] || {
      echo "error: OASIS7_HARNESS_TEST_ROOT is not an owned disposable root" >&2
      return 1
    }
    marker_value="$(tr -d '\n' <"$test_root_marker")"
    [[ "$marker_value" == "oasis7-harness-lifecycle-test-v1" ]] || {
      echo "error: OASIS7_HARNESS_TEST_ROOT ownership marker is invalid" >&2
      return 1
    }
    printf '%s\n' "$test_root"
    return 0
  fi
  printf '%s/output/harness/%s\n' "$repo_root" "$worktree_id"
}

wh_runtime_dir() {
  local harness_root=$1
  printf '%s/runtime\n' "$harness_root"
}

wh_artifacts_dir() {
  local harness_root=$1
  printf '%s/artifacts\n' "$harness_root"
}

wh_browser_dir() {
  local harness_root=$1
  printf '%s/browser\n' "$harness_root"
}

wh_bundle_root() {
  local harness_root=$1
  printf '%s/bundle\n' "$harness_root"
}

wh_default_bundle_dir() {
  local harness_root=$1
  printf '%s/game-launcher-local\n' "$(wh_bundle_root "$harness_root")"
}

wh_default_producer_bundle_dir() {
  local harness_root=$1
  printf '%s/game-launcher-producer-local\n' "$(wh_bundle_root "$harness_root")"
}

wh_state_file() {
  local harness_root=$1
  printf '%s/state.json\n' "$harness_root"
}

wh_startup_log() {
  local harness_root=$1
  printf '%s/startup.log\n' "$harness_root"
}

wh_runtime_meta_file() {
  local harness_root=$1
  printf '%s/session.meta\n' "$(wh_runtime_dir "$harness_root")"
}

wh_browser_session() {
  local worktree_id=$1
  printf '%s\n' "$worktree_id"
}

wh_prepare_dirs() {
  local harness_root=$1
  mkdir -p "$harness_root" "$(wh_runtime_dir "$harness_root")" "$(wh_artifacts_dir "$harness_root")" "$(wh_browser_dir "$harness_root")" "$(wh_bundle_root "$harness_root")"
}

# Publish a complete record with a same-directory temporary file and an
# atomic replacement.  Readers that do not take the lock still see either the
# old complete record or the new complete record, never a partially-written
# file.  The lock is deliberately kept beside the record so concurrent
# read/modify/write callers do not lose updates while they are assembling a
# new state snapshot.
wh_atomic_write() {
  local destination=$1
  local contents=${2-}
  mkdir -p "$(dirname "$destination")"
  python3 - "$destination" "$contents" <<'PY'
from __future__ import annotations

import fcntl
import os
import pathlib
import tempfile
import sys

destination = pathlib.Path(sys.argv[1])
contents = sys.argv[2]
destination.parent.mkdir(parents=True, exist_ok=True)
lock_path = destination.with_name(f".{destination.name}.lock")

with lock_path.open("a+", encoding="utf-8") as lock:
    fcntl.flock(lock.fileno(), fcntl.LOCK_EX)
    mode = destination.stat().st_mode & 0o777 if destination.exists() else 0o644
    fd, temporary_name = tempfile.mkstemp(
        prefix=f".{destination.name}.",
        dir=str(destination.parent),
    )
    temporary_path = pathlib.Path(temporary_name)
    try:
        with os.fdopen(fd, "w", encoding="utf-8") as temporary:
            temporary.write(contents)
            temporary.flush()
            os.fsync(temporary.fileno())
        os.chmod(temporary_path, mode)
        os.replace(temporary_path, destination)
        directory_flags = os.O_RDONLY | getattr(os, "O_DIRECTORY", 0)
        directory_fd = os.open(destination.parent, directory_flags)
        try:
            os.fsync(directory_fd)
        finally:
            os.close(directory_fd)
    finally:
        temporary_path.unlink(missing_ok=True)
PY
}

wh_resolve_ports_json() {
  local harness_root=${1:-}
  local owner_pid=${2:-$$}
  local worktree_path=${3:-$(wh_worktree_path)}
  local common_dir=${4:-$(wh_git_common_dir)}
  [[ -n "$harness_root" ]] || {
    echo "error: wh_resolve_ports_json requires a harness root" >&2
    return 2
  }
  [[ -n "$common_dir" ]] || {
    echo "error: wh_resolve_ports_json requires a git common directory" >&2
    return 2
  }
  mkdir -p "$harness_root"
  python3 - "$harness_root" "$owner_pid" "$worktree_path" "$common_dir" <<'PY'
from __future__ import annotations

import fcntl
import ctypes
import hashlib
import json
import os
import socket
import subprocess
import sys
import tempfile
import uuid
from pathlib import Path

harness_root = Path(sys.argv[1]).resolve()
owner_pid = int(sys.argv[2]) if sys.argv[2] else os.getppid()
worktree_path = str(Path(sys.argv[3]).resolve())
common_dir = Path(sys.argv[4]).resolve()
seed = int(hashlib.sha256(worktree_path.encode("utf-8")).hexdigest()[:8], 16)
start = 43000 + (seed % 1500) * 10
reservation_path = harness_root / ".ports.reservation.json"
local_lock_path = harness_root / ".ports.lock"
registry_dir = common_dir / ".oasis7-harness-port-registry"
registry_path = registry_dir / "reservations.json"
registry_lock_path = registry_dir / "registry.lock"
registry_dir.mkdir(parents=True, exist_ok=True)


def pid_alive(pid: int) -> bool:
    if pid <= 1:
        return False
    try:
        os.kill(pid, 0)
    except OSError:
        return False
    proc_stat = Path(f"/proc/{pid}/stat")
    try:
        contents = proc_stat.read_text(encoding="utf-8")
    except OSError:
        contents = ""
    if contents:
        right_paren = contents.rfind(") ")
        if right_paren >= 0:
            fields = contents[right_paren + 2 :].split()
            if fields and fields[0].startswith("Z"):
                return False
    else:
        try:
            result = subprocess.run(
                ["ps", "-o", "stat=", "-p", str(pid)],
                check=False,
                capture_output=True,
                text=True,
            )
        except OSError:
            result = None
        if result is not None:
            status = result.stdout.strip().split(maxsplit=1)
            if status and status[0].startswith("Z"):
                return False
    return True


def darwin_process_identity(pid: int) -> str | None:
    if sys.platform != "darwin":
        return None

    class ProcBsdInfo(ctypes.Structure):
        _fields_ = [
            ("pbi_flags", ctypes.c_uint32),
            ("pbi_status", ctypes.c_uint32),
            ("pbi_xstatus", ctypes.c_uint32),
            ("pbi_pid", ctypes.c_uint32),
            ("pbi_ppid", ctypes.c_uint32),
            ("pbi_uid", ctypes.c_uint32),
            ("pbi_gid", ctypes.c_uint32),
            ("pbi_ruid", ctypes.c_uint32),
            ("pbi_rgid", ctypes.c_uint32),
            ("pbi_svuid", ctypes.c_uint32),
            ("pbi_svgid", ctypes.c_uint32),
            ("rfu_1", ctypes.c_uint32),
            ("pbi_comm", ctypes.c_char * 16),
            ("pbi_name", ctypes.c_char * 32),
            ("pbi_nfiles", ctypes.c_uint32),
            ("pbi_pgid", ctypes.c_uint32),
            ("pbi_pjobc", ctypes.c_uint32),
            ("e_tdev", ctypes.c_uint32),
            ("e_tpgid", ctypes.c_uint32),
            ("pbi_nice", ctypes.c_int32),
            ("pbi_start_tvsec", ctypes.c_uint64),
            ("pbi_start_tvusec", ctypes.c_uint64),
        ]

    try:
        libproc = ctypes.CDLL("/usr/lib/libproc.dylib")
        proc_pidinfo = libproc.proc_pidinfo
        proc_pidinfo.argtypes = [
            ctypes.c_int,
            ctypes.c_int,
            ctypes.c_uint64,
            ctypes.c_void_p,
            ctypes.c_int,
        ]
        proc_pidinfo.restype = ctypes.c_int
        info = ProcBsdInfo()
        size = ctypes.sizeof(info)
        if proc_pidinfo(pid, 3, 0, ctypes.byref(info), size) != size:
            return None
        if info.pbi_pid != pid:
            return None
        return f"mac-proc-start:{info.pbi_start_tvsec}:{info.pbi_start_tvusec}"
    except (AttributeError, OSError, TypeError, ValueError):
        return None


def process_identity(pid: int) -> str | None:
    if pid <= 1:
        return None
    proc_stat = Path(f"/proc/{pid}/stat")
    try:
        contents = proc_stat.read_text(encoding="utf-8")
    except OSError:
        contents = ""
    if contents:
        right_paren = contents.rfind(") ")
        if right_paren >= 0:
            fields = contents[right_paren + 2 :].split()
            if len(fields) > 19:
                boot_id_path = Path("/proc/sys/kernel/random/boot_id")
                try:
                    boot_id = boot_id_path.read_text(encoding="utf-8").strip()
                except OSError:
                    boot_id = ""
                if boot_id:
                    return f"proc-starttime:{boot_id}:{fields[19]}"
                return None
    if sys.platform == "darwin":
        return darwin_process_identity(pid)
    return None


def reservation_owner_status(record: dict) -> str:
    try:
        pid = int(record.get("owner_pid", 0))
    except (TypeError, ValueError):
        return "unknown"
    if pid <= 1:
        return "unknown"
    if not pid_alive(pid):
        return "stale"
    expected_identity = record.get("owner_identity")
    if not isinstance(expected_identity, str) or not expected_identity:
        return "unknown"
    # Linux records written before boot_id was added contain only the
    # monotonic start-time tick.  That value is not comparable across boots,
    # so a live legacy owner must remain reserved rather than being treated
    # as a different incarnation and reclaimed.  A dead legacy owner is
    # already safely classified as stale above.
    if (
        expected_identity.startswith("ps-start:")
        or (
            expected_identity.startswith("proc-starttime:")
            and expected_identity.count(":") == 1
        )
    ):
        return "unknown"
    current_identity = process_identity(pid)
    if not current_identity:
        return "unknown"
    return "live" if current_identity == expected_identity else "stale"


def atomic_write(path: Path, contents: str) -> None:
    fd, temporary_name = tempfile.mkstemp(prefix=f".{path.name}.", dir=str(path.parent))
    temporary_path = Path(temporary_name)
    try:
        with os.fdopen(fd, "w", encoding="utf-8") as temporary:
            temporary.write(contents)
            temporary.flush()
            os.fsync(temporary.fileno())
        os.replace(temporary_path, path)
        directory_flags = os.O_RDONLY | getattr(os, "O_DIRECTORY", 0)
        directory_fd = os.open(path.parent, directory_flags)
        try:
            os.fsync(directory_fd)
        finally:
            os.close(directory_fd)
    finally:
        temporary_path.unlink(missing_ok=True)


def load_registry() -> dict:
    if not registry_path.exists():
        return {"schema": 1, "reservations": {}}
    try:
        registry = json.loads(registry_path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as exc:
        raise SystemExit(f"error: invalid shared harness port registry: {exc}")
    if registry.get("schema") != 1 or not isinstance(registry.get("reservations"), dict):
        raise SystemExit("error: unsupported shared harness port registry schema")
    return registry


def save_registry(registry: dict) -> None:
    atomic_write(registry_path, json.dumps(registry, sort_keys=True) + "\n")


def reservation_ports(reservation: dict) -> set[int]:
    ports = reservation.get("ports", {})

    def port_value(value: object) -> int:
        if isinstance(value, str):
            return int(value.rsplit(":", 1)[-1])
        return int(value)

    return {
        port_value(ports[key])
        for key in ("viewer_port", "web_bind", "live_bind", "chain_status_bind")
    }


def free(port: int) -> bool:
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as sock:
        sock.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        try:
            sock.bind(("127.0.0.1", port))
        except OSError:
            return False
    return True


with local_lock_path.open("a+", encoding="utf-8") as local_lock:
    fcntl.flock(local_lock.fileno(), fcntl.LOCK_EX)
    with registry_lock_path.open("a+", encoding="utf-8") as registry_lock:
        fcntl.flock(registry_lock.fileno(), fcntl.LOCK_EX)
        if reservation_path.exists():
            try:
                local_reservation = json.loads(reservation_path.read_text(encoding="utf-8"))
            except (OSError, json.JSONDecodeError) as exc:
                raise SystemExit(f"error: invalid harness port reservation: {exc}")
            local_owner_status = reservation_owner_status(local_reservation)
            if local_owner_status == "live":
                raise SystemExit(
                    f"error: harness ports are already reserved by live owner {local_reservation.get('owner_pid')}"
                )
            if local_owner_status == "unknown":
                raise SystemExit(
                    "error: harness port reservation owner identity is unavailable; reservation retained"
                )
            reservation_path.unlink()

        registry = load_registry()
        reservations = registry["reservations"]
        for token, record in list(reservations.items()):
            owner_status = reservation_owner_status(record)
            if owner_status == "stale":
                del reservations[token]
            elif owner_status == "unknown":
                raise SystemExit(
                    f"error: shared harness port reservation {token} has a live owner with unavailable identity; reservation retained"
                )

        for token, record in reservations.items():
            if record.get("harness_root") == str(harness_root) and reservation_owner_status(record) == "live":
                raise SystemExit(
                    f"error: harness ports are already reserved by live owner {record.get('owner_pid')}"
                )

        owner_identity = process_identity(owner_pid)
        if not owner_identity:
            raise SystemExit(
                f"error: unable to capture stable identity for reservation owner {owner_pid}"
            )

        reserved_ports = set()
        for record in reservations.values():
            reserved_ports.update(reservation_ports(record))

        for step in range(1500):
            base = start + step * 10
            ports = [base, base + 1, base + 2, base + 3]
            if ports[-1] > 65000:
                continue
            if set(ports) & reserved_ports:
                continue
            if all(free(port) for port in ports):
                token = uuid.uuid4().hex
                payload = {
                    "viewer_port": ports[0],
                    "web_bind": f"127.0.0.1:{ports[1]}",
                    "live_bind": f"127.0.0.1:{ports[2]}",
                    "chain_status_bind": f"127.0.0.1:{ports[3]}",
                }
                reservation = {
                    "schema": 1,
                    "reservation_token": token,
                    "owner_pid": owner_pid,
                    "owner_identity": owner_identity,
                    "worktree_path": worktree_path,
                    "harness_root": str(harness_root),
                    "common_dir": str(common_dir),
                    "registry_path": str(registry_path),
                    "ports": payload,
                }
                reservations[token] = reservation
                save_registry(registry)
                atomic_write(reservation_path, json.dumps(reservation, sort_keys=True) + "\n")
                payload["reservation_token"] = token
                payload["reservation_file"] = str(reservation_path)
                payload["registry_file"] = str(registry_path)
                print(json.dumps(payload, ensure_ascii=True))
                break
        else:
            raise SystemExit("error: unable to allocate free loopback ports for worktree harness")
PY
}

wh_bind_ports_owner() {
  local harness_root=$1
  local reservation_token=$2
  local old_owner_pid=$3
  local new_owner_pid=$4
  local common_dir=${5:-}
  python3 - "$harness_root" "$reservation_token" "$old_owner_pid" "$new_owner_pid" "$common_dir" <<'PY'
from __future__ import annotations

import fcntl
import ctypes
import json
import os
import pathlib
import subprocess
import tempfile
import sys

harness_root = pathlib.Path(sys.argv[1]).resolve()
expected_token = sys.argv[2]
old_owner_pid = int(sys.argv[3]) if sys.argv[3] else os.getppid()
new_owner_pid = int(sys.argv[4])
common_dir = pathlib.Path(sys.argv[5]).resolve() if sys.argv[5] else None
reservation_path = harness_root / ".ports.reservation.json"


def pid_alive(pid: int) -> bool:
    if pid <= 1:
        return False
    try:
        os.kill(pid, 0)
    except OSError:
        return False
    proc_stat = pathlib.Path(f"/proc/{pid}/stat")
    try:
        contents = proc_stat.read_text(encoding="utf-8")
    except OSError:
        contents = ""
    if contents:
        right_paren = contents.rfind(") ")
        if right_paren >= 0:
            fields = contents[right_paren + 2 :].split()
            if fields and fields[0].startswith("Z"):
                return False
    else:
        try:
            result = subprocess.run(
                ["ps", "-o", "stat=", "-p", str(pid)],
                check=False,
                capture_output=True,
                text=True,
            )
        except OSError:
            result = None
        if result is not None:
            status = result.stdout.strip().split(maxsplit=1)
            if status and status[0].startswith("Z"):
                return False
    return True


def darwin_process_identity(pid: int) -> str | None:
    if sys.platform != "darwin":
        return None

    class ProcBsdInfo(ctypes.Structure):
        _fields_ = [
            ("pbi_flags", ctypes.c_uint32),
            ("pbi_status", ctypes.c_uint32),
            ("pbi_xstatus", ctypes.c_uint32),
            ("pbi_pid", ctypes.c_uint32),
            ("pbi_ppid", ctypes.c_uint32),
            ("pbi_uid", ctypes.c_uint32),
            ("pbi_gid", ctypes.c_uint32),
            ("pbi_ruid", ctypes.c_uint32),
            ("pbi_rgid", ctypes.c_uint32),
            ("pbi_svuid", ctypes.c_uint32),
            ("pbi_svgid", ctypes.c_uint32),
            ("rfu_1", ctypes.c_uint32),
            ("pbi_comm", ctypes.c_char * 16),
            ("pbi_name", ctypes.c_char * 32),
            ("pbi_nfiles", ctypes.c_uint32),
            ("pbi_pgid", ctypes.c_uint32),
            ("pbi_pjobc", ctypes.c_uint32),
            ("e_tdev", ctypes.c_uint32),
            ("e_tpgid", ctypes.c_uint32),
            ("pbi_nice", ctypes.c_int32),
            ("pbi_start_tvsec", ctypes.c_uint64),
            ("pbi_start_tvusec", ctypes.c_uint64),
        ]

    try:
        libproc = ctypes.CDLL("/usr/lib/libproc.dylib")
        proc_pidinfo = libproc.proc_pidinfo
        proc_pidinfo.argtypes = [
            ctypes.c_int,
            ctypes.c_int,
            ctypes.c_uint64,
            ctypes.c_void_p,
            ctypes.c_int,
        ]
        proc_pidinfo.restype = ctypes.c_int
        info = ProcBsdInfo()
        size = ctypes.sizeof(info)
        if proc_pidinfo(pid, 3, 0, ctypes.byref(info), size) != size:
            return None
        if info.pbi_pid != pid:
            return None
        return f"mac-proc-start:{info.pbi_start_tvsec}:{info.pbi_start_tvusec}"
    except (AttributeError, OSError, TypeError, ValueError):
        return None


def process_identity(pid: int) -> str | None:
    if pid <= 1:
        return None
    proc_stat = pathlib.Path(f"/proc/{pid}/stat")
    try:
        contents = proc_stat.read_text(encoding="utf-8")
    except OSError:
        contents = ""
    if contents:
        right_paren = contents.rfind(") ")
        if right_paren >= 0:
            fields = contents[right_paren + 2 :].split()
            if len(fields) > 19:
                try:
                    boot_id = pathlib.Path(
                        "/proc/sys/kernel/random/boot_id"
                    ).read_text(encoding="utf-8").strip()
                except OSError:
                    boot_id = ""
                if boot_id:
                    return f"proc-starttime:{boot_id}:{fields[19]}"
                return None
    if sys.platform == "darwin":
        return darwin_process_identity(pid)
    return None


def atomic_write(path: pathlib.Path, contents: str) -> None:
    fd, temporary_name = tempfile.mkstemp(prefix=f".{path.name}.", dir=str(path.parent))
    temporary_path = pathlib.Path(temporary_name)
    try:
        with os.fdopen(fd, "w", encoding="utf-8") as temporary:
            temporary.write(contents)
            temporary.flush()
            os.fsync(temporary.fileno())
        os.replace(temporary_path, path)
        directory_fd = os.open(path.parent, os.O_RDONLY | getattr(os, "O_DIRECTORY", 0))
        try:
            os.fsync(directory_fd)
        finally:
            os.close(directory_fd)
    finally:
        temporary_path.unlink(missing_ok=True)


local_lock_path = harness_root / ".ports.lock"
with local_lock_path.open("a+", encoding="utf-8") as local_lock:
    fcntl.flock(local_lock.fileno(), fcntl.LOCK_EX)
    if not reservation_path.exists():
        raise SystemExit("error: harness port reservation disappeared before owner binding")
    reservation = json.loads(reservation_path.read_text(encoding="utf-8"))
    if reservation.get("reservation_token") != expected_token:
        raise SystemExit("error: harness port reservation token changed before owner binding")
    if int(reservation.get("owner_pid", 0)) != old_owner_pid:
        raise SystemExit("error: harness port reservation owner changed before owner binding")
    recorded_owner_identity = reservation.get("owner_identity")
    if not isinstance(recorded_owner_identity, str) or not recorded_owner_identity:
        raise SystemExit(
            "error: harness port reservation owner identity is unavailable before owner binding"
        )
    current_old_identity = process_identity(old_owner_pid) if pid_alive(old_owner_pid) else None
    if not current_old_identity or current_old_identity != recorded_owner_identity:
        raise SystemExit("error: harness port reservation owner identity changed before owner binding")
    new_owner_identity = process_identity(new_owner_pid)
    if not new_owner_identity:
        raise SystemExit(f"error: unable to identify new harness owner {new_owner_pid}")
    registry_path = pathlib.Path(reservation.get("registry_path", ""))
    if not registry_path.is_absolute():
        if common_dir is None:
            raise SystemExit("error: harness port reservation has no shared registry path")
        registry_path = common_dir / ".oasis7-harness-port-registry" / "reservations.json"
    registry_lock_path = registry_path.parent / "registry.lock"
    with registry_lock_path.open("a+", encoding="utf-8") as registry_lock:
        fcntl.flock(registry_lock.fileno(), fcntl.LOCK_EX)
        if not registry_path.exists():
            raise SystemExit("error: shared harness port registry disappeared before owner binding")
        registry = json.loads(registry_path.read_text(encoding="utf-8"))
        reservations = registry.get("reservations")
        record = reservations.get(expected_token) if isinstance(reservations, dict) else None
        if not isinstance(record, dict) or record.get("harness_root") != str(harness_root):
            raise SystemExit("error: shared harness port reservation changed before owner binding")
        if int(record.get("owner_pid", 0)) != old_owner_pid:
            raise SystemExit("error: shared harness port owner changed before owner binding")
        if record.get("owner_identity") != recorded_owner_identity:
            raise SystemExit("error: shared harness port owner identity changed before owner binding")
        reservation["owner_pid"] = new_owner_pid
        reservation["owner_identity"] = new_owner_identity
        record["owner_pid"] = new_owner_pid
        record["owner_identity"] = new_owner_identity
        atomic_write(registry_path, json.dumps(registry, sort_keys=True) + "\n")
        atomic_write(reservation_path, json.dumps(reservation, sort_keys=True) + "\n")
PY
}

wh_release_ports_reservation() {
  local harness_root=$1
  local reservation_token=$2
  local common_dir=${3:-}
  python3 - "$harness_root" "$reservation_token" "$common_dir" <<'PY'
from __future__ import annotations

import fcntl
import json
import os
import pathlib
import tempfile
import sys

harness_root = pathlib.Path(sys.argv[1]).resolve()
expected_token = sys.argv[2]
common_dir = pathlib.Path(sys.argv[3]).resolve() if sys.argv[3] else None
reservation_path = harness_root / ".ports.reservation.json"


def atomic_write(path: pathlib.Path, contents: str) -> None:
    fd, temporary_name = tempfile.mkstemp(prefix=f".{path.name}.", dir=str(path.parent))
    temporary_path = pathlib.Path(temporary_name)
    try:
        with os.fdopen(fd, "w", encoding="utf-8") as temporary:
            temporary.write(contents)
            temporary.flush()
            os.fsync(temporary.fileno())
        os.replace(temporary_path, path)
        directory_fd = os.open(path.parent, os.O_RDONLY | getattr(os, "O_DIRECTORY", 0))
        try:
            os.fsync(directory_fd)
        finally:
            os.close(directory_fd)
    finally:
        temporary_path.unlink(missing_ok=True)


local_lock_path = harness_root / ".ports.lock"
with local_lock_path.open("a+", encoding="utf-8") as local_lock:
    fcntl.flock(local_lock.fileno(), fcntl.LOCK_EX)
    reservation = {}
    if reservation_path.exists():
        reservation = json.loads(reservation_path.read_text(encoding="utf-8"))
    registry_dir = common_dir / ".oasis7-harness-port-registry" if common_dir is not None else None
    if not reservation_path.exists() and (registry_dir is None or not registry_dir.exists()):
        raise SystemExit(0)
    registry_path = pathlib.Path(reservation.get("registry_path", ""))
    if not registry_path.is_absolute():
        if common_dir is None:
            if reservation_path.exists():
                raise SystemExit("error: harness port reservation has no shared registry path")
            raise SystemExit(0)
        registry_path = common_dir / ".oasis7-harness-port-registry" / "reservations.json"
    registry_lock_path = registry_path.parent / "registry.lock"
    with registry_lock_path.open("a+", encoding="utf-8") as registry_lock:
        fcntl.flock(registry_lock.fileno(), fcntl.LOCK_EX)
        if registry_path.exists():
            registry = json.loads(registry_path.read_text(encoding="utf-8"))
            reservations = registry.get("reservations")
            if isinstance(reservations, dict):
                record = reservations.get(expected_token)
                if isinstance(record, dict) and record.get("harness_root") == str(harness_root):
                    del reservations[expected_token]
                    atomic_write(registry_path, json.dumps(registry, sort_keys=True) + "\n")
        if reservation.get("reservation_token") == expected_token and reservation_path.exists():
            reservation_path.unlink()
PY
}

wh_state_write() {
  local state_file=$1
  local patch_json=$2
  mkdir -p "$(dirname "$state_file")"
  python3 - "$state_file" "$patch_json" <<'PY'
from __future__ import annotations

import fcntl
import json
import os
import pathlib
import tempfile
import sys

state_path = pathlib.Path(sys.argv[1])
patch = json.loads(sys.argv[2])
state_path.parent.mkdir(parents=True, exist_ok=True)
lock_path = state_path.with_name(f".{state_path.name}.lock")
with lock_path.open("a+", encoding="utf-8") as lock:
    fcntl.flock(lock.fileno(), fcntl.LOCK_EX)
    if state_path.exists():
        current = json.loads(state_path.read_text(encoding="utf-8"))
    else:
        current = {}
    current.update(patch)
    contents = json.dumps(current, ensure_ascii=False, indent=2) + "\n"
    mode = state_path.stat().st_mode & 0o777 if state_path.exists() else 0o644
    fd, temporary_name = tempfile.mkstemp(
        prefix=f".{state_path.name}.",
        dir=str(state_path.parent),
    )
    temporary_path = pathlib.Path(temporary_name)
    try:
        with os.fdopen(fd, "w", encoding="utf-8") as temporary:
            temporary.write(contents)
            temporary.flush()
            os.fsync(temporary.fileno())
        os.chmod(temporary_path, mode)
        os.replace(temporary_path, state_path)
        directory_flags = os.O_RDONLY | getattr(os, "O_DIRECTORY", 0)
        directory_fd = os.open(state_path.parent, directory_flags)
        try:
            os.fsync(directory_fd)
        finally:
            os.close(directory_fd)
    finally:
        temporary_path.unlink(missing_ok=True)
PY
}

# Return a comparable wall-epoch clock in milliseconds.  Some hosted CI runners
# may give separate Python processes independent monotonic origins, so the
# watchdog uses one epoch clock for both deadline creation and polling.
wh_clock_ms() {
  python3 - <<'PY'
import time
print(time.time_ns() // 1_000_000)
PY
}

# Run a command (or a shell function) until an absolute epoch deadline.
# `timeout(1)` is not available on every supported macOS/Linux runner, so the
# small watchdog stays in bash and kills the child on the same portable path.
# 124 mirrors the conventional timeout exit code and is reserved for the
# deadline case; callers can distinguish a browser failure from a deadline.
wh_run_with_deadline() {
  local deadline_ms=$1
  shift
  [[ "$deadline_ms" =~ ^[0-9]+$ ]] || {
    echo "error: invalid epoch deadline: $deadline_ms" >&2
    return 2
  }
  [[ "$#" -gt 0 ]] || {
    echo "error: wh_run_with_deadline requires a command" >&2
    return 2
  }

  # Enable bash job control for this one launch so the background command gets
  # its own process group.  Preserve the caller's setting after the launch;
  # this keeps shell-function commands usable on macOS where `setsid` is not a
  # guaranteed system utility, while still giving Linux the same group-bound
  # kill semantics.
  local monitor_was_enabled=0
  case "$-" in
    *m*) monitor_was_enabled=1 ;;
  esac
  set -m
  "$@" &
  local child_pid=$!
  if [[ "$monitor_was_enabled" -eq 0 ]]; then
    set +m
  fi
  while kill -0 "$child_pid" >/dev/null 2>&1; do
    local now_ms
    now_ms=$(wh_clock_ms)
    if (( now_ms >= deadline_ms )); then
      kill -TERM "-$child_pid" >/dev/null 2>&1 || kill -TERM "$child_pid" >/dev/null 2>&1 || true
      sleep 0.1
      kill -KILL "-$child_pid" >/dev/null 2>&1 || kill -KILL "$child_pid" >/dev/null 2>&1 || true
      wait "$child_pid" >/dev/null 2>&1 || true
      return 124
    fi
    sleep 0.05
  done
  wait "$child_pid"
}

wh_state_phase() {
  local state_file=$1
  local phase=$2
  local message=${3:-}
  local deadline_ms=${4:-}
  local now_ms
  now_ms=$(wh_clock_ms)
  wh_state_write "$state_file" "$(python3 - "$phase" "$message" "$deadline_ms" "$now_ms" <<'PY'
from __future__ import annotations

import json
import sys

phase, message, deadline_raw, now_raw = sys.argv[1:]
deadline = int(deadline_raw) if deadline_raw.isdigit() else None
now = int(now_raw)
payload = {
    "phase": phase,
    "phase_started_epoch_ms": now,
    "phase_deadline_epoch_ms": deadline,
    "progress": {
        "phase": phase,
        "message": message,
        "updated_epoch_ms": now,
    },
}
print(json.dumps(payload, ensure_ascii=False))
PY
)"
}

wh_state_progress() {
  local state_file=$1
  local message=$2
  local attempt=${3:-}
  local now_ms current_phase
  now_ms=$(wh_clock_ms)
  current_phase=$(wh_state_get "$state_file" phase 2>/dev/null || true)
  wh_state_write "$state_file" "$(python3 - "$current_phase" "$message" "$attempt" "$now_ms" <<'PY'
from __future__ import annotations

import json
import sys

phase, message, attempt_raw, now_raw = sys.argv[1:]
progress = {
    "phase": phase,
    "message": message,
    "updated_epoch_ms": int(now_raw),
}
if attempt_raw.isdigit():
    progress["attempt"] = int(attempt_raw)
print(json.dumps({"progress": progress}))
PY
)"
}

wh_state_get() {
  local state_file=$1
  local key=$2
  python3 - "$state_file" "$key" <<'PY'
from __future__ import annotations

import json
import pathlib
import sys

state_path = pathlib.Path(sys.argv[1])
key = sys.argv[2]
if not state_path.exists():
    raise SystemExit(1)
data = json.loads(state_path.read_text(encoding="utf-8"))
value = data.get(key)
if value is None:
    raise SystemExit(1)
if isinstance(value, bool):
    print("true" if value else "false")
elif isinstance(value, (dict, list)):
    print(json.dumps(value, ensure_ascii=False))
else:
    print(value)
PY
}

wh_state_show() {
  local state_file=$1
  if [[ -f "$state_file" ]]; then
    cat "$state_file"
    return 0
  fi
  echo "error: state file does not exist: $state_file" >&2
  return 1
}

wh_pid_alive() {
  local pid=$1
  [[ -n "$pid" ]] || return 1
  kill -0 "$pid" >/dev/null 2>&1 || return 1
  local process_state
  process_state=$(ps -o stat= -p "$pid" 2>/dev/null | awk 'NF { print $1; exit }' || true)
  [[ -z "$process_state" || "$process_state" != Z* ]]
}

wh_process_group_id() {
  local pid=$1
  [[ "$pid" =~ ^[1-9][0-9]*$ ]] || return 1
  ps -o pgid= -p "$pid" | awk 'NF { print $1; exit }'
}

wh_process_group_alive() {
  local pgid=$1
  [[ "$pgid" =~ ^[1-9][0-9]*$ ]] || return 1
  ps -axo pid=,pgid=,stat= |
    awk -v target="$pgid" '$2 == target && $3 !~ /^Z/ { found = 1 } END { exit found ? 0 : 1 }'
}

# Capture stable identities for the members visible in a managed process
# group. The launch leader can exit while a descendant remains in the group;
# retaining the member PID/incarnation pair lets termination prove that the
# surviving group is the one originally launched. An unavailable member
# identity is omitted, which deliberately makes leader-less cleanup fail
# closed when no recorded survivor can be authenticated.
wh_process_group_member_identities() {
  local pgid=${1:-}
  [[ "$pgid" =~ ^[1-9][0-9]*$ && "$pgid" -gt 1 ]] || return 1
  local member_pid member_identity member_records=""
  while read -r member_pid; do
    [[ "$member_pid" =~ ^[1-9][0-9]*$ ]] || continue
    member_identity=$(wh_process_identity "$member_pid" 2>/dev/null || true)
    [[ -n "$member_identity" ]] || continue
    if [[ -n "$member_records" ]]; then
      member_records+=","
    fi
    member_records+="${member_pid}=${member_identity}"
  done < <(
    ps -axo pid=,pgid=,stat= |
      awk -v target="$pgid" '$2 == target && $3 !~ /^Z/ { print $1 }'
  )
  [[ -n "$member_records" ]] || return 1
  printf '%s\n' "$member_records"
}

# Refresh the authenticated member set while the recorded leader is still
# live. Launchers commonly create their chain/viewer children after the
# initial wh_start_managed snapshot; collecting those members during a normal
# liveness handoff lets a later leader crash be cleaned up without trusting a
# bare PGID. Existing PID/incarnation records are never replaced, so PID reuse
# or a foreign process cannot gain authority through a refresh.
wh_process_group_refresh_identity() {
  local pid=${1:-}
  local pgid=${2:-}
  local expected_identity=${3:-}
  local leader_identity member_records refreshed_identity

  [[ "$pid" =~ ^[1-9][0-9]*$ ]] || return 2
  [[ "$pgid" =~ ^[1-9][0-9]*$ && "$pgid" -gt 1 ]] || return 2
  [[ -n "$expected_identity" ]] || return 2
  wh_process_record_alive "$pid" "$pgid" "$expected_identity" || return 1
  leader_identity=$(wh_process_identity_leader_part "$expected_identity")
  member_records=$(wh_process_group_member_identities "$pgid" 2>/dev/null || true)
  [[ -n "$member_records" ]] || return 1
  refreshed_identity="$leader_identity|group=$member_records"
  wh_process_identity_merge_compatible "$expected_identity" "$refreshed_identity"
}

wh_process_identity_leader_part() {
  local identity=${1:-}
  if [[ "$identity" == *"|group="* ]]; then
    printf '%s\n' "${identity%%|group=*}"
  else
    printf '%s\n' "$identity"
  fi
}

wh_process_identity_group_part() {
  local identity=${1:-}
  [[ "$identity" == *"|group="* ]] || return 1
  local group_records=${identity#*|group=}
  [[ -n "$group_records" ]] || return 1
  printf '%s\n' "$group_records"
}

# Legacy identities either have only the monotonic /proc start-time tick and
# no boot ID, or were derived from ps's whole-second launch time. Neither can
# be compared safely with a current identity. Keep live owners carrying these
# formats uncertain so callers retain/reject them instead of reclaiming them.
wh_process_identity_is_legacy() {
  local identity=${1:-}
  [[ "$identity" =~ ^proc-starttime:[^:]+$ || "$identity" =~ ^ps-start:.+$ ]]
}

# Merge process-group identity snapshots without ever weakening the
# authenticated leader. A later launcher snapshot may add descendants after
# the initial record; preserve the union of compatible PID/incarnation pairs.
# Reusing a member PID with a different identity is a conflict and must fail
# closed rather than grant authority to either incarnation.
wh_process_identity_merge_compatible() {
  local first_identity=${1:-}
  local second_identity=${2:-}
  local first_leader second_leader first_records second_records merged_records
  local member_record member_pid member_identity existing_record scan_records
  local found_member

  [[ -n "$first_identity" && -n "$second_identity" ]] || return 2
  first_leader=$(wh_process_identity_leader_part "$first_identity")
  second_leader=$(wh_process_identity_leader_part "$second_identity")
  [[ -n "$first_leader" && "$first_leader" == "$second_leader" ]] || return 2
  first_records=$(wh_process_identity_group_part "$first_identity" 2>/dev/null || true)
  second_records=$(wh_process_identity_group_part "$second_identity" 2>/dev/null || true)
  merged_records="$first_records"

  scan_records="$merged_records"
  while [[ -n "$scan_records" ]]; do
    if [[ "$scan_records" == *,* ]]; then
      member_record=${scan_records%%,*}
      scan_records=${scan_records#*,}
    else
      member_record=$scan_records
      scan_records=""
    fi
    [[ "$member_record" == *=* ]] || return 2
    member_pid=${member_record%%=*}
    member_identity=${member_record#*=}
    [[ "$member_pid" =~ ^[1-9][0-9]*$ && -n "$member_identity" ]] || return 2
  done

  while [[ -n "$second_records" ]]; do
    if [[ "$second_records" == *,* ]]; then
      member_record=${second_records%%,*}
      second_records=${second_records#*,}
    else
      member_record=$second_records
      second_records=""
    fi
    [[ "$member_record" == *=* ]] || return 2
    member_pid=${member_record%%=*}
    member_identity=${member_record#*=}
    [[ "$member_pid" =~ ^[1-9][0-9]*$ && -n "$member_identity" ]] || return 2
    found_member=0
    scan_records="$merged_records"
    while [[ -n "$scan_records" ]]; do
      if [[ "$scan_records" == *,* ]]; then
        existing_record=${scan_records%%,*}
        scan_records=${scan_records#*,}
      else
        existing_record=$scan_records
        scan_records=""
      fi
      if [[ "${existing_record%%=*}" == "$member_pid" ]]; then
        [[ "${existing_record#*=}" == "$member_identity" ]] || return 2
        found_member=1
        break
      fi
    done
    if [[ "$found_member" -eq 0 ]]; then
      if [[ -n "$merged_records" ]]; then
        merged_records+=","
      fi
      merged_records+="${member_pid}=${member_identity}"
    fi
  done
  if [[ -n "$merged_records" ]]; then
    printf '%s|group=%s\n' "$first_leader" "$merged_records"
  else
    printf '%s\n' "$first_leader"
  fi
}

# Prove that a process group whose recorded leader has exited still contains
# an authenticated member captured at launch. A PGID alone is insufficient:
# it can be reused by an unrelated process group, so no signal is sent unless
# a recorded PID/incarnation pair is still in the recorded PGID.
wh_process_group_has_recorded_member() {
  local pgid=${1:-}
  local leader_pid=${2:-}
  local expected_identity=${3:-}
  local group_records member_record member_pid member_identity current_pgid current_identity
  [[ "$pgid" =~ ^[1-9][0-9]*$ && "$pgid" -gt 1 ]] || return 1
  [[ "$leader_pid" =~ ^[1-9][0-9]*$ ]] || return 1
  group_records=$(wh_process_identity_group_part "$expected_identity" 2>/dev/null || true)
  [[ -n "$group_records" ]] || return 1
  while [[ -n "$group_records" ]]; do
    if [[ "$group_records" == *,* ]]; then
      member_record=${group_records%%,*}
      group_records=${group_records#*,}
    else
      member_record=$group_records
      group_records=""
    fi
    member_pid=${member_record%%=*}
    member_identity=${member_record#*=}
    [[ "$member_pid" =~ ^[1-9][0-9]*$ && "$member_pid" != "$leader_pid" ]] || continue
    current_pgid=$(wh_process_group_id "$member_pid" 2>/dev/null || true)
    [[ "$current_pgid" == "$pgid" ]] || continue
    current_identity=$(wh_process_identity "$member_pid" 2>/dev/null || true)
    [[ -n "$current_identity" && "$current_identity" == "$member_identity" ]] && return 0
  done
  return 1
}

# Validate a recorded process incarnation before treating it as live.  A PID
# and PGID are only reusable numbers; the identity binds them to the process
# that was actually launched by this harness.  Callers that make lifecycle or
# readiness decisions must use this predicate rather than bare kill -0.
wh_process_record_alive() {
  local pid=${1:-}
  local pgid=${2:-}
  local expected_identity=${3:-}
  local current_identity current_pgid expected_leader_identity

  [[ "$pid" =~ ^[1-9][0-9]*$ ]] || return 1
  [[ "$pgid" =~ ^[1-9][0-9]*$ && "$pgid" -gt 1 ]] || return 1
  [[ -n "$expected_identity" ]] || return 1
  wh_process_identity_is_legacy "$expected_identity" && return 1
  wh_pid_alive "$pid" || return 1
  expected_leader_identity=$(wh_process_identity_leader_part "$expected_identity")
  current_identity=$(wh_process_identity "$pid" 2>/dev/null || true)
  [[ -n "$current_identity" && "$current_identity" == "$expected_leader_identity" ]] || return 1
  current_pgid=$(wh_process_group_id "$pid" 2>/dev/null || true)
  [[ "$current_pgid" == "$pgid" ]] || return 1
  # The recorded leader itself proves that the group has a member.  Full
  # group enumeration is reserved for termination/quiescence checks; keeping
  # it out of read/readiness paths avoids making those paths depend on a
  # potentially slow process table scan.
  return 0
}

# Return a stable identity for a process incarnation.  PID and PGID values can
# be reused after a crash, so cleanup records this value at launch and refuses
# to signal a replacement process. Linux binds the monotonic start-time tick
# in /proc to the current boot ID. Persisted legacy records without that boot
# binding are retained as unknown while their owner is live, preventing an
# unsafe cross-boot reclaim; dead legacy owners remain safely stale. macOS
# reads the kernel-backed proc_pidinfo start seconds/useconds pair. If that
# API is unavailable, identity resolution fails closed rather than falling
# back to ps's whole-second launch time.
wh_process_identity() {
  local pid=${1:-}
  [[ "$pid" =~ ^[1-9][0-9]*$ ]] || return 1
  if [[ -r "/proc/$pid/stat" ]]; then
    python3 - "$pid" <<'PY'
from __future__ import annotations

import pathlib
import sys

pid = sys.argv[1]
contents = pathlib.Path(f"/proc/{pid}/stat").read_text(encoding="utf-8")
boot_id = pathlib.Path("/proc/sys/kernel/random/boot_id").read_text(encoding="utf-8").strip()
right_paren = contents.rfind(") ")
if right_paren < 0 or not boot_id:
    raise SystemExit(1)
fields = contents[right_paren + 2 :].split()
if len(fields) <= 19:
    raise SystemExit(1)
print(f"proc-starttime:{boot_id}:{fields[19]}")
PY
    return
  fi

  if [[ "$(uname -s 2>/dev/null || true)" == "Darwin" ]]; then
    python3 - "$pid" <<'PY'
from __future__ import annotations

import ctypes
import sys

pid = int(sys.argv[1])


class ProcBsdInfo(ctypes.Structure):
    _fields_ = [
        ("pbi_flags", ctypes.c_uint32),
        ("pbi_status", ctypes.c_uint32),
        ("pbi_xstatus", ctypes.c_uint32),
        ("pbi_pid", ctypes.c_uint32),
        ("pbi_ppid", ctypes.c_uint32),
        ("pbi_uid", ctypes.c_uint32),
        ("pbi_gid", ctypes.c_uint32),
        ("pbi_ruid", ctypes.c_uint32),
        ("pbi_rgid", ctypes.c_uint32),
        ("pbi_svuid", ctypes.c_uint32),
        ("pbi_svgid", ctypes.c_uint32),
        ("rfu_1", ctypes.c_uint32),
        ("pbi_comm", ctypes.c_char * 16),
        ("pbi_name", ctypes.c_char * 32),
        ("pbi_nfiles", ctypes.c_uint32),
        ("pbi_pgid", ctypes.c_uint32),
        ("pbi_pjobc", ctypes.c_uint32),
        ("e_tdev", ctypes.c_uint32),
        ("e_tpgid", ctypes.c_uint32),
        ("pbi_nice", ctypes.c_int32),
        ("pbi_start_tvsec", ctypes.c_uint64),
        ("pbi_start_tvusec", ctypes.c_uint64),
    ]


try:
    libproc = ctypes.CDLL("/usr/lib/libproc.dylib")
    proc_pidinfo = libproc.proc_pidinfo
    proc_pidinfo.argtypes = [
        ctypes.c_int,
        ctypes.c_int,
        ctypes.c_uint64,
        ctypes.c_void_p,
        ctypes.c_int,
    ]
    proc_pidinfo.restype = ctypes.c_int
    info = ProcBsdInfo()
    size = ctypes.sizeof(info)
    if proc_pidinfo(pid, 3, 0, ctypes.byref(info), size) != size:
        raise SystemExit(1)
    if info.pbi_pid != pid:
        raise SystemExit(1)
    print(f"mac-proc-start:{info.pbi_start_tvsec}:{info.pbi_start_tvusec}")
except (AttributeError, OSError, TypeError, ValueError):
    raise SystemExit(1)
PY
    return
  fi

  return 1
}

# Return the ownership state of a recovery marker target: 0 means live and
# identity-matching, 1 means stopped or known-stale and safe to reclaim, and 2
# means unknown and therefore fail-closed. The marker stores the same
# PID:identity record as the lifecycle lock; unlike a lock owner, a marker may
# be removed only after this explicit authentication step.
wh_lifecycle_recovery_marker_status() {
  local marker_record=${1:-}
  local marker_pid marker_identity observed_identity
  [[ "$marker_record" =~ ^([1-9][0-9]*):(.+)$ ]] || return 2
  marker_pid="${BASH_REMATCH[1]}"
  marker_identity="${BASH_REMATCH[2]}"
  if ! wh_pid_alive "$marker_pid"; then
    return 1
  fi
  if wh_process_identity_is_legacy "$marker_identity"; then
    return 2
  fi
  observed_identity=$(wh_process_identity "$marker_pid" 2>/dev/null || true)
  [[ -n "$observed_identity" ]] || return 2
  [[ "$observed_identity" == "$marker_identity" ]] && return 0
  return 1
}

# Serialize up/down transitions before they touch state, process records, or
# port reservations.  The symlink target is the owner PID plus its launch
# identity, allowing a later invocation to recover a lock left by a crashed
# process without trusting a reused PID.  Stale-lock recovery claims a second
# symlink before removing the lock; the claim serializes recovery and prevents
# one recoverer from unlinking a lock that another recoverer has acquired.  An
# unavailable identity is never treated as stale.  The lifecycle lock is
# acquired before the per-harness port lock; port helpers retain their
# local-then-shared registry lock order.
WH_LIFECYCLE_LOCK_PATH=""
WH_LIFECYCLE_LOCK_RECORD=""
wh_lifecycle_lock_acquire() {
  local harness_root=${1:-}
  [[ -n "$harness_root" ]] || return 2
  mkdir -p "$harness_root"
  local lock_path="$harness_root/.lifecycle.lock"
  local recovery_path="$lock_path.recovery"
  local owner_identity
  owner_identity=$(wh_process_identity "$$") || {
    echo "error: unable to identify lifecycle lock owner $$" >&2
    return 1
  }
  local owner_record="$$:$owner_identity"
  local lock_timeout_ms=${OASIS7_HARNESS_LIFECYCLE_LOCK_TIMEOUT_MS:-120000}
  [[ "$lock_timeout_ms" =~ ^[0-9]+$ ]] || {
    echo "error: invalid lifecycle lock timeout: $lock_timeout_ms" >&2
    return 2
  }
  local waited_ms=0 current_record current_pid current_identity observed_identity
  local recovery_record recovery_status

  while true; do
    # A recovery claim is deliberately fail-closed.  If its owner is killed
    # between removing the stale lock and publishing the replacement, no
    # other invocation may guess whether it is safe to remove that claim.
    if [[ -L "$recovery_path" || -e "$recovery_path" ]]; then
      recovery_record=$(readlink "$recovery_path" 2>/dev/null || true)
      recovery_status=2
      if [[ -n "$recovery_record" ]]; then
        if wh_lifecycle_recovery_marker_status "$recovery_record"; then
          recovery_status=0
        else
          recovery_status=$?
        fi
      fi
      if [[ "$recovery_status" -eq 1 ]]; then
        # Compare before removal so a replacement marker cannot be deleted by
        # a stale-owner observation from an earlier iteration.
        if [[ "$(readlink "$recovery_path" 2>/dev/null || true)" == "$recovery_record" ]]; then
          rm -f "$recovery_path"
        fi
        continue
      fi
      if (( waited_ms >= lock_timeout_ms )); then
        echo "error: timed out waiting for lifecycle lock: $lock_path" >&2
        return 1
      fi
      sleep 0.05
      waited_ms=$((waited_ms + 50))
      continue
    fi

    if ln -s "$owner_record" "$lock_path" 2>/dev/null; then
      WH_LIFECYCLE_LOCK_PATH="$lock_path"
      WH_LIFECYCLE_LOCK_RECORD="$owner_record"
      return 0
    fi

    current_record=$(readlink "$lock_path" 2>/dev/null || true)
    if [[ "$current_record" =~ ^([1-9][0-9]*):(.+)$ ]]; then
      current_pid="${BASH_REMATCH[1]}"
      current_identity="${BASH_REMATCH[2]}"
      if ! wh_pid_alive "$current_pid"; then
        observed_identity="stopped"
      elif wh_process_identity_is_legacy "$current_identity"; then
        # The owner is live, but a pre-boot_id record cannot be compared
        # across boots.  Treat it as unknown and retain the lock.
        # An empty observation is intentional: the recovery predicate below
        # only reclaims a lock for a stopped owner or a known mismatched
        # identity.  A sentinel string would itself differ from the legacy
        # record and incorrectly trigger recovery.
        observed_identity=""
      else
        # A live PID with an unavailable identity is an uncertainty, not a
        # stale owner.  Waiting is the only safe choice because removing the
        # lock could permit two lifecycle transitions at once.
        observed_identity=$(wh_process_identity "$current_pid" 2>/dev/null || true)
      fi
      if [[ "$observed_identity" == "stopped" || \
        ( -n "$observed_identity" && "$observed_identity" != "$current_identity" ) ]]; then
        # ln is the atomic claim.  Once it succeeds, all other invocations
        # wait on this claim and cannot race the compare-then-remove below.
        recovery_record="$owner_record"
        if ln -s "$recovery_record" "$recovery_path" 2>/dev/null; then
          if [[ "$(readlink "$lock_path" 2>/dev/null || true)" == "$current_record" ]]; then
            rm -f "$lock_path"
            if ln -s "$owner_record" "$lock_path" 2>/dev/null; then
              if [[ "$(readlink "$recovery_path" 2>/dev/null || true)" == "$recovery_record" ]]; then
                rm -f "$recovery_path"
              fi
              WH_LIFECYCLE_LOCK_PATH="$lock_path"
              WH_LIFECYCLE_LOCK_RECORD="$owner_record"
              return 0
            fi
          fi
          if [[ "$(readlink "$recovery_path" 2>/dev/null || true)" == "$recovery_record" ]]; then
            rm -f "$recovery_path"
          fi
        fi
        continue
      fi
    fi
    if (( waited_ms >= lock_timeout_ms )); then
      echo "error: timed out waiting for lifecycle lock: $lock_path" >&2
      return 1
    fi
    sleep 0.05
    waited_ms=$((waited_ms + 50))
  done
}

wh_lifecycle_lock_release() {
  [[ -n "$WH_LIFECYCLE_LOCK_PATH" && -n "$WH_LIFECYCLE_LOCK_RECORD" ]] || return 0
  local recovery_path="$WH_LIFECYCLE_LOCK_PATH.recovery"
  # Claim the recovery marker before checking and unlinking our lock.  This
  # keeps release ownership serialized with stale recovery, including the
  # small window between the readlink check and rm.
  if ln -s "$WH_LIFECYCLE_LOCK_RECORD" "$recovery_path" 2>/dev/null; then
    if [[ "$(readlink "$WH_LIFECYCLE_LOCK_PATH" 2>/dev/null || true)" == "$WH_LIFECYCLE_LOCK_RECORD" ]]; then
      rm -f "$WH_LIFECYCLE_LOCK_PATH"
    fi
    if [[ "$(readlink "$recovery_path" 2>/dev/null || true)" == "$WH_LIFECYCLE_LOCK_RECORD" ]]; then
      rm -f "$recovery_path"
    fi
  fi
  WH_LIFECYCLE_LOCK_PATH=""
  WH_LIFECYCLE_LOCK_RECORD=""
}

# Terminate only the process group that was recorded at launch. The caller
# must provide the launcher's PID, the PGID captured from that PID, and the
# stable identity captured at launch; a reused PID or changed group is
# rejected before any signal is sent. When the leader has exited, a surviving
# member identity captured at launch must still prove group ownership.
wh_terminate_process_group() {
  local pid=${1:-}
  local pgid=${2:-}
  local timeout_ms=${3:-2000}
  local expected_identity=${4:-}
  local current_pgid current_identity expected_leader_identity deadline_ms

  [[ "$pid" =~ ^[1-9][0-9]*$ ]] || return 2
  [[ "$pgid" =~ ^[1-9][0-9]*$ ]] || return 2
  [[ "$timeout_ms" =~ ^[0-9]+$ ]] || return 2
  [[ "$pgid" -gt 1 ]] || return 2

  # Never permit a stale record to target the process group running the
  # cleanup caller itself.
  current_pgid=$(wh_process_group_id "$$" 2>/dev/null || true)
  [[ -n "$current_pgid" && "$pgid" != "$current_pgid" ]] || return 2

  if ! wh_process_group_alive "$pgid"; then
    if wh_pid_alive "$pid"; then
      return 2
    fi
    return 0
  fi
  [[ -n "$expected_identity" ]] || return 2
  wh_process_identity_is_legacy "$expected_identity" && return 2
  expected_leader_identity=$(wh_process_identity_leader_part "$expected_identity")
  if wh_pid_alive "$pid"; then
    current_identity=$(wh_process_identity "$pid" 2>/dev/null || true)
    [[ -n "$current_identity" && "$current_identity" == "$expected_leader_identity" ]] || return 2
    current_pgid=$(wh_process_group_id "$pid" 2>/dev/null || true)
    [[ "$current_pgid" == "$pgid" ]] || return 2
  else
    # The leader is gone, so its identity cannot be checked directly. A
    # launch-captured member incarnation is the only safe proof that this
    # still-live PGID is the original group rather than a reused foreign one.
    wh_process_group_has_recorded_member "$pgid" "$pid" "$expected_identity" || return 2
  fi

  kill -TERM "-$pgid" >/dev/null 2>&1 || return 1
  deadline_ms=$(( $(wh_clock_ms) + timeout_ms ))
  while wh_process_group_alive "$pgid"; do
    if (( $(wh_clock_ms) >= deadline_ms )); then
      break
    fi
    sleep 0.05
  done

  if wh_process_group_alive "$pgid"; then
    kill -KILL "-$pgid" >/dev/null 2>&1 || return 1
    deadline_ms=$(( $(wh_clock_ms) + 1000 ))
    while wh_process_group_alive "$pgid" && (( $(wh_clock_ms) < deadline_ms )); do
      sleep 0.05
    done
  fi

  if wh_process_group_alive "$pgid"; then
    return 1
  fi
  wait "$pid" >/dev/null 2>&1 || true
}

# Launch a shell command in a dedicated process group and expose the recorded
# identity through WH_MANAGED_PID/WH_MANAGED_PGID.  Redirections belong around
# this function call so the child inherits the caller's chosen logs.
wh_start_managed() {
  local monitor_was_enabled=0
  local managed_identity group_member_identities
  case "$-" in
    *m*) monitor_was_enabled=1 ;;
  esac
  set -m
  "$@" &
  WH_MANAGED_PID=$!
  # The managed process outlives the launching shell in normal harness use;
  # disown it so a later group KILL does not produce a misleading job-control
  # diagnostic on the operator's terminal.
  disown "$WH_MANAGED_PID" >/dev/null 2>&1 || true
  if [[ "$monitor_was_enabled" -eq 0 ]]; then
    set +m
  fi
  WH_MANAGED_PGID="$(wh_process_group_id "$WH_MANAGED_PID" 2>/dev/null || true)"
  managed_identity="$(wh_process_identity "$WH_MANAGED_PID" 2>/dev/null || true)"
  group_member_identities="$(wh_process_group_member_identities "$WH_MANAGED_PGID" 2>/dev/null || true)"
  if [[ -z "$WH_MANAGED_PGID" || -z "$managed_identity" || -z "$group_member_identities" ]]; then
    kill "$WH_MANAGED_PID" >/dev/null 2>&1 || true
    wait "$WH_MANAGED_PID" >/dev/null 2>&1 || true
    echo "error: unable to record managed process identity for PID $WH_MANAGED_PID" >&2
    return 1
  fi
  WH_MANAGED_IDENTITY="${managed_identity}|group=${group_member_identities}"
}

wh_env_file_get() {
  local env_file=$1
  local key=$2
  python3 - "$env_file" "$key" <<'PY'
from __future__ import annotations

import pathlib
import sys

env_path = pathlib.Path(sys.argv[1])
key = sys.argv[2]
if not env_path.exists():
    raise SystemExit(1)
for raw in env_path.read_text(encoding="utf-8").splitlines():
    if "=" not in raw:
        continue
    left, right = raw.split("=", 1)
    if left == key:
        print(right)
        raise SystemExit(0)
raise SystemExit(1)
PY
}
