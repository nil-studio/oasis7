#!/usr/bin/env python3
"""One immutable, diagnostic-only GREEN/RED/restored-GREEN experiment.

No source, patch, command, or test selector is accepted from dispatch inputs.
Only the disposable checkout is mutated; never run in a developer checkout.
"""
import hashlib
import json
import os
from pathlib import Path
import selectors
import signal
import subprocess
import time

SOURCE = "ef4f2cee67b696430d28ce6062c45eae2638364e"
FILE = "crates/oasis7_chain_runtime/src/host/node_lifecycle/formal_full_sync_failover.rs"
FILE_SHA = "30f4c634d0465108f65cae34524793ecbbb0911d41a6e8f61aa2fc0abb882ee8"
TEST = "node_lifecycle::formal_full_sync::tests::provider_budget_tests::formal_full_sync_real_provider_charges_survive_failover_and_fresh_call_recovers"
OLD = b"        for offset in 0..provider_count {\n"
NEW = OLD + b"            attempt.budget = super::cas_lifecycle::CasBudget::new(self.config.max_total_sync_bytes);\n"
CAP = 8 * 1024 * 1024


class Cancelled(RuntimeError):
    pass


def entrypoint():
    previous = signal.getsignal(signal.SIGTERM)

    def cancel(_signum, _frame):
        # A repeated TERM must not interrupt bounded cleanup/restoration.
        signal.signal(signal.SIGTERM, signal.SIG_IGN)
        raise Cancelled("diagnostic cancelled")

    signal.signal(signal.SIGTERM, cancel)
    try:
        main()
    finally:
        signal.signal(signal.SIGTERM, previous)


def mutate(data):
    if hashlib.sha256(data).hexdigest() != FILE_SHA or data.count(OLD) != 1:
        raise ValueError("unrecognized immutable mutation input")
    return data.replace(OLD, NEW, 1)


def classify(code, output, red):
    text = output.decode("utf-8", errors="replace")
    if "running 1 test\n" not in text:
        return False
    if red:
        return (code == 101 and f"test {TEST} ... FAILED" in text
                and "failed-provider charge must consume this call's budget" in text
                and "test result: FAILED. 0 passed; 1 failed; 0 ignored;" in text)
    return (code == 0 and f"test {TEST} ... ok" in text
            and "test result: ok. 1 passed; 0 failed; 0 ignored;" in text)


def run(argv, cwd, deadline):
    """Bound combined output and wall time, reap the entire owned process group."""
    if time.monotonic() >= deadline:
        raise TimeoutError("experiment deadline")
    env = os.environ.copy()
    for key in ("RUSTC_WRAPPER", "RUSTC_WORKSPACE_WRAPPER", "CARGO_TARGET_DIR"):
        env.pop(key, None)
    proc = subprocess.Popen(argv, cwd=cwd, env=env, stdout=subprocess.PIPE,
                            stderr=subprocess.STDOUT, start_new_session=True)
    output = bytearray()
    try:
        with selectors.DefaultSelector() as selector:
            selector.register(proc.stdout, selectors.EVENT_READ)
            while selector.get_map():
                if time.monotonic() >= deadline:
                    raise TimeoutError("experiment deadline")
                for key, _ in selector.select(min(0.2, max(0, deadline-time.monotonic()))):
                    chunk = os.read(key.fileobj.fileno(), 65536)
                    if not chunk:
                        selector.unregister(key.fileobj)
                    else:
                        output.extend(chunk)
                        if len(output) > CAP:
                            raise ValueError("experiment output cap")
        code = proc.wait(timeout=max(0.001, deadline-time.monotonic()))
        if time.monotonic() >= deadline:
            raise TimeoutError("experiment deadline")
        return code, bytes(output)
    finally:
        try:
            os.killpg(proc.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        proc.wait()
        proc.stdout.close()


def main():
    # The trusted workflow supplies only fixed paths, never user inputs.
    root = Path(os.environ["GITHUB_WORKSPACE"]).resolve()
    source = root / "experiment-source"
    trusted = root / "trusted-runner"
    if source.is_symlink() or not source.is_dir() or source == trusted:
        raise ValueError("invalid isolated checkout")
    deadline = time.monotonic() + 3900
    code, head = run(["git", "rev-parse", "HEAD"], source, deadline)
    if code or head.decode().strip() != SOURCE:
        raise ValueError("wrong source commit")
    code, dirty = run(["git", "status", "--porcelain", "--untracked-files=no"], source, deadline)
    if code or dirty:
        raise ValueError("dirty source checkout")
    path = source / FILE
    if path.is_symlink():
        raise ValueError("symlink mutation input")
    original = path.read_bytes()
    mutated = mutate(original)
    receipts = []
    restored = False
    tracked_clean = False
    output = root / "mutation-5515-result.json"
    # No cache is saved. All phases use this checkout's own Cargo target.
    command = ["cargo", "test", "--locked", "-p", "oasis7_chain_runtime", "--lib",
               "--features", "test_tier_full", TEST, "--", "--exact", "--test-threads=1"]
    try:
        for phase, data, red in [("green", original, False), ("red", mutated, True),
                                 ("restored-green", original, False)]:
            path.write_bytes(data)
            start = time.monotonic()
            try:
                code, log = run(command, source, deadline)
            except (Cancelled, TimeoutError, ValueError, OSError, subprocess.SubprocessError):
                receipts.append({"phase": phase, "accepted": False,
                                 "execution_completed": False})
                raise
            accepted = classify(code, log, red)
            receipts.append({"phase": phase, "exit_code": code, "accepted": accepted,
                             "seconds": round(time.monotonic()-start, 3),
                             "source_file_sha256": hashlib.sha256(data).hexdigest(),
                             "output_sha256": hashlib.sha256(log).hexdigest(),
                             "output_bytes": len(log)})
            if not accepted:
                raise ValueError("phase did not satisfy exact test oracle: " + phase)
    finally:
        # Normal cancellation must not interrupt restoration half-way through.
        # entrypoint restores the original handler when this cleanup finishes.
        signal.signal(signal.SIGTERM, signal.SIG_IGN)
        path.write_bytes(original)
        restored = path.read_bytes() == original
        try:
            code, dirty = run(["git", "status", "--porcelain", "--untracked-files=no"],
                              source, time.monotonic()+5)
            tracked_clean = code == 0 and not dirty
        except (TimeoutError, ValueError, OSError, subprocess.SubprocessError):
            tracked_clean = False
        # Keep only structured diagnostic metadata; no world stores or raw logs.
        output.write_text(json.dumps({"source": SOURCE, "test": TEST,
                          "restored": restored, "tracked_clean": tracked_clean,
                          "phases": receipts}, indent=2)+"\n")
    if not restored or not tracked_clean:
        raise ValueError("tracked source restoration failed")
    print("one fixed mutation killed; both unmodified runs passed; diagnostic only")


if __name__ == "__main__":
    entrypoint()
