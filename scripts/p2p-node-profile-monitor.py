#!/usr/bin/env python3
"""Collect one explicit local node status into the shared durable publisher."""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import selectors
import signal
import stat
import subprocess
import sys
import time


def module(name, filename):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(filename))
    result = importlib.util.module_from_spec(spec)
    sys.modules[name] = result
    spec.loader.exec_module(result)
    return result


publisher = module("profile_publisher", "p2p-real-env-observability-attempt.py")
reporter = module("local_profile_report", "p2p-node-profile-report.py")
CAP = 4 * 1024 * 1024


class Interrupted(Exception):
    pass


def now():
    return time.time_ns() // 1_000_000


def overlaps(a, b):
    return a == b or a in b.parents or b in a.parents


def arguments():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--node-binary", required=True)
    parser.add_argument("--data-dir", required=True)
    parser.add_argument("--out-dir", default=".tmp/node_profile_monitor")
    parser.add_argument("--timeout-ms", default="10000")
    args = parser.parse_args()
    try:
        if not args.timeout_ms.isascii() or not args.timeout_ms.isdecimal() or not 1 <= int(args.timeout_ms) <= 60000:
            raise ValueError()
        binary, data = Path(args.node_binary), Path(args.data_dir)
        if not binary.is_absolute() or not data.is_absolute():
            raise ValueError()
        binary, data, root = binary.resolve(strict=True), data.resolve(strict=True), Path(args.out_dir).resolve()
        if not stat.S_ISREG(binary.stat().st_mode) or not os.access(binary, os.X_OK) or not data.is_dir():
            raise ValueError()
        if overlaps(root, data) or binary == root or root in binary.parents:
            raise ValueError()
    except (ValueError, OSError, RuntimeError):
        parser.exit(2, "invalid collector configuration\n")
    return binary, data, root, int(args.timeout_ms)


def digest_file(path):
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def stop(child):
    if child is not None:
        try:
            os.killpg(child.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        child.wait()
        if child.stdout is not None:
            child.stdout.close()


def probe(binary, data, timeout, interrupted=lambda: False):
    child = None
    result = "spawn_failed"
    output = bytearray()
    try:
        deadline = time.monotonic() + timeout / 1000
        child = subprocess.Popen([str(binary), "node", "status", "--data-dir", str(data)],
                                 stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, start_new_session=True)
        os.set_blocking(child.stdout.fileno(), False)
        with selectors.DefaultSelector() as selector:
            selector.register(child.stdout, selectors.EVENT_READ)
            eof = False
            while not eof or child.poll() is None:
                if interrupted():
                    return "interrupted", None
                remaining = deadline - time.monotonic()
                if remaining <= 0:
                    return "timeout", None
                if not eof:
                    for key, _ in selector.select(min(remaining, 0.05)):
                        chunk = os.read(key.fd, min(65536, CAP + 1 - len(output)))
                        if not chunk:
                            selector.unregister(child.stdout)
                            eof = True
                        else:
                            output.extend(chunk)
                            if len(output) > CAP:
                                return "output_limit", None
                else:
                    time.sleep(min(remaining, 0.01))
            if interrupted():
                return "interrupted", None
            if child.returncode != 0:
                return "command_failed", None
        def pairs(items):
            value = {}
            for key, item in items:
                if key in value:
                    raise ValueError()
                value[key] = item
            return value
        def constant(_):
            raise ValueError()
        try:
            payload = json.loads(output.decode("utf-8"), object_pairs_hook=pairs, parse_constant=constant)
            if not isinstance(payload, dict) or payload.get("api_version") != "oasis7.local-node-status/v1":
                raise ValueError()
        except (ValueError, UnicodeError, RecursionError):
            return "invalid_status", None
        return "success", payload
    except Interrupted:
        return "interrupted", None
    except (OSError, ValueError, subprocess.SubprocessError):
        return result, None
    finally:
        stop(child)


def main():
    binary, data, root, timeout = arguments()
    try:
        config = {"binary_sha256": digest_file(binary), "data_dir_sha256": hashlib.sha256(os.fsencode(data)).hexdigest(), "timeout_ms": timeout}
        scope = ["local_cli", config["binary_sha256"], config["data_dir_sha256"], str(timeout)]
        attempt = publisher.begin(root, scope, "node-profile")
    except (OSError, ValueError, publisher.AttemptError):
        print("collector initialization failed", file=sys.stderr)
        return 1
    directory = Path(attempt["run_dir"])
    previous = {}
    interrupted = [False]
    def interrupt(_signum, _frame):
        interrupted[0] = True
    for sig in (signal.SIGTERM, signal.SIGINT):
        previous[sig] = signal.signal(sig, interrupt)
    started = now()
    try:
        result, payload = probe(binary, data, timeout, lambda: interrupted[0])
        finished = now()
        profile = reporter.adapter.project(payload, "local_cli", finished) if result == "success" else reporter.unknown("collector_" + result)
        collection = dict(mode="local_cli", result=result, expected_sample_count=1, attempted_count=1,
                          failed_sample_count=0 if result == "success" else 1, complete=result == "success",
                          started_at_unix_ms=started, finished_at_unix_ms=finished, **config)
        source = reporter.build(profile, collection, finished)
        source.update(run_id=str(attempt["sequence"]) + "-" + attempt["attempt_id"], run_dir=str(directory))
        errors = [] if result == "success" else [{"stage": "node-profile", "exit_code": 1, "error": "collector_failed"}]
        try:
            (directory / "report").mkdir(exist_ok=True)
            publisher.atomic_write(directory / "config.json", publisher.encoded(config))
            publisher.atomic_write(directory / "report" / "latest_summary.json", publisher.encoded(source))
            publisher.atomic_write(directory / "report" / "latest_summary.md", reporter.markdown(source).encode("utf-8"))
        except (OSError, ValueError):
            errors.append({"stage": "summary", "exit_code": 1, "error": "summary_failed"})
        return publisher.finish(root, attempt["sequence"], attempt["attempt_id"], errors, "node-profile")
    except (OSError, ValueError, publisher.AttemptError, Interrupted):
        print("collector publication failed", file=sys.stderr)
        return 1
    finally:
        for sig, handler in previous.items():
            signal.signal(sig, handler)


if __name__ == "__main__":
    raise SystemExit(main())
