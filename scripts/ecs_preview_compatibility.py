"""Conservative, locally authorized viewer-only compatibility evaluator.

Complete Git tree proofs bind every input name/mode/object to the source commit.
The local policy accepts an exact display-input risk scope; it is not a remote
safe flag or a general claim of persistence compatibility.
"""
import base64
import hashlib
import json
import re
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


# General player families are host-approved publisher commitments. Unlike the
# viewer evaluator, this does not statically prove absence of migrations.
PLAYER_CONTRACTS = {"program_contract", "persistence_contract", "identity_contract"}
PLAYER_SOURCE = re.compile(r"[0-9a-f]{40}")
PLAYER_RELEASE = re.compile(r"[0-9a-f]{40}/[1-9][0-9]*-[1-9][0-9]*")


def validate_player_policy(policy):
    if not isinstance(policy, dict) or set(policy) != {"schema_version", "contracts", "migration_free", "evidence"}:
        raise ValueError("invalid player policy schema")
    contracts = policy["contracts"]
    if type(policy["schema_version"]) is not int or policy["schema_version"] != 1 or policy["migration_free"] is not True:
        raise ValueError("player policy requires explicit migration-free approval")
    if not isinstance(contracts, dict) or set(contracts) != PLAYER_CONTRACTS or any(not isinstance(value, str) or not re.fullmatch(r"[A-Za-z0-9_.:-]{1,128}", value) for value in contracts.values()):
        raise ValueError("invalid player policy contracts")
    if not isinstance(policy["evidence"], str) or not policy["evidence"].strip():
        raise ValueError("independent player policy evidence required")
    return policy


def player_release_identity(release, releases_dir, source):
    root, path = Path(releases_dir), Path(release)
    if not root.is_absolute() or not path.is_absolute():
        raise ValueError("player release must be a canonical installed release path")
    try:
        identity = path.relative_to(root).as_posix()
    except ValueError as error:
        raise ValueError("player release is outside releases directory") from error
    if not PLAYER_RELEASE.fullmatch(identity) or not isinstance(source, str) or not PLAYER_SOURCE.fullmatch(source) or identity.split("/")[0] != source:
        raise ValueError("player release/source identity is not a published run-attempt")
    try:
        canonical = root.resolve(strict=True) == root and path.resolve(strict=True) == path
    except OSError as error:
        raise ValueError("player release path is not installed") from error
    if not canonical:
        raise ValueError("player release must be a canonical installed release path")
    return identity


def player_binding_identities(before, after, manifest, releases_dir):
    if manifest["components"] != ["player"] or before.get("chain") != after.get("chain"):
        raise ValueError("player policy cannot authorize a chain update")
    identities = []
    for bindings in (before, after):
        if set(bindings) != {"chain", "player", "web", "skill"}:
            raise ValueError("player policy requires all component bindings")
        player = bindings["player"]
        for name in ("player", "web", "skill"):
            binding = bindings[name]
            if not isinstance(binding, dict) or set(binding) != {"path", "release", "source_sha", "sha256"} or any(not isinstance(value, str) for value in binding.values()) or not re.fullmatch(r"[0-9a-f]{64}", binding["sha256"]):
                raise ValueError("invalid player component binding")
            if binding["release"] != player["release"] or binding["source_sha"] != player["source_sha"]:
                raise ValueError("player/web/skill must share one release and source")
        identities.append(player_release_identity(player["release"], releases_dir, player["source_sha"]))
    if identities[1] != manifest["release_id"] or after["player"]["source_sha"] != manifest["source_sha"]:
        raise ValueError("new player manifest does not match installed candidate")
    return identities


def evaluate_player(before, after, manifest, policy, releases_dir, store, release_api, prefix):
    validate_player_policy(policy)
    old_id, _new_id = player_binding_identities(before, after, manifest, releases_dir)
    # This key is derived only from the actual installed binding and the fixed
    # configured prefix. Local bootstrap metadata cannot stand in for a release.
    if store is None:
        raise ValueError("trusted old publication manifest is unavailable")
    key = release_api.object_key(f"{prefix}/releases/{old_id}/manifest.json", prefix)
    previous = json.loads(store.get(key))
    release_api.validate_manifest(previous, prefix)
    if "player" not in previous["components"]:
        raise ValueError("old immutable manifest did not publish a player component")
    if previous["release_id"] != old_id or previous["source_sha"] != before["player"]["source_sha"]:
        raise ValueError("old immutable manifest does not match installed player")
    if previous["compatibility"] != manifest["compatibility"] or manifest["compatibility"] != policy["contracts"]:
        raise ValueError("player compatibility contract family is not independently approved")
    return True
