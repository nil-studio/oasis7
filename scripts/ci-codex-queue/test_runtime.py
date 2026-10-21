"""Exercise the real stdio transport with an isolated, non-model fixture."""

import fcntl
import importlib.util
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile
import time
import unittest
from unittest.mock import patch


HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location("queue_runtime", HERE / "runtime.py")
runtime = importlib.util.module_from_spec(spec)
spec.loader.exec_module(runtime)


FAKE_SERVER = r'''
import json, os, subprocess, sys, time
from pathlib import Path
root = Path(sys.argv[1])
mode = sys.argv[2]
lock_fd = int(sys.argv[3])
os.fstat(lock_fd)
(root / "server.json").write_text(json.dumps({"pid": os.getpid(), "lock_fd": lock_fd}))
state_file = root / "history.json"
state = json.loads(state_file.read_text()) if state_file.exists() else {"turns": []}
def save():
    state_file.write_text(json.dumps(state))
def send(message):
    print(json.dumps(message), flush=True)
def reply(message, result):
    send({"id": message["id"], "result": result})
def finish(status):
    state["turns"][-1]["status"] = status
    save()
    send({"method": "turn/completed", "params": {"threadId": "thread-1", "turn": state["turns"][-1]}})
for line in sys.stdin:
    message = json.loads(line)
    with (root / "trace.jsonl").open("a") as output:
        output.write(json.dumps(message) + "\n")
    method = message.get("method")
    params = message.get("params", {})
    if method == "initialize":
        reply(message, {"userAgent": "isolated fixture"})
    elif method == "initialized":
        pass
    elif method == "thread/start":
        reply(message, {"thread": {"id": "thread-1", "ephemeral": False}})
    elif method == "thread/read":
        identity = "wrong-thread" if mode == "wrong_identity" else "thread-1"
        reply(message, {"thread": {"id": identity, "ephemeral": False, "cwd": str(root),
                                  "status": {"type": state.get("thread_status", "notLoaded")}}})
    elif method == "thread/resume":
        reply(message, {"thread": {"id": "thread-1", "ephemeral": False, "status": {"type": "idle"}}})
    elif method == "thread/turns/list":
        if mode == "unsupported_history":
            send({"id": message["id"], "error": {"code": -32601, "message": "unsupported"}})
        elif state.get("paginate") and not params.get("cursor"):
            reply(message, {"data": [], "nextCursor": "page-2"})
        else:
            reply(message, {"data": list(reversed(state["turns"])), "nextCursor": None})
    elif method == "turn/start":
        if not (root / "thread_saved").exists():
            send({"id": message["id"], "error": {"code": 9, "message": "thread was not saved"}})
            continue
        turn_id = "turn-" + str(len(state["turns"]) + 1)
        turn = {"id": turn_id, "status": "inProgress", "items": [
            {"type": "userMessage", "id": "user-1", "content": params["input"]}]}
        state["turns"].append(turn)
        save()
        if mode == "orphan_writer":
            # A command can close the inherited lock and survive app-server.
            # Keep its process group, but ignore TERM to exercise escalation.
            child_code = """import os, signal, sys, time
from pathlib import Path
root = Path(sys.argv[1])
signal.signal(signal.SIGTERM, signal.SIG_IGN)
(root / "writer.pid").write_text(str(os.getpid()))
while True:
    with (root / "writes").open("a") as stream:
        stream.write("still running\\n")
    time.sleep(0.01)
"""
            subprocess.Popen([sys.executable, "-c", child_code, str(root)],
                             stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL,
                             stderr=subprocess.DEVNULL, close_fds=True)
            deadline = time.monotonic() + 2
            while not (root / "writes").exists() and time.monotonic() < deadline:
                time.sleep(0.01)
            sys.exit(0)
        if mode == "disconnect":
            sys.exit(0)
        if mode == "early_completed":
            finish("completed")
        reply(message, {"turn": {"id": turn_id, "status": "inProgress"}})
        send({"method": "turn/started", "params": {"threadId": "thread-1", "turn": {"id": turn_id, "status": "inProgress"}}})
        if mode.startswith("approval:"):
            send({"id": 9007199254740993, "method": mode.split(":", 1)[1],
                  "params": {"threadId": "thread-1", "turnId": turn_id}})
        elif mode == "partial":
            sys.stdout.write('{"method":')
            sys.stdout.flush()
        elif mode == "failed":
            finish("failed")
        elif mode not in {"hang", "orphan", "ignore_interrupt", "early_completed"}:
            finish("completed")
    elif method == "turn/interrupt":
        reply(message, {})
        if mode != "ignore_interrupt":
            finish("interrupted")
if mode == "orphan":
    while True:
        time.sleep(0.1)
'''


@unittest.skipUnless(os.name == "posix", "The worker uses POSIX execution locks")
class RuntimeTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.fake = self.root / "fake_server.py"
        self.fake.write_text(FAKE_SERVER)
        self.lock = (self.root / "executor.lock").open("a+")
        self.addCleanup(self.lock.close)
        fcntl.flock(self.lock.fileno(), fcntl.LOCK_EX | fcntl.LOCK_NB)

    def trace(self):
        path = self.root / "trace.jsonl"
        return [json.loads(line) for line in path.read_text().splitlines()] if path.exists() else []

    def methods(self):
        return [message.get("method") for message in self.trace()]

    def save_thread(self, thread_id):
        (self.root / "thread_saved").write_text(thread_id)

    def run_turn(self, mode="normal", thread_id=None, event_id="event-1", **overrides):
        arguments = {
            "codex_command": [sys.executable, str(self.fake), str(self.root), mode, str(self.lock.fileno())],
            "cwd": str(self.root), "prompt": "Fix the bounded test failure", "thread_id": thread_id,
            "event_id": event_id, "on_thread": self.save_thread, "should_stop": lambda: False,
            "lock_fd": self.lock.fileno(), "timeout_seconds": 5,
        }
        arguments.update(overrides)
        return runtime.execute(**arguments)

    def stored_turn(self, status="completed", event_id="event-1"):
        return {"id": "turn-1", "status": status, "items": [
            {"type": "userMessage", "content": [{"type": "text", "text": runtime.event_marker(event_id) + "\nprior input"}]}]}

    def history(self, turns, **other):
        (self.root / "history.json").write_text(json.dumps({"turns": turns, **other}))
        self.save_thread("thread-1")

    def test_persist_before_start_and_recover_completed_event_without_replay(self):
        first = self.run_turn()
        self.assertEqual((first["status"], first["thread_id"], first["turn_id"]),
                         ("completed", "thread-1", "turn-1"))
        recovered = self.run_turn(thread_id="thread-1")
        self.assertEqual(recovered["status"], "completed")
        self.assertEqual(recovered["turn_id"], "turn-1")
        self.assertEqual(self.methods().count("thread/start"), 1)
        self.assertEqual(self.methods().count("turn/start"), 1)
        self.assertNotIn("thread/resume", self.methods())
        start = next(message for message in self.trace() if message.get("method") == "thread/start")
        self.assertEqual(start["params"]["approvalPolicy"], "never")
        self.assertEqual(start["params"]["sandbox"], "workspaceWrite")
        self.assertNotIn("model", start["params"])

    def test_new_event_resumes_same_thread(self):
        self.history([self.stored_turn(event_id="old-event")])
        result = self.run_turn(thread_id="thread-1")
        self.assertEqual(result["status"], "completed")
        self.assertEqual(result["thread_id"], "thread-1")
        self.assertEqual(result["turn_id"], "turn-2")
        self.assertEqual(self.methods().count("thread/resume"), 1)
        self.assertNotIn("thread/start", self.methods())

    def test_recovery_does_not_replay_failed_interrupted_or_uncertain_event(self):
        for status in ("failed", "interrupted", "inProgress", "unknown"):
            with self.subTest(status=status):
                self.history([self.stored_turn(status)])
                result = self.run_turn(thread_id="thread-1")
                self.assertEqual(result["status"], "blocked")
                self.assertEqual(result["turn_id"], "turn-1")
        self.assertNotIn("turn/start", self.methods())

    def test_unrelated_active_turn_prevents_new_execution(self):
        self.history([self.stored_turn("inProgress", "another-event")])
        result = self.run_turn(thread_id="thread-1")
        self.assertEqual(result["status"], "blocked")
        self.assertNotIn("thread/resume", self.methods())
        self.assertNotIn("turn/start", self.methods())

    def test_incomplete_turn_or_thread_status_cannot_be_treated_as_idle(self):
        self.history([{ "id": "turn-1", "status": "completed", "items": []}])
        self.assertEqual(self.run_turn(thread_id="thread-1")["status"], "blocked")
        missing_status = self.stored_turn()
        del missing_status["status"]
        self.history([missing_status])
        self.assertEqual(self.run_turn(thread_id="thread-1")["status"], "blocked")
        self.history([], thread_status=None)
        self.assertEqual(self.run_turn(thread_id="thread-1")["status"], "blocked")
        self.assertNotIn("turn/start", self.methods())

    def test_history_pagination_finds_completed_event(self):
        self.history([self.stored_turn()], paginate=True)
        result = self.run_turn(thread_id="thread-1")
        self.assertEqual(result["status"], "completed")
        self.assertEqual(self.methods().count("thread/turns/list"), 2)
        self.assertNotIn("turn/start", self.methods())

    def test_missing_history_capability_and_wrong_identity_fail_closed(self):
        self.history([])
        for mode in ("unsupported_history", "wrong_identity"):
            with self.subTest(mode=mode):
                self.assertEqual(self.run_turn(mode, thread_id="thread-1")["status"], "blocked")
        self.assertNotIn("turn/start", self.methods())
        self.assertNotIn("thread/start", self.methods())

    def test_failed_persistence_never_sends_turn(self):
        def failed_callback(_):
            raise OSError("simulated disk full")
        result = self.run_turn(on_thread=failed_callback)
        self.assertEqual(result["status"], "blocked")
        self.assertEqual(result["thread_id"], "thread-1")
        self.assertNotIn("turn/start", self.methods())

    def test_completion_notification_can_precede_start_response(self):
        self.assertEqual(self.run_turn("early_completed")["status"], "completed")

    def test_pausing_interrupts_the_actual_turn_and_waits_for_terminal_event(self):
        started = time.monotonic()
        result = self.run_turn("hang", should_stop=lambda: time.monotonic() - started > 0.3)
        self.assertLess(time.monotonic() - started, 2)
        self.assertEqual(result["status"], "interrupted")
        interrupt = next(message for message in self.trace() if message.get("method") == "turn/interrupt")
        self.assertEqual(interrupt["params"], {"threadId": "thread-1", "turnId": "turn-1"})

    def test_partial_stdout_cannot_block_stop_checks(self):
        started = time.monotonic()
        with patch.object(runtime, "INTERRUPT_GRACE_SECONDS", 0.15):
            result = self.run_turn("partial", should_stop=lambda: time.monotonic() - started > 0.3)
        self.assertLess(time.monotonic() - started, 2)
        self.assertEqual(result["status"], "blocked")
        self.assertIn("turn/interrupt", self.methods())

    def test_timeout_and_unconfirmed_interrupt_remain_blocked(self):
        with patch.object(runtime, "INTERRUPT_GRACE_SECONDS", 0.15):
            result = self.run_turn("ignore_interrupt", timeout_seconds=0.3)
        self.assertEqual(result["status"], "blocked")
        self.assertIn("timed out", result["message"])
        self.assertEqual(self.methods().count("turn/start"), 1)
        self.assertIn("turn/interrupt", self.methods())

    def test_approval_requests_are_denied_and_never_report_completed(self):
        for method, expected in (
            ("item/commandExecution/requestApproval", {"decision": "decline"}),
            ("item/fileChange/requestApproval", {"decision": "decline"}),
            ("item/permissions/requestApproval", {"permissions": {}, "scope": "turn"}),
            ("mcpServer/elicitation/request", {"action": "decline", "content": None}),
        ):
            with self.subTest(method=method):
                result = self.run_turn("approval:" + method)
                self.assertEqual(result["status"], "blocked")
                reply = [message for message in self.trace() if message.get("id") == 9007199254740993][-1]
                self.assertEqual(reply["result"], expected)
        self.assertEqual(self.methods().count("turn/interrupt"), 4)

    def test_disconnect_after_submission_cannot_start_another_turn(self):
        interrupted = self.run_turn("disconnect")
        self.assertEqual(interrupted["status"], "blocked")
        self.assertEqual(interrupted["thread_id"], "thread-1")
        recovered = self.run_turn(thread_id="thread-1")
        self.assertEqual(recovered["status"], "blocked")
        self.assertEqual(self.methods().count("turn/start"), 1)

    def test_missing_lock_or_preexisting_stop_never_launches_server(self):
        self.assertEqual(self.run_turn(lock_fd=-1)["status"], "blocked")
        self.assertEqual(self.run_turn(should_stop=lambda: True)["status"], "blocked")
        self.assertFalse((self.root / "server.json").exists())

    def test_exited_app_server_cannot_leave_a_command_writing_the_workspace(self):
        try:
            result = self.run_turn("orphan_writer")
            self.assertEqual(result["status"], "blocked")
            self.assertTrue((self.root / "writer.pid").exists())
            time.sleep(0.05)
            previous = (self.root / "writes").read_bytes()
            time.sleep(0.15)
            self.assertEqual((self.root / "writes").read_bytes(), previous,
                             "app-server exited but its command continues writing")
        finally:
            info = self.root / "server.json"
            if info.exists():
                try:
                    os.killpg(json.loads(info.read_text())["pid"], signal.SIGKILL)
                except ProcessLookupError:
                    pass

    def test_lock_survives_worker_sigkill_until_app_server_exits(self):
        # The subprocess, not this test process, must own the open-file lock.
        self.lock.close()
        worker_code = r'''
import fcntl, importlib.util, sys
from pathlib import Path
spec = importlib.util.spec_from_file_location("runtime", sys.argv[1])
runtime = importlib.util.module_from_spec(spec)
spec.loader.exec_module(runtime)
root = Path(sys.argv[2])
lock = (root / "executor.lock").open("a+")
fcntl.flock(lock.fileno(), fcntl.LOCK_EX | fcntl.LOCK_NB)
runtime.execute([sys.executable, str(root / "fake_server.py"), str(root), "orphan", str(lock.fileno())],
                str(root), "fixture", None, "event-1",
                lambda thread: (root / "thread_saved").write_text(thread),
                lambda: False, lock.fileno(), 10)
'''
        worker = subprocess.Popen([sys.executable, "-c", worker_code, str(HERE / "runtime.py"), str(self.root)])
        server_pid = None
        try:
            deadline = time.monotonic() + 3
            while "turn/start" not in self.methods():
                self.assertLess(time.monotonic(), deadline, "fake server did not start")
                time.sleep(0.02)
            server_pid = json.loads((self.root / "server.json").read_text())["pid"]
            worker.kill()
            worker.wait(timeout=2)
            with (self.root / "executor.lock").open("a+") as contender:
                with self.assertRaises(BlockingIOError):
                    fcntl.flock(contender.fileno(), fcntl.LOCK_EX | fcntl.LOCK_NB)
                os.killpg(server_pid, signal.SIGTERM)
                deadline = time.monotonic() + 3
                while True:
                    try:
                        fcntl.flock(contender.fileno(), fcntl.LOCK_EX | fcntl.LOCK_NB)
                        break
                    except BlockingIOError:
                        self.assertLess(time.monotonic(), deadline, "executor lock was not released")
                        time.sleep(0.02)
        finally:
            if worker.poll() is None:
                worker.kill()
                worker.wait()
            if server_pid:
                try:
                    os.killpg(server_pid, signal.SIGKILL)
                except ProcessLookupError:
                    pass


if __name__ == "__main__":
    unittest.main()
