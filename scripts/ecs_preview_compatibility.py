"""Conservative, locally authorized viewer-only compatibility evaluator.

Complete Git tree proofs bind every input name/mode/object to the source commit.
The local policy accepts an exact display-input risk scope; it is not a remote
safe flag or a general claim of persistence compatibility.
"""
import base64
import hashlib
import json
from pathlib import Path
import subprocess


def git_hash(kind, data):
    return hashlib.sha1(kind.encode() + b" " + str(len(data)).encode() + b"\0" + data).hexdigest()


def source_evidence(repo, source):
    commit = subprocess.check_output(["git", "cat-file", "commit", source], cwd=repo)
    raw = subprocess.check_output(["git", "ls-tree", "-rz", source], cwd=repo)
    entries = {}
    for entry in raw.split(b"\0"):
        if not entry:
            continue
        metadata, name = entry.split(b"\t", 1)
        mode, kind, oid = metadata.decode().split()
        entries[name.decode()] = {"mode": mode, "kind": kind, "oid": oid}
    value = {"schema_version": 1, "source_sha": source,
             "commit": base64.b64encode(commit).decode(), "entries": entries}
    validate_source_evidence(value, source)
    return value


def validate_source_evidence(value, source):
    if set(value) != {"schema_version", "source_sha", "commit", "entries"} or value["schema_version"] != 1 or value["source_sha"] != source:
        raise ValueError("invalid compatibility source evidence schema")
    commit = base64.b64decode(value["commit"], validate=True)
    if git_hash("commit", commit) != source or not commit.startswith(b"tree "):
        raise ValueError("compatibility commit proof mismatch")
    root_oid = commit.split(b"\n", 1)[0][5:].decode()
    root = {}
    for name, info in value["entries"].items():
        parts = name.split("/")
        if any(part in ("", ".", "..") or "\0" in part for part in parts):
            raise ValueError("unsafe source evidence name")
        if set(info) != {"mode", "kind", "oid"} or (info["mode"], info["kind"]) not in (("100644", "blob"), ("100755", "blob"), ("120000", "blob"), ("160000", "commit")):
            raise ValueError("invalid source evidence object")
        if len(info["oid"]) != 40 or any(c not in "0123456789abcdef" for c in info["oid"]):
            raise ValueError("invalid source object id")
        node = root
        for part in parts[:-1]:
            node = node.setdefault(part, {})
            if not isinstance(node, dict) or "oid" in node:
                raise ValueError("ambiguous source tree")
        if parts[-1] in node:
            raise ValueError("duplicate source input")
        node[parts[-1]] = info
    def tree(node):
        records = []
        for name, info in node.items():
            directory = "oid" not in info
            mode, oid = ("40000", tree(info)) if directory else (info["mode"], info["oid"])
            records.append((name.encode() + (b"/" if directory else b""), mode.encode() + b" " + name.encode() + b"\0" + bytes.fromhex(oid)))
        return git_hash("tree", b"".join(record for _, record in sorted(records)))
    if tree(root) != root_oid:
        raise ValueError("incomplete or forged compatibility source input map")
    return value["entries"]


def file_hash(path):
    result = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            result.update(block)
    return result.hexdigest()


def evaluate_viewer(before_release, after_release, policy):
    if set(policy) != {"schema_version", "allowed_inputs", "evidence", "read_only_viewer"} or policy["schema_version"] != 1 or policy["read_only_viewer"] is not True or not isinstance(policy["evidence"], str) or not policy["evidence"].strip():
        raise ValueError("independent read-only viewer compatibility authorization required")
    allowed = policy["allowed_inputs"]
    if not isinstance(allowed, list) or not allowed or any(not isinstance(p, str) or not p.startswith("crates/") or ".." in p.split("/") or "*" in p for p in allowed):
        raise ValueError("exact audited display input paths required")
    before_release, after_release = Path(before_release), Path(after_release)
    maps = []
    for release in (before_release, after_release):
        source = json.loads((release / "manifest.json").read_text())["source"]
        data = (release / "compatibility-inputs.json").read_bytes()
        checksums = dict(line.split("  ", 1)[::-1] for line in (release / "SHA256SUMS").read_text().splitlines())
        if checksums.get("compatibility-inputs.json") != hashlib.sha256(data).hexdigest():
            raise ValueError("compatibility evidence is not bound to the verified bundle")
        maps.append(validate_source_evidence(json.loads(data), source))
    old, new = maps
    if set(old) != set(new):
        raise ValueError("source input additions/deletions require independent compatibility review")
    changed = {p for p in old if old[p] != new[p]}
    # Root-approved paths are the sole exception. All dependency locks, build
    # tools, resources, modes and source names remain in the comparison.
    if not changed <= set(allowed):
        raise ValueError("source changes exceed the independently audited viewer input scope")
    for binary in ("oasis7_chain_runtime", "oasis7_game_launcher", "oasis7_game_client"):
        if file_hash(before_release / "bin" / binary) != file_hash(after_release / "bin" / binary):
            raise ValueError("viewer compatibility requires unchanged chain/launcher/client program bytes")
    return True
