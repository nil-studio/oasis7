"""Bounded trusted baseline artifact association; records never establish authority."""
import base64
import hashlib
import io
import json
import re
import stat
import subprocess
import tempfile
import zipfile
from urllib.parse import quote

WORKFLOW = ".github/workflows/codeql.yml"
ARCHIVE_LIMIT = 1024 * 1024
RECORD_LIMIT = 64 * 1024
PAGE_LIMIT = 10
FIELDS = {"schema", "repository", "workflow_path", "workflow_ref", "workflow_sha",
          "run_id", "run_attempt", "job_key", "job_name", "checkout_sha", "ref",
          "unit", "profile", "category", "execution_status", "upload_status", "upload_sarif_id"}


def integer(value):
    return type(value) is int and value > 0


def require(condition, reason):
    if not condition:
        raise RuntimeError(reason)


def object_field(value, key):
    require(isinstance(value, dict) and isinstance(value.get(key), dict),
            "malformed " + key + " metadata")
    return value[key]


def api(endpoint):
    response = subprocess.run(["gh", "api", endpoint], capture_output=True, text=True, timeout=30)
    require(response.returncode == 0, "association API read failed: " + endpoint.split("?")[0])
    return json.loads(response.stdout)


def pages(endpoint, key):
    rows, count, ids = [], None, set()
    for page in range(1, PAGE_LIMIT + 1):
        sep = "&" if "?" in endpoint else "?"
        value = api(f"{endpoint}{sep}per_page=100&page={page}")
        require(isinstance(value, dict) and isinstance(value.get(key), list)
                and type(value.get("total_count")) is int, "association pagination schema invalid")
        batch, observed = value[key], value["total_count"]
        require(0 <= observed <= PAGE_LIMIT * 100 and len(batch) <= 100
                and (count is None or count == observed), "association pagination count changed or exceeded")
        count = observed
        for row in batch:
            require(isinstance(row, dict) and integer(row.get("id")) and row["id"] not in ids,
                    "association pagination duplicate or invalid identity")
            ids.add(row["id"])
        rows.extend(batch)
        require(len(rows) <= count, "association pagination count mismatch")
        if len(rows) == count:
            return rows
        require(len(batch) == 100, "association pagination incomplete")
    raise RuntimeError("association pagination budget exceeded")


def workflow_bytes(prefix, ref):
    value = api(f"{prefix}/contents/{WORKFLOW}?ref={quote(ref, safe='')}")
    require(isinstance(value, dict) and value.get("type") == "file"
            and value.get("encoding") == "base64" and isinstance(value.get("content"), str)
            and len(value["content"]) <= 512 * 1024, "trusted workflow content unavailable")
    return base64.b64decode(value["content"], validate=False)


def archive_download(prefix, artifact_id):
    """gh owns authenticated redirects; child file limit caps bytes before buffering."""
    try:
        import resource
    except ImportError as exc:
        raise RuntimeError("bounded artifact downloads require POSIX resource limits") from exc
    def limit_file():
        resource.setrlimit(resource.RLIMIT_FSIZE, (ARCHIVE_LIMIT, ARCHIVE_LIMIT))

    with tempfile.TemporaryFile() as output, tempfile.TemporaryFile() as errors:
        result = subprocess.run(["gh", "api", f"{prefix}/actions/artifacts/{artifact_id}/zip"],
                                stdout=output, stderr=errors, timeout=30, preexec_fn=limit_file)
        require(result.returncode == 0, "artifact download failed or exceeded byte limit")
        size = output.tell()
        require(0 < size <= ARCHIVE_LIMIT, "artifact download size invalid")
        output.seek(0)
        return output.read(ARCHIVE_LIMIT)


def unique_json(pairs):
    result = {}
    for key, value in pairs:
        require(key not in result, "artifact JSON duplicate key")
        result[key] = value
    return result


def decode_archive(content, digest):
    require(isinstance(digest, str) and re.fullmatch(r"sha256:[0-9a-f]{64}", digest),
            "artifact digest unavailable")
    require(hashlib.sha256(content).hexdigest() == digest[7:], "artifact digest mismatch")
    with zipfile.ZipFile(io.BytesIO(content)) as archive:
        entries = archive.infolist()
        require(len(entries) == 1, "artifact must contain one record")
        entry = entries[0]
        mode = entry.external_attr >> 16
        require(entry.filename == "evidence.json" and not entry.is_dir()
                and not entry.flag_bits & 1 and not stat.S_ISLNK(mode)
                and (not stat.S_IFMT(mode) or stat.S_ISREG(mode))
                and 0 < entry.file_size <= RECORD_LIMIT, "artifact member unsafe or oversized")
        record = archive.read(entry).decode("utf-8", errors="strict")
        value = json.loads(record, object_pairs_hook=unique_json)
    require(isinstance(value, dict) and set(value) == FIELDS, "artifact record schema invalid")
    return value


def step(job, name):
    steps = job.get("steps")
    require(isinstance(steps, list) and all(isinstance(s, dict) for s in steps),
            "malformed job steps metadata")
    matches = [s for s in steps if s.get("name") == name]
    require(len(matches) == 1, "job step identity missing or duplicate")
    return matches[0].get("conclusion")


def execution_status(job):
    """Read both actual production phases; old and skipped paths stay single-stage."""
    steps = job.get("steps", [])
    require(isinstance(steps, list) and all(isinstance(s, dict) for s in steps),
            "malformed job steps metadata")
    extraction = [s for s in steps if s.get("name") == "CodeQL extraction and queries"]
    if len(extraction) != 1:
        state, conclusion = job.get("status"), job.get("conclusion")
        return (conclusion if conclusion in ("cancelled", "timed_out") else
                state if state in ("queued", "in_progress", "waiting", "pending") else "unknown")
    result = extraction[0].get("conclusion") or "unknown"
    if result != "success" or job.get("name") != "CodeQL / rust-repo / default":
        return result
    tools = [s for s in steps if s.get("name") == "Verify the pinned Rust default production bundle"]
    queries = [s for s in steps if s.get("name") == "Run and verify all default Rust queries with the fixed libraries"]
    if not tools and not queries:
        return result  # The previous workflow executed queries inside analyze.
    if len(tools) != 1:
        return "unknown"
    tool_status = tools[0].get("conclusion")
    if tool_status == "skipped":
        # PRs retain their stock analysis path. A skipped query step in the new
        # workflow is not evidence that this job selected production libraries.
        return result if all(q.get("conclusion") == "skipped" for q in queries) else "unknown"
    if tool_status != "success":
        return tool_status or "unknown"
    if len(queries) != 1:
        return "unknown"
    return queries[0].get("conclusion") or "unknown"


class Reader:
    def __init__(self, repo, ref, sha):
        self.repo, self.ref, self.sha = repo, ref, sha
        self.prefix = "repos/" + repo
        self.context = None

    def trusted_context(self):
        if self.context is None:
            repository = api(self.prefix)
            require(isinstance(repository, dict) and integer(repository.get("id"))
                    and repository.get("full_name") == self.repo
                    and isinstance(repository.get("default_branch"), str), "repository identity invalid")
            branch = repository["default_branch"]
            require(self.ref == "refs/heads/" + branch, "association supports trusted default-branch baselines only")
            workflow = api(self.prefix + "/actions/workflows/codeql.yml")
            require(isinstance(workflow, dict) and integer(workflow.get("id")) and workflow.get("path") == WORKFLOW
                    and workflow.get("state") == "active", "trusted workflow identity unavailable")
            provider = api("apps/github-actions")
            owner = object_field(provider, "owner")
            require(isinstance(provider, dict) and integer(provider.get("id")) and provider.get("slug") == "github-actions"
                    and owner.get("login") == "github"
                    and owner.get("type") == "Organization", "provider identity invalid")
            trusted = workflow_bytes(self.prefix, branch)
            self.context = repository, workflow, provider, trusted
        return self.context

    def associate(self, run, jobs):
        repository, workflow, provider, trusted = self.trusted_context()
        run_repository = object_field(run, "repository")
        head_repository = object_field(run, "head_repository")
        require(integer(run.get("id")) and integer(run.get("run_attempt"))
                and run.get("workflow_id") == workflow["id"] and run.get("path") == WORKFLOW
                and run.get("head_sha") == self.sha and run.get("head_branch") == repository["default_branch"]
                and run.get("event") in ("schedule", "workflow_dispatch")
                and run_repository.get("id") == repository["id"]
                and run_repository.get("full_name") == self.repo
                and head_repository.get("id") == repository["id"]
                and head_repository.get("full_name") == self.repo
                and integer(run.get("check_suite_id")), "baseline run identity untrusted")
        require(workflow_bytes(self.prefix, self.sha) == trusted, "executed workflow differs from trusted default branch")
        current = api(f"{self.prefix}/actions/runs/{run['id']}")
        self.same_run(run, current)
        # Authentic jobs are read separately: caller's diagnostic projection is not authority.
        observed = pages(f"{self.prefix}/actions/runs/{run['id']}/attempts/{run['run_attempt']}/jobs", "jobs")
        artifacts = pages(f"{self.prefix}/actions/runs/{run['id']}/artifacts", "artifacts")
        errors = []
        for job in jobs:
            job.pop("upload_sarif_id", None)
            match = re.fullmatch(r"CodeQL / (actions-repo|python-repo|javascript-repo|rust-repo) / (default|extended)", str(job.get("name")))
            if not match:
                continue
            try:
                candidates = [j for j in observed if j.get("name") == job["name"]]
                require(len(candidates) == 1 and len([j for j in jobs if j.get("name") == job["name"]]) == 1,
                        "job identity missing or duplicate")
                actual = candidates[0]
                require(integer(actual.get("id")) and actual.get("id") == job.get("id")
                        and actual.get("run_id") == run["id"] and actual.get("run_attempt") == run["run_attempt"]
                        and actual.get("head_sha") == self.sha and job.get("run_id") == run["id"]
                        and job.get("run_attempt") == run["run_attempt"] and job.get("head_sha") == self.sha,
                        "job latest-attempt identity mismatch")
                expected_url = f"https://api.github.com/{self.prefix}/check-runs/{actual['id']}"
                require(actual.get("check_run_url") == expected_url, "job check URL mismatch")
                check = api(f"{self.prefix}/check-runs/{actual['id']}")
                app = object_field(check, "app")
                suite = object_field(check, "check_suite")
                require(isinstance(check, dict) and integer(check.get("id")) and check.get("id") == actual["id"] and check.get("name") == actual["name"]
                        and check.get("head_sha") == self.sha and app.get("id") == provider["id"]
                        and suite.get("id") == run["check_suite_id"]
                        and check.get("conclusion") == actual.get("conclusion"), "job check provenance mismatch")
                execution, upload = execution_status(actual), step(actual, "CodeQL SARIF upload")
                require(execution == "success" and upload == "success", "job execution/upload not successful")
                unit, profile = match.groups()
                name = f"oasis7-codeql-upload-{run['id']}-{run['run_attempt']}-{unit}-{profile}"
                candidates = [a for a in artifacts if a.get("name") == name]
                require(len(candidates) == 1, "artifact identity missing or duplicate")
                artifact = candidates[0]
                self.artifact_identity(artifact, run, repository)
                content = archive_download(self.prefix, artifact["id"])
                require(len(content) == artifact["size_in_bytes"], "artifact download size mismatch")
                record = decode_archive(content, artifact.get("digest"))
                expected = {"schema": "oasis7-codeql-upload-evidence/v1", "repository": self.repo,
                            "workflow_path": WORKFLOW, "workflow_ref": self.repo + "/" + WORKFLOW + "@" + self.ref,
                            "workflow_sha": self.sha, "run_id": run["id"], "run_attempt": run["run_attempt"],
                            "job_key": "analyze", "job_name": actual["name"], "checkout_sha": self.sha,
                            "ref": self.ref, "unit": unit, "profile": profile, "category": f"oasis7/{unit}/{profile}",
                            "execution_status": execution, "upload_status": upload}
                require(all(type(record.get(k)) is type(v) and record.get(k) == v for k, v in expected.items()),
                        "artifact record context mismatch")
                sarif = record["upload_sarif_id"]
                require(isinstance(sarif, str) and 0 < len(sarif) <= 128
                        and re.fullmatch(r"[A-Za-z0-9_-]+", sarif), "official SARIF ID missing or invalid")
                refreshed = api(f"{self.prefix}/actions/artifacts/{artifact['id']}")
                require(refreshed == artifact, "artifact metadata changed during read")
                job["upload_sarif_id"] = sarif
            except (RuntimeError, ValueError, KeyError, TypeError, OSError, NotImplementedError,
                    zipfile.BadZipFile, subprocess.TimeoutExpired) as exc:
                errors.append(str(exc))
        self.same_run(run, api(f"{self.prefix}/actions/runs/{run['id']}"))
        return errors

    @staticmethod
    def same_run(run, current):
        require(isinstance(current, dict) and all(type(current.get(key)) is type(run.get(key))
                    and current.get(key) == run.get(key) for key in
                    ("id", "run_attempt", "head_sha", "head_branch", "workflow_id", "path", "event", "check_suite_id")),
                "run identity changed during read")

    def artifact_identity(self, artifact, run, repository):
        linked = object_field(artifact, "workflow_run")
        require(integer(artifact.get("id")) and artifact.get("expired") is False
                and type(artifact.get("size_in_bytes")) is int and 0 < artifact["size_in_bytes"] <= ARCHIVE_LIMIT
                and linked.get("id") == run["id"] and linked.get("repository_id") == repository["id"]
                and linked.get("head_repository_id") == repository["id"]
                and linked.get("head_branch") == run["head_branch"] and linked.get("head_sha") == self.sha,
                "artifact platform identity invalid or unavailable")
