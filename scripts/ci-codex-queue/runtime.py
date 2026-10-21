"""One bounded, persistent Codex App Server turn, using only the stdlib.

Protocol reference: https://learn.chatgpt.com/docs/app-server (2026-10-21).
The caller owns the execution flock and must close, never explicitly unlock,
its descriptor while a child may still own it. The descriptor is inherited by
app-server so a killed worker cannot immediately create a second executor.
"""

from __future__ import annotations

import hashlib
import json
import os
from pathlib import Path
import select
import selectors
import signal
import stat
import subprocess
import time
from typing import Callable


MAX_MESSAGE_BYTES = 8 * 1024 * 1024
MAX_HISTORY_PAGES = 100
INTERRUPT_GRACE_SECONDS = 5
RPC_TIMEOUT_SECONDS = 30
TERMINAL = {"completed", "failed", "interrupted"}


class _Blocked(Exception):
    pass


class _Stopped(_Blocked):
    pass


def event_marker(event_id: str) -> str:
    """An exact first-line marker, never a substring of tool/agent output."""
    digest = hashlib.sha256(event_id.encode("utf-8")).hexdigest()
    return f"[oasis7-ci-queue-event:{digest}]"


def _identifier(value: object, kind: str) -> str:
    if not isinstance(value, str) or not value:
        raise _Blocked(f"App Server returned no valid {kind} identity")
    return value


class _Server:
    def __init__(self, command, cwd, lock_fd, should_stop, timeout_seconds):
        self.deadline = time.monotonic() + timeout_seconds
        self.should_stop = should_stop
        self.cancelling = False
        self.thread_id = None
        self.turn_id = None
        self.starting_turn = False
        self.terminal_turns = {}
        self.blocked_request = None
        self.sequence = 0
        self.buffer = bytearray()
        self.stdout_eof = False
        # Do not use shell=True, modify config, change models, or grant access.
        self.process = subprocess.Popen(
            command, cwd=cwd, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
            stderr=subprocess.PIPE, bufsize=0, start_new_session=True,
            pass_fds=(lock_fd,),
        )
        self.selector = selectors.DefaultSelector()
        for pipe in (self.process.stdin, self.process.stdout, self.process.stderr):
            os.set_blocking(pipe.fileno(), False)
        self.selector.register(self.process.stdout, selectors.EVENT_READ, "stdout")
        self.selector.register(self.process.stderr, selectors.EVENT_READ, "stderr")

    def check(self):
        if not self.cancelling:
            try:
                stopped = self.should_stop()
            except Exception as exc:
                raise _Stopped("Pause state could not be read") from exc
            if stopped:
                raise _Stopped("Queue stop requested")
        if time.monotonic() >= self.deadline:
            raise _Stopped("App Server execution timed out")

    def send(self, message):
        payload = memoryview((json.dumps(message, ensure_ascii=False) + "\n").encode())
        if len(payload) > MAX_MESSAGE_BYTES:
            raise _Blocked("App Server request exceeds the supported size")
        while payload:
            self.check()
            try:
                written = os.write(self.process.stdin.fileno(), payload)
                payload = payload[written:]
            except BlockingIOError:
                select.select([], [self.process.stdin], [], 0.1)
            except (BrokenPipeError, OSError) as exc:
                raise _Blocked("App Server input closed; execution state is unknown") from exc

    def read(self):
        while True:
            self.check()
            newline = self.buffer.find(b"\n")
            if newline >= 0:
                line = bytes(self.buffer[:newline])
                del self.buffer[:newline + 1]
                try:
                    message = json.loads(line)
                except (ValueError, UnicodeDecodeError) as exc:
                    raise _Blocked("App Server emitted invalid JSON") from exc
                if not isinstance(message, dict):
                    raise _Blocked("App Server emitted an invalid protocol envelope")
                return message
            if self.stdout_eof:
                raise _Blocked("App Server output closed; execution state is unknown")
            for key, _ in self.selector.select(0.1):
                try:
                    data = os.read(key.fileobj.fileno(), 65536)
                except BlockingIOError:
                    continue
                if not data:
                    self.selector.unregister(key.fileobj)
                    if key.data == "stdout":
                        self.stdout_eof = True
                elif key.data == "stdout":
                    self.buffer.extend(data)
                    if len(self.buffer) > MAX_MESSAGE_BYTES:
                        raise _Blocked("App Server output exceeds the supported size")
                # Drain stderr without copying credentials or user text into logs.

    def dispatch(self, message):
        method = message.get("method")
        if not method:
            return
        params = message.get("params") or {}
        if not isinstance(params, dict):
            raise _Blocked("App Server emitted invalid notification parameters")
        if "id" in message:
            self.blocked_request = f"Unattended execution requires a user decision: {method}"
            if method in {"item/commandExecution/requestApproval", "item/fileChange/requestApproval"}:
                reply = {"result": {"decision": "decline"}}
            elif method == "item/permissions/requestApproval":
                reply = {"result": {"permissions": {}, "scope": "turn"}}
            elif method == "mcpServer/elicitation/request":
                reply = {"result": {"action": "decline", "content": None}}
            else:
                reply = {"error": {"code": -32601, "message": "Unattended queue cannot answer this request"}}
            self.send({"id": message["id"], **reply})
            if params.get("threadId") == self.thread_id and self.starting_turn and not self.turn_id:
                self.turn_id = params.get("turnId")
            return
        if method not in {"turn/started", "turn/completed"}:
            return
        if params.get("threadId", self.thread_id) != self.thread_id:
            return
        turn = params.get("turn")
        if not isinstance(turn, dict):
            raise _Blocked("App Server emitted an invalid turn")
        turn_id = _identifier(turn.get("id"), "turn")
        if method == "turn/started" and self.starting_turn and self.turn_id is None:
            self.turn_id = turn_id
        if method == "turn/completed":
            if turn.get("status") not in TERMINAL:
                raise _Blocked("App Server completion has an unknown status")
            if len(self.terminal_turns) >= 10:
                raise _Blocked("Unexpected concurrent turn notifications")
            self.terminal_turns[turn_id] = turn

    def request(self, method, params):
        self.sequence += 1
        request_id = self.sequence
        self.send({"id": request_id, "method": method, "params": params})
        request_deadline = min(self.deadline, time.monotonic() + RPC_TIMEOUT_SECONDS)
        previous_deadline = self.deadline
        self.deadline = request_deadline
        try:
            while True:
                message = self.read()
                if message.get("id") == request_id and "method" not in message:
                    if "error" in message:
                        error = message["error"]
                        code = error.get("code", "unknown") if isinstance(error, dict) else "unknown"
                        raise _Blocked(f"App Server {method} failed (code {code}); no fallback was started")
                    result = message.get("result")
                    if not isinstance(result, dict):
                        raise _Blocked(f"App Server {method} returned an invalid result")
                    return result
                self.dispatch(message)
                if self.blocked_request:
                    raise _Blocked(self.blocked_request)
        finally:
            self.deadline = previous_deadline

    def wait_turn(self):
        while self.turn_id not in self.terminal_turns:
            if self.blocked_request:
                raise _Blocked(self.blocked_request)
            self.dispatch(self.read())
        if self.blocked_request:
            raise _Blocked(self.blocked_request)
        return self.terminal_turns[self.turn_id]

    def interrupt(self):
        """An interrupt acknowledgement alone does not prove cancellation."""
        if not self.turn_id or not self.starting_turn:
            return None
        self.cancelling = True
        self.deadline = time.monotonic() + INTERRUPT_GRACE_SECONDS
        try:
            self.sequence += 1
            self.send({"id": self.sequence, "method": "turn/interrupt", "params": {
                "threadId": self.thread_id, "turnId": self.turn_id,
            }})
            while self.turn_id not in self.terminal_turns:
                self.dispatch(self.read())
            return self.terminal_turns[self.turn_id]
        except (OSError, _Blocked):
            return None

    def close(self):
        # Clean our whole process group, including commands whose app-server
        # parent has already exited. Signal before reaping the leader so its
        # PID cannot be recycled for an unrelated new process group.
        try:
            self.process.stdin.close()
        except OSError:
            pass
        try:
            os.killpg(self.process.pid, signal.SIGTERM)
        except ProcessLookupError:
            pass
        # Keep the leader unreaped through the bounded grace interval. Even if
        # it has exited, this reserves its PID until the final group signal.
        time.sleep(0.1)
        # An exited leader says nothing about its surviving commands. A command
        # may have closed its lock FD and may ignore TERM; never skip escalation
        # merely because process.poll()/wait() reports the leader has exited.
        try:
            os.killpg(self.process.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        try:
            self.process.wait(timeout=1)
        finally:
            self.selector.close()
            self.process.stdout.close()
            self.process.stderr.close()


def _read_thread(server, thread_id, cwd):
    result = server.request("thread/read", {"threadId": thread_id})
    thread = result.get("thread")
    if not isinstance(thread, dict) or thread.get("id") != thread_id:
        raise _Blocked("Stored thread identity could not be verified")
    if thread.get("ephemeral") is True:
        raise _Blocked("Ephemeral threads cannot be used for recovery")
    if thread.get("cwd") and Path(thread["cwd"]).resolve() != Path(cwd).resolve():
        raise _Blocked("Stored thread belongs to a different workspace")
    return thread


def _thread_status(thread):
    status = thread.get("status")
    return status.get("type") if isinstance(status, dict) else None


def _history(server, marker):
    matches = []
    cursor = None
    seen_cursors = set()
    uncertain_turn = False
    for _ in range(MAX_HISTORY_PAGES):
        params = {"threadId": server.thread_id, "limit": 50,
                  "sortDirection": "desc", "itemsView": "full"}
        if cursor is not None:
            params["cursor"] = cursor
        page = server.request("thread/turns/list", params)
        turns = page.get("data")
        if not isinstance(turns, list) or "nextCursor" not in page:
            raise _Blocked("Stored turn history is incomplete")
        for turn in turns:
            if not isinstance(turn, dict) or not isinstance(turn.get("items"), list):
                raise _Blocked("Stored turn items are unavailable")
            _identifier(turn.get("id"), "stored turn")
            if turn.get("status") not in TERMINAL:
                uncertain_turn = True
            # This is a dedicated user-input queue thread. Missing user input
            # cannot prove that a submitted event is absent from its history.
            user_items = [item for item in turn["items"]
                          if isinstance(item, dict) and item.get("type") == "userMessage"]
            if not user_items or any(not isinstance(item.get("content"), list) for item in user_items):
                raise _Blocked("Stored user input is unavailable; event recovery is uncertain")
            found = any(
                isinstance(item, dict) and item.get("type") == "userMessage"
                and isinstance(item.get("content"), list)
                and any(isinstance(part, dict) and part.get("type") == "text"
                        and isinstance(part.get("text"), str)
                        and part["text"].split("\n", 1)[0] == marker
                        for part in item["content"])
                for item in turn["items"]
            )
            if found:
                matches.append(turn)
        cursor = page["nextCursor"]
        if cursor is None:
            return matches, uncertain_turn
        if not isinstance(cursor, str) or not cursor or cursor in seen_cursors:
            raise _Blocked("Stored turn history pagination is inconsistent")
        seen_cursors.add(cursor)
    raise _Blocked("Stored turn history exceeds the recovery bound")


def execute(
    codex_command: list[str], cwd: str, prompt: str, thread_id: str | None,
    event_id: str, on_thread: Callable[[str], None], should_stop: Callable[[], bool],
    lock_fd: int, timeout_seconds: int,
) -> dict:
    """Execute at most one turn; completed is not a PR/CI success assertion.

    A recorded event is never replayed. Verified terminal history returns its
    actual status; blocked is not proof of an outcome and the queue must retain
    the original event for reconciliation. No command is invoked unless the
    caller has supplied an open execution-lock descriptor.
    """
    result = {"thread_id": thread_id, "turn_id": None, "status": "blocked", "message": ""}
    server = None
    try:
        if os.name != "posix":
            raise _Blocked("The queue requires POSIX process and lock semantics")
        if not isinstance(codex_command, list) or not codex_command or any(
            not isinstance(value, str) or not value for value in codex_command
        ):
            raise _Blocked("Codex command must be a nonempty argument list")
        if not isinstance(event_id, str) or not event_id or not isinstance(prompt, str):
            raise _Blocked("A stable event identity and text prompt are required")
        if not isinstance(cwd, str) or not Path(cwd).is_dir():
            raise _Blocked("A valid local workspace directory is required")
        if not isinstance(timeout_seconds, (int, float)) or not 0 < timeout_seconds <= 86400:
            raise _Blocked("Execution timeout must be between zero and 86400 seconds")
        if not isinstance(lock_fd, int) or not stat.S_ISREG(os.fstat(lock_fd).st_mode):
            raise _Blocked("An open execution-lock file descriptor is required")
        if should_stop():
            raise _Stopped("Queue stop requested before starting App Server")
        server = _Server(codex_command, cwd, lock_fd, should_stop, timeout_seconds)
        server.request("initialize", {"clientInfo": {
            "name": "oasis7_ci_queue", "title": "Oasis7 CI issue queue", "version": "1",
        }})
        server.send({"method": "initialized", "params": {}})
        marker = event_marker(event_id)
        if thread_id is not None:
            server.thread_id = _identifier(thread_id, "stored thread")
            thread = _read_thread(server, thread_id, cwd)
            matches, uncertain = _history(server, marker)
            if len(matches) > 1:
                raise _Blocked("The event appears in multiple turns; manual reconciliation is required")
            if matches:
                turn = matches[0]
                result["turn_id"] = turn["id"]
                if turn.get("status") in TERMINAL:
                    result.update(status=turn["status"],
                                  message=f"Existing {turn['status']} event recovered; no new turn started")
                    return result
                raise _Blocked(f"Existing event turn is {turn.get('status', 'unknown')}; it was not replayed")
            status = _thread_status(thread)
            if uncertain or status not in {"idle", "notLoaded"}:
                raise _Blocked("Stored thread may have unfinished work; no new turn started")
            resumed = server.request("thread/resume", {
                "threadId": thread_id, "excludeTurns": True, "cwd": cwd,
                "approvalPolicy": "never", "sandbox": "workspaceWrite",
            }).get("thread", {})
            if not isinstance(resumed, dict) or resumed.get("id") != thread_id:
                raise _Blocked("Resumed thread identity does not match")
            if "status" in resumed and _thread_status(resumed) != "idle":
                raise _Blocked("Resumed thread is not idle; no new turn started")
        else:
            thread = server.request("thread/start", {
                "cwd": cwd, "approvalPolicy": "never", "sandbox": "workspaceWrite",
                "ephemeral": False,
            }).get("thread", {})
            if not isinstance(thread, dict) or thread.get("ephemeral") is True:
                raise _Blocked("App Server did not create a persistent thread")
            server.thread_id = _identifier(thread.get("id"), "thread")
            result["thread_id"] = server.thread_id
            try:
                on_thread(server.thread_id)
            except Exception as exc:
                raise _Blocked("Thread identity could not be persisted; no turn started") from exc
        server.check()
        if server.blocked_request:
            raise _Blocked(server.blocked_request)
        server.starting_turn = True
        turn = server.request("turn/start", {
            "threadId": server.thread_id,
            "input": [{"type": "text", "text": marker + "\n" + prompt}],
        }).get("turn", {})
        if not isinstance(turn, dict):
            raise _Blocked("App Server did not identify the submitted turn")
        turn_id = _identifier(turn.get("id"), "turn")
        if server.turn_id is not None and server.turn_id != turn_id:
            raise _Blocked("App Server turn response and notification identities differ")
        server.turn_id = turn_id
        if turn.get("status") in TERMINAL:
            server.terminal_turns[turn_id] = turn
        elif turn.get("status") != "inProgress":
            raise _Blocked("App Server returned an unknown turn status")
        final_turn = server.wait_turn()
        result.update(turn_id=turn_id, status=final_turn["status"],
                      message=f"Codex turn {final_turn['status']}; PR and CI outcomes require independent observation")
    except (OSError, ValueError, _Blocked) as exc:
        result["message"] = str(exc)
        if server:
            result["thread_id"] = server.thread_id or result["thread_id"]
            result["turn_id"] = server.turn_id or result["turn_id"]
            stopped_turn = server.interrupt()
            if isinstance(exc, _Stopped) and stopped_turn and not server.blocked_request:
                result["status"] = stopped_turn["status"]
    finally:
        if server:
            server.close()
    return result
