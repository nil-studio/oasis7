"""Recover across the real dispatcher, SQLite and App Server stdio boundary."""

import contextlib
import io
import json
import os
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

import dispatcher as q
from test_dispatcher import FakeGitHub, attach_pr, comment
from test_runtime import FAKE_SERVER, runtime


@unittest.skipUnless(os.name == "posix", "The worker uses POSIX execution locks")
class RecoveryTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.path = self.root / "config.json"
        self.path.write_text(json.dumps({
            "repository": "nil-studio/oasis7", "checkout": str(self.root / "repo"),
            "state_dir": str(self.root / "state"), "trusted_users": ["dot", "codex"],
        }))
        self.config = q.Config.load(self.path)
        self.store = q.Store(self.config)
        self.addCleanup(self.store.close)
        self.config.worktree.mkdir()
        self.fake = self.root / "fake_server.py"
        self.fake.write_text(FAKE_SERVER)
        self.github = FakeGitHub(self.config)
        self.spawns, self.calls = [], []
        self.dispatch = q.Dispatcher(self.config, self.path, self.github, self.store,
                                     lambda issue, event: self.spawns.append((issue, event)))
        self.mode = "normal"

    def execute(self, **kwargs):
        self.calls.append((kwargs["event_id"], kwargs["thread_id"]))
        save_thread = kwargs["on_thread"]

        def persisted(thread_id):
            save_thread(thread_id)
            (self.config.worktree / "thread_saved").write_text(thread_id)

        kwargs.update(codex_command=[sys.executable, str(self.fake), str(self.config.worktree),
                                     self.mode, str(kwargs["lock_fd"])], on_thread=persisted)
        with contextlib.ExitStack() as stack:
            stack.enter_context(patch.object(runtime, "INTERRUPT_GRACE_SECONDS", 0.15))
            if self.mode == "ignore_interrupt":
                wait_turn = runtime._Server.wait_turn

                def expire_after_submission(server):
                    # Exercise an uncertain in-flight interrupt without making
                    # process startup speed part of the test's precondition.
                    self.assertIsNotNone(server.turn_id)
                    server.deadline = 0
                    return wait_turn(server)

                stack.enter_context(patch.object(runtime._Server, "wait_turn", expire_after_submission))
            return runtime.execute(**kwargs)

    def work(self, prepare=lambda *_: None):
        return q.worker(self.config, *self.spawns[-1], self.github, self.execute, prepare)

    def resume(self, issue=7):
        args = ["--config", str(self.path), "resume"]
        if issue is not None:
            args += ["--issue", str(issue)]
        with contextlib.redirect_stdout(io.StringIO()):
            self.assertEqual(q.main(args), 0)

    def methods(self):
        path = self.config.worktree / "trace.jsonl"
        return [json.loads(line).get("method") for line in path.read_text().splitlines()]

    def settle_history(self, status):
        path = self.config.worktree / "history.json"
        state = json.loads(path.read_text())
        state["turns"][-1]["status"] = status
        path.write_text(json.dumps(state))

    def ambiguous_turn(self, mode):
        self.assertEqual(self.dispatch.tick()["status"], "dispatched")
        event = self.spawns[-1][1]
        self.mode = mode
        self.assertEqual(self.work()["status"], "blocked")
        row = self.store.get(7)
        self.assertEqual(row["pending_event"], event)
        self.assertIsNone(row["handled_event"])
        self.assertIsNone(row["handled_input"])
        self.assertEqual((row["thread_id"], row["turn_count"]), ("thread-1", 1))
        if mode == "ignore_interrupt":
            self.assertIn("turn/interrupt", self.methods())
        self.mode = "normal"
        return event

    def test_lost_completion_response_resumes_same_event_without_another_turn(self):
        event = self.ambiguous_turn("complete_disconnect")
        attach_pr(self.github.data[7])
        self.resume()
        self.assertEqual(self.dispatch.tick()["status"], "resuming")
        self.assertEqual(self.work()["status"], "completed")
        row = self.store.get(7)
        self.assertEqual(self.calls[-1], (event, "thread-1"))
        self.assertIsNone(row["pending_event"])
        self.assertEqual((row["handled_event"], row["turn_count"]), (event, 1))
        self.assertEqual(row["handled_generation"], row["retry_generation"])
        self.assertEqual(self.dispatch.tick()["status"], "idle")
        self.assertEqual(self.dispatch.tick()["status"], "idle")
        self.assertEqual(self.methods().count("thread/start"), 1)
        self.assertEqual(self.methods().count("turn/start"), 1)

    def recover_terminal(self, status, mode):
        event = self.ambiguous_turn(mode)
        self.resume()
        for _ in range(2):
            self.assertEqual(self.dispatch.tick()["status"], "resuming")
            self.assertEqual(self.work()["status"], "blocked")
            self.assertEqual(self.spawns[-1][1], event)
            self.assertEqual(self.store.get(7)["pending_event"], event)
            self.assertEqual(self.store.get(7)["turn_count"], 1)
        self.assertEqual(self.methods().count("turn/start"), 1)
        self.settle_history(status)
        self.assertEqual(self.dispatch.tick()["status"], "resuming")
        self.assertEqual(self.work()["status"], status)
        self.assertIsNone(self.store.get(7)["pending_event"])
        self.assertEqual(self.store.get(7)["handled_event"], event)
        self.assertEqual(self.methods().count("turn/start"), 1)
        # The earlier explicit recovery request may start a new event only now
        # that history has proved the old event terminal.
        self.assertEqual(self.dispatch.tick()["status"], "dispatched")
        self.assertNotEqual(self.spawns[-1][1], event)
        self.assertEqual(self.work()["status"], "completed")
        self.assertEqual(self.calls[-1][1], "thread-1")
        self.assertEqual(self.methods().count("thread/start"), 1)
        self.assertEqual(self.methods().count("turn/start"), 2)
        self.assertEqual(self.store.get(7)["turn_count"], 2)

    def test_disconnect_remains_pending_until_failed_turn_is_reconciled(self):
        self.recover_terminal("failed", "disconnect")

    def test_unconfirmed_interrupt_remains_pending_until_terminal_history(self):
        self.recover_terminal("interrupted", "ignore_interrupt")

    def test_recovery_preparation_or_github_failure_preserves_pending_event(self):
        event = self.ambiguous_turn("complete_disconnect")
        self.resume()
        self.assertEqual(self.dispatch.tick()["status"], "resuming")

        def unavailable(*_):
            raise FileNotFoundError("temporary preparation dependency unavailable")

        self.assertEqual(self.work(prepare=unavailable)["status"], "blocked")
        self.assertEqual(self.store.get(7)["pending_event"], event)
        self.github.fail_read = True
        self.assertEqual(self.work()["status"], "blocked")
        self.assertEqual(self.store.get(7)["pending_event"], event)
        self.github.fail_read = False
        attach_pr(self.github.data[7])
        self.assertEqual(self.work()["status"], "completed")
        self.assertEqual(self.calls[-1], (event, "thread-1"))
        self.assertEqual(self.methods().count("turn/start"), 1)
        self.assertEqual(self.store.get(7)["turn_count"], 1)
        self.assertEqual(self.dispatch.tick()["status"], "idle")

    def test_local_and_remote_pause_keep_pending_identity_without_dispatch(self):
        event = self.ambiguous_turn("disconnect")
        (self.config.state_dir / "STOP").touch()
        self.assertEqual(self.dispatch.tick()["status"], "paused")
        self.assertEqual(self.work()["status"], "paused")
        self.assertEqual(self.store.get(7)["pending_event"], event)
        self.assertEqual(len(self.spawns), 1)
        self.resume(issue=None)
        self.github.data[7]["comments"].append(comment(1, "/codex pause"))
        self.assertEqual(self.dispatch.tick()["status"], "idle")
        self.assertEqual(self.work()["status"], "paused")
        self.assertEqual(self.store.get(7)["pending_event"], event)
        self.assertEqual(len(self.spawns), 1)
        self.github.data[7]["comments"].append(comment(2, "/codex resume"))
        self.settle_history("completed")
        attach_pr(self.github.data[7])
        self.assertEqual(self.dispatch.tick()["status"], "resuming")
        self.assertEqual(self.work()["status"], "completed")
        self.assertEqual(self.calls[-1], (event, "thread-1"))
        self.assertEqual(self.dispatch.tick()["status"], "idle")
        self.assertEqual(self.methods().count("turn/start"), 1)

    def test_reconciliation_does_not_consume_new_development_feedback(self):
        event = self.ambiguous_turn("complete_disconnect")
        attach_pr(self.github.data[7])
        self.github.data[7]["comments"].append(comment(1, "An additional acceptance case still fails"))
        self.resume()
        self.assertEqual(self.dispatch.tick()["status"], "resuming")
        self.assertEqual(self.work()["status"], "completed")
        self.assertEqual(self.methods().count("turn/start"), 1)
        self.assertEqual(self.dispatch.tick()["status"], "dispatched")
        self.assertNotEqual(self.spawns[-1][1], event)
        self.assertEqual(self.work()["status"], "completed")
        self.assertEqual(self.calls[-1][1], "thread-1")
        self.assertEqual(self.methods().count("turn/start"), 2)

    def test_budget_limit_still_allows_reading_original_completion(self):
        config = json.loads(self.path.read_text())
        config["max_turns_per_issue"] = 1
        self.path.write_text(json.dumps(config))
        self.config = q.Config.load(self.path)
        self.store.config = self.dispatch.config = self.config
        event = self.ambiguous_turn("complete_disconnect")
        attach_pr(self.github.data[7])
        self.resume()
        self.assertEqual(self.dispatch.tick()["status"], "resuming")
        self.assertEqual(self.work()["status"], "completed")
        self.assertEqual(self.calls[-1], (event, "thread-1"))
        self.assertEqual(self.methods().count("turn/start"), 1)
        self.assertEqual(self.store.get(7)["turn_count"], 1)
        self.github.data[7]["comments"].append(comment(1, "A real new regression"))
        self.assertEqual(self.dispatch.tick()["status"], "idle")
        self.assertEqual(self.store.get(7)["status"], "blocked")
        self.assertEqual(self.store.get(7)["turn_count"], 1)

    def test_recovery_does_not_consume_requests_arriving_after_its_snapshot(self):
        event = self.ambiguous_turn("complete_disconnect")
        attach_pr(self.github.data[7])
        self.resume()
        generation = self.store.get(7)["retry_generation"]
        self.assertEqual(self.dispatch.tick()["status"], "resuming")

        def concurrent_request(**kwargs):
            self.resume()
            self.github.data[7]["comments"].append(comment(1, "New feedback during recovery"))
            return self.execute(**kwargs)

        result = q.worker(self.config, *self.spawns[-1], self.github,
                          concurrent_request, lambda *_: None)
        self.assertEqual(result["status"], "completed")
        row = self.store.get(7)
        self.assertEqual(row["handled_generation"], generation)
        self.assertEqual(row["retry_generation"], generation + 1)
        self.assertNotEqual(row["handled_input"], q.inputs(self.github.data[7]))
        self.assertEqual(self.methods().count("turn/start"), 1)
        self.assertEqual(self.dispatch.tick()["status"], "dispatched")
        self.assertNotEqual(self.spawns[-1][1], event)
        self.assertEqual(self.work()["status"], "completed")
        self.assertEqual(self.methods().count("turn/start"), 2)


if __name__ == "__main__":
    unittest.main()
