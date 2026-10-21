"""Queue lifecycle tests: real SQLite/locks, fake GitHub and model boundaries."""
import contextlib
import copy
import io
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import dispatcher as q


SHA = "a" * 40
MERGED_AT = "2026-10-21T10:00:00Z"


def comment(number, body, stamp="2026-10-21T09:00:00Z", user="dot"):
    return {"id": number, "body": body, "created_at": stamp, "updated_at": stamp,
            "user": {"login": user}}


def snapshot(number=7):
    return {"issue": {"number": number, "title": "Fix a Rust regression", "body": "code-development / Rust\nAcceptance: regression passes",
                      "state": "open", "labels": [{"name": q.READY}], "user": {"login": "dot"}},
            "comments": [], "reviews": [], "pr": None, "checks": [], "statuses": []}


def attach_pr(data, merged=False):
    data["pr"] = {"number": 42, "head": {"sha": SHA}, "state": "closed" if merged else "open",
                  "merged": merged, "merged_at": MERGED_AT if merged else None}
    data["checks"] = [{"id": 100, "name": "required", "status": "in_progress", "conclusion": None}]


class FakeGitHub:
    def __init__(self, config):
        self.config = config
        self.data = {7: snapshot()}
        self.notes = {}
        self.label_calls = []
        self.fail_read = False

    def trusted(self, item):
        return item.get("user", {}).get("login") in self.config.trusted_users

    def candidates(self):
        return [copy.deepcopy(x["issue"]) for x in self.data.values()
                if x["issue"]["state"] == "open" and q.READY in {l["name"] for l in x["issue"]["labels"]}]

    def eligible(self, issue):
        return self.trusted(issue) and "code-development" in issue["body"]

    def snapshot(self, number, branch):
        if self.fail_read:
            raise q.QueueError("GitHub unavailable")
        return copy.deepcopy(self.data[number])

    def issue(self, number):
        if self.fail_read:
            raise q.QueueError("GitHub unavailable")
        return copy.deepcopy(self.data[number]["issue"])

    def comments(self, number):
        return copy.deepcopy(self.data[number]["comments"])

    def labels(self, number, add=(), remove=()):
        self.label_calls.append((number, add, remove))
        labels = {x["name"] for x in self.data[number]["issue"]["labels"]}
        self.data[number]["issue"]["labels"] = [{"name": x} for x in sorted((labels | set(add)) - set(remove))]

    def note(self, number, key, text):
        self.notes.setdefault((number, key), text)


class FakeRuntime:
    def __init__(self):
        self.threads = []
        self.calls = []
        self.executions = []
        self.completed = set()
        self.after_start = None
        self.result = "completed"

    def __call__(self, **kwargs):
        self.calls.append(kwargs)
        thread = kwargs["thread_id"]
        if not thread:
            thread = "thread-" + str(len(self.threads) + 1)
            self.threads.append(thread)
        kwargs["on_thread"](thread)
        event = kwargs["event_id"]
        if event not in self.completed:
            self.executions.append(event)
            if self.after_start:
                self.after_start(kwargs)
            if self.result == "completed":
                self.completed.add(event)
        return {"thread_id": thread, "turn_id": "turn", "status": self.result, "message": "fixture"}


class QueueLifecycleTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        root = Path(self.tmp.name)
        self.path = root / "config.json"
        self.path.write_text(json.dumps({"repository": "nil-studio/oasis7", "checkout": str(root / "repo"),
                                        "state_dir": str(root / "state"), "trusted_users": ["dot", "codex"]}))
        self.config = q.Config.load(self.path)
        self.store = q.Store(self.config)
        self.addCleanup(self.store.close)
        self.github = FakeGitHub(self.config)
        self.spawns = []
        self.dispatch = q.Dispatcher(self.config, self.path, self.github, self.store,
                                     lambda issue, event: self.spawns.append((issue, event)))
        self.model = FakeRuntime()

    def work(self, prepare=lambda *_: None):
        issue, event = self.spawns[-1]
        return q.worker(self.config, issue, event, self.github, self.model, prepare)

    def resume(self, issue=None):
        with contextlib.redirect_stdout(io.StringIO()):
            args = ["--config", str(self.path), "resume"]
            if issue is not None:
                args += ["--issue", str(issue)]
            self.assertEqual(q.main(args), 0)

    def test_two_polls_and_duplicate_worker_do_not_create_another_thread(self):
        self.model.after_start = lambda _: attach_pr(self.github.data[7])
        self.assertEqual(self.dispatch.tick()["status"], "dispatched")
        first = self.spawns[0]
        with q.file_lock(self.config.state_dir / "executor.lock"):
            self.assertEqual(self.dispatch.tick()["status"], "worker_running")
            self.assertEqual(self.work()["status"], "worker_busy")
        self.assertEqual(self.work()["status"], "completed")
        self.assertEqual(self.dispatch.tick()["status"], "idle")
        self.assertEqual(self.dispatch.tick()["status"], "idle")
        self.assertEqual(q.worker(self.config, *first, self.github, self.model, lambda *_: None)["status"], "stale_dispatch")
        self.assertEqual(len(self.spawns), 1)
        self.assertEqual(self.model.threads, ["thread-1"])
        self.assertEqual(len(self.model.executions), 1)
        self.assertIn("thread-1", self.github.notes[(7, "session")])

    def test_crash_after_model_completion_reconciles_same_event_and_thread(self):
        def complete_then_crash(kwargs):
            attach_pr(self.github.data[7])
            self.model.completed.add(kwargs["event_id"])
            raise KeyboardInterrupt("worker killed before bookkeeping")
        self.model.after_start = complete_then_crash
        self.dispatch.tick()
        with self.assertRaises(KeyboardInterrupt):
            self.work()
        old = self.store.get(7)
        self.assertEqual(old["thread_id"], "thread-1")
        self.assertIsNotNone(old["pending_event"])
        self.assertEqual(self.dispatch.tick()["status"], "resuming")
        self.model.after_start = None
        self.assertEqual(self.work()["status"], "completed")
        self.assertEqual(self.model.calls[-1]["event_id"], old["pending_event"])
        self.assertEqual(self.model.calls[-1]["thread_id"], "thread-1")
        self.assertEqual(len(self.model.executions), 1)
        self.assertEqual(self.store.get(7)["turn_count"], 1)

    def test_interrupted_task_recovers_original_thread_via_explicit_entry(self):
        self.dispatch.tick()
        self.model.result = "interrupted"
        self.work()
        first = self.model.calls[-1]["event_id"]
        self.resume(7)
        self.assertEqual(self.dispatch.tick()["status"], "dispatched")
        self.model.result = "completed"
        self.model.after_start = lambda _: attach_pr(self.github.data[7])
        self.work()
        self.assertNotEqual(self.model.calls[-1]["event_id"], first)
        self.assertEqual(self.model.calls[-1]["thread_id"], "thread-1")
        self.assertEqual(len(self.model.threads), 1)
        self.assertEqual(self.store.get(7)["turn_count"], 2)

    def test_current_head_ci_failure_automatically_resumes_thread(self):
        self.dispatch.tick()
        self.model.after_start = lambda _: attach_pr(self.github.data[7])
        self.work()
        self.assertEqual(self.dispatch.tick()["status"], "idle")
        self.github.data[7]["checks"][0].update(status="completed", conclusion="failure")
        self.assertEqual(self.dispatch.tick()["status"], "dispatched")
        def repair(_):
            self.github.data[7]["pr"]["head"]["sha"] = "b" * 40
            self.github.data[7]["checks"][0].update(id=101, status="in_progress", conclusion=None)
        self.model.after_start = repair
        self.work()
        self.assertEqual(self.model.calls[-1]["thread_id"], "thread-1")
        self.assertIn('"reason": "feedback"', self.model.calls[-1]["prompt"])
        self.assertEqual(self.dispatch.tick()["status"], "idle")

    def test_global_pause_prevents_dispatch_and_resume_keeps_thread(self):
        self.dispatch.tick()
        def pause(kwargs):
            (self.config.state_dir / "STOP").touch()
            self.assertTrue(kwargs["should_stop"]())
        self.model.after_start = pause
        self.model.result = "interrupted"
        self.work()
        self.github.data[8] = snapshot(8)
        self.assertEqual(self.dispatch.tick()["status"], "paused")
        self.assertEqual(len(self.spawns), 1)
        self.assertIsNone(self.store.get(8))
        self.resume()
        self.model.after_start = None
        self.assertEqual(self.dispatch.tick()["status"], "dispatched")
        self.assertEqual(self.spawns[-1][0], 7)
        self.work()
        self.assertEqual(self.model.calls[-1]["thread_id"], "thread-1")

    def test_remote_pause_and_label_override_resume(self):
        self.github.data[7]["comments"] = [comment(1, "/codex pause")]
        self.assertEqual(self.dispatch.tick()["status"], "idle")
        self.assertEqual(self.spawns, [])
        self.github.data[7]["comments"].append(comment(2, "/codex resume"))
        self.github.data[7]["issue"]["labels"].append({"name": q.PAUSED})
        self.assertEqual(self.dispatch.tick()["status"], "idle")
        self.github.data[7]["issue"]["labels"] = [{"name": q.READY}]
        self.assertEqual(self.dispatch.tick()["status"], "dispatched")

    def test_remote_pause_observed_by_running_worker(self):
        self.dispatch.tick()
        def pause(kwargs):
            self.github.data[7]["comments"].append(comment(1, "/codex pause"))
            self.assertTrue(kwargs["should_stop"]())
        self.model.after_start, self.model.result = pause, "interrupted"
        self.work()
        self.assertEqual(self.dispatch.tick()["status"], "idle")
        self.assertEqual(len(self.model.executions), 1)

    def test_merge_and_model_completion_do_not_close_or_accept_issue(self):
        self.dispatch.tick()
        self.model.after_start = lambda _: attach_pr(self.github.data[7], merged=True)
        self.work()
        self.assertEqual(self.dispatch.tick()["status"], "idle")
        self.assertEqual(self.store.get(7)["status"], "awaiting_acceptance")
        data = self.github.data[7]
        data["comments"].append(comment(1, "/codex accept " + "b" * 40, "2026-10-21T11:00:00Z"))
        self.dispatch.tick()
        self.assertEqual(self.store.get(7)["status"], "awaiting_acceptance")
        data["comments"].append(comment(2, "/codex accept " + SHA, "2026-10-21T12:00:00Z"))
        self.dispatch.tick()
        self.assertEqual(self.store.get(7)["status"], "done")
        self.assertEqual(data["issue"]["state"], "open")
        self.assertEqual(len(self.model.executions), 1)

    def test_feedback_after_acceptance_invalidates_old_accept(self):
        row = self.store.add(7)
        attach_pr(self.github.data[7], merged=True)
        data = self.github.data[7]
        data["comments"] = [comment(1, "/codex accept " + SHA, "2026-10-21T11:00:00Z")]
        self.assertTrue(q.accepted(data))
        data["comments"].append(comment(2, "Regression remains on the accepted source", "2026-10-21T12:00:00Z"))
        self.assertFalse(q.accepted(data))
        self.assertIsNone(q.decide(data, row, self.config)[0])
        data["comments"].append(comment(3, "/codex fix " + SHA + "\nRegression reproduction and evidence", "2026-10-21T12:01:00Z"))
        self.assertEqual(q.decide(data, row, self.config)[0], "post-merge-feedback")

    def test_deployment_progress_or_wrong_build_does_not_dispatch_code_work(self):
        row = self.store.add(7)
        attach_pr(self.github.data[7], merged=True)
        data = self.github.data[7]
        data["comments"] = [comment(1, "Deployment is starting", "2026-10-21T11:00:00Z"),
                            comment(2, "/codex fix " + "b" * 40 + "\nFailure on an older build", "2026-10-21T12:00:00Z")]
        self.assertEqual(q.decide(data, row, self.config), (None, "awaiting_acceptance"))

    def test_progress_cannot_revive_a_fix_superseded_by_acceptance(self):
        row = self.store.add(7)
        attach_pr(self.github.data[7], merged=True)
        data = self.github.data[7]
        data["comments"] = [comment(1, "/codex fix " + SHA + "\nSuspected code failure", "2026-10-21T11:00:00Z"),
                            comment(2, "/codex accept " + SHA, "2026-10-21T12:00:00Z"),
                            comment(3, "A later deployment report", "2026-10-21T13:00:00Z")]
        self.assertFalse(q.accepted(data))
        self.assertEqual(q.decide(data, row, self.config), (None, "awaiting_acceptance"))
        data["comments"].append(comment(4, "/codex fix " + SHA + "\nNew confirmed reproduction", "2026-10-21T14:00:00Z"))
        self.assertEqual(q.decide(data, row, self.config)[0], "post-merge-feedback")

    def test_closed_without_acceptance_is_stopped_not_done(self):
        self.store.add(7)
        self.github.data[7]["issue"]["state"] = "closed"
        self.dispatch.tick()
        self.assertEqual(self.store.get(7)["status"], "closed")
        self.assertEqual(self.spawns, [])

    def test_no_progress_is_bounded_and_new_feedback_reuses_thread(self):
        for _ in range(2):
            self.assertEqual(self.dispatch.tick()["status"], "dispatched")
            self.work()
        self.assertEqual(self.dispatch.tick()["status"], "idle")
        self.assertEqual(self.store.get(7)["status"], "blocked")
        self.assertEqual(len(self.model.executions), 2)
        self.github.data[7]["comments"].append(comment(1, "Missing test fixture is now available"))
        self.assertEqual(self.dispatch.tick()["status"], "dispatched")
        self.work()
        self.assertEqual(self.model.calls[-1]["thread_id"], "thread-1")

    def test_preparation_failure_does_not_spin_every_poll(self):
        self.dispatch.tick()
        def missing(*_):
            raise FileNotFoundError("missing local executable; private detail")
        self.assertEqual(self.work(missing)["status"], "blocked")
        self.assertEqual(self.dispatch.tick()["status"], "idle")
        self.assertEqual(self.dispatch.tick()["status"], "idle")
        self.assertEqual(len(self.spawns), 1)
        self.assertEqual(self.model.executions, [])
        self.assertTrue(any("missing_dependency" in text for text in self.github.notes.values()))
        self.assertFalse(any("private detail" in text for text in self.github.notes.values()))
        self.resume(7)
        self.assertEqual(self.dispatch.tick()["status"], "dispatched")

    def test_frozen_unsent_input_is_superseded_before_calling_model(self):
        self.dispatch.tick()
        old_event = self.spawns[-1][1]
        self.github.data[7]["comments"].append(comment(1, "Corrected acceptance condition"))
        self.assertEqual(self.work()["status"], "input_changed")
        self.assertEqual(self.model.executions, [])
        self.dispatch.tick()
        self.assertNotEqual(self.spawns[-1][1], old_event)
        self.work()
        self.assertIn("Corrected acceptance", self.model.calls[-1]["prompt"])

    def test_per_issue_budget_is_not_reset_by_resume(self):
        self.store.add(7)
        self.store.update(7, turn_count=self.config.max_turns_per_issue)
        self.resume(7)
        self.dispatch.tick()
        self.assertEqual(self.spawns, [])
        self.assertEqual(self.store.get(7)["status"], "blocked")
        self.assertEqual(self.store.get(7)["turn_count"], 8)

    def test_two_dispatchers_and_two_issues_share_one_execution_slot(self):
        self.github.data[8] = snapshot(8)
        with q.file_lock(self.config.state_dir / "dispatch.lock"):
            self.assertEqual(self.dispatch.tick()["status"], "dispatcher_busy")
        self.dispatch.tick()
        with q.file_lock(self.config.state_dir / "executor.lock"):
            other = q.Dispatcher(self.config, self.path, self.github, self.store, self.dispatch.spawn)
            self.assertEqual(other.tick()["status"], "worker_running")
        self.assertEqual(len(self.spawns), 1)

    def test_remote_read_failure_never_dispatches_model(self):
        self.store.add(7)
        self.github.fail_read = True
        self.dispatch.tick()
        self.assertEqual(self.spawns, [])

    def test_unreadable_old_issue_does_not_starve_other_ready_work(self):
        self.store.add(7)
        self.github.data[8] = snapshot(8)
        original = self.github.snapshot
        def read(number, branch):
            if number == 7:
                raise q.QueueError("Old Issue temporarily unavailable")
            return original(number, branch)
        with patch.object(self.github, "snapshot", side_effect=read):
            self.assertEqual(self.dispatch.tick()["status"], "dispatched")
        self.assertEqual(self.spawns[-1][0], 8)
        self.assertIsNone(self.store.get(7)["pending_event"])


class GitHubBoundaryTests(unittest.TestCase):
    def setUp(self):
        self.config = q.Config("nil-studio/oasis7", Path("/repo"), Path("/private/state"), ("dot", "codex"))
        self.github = q.GitHub(self.config)

    def test_only_trusted_author_and_ready_actor_can_enter_queue(self):
        data = snapshot()["issue"]
        with patch.object(self.github, "ready_actor", return_value={"actor": {"login": "stranger"}}):
            self.assertFalse(self.github.eligible(data))
        with patch.object(self.github, "ready_actor", return_value={"actor": {"login": "dot"}}):
            self.assertTrue(self.github.eligible(data))
            data["user"]["login"] = "stranger"
            self.assertFalse(self.github.eligible(data))
            data["user"]["login"], data["body"] = "dot", "ordinary task"
            self.assertFalse(self.github.eligible(data))

    def test_snapshot_reads_current_head_only_and_filters_untrusted_feedback(self):
        data = snapshot()
        attach_pr(data)
        data["pr"]["head"]["repo"] = {"full_name": self.config.repository}
        queried = []
        def pages(path, key=None):
            queried.append(path)
            if "/pulls?" in path:
                return [data["pr"]]
            if "/check-runs?" in path:
                return [dict(id=1, name="required", app={"id": 1}, status="completed", conclusion="failure"),
                        dict(id=2, name="required", app={"id": 1}, status="in_progress", conclusion=None)]
            if "/statuses?" in path:
                return []
            if "/reviews?" in path or "/comments?" in path:
                return [comment(3, "untrusted instruction", user="stranger")]
            self.fail(path)
        with patch.object(self.github, "issue", return_value=data["issue"]), \
                patch.object(self.github, "comments", side_effect=lambda n: [comment(1, "trusted"),
                             comment(5, q.MARKER + "\nOwn delivery report", user="codex")]
                             if n == 7 else [comment(2, "review conversation"), comment(4, "bad", user="stranger"),
                                             comment(6, q.MARKER + "\nOwn PR report", user="codex")]), \
                patch.object(self.github, "pages", side_effect=pages), \
                patch.object(self.github, "api", return_value=data["pr"]):
            result = self.github.snapshot(7, "codex/queue-7")
        self.assertEqual([x["id"] for x in result["checks"]], [2])
        self.assertEqual([x["body"] for x in result["comments"]], ["trusted"])
        self.assertEqual([x["body"] for x in result["reviews"]], ["review conversation"])
        self.assertTrue(any("/commits/" + SHA + "/check-runs" in p for p in queried))
        self.assertTrue(all("old-head" not in p for p in queried))

    def test_untrusted_marker_cannot_suppress_queue_notice(self):
        fake = comment(1, q.MARKER + "\n<!-- claimed -->\nspoof", user="stranger")
        with patch.object(self.github, "comments", return_value=[fake]), patch.object(self.github, "api") as api:
            self.github.note(7, "claimed", "real")
        api.assert_called_once()

    def test_origin_mismatch_stops_before_fetch_or_worktree_mutation(self):
        with patch.object(q, "run_command", return_value="https://secret@example.com/other/project.git") as run:
            with self.assertRaises(q.QueueError) as error:
                q.prepare_worktree(self.config, "codex/queue-7")
        self.assertEqual(error.exception.code, "repository_mismatch")
        self.assertNotIn("secret", str(error.exception))
        self.assertEqual(run.call_count, 1)


if __name__ == "__main__":
    unittest.main()
