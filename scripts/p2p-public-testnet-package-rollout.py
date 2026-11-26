#!/usr/bin/env python3
from __future__ import annotations

import argparse
import base64
import hashlib
import json
import os
import re
import shlex
import stat
import subprocess
import sys
from pathlib import Path, PurePosixPath, PureWindowsPath
from typing import Any


ROOT_DIR = Path(__file__).resolve().parents[1]
WINDOWS_GOVERNED_BUNDLE = (
    r"C:\oasis7-deploy\config\public-testnet-governed-bootstrap-bundle-2026-06-06.windows.json"
)
WINDOWS_GOVERNED_GENESIS = (
    r"C:\oasis7-deploy\config\public-testnet-governed-bootstrap-genesis-2026-06-06.windows.json"
)
WINDOWS_GOVERNED_MANIFEST = (
    r"C:\oasis7-deploy\config\public-testnet-governed-bootstrap-manifest-2026-06-06.windows.json"
)
WINDOWS_GOVERNED_BOOTSTRAP = (
    r"C:\oasis7-deploy\config\public-testnet-governed-bootstrap-bootstrap-peers-2026-06-06.windows.txt"
)
CANONICAL_PROVIDER_NAMES = ("sequencer", "storage")
MAX_PROVIDER_CHECKPOINT_HEIGHT_DELTA = 1
LINUX_PROVIDER_ROOT = Path("/opt/oasis7/p2p-testnet")


def validate_operator_known_hosts(path: Path | None) -> Path:
    """Return an operator-local host-key file, rejecting provider paths."""
    if path is None:
        die(
            "remote Linux provider rollout requires an explicit --known-hosts "
            "operator-local pinned host-key file"
        )
    candidate = path.expanduser()
    if candidate.is_symlink():
        die(f"operator known-hosts file must not be a symlink: {candidate}")
    if not candidate.is_file() or not stat.S_ISREG(candidate.lstat().st_mode):
        die(f"operator known-hosts file must be a regular file: {candidate}")
    try:
        resolved = candidate.resolve(strict=True)
    except OSError as error:
        die(f"cannot resolve operator known-hosts file {candidate}: {error}")
    try:
        resolved.relative_to(LINUX_PROVIDER_ROOT.resolve())
    except ValueError:
        pass
    else:
        die(
            "operator known-hosts file must be local to the rollout operator; "
            f"provider filesystem path is forbidden: {candidate}"
        )
    if resolved.stat().st_size == 0:
        die(f"operator known-hosts file must contain a pinned host key: {candidate}")
    return resolved


def die(message: str) -> None:
    print(f"error: {message}", file=sys.stderr)
    raise SystemExit(1)


def shell_join(args: list[str]) -> str:
    return " ".join(shlex.quote(arg) for arg in args)


def read_buildinfo(path: Path) -> dict[str, str]:
    info: dict[str, str] = {}
    for raw in path.read_text(encoding="utf-8").splitlines():
        if not raw or raw.startswith("#"):
            continue
        key, sep, value = raw.partition("=")
        if sep:
            info[key.strip()] = value.strip()
    return info


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def verify_sha256sums(package_dir: Path, sums_path: Path) -> list[str]:
    package_root = package_dir.resolve()
    if sums_path.is_symlink() or not sums_path.is_file():
        die(f"checksum manifest must be a regular non-symlink file: {sums_path}")
    verified: list[str] = []
    seen: set[str] = set()
    for raw in sums_path.read_text(encoding="utf-8").splitlines():
        line = raw.strip()
        if not line:
            continue
        expected, _, name = line.partition("  ")
        if not name:
            parts = line.split(maxsplit=1)
            if len(parts) != 2:
                die(f"cannot parse checksum line in {sums_path}: {raw}")
            expected, name = parts
        expected = expected.strip()
        rel_name = name.lstrip("*").strip()
        relative = PurePosixPath(rel_name.replace("\\", "/"))
        if (
            not re.fullmatch(r"[0-9a-fA-F]{64}", expected)
            or not rel_name
            or relative.is_absolute()
            or any(part in ("", ".", "..") for part in relative.parts)
        ):
            die(f"checksum manifest contains unsafe entry in {sums_path}: {raw}")
        target = package_root.joinpath(*relative.parts)
        try:
            target.resolve().relative_to(package_root)
        except ValueError:
            die(f"checksum target escapes package directory: {rel_name}")
        if target.is_symlink() or not target.is_file() or not stat.S_ISREG(target.lstat().st_mode):
            die(f"checksum target missing: {target}")
        actual = sha256_file(target)
        if actual.lower() != expected.lower():
            die(f"checksum mismatch for {target}: expected {expected}, got {actual}")
        if rel_name in seen:
            die(f"checksum manifest contains duplicate entry: {rel_name}")
        seen.add(rel_name)
        verified.append(rel_name)
    if not verified:
        die(f"checksum manifest is empty: {sums_path}")
    return verified


def find_platform_dir(package_dir: Path, platform: str) -> Path:
    buildinfo = package_dir / f"{platform}-BUILDINFO"
    if not buildinfo.is_symlink() and buildinfo.is_file() and stat.S_ISREG(buildinfo.lstat().st_mode):
        return package_dir
    for candidate in package_dir.rglob("*"):
        if not candidate.is_symlink():
            continue
        resolved = candidate.resolve()
        if resolved.is_dir() and any(resolved.rglob(f"{platform}-BUILDINFO")):
            die(f"platform package path contains symlink component: {candidate}")
    matches = sorted(path.parent for path in package_dir.rglob(f"{platform}-BUILDINFO"))
    if not matches:
        die(f"cannot find {platform}-BUILDINFO under {package_dir}")
    if len(matches) > 1:
        die(f"multiple {platform}-BUILDINFO files under {package_dir}: {matches}")
    return matches[0]


def platform_asset(platform_dir: Path, platform: str) -> Path:
    names = {
        "linux-x64": "oasis7-linux-x64.deb",
        "windows-x64": "oasis7-windows-x64.exe",
        "macos-x64": "oasis7-macos-x64.dmg",
        "macos-arm64": "oasis7-macos-arm64.dmg",
    }
    name = names.get(platform)
    if not name:
        die(f"unsupported platform: {platform}")
    asset = platform_dir / name
    if asset.is_symlink() or not asset.is_file() or not stat.S_ISREG(asset.lstat().st_mode):
        die(f"missing {platform} asset: {asset}")
    return asset


def require_verified_files(platform: str, platform_dir: Path, buildinfo: Path, asset: Path, verified: list[str]) -> None:
    verified_set = set(verified)
    required = [
        buildinfo.relative_to(platform_dir).as_posix(),
        asset.relative_to(platform_dir).as_posix(),
    ]
    for rel_name in required:
        if rel_name not in verified_set:
            die(f"{platform} checksum file does not cover required artifact: {rel_name}")


def platform_ops_tools_asset(platform_dir: Path, platform: str) -> Path:
    asset = platform_dir / f"oasis7-{platform}-ops-tools.tar.gz"
    if asset.is_symlink() or not asset.is_file() or not stat.S_ISREG(asset.lstat().st_mode):
        die(f"missing {platform} ops-tools asset: {asset}")
    return asset


def verify_ops_tools_contract(platform_dir: Path, platform: str) -> Path:
    asset = platform_ops_tools_asset(platform_dir, platform)
    sums = platform_dir / f"{platform}-ops-tools-SHA256SUMS"
    if sums.is_symlink() or not sums.is_file() or not stat.S_ISREG(sums.lstat().st_mode):
        die(f"missing {platform} ops-tools checksum file: {sums}")
    verified = verify_sha256sums(platform_dir, sums)
    relative = asset.relative_to(platform_dir).as_posix()
    if relative not in set(verified):
        die(f"{platform} ops-tools checksum file does not cover required artifact: {relative}")
    return asset


def require_macos_arm64_metadata(info: dict[str, str]) -> None:
    if info.get("platform") != "macos-arm64":
        die(f"macos-arm64 BUILDINFO platform mismatch: {info.get('platform', '')!r}")
    if info.get("target_triple") != "aarch64-apple-darwin":
        die(
            "macos-arm64 BUILDINFO target_triple must be "
            "aarch64-apple-darwin"
        )


def windows_governed_files(platform_dir: Path, verified: list[str]) -> list[tuple[Path, str]]:
    bundle = platform_dir / WINDOWS_GOVERNED_BUNDLE.rsplit("\\", 1)[-1]
    genesis = platform_dir / WINDOWS_GOVERNED_GENESIS.rsplit("\\", 1)[-1]
    manifest = platform_dir / WINDOWS_GOVERNED_MANIFEST.rsplit("\\", 1)[-1]
    bootstrap = platform_dir / WINDOWS_GOVERNED_BOOTSTRAP.rsplit("\\", 1)[-1]

    def reject_symlink_component(path: Path) -> None:
        current = platform_dir
        if current.is_symlink():
            die(f"Windows governed bootstrap artifact contains symlink component: {current}")
        for component in path.relative_to(platform_dir).parts:
            current /= component
            if current.is_symlink():
                die(f"Windows governed bootstrap artifact contains symlink component: {current}")

    for path in (bundle, genesis, manifest, bootstrap):
        reject_symlink_component(path)
        if not path.is_file():
            die(f"missing Windows governed bootstrap artifact: {path}")
    bundle_data = json.loads(bundle.read_text(encoding="utf-8"))
    manifest_data = json.loads(manifest.read_text(encoding="utf-8"))
    genesis_data = json.loads(genesis.read_text(encoding="utf-8"))
    refs = genesis_data.get("governance_bootstrap_refs")
    if not isinstance(refs, dict):
        die(f"Windows governed genesis missing governance_bootstrap_refs: {genesis}")
    public_testnet = manifest_data.get("tier") == "public_testnet" or bundle_data.get("track") == "public_testnet"
    runtime_refs = manifest_data.get("runtime_refs")
    if not isinstance(runtime_refs, dict):
        die(f"Windows governed manifest missing runtime_refs: {manifest}")
    if public_testnet:
        for field in ("generated_world_sidecar", "world_generation_provenance"):
            if not isinstance(bundle_data.get(field), dict) or not runtime_refs.get(f"{field}_ref"):
                die(f"public_testnet Windows source missing {field}")

    files_by_remote: dict[str, Path] = {
        bundle.name: bundle,
        genesis.name: genesis,
        manifest.name: manifest,
        bootstrap.name: bootstrap,
    }

    platform_root = platform_dir.resolve()

    def confined_artifact_ref(raw_ref: str, label: str) -> tuple[Path, Path]:
        normalized = raw_ref.replace("\\", "/")
        posix_ref = PurePosixPath(normalized)
        windows_ref = PureWindowsPath(raw_ref)
        if (
            posix_ref.is_absolute()
            or windows_ref.is_absolute()
            or bool(windows_ref.drive)
            or ".." in posix_ref.parts
        ):
            die(f"Windows runtime truth ref escapes platform closure for {label}: {raw_ref}")
        relative = Path(*posix_ref.parts)
        if relative == Path("."):
            die(f"Windows runtime truth ref escapes platform closure for {label}: {raw_ref}")
        unresolved_source = platform_root / relative
        if unresolved_source.is_symlink():
            die(f"Windows runtime truth tree contains symlink for {label}: {raw_ref}")
        source = unresolved_source.resolve()
        try:
            source.relative_to(platform_root)
        except ValueError:
            die(f"Windows runtime truth ref escapes platform closure for {label}: {raw_ref}")
        return relative, source

    def add_file(source: Path, remote_relative: str, label: str) -> None:
        if source.is_symlink():
            die(f"Windows runtime truth tree contains symlink for {label}: {source}")
        source = source.resolve()
        try:
            source.relative_to(platform_root)
        except ValueError:
            die(f"Windows runtime truth ref escapes platform closure for {label}: {source}")
        if not source.is_file():
            die(f"Windows runtime truth source missing: {source}")
        previous = files_by_remote.get(remote_relative)
        if previous is not None and previous.resolve() != source:
            die(f"Windows runtime truth paths collide at localized target {remote_relative}")
        files_by_remote[remote_relative] = source

    def add_artifact(metadata: Any, label: str) -> None:
        if not isinstance(metadata, dict):
            return
        raw_ref = metadata.get("ref")
        if not isinstance(raw_ref, str) or not raw_ref:
            die(f"Windows bundle {label} missing ref")
        relative, source = confined_artifact_ref(raw_ref, label)
        if metadata.get("kind") == "directory":
            if not source.is_dir():
                die(f"Windows runtime truth directory missing for {label}: {source}")
            tree_entries = sorted(source.rglob("*"))
            symlink = next((path for path in tree_entries if path.is_symlink()), None)
            if symlink is not None:
                die(f"Windows runtime truth tree contains symlink for {label}: {symlink}")
            members = [path for path in tree_entries if path.is_file()]
            if not members:
                die(f"Windows runtime truth directory empty for {label}: {source}")
            for member in members:
                add_file(member, (relative / member.relative_to(source)).as_posix(), label)
        else:
            add_file(source, relative.as_posix(), label)

    targets: dict[str, Path] = {}
    for key, raw_ref in refs.items():
        if not isinstance(raw_ref, str) or not raw_ref:
            continue
        source = Path(raw_ref)
        if not source.is_absolute():
            source = platform_dir / source
        source = source.resolve()
        if not source.is_file():
            die(f"Windows genesis governance ref source missing for {key}: {source}")
        target_name = source.name
        previous = targets.get(target_name)
        if previous is not None and previous != source:
            die(f"Windows genesis governance refs collide at localized target {target_name}")
        targets[target_name] = source
        add_file(source, f"doc/testing/evidence/{target_name}", key)

    for field in (
        "world_snapshot",
        "generated_world_sidecar",
        "world_generation_provenance",
        "governance_manifest",
        "network_manifest",
    ):
        add_artifact(bundle_data.get(field), field)
    evidence_refs = bundle_data.get("evidence_refs", [])
    if not isinstance(evidence_refs, list):
        die("Windows bundle evidence_refs must be an array")
    for index, metadata in enumerate(evidence_refs):
        add_artifact(metadata, f"evidence_refs[{index}]")

    files = sorted(((source, remote) for remote, source in files_by_remote.items()), key=lambda item: item[1])
    verified_set = set(verified)
    for source, _ in files:
        relative = source.relative_to(platform_dir).as_posix()
        if relative not in verified_set:
            die(f"windows-x64 checksum file does not cover required artifact: {relative}")
    return files


def require_same_commit(platform_infos: dict[str, dict[str, str]]) -> str:
    first_platform = next(iter(platform_infos))
    expected = platform_infos[first_platform].get("commit", "")
    if not expected:
        die(f"{first_platform} BUILDINFO missing commit")
    for platform, info in platform_infos.items():
        actual = info.get("commit", "")
        if actual != expected:
            die(
                f"{platform} BUILDINFO commit={actual!r} does not match "
                f"{first_platform} commit={expected!r}"
            )
    return expected


def package_provenance(platform: str, platform_dir: Path, asset: Path, info: dict[str, str]) -> dict[str, str]:
    package_version = info.get("package_version", "")
    run_id = info.get("run_id", "")
    commit = info.get("commit", "")
    if not package_version or not run_id or not commit:
        die(f"{platform} BUILDINFO missing package_version, run_id, or commit")
    buildinfo = platform_dir / f"{platform}-BUILDINFO"
    sums = platform_dir / f"{platform}-SHA256SUMS"
    return {
        "platform": platform,
        "package_version": package_version,
        "run_id": run_id,
        "commit": commit,
        "asset": asset.name,
        "asset_sha256": sha256_file(asset),
        "buildinfo_sha256": sha256_file(buildinfo),
        "sha256sums_sha256": sha256_file(sums),
    }


def verify_package_trust(
    package_dir: Path, platforms: list[str]
) -> tuple[dict[str, Path], dict[str, dict[str, str]], dict[str, Path], dict[str, list[str]]]:
    """Verify every requested platform before any package executable can run."""
    platform_dirs: dict[str, Path] = {}
    platform_infos: dict[str, dict[str, str]] = {}
    platform_assets: dict[str, Path] = {}
    verified_files: dict[str, list[str]] = {}
    for platform in platforms:
        platform_dir = find_platform_dir(package_dir, platform)
        platform_root = platform_dir.resolve()
        if platform_dir.is_symlink() or not platform_dir.is_dir():
            die(f"{platform} package directory must be a regular non-symlink directory: {platform_dir}")
        for current, directories, files in os.walk(platform_dir, followlinks=False):
            current_path = Path(current)
            try:
                current_path.resolve().relative_to(platform_root)
            except ValueError:
                die(f"{platform} package tree escapes its platform directory: {current_path}")
            for name in directories + files:
                path = current_path / name
                if path.is_symlink():
                    die(f"{platform} package tree contains symlink: {path}")
                try:
                    mode = path.lstat().st_mode
                except OSError as error:
                    die(f"cannot inspect {platform} package member {path}: {error}")
                if not (stat.S_ISREG(mode) or stat.S_ISDIR(mode)):
                    die(f"{platform} package tree contains non-regular member: {path}")
                try:
                    path.resolve().relative_to(platform_root)
                except ValueError:
                    die(f"{platform} package member escapes its platform directory: {path}")
        buildinfo = platform_dir / f"{platform}-BUILDINFO"
        sums = platform_dir / f"{platform}-SHA256SUMS"
        if not sums.is_file():
            die(f"missing {platform} checksum file: {sums}")
        verified = verify_sha256sums(platform_dir, sums)
        asset = platform_asset(platform_dir, platform)
        require_verified_files(platform, platform_dir, buildinfo, asset, verified)
        ops_asset = verify_ops_tools_contract(platform_dir, platform)
        ops_relative = ops_asset.relative_to(platform_dir).as_posix()
        if ops_relative not in set(verified):
            die(
                f"{platform} package checksum file does not cover its co-located ops-tools artifact: "
                f"{ops_relative}"
            )
        info = read_buildinfo(buildinfo)
        if platform == "macos-arm64":
            require_macos_arm64_metadata(info)
        platform_dirs[platform] = platform_dir
        platform_infos[platform] = info
        platform_assets[platform] = asset
        verified_files[platform] = verified
    return platform_dirs, platform_infos, platform_assets, verified_files


def load_manifest(path: Path) -> dict[str, Any]:
    data = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(data, dict):
        die("manifest must be a JSON object")
    nodes = data.get("nodes")
    if not isinstance(nodes, list) or not nodes:
        die("manifest must contain a non-empty nodes array")
    return data


def artifact_ref(platform: str, version: str, asset_name: str, runtime_name: str) -> str:
    if platform == "linux-x64":
        return f"testnet-package-{platform}-{version}/{asset_name}!/opt/oasis7/bin/{runtime_name}"
    return f"testnet-package-{platform}-{version}/{asset_name}!/bin/{runtime_name}"


def canonical_provider_status_urls(manifest: dict[str, Any]) -> tuple[str, str]:
    providers: dict[str, str] = {}
    for node in manifest["nodes"]:
        if not isinstance(node, dict):
            continue
        name = str(node.get("name") or "")
        if name not in CANONICAL_PROVIDER_NAMES:
            continue
        if name in providers:
            die(f"rollout manifest declares canonical provider {name} more than once")
        status_url = node.get("status_url")
        if not isinstance(status_url, str) or not status_url.endswith("/v1/chain/status"):
            die(f"canonical provider {name} must declare a /v1/chain/status status_url")
        providers[name] = status_url
    missing = [name for name in CANONICAL_PROVIDER_NAMES if name not in providers]
    if missing:
        die("observer rollout requires canonical provider status_url entries: " + ", ".join(missing))
    return providers["sequencer"], providers["storage"]


def load_probe_result(manifest: Path, package_dir: Path, probe_out: Path) -> dict[str, Any]:
    """Validate a receipt produced by the repository-owned clean-room probe."""
    if probe_out.is_symlink() or not probe_out.is_file():
        die("checkpoint closure probe did not produce its canonical receipt")
    try:
        result = json.loads(probe_out.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        die(f"checkpoint closure probe result is invalid JSON: {error}")
    if not isinstance(result, dict) or result.get("schema_version") != "oasis7.observer_checkpoint_closure_probe.v1":
        die("checkpoint closure probe result has invalid schema_version")
    digest = result.get("canonical_digest")
    material = dict(result)
    material.pop("canonical_digest", None)
    if not isinstance(digest, str) or hashlib.sha256(
        json.dumps(material, sort_keys=True, separators=(",", ":"), ensure_ascii=True).encode()
    ).hexdigest() != digest:
        die("checkpoint closure probe canonical digest mismatch")
    runtime = result.get("runtime_receipt")
    if not isinstance(runtime, dict) or runtime.get("schema_version") != "oasis7.checkpoint_closure_verification_receipt.v1":
        die("checkpoint closure probe lacks runtime receipt")
    if isinstance(runtime.get("height"), bool) or not isinstance(runtime.get("height"), int) or runtime["height"] <= 0:
        die("checkpoint closure probe receipt has invalid or stale checkpoint height")
    for field in ("execution_block_hash", "execution_state_root", "manifest_hash"):
        if not isinstance(runtime.get(field), str) or not runtime[field]:
            die(f"checkpoint closure probe receipt lacks {field} binding")
    observations = runtime.get("fetch_observations")
    objects = runtime.get("objects")
    if not isinstance(observations, list) or not observations or not isinstance(objects, list) or len(objects) != len(observations):
        die("checkpoint closure probe has invalid closure observations")
    for obj, observed in zip(objects, observations):
        if not isinstance(obj, dict) or not isinstance(observed, dict) or observed.get("source") != "network_fetch" or observed.get("signed_request") is not True or observed.get("response_found") is not True or not isinstance(observed.get("connected_candidate_ids"), list) or not observed["connected_candidate_ids"] or observed.get("content_hash") != obj.get("expected_content_hash") or observed.get("observed_content_hash") != obj.get("expected_content_hash") or observed.get("observed_size_bytes") != obj.get("expected_size_bytes"):
            die("checkpoint closure probe receipt has unbound network fetch observation")
    bindings = result.get("input_bindings")
    if not isinstance(bindings, dict) or bindings.get("rollout_manifest_sha256") != sha256_file(manifest):
        die("checkpoint closure probe manifest binding mismatch")
    observer_names = [str(node.get("name") or "") for node in load_manifest(manifest)["nodes"]
                      if isinstance(node, dict) and str(node.get("name") or "") not in CANONICAL_PROVIDER_NAMES]
    if bindings.get("observer_name") not in observer_names:
        die("checkpoint closure probe observer binding mismatch")
    if bindings.get("world_id") != runtime.get("world_id"):
        die("checkpoint closure probe world identity mismatch")
    buildinfo = bindings.get("buildinfo")
    if not isinstance(buildinfo, dict) or not all(isinstance(buildinfo.get(k), str) and buildinfo[k] for k in ("commit", "package_version", "run_id")):
        die("checkpoint closure probe BUILDINFO binding is incomplete")
    return {
        "checkpoint_id": runtime["execution_block_hash"],
        "manifest_hash": runtime["manifest_hash"],
        "height": runtime["height"],
        "clean_state": True,
        "providers": [
            {
                "provider_id": "clean-room-probe",
                "authorized": True,
                "connected": True,
                "complete": True,
                "hash_size_binding_verified": True,
                "missing_hashes": [],
            }
        ],
        "probe_canonical_digest": digest,
        "fetch_observations": observations,
    }


def run_checkpoint_closure_probe(manifest: Path, package_dir: Path, out_dir: Path) -> dict[str, Any]:
    """Obtain closure evidence only by executing the repository-owned probe.

    This is intentionally not an argument: a JSON file supplied by an operator
    has no binding to the fresh runtime invocation that fetched the closure.
    """
    probe_out = out_dir / "checkpoint-closure-probe.json"
    command = [sys.executable, str(ROOT_DIR / "scripts/p2p-observer-checkpoint-closure-probe.py"),
        "--manifest", str(manifest), "--package-dir", str(package_dir),
        "--out", str(probe_out),
    ]
    completed = subprocess.run(command, text=True, capture_output=True)
    if completed.returncode:
        detail = completed.stderr.strip() or completed.stdout.strip() or "probe failed"
        die(f"checkpoint closure probe failed: {detail}")
    return load_probe_result(manifest, package_dir, probe_out)


def observer_checkpoint_gate_bash(
    sequencer_status_url: str,
    storage_status_url: str,
    closure_receipt: dict[str, Any],
) -> str:
    receipt_json = json.dumps(closure_receipt, ensure_ascii=True, sort_keys=True)
    return f'''python3 - {shlex.quote(sequencer_status_url)} {shlex.quote(storage_status_url)} <<'PY'
import json
import re
import sys
from urllib.request import Request, urlopen

MAX_HEIGHT_DELTA = {MAX_PROVIDER_CHECKPOINT_HEIGHT_DELTA}
CLOSURE_RECEIPT = json.loads({receipt_json!r})

def checkpoint(name, url):
    try:
        with urlopen(Request(url, headers={{"Accept": "application/json"}}), timeout=10) as response:
            payload = json.loads(response.read().decode("utf-8"))
    except Exception as error:
        raise SystemExit(f"provider_checkpoint_gate={{name}} collection_failed={{error}}")
    try:
        value = payload["chain_proof"]["latest_execution_checkpoint"]
    except (KeyError, TypeError):
        value = None
    if not isinstance(value, dict):
        raise SystemExit(f"provider_checkpoint_gate={{name}} missing_latest_execution_checkpoint")
    schema = value.get("schema_version")
    checkpoint_id = value.get("checkpoint_id")
    height = value.get("height")
    manifest_hash = value.get("manifest_hash")
    if isinstance(schema, bool) or not isinstance(schema, int) or schema < 2:
        raise SystemExit(f"provider_checkpoint_gate={{name}} invalid_schema_version")
    if not isinstance(checkpoint_id, str) or not checkpoint_id.strip() or not isinstance(manifest_hash, str) or not re.fullmatch(r"[0-9a-fA-F]{{64}}", manifest_hash):
        raise SystemExit(f"provider_checkpoint_gate={{name}} invalid_checkpoint_identity")
    if isinstance(height, bool) or not isinstance(height, int) or height <= 0:
        raise SystemExit(f"provider_checkpoint_gate={{name}} invalid_checkpoint_height")
    return checkpoint_id, height, manifest_hash.lower()

sequencer = checkpoint("sequencer", sys.argv[1])
storage = checkpoint("storage", sys.argv[2])
if sequencer[0] != storage[0] or sequencer[2] != storage[2]:
    raise SystemExit("provider_checkpoint_gate identity_mismatch")
if abs(sequencer[1] - storage[1]) > MAX_HEIGHT_DELTA:
    raise SystemExit("provider_checkpoint_gate height_incompatible")
expected = (CLOSURE_RECEIPT["checkpoint_id"], CLOSURE_RECEIPT["height"], CLOSURE_RECEIPT["manifest_hash"].lower())
if sequencer != expected or storage != expected:
    raise SystemExit("provider_checkpoint_gate closure_receipt_identity_mismatch")
print(f"provider_checkpoint_gate=passed checkpoint_id={{sequencer[0]}} height_delta={{abs(sequencer[1] - storage[1])}} hash_size_binding_verified=true missing_hashes=[]")
PY'''


def observer_checkpoint_gate_macos(
    sequencer_status_url: str,
    storage_status_url: str,
    closure_receipt: dict[str, Any],
) -> str:
    return """provider_checkpoint_gate_macos() {
  local provider name url payload schema checkpoint_id checkpoint_id_without_whitespace height manifest_hash
  local sequencer_id sequencer_height sequencer_hash
  local storage_id storage_height storage_hash
  command -v plutil >/dev/null 2>&1 || {
    echo "provider_checkpoint_gate=macos_native_parser_unavailable" >&2
    return 1
  }
  for provider in \\
    "sequencer	%s" \\
    "storage	%s"; do
    IFS=$'\\t' read -r name url <<<"$provider"
    payload="$(mktemp "${TMPDIR:-/tmp}/oasis7-provider-checkpoint.json.XXXXXX")" || return 1
    if ! curl -fsS --connect-timeout 10 --max-time 10 "$url" -o "$payload"; then
      rm -f "$payload"
      echo "provider_checkpoint_gate=$name collection_failed" >&2
      return 1
    fi
    schema="$(plutil -extract chain_proof.latest_execution_checkpoint.schema_version raw -o - "$payload" 2>/dev/null || true)"
    checkpoint_id="$(plutil -extract chain_proof.latest_execution_checkpoint.checkpoint_id raw -o - "$payload" 2>/dev/null || true)"
    height="$(plutil -extract chain_proof.latest_execution_checkpoint.height raw -o - "$payload" 2>/dev/null || true)"
    manifest_hash="$(plutil -extract chain_proof.latest_execution_checkpoint.manifest_hash raw -o - "$payload" 2>/dev/null || true)"
    rm -f "$payload"
    if ! [[ "$schema" =~ ^[0-9]+$ ]] || (( schema < 2 )); then
      echo "provider_checkpoint_gate=$name invalid_schema_version" >&2
      return 1
    fi
    checkpoint_id_without_whitespace="${checkpoint_id//[[:space:]]/}"
    if [[ -z "$checkpoint_id_without_whitespace" || ! "$manifest_hash" =~ ^[[:xdigit:]]{64}$ ]]; then
      echo "provider_checkpoint_gate=$name invalid_checkpoint_identity" >&2
      return 1
    fi
    if ! [[ "$height" =~ ^[0-9]+$ ]] || (( height <= 0 )); then
      echo "provider_checkpoint_gate=$name invalid_checkpoint_height" >&2
      return 1
    fi
    manifest_hash="$(tr '[:upper:]' '[:lower:]' <<<"$manifest_hash")"
    if [[ "$name" == sequencer ]]; then
      sequencer_id="$checkpoint_id"
      sequencer_height="$height"
      sequencer_hash="$manifest_hash"
    else
      storage_id="$checkpoint_id"
      storage_height="$height"
      storage_hash="$manifest_hash"
    fi
  done
  if [[ "$sequencer_id" != "$storage_id" || "$sequencer_hash" != "$storage_hash" ]]; then
    echo "provider_checkpoint_gate identity_mismatch" >&2
    return 1
  fi
  if (( sequencer_height - storage_height > %d || storage_height - sequencer_height > %d )); then
    echo "provider_checkpoint_gate height_incompatible" >&2
    return 1
  fi
  if [[ "$sequencer_id" != %s || "$storage_id" != %s || "$sequencer_hash" != %s || "$storage_hash" != %s || "$sequencer_height" != %s || "$storage_height" != %s ]]; then
    echo "provider_checkpoint_gate closure_receipt_identity_mismatch" >&2
    return 1
  fi
  echo "provider_checkpoint_gate=passed checkpoint_id=$sequencer_id height_delta=$(( sequencer_height - storage_height < 0 ? storage_height - sequencer_height : sequencer_height - storage_height )) hash_size_binding_verified=true missing_hashes=[]"
}

provider_checkpoint_gate_macos
""" % (
        sequencer_status_url,
        storage_status_url,
        MAX_PROVIDER_CHECKPOINT_HEIGHT_DELTA,
        MAX_PROVIDER_CHECKPOINT_HEIGHT_DELTA,
        shlex.quote(str(closure_receipt["checkpoint_id"])),
        shlex.quote(str(closure_receipt["checkpoint_id"])),
        shlex.quote(str(closure_receipt["manifest_hash"]).lower()),
        shlex.quote(str(closure_receipt["manifest_hash"]).lower()),
        shlex.quote(str(closure_receipt["height"])),
        shlex.quote(str(closure_receipt["height"])),
    )


def observer_checkpoint_gate_powershell(
    sequencer_status_url: str,
    storage_status_url: str,
    closure_receipt: dict[str, Any],
) -> str:
    def ps_literal(value: str) -> str:
        return "'" + value.replace("'", "''") + "'"

    return f'''function Get-ProviderCheckpoint {{
  param([Parameter(Mandatory = $true)] [string] $Name, [Parameter(Mandatory = $true)] [string] $Url)
  try {{ $status = Invoke-RestMethod -UseBasicParsing -Uri $Url -TimeoutSec 10 }} catch {{ throw "provider_checkpoint_gate=$Name collection_failed=$($_.Exception.Message)" }}
  $checkpoint = $status.chain_proof.latest_execution_checkpoint
  if ($null -eq $checkpoint) {{ throw "provider_checkpoint_gate=$Name missing_latest_execution_checkpoint" }}
  if ($checkpoint.schema_version -isnot [int] -and $checkpoint.schema_version -isnot [long]) {{ throw "provider_checkpoint_gate=$Name invalid_schema_version" }}
  if ([int64]$checkpoint.schema_version -lt 2) {{ throw "provider_checkpoint_gate=$Name invalid_schema_version" }}
  $checkpointId = [string]$checkpoint.checkpoint_id
  $manifestHash = [string]$checkpoint.manifest_hash
  if ([string]::IsNullOrWhiteSpace($checkpointId) -or $manifestHash -notmatch '^[0-9a-fA-F]{{64}}$') {{ throw "provider_checkpoint_gate=$Name invalid_checkpoint_identity" }}
  if ($checkpoint.height -isnot [int] -and $checkpoint.height -isnot [long]) {{ throw "provider_checkpoint_gate=$Name invalid_checkpoint_height" }}
  $height = [int64]$checkpoint.height
  if ($height -le 0) {{ throw "provider_checkpoint_gate=$Name invalid_checkpoint_height" }}
  return [PSCustomObject]@{{ checkpoint_id = $checkpointId; height = $height; manifest_hash = $manifestHash.ToLowerInvariant() }}
}}
$sequencerCheckpoint = Get-ProviderCheckpoint -Name 'sequencer' -Url {ps_literal(sequencer_status_url)}
$storageCheckpoint = Get-ProviderCheckpoint -Name 'storage' -Url {ps_literal(storage_status_url)}
if ($sequencerCheckpoint.checkpoint_id -ne $storageCheckpoint.checkpoint_id -or $sequencerCheckpoint.manifest_hash -ne $storageCheckpoint.manifest_hash) {{ throw 'provider_checkpoint_gate identity_mismatch' }}
if ([Math]::Abs($sequencerCheckpoint.height - $storageCheckpoint.height) -gt {MAX_PROVIDER_CHECKPOINT_HEIGHT_DELTA}) {{ throw 'provider_checkpoint_gate height_incompatible' }}
if ($sequencerCheckpoint.checkpoint_id -ne {ps_literal(str(closure_receipt['checkpoint_id']))} -or $storageCheckpoint.checkpoint_id -ne {ps_literal(str(closure_receipt['checkpoint_id']))} -or $sequencerCheckpoint.manifest_hash -ne {ps_literal(str(closure_receipt['manifest_hash']).lower())} -or $storageCheckpoint.manifest_hash -ne {ps_literal(str(closure_receipt['manifest_hash']).lower())} -or $sequencerCheckpoint.height -ne {int(closure_receipt['height'])} -or $storageCheckpoint.height -ne {int(closure_receipt['height'])}) {{ throw 'provider_checkpoint_gate closure_receipt_identity_mismatch' }}
Write-Output "provider_checkpoint_gate=passed checkpoint_id=$($sequencerCheckpoint.checkpoint_id) height_delta=$([Math]::Abs($sequencerCheckpoint.height - $storageCheckpoint.height)) hash_size_binding_verified=true missing_hashes=[]"
'''


def linux_command(
    node: dict[str, Any],
    linux_asset: Path,
    linux_ops_tools_asset: Path,
    version: str,
    commit: str,
    run_id: str,
    readiness_policy: str,
    package_deb: str | None = None,
    ops_tools_tar: str | None = None,
    script_path: str | None = None,
) -> list[str]:
    node_root = str(node.get("node_root") or "")
    if not node_root:
        die(f"linux node {node.get('name', '<unnamed>')} missing node_root")
    command = [
        script_path or str(ROOT_DIR / "scripts" / "p2p-public-testnet-package-node-upgrade.sh"),
        "--node-root",
        node_root,
        "--package-deb",
        package_deb or str(linux_asset),
        "--ops-tools-tar",
        ops_tools_tar or str(linux_ops_tools_asset),
        "--package-version",
        version,
        "--commit",
        commit,
        "--run-id",
        run_id,
        "--artifact-ref",
        artifact_ref("linux-x64", version, linux_asset.name, "oasis7_chain_runtime"),
    ]
    service = str(node.get("systemd_service") or "")
    if node.get("restart", False):
        if not service:
            die(f"linux node {node.get('name', '<unnamed>')} has restart=true but no systemd_service")
        command.extend(["--systemd-service", service, "--restart-service"])
        status_url = str(node.get("status_url") or "")
        if readiness_policy == "strict-ready" and not status_url:
            die(f"linux node {node.get('name', '<unnamed>')} uses strict-ready but has no status_url")
        if readiness_policy == "strict-ready":
            healthz_url = str(node.get("healthz_url") or "")
            if not healthz_url:
                status_suffix = "/v1/chain/status"
                if not status_url.endswith(status_suffix):
                    die(
                        f"linux node {node.get('name', '<unnamed>')} strict-ready status_url "
                        "must end with /v1/chain/status when healthz_url is omitted"
                    )
                healthz_url = status_url[: -len(status_suffix)] + "/healthz"
            if not healthz_url.endswith("/healthz"):
                die(
                    f"linux node {node.get('name', '<unnamed>')} strict-ready healthz_url "
                    "must end with /healthz"
                )
            command.extend(["--post-restart-health-url", healthz_url])
            timeout_secs = str(node.get("post_restart_timeout_secs") or 120)
            command.extend(["--post-restart-timeout-secs", timeout_secs])
    return command


def linux_plan_commands(
    node: dict[str, Any],
    linux_asset: Path,
    linux_ops_tools_asset: Path,
    version: str,
    commit: str,
    run_id: str,
    readiness_policy: str,
    known_hosts_path: Path | None = None,
) -> list[str]:
    host = str(node.get("host") or "")
    if not host:
        return [shell_join(linux_command(node, linux_asset, linux_ops_tools_asset, version, commit, run_id, readiness_policy))]
    user = str(node.get("user") or "root")
    operator_known_hosts = validate_operator_known_hosts(known_hosts_path)
    helper_asset = ROOT_DIR / "scripts" / "p2p-public-testnet-package-node-upgrade.sh"
    direct_helper_assets = [
        ("safe_extract", ROOT_DIR / "scripts" / "p2p-safe-extract-tar.py"),
        ("archive_policy", ROOT_DIR / "scripts" / "safe_git_archive.py"),
        ("safe_validate", ROOT_DIR / "scripts" / "p2p-safe-validate-deb-tree.py"),
        ("verify_bundle", ROOT_DIR / "scripts" / "p2p-verify-linux-package-bundle.py"),
        ("rebuild_checksums", ROOT_DIR / "scripts" / "p2p-rebuild-linux-bundle-checksums.py"),
    ]
    local_assets = [("upgrade helper", helper_asset), *direct_helper_assets]
    for description, asset in local_assets:
        if asset.is_symlink() or not asset.is_file() or not stat.S_ISREG(asset.lstat().st_mode):
            die(f"missing Linux package {description}: {asset}")
    node_root = str(node.get("node_root") or "")
    if not node_root:
        die(f"linux node {node.get('name', '<unnamed>')} missing node_root")

    package_name = linux_asset.name
    ops_tools_name = linux_ops_tools_asset.name
    helper_name = helper_asset.name
    package_sha = sha256_file(linux_asset)
    ops_tools_sha = sha256_file(linux_ops_tools_asset)
    helper_sha = sha256_file(helper_asset)
    package_size = linux_asset.stat().st_size
    ops_tools_size = linux_ops_tools_asset.stat().st_size
    helper_size = helper_asset.stat().st_size
    direct_helper_metadata = [
        (label, asset.name, sha256_file(asset), asset.stat().st_size)
        for label, asset in direct_helper_assets
    ]
    direct_helper_names = [name for _, name, _, _ in direct_helper_metadata]

    def remote_file(name: str) -> str:
        return f'"$stage/{name}"'

    def remote_quote(value: str) -> str:
        return shlex.quote(value)

    if node.get("restart", False):
        status_url = str(node.get("status_url") or "")
        healthz_url = str(node.get("healthz_url") or "")
        if not healthz_url:
            status_suffix = "/v1/chain/status"
            if not status_url.endswith(status_suffix):
                die(
                    f"linux node {node.get('name', '<unnamed>')} remote restart requires "
                    "healthz_url or status_url ending with /v1/chain/status"
                )
            healthz_url = status_url[: -len(status_suffix)] + "/healthz"
        if not healthz_url.endswith("/healthz"):
            die(
                f"linux node {node.get('name', '<unnamed>')} remote healthz_url must end with /healthz"
            )

    remote_invocation = " ".join(
        [
            "bash",
            remote_file(helper_name),
            "--node-root",
            remote_quote(node_root),
            "--package-deb",
            remote_file(package_name),
            "--ops-tools-tar",
            remote_file(ops_tools_name),
            "--package-version",
            remote_quote(version),
            "--commit",
            remote_quote(commit),
            "--run-id",
            remote_quote(run_id),
            "--artifact-ref",
            remote_quote(artifact_ref("linux-x64", version, linux_asset.name, "oasis7_chain_runtime")),
        ]
    )
    service = str(node.get("systemd_service") or "")
    if node.get("restart", False):
        if not service:
            die(f"linux node {node.get('name', '<unnamed>')} has restart=true but no systemd_service")
        remote_invocation += (
            f" --systemd-service {remote_quote(service)} --restart-service"
            f" --post-restart-health-url {remote_quote(healthz_url)}"
            f" --post-restart-timeout-secs {remote_quote(str(node.get('post_restart_timeout_secs') or 120))}"
        )

    remote_body_lines = [
        "set -euo pipefail",
        'stage="$(mktemp -d /tmp/oasis7-package-rollout.XXXXXX)"',
        'cleanup() { rm -rf "$stage"; }',
        "trap cleanup EXIT",
        'tar -xzf - -C "$stage"',
        f'test -f {remote_file(package_name)}',
        f'test -f {remote_file(ops_tools_name)}',
        f'test -f {remote_file(helper_name)}',
    ]
    remote_body_lines.extend(
        f'test -f {remote_file(name)}' for name in direct_helper_names
    )
    remote_body_lines.extend(
        [
            f"expected_package_sha={remote_quote(package_sha)}",
            f"expected_ops_tools_sha={remote_quote(ops_tools_sha)}",
            f"expected_helper_sha={remote_quote(helper_sha)}",
            f"expected_package_size={remote_quote(str(package_size))}",
            f"expected_ops_tools_size={remote_quote(str(ops_tools_size))}",
            f"expected_helper_size={remote_quote(str(helper_size))}",
        ]
    )
    remote_body_lines.extend(
        f"expected_{label}_sha={remote_quote(digest)}\nexpected_{label}_size={remote_quote(str(size))}"
        for label, _, digest, size in direct_helper_metadata
    )
    remote_body_lines.extend(
        [
            f'test "$(sha256sum {remote_file(package_name)} | awk \'{{print $1}}\')" = "$expected_package_sha"',
            f'test "$(sha256sum {remote_file(ops_tools_name)} | awk \'{{print $1}}\')" = "$expected_ops_tools_sha"',
            f'test "$(sha256sum {remote_file(helper_name)} | awk \'{{print $1}}\')" = "$expected_helper_sha"',
            f'test "$(stat -c %s {remote_file(package_name)})" = "$expected_package_size"',
            f'test "$(stat -c %s {remote_file(ops_tools_name)})" = "$expected_ops_tools_size"',
            f'test "$(stat -c %s {remote_file(helper_name)})" = "$expected_helper_size"',
        ]
    )
    for label, name, _, _ in direct_helper_metadata:
        remote_body_lines.extend(
            [
                f'test "$(sha256sum {remote_file(name)} | awk \'{{print $1}}\')" = "$expected_{label}_sha"',
                f'test "$(stat -c %s {remote_file(name)})" = "$expected_{label}_size"',
            ]
        )
    remote_body_lines.extend(
        [
            "printf '%s\\n' streamed_envelope_sha256_verified=true",
            "printf '%s\\n' streamed_envelope_size_verified=true",
            "printf '%s\\n' streamed_envelope_complete=true",
            f"exec {remote_invocation}",
        ]
    )
    remote_body = "\n".join(remote_body_lines)
    provider_env_name = re.sub(
        r"[^A-Z0-9_]",
        "_",
        str(node.get("name") or "LINUX_PROVIDER").upper(),
    )
    credential_env = f"PUBLIC_TESTNET_{provider_env_name}_SSHPASS"
    tar_command = (
        f"tar -czf - -C {remote_quote(str(linux_asset.parent))} {remote_quote(package_name)}"
        f" {remote_quote(ops_tools_name)} -C {remote_quote(str(helper_asset.parent))}"
        f" {remote_quote(helper_name)}"
        + " ".join(f" {remote_quote(name)}" for name in direct_helper_names)
    )
    ssh_command = (
        f'SSHPASS="${{{credential_env}:?{credential_env} is required}}" '
        + shell_join(
            [
                "sshpass",
                "-e",
                "ssh",
                "-o",
                "HostKeyAlgorithms=ssh-ed25519",
                "-o",
                "StrictHostKeyChecking=yes",
                "-o",
                f"UserKnownHostsFile={operator_known_hosts}",
                "-o",
                "GlobalKnownHostsFile=/dev/null",
                "-o",
                "PreferredAuthentications=password",
                "-o",
                "PubkeyAuthentication=no",
                "-o",
                "NumberOfPasswordPrompts=1",
                "-o",
                "ControlMaster=no",
                "-o",
                "ControlPath=none",
                f"{user}@{host}",
                "bash",
                "-c",
                remote_quote(remote_body),
            ]
        )
    )
    return [f"set -o pipefail; {tar_command} | {ssh_command}"]


def write_linux_observer_plan(
    out_dir: Path,
    node: dict[str, Any],
    linux_asset: Path,
    linux_ops_tools_asset: Path,
    version: str,
    commit: str,
    run_id: str,
    readiness_policy: str,
    sequencer_status_url: str,
    storage_status_url: str,
    closure_receipt: dict[str, Any],
) -> tuple[Path, list[str], list[str]]:
    name = str(node.get("name") or "linux-observer")
    safe_name = "".join(ch if ch.isalnum() or ch in "._-" else "-" for ch in name)
    script_path = out_dir / f"{safe_name}-linux-observer-upgrade.sh"
    host = str(node.get("host") or "")
    if host:
        user = str(node.get("user") or "root")
        remote_package = str(node.get("remote_package") or linux_asset.name)
        remote_ops_tools = str(node.get("remote_ops_tools") or linux_ops_tools_asset.name)
        remote_script = str(node.get("remote_script") or "./scripts/p2p-public-testnet-package-node-upgrade.sh")
        command = linux_command(
            node,
            linux_asset,
            linux_ops_tools_asset,
            version,
            commit,
            run_id,
            readiness_policy,
            package_deb=remote_package,
            ops_tools_tar=remote_ops_tools,
            script_path=remote_script,
        )
        remote_wrapper = str(node.get("remote_observer_gate_script") or script_path.name)
        commands = [
            shell_join(["scp", str(linux_asset), f"{user}@{host}:{remote_package}"]),
            shell_join(["scp", str(linux_ops_tools_asset), f"{user}@{host}:{remote_ops_tools}"]),
            shell_join(["scp", str(script_path), f"{user}@{host}:{remote_wrapper}"]),
            shell_join(["ssh", f"{user}@{host}", "bash", remote_wrapper]),
        ]
    else:
        command = linux_command(node, linux_asset, linux_ops_tools_asset, version, commit, run_id, readiness_policy)
        commands = [shell_join(["bash", str(script_path)])]
    script_path.write_text(
        "#!/usr/bin/env bash\nset -euo pipefail\n\n"
        + observer_checkpoint_gate_bash(
            sequencer_status_url, storage_status_url, closure_receipt
        )
        + "\n\nexec "
        + shell_join(command)
        + "\n",
        encoding="utf-8",
    )
    script_path.chmod(0o755)
    return script_path, commands, command


def windows_script(
    node: dict[str, Any],
    installer_name: str,
    version: str,
    commit: str,
    run_id: str,
    governed_hashes: dict[str, str],
    readiness_policy: str,
    sequencer_status_url: str,
    storage_status_url: str,
    closure_receipt: dict[str, Any],
) -> str:
    def ps_literal(value: str) -> str:
        return "'" + value.replace("'", "''") + "'"

    def ps_expanded_path(value: str) -> str:
        environment_form = re.sub(
            r"\$env:([A-Za-z_][A-Za-z0-9_]*)",
            lambda match: f"%{match.group(1)}%",
            value,
            flags=re.IGNORECASE,
        )
        return f"[Environment]::ExpandEnvironmentVariables({ps_literal(environment_form)})"

    deploy_root = str(node.get("deploy_root") or r"C:\oasis7-deploy")
    staging_root = str(
        node.get("staging_root")
        or (deploy_root.rstrip("\\/") + r"\staging\package-rollout\manual")
    )
    task_name = str(node.get("scheduled_task") or "Oasis7Observer")
    status_url = str(node.get("status_url") or "")
    install_root = str(node.get("install_root") or "$env:LOCALAPPDATA\\Programs\\oasis7")
    installer_path = str(node.get("installer_path") or f"$env:USERPROFILE\\{installer_name}")
    configured_installer_path = str(node.get("configured_installer_path") or installer_path)
    governed_bundle_path = str(node.get("governed_bundle_path") or WINDOWS_GOVERNED_BUNDLE)
    governed_genesis_path = str(node.get("governed_genesis_path") or WINDOWS_GOVERNED_GENESIS)
    governed_manifest_path = str(node.get("governed_manifest_path") or WINDOWS_GOVERNED_MANIFEST)
    governed_bootstrap_path = str(node.get("governed_bootstrap_path") or WINDOWS_GOVERNED_BOOTSTRAP)
    active_governed_bundle_path = str(
        node.get("active_governed_bundle_path") or WINDOWS_GOVERNED_BUNDLE
    )
    active_governed_genesis_path = str(
        node.get("active_governed_genesis_path") or WINDOWS_GOVERNED_GENESIS
    )
    active_governed_manifest_path = str(
        node.get("active_governed_manifest_path") or WINDOWS_GOVERNED_MANIFEST
    )
    active_governed_bootstrap_path = str(
        node.get("active_governed_bootstrap_path") or WINDOWS_GOVERNED_BOOTSTRAP
    )
    rollback_backup_root = str(
        node.get("rollback_backup_root")
        or (deploy_root.rstrip("\\/") + r"\backups\known-good")
    )
    try:
        rollback_unlock_timeout_secs = int(node.get("rollback_unlock_timeout_secs") or 30)
    except (TypeError, ValueError):
        die(f"windows node {node.get('name', '<unnamed>')} has invalid rollback_unlock_timeout_secs")
    if not 1 <= rollback_unlock_timeout_secs <= 30:
        die(
            f"windows node {node.get('name', '<unnamed>')} rollback_unlock_timeout_secs "
            "must be between 1 and 30"
        )
    ref = artifact_ref("windows-x64", version, installer_name, "oasis7_chain_runtime.exe")
    if readiness_policy in ("rpc-running", "strict-ready") and not status_url:
        die(
            f"windows node {node.get('name', '<unnamed>')} uses {readiness_policy} "
            "but has no status_url"
        )
    try:
        verification_timeout_secs = int(node.get("post_restart_timeout_secs") or 120)
    except (TypeError, ValueError):
        die(f"windows node {node.get('name', '<unnamed>')} has invalid post_restart_timeout_secs")
    if not 60 <= verification_timeout_secs <= 300:
        die(
            f"windows node {node.get('name', '<unnamed>')} post_restart_timeout_secs "
            "must be between 60 and 300"
        )
    require_strict_ready = "$true" if readiness_policy == "strict-ready" else "$false"
    require_rpc_running = "$true" if readiness_policy in ("rpc-running", "strict-ready") else "$false"
    integrity_entries = "\n".join(
        f"  {ps_literal(path)} = {ps_literal(digest)}"
        for path, digest in sorted(governed_hashes.items())
    )
    checkpoint_gate = observer_checkpoint_gate_powershell(
        sequencer_status_url, storage_status_url, closure_receipt
    )
    return f"""$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'

{checkpoint_gate}

function Set-JsonProperty {{
  param(
    [Parameter(Mandatory = $true)] [object] $Object,
    [Parameter(Mandatory = $true)] [string] $Name,
    [Parameter(Mandatory = $true)] $Value
  )
  if ($Object.PSObject.Properties.Name -contains $Name) {{
    $Object.$Name = $Value
  }} else {{
    $Object | Add-Member -NotePropertyName $Name -NotePropertyValue $Value
  }}
}}

function Get-TreeMetadata {{
  param([Parameter(Mandatory = $true)] [string] $Root)
  $rootItem = Get-Item -LiteralPath $Root -ErrorAction Stop
  $files = @(Get-ChildItem -LiteralPath $rootItem.FullName -File -Recurse | Sort-Object FullName)
  $stream = New-Object System.IO.MemoryStream
  $totalBytes = [int64]0
  foreach ($file in $files) {{
    $relative = $file.FullName.Substring($rootItem.FullName.Length).TrimStart('\\').Replace('\\', '/')
    $fileHash = (Get-FileHash -LiteralPath $file.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
    $payload = [System.Text.Encoding]::UTF8.GetBytes($relative + [char]0 + $fileHash + [char]0 + [string]$file.Length + "`n")
    $stream.Write($payload, 0, $payload.Length)
    $totalBytes += $file.Length
  }}
  $sha = [System.Security.Cryptography.SHA256]::Create()
  try {{
    $treeHash = ([System.BitConverter]::ToString($sha.ComputeHash($stream.ToArray()))).Replace('-', '').ToLowerInvariant()
  }} finally {{
    $sha.Dispose()
    $stream.Dispose()
  }}
  return [PSCustomObject]@{{ sha256_tree = $treeHash; file_count = $files.Count; total_bytes = $totalBytes }}
}}

function Assert-TreeIntegrity {{
  param([object] $Metadata, [string] $Path, [string] $Label)
  # Dynamic labels produce "world_snapshot tree integrity mismatch" and
  # "generated_world_sidecar tree integrity mismatch" diagnostics.
  $actual = Get-TreeMetadata -Root $Path
  if ($actual.sha256_tree -ne $Metadata.sha256_tree -or
      $actual.file_count -ne $Metadata.file_count -or
      $actual.total_bytes -ne $Metadata.total_bytes) {{
    throw "$Label tree integrity mismatch: path=$Path expected_hash=$($Metadata.sha256_tree) actual_hash=$($actual.sha256_tree) expected_files=$($Metadata.file_count) actual_files=$($actual.file_count) expected_bytes=$($Metadata.total_bytes) actual_bytes=$($actual.total_bytes)"
  }}
}}

function Assert-FileMetadata {{
  param([object] $Metadata, [string] $Path, [string] $Label)
  if (!(Test-Path -LiteralPath $Path -PathType Leaf)) {{ throw "$Label file missing: $Path" }}
  $actualHash = (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant()
  $actualSize = (Get-Item -LiteralPath $Path).Length
  if ($actualHash -ne $Metadata.sha256 -or $actualSize -ne $Metadata.size_bytes) {{
    throw "$Label file integrity mismatch: path=$Path expected_hash=$($Metadata.sha256) actual_hash=$actualHash expected_bytes=$($Metadata.size_bytes) actual_bytes=$actualSize"
  }}
}}

function Set-ArtifactLocation {{
  param([object] $Metadata, [string] $Path, [string] $Ref)
  if ($null -eq $Metadata) {{ return }}
  Set-JsonProperty $Metadata 'ref' $Ref
  Set-JsonProperty $Metadata 'resolved_path' $Path
  Set-JsonProperty $Metadata 'path' $Path
}}

function Test-NodeLocalPath {{
  param([Parameter(Mandatory = $true)] [string] $Path)
  if ([string]::IsNullOrWhiteSpace($Path) -or ![System.IO.Path]::IsPathRooted($Path)) {{
    return $false
  }}
  try {{
    $candidate = [System.IO.Path]::GetFullPath([Environment]::ExpandEnvironmentVariables($Path)).TrimEnd('\\')
  }} catch {{
    return $false
  }}
  $allowedRoots = @($deployRoot, $installRoot)
  foreach ($allowedRoot in $allowedRoots) {{
    if ([string]::IsNullOrWhiteSpace($allowedRoot) -or ![System.IO.Path]::IsPathRooted($allowedRoot)) {{ continue }}
    $root = [System.IO.Path]::GetFullPath([Environment]::ExpandEnvironmentVariables($allowedRoot)).TrimEnd('\\')
    if ($candidate.Equals($root, [System.StringComparison]::OrdinalIgnoreCase) -or
        $candidate.StartsWith($root + '\\', [System.StringComparison]::OrdinalIgnoreCase)) {{
      return $true
    }}
  }}
  return $false
}}

function Assert-NodeLocalPhysicalPath {{
  param(
    [Parameter(Mandatory = $true)] [string] $Path,
    [Parameter(Mandatory = $true)] [string] $Label
  )
  if (!(Test-NodeLocalPath -Path $Path)) {{
    throw "$Label is not node-local: $Path"
  }}
  $candidate = [System.IO.Path]::GetFullPath([Environment]::ExpandEnvironmentVariables($Path))
  $probe = $candidate
  while (![string]::IsNullOrWhiteSpace($probe)) {{
    $item = Get-Item -LiteralPath $probe -Force -ErrorAction SilentlyContinue
    if ($null -ne $item -and
        (($item.Attributes -band [System.IO.FileAttributes]::ReparsePoint) -ne 0)) {{
      throw "$Label contains reparse-point component: $probe"
    }}
    $parent = [System.IO.Path]::GetDirectoryName($probe)
    if ([string]::IsNullOrWhiteSpace($parent) -or
        $parent.Equals($probe, [System.StringComparison]::OrdinalIgnoreCase)) {{
      break
    }}
    $probe = $parent
  }}
  return $candidate
}}

function Preserve-AttemptDiagnostics {{
  param([string] $StdoutPath, [string] $StderrPath, [string] $ExitMarkerPath)
  foreach ($diagnosticPath in @($StdoutPath, $StderrPath, $ExitMarkerPath)) {{
    if (![string]::IsNullOrWhiteSpace($diagnosticPath) -and (Test-Path -LiteralPath $diagnosticPath)) {{
      Write-Output "preserved_attempt_diagnostic=$diagnosticPath"
    }}
  }}
}}

$version = {ps_literal(version)}
$commit = {ps_literal(commit)}
$runId = {ps_literal(run_id)}
$artifactRef = {ps_literal(ref)}
$installRoot = {ps_expanded_path(install_root)}
$runtime = Join-Path $installRoot 'bin\\oasis7_chain_runtime.exe'
$installer = {ps_expanded_path(configured_installer_path)}
# Preserve the configured destination as plan evidence, then consume only the staged closure.
$installer = {ps_expanded_path(installer_path)}
$deployRoot = {ps_expanded_path(deploy_root)}
$stagingRoot = {ps_expanded_path(staging_root)}
$taskName = {ps_literal(task_name)}
$bundlePath = {ps_expanded_path(governed_bundle_path)}
$genesisPath = {ps_expanded_path(governed_genesis_path)}
$manifestPath = {ps_expanded_path(governed_manifest_path)}
$bootstrapPath = {ps_expanded_path(governed_bootstrap_path)}
$activeBundlePath = {ps_expanded_path(active_governed_bundle_path)}
$activeGenesisPath = {ps_expanded_path(active_governed_genesis_path)}
$activeManifestPath = {ps_expanded_path(active_governed_manifest_path)}
$activeBootstrapPath = {ps_expanded_path(active_governed_bootstrap_path)}
$rollbackBackupRoot = {ps_expanded_path(rollback_backup_root)}
$rollbackUnlockTimeoutSeconds = {rollback_unlock_timeout_secs}
$statusUrl = {ps_literal(status_url)}
$requireStrictReady = {require_strict_ready}
$requireRpcRunning = {require_rpc_running}
$verificationTimeoutSeconds = {verification_timeout_secs}
$activeConfigRoot = [System.IO.Path]::GetFullPath((Join-Path $deployRoot 'config')).TrimEnd('\\')
$stagingConfigRoot = [System.IO.Path]::GetFullPath((Join-Path $stagingRoot 'config')).TrimEnd('\\')
$stagingConfigPrefix = $stagingConfigRoot + '\\'
$physicalPreflightTargets = @(
  [PSCustomObject]@{{ path = $stagingRoot; label = 'staging root' }},
  [PSCustomObject]@{{ path = $stagingConfigRoot; label = 'staging config root' }},
  [PSCustomObject]@{{ path = $deployRoot; label = 'active deploy root' }},
  [PSCustomObject]@{{ path = $activeConfigRoot; label = 'active config root' }},
  [PSCustomObject]@{{ path = $installRoot; label = 'install root' }},
  [PSCustomObject]@{{ path = $runtime; label = 'runtime path' }},
  [PSCustomObject]@{{ path = $activeBundlePath; label = 'active config bundle path' }},
  [PSCustomObject]@{{ path = $activeGenesisPath; label = 'active config genesis path' }},
  [PSCustomObject]@{{ path = $activeManifestPath; label = 'active config manifest path' }},
  [PSCustomObject]@{{ path = $activeBootstrapPath; label = 'active config bootstrap path' }},
  [PSCustomObject]@{{ path = $rollbackBackupRoot; label = 'rollback root' }}
)
foreach ($physicalPreflightTarget in $physicalPreflightTargets) {{
  Assert-NodeLocalPhysicalPath `
    -Path ([string]$physicalPreflightTarget.path) `
    -Label ([string]$physicalPreflightTarget.label) | Out-Null
}}
$logRoot = Join-Path $deployRoot 'logs\\package-rollout-attempts'
Assert-NodeLocalPhysicalPath -Path $logRoot -Label 'active deploy log root' | Out-Null
New-Item -ItemType Directory -Path $logRoot -Force | Out-Null
$attemptId = [Guid]::NewGuid().ToString('N')
$attemptStdoutPath = Join-Path $logRoot "$attemptId.stdout.log"
$attemptStderrPath = Join-Path $logRoot "$attemptId.stderr.log"
$attemptExitMarkerPath = Join-Path $logRoot "$attemptId.exit"
$attemptWrapperPath = Join-Path $logRoot "$attemptId.wrapper.ps1"
[System.IO.File]::AppendAllText($attemptStdoutPath, "attempt_id=$attemptId phase=staging_preflight" + [Environment]::NewLine)
$expectedGovernedSha256 = @{{
{integrity_entries}
}}

$transformedConfigTargets = @(
  [System.IO.Path]::GetFullPath($activeBundlePath),
  [System.IO.Path]::GetFullPath($activeGenesisPath),
  [System.IO.Path]::GetFullPath($activeManifestPath)
)
$transformedConfigTargetSet = [System.Collections.Generic.HashSet[string]]::new(
  [System.StringComparer]::OrdinalIgnoreCase
)
$rollbackConfigTargetSet = [System.Collections.Generic.HashSet[string]]::new(
  [System.StringComparer]::OrdinalIgnoreCase
)
$stagedToActiveConfigTargets = [System.Collections.Generic.Dictionary[string, string]]::new(
  [System.StringComparer]::OrdinalIgnoreCase
)

foreach ($entry in $expectedGovernedSha256.GetEnumerator()) {{
  try {{
    $stagedConfigSource = [System.IO.Path]::GetFullPath([string]$entry.Key)
    if (!$stagedConfigSource.StartsWith($stagingConfigPrefix, [System.StringComparison]::OrdinalIgnoreCase)) {{
      throw "staged governed source escapes staging config root: $stagedConfigSource"
    }}
    $stagedConfigRelativePath = $stagedConfigSource.Substring($stagingConfigPrefix.Length)
    if ([string]::IsNullOrWhiteSpace($stagedConfigRelativePath)) {{
      throw "staged governed source has empty staging-relative path: $stagedConfigSource"
    }}
    $activeConfigTarget = [System.IO.Path]::GetFullPath((Join-Path $activeConfigRoot $stagedConfigRelativePath))
    Assert-NodeLocalPhysicalPath -Path $stagedConfigSource -Label 'staging governed source' | Out-Null
    if (!(Test-Path -LiteralPath $stagedConfigSource -PathType Leaf)) {{
      throw "staged governed bootstrap source missing: $stagedConfigSource"
    }}
    $actual = (Get-FileHash -LiteralPath $stagedConfigSource -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($actual -ne $entry.Value) {{
      throw "staged governed bootstrap integrity mismatch: $stagedConfigSource"
    }}
    $stagedToActiveConfigTargets[$stagedConfigSource] = $activeConfigTarget
    $rollbackConfigTargetSet.Add($activeConfigTarget) | Out-Null
  }} catch {{
    $stagingPreflightDiagnostic = @(
      'failure_phase=staging_preflight',
      "staging_preflight_failed_path=$($entry.Key)",
      "staging_preflight_error=$($_.Exception.Message)",
      'staging_preflight_exit_code=1',
      'rollback_required=true'
    ) -join ' '
    [System.IO.File]::AppendAllText($attemptStderrPath, $stagingPreflightDiagnostic + [Environment]::NewLine)
    [System.IO.File]::AppendAllText($attemptExitMarkerPath, '1' + [Environment]::NewLine)
    Write-Output 'rollback_required=true'
    throw
  }}
}}
foreach ($transformedConfigTarget in $transformedConfigTargets) {{
  $transformedConfigTargetSet.Add($transformedConfigTarget) | Out-Null
  $rollbackConfigTargetSet.Add($transformedConfigTarget) | Out-Null
}}
$rollbackConfigTargets = @($rollbackConfigTargetSet | Sort-Object)

$bundle = Get-Item $bundlePath -ErrorAction Stop
$json = Get-Content $bundle.FullName -Raw | ConvertFrom-Json
if ($null -eq $json.runtime_build) {{
  throw "governed bundle missing runtime_build: $($bundle.FullName)"
}}
$manifestItem = Get-Item $manifestPath -ErrorAction Stop
$manifestJson = Get-Content $manifestItem.FullName -Raw | ConvertFrom-Json
if ($null -eq $manifestJson.runtime_refs) {{
  throw "governed network manifest missing runtime_refs: $($manifestItem.FullName)"
}}
$networkManifestMetadata = [PSCustomObject]@{{
  ref = [System.IO.Path]::GetFileName($manifestItem.FullName)
  resolved_path = $manifestItem.FullName
  kind = 'file'
  sha256 = $expectedGovernedSha256[$manifestPath]
  size_bytes = $manifestItem.Length
}}

$genesis = Get-Item $genesisPath -ErrorAction Stop
$genesisJson = Get-Content $genesis.FullName -Raw | ConvertFrom-Json
if ($null -eq $genesisJson.governance_bootstrap_refs) {{
  throw "governed genesis missing governance_bootstrap_refs: $($genesis.FullName)"
}}
$evidenceDir = Join-Path $genesis.Directory.FullName 'doc\\testing\\evidence'
$activeEvidenceDir = Join-Path $activeConfigRoot 'doc\\testing\\evidence'
$localizedTargets = @{{}}
$localizedGovernedRefCount = 0
$currentGovernedRefKeys = @(
  'governance_public_manifest_ref',
  'liveops_public_manifest_ref',
  'binding_notes_ref',
  'genesis_validator_registry_ref',
  'topology_ref'
)
foreach ($property in @($genesisJson.governance_bootstrap_refs.PSObject.Properties)) {{
  if ($property.Value -isnot [string] -or [string]::IsNullOrEmpty($property.Value)) {{ continue }}
  $sourcePath = [Environment]::ExpandEnvironmentVariables($property.Value)
  if (![System.IO.Path]::IsPathRooted($sourcePath)) {{
    $sourcePath = Join-Path $genesis.Directory.FullName $sourcePath
  }}
  $sourcePath = [System.IO.Path]::GetFullPath($sourcePath)
  if (!(Test-Path -LiteralPath $sourcePath -PathType Leaf)) {{
    throw "genesis governance ref source missing for $($property.Name): $sourcePath"
  }}
  $targetPath = [System.IO.Path]::GetFullPath((Join-Path $evidenceDir ([System.IO.Path]::GetFileName($sourcePath))))
  if ($localizedTargets.ContainsKey($targetPath) -and $localizedTargets[$targetPath] -ne $sourcePath) {{
    throw "genesis governance refs collide at localized target $targetPath"
  }}
  $localizedTargets[$targetPath] = $sourcePath
  if (!(Test-Path -LiteralPath $targetPath -PathType Leaf)) {{
    throw "localized genesis governance ref target missing for $($property.Name): $targetPath"
  }}
  $property.Value = Join-Path $activeEvidenceDir ([System.IO.Path]::GetFileName($sourcePath))
  $localizedGovernedRefCount += 1
}}
$genesisText = $genesisJson | ConvertTo-Json -Depth 100

$worldSnapshotPath = Join-Path $genesis.Directory.FullName 'generated-world\\world'
$sidecarPath = Join-Path $genesis.Directory.FullName 'generated-world\\generated-scenario-world'
$provenancePath = Join-Path $genesis.Directory.FullName 'generated-world\\world-generation-provenance.json'
$activeWorldSnapshotPath = Join-Path $activeConfigRoot 'generated-world\\world'
$activeSidecarPath = Join-Path $activeConfigRoot 'generated-world\\generated-scenario-world'
$activeProvenancePath = Join-Path $activeConfigRoot 'generated-world\\world-generation-provenance.json'
if ($manifestJson.tier -eq 'public_testnet') {{
  if ($null -eq $json.generated_world_sidecar -or $null -eq $json.world_generation_provenance) {{
    throw 'public_testnet Windows source missing generated_world_sidecar or world_generation_provenance'
  }}
}}
Assert-TreeIntegrity $json.world_snapshot $worldSnapshotPath 'world_snapshot'
Assert-TreeIntegrity $json.generated_world_sidecar $sidecarPath 'generated_world_sidecar'
Assert-FileMetadata $json.world_generation_provenance $provenancePath 'world_generation_provenance'
Assert-FileMetadata $json.governance_manifest (Join-Path $evidenceDir ([System.IO.Path]::GetFileName($json.governance_manifest.ref))) 'governance_manifest'
Assert-FileMetadata $networkManifestMetadata $manifestItem.FullName 'network_manifest'
foreach ($entry in @($json.evidence_refs)) {{
  Assert-FileMetadata $entry (Join-Path $evidenceDir ([System.IO.Path]::GetFileName($entry.ref))) 'evidence_refs'
}}

Set-ArtifactLocation $json.runtime_build $runtime 'bin\\oasis7_chain_runtime.exe'
Set-ArtifactLocation $json.world_snapshot $activeWorldSnapshotPath 'generated-world/world'
Set-ArtifactLocation $json.generated_world_sidecar $activeSidecarPath 'generated-world/generated-scenario-world'
Set-ArtifactLocation $json.world_generation_provenance $activeProvenancePath 'generated-world/world-generation-provenance.json'
Set-ArtifactLocation $json.governance_manifest (Join-Path $activeEvidenceDir ([System.IO.Path]::GetFileName($json.governance_manifest.ref))) ('doc/testing/evidence/' + [System.IO.Path]::GetFileName($json.governance_manifest.ref))
Set-ArtifactLocation $networkManifestMetadata $activeManifestPath ([System.IO.Path]::GetFileName($activeManifestPath))
Set-JsonProperty $json 'network_manifest' $networkManifestMetadata
foreach ($entry in @($json.evidence_refs)) {{
  $entryName = [System.IO.Path]::GetFileName($entry.ref)
  Set-ArtifactLocation $entry (Join-Path $activeEvidenceDir $entryName) ('doc/testing/evidence/' + $entryName)
}}
if ($json.PSObject.Properties.Name -contains 'repo_root') {{ $json.repo_root = $deployRoot }}
$manifestJson.runtime_refs.release_candidate_bundle_ref = $activeBundlePath
$manifestJson.runtime_refs.genesis_ref = $activeGenesisPath
$manifestJson.runtime_refs.bootstrap_peer_ref = $activeBootstrapPath
$manifestJson.runtime_refs.generated_world_sidecar_ref = $activeSidecarPath
$manifestJson.runtime_refs.world_generation_provenance_ref = $activeProvenancePath
$manifestText = $manifestJson | ConvertTo-Json -Depth 100

$structuredPaths = @(
  [PSCustomObject]@{{ field = 'runtime_build.path'; path = $json.runtime_build.path }},
  [PSCustomObject]@{{ field = 'runtime_build.resolved_path'; path = $json.runtime_build.resolved_path }},
  [PSCustomObject]@{{ field = 'world_snapshot.path'; path = $json.world_snapshot.path }},
  [PSCustomObject]@{{ field = 'world_snapshot.resolved_path'; path = $json.world_snapshot.resolved_path }},
  [PSCustomObject]@{{ field = 'generated_world_sidecar.path'; path = $json.generated_world_sidecar.path }},
  [PSCustomObject]@{{ field = 'generated_world_sidecar.resolved_path'; path = $json.generated_world_sidecar.resolved_path }},
  [PSCustomObject]@{{ field = 'world_generation_provenance.path'; path = $json.world_generation_provenance.path }},
  [PSCustomObject]@{{ field = 'world_generation_provenance.resolved_path'; path = $json.world_generation_provenance.resolved_path }},
  [PSCustomObject]@{{ field = 'governance_manifest.path'; path = $json.governance_manifest.path }},
  [PSCustomObject]@{{ field = 'governance_manifest.resolved_path'; path = $json.governance_manifest.resolved_path }},
  [PSCustomObject]@{{ field = 'network_manifest.path'; path = $networkManifestMetadata.path }},
  [PSCustomObject]@{{ field = 'network_manifest.resolved_path'; path = $networkManifestMetadata.resolved_path }},
  [PSCustomObject]@{{ field = 'runtime_refs.release_candidate_bundle_ref'; path = $manifestJson.runtime_refs.release_candidate_bundle_ref }},
  [PSCustomObject]@{{ field = 'runtime_refs.genesis_ref'; path = $manifestJson.runtime_refs.genesis_ref }},
  [PSCustomObject]@{{ field = 'runtime_refs.bootstrap_peer_ref'; path = $manifestJson.runtime_refs.bootstrap_peer_ref }},
  [PSCustomObject]@{{ field = 'runtime_refs.generated_world_sidecar_ref'; path = $manifestJson.runtime_refs.generated_world_sidecar_ref }},
  [PSCustomObject]@{{ field = 'runtime_refs.world_generation_provenance_ref'; path = $manifestJson.runtime_refs.world_generation_provenance_ref }}
)
foreach ($entry in @($json.evidence_refs)) {{
  $structuredPaths += [PSCustomObject]@{{ field = 'evidence_refs.path'; path = $entry.path }}
  $structuredPaths += [PSCustomObject]@{{ field = 'evidence_refs.resolved_path'; path = $entry.resolved_path }}
}}
foreach ($property in @($genesisJson.governance_bootstrap_refs.PSObject.Properties)) {{
  if ($property.Value -is [string] -and ![string]::IsNullOrEmpty($property.Value)) {{
    $structuredPaths += [PSCustomObject]@{{ field = 'governance_bootstrap_refs.' + $property.Name; path = $property.Value }}
  }}
}}
foreach ($entry in $structuredPaths) {{
  if (!(Test-NodeLocalPath -Path $entry.path)) {{
    throw "staged target path is not node-local: field=$($entry.field) path=$($entry.path); build-host absolute path survived Windows localization"
  }}
}}

function Resolve-RollbackRuntimeSource {{
  param([Parameter(Mandatory = $true)] [string] $BackupRoot)
  Assert-NodeLocalPhysicalPath -Path $BackupRoot -Label 'rollback root' | Out-Null
  $backupRootFull = [System.IO.Path]::GetFullPath($BackupRoot).TrimEnd('\\')
  $backupRootPrefix = $backupRootFull + '\\'
  $backupManifestPath = Join-Path $backupRootFull 'backup-manifest.json'
  $backupProvenancePath = Join-Path $backupRootFull 'backup-provenance.json'
  $runtimeCandidate = Join-Path $backupRootFull 'runtime\\oasis7_chain_runtime.exe'
  $binCandidate = Join-Path $backupRootFull 'bin\\oasis7_chain_runtime.exe'
  $metadataPaths = @(
    @($backupManifestPath, $backupProvenancePath) |
      Where-Object {{ Test-Path -LiteralPath $_ -PathType Leaf }}
  )
  if ($metadataPaths.Count -gt 1) {{
    throw "rollback runtime ambiguous: multiple backup manifest/provenance files under $backupRootFull"
  }}
  if ($metadataPaths.Count -eq 1) {{
    Assert-NodeLocalPhysicalPath -Path ([string]$metadataPaths[0]) -Label 'rollback metadata source' | Out-Null
    $backupMetadata = Get-Content -LiteralPath $metadataPaths[0] -Raw | ConvertFrom-Json
    $runtimeRelativePath = ''
    $expectedRuntimeSha256 = ''
    $usesTopLevelRuntimePath = $false
    if ($backupMetadata.PSObject.Properties.Name -contains 'runtime_path') {{
      $usesTopLevelRuntimePath = $true
      $runtimeRelativePath = [string]$backupMetadata.runtime_path
      if ($backupMetadata.PSObject.Properties.Name -contains 'runtime_sha256') {{
        $expectedRuntimeSha256 = [string]$backupMetadata.runtime_sha256
      }}
    }} else {{
      $runtimeMetadata = $backupMetadata.runtime
      if ($null -eq $runtimeMetadata -and $null -ne $backupMetadata.provenance) {{
        $runtimeMetadata = $backupMetadata.provenance.runtime
      }}
      if ($null -eq $runtimeMetadata) {{
        throw "known-good rollback runtime missing from backup manifest/provenance: $($metadataPaths[0])"
      }}
      $runtimeRelativePath = if ($runtimeMetadata.PSObject.Properties.Name -contains 'relative_path') {{
        [string]$runtimeMetadata.relative_path
      }} elseif ($runtimeMetadata.PSObject.Properties.Name -contains 'path') {{
        [string]$runtimeMetadata.path
      }} else {{
        ''
      }}
      if ($runtimeMetadata.PSObject.Properties.Name -contains 'sha256') {{
        $expectedRuntimeSha256 = [string]$runtimeMetadata.sha256
      }}
    }}
    if ([string]::IsNullOrWhiteSpace($runtimeRelativePath)) {{
      throw "known-good rollback runtime missing path in backup manifest/provenance: $($metadataPaths[0])"
    }}
    $manifestCandidate = if ([System.IO.Path]::IsPathRooted($runtimeRelativePath)) {{
      [System.IO.Path]::GetFullPath($runtimeRelativePath)
    }} else {{
      [System.IO.Path]::GetFullPath((Join-Path $backupRootFull $runtimeRelativePath))
    }}
    $manifestCandidateIsConfined = $manifestCandidate.StartsWith(
      $backupRootPrefix,
      [System.StringComparison]::OrdinalIgnoreCase
    )
    if ($usesTopLevelRuntimePath -and
        ([System.IO.Path]::IsPathRooted($runtimeRelativePath) -or !$manifestCandidateIsConfined)) {{
      $legacyConfinedCandidates = @(
        @($runtimeCandidate, $binCandidate) |
          Where-Object {{ Test-Path -LiteralPath $_ -PathType Leaf }}
      )
      if ($legacyConfinedCandidates.Count -eq 0) {{
        throw "confined backup runtime candidate missing for legacy rooted runtime_path: declared=$runtimeRelativePath root=$backupRootFull"
      }}
      if ($legacyConfinedCandidates.Count -gt 1) {{
        throw "confined backup runtime candidate ambiguous for legacy rooted runtime_path: declared=$runtimeRelativePath root=$backupRootFull"
      }}
      $legacyConfinedCandidate = [System.IO.Path]::GetFullPath([string]$legacyConfinedCandidates[0])
      if (!$legacyConfinedCandidate.StartsWith($backupRootPrefix, [System.StringComparison]::OrdinalIgnoreCase)) {{
        throw "rollback runtime path escapes backup root: path=$legacyConfinedCandidate root=$backupRootFull"
      }}
      Assert-NodeLocalPhysicalPath -Path $legacyConfinedCandidate -Label 'rollback runtime source' | Out-Null
      if ($expectedRuntimeSha256 -notmatch '^[0-9a-fA-F]{{64}}$') {{
        throw "confined backup runtime sha256 mismatch for legacy rooted runtime_path: declared runtime_sha256 is missing or invalid"
      }}
      $legacyCandidateSha256 = (Get-FileHash -LiteralPath $legacyConfinedCandidate -Algorithm SHA256).Hash.ToLowerInvariant()
      if ($legacyCandidateSha256 -ne $expectedRuntimeSha256.ToLowerInvariant()) {{
        throw "confined backup runtime sha256 mismatch for legacy rooted runtime_path: candidate=$legacyConfinedCandidate expected=$expectedRuntimeSha256 actual=$legacyCandidateSha256"
      }}
      return $legacyConfinedCandidate
    }}
    if (!$manifestCandidate.StartsWith($backupRootPrefix, [System.StringComparison]::OrdinalIgnoreCase)) {{
      throw "rollback runtime path escapes backup root: path=$manifestCandidate root=$backupRootFull"
    }}
    if (!(Test-Path -LiteralPath $manifestCandidate -PathType Leaf)) {{
      throw "known-good rollback runtime missing: $manifestCandidate"
    }}
    Assert-NodeLocalPhysicalPath -Path $manifestCandidate -Label 'rollback runtime source' | Out-Null
    if (![string]::IsNullOrWhiteSpace($expectedRuntimeSha256)) {{
      if ($expectedRuntimeSha256 -notmatch '^[0-9a-fA-F]{{64}}$') {{
        throw "invalid rollback runtime sha256 in backup manifest/provenance: $expectedRuntimeSha256"
      }}
      $actualRuntimeSha256 = (Get-FileHash -LiteralPath $manifestCandidate -Algorithm SHA256).Hash.ToLowerInvariant()
      if ($actualRuntimeSha256 -ne $expectedRuntimeSha256.ToLowerInvariant()) {{
        throw "rollback runtime sha256 mismatch: path=$manifestCandidate expected=$expectedRuntimeSha256 actual=$actualRuntimeSha256"
      }}
    }}
    return $manifestCandidate
  }}

  $fallbackCandidates = @(
    @($runtimeCandidate, $binCandidate) |
      Where-Object {{ Test-Path -LiteralPath $_ -PathType Leaf }}
  )
  if ($fallbackCandidates.Count -gt 1) {{
    throw "rollback runtime ambiguous: both supported fallback candidates exist under $backupRootFull"
  }}
  if ($fallbackCandidates.Count -eq 0) {{
    throw "known-good rollback runtime missing: expected runtime\\oasis7_chain_runtime.exe or bin\\oasis7_chain_runtime.exe under $backupRootFull"
  }}
  $resolvedFallback = [System.IO.Path]::GetFullPath([string]$fallbackCandidates[0])
  if (!$resolvedFallback.StartsWith($backupRootPrefix, [System.StringComparison]::OrdinalIgnoreCase)) {{
    throw "rollback runtime path escapes backup root: path=$resolvedFallback root=$backupRootFull"
  }}
  Assert-NodeLocalPhysicalPath -Path $resolvedFallback -Label 'rollback runtime source' | Out-Null
  return $resolvedFallback
}}

$configRoot = $activeConfigRoot
$configRootPrefix = $configRoot + '\\'
$rollbackProvenanceTargets = @(
  (Join-Path $deployRoot 'CURRENT_VERSION'),
  (Join-Path $deployRoot 'DEPLOYED_BUILDINFO')
)
try {{
  Assert-NodeLocalPhysicalPath -Path $rollbackBackupRoot -Label 'rollback root' | Out-Null
  $rollbackRuntimeSource = Resolve-RollbackRuntimeSource -BackupRoot $rollbackBackupRoot
  foreach ($rollbackConfigTarget in $rollbackConfigTargets) {{
    if (!$rollbackConfigTarget.StartsWith($configRootPrefix, [System.StringComparison]::OrdinalIgnoreCase)) {{
      throw "rollback config target is outside node config root: $rollbackConfigTarget"
    }}
    $rollbackRelativePath = $rollbackConfigTarget.Substring($configRootPrefix.Length)
    $rollbackConfigSource = Join-Path (Join-Path $rollbackBackupRoot 'config') $rollbackRelativePath
    Assert-NodeLocalPhysicalPath -Path $rollbackConfigTarget -Label 'active config rollback target' | Out-Null
    Assert-NodeLocalPhysicalPath -Path $rollbackConfigSource -Label 'rollback config source' | Out-Null
    if (!(Test-Path -LiteralPath $rollbackConfigSource -PathType Leaf)) {{
      throw "known-good rollback config missing: $rollbackConfigSource"
    }}
  }}
  foreach ($rollbackProvenanceTarget in $rollbackProvenanceTargets) {{
    $rollbackProvenanceSource = Join-Path $rollbackBackupRoot ([System.IO.Path]::GetFileName($rollbackProvenanceTarget))
    Assert-NodeLocalPhysicalPath -Path $rollbackProvenanceTarget -Label 'active deploy rollback target' | Out-Null
    Assert-NodeLocalPhysicalPath -Path $rollbackProvenanceSource -Label 'rollback provenance source' | Out-Null
    if (!(Test-Path -LiteralPath $rollbackProvenanceSource -PathType Leaf)) {{
      throw "known-good rollback provenance missing: $rollbackProvenanceSource"
    }}
  }}
}} catch {{
  $rollbackPreflightDiagnostic = "failure_phase=rollback_preflight rollback_path=$rollbackBackupRoot rollback_error=$($_.Exception.Message) rollback_exit_code=1 rollback_required=true"
  [System.IO.File]::AppendAllText($attemptStderrPath, $rollbackPreflightDiagnostic + [Environment]::NewLine)
  [System.IO.File]::AppendAllText($attemptExitMarkerPath, '1' + [Environment]::NewLine)
  Write-Output 'rollback_required=true'
  throw
}}

Write-Output 'rollback_closure_complete=true'
Write-Output 'staged_sha_closure_complete=true'
Write-Output 'promotion_begin=true'

function Copy-StagedFileAtomically {{
  param([Parameter(Mandatory = $true)] [string] $Source, [Parameter(Mandatory = $true)] [string] $Destination)
  Assert-NodeLocalPhysicalPath -Path $Source -Label 'staging promotion source' | Out-Null
  Assert-NodeLocalPhysicalPath -Path $Destination -Label 'active promotion destination' | Out-Null
  $destinationParent = [System.IO.Path]::GetDirectoryName($Destination)
  Assert-NodeLocalPhysicalPath -Path $destinationParent -Label 'active promotion destination parent' | Out-Null
  New-Item -ItemType Directory -Path $destinationParent -Force | Out-Null
  $destinationTemp = "$Destination.rollout-$attemptId.tmp"
  Copy-Item -LiteralPath $Source -Destination $destinationTemp -Force
  Move-Item -LiteralPath $destinationTemp -Destination $Destination -Force
}}

function Move-NodeLocalFileAtomically {{
  param([Parameter(Mandatory = $true)] [string] $Source, [Parameter(Mandatory = $true)] [string] $Destination)
  Assert-NodeLocalPhysicalPath -Path $Source -Label 'active promotion temporary source' | Out-Null
  Assert-NodeLocalPhysicalPath -Path $Destination -Label 'active promotion destination' | Out-Null
  Move-Item -LiteralPath $Source -Destination $Destination -Force
}}

function Invoke-KnownGoodRollback {{
  param([object] $OriginalTaskAction, [string] $FailurePhase, [string] $FailureError, [int] $FailureExitCode = 1)
  if ($script:rollbackInvoked) {{
    Write-Output "rollback_already_invoked=true failure_phase=$FailurePhase"
    return
  }}
  $script:rollbackInvoked = $true
  Write-Output 'rollback_begin=true'
  $rollbackDiagnostic = "failure_phase=$FailurePhase rollback_path=$rollbackBackupRoot rollback_error=$FailureError rollback_exit_code=$FailureExitCode rollback_required=true"
  [System.IO.File]::AppendAllText($attemptStderrPath, $rollbackDiagnostic + [Environment]::NewLine)
  [System.IO.File]::AppendAllText($attemptExitMarkerPath, [string]$FailureExitCode + [Environment]::NewLine)
  Preserve-AttemptDiagnostics $attemptStdoutPath $attemptStderrPath $attemptExitMarkerPath

  $rollbackRuntimeSourceAtRestore = Resolve-RollbackRuntimeSource -BackupRoot $rollbackBackupRoot
  if (!$rollbackRuntimeSourceAtRestore.Equals($rollbackRuntimeSource, [System.StringComparison]::OrdinalIgnoreCase)) {{
    throw "rollback runtime source changed after preflight: preflight=$rollbackRuntimeSource restore=$rollbackRuntimeSourceAtRestore"
  }}
  Assert-NodeLocalPhysicalPath -Path $rollbackRuntimeSourceAtRestore -Label 'rollback runtime source' | Out-Null
  Assert-NodeLocalPhysicalPath -Path $runtime -Label 'runtime rollback target' | Out-Null
  $rollbackRuntimeSourceSha256 = (Get-FileHash -LiteralPath $rollbackRuntimeSourceAtRestore -Algorithm SHA256).Hash.ToLowerInvariant()
  $installedRuntimeSha256 = if (Test-Path -LiteralPath $runtime -PathType Leaf) {{
    (Get-FileHash -LiteralPath $runtime -Algorithm SHA256).Hash.ToLowerInvariant()
  }} else {{
    'missing'
  }}
  $rollbackRuntimeRestoreRequired = $installedRuntimeSha256 -ne $rollbackRuntimeSourceSha256
  Write-Output "rollback_runtime_restore_required=$($rollbackRuntimeRestoreRequired.ToString().ToLowerInvariant()) installed_sha256=$installedRuntimeSha256 backup_sha256=$rollbackRuntimeSourceSha256"

  Stop-ScheduledTask -TaskName $taskName -ErrorAction SilentlyContinue
  Get-Process oasis7_chain_runtime -ErrorAction SilentlyContinue |
    Stop-Process -Force -ErrorAction SilentlyContinue
  $rollbackProcessExitDeadline = (Get-Date).AddSeconds($rollbackUnlockTimeoutSeconds)
  while ((Get-Date) -lt $rollbackProcessExitDeadline -and
         $null -ne (Get-Process oasis7_chain_runtime -ErrorAction SilentlyContinue | Select-Object -First 1)) {{
    Start-Sleep -Milliseconds 250
  }}
  if ($null -ne (Get-Process oasis7_chain_runtime -ErrorAction SilentlyContinue | Select-Object -First 1)) {{
    throw "rollback_required=true process_remains=true runtime=$runtime"
  }}

  if ($rollbackRuntimeRestoreRequired) {{
    $rollbackFileUnlockDeadline = (Get-Date).AddSeconds($rollbackUnlockTimeoutSeconds)
    $runtimeUnlocked = $false
    while ((Get-Date) -lt $rollbackFileUnlockDeadline) {{
      if (!(Test-Path -LiteralPath $runtime -PathType Leaf)) {{
        $runtimeUnlocked = $true
        break
      }}
      try {{
        $lockProbe = [System.IO.File]::Open(
          $runtime,
          [System.IO.FileMode]::Open,
          [System.IO.FileAccess]::ReadWrite,
          [System.IO.FileShare]::None
        )
        $lockProbe.Dispose()
        $runtimeUnlocked = $true
        break
      }} catch {{
        Start-Sleep -Milliseconds 250
      }}
    }}
    Assert-NodeLocalPhysicalPath -Path $runtime -Label 'runtime rollback target' | Out-Null
    if (!$runtimeUnlocked) {{
      $lockDiagnostic = "failure_phase=rollback_unlock rollback_path=$runtime rollback_error=exclusive_file_unlock_timeout rollback_exit_code=1 rollback_required=true lock_remains=true"
      [System.IO.File]::AppendAllText($attemptStderrPath, $lockDiagnostic + [Environment]::NewLine)
      throw "rollback_required=true lock_remains=true runtime=$runtime"
    }}
    New-Item -ItemType Directory -Path ([System.IO.Path]::GetDirectoryName($runtime)) -Force | Out-Null
    Copy-Item -LiteralPath $rollbackRuntimeSourceAtRestore -Destination $runtime -Force
    Write-Output "rollback_component_restored=runtime path=$runtime"
  }} else {{
    Write-Output "rollback_component_unchanged=runtime path=$runtime"
  }}
  foreach ($rollbackConfigTarget in $rollbackConfigTargets) {{
    $rollbackRelativePath = $rollbackConfigTarget.Substring($configRootPrefix.Length)
    $rollbackConfigSource = Join-Path (Join-Path $rollbackBackupRoot 'config') $rollbackRelativePath
    Assert-NodeLocalPhysicalPath -Path $rollbackConfigSource -Label 'rollback config source' | Out-Null
    Assert-NodeLocalPhysicalPath -Path $rollbackConfigTarget -Label 'active config rollback target' | Out-Null
    $rollbackConfigSourceHash = (Get-FileHash -LiteralPath $rollbackConfigSource -Algorithm SHA256).Hash.ToLowerInvariant()
    $rollbackConfigTargetHash = if (Test-Path -LiteralPath $rollbackConfigTarget -PathType Leaf) {{
      (Get-FileHash -LiteralPath $rollbackConfigTarget -Algorithm SHA256).Hash.ToLowerInvariant()
    }} else {{
      'missing'
    }}
    if ($rollbackConfigTargetHash -ne $rollbackConfigSourceHash) {{
      Copy-Item -LiteralPath $rollbackConfigSource -Destination $rollbackConfigTarget -Force
      Write-Output "rollback_component_restored=config path=$rollbackConfigTarget"
    }} else {{
      Write-Output "rollback_component_unchanged=config path=$rollbackConfigTarget"
    }}
  }}
  foreach ($rollbackProvenanceTarget in $rollbackProvenanceTargets) {{
    $rollbackProvenanceSource = Join-Path $rollbackBackupRoot ([System.IO.Path]::GetFileName($rollbackProvenanceTarget))
    Assert-NodeLocalPhysicalPath -Path $rollbackProvenanceSource -Label 'rollback provenance source' | Out-Null
    Assert-NodeLocalPhysicalPath -Path $rollbackProvenanceTarget -Label 'active deploy rollback target' | Out-Null
    $rollbackProvenanceSourceHash = (Get-FileHash -LiteralPath $rollbackProvenanceSource -Algorithm SHA256).Hash.ToLowerInvariant()
    $rollbackProvenanceTargetHash = if (Test-Path -LiteralPath $rollbackProvenanceTarget -PathType Leaf) {{
      (Get-FileHash -LiteralPath $rollbackProvenanceTarget -Algorithm SHA256).Hash.ToLowerInvariant()
    }} else {{
      'missing'
    }}
    if ($rollbackProvenanceTargetHash -ne $rollbackProvenanceSourceHash) {{
      Copy-Item -LiteralPath $rollbackProvenanceSource -Destination $rollbackProvenanceTarget -Force
      Write-Output "rollback_component_restored=provenance path=$rollbackProvenanceTarget"
    }} else {{
      Write-Output "rollback_component_unchanged=provenance path=$rollbackProvenanceTarget"
    }}
  }}
  Set-ScheduledTask -TaskName $taskName -Action $OriginalTaskAction -ErrorAction Stop | Out-Null
  Write-Output 'rollback_applied=true restart_required=true'
}}

function Invoke-RolloutFailureInjection {{
  param([Parameter(Mandatory = $true)] [string] $Phase)
  if ($env:OASIS7_ROLLOUT_INJECT_FAILURE_PHASE -eq $Phase) {{
    throw "injected rollout failure: phase=$Phase"
  }}
}}

$script:rollbackInvoked = $false

$oldHash = if (Test-Path $runtime) {{
  (Get-FileHash $runtime -Algorithm SHA256).Hash.ToLowerInvariant()
}} else {{
  'missing'
}}
Write-Output "old_runtime_sha256=$oldHash"

$scheduledTask = Get-ScheduledTask -TaskName $taskName -ErrorAction Stop
$originalTaskAction = @($scheduledTask.Actions)[0]
if ($null -eq $originalTaskAction) {{ throw "scheduled task has no action: $taskName" }}
$childExecute = [string]$originalTaskAction.Execute
$childArguments = [string]$originalTaskAction.Arguments
$wrapperTemplate = @'
$attemptStdoutPath = '__ATTEMPT_STDOUT__'
$attemptStderrPath = '__ATTEMPT_STDERR__'
$attemptExitMarkerPath = '__ATTEMPT_EXIT__'
$childExecute = '__CHILD_EXECUTE__'
$childArguments = '__CHILD_ARGUMENTS__'
[System.IO.File]::AppendAllText($attemptStdoutPath, "attempt_started=" + [DateTime]::UtcNow.ToString('o') + [Environment]::NewLine)
[System.IO.File]::AppendAllText($attemptStderrPath, "attempt_id=__ATTEMPT_ID__" + [Environment]::NewLine)
$processStartInfo = [System.Diagnostics.ProcessStartInfo]::new()
$processStartInfo.FileName = $childExecute
$processStartInfo.Arguments = $childArguments
$processStartInfo.UseShellExecute = $false
$processStartInfo.CreateNoWindow = $true
$processStartInfo.RedirectStandardOutput = $true
$processStartInfo.RedirectStandardError = $true
$childProcess = [System.Diagnostics.Process]::new()
$childProcess.StartInfo = $processStartInfo
$stdoutHandler = [System.Diagnostics.DataReceivedEventHandler] {{
  param($sender, $eventArgs)
  if ($null -ne $eventArgs.Data) {{
    [System.IO.File]::AppendAllText($attemptStdoutPath, $eventArgs.Data + [Environment]::NewLine)
  }}
}}
$stderrHandler = [System.Diagnostics.DataReceivedEventHandler] {{
  param($sender, $eventArgs)
  if ($null -ne $eventArgs.Data) {{
    [System.IO.File]::AppendAllText($attemptStderrPath, $eventArgs.Data + [Environment]::NewLine)
  }}
}}
$childProcess.add_OutputDataReceived($stdoutHandler)
$childProcess.add_ErrorDataReceived($stderrHandler)
try {{
  if (!$childProcess.Start()) {{ throw "failed to start scheduled-task child: $childExecute" }}
  $childProcess.BeginOutputReadLine()
  $childProcess.BeginErrorReadLine()
  $childProcess.WaitForExit()
  $childExitCode = $childProcess.ExitCode
}} finally {{
  $childProcess.remove_OutputDataReceived($stdoutHandler)
  $childProcess.remove_ErrorDataReceived($stderrHandler)
  $childProcess.Dispose()
}}
[System.IO.File]::AppendAllText($attemptExitMarkerPath, [string]$childExitCode + [Environment]::NewLine)
exit $childExitCode
'@
$wrapperText = $wrapperTemplate.Replace('__ATTEMPT_STDOUT__', $attemptStdoutPath.Replace("'", "''"))
$wrapperText = $wrapperText.Replace('__ATTEMPT_STDERR__', $attemptStderrPath.Replace("'", "''"))
$wrapperText = $wrapperText.Replace('__ATTEMPT_EXIT__', $attemptExitMarkerPath.Replace("'", "''"))
$wrapperText = $wrapperText.Replace('__ATTEMPT_ID__', $attemptId)
$wrapperText = $wrapperText.Replace('__CHILD_EXECUTE__', $childExecute.Replace("'", "''"))
$wrapperText = $wrapperText.Replace('__CHILD_ARGUMENTS__', $childArguments.Replace("'", "''"))
[System.IO.File]::WriteAllText($attemptWrapperPath, $wrapperText, [System.Text.UTF8Encoding]::new($false))
$attemptTaskAction = New-ScheduledTaskAction -Execute 'powershell.exe' -Argument "-NoProfile -ExecutionPolicy Bypass -File `"$attemptWrapperPath`""
Preserve-AttemptDiagnostics $attemptStdoutPath $attemptStderrPath $attemptExitMarkerPath
Set-ScheduledTask -TaskName $taskName -Action $attemptTaskAction -ErrorAction Stop | Out-Null

Stop-ScheduledTask -TaskName $taskName -ErrorAction SilentlyContinue
Get-Process oasis7_chain_runtime -ErrorAction SilentlyContinue |
  Stop-Process -Force -ErrorAction SilentlyContinue
Start-Sleep -Seconds 3

try {{
Invoke-RolloutFailureInjection -Phase 'installer'
$install = Start-Process -FilePath $installer -ArgumentList '/S' -Wait -PassThru
Write-Output "installer_exit_code=$($install.ExitCode)"
if ($install.ExitCode -ne 0) {{
  Invoke-KnownGoodRollback -OriginalTaskAction $originalTaskAction -FailurePhase 'installer' -FailureError "installer exit code $($install.ExitCode)" -FailureExitCode $install.ExitCode
  throw "installer failed with exit code $($install.ExitCode)"
}}
if (!(Test-Path $runtime)) {{
  Invoke-KnownGoodRollback -OriginalTaskAction $originalTaskAction -FailurePhase 'installer' -FailureError "runtime missing after install: $runtime" -FailureExitCode 1
  throw "runtime missing after install: $runtime"
}}
Assert-NodeLocalPhysicalPath -Path $runtime -Label 'runtime promotion target' | Out-Null

$hash = (Get-FileHash $runtime -Algorithm SHA256).Hash.ToLowerInvariant()
$size = (Get-Item $runtime).Length
Write-Output "new_runtime_sha256=$hash"
Write-Output "new_runtime_size=$size"

foreach ($stagedConfigSource in @($stagedToActiveConfigTargets.Keys)) {{
  $activeConfigTarget = $stagedToActiveConfigTargets[$stagedConfigSource]
  if ($transformedConfigTargetSet.Contains($activeConfigTarget)) {{ continue }}
  Invoke-RolloutFailureInjection -Phase 'governed_copy'
  Copy-StagedFileAtomically -Source $stagedConfigSource -Destination $activeConfigTarget
}}

Set-JsonProperty $json.runtime_build 'git_commit' $commit
Set-JsonProperty $json.runtime_build 'kind' 'file'
Set-JsonProperty $json.runtime_build 'path' $runtime
Set-JsonProperty $json.runtime_build 'resolved_path' $runtime
Set-JsonProperty $json.runtime_build 'ref' $artifactRef
Set-JsonProperty $json.runtime_build 'sha256' $hash
Set-JsonProperty $json.runtime_build 'size_bytes' $size
Set-JsonProperty $json.runtime_build 'updated_by' "windows package upgrade $version (run $runId, commit $commit)"
Set-JsonProperty $json 'git_commit' $commit
Set-JsonProperty $json 'updated_by' "windows package upgrade $version (run $runId, commit $commit)"
$jsonText = $json | ConvertTo-Json -Depth 100
$bundleTemp = "$activeBundlePath.rollout-$attemptId.tmp"
[System.IO.File]::WriteAllText(
  $bundleTemp,
  $jsonText + [Environment]::NewLine,
  [System.Text.UTF8Encoding]::new($false)
)
Invoke-RolloutFailureInjection -Phase 'bundle_move'
Move-NodeLocalFileAtomically -Source $bundleTemp -Destination $activeBundlePath
$updated = 1

$genesisTemp = "$activeGenesisPath.rollout-$attemptId.tmp"
[System.IO.File]::WriteAllText(
  $genesisTemp,
  $genesisText + [Environment]::NewLine,
  [System.Text.UTF8Encoding]::new($false)
)
Invoke-RolloutFailureInjection -Phase 'genesis_move'
Move-NodeLocalFileAtomically -Source $genesisTemp -Destination $activeGenesisPath
$manifestTemp = "$activeManifestPath.rollout-$attemptId.tmp"
[System.IO.File]::WriteAllText(
  $manifestTemp,
  $manifestText + [Environment]::NewLine,
  [System.Text.UTF8Encoding]::new($false)
)
Invoke-RolloutFailureInjection -Phase 'manifest_move'
Move-NodeLocalFileAtomically -Source $manifestTemp -Destination $activeManifestPath
foreach ($key in $currentGovernedRefKeys) {{
  $property = $genesisJson.governance_bootstrap_refs.PSObject.Properties[$key]
  if ($null -ne $property -and $property.Value -is [string] -and ![string]::IsNullOrEmpty($property.Value)) {{
    if (!(Test-Path -LiteralPath $property.Value -PathType Leaf)) {{
      throw "localized current-schema governance ref target missing for $key`: $($property.Value)"
    }}
  }}
}}
Write-Output "localized_governed_ref_count=$localizedGovernedRefCount"

Invoke-RolloutFailureInjection -Phase 'current_version_write'
$currentVersionPath = Join-Path $deployRoot 'CURRENT_VERSION'
Assert-NodeLocalPhysicalPath -Path $currentVersionPath -Label 'active deploy current version target' | Out-Null
Set-Content -Encoding UTF8 $currentVersionPath $version
$deployedBuildInfo = @(
  'workflow=Testnet Packages',
  "run_id=$runId",
  'repository=eng-cc/oasis7',
  "commit=$commit",
  "package_version=$version",
  'platform=windows-x64',
  "runtime_sha256=$hash",
  "runtime_size=$size"
)
Invoke-RolloutFailureInjection -Phase 'deployed_buildinfo_write'
$deployedBuildInfoPath = Join-Path $deployRoot 'DEPLOYED_BUILDINFO'
Assert-NodeLocalPhysicalPath -Path $deployedBuildInfoPath -Label 'active deploy buildinfo target' | Out-Null
$deployedBuildInfo | Set-Content -Encoding UTF8 $deployedBuildInfoPath
Write-Output "updated_bundle_count=$updated"
Write-Output 'promotion_complete=true'

$taskStartTime = Get-Date
Start-ScheduledTask -TaskName $taskName
$verificationDeadline = (Get-Date).AddSeconds($verificationTimeoutSeconds)
$verified = $false
$processRunning = $false
$statusRunning = $false
$statusReady = $false
$lastStatusError = 'not_attempted'
$lastStatusPayload = 'none'
while ((Get-Date) -lt $verificationDeadline) {{
  $processRunning = $null -ne (Get-Process oasis7_chain_runtime -ErrorAction SilentlyContinue | Select-Object -First 1)
  $taskInfo = Get-ScheduledTaskInfo -TaskName $taskName -ErrorAction SilentlyContinue
  $lastTaskResult = if ($null -ne $taskInfo) {{ $taskInfo.LastTaskResult }} else {{ $null }}
  $schedulerRunningResult = 267009
  $schedulerReportsRunning = $lastTaskResult -eq $schedulerRunningResult
  $attemptChildExitCode = $null
  if (Test-Path -LiteralPath $attemptExitMarkerPath -PathType Leaf) {{
    $markerValue = Get-Content -LiteralPath $attemptExitMarkerPath -Tail 1 -ErrorAction SilentlyContinue
    $parsedExitCode = 0
    if ([int]::TryParse([string]$markerValue, [ref]$parsedExitCode)) {{ $attemptChildExitCode = $parsedExitCode }}
  }}
  $terminalExitCode = if ($null -ne $attemptChildExitCode) {{ $attemptChildExitCode }} else {{ $null }}
  if ($null -ne $attemptChildExitCode -and
      $null -ne $terminalExitCode -and $terminalExitCode -ne 0) {{
    Preserve-AttemptDiagnostics $attemptStdoutPath $attemptStderrPath $attemptExitMarkerPath
    $diagnosticLines = @(
      "task=$taskName",
      "attempt_id=$attemptId",
      "attempt_stdout=$attemptStdoutPath",
      "attempt_stderr=$attemptStderrPath",
      "attempt_exit_marker=$attemptExitMarkerPath",
      "last_task_result=$lastTaskResult",
      "scheduler_reports_running=$schedulerReportsRunning",
      "child_exit_code=$attemptChildExitCode"
    )
    foreach ($diagnosticPath in @($attemptStdoutPath, $attemptStderrPath, $attemptExitMarkerPath)) {{
      if (Test-Path -LiteralPath $diagnosticPath -PathType Leaf) {{
        $diagnosticLines += @(Get-Content -LiteralPath $diagnosticPath -Tail 40 -ErrorAction SilentlyContinue)
      }}
    }}
    $failureDiagnostics = ($diagnosticLines -join ' | ')
    Invoke-KnownGoodRollback -OriginalTaskAction $originalTaskAction -FailurePhase 'startup' -FailureError $failureDiagnostics -FailureExitCode $terminalExitCode
    Write-Output 'rollback_required=true'
    Write-Output "failure_diagnostics=$failureDiagnostics"
    throw "terminal task child exited nonzero; rollback_required=true task=$taskName result=$terminalExitCode"
  }}
  $statusRunning = $false
  $statusReady = $false
  if (![string]::IsNullOrEmpty($statusUrl)) {{
    try {{
      $status = Invoke-RestMethod -Uri $statusUrl -TimeoutSec 8
      $lastStatusPayload = $status | ConvertTo-Json -Compress -Depth 8
      $lastStatusError = 'none'
      $statusRunning = $status.running -eq $true
      if ($status.PSObject.Properties.Name -contains 'ready') {{
        $statusReady = $status.ready -eq $true
      }} elseif ($status.PSObject.Properties.Name -contains 'readiness') {{
        if ($status.readiness -is [bool]) {{
          $statusReady = $status.readiness -eq $true
        }} elseif ($null -ne $status.readiness -and $status.readiness.PSObject.Properties.Name -contains 'ready') {{
          $statusReady = $status.readiness.ready -eq $true
        }}
      }}
    }} catch {{
      $lastStatusError = $_.Exception.Message
    }}
  }}
  if ($processRunning -and $statusRunning) {{
    if (!$requireStrictReady -or $statusReady) {{
      $verified = $true
      break
    }}
  }} elseif ($processRunning -and !$requireRpcRunning) {{
    $verified = $true
    break
  }}
  Start-Sleep -Seconds 2
}}
if (!$verified) {{
  Preserve-AttemptDiagnostics $attemptStdoutPath $attemptStderrPath $attemptExitMarkerPath
  Invoke-KnownGoodRollback -OriginalTaskAction $originalTaskAction -FailurePhase 'startup_timeout' -FailureError $lastStatusError -FailureExitCode 1
  throw "Windows observer startup verification timed out; rollback_required=true after=$verificationTimeoutSeconds seconds process_running=$processRunning status_running=$statusRunning status_ready=$statusReady rpc_error=$lastStatusError payload=$lastStatusPayload"
}}
Invoke-RolloutFailureInjection -Phase 'task_action_restore'
Set-ScheduledTask -TaskName $taskName -Action $originalTaskAction -ErrorAction Stop | Out-Null
Write-Output "startup_verified=true process_running=$processRunning status_running=$statusRunning status_ready=$statusReady"
Get-Process oasis7_chain_runtime -ErrorAction Stop |
  Select-Object -First 1 Id,Path |
  ConvertTo-Json -Compress
Get-ScheduledTask -TaskName $taskName |
  Select-Object TaskName,State |
  ConvertTo-Json -Compress
Write-Output $lastStatusPayload
}} catch {{
  $postStopFailure = $_
  if (!$script:rollbackInvoked) {{
    Invoke-KnownGoodRollback -OriginalTaskAction $originalTaskAction -FailurePhase 'post_stop_mutation' -FailureError $postStopFailure.Exception.Message -FailureExitCode 1
  }}
  throw $postStopFailure
}}
"""


def write_windows_plan(
    out_dir: Path,
    node: dict[str, Any],
    windows_asset: Path,
    version: str,
    commit: str,
    run_id: str,
    governed_files: list[tuple[Path, str]],
    readiness_policy: str,
    sequencer_status_url: str,
    storage_status_url: str,
    closure_receipt: dict[str, Any],
) -> tuple[Path, list[str]]:
    name = str(node.get("name") or "windows-node")
    safe_name = "".join(ch if ch.isalnum() or ch in "._-" else "-" for ch in name)
    script_path = out_dir / f"{safe_name}-windows-upgrade.ps1"
    host = str(node.get("host") or "")
    user = str(node.get("user") or "Administrator")
    remote_script = str(node.get("remote_script") or f"{safe_name}-windows-upgrade.ps1")
    remote_installer = str(node.get("remote_installer") or windows_asset.name)
    script_node = dict(node)
    if host:
        rollback_backup_root = node.get("rollback_backup_root")
        if not isinstance(rollback_backup_root, str) or not rollback_backup_root.strip():
            die(
                f"remote windows node {name} must declare a non-empty "
                "rollback_backup_root"
            )
        script_node["rollback_backup_root"] = rollback_backup_root.strip()
    deploy_root = str(node.get("deploy_root") or r"C:\oasis7-deploy").replace("\\", "/").rstrip("/")
    attempt_seed = "\0".join((safe_name, version, commit, run_id))
    attempt_id = hashlib.sha256(attempt_seed.encode("utf-8")).hexdigest()[:24]
    staging_root = f"{deploy_root}/staging/package-rollout/{attempt_id}"
    remote_installer_name = Path(remote_installer.replace("\\", "/")).name
    remote_script_name = Path(remote_script.replace("\\", "/")).name
    staged_installer = f"{staging_root}/{remote_installer_name}"
    staged_script = f"{staging_root}/{remote_script_name}"
    active_bundle_path = str(node.get("governed_bundle_path") or WINDOWS_GOVERNED_BUNDLE)
    active_genesis_path = str(node.get("governed_genesis_path") or WINDOWS_GOVERNED_GENESIS)
    active_manifest_path = str(node.get("governed_manifest_path") or WINDOWS_GOVERNED_MANIFEST)
    active_bootstrap_path = str(node.get("governed_bootstrap_path") or WINDOWS_GOVERNED_BOOTSTRAP)
    active_bundle_name = Path(active_bundle_path.replace("\\", "/")).name
    active_genesis_name = Path(active_genesis_path.replace("\\", "/")).name
    active_manifest_name = Path(active_manifest_path.replace("\\", "/")).name
    active_bootstrap_name = Path(active_bootstrap_path.replace("\\", "/")).name
    script_node.update(
        {
            "staging_root": staging_root,
            "installer_path": staged_installer,
            "configured_installer_path": remote_installer,
            "governed_bundle_path": f"{staging_root}/config/{active_bundle_name}",
            "governed_genesis_path": f"{staging_root}/config/{active_genesis_name}",
            "governed_manifest_path": f"{staging_root}/config/{active_manifest_name}",
            "governed_bootstrap_path": f"{staging_root}/config/{active_bootstrap_name}",
            "active_governed_bundle_path": active_bundle_path,
            "active_governed_genesis_path": active_genesis_path,
            "active_governed_manifest_path": active_manifest_path,
            "active_governed_bootstrap_path": active_bootstrap_path,
        }
    )
    remote_governed = {
        f"{staging_root}/config/{remote_relative}": sha256_file(source)
        for source, remote_relative in governed_files
    }
    script_text = windows_script(
        script_node,
        windows_asset.name,
        version,
        commit,
        run_id,
        remote_governed,
        readiness_policy,
        sequencer_status_url,
        storage_status_url,
        closure_receipt,
    )
    script_path.write_text(script_text, encoding="utf-8")
    # Rewrite without BOM explicitly; Windows PowerShell accepts this and the runtime JSON writer also uses no-BOM.
    script_path.write_bytes(script_text.encode("utf-8"))
    commands: list[str] = []
    if host:
        remote_target = f"{user}@{host}"

        def ps_literal(value: str) -> str:
            return "'" + value.replace("'", "''") + "'"

        def encoded_powershell_argv(statement: str) -> list[str]:
            encoded = base64.b64encode(statement.encode("utf-16le")).decode("ascii")
            return [
                "powershell.exe",
                "-NoProfile",
                "-NonInteractive",
                "-EncodedCommand",
                encoded,
            ]

        def remote_staging_command(statement: str, plan_annotation: str) -> str:
            encoded_command = shell_join(
                ["command", "ssh", remote_target, *encoded_powershell_argv(statement)]
            )
            return f": {shlex.quote(plan_annotation)} && {encoded_command}"

        def append_verified_transfer(source: Path, remote_path: str) -> None:
            normalized_remote = remote_path.replace("\\", "/")
            remote_parent = str(Path(normalized_remote).parent).replace("\\", "/")
            expected_sha256 = sha256_file(source)
            commands.append(
                remote_staging_command(
                    f"$parent={ps_literal(remote_parent)}; "
                    "$null = New-Item -ItemType Directory -Path $parent -Force; "
                    f"Write-Output ('staging_parent_ready=' + $parent + ' path=' + {ps_literal(normalized_remote)} + "
                    f"' transfer_target={remote_target}:{normalized_remote}')",
                    (
                        f"staging_parent_ready={remote_parent} "
                        f"path={normalized_remote} transfer_target={remote_target}:{normalized_remote} "
                        "operation=New-Item -ItemType Directory"
                    ),
                )
            )
            commands.append(shell_join(["scp", str(source), f"{remote_target}:{normalized_remote}"]))
            commands.append(
                remote_staging_command(
                    f"$path={ps_literal(normalized_remote)}; $expected={ps_literal(expected_sha256)}; "
                    "$actual=(Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLowerInvariant(); "
                    "if ($actual -ne $expected) { throw ('staging transfer checksum mismatch: path=' + $path + "
                    "' expected=' + $expected + ' actual=' + $actual) }; "
                    "Write-Output ('staging_transfer_ack=' + $path + ' sha256=' + $actual)",
                    (
                        f"staging_transfer_ack={normalized_remote} sha256={expected_sha256} "
                        "operation=Get-FileHash SHA256 throw_on_mismatch=true"
                    ),
                )
            )

        append_verified_transfer(windows_asset, staged_installer)
        for source, remote_relative in governed_files:
            remote_path = f"{staging_root}/config/{remote_relative}"
            append_verified_transfer(source, remote_path)
        append_verified_transfer(script_path, staged_script)
        apply_statement = (
            "$rolloutEvidenceMarker='task-2269-windows-upgrade.ps1'; "
            "& powershell.exe -NoProfile -NonInteractive -ExecutionPolicy Bypass -File "
            f"{ps_literal(staged_script)} ; exit $LASTEXITCODE"
        )
        commands.append(shell_join(["ssh", remote_target, *encoded_powershell_argv(apply_statement)]))
    else:
        commands.append(
            shell_join(
                [
                    "powershell",
                    "-NoProfile",
                    "-ExecutionPolicy",
                    "Bypass",
                    "-File",
                    str(script_path),
                ]
            )
        )
    return script_path, commands


def macos_absolute_path(node: dict[str, Any], field: str) -> str:
    value = node.get(field)
    if (
        not isinstance(value, str)
        or not value.startswith("/")
        or ".." in PurePosixPath(value).parts
    ):
        die(f"macOS node {node.get('name', '<unnamed>')} must declare absolute {field}")
    return str(PurePosixPath(value))


def macos_string_paths(node: dict[str, Any], field: str) -> list[str]:
    values = node.get(field)
    if not isinstance(values, list) or not values or not all(
        isinstance(value, str)
        and value.startswith("/")
        and ".." not in PurePosixPath(value).parts
        for value in values
    ):
        die(f"macOS node {node.get('name', '<unnamed>')} must declare non-empty absolute {field}")
    return [str(PurePosixPath(value)) for value in values]


def macos_paths_overlap(first: str, second: str) -> bool:
    first_path = PurePosixPath(first)
    second_path = PurePosixPath(second)
    return (
        first_path == second_path
        or first_path in second_path.parents
        or second_path in first_path.parents
    )


def macos_launchd_contract(node: dict[str, Any]) -> tuple[str, str, str]:
    name = str(node.get("name") or "macos-observer")
    target = str(node.get("launchd_target") or "")
    match = re.fullmatch(
        r"(?:(system)/([A-Za-z0-9][A-Za-z0-9._-]*)|gui/([0-9]+)/([A-Za-z0-9][A-Za-z0-9._-]*))",
        target,
    )
    if not match:
        die(
            f"macOS node {name} launchd_target must be "
            "system/<label> or gui/<numeric-uid>/<label>"
        )
    domain = "system" if match.group(1) else f"gui/{match.group(3)}"
    label = match.group(2) or match.group(4)
    assert label is not None
    return target, domain, label


def macos_script(
    node: dict[str, Any],
    version: str,
    commit: str,
    run_id: str,
    expected_dmg_sha256: str,
    sequencer_status_url: str,
    storage_status_url: str,
    closure_receipt: dict[str, Any],
) -> str:
    name = str(node.get("name") or "macos-observer")
    node_root = macos_absolute_path(node, "node_root").rstrip("/")
    launchd_target, launchd_bootstrap_domain, launchd_label = macos_launchd_contract(node)
    launchd_plist = macos_absolute_path(node, "launchd_plist")
    runtime_path = macos_absolute_path(node, "runtime_path")
    governed_bundle_path = macos_absolute_path(node, "governed_bundle_path")
    if PurePosixPath(governed_bundle_path).parent != PurePosixPath(node_root):
        die(
            f"macOS node {name} governed_bundle_path must be a root-level file under "
            f"node_root: {governed_bundle_path}"
        )
    canonical_governed_bundle_path = str(
        node.get("canonical_governed_bundle_path")
        or f"{node_root}/doc/testing/evidence/{PurePosixPath(governed_bundle_path).name}"
    )
    canonical_node = {**node, "canonical_governed_bundle_path": canonical_governed_bundle_path}
    canonical_governed_bundle_path = macos_absolute_path(
        canonical_node, "canonical_governed_bundle_path"
    )
    if not canonical_governed_bundle_path.startswith(f"{node_root}/"):
        die(
            f"macOS node {name} canonical_governed_bundle_path must stay under "
            f"node_root: {canonical_governed_bundle_path}"
        )
    if canonical_governed_bundle_path == governed_bundle_path:
        die(f"macOS node {name} canonical governed bundle must differ from active bundle")
    healthz_url = str(node.get("healthz_url") or "")
    status_url = str(node.get("status_url") or "")
    if not healthz_url or not status_url:
        die(f"macOS node {name} must declare healthz_url and status_url")
    if not healthz_url.endswith("/healthz") or not status_url.endswith("/v1/chain/status"):
        die(f"macOS node {name} must use /healthz and /v1/chain/status endpoints")
    config_paths = macos_string_paths(node, "config_paths")
    state_paths = macos_string_paths(node, "persistent_state_paths")
    config_root = f"{node_root}/config/"
    for path in config_paths:
        if not path.startswith(config_root):
            die(f"macOS node {name} config path escapes node_root/config: {path}")
    for path in state_paths:
        if not path.startswith(f"{node_root}/"):
            die(f"macOS node {name} persistent state path escapes node_root: {path}")
        if path == node_root:
            die(f"macOS node {name} persistent state path equals node_root: {path}")
    for index, path in enumerate(state_paths):
        for other in state_paths[index + 1 :]:
            if macos_paths_overlap(path, other):
                die(
                    f"macOS node {name} persistent state paths duplicate or nest: "
                    f"{path} and {other}"
                )
    protected_paths = [
        runtime_path,
        *config_paths,
        f"{node_root}/CURRENT_VERSION",
        f"{node_root}/DEPLOYED_BUILDINFO",
        governed_bundle_path,
        launchd_plist,
    ]
    for path in state_paths:
        for protected in protected_paths:
            if macos_paths_overlap(path, protected):
                die(
                    f"macOS node {name} persistent state path overlaps protected path: "
                    f"state={path} protected={protected}"
                )
    q = shlex.quote
    config_entries = " ".join(q(path) for path in config_paths)
    state_entries = " ".join(q(path) for path in state_paths)
    checkpoint_gate = observer_checkpoint_gate_macos(
        sequencer_status_url, storage_status_url, closure_receipt
    )
    return f'''#!/usr/bin/env bash
set -euo pipefail

DMG_PATH="${{1:?usage: $0 /path/to/oasis7-macos-arm64.dmg}}"
NODE_ROOT={q(node_root)}
RUNTIME_PATH={q(runtime_path)}
GOVERNED_BUNDLE_PATH={q(governed_bundle_path)}
CANONICAL_GOVERNED_BUNDLE_PATH={q(canonical_governed_bundle_path)}
LAUNCHD_PLIST={q(launchd_plist)}
LAUNCHD_TARGET={q(launchd_target)}
LAUNCHD_BOOTSTRAP_DOMAIN={q(launchd_bootstrap_domain)}
LAUNCHD_LABEL={q(launchd_label)}
HEALTHZ_URL={q(healthz_url)}
STATUS_URL={q(status_url)}
PACKAGE_VERSION={q(version)}
PACKAGE_COMMIT={q(commit)}
PACKAGE_RUN_ID={q(run_id)}
RUNTIME_ARTIFACT_REF={q(artifact_ref("macos-arm64", version, "oasis7-macos-arm64.dmg", "oasis7_chain_runtime"))}
EXPECTED_DMG_SHA256={expected_dmg_sha256}
STATE_ROLLBACK_POLICY=restore_pre_upgrade_snapshot
CONFIG_TARGETS=({config_entries})
STATE_PATHS=({state_entries})
MOUNT_POINT=""
MOUNT_ATTACHED=0
ATTEMPT_ROOT=""
MUTATED=0

die() {{
  local message="$*"
  echo "error: $message" >&2
  if [[ "$MUTATED" -eq 1 ]]; then
    if ! rollback "fatal_error"; then
      echo "rollback_failed=true fatal_error=$message" >&2
    fi
  fi
  exit 1
}}
cleanup() {{
  if [[ "$MOUNT_ATTACHED" -eq 1 ]]; then
    hdiutil detach "$MOUNT_POINT" -quiet || true
    MOUNT_ATTACHED=0
  fi
  [[ -z "$MOUNT_POINT" ]] || rm -rf "$MOUNT_POINT"
}}
trap cleanup EXIT

backup_path() {{
  local source="$1" destination="$2"
  [[ -e "$source" ]] || return 1
  mkdir -p "$(dirname "$destination")" || return 1
  if [[ -d "$source" ]]; then
    ditto "$source" "$destination" || return 1
    diff -qr "$source" "$destination" >/dev/null || return 1
  else
    cp -p "$source" "$destination" || return 1
    cmp -s "$source" "$destination" || return 1
  fi
}}

restore_file() {{
  local source="$1" destination="$2" temporary
  [[ -f "$source" ]] || return 1
  mkdir -p "$(dirname "$destination")" || return 1
  temporary="$destination.rollout-restore-$$.tmp"
  cp -p "$source" "$temporary" || return 1
  mv -f "$temporary" "$destination" || return 1
}}

runtime_sha256() {{
  shasum -a 256 "$1" | awk '{{print $1}}'
}}

file_size_bytes() {{ stat -f %z "$1"; }}

tree_metadata() {{
  local root="$1" records file relative digest size count=0 total=0 tree_hash
  records="$(mktemp "${{TMPDIR:-/tmp}}/oasis7-tree-metadata.XXXXXX")" || return 1
  while IFS= read -r file; do
    relative="${{file#$root/}}"
    digest="$(runtime_sha256 "$file")" || {{ rm -f "$records"; return 1; }}
    size="$(file_size_bytes "$file")" || {{ rm -f "$records"; return 1; }}
    printf '%s\\000%s\\000%s\n' "$relative" "$digest" "$size" >>"$records" || {{ rm -f "$records"; return 1; }}
    count=$((count + 1))
    total=$((total + size))
  done < <(find "$root" -type f -print | LC_ALL=C sort)
  tree_hash="$(runtime_sha256 "$records")" || {{ rm -f "$records"; return 1; }}
  rm -f "$records"
  printf '%s\t%s\t%s\n' "$tree_hash" "$count" "$total"
}}

verify_governed_bundle_runtime_metadata() {{
  local expected_sha256="$1" expected_size="$2" actual
  actual="$(plutil -extract runtime_build.path raw -expect string -o - "$GOVERNED_BUNDLE_PATH")" || return 1
  [[ "$actual" == "$RUNTIME_PATH" ]] || return 1
  actual="$(plutil -extract runtime_build.ref raw -expect string -o - "$GOVERNED_BUNDLE_PATH")" || return 1
  [[ "$actual" == "$RUNTIME_ARTIFACT_REF" ]] || return 1
  actual="$(plutil -extract runtime_build.resolved_path raw -expect string -o - "$GOVERNED_BUNDLE_PATH")" || return 1
  [[ "$actual" == "$RUNTIME_PATH" ]] || return 1
  actual="$(plutil -extract runtime_build.sha256 raw -expect string -o - "$GOVERNED_BUNDLE_PATH")" || return 1
  [[ "$actual" == "$expected_sha256" ]] || return 1
  actual="$(plutil -extract runtime_build.size_bytes raw -expect integer -o - "$GOVERNED_BUNDLE_PATH")" || return 1
  [[ "$actual" == "$expected_size" ]] || return 1
}}

verify_full_governed_bundle_schema() {{
  local bundle_path="$1"
  local field
  [[ "$(plutil -extract schema_version raw -expect string -o - "$bundle_path")" == "oasis7.release_candidate_bundle.v1" ]] || return 1
  for field in runtime_build world_snapshot generated_world_sidecar world_generation_provenance governance_manifest; do
    plutil -extract "$field" json -expect dictionary -o - "$bundle_path" >/dev/null || return 1
  done
}}

validate_legacy_flat_runtime_metadata() {{
  local field value
  for field in path ref resolved_path sha256 size_bytes; do
    value="$(plutil -extract "$field" raw -o - "$GOVERNED_BUNDLE_PATH")" || return 1
    [[ -n "$value" ]] || return 1
  done
}}

node_local_bundle_artifact_relative_path() {{
  case "$1" in
    world_snapshot) printf '%s\n' 'world' ;;
    generated_world_sidecar) printf '%s\n' 'generated-world/generated-scenario-world' ;;
    world_generation_provenance) printf '%s\n' 'generated-world/world-generation-provenance.json' ;;
    governance_manifest) printf '%s\n' 'doc/testing/evidence/public-testnet-governed-bootstrap-validator-registry-2026-06-06.json' ;;
    *) return 1 ;;
  esac
}}

localize_bundle_artifact() {{
  local source="$1" key="$2" relative_path local_path
  relative_path="$(node_local_bundle_artifact_relative_path "$key")" || return 1
  local_path="$NODE_ROOT/$relative_path"
  [[ -e "$local_path" ]] || return 1
  set_bundle_string "$source" "$key.ref" "$relative_path" || return 1
  set_bundle_string "$source" "$key.path" "$local_path" || return 1
  set_bundle_string "$source" "$key.resolved_path" "$local_path" || return 1
}}

set_bundle_string() {{
  local source="$1" key="$2" value="$3"
  if plutil -extract "$key" raw -o - "$source" >/dev/null 2>&1; then
    plutil -replace "$key" -string "$value" "$source"
  else
    plutil -insert "$key" -string "$value" "$source"
  fi
}}

set_bundle_integer() {{
  local source="$1" key="$2" value="$3"
  if plutil -extract "$key" raw -o - "$source" >/dev/null 2>&1; then
    plutil -replace "$key" -integer "$value" "$source"
  else
    plutil -insert "$key" -integer "$value" "$source"
  fi
}}

localize_optional_evidence_refs() {{
  local source="$1" index=0 ref basename kind local_path expected_hash expected_size actual_hash actual_size
  while plutil -extract "evidence_refs.$index" json -expect dictionary -o - "$source" >/dev/null 2>&1; do
    ref="$(plutil -extract "evidence_refs.$index.ref" raw -expect string -o - "$source")" || return 1
    kind="$(plutil -extract "evidence_refs.$index.kind" raw -expect string -o - "$source")" || return 1
    [[ "$kind" == file ]] || return 1
    basename="$(basename "$ref")"
    [[ -n "$basename" && "$basename" != . && "$basename" != .. && "$basename" != */* ]] || return 1
    local_path="$NODE_ROOT/doc/testing/evidence/$basename"
    if [[ -f "$local_path" ]]; then
      expected_hash="$(plutil -extract "evidence_refs.$index.sha256" raw -expect string -o - "$source")" || return 1
      expected_size="$(plutil -extract "evidence_refs.$index.size_bytes" raw -expect integer -o - "$source")" || return 1
      actual_hash="$(runtime_sha256 "$local_path")" || return 1
      actual_size="$(file_size_bytes "$local_path")" || return 1
      [[ "$actual_hash" == "$expected_hash" && "$actual_size" == "$expected_size" ]] || return 1
      set_bundle_string "$source" "evidence_refs.$index.ref" "doc/testing/evidence/$basename" || return 1
      set_bundle_string "$source" "evidence_refs.$index.path" "$local_path" || return 1
      set_bundle_string "$source" "evidence_refs.$index.resolved_path" "$local_path" || return 1
      set_bundle_string "$source" "evidence_refs.$index.deployment_status" localized || return 1
    else
      set_bundle_string "$source" "evidence_refs.$index.ref" "optional-unresolved/$basename" || return 1
      set_bundle_string "$source" "evidence_refs.$index.path" '' || return 1
      set_bundle_string "$source" "evidence_refs.$index.resolved_path" '' || return 1
      set_bundle_string "$source" "evidence_refs.$index.deployment_status" optional_unresolved || return 1
    fi
    index=$((index + 1))
  done
}}

localize_canonical_governed_bundle() {{
  local source="$1"
  localize_bundle_artifact "$source" world_snapshot || return 1
  localize_bundle_artifact "$source" generated_world_sidecar || return 1
  localize_bundle_artifact "$source" world_generation_provenance || return 1
  localize_bundle_artifact "$source" governance_manifest || return 1
  localize_optional_evidence_refs "$source" || return 1
  set_bundle_string "$source" repo_root "$NODE_ROOT" || return 1
}}

verify_required_node_local_bundle_artifacts() {{
  local source="$1" key kind local_path expected_hash expected_size actual_hash actual_size
  local expected_count expected_total actual_count actual_total
  for key in world_snapshot generated_world_sidecar world_generation_provenance governance_manifest; do
    kind="$(plutil -extract "$key.kind" raw -expect string -o - "$source")" || return 1
    case "$key:$kind" in
      world_snapshot:directory|generated_world_sidecar:directory|world_generation_provenance:file|governance_manifest:file) ;;
      *) return 1 ;;
    esac
    local_path="$NODE_ROOT/$(node_local_bundle_artifact_relative_path "$key")" || return 1
    [[ -e "$local_path" ]] || return 1
    if [[ "$key" == world_snapshot ]]; then
      # world_snapshot records the governed bootstrap tree, but this node-local
      # location is the running chain's mutable world. Require that the live
      # world remains a directory without comparing it to bootstrap-era tree
      # metadata; immutable generated artifacts remain hash-checked below.
      [[ -d "$local_path" && ! -L "$local_path" ]] || return 1
      continue
    fi
    if [[ "$kind" == directory ]]; then
      [[ -f "$local_path/snapshot.json" && -f "$local_path/journal.json" ]] || return 1
      expected_hash="$(plutil -extract "$key.sha256_tree" raw -expect string -o - "$source")" || return 1
      expected_count="$(plutil -extract "$key.file_count" raw -expect integer -o - "$source")" || return 1
      expected_total="$(plutil -extract "$key.total_bytes" raw -expect integer -o - "$source")" || return 1
      IFS=$'\t' read -r actual_hash actual_count actual_total < <(tree_metadata "$local_path") || return 1
      if [[ "$actual_hash" != "$expected_hash" || "$actual_count" != "$expected_count" || "$actual_total" != "$expected_total" ]]; then
        echo "node-local bundle tree integrity mismatch: artifact=$key path=$local_path expected_hash=$expected_hash actual_hash=$actual_hash expected_count=$expected_count actual_count=$actual_count expected_bytes=$expected_total actual_bytes=$actual_total" >&2
        return 1
      fi
    else
      expected_hash="$(plutil -extract "$key.sha256" raw -expect string -o - "$source")" || return 1
      expected_size="$(plutil -extract "$key.size_bytes" raw -expect integer -o - "$source")" || return 1
      actual_hash="$(runtime_sha256 "$local_path")" || return 1
      actual_size="$(file_size_bytes "$local_path")" || return 1
      if [[ "$actual_hash" != "$expected_hash" || "$actual_size" != "$expected_size" ]]; then
        echo "node-local bundle file integrity mismatch: artifact=$key path=$local_path expected_hash=$expected_hash actual_hash=$actual_hash expected_bytes=$expected_size actual_bytes=$actual_size" >&2
        return 1
      fi
    fi
  done
}}

preflight_governed_bundle_schema() {{
  [[ -f "$CANONICAL_GOVERNED_BUNDLE_PATH" ]] || {{ echo "canonical governed bundle missing" >&2; return 1; }}
  verify_full_governed_bundle_schema "$CANONICAL_GOVERNED_BUNDLE_PATH" || {{ echo "canonical governed bundle schema invalid" >&2; return 1; }}
  if ! plutil -extract runtime_build json -expect dictionary -o - "$GOVERNED_BUNDLE_PATH" >/dev/null 2>&1; then
    validate_legacy_flat_runtime_metadata || {{ echo "legacy governed runtime metadata invalid" >&2; return 1; }}
  fi
  verify_required_node_local_bundle_artifacts "$CANONICAL_GOVERNED_BUNDLE_PATH" || {{ echo "canonical governed bundle required node-local artifacts invalid" >&2; return 1; }}
}}

promote_governed_bundle_runtime_metadata() {{
  local expected_sha256="$1" expected_size="$2" source temporary source_bundle
  source="$GOVERNED_BUNDLE_PATH.rollout-$$.source.tmp"
  temporary="$GOVERNED_BUNDLE_PATH.rollout-$$.tmp"
  rm -f "$source" "$temporary"
  if plutil -extract runtime_build json -expect dictionary -o - "$GOVERNED_BUNDLE_PATH" >/dev/null 2>&1; then
    source_bundle="$GOVERNED_BUNDLE_PATH"
  else
    source_bundle="$CANONICAL_GOVERNED_BUNDLE_PATH"
  fi
  cp -p "$source_bundle" "$source" || return 1
  verify_full_governed_bundle_schema "$source" || return 1
  localize_canonical_governed_bundle "$source" || return 1
  set_bundle_string "$source" runtime_build.path "$RUNTIME_PATH" || return 1
  set_bundle_string "$source" runtime_build.ref "$RUNTIME_ARTIFACT_REF" || return 1
  set_bundle_string "$source" runtime_build.resolved_path "$RUNTIME_PATH" || return 1
  set_bundle_string "$source" runtime_build.sha256 "$expected_sha256" || return 1
  set_bundle_integer "$source" runtime_build.size_bytes "$expected_size" || return 1
  plutil -convert json -o "$temporary" "$source" || return 1
  mv -f "$temporary" "$GOVERNED_BUNDLE_PATH" || return 1
  rm -f "$source"
  verify_governed_bundle_runtime_metadata "$expected_sha256" "$expected_size" || return 1
  echo "governed_bundle_runtime_metadata_verified=true sha256=$expected_sha256 size_bytes=$expected_size"
}}

assert_launchd_plist_label() {{
  local plist_label
  plist_label="$(/usr/libexec/PlistBuddy -c 'Print :Label' "$LAUNCHD_PLIST" 2>/dev/null)" || {{
    echo "unable to read launchd plist Label: $LAUNCHD_PLIST" >&2
    return 1
  }}
  [[ "$plist_label" == "$LAUNCHD_LABEL" ]] || {{
    echo "launchd plist Label mismatch: target=$LAUNCHD_TARGET expected=$LAUNCHD_LABEL actual=$plist_label" >&2
    return 1
  }}
}}

assert_launchd_target_loaded() {{
  launchctl print "$LAUNCHD_TARGET" >/dev/null 2>&1 || {{
    echo "launchd target is not loaded: $LAUNCHD_TARGET" >&2
    return 1
  }}
}}

status_rollout_state() {{
  local status="$1" status_file fallback_required running network_head_available \
    consensus_progress_error last_error alert_index=0 alert_code transient_alert_seen=0
  [[ -n "$status" ]] || return 1
  status_file="$(mktemp "${{TMPDIR:-/tmp}}/oasis7-status.XXXXXX")" || return 1
  printf '%s' "$status" >"$status_file"

  fallback_required="$(plutil -extract consensus.state_sync_fallback_required raw -expect bool -o - "$status_file" 2>/dev/null)" || {{ rm -f "$status_file"; return 1; }}
  if [[ "$fallback_required" == true ]]; then
    rm -f "$status_file"
    return 2
  fi
  plutil -extract observability.alerts json -expect array -o /dev/null "$status_file" >/dev/null 2>&1 || {{ rm -f "$status_file"; return 1; }}
  while plutil -extract "observability.alerts.$alert_index" json -expect dictionary -o /dev/null "$status_file" >/dev/null 2>&1; do
    alert_code="$(plutil -extract "observability.alerts.$alert_index.code" raw -expect string -o - "$status_file" 2>/dev/null)" || {{ rm -f "$status_file"; return 1; }}
    case "$alert_code" in
      authority_failure|execution_driver_peer_mismatch) rm -f "$status_file"; return 2 ;;
      consensus_peer_head_unavailable) transient_alert_seen=1 ;;
    esac
    ((alert_index += 1))
  done
  if plutil -extract "observability.alerts.$alert_index" json -o /dev/null "$status_file" >/dev/null 2>&1; then
    rm -f "$status_file"
    return 1
  fi
  consensus_progress_error="$(plutil -extract consensus_progress_observer_error raw -expect string -o - "$status_file" 2>/dev/null || true)"
  last_error="$(plutil -extract last_error raw -expect string -o - "$status_file" 2>/dev/null || true)"
  case "$consensus_progress_error:$last_error" in
    *'execution driver peer mismatch'*|*'authority_failure'*) rm -f "$status_file"; return 2 ;;
  esac
  if [[ "$transient_alert_seen" -eq 1 ]]; then
    rm -f "$status_file"
    return 1
  fi
  network_head_available="$(plutil -extract observability.network_head_available raw -expect bool -o - "$status_file" 2>/dev/null)" || {{ rm -f "$status_file"; return 1; }}
  if [[ "$network_head_available" == false ]]; then
    rm -f "$status_file"
    return 1
  fi
  running="$(plutil -extract running raw -expect bool -o - "$status_file" 2>/dev/null)" || {{ rm -f "$status_file"; return 1; }}
  rm -f "$status_file"
  [[ "$fallback_required" == false && "$network_head_available" == true && "$running" == true ]]
}}

wait_for_service_health() {{
  local deadline=$((SECONDS + 120)) health status
  while (( SECONDS < deadline )); do
    health="$(curl -fsS "$HEALTHZ_URL" 2>/dev/null || true)"
    status="$(curl -fsS "$STATUS_URL" 2>/dev/null || true)"
    if status_rollout_state "$status"; then
      :
    elif [[ "$?" -eq 2 ]]; then
      echo "authority_failure_detected=true" >&2
      return 2
    else
      sleep 2
      continue
    fi
    if grep -Eq '"ok"[[:space:]]*:[[:space:]]*true' <<<"$health"; then
      return 0
    fi
    sleep 2
  done
  return 1
}}

restart_original_service() {{
  if ! launchctl print "$LAUNCHD_TARGET" >/dev/null 2>&1; then
    launchctl bootstrap "$LAUNCHD_BOOTSTRAP_DOMAIN" "$LAUNCHD_PLIST" || return 1
    assert_launchd_target_loaded || return 1
  fi
  assert_launchd_target_loaded || return 1
  wait_for_service_health || return 1
  echo "original_service_recovery_verified=true"
}}

assert_no_symlink_components() {{
  local path="$1" component current=""
  [[ "$path" == /* ]] || {{ echo "path is not absolute: $path" >&2; return 1; }}
  IFS=/ read -r -a components <<<"${{path#/}}"
  for component in "${{components[@]}}"; do
    [[ -n "$component" ]] || continue
    current="$current/$component"
    [[ ! -L "$current" ]] || {{ echo "persistent state root has symlink component: $current" >&2; return 1; }}
  done
}}

resolve_existing_physical_directory() {{
  local path="$1"
  [[ -d "$path" && ! -L "$path" ]] || return 1
  (cd -P "$path" && pwd -P)
}}

assert_state_roots_safe() {{
  local require_present="$1" state_path state_parent physical_node_root physical_state_path
  assert_no_symlink_components "$NODE_ROOT" || return 1
  physical_node_root="$(resolve_existing_physical_directory "$NODE_ROOT")" || {{
    echo "node root is not a physical directory: $NODE_ROOT" >&2
    return 1
  }}
  for state_path in "${{STATE_PATHS[@]}}"; do
    if [[ "$require_present" == true && ! -e "$state_path" ]]; then
      echo "persistent state root missing: $state_path" >&2
      return 1
    fi
    assert_no_symlink_components "$state_path" || return 1
    if [[ -e "$state_path" && ! -d "$state_path" ]]; then
      echo "persistent state root is not a directory: $state_path" >&2
      return 1
    fi
    if [[ -e "$state_path" ]]; then
      physical_state_path="$(resolve_existing_physical_directory "$state_path")" || return 1
    else
      state_parent="$(dirname "$state_path")"
      physical_state_path="$(resolve_existing_physical_directory "$state_parent")/$(basename "$state_path")" || {{
        echo "persistent state parent is not a physical directory: $state_parent" >&2
        return 1
      }}
    fi
    [[ "$physical_state_path" == "$physical_node_root/"* ]] || {{
      echo "persistent state root escapes physical node root: state=$state_path resolved=$physical_state_path node_root=$physical_node_root" >&2
      return 1
    }}
  done
}}

backup_persistent_state() {{
  local index
  assert_state_roots_safe true || return 1
  for index in "${{!STATE_PATHS[@]}}"; do
    backup_path "${{STATE_PATHS[$index]}}" "$ATTEMPT_ROOT/state/$index" || return 1
  done
  echo "state_backup_closure_complete=true root=$ATTEMPT_ROOT/state"
}}

preserve_failed_state() {{
  local index
  assert_state_roots_safe true || return 1
  for index in "${{!STATE_PATHS[@]}}"; do
    backup_path "${{STATE_PATHS[$index]}}" "$ATTEMPT_ROOT/failed-state/$index" || return 1
  done
  echo "failed_state_evidence_complete=true root=$ATTEMPT_ROOT/failed-state"
}}

restore_pre_upgrade_state() {{
  local index target
  assert_state_roots_safe false || return 1
  for index in "${{!STATE_PATHS[@]}}"; do
    target="${{STATE_PATHS[$index]}}"
    [[ -e "$ATTEMPT_ROOT/state/$index" ]] || return 1
    rm -rf "$target" || return 1
    ditto "$ATTEMPT_ROOT/state/$index" "$target" || return 1
    diff -qr "$ATTEMPT_ROOT/state/$index" "$target" >/dev/null || return 1
    echo "rollback_state_restored=true policy=$STATE_ROLLBACK_POLICY path=$target"
  done
}}

rollback() {{
  local reason="$1" index
  [[ "$MUTATED" -eq 1 ]] || return 0
  MUTATED=0
  trap - ERR
  echo "rollback_begin=true reason=$reason"
  if launchctl print "$LAUNCHD_TARGET" >/dev/null 2>&1; then
    launchctl bootout "$LAUNCHD_TARGET" || return 1
  fi
  preserve_failed_state || return 1
  restore_pre_upgrade_state || return 1
  restore_file "$ATTEMPT_ROOT/runtime/oasis7_chain_runtime" "$RUNTIME_PATH" || return 1
  for index in "${{!CONFIG_TARGETS[@]}}"; do
    restore_file "$ATTEMPT_ROOT/config/$index" "${{CONFIG_TARGETS[$index]}}" || return 1
  done
  restore_file "$ATTEMPT_ROOT/governed-bundle.json" "$GOVERNED_BUNDLE_PATH" || return 1
  cmp -s "$ATTEMPT_ROOT/governed-bundle.json" "$GOVERNED_BUNDLE_PATH" || return 1
  echo "rollback_governed_bundle_metadata_verified=true"
  restore_file "$ATTEMPT_ROOT/CURRENT_VERSION" "$NODE_ROOT/CURRENT_VERSION" || return 1
  restore_file "$ATTEMPT_ROOT/DEPLOYED_BUILDINFO" "$NODE_ROOT/DEPLOYED_BUILDINFO" || return 1
  launchctl bootstrap "$LAUNCHD_BOOTSTRAP_DOMAIN" "$LAUNCHD_PLIST" || return 1
  assert_launchd_target_loaded || return 1
  wait_for_service_health || return 1
  echo "rollback_service_health_verified=true"
  echo "rollback_complete=true"
}}

on_error() {{
  local status=$?
  trap - ERR
  if ! rollback "unexpected_failure"; then
    echo "rollback_failed=true original_exit=$status" >&2
    exit 1
  fi
  exit "$status"
}}
trap on_error ERR

[[ "$(uname -s)" == Darwin ]] || die "macOS operator script must run on Darwin"
[[ "$(uname -m)" == arm64 ]] || die "macOS observer requires native arm64 host"
[[ "$(sysctl -n hw.optional.arm64 2>/dev/null || echo 0)" == 1 ]] || die "native arm64 capability unavailable"
[[ -f "$DMG_PATH" ]] || die "DMG missing: $DMG_PATH"
[[ -f "$LAUNCHD_PLIST" ]] || die "launchd plist missing: $LAUNCHD_PLIST"
assert_launchd_plist_label || die "launchd plist Label preflight failed"
assert_launchd_target_loaded || die "launchd target preflight failed"
[[ -x "$RUNTIME_PATH" ]] || die "active runtime missing: $RUNTIME_PATH"
[[ -f "$GOVERNED_BUNDLE_PATH" ]] || die "active governed bundle missing: $GOVERNED_BUNDLE_PATH"
preflight_governed_bundle_schema || die "active governed bundle schema preflight failed"
[[ -f "$NODE_ROOT/CURRENT_VERSION" && -f "$NODE_ROOT/DEPLOYED_BUILDINFO" ]] || die "active deployment provenance missing"
for state_path in "${{STATE_PATHS[@]}}"; do [[ -e "$state_path" ]] || die "persistent state missing: $state_path"; done
for index in "${{!CONFIG_TARGETS[@]}}"; do [[ -f "${{CONFIG_TARGETS[$index]}}" ]] || die "active config missing: ${{CONFIG_TARGETS[$index]}}"; done

{checkpoint_gate}

actual_dmg_sha256="$(shasum -a 256 "$DMG_PATH" | awk '{{print $1}}')"
[[ "$actual_dmg_sha256" == "$EXPECTED_DMG_SHA256" ]] || die "DMG checksum mismatch: expected=$EXPECTED_DMG_SHA256 actual=$actual_dmg_sha256"
echo "dmg_sha256_verified=true sha256=$actual_dmg_sha256"
MOUNT_POINT="$(mktemp -d "${{TMPDIR:-/tmp}}/oasis7-macos-dmg.XXXXXX")"
hdiutil attach -readonly -nobrowse -mountpoint "$MOUNT_POINT" "$DMG_PATH" >/dev/null
MOUNT_ATTACHED=1
PACKAGE_RUNTIME="$(find "$MOUNT_POINT" -type f -name oasis7_chain_runtime -perm -u+x -print -quit)"
[[ -n "$PACKAGE_RUNTIME" ]] || die "DMG lacks executable oasis7_chain_runtime"
if command -v lipo >/dev/null 2>&1; then
  lipo -archs "$PACKAGE_RUNTIME" | tr ' ' '\\n' | grep -qx arm64 || die "DMG runtime is not arm64"
else
  file "$PACKAGE_RUNTIME" | grep -q arm64 || die "cannot verify arm64 runtime identity"
fi
echo "native_identity_verified=aarch64-apple-darwin"

ATTEMPT_ROOT="$(mktemp -d "$NODE_ROOT/.package-rollout-attempt.XXXXXX")"
backup_path "$RUNTIME_PATH" "$ATTEMPT_ROOT/runtime/oasis7_chain_runtime" || die "runtime preflight backup failed"
backup_path "$GOVERNED_BUNDLE_PATH" "$ATTEMPT_ROOT/governed-bundle.json" || die "governed bundle preflight backup failed"
backup_path "$NODE_ROOT/CURRENT_VERSION" "$ATTEMPT_ROOT/CURRENT_VERSION" || die "CURRENT_VERSION preflight backup failed"
backup_path "$NODE_ROOT/DEPLOYED_BUILDINFO" "$ATTEMPT_ROOT/DEPLOYED_BUILDINFO" || die "DEPLOYED_BUILDINFO preflight backup failed"
for index in "${{!CONFIG_TARGETS[@]}}"; do
  backup_path "${{CONFIG_TARGETS[$index]}}" "$ATTEMPT_ROOT/config/$index" || die "config preflight backup failed: ${{CONFIG_TARGETS[$index]}}"
done
echo "preflight_backup_closure_complete=true root=$ATTEMPT_ROOT"

if ! launchctl bootout "$LAUNCHD_TARGET"; then
  echo "original_service_stop_failed=true" >&2
  restart_original_service || die "original service recovery failed after bootout error"
  exit 1
fi
echo "original_service_stopped=true"
if ! backup_persistent_state; then
  echo "state_backup_failed_before_promotion=true" >&2
  restart_original_service || die "original service recovery failed after state backup error"
  exit 1
fi
echo "rollback_closure_complete=true root=$ATTEMPT_ROOT"

MUTATED=1
echo "promotion_begin=true"
install -m 755 "$PACKAGE_RUNTIME" "$RUNTIME_PATH"
PROMOTED_RUNTIME_SHA256="$(runtime_sha256 "$RUNTIME_PATH")"
PROMOTED_RUNTIME_SIZE_BYTES="$(stat -f %z "$RUNTIME_PATH")"
[[ "$PROMOTED_RUNTIME_SHA256" == "$(runtime_sha256 "$PACKAGE_RUNTIME")" ]] || die "promoted runtime checksum mismatch"
promote_governed_bundle_runtime_metadata "$PROMOTED_RUNTIME_SHA256" "$PROMOTED_RUNTIME_SIZE_BYTES" || die "governed bundle runtime metadata promotion failed"
printf '%s\\n' "$PACKAGE_VERSION" >"$NODE_ROOT/CURRENT_VERSION"
printf 'workflow=Testnet Packages\\nrun_id=%s\\ncommit=%s\\nplatform=macos-arm64\\n' "$PACKAGE_RUN_ID" "$PACKAGE_COMMIT" >"$NODE_ROOT/DEPLOYED_BUILDINFO"
launchctl bootstrap "$LAUNCHD_BOOTSTRAP_DOMAIN" "$LAUNCHD_PLIST"
assert_launchd_target_loaded || die "launchd target missing after promotion bootstrap"

authority_failure=0
deadline=$((SECONDS + 120))
while (( SECONDS < deadline )); do
  health="$(curl -fsS "$HEALTHZ_URL" 2>/dev/null || true)"
  status="$(curl -fsS "$STATUS_URL" 2>/dev/null || true)"
  if status_rollout_state "$status"; then
    :
  elif [[ "$?" -eq 2 ]]; then
    authority_failure=1
    break
  else
    sleep 2
    continue
  fi
  if grep -Eq '"ok"[[:space:]]*:[[:space:]]*true' <<<"$health"; then
    echo "startup_verified=true healthz=$HEALTHZ_URL status=$STATUS_URL"
    MUTATED=0
    trap - ERR
    exit 0
  fi
  sleep 2
done
if [[ "$authority_failure" -eq 1 ]]; then
  rollback_status=0
  rollback "authority_failure" || rollback_status=$?
  echo "state_sync_escalation_required=true reason=authority_failure rollback_status=$rollback_status action=use_verified_state_sync"
  exit 1
fi
if ! rollback "readiness_timeout"; then
  echo "rollback_failed=true reason=readiness_timeout" >&2
  exit 1
fi
die "macOS observer readiness timed out"
'''


def write_macos_plan(
    out_dir: Path,
    node: dict[str, Any],
    macos_asset: Path,
    version: str,
    commit: str,
    run_id: str,
    sequencer_status_url: str,
    storage_status_url: str,
    closure_receipt: dict[str, Any],
) -> tuple[Path, list[str]]:
    name = str(node.get("name") or "macos-observer")
    safe_name = "".join(ch if ch.isalnum() or ch in "._-" else "-" for ch in name)
    script_path = out_dir / f"{safe_name}-macos-arm64-upgrade.sh"
    expected_dmg_sha256 = sha256_file(macos_asset)
    script_path.write_text(
        macos_script(
            node,
            version,
            commit,
            run_id,
            expected_dmg_sha256,
            sequencer_status_url,
            storage_status_url,
            closure_receipt,
        ),
        encoding="utf-8",
    )
    script_path.chmod(0o755)
    host = str(node.get("host") or "")
    if not host:
        return script_path, [shell_join(["bash", str(script_path), str(macos_asset)])]
    user = str(node.get("user") or "root")
    remote_dmg = str(node.get("remote_dmg") or macos_asset.name)
    remote_script = str(node.get("remote_script") or script_path.name)
    remote_target = f"{user}@{host}"
    return script_path, [
        shell_join(["scp", str(macos_asset), f"{remote_target}:{remote_dmg}"]),
        shell_join(["scp", str(script_path), f"{remote_target}:{remote_script}"]),
        shell_join(["ssh", remote_target, "bash", remote_script, remote_dmg]),
    ]


def main() -> int:
    parser = argparse.ArgumentParser(
        description=(
            "Plan standardized public testnet package version replacements. The default mode is "
            "plan-only and does not mutate nodes. Mutation requires either --apply-local for local "
            "linux-x64 entries or deliberate execution of the generated operator commands/scripts. "
            "The script never reads or stores credentials; remote execution commands are rendered "
            "for the operator's SSH transport."
        )
    )
    parser.add_argument("--manifest", required=True, type=Path, help="JSON node rollout manifest")
    parser.add_argument("--package-dir", required=True, type=Path, help="Downloaded GitHub artifact directory")
    parser.add_argument("--out-dir", type=Path, default=Path(".tmp/testnet-package-rollout"))
    parser.add_argument(
        "--apply-local",
        action="store_true",
        help="Mutate local linux-x64 nodes without a host; omitted means plan-only.",
    )
    parser.add_argument(
        "--readiness-policy",
        choices=("rpc-running", "strict-ready", "degraded-ok"),
        default="rpc-running",
        help=(
            "Post-restart health policy for generated plans. rpc-running keeps replacement "
            "separate from network recovery, strict-ready passes an explicit healthz_url into "
            "the Linux replacement primitive while retaining status_url for later pair "
            "agreement, and degraded-ok records an operator-tolerated degraded rollout."
        ),
    )
    parser.add_argument("--json", action="store_true", help="Print machine-readable rollout plan")
    parser.add_argument(
        "--known-hosts",
        "--operator-known-hosts",
        dest="known_hosts",
        type=Path,
        help=(
            "operator-local pinned known-hosts file for remote Linux provider SSH; "
            "required when a canonical provider has a host"
        ),
    )
    args = parser.parse_args()

    manifest = load_manifest(args.manifest)
    platforms = sorted({str(node.get("platform") or "") for node in manifest["nodes"]})
    if "" in platforms:
        die("all nodes must declare platform")
    names = [str(node.get("name") or "") for node in manifest["nodes"] if isinstance(node, dict)]
    if len(names) != len(manifest["nodes"]) or not all(names) or len(set(names)) != len(names):
        die("all rollout manifest nodes must have unique non-empty names")
    observer_rollout_present = any(name not in CANONICAL_PROVIDER_NAMES for name in names)
    remote_linux_provider_present = any(
        isinstance(node, dict)
        and str(node.get("platform") or "") == "linux-x64"
        and str(node.get("name") or "") in CANONICAL_PROVIDER_NAMES
        and bool(str(node.get("host") or ""))
        for node in manifest["nodes"]
    )
    operator_known_hosts = None
    if args.known_hosts is not None:
        operator_known_hosts = validate_operator_known_hosts(args.known_hosts)
    elif remote_linux_provider_present:
        validate_operator_known_hosts(None)

    package_dir = args.package_dir.resolve()
    out_dir = args.out_dir.resolve()
    out_dir.mkdir(parents=True, exist_ok=True)

    # This happens before the probe starts a runtime from the downloaded Linux
    # bundle.  A malformed package must fail before any package code executes.
    trust_platforms = list(platforms)
    if observer_rollout_present and "linux-x64" not in trust_platforms:
        # The clean-room closure probe always executes the Linux bundle, even
        # when the rollout plan mutates only a Windows or macOS observer.
        trust_platforms.append("linux-x64")
    platform_dirs, platform_infos, platform_assets, verified_files = verify_package_trust(
        package_dir, sorted(trust_platforms)
    )
    platform_ops_tools_assets = {
        platform: platform_ops_tools_asset(platform_dirs[platform], platform)
        for platform in trust_platforms
    }
    # A multi-platform package is one release truth.  Reject divergent
    # BUILDINFO before a trusted bundle is allowed to start the probe.
    commit = require_same_commit(platform_infos)
    provider_status_urls = (
        canonical_provider_status_urls(manifest) if observer_rollout_present else None
    )
    checkpoint_closure_receipt = None
    if observer_rollout_present:
        if os.environ.get("OASIS7_CHECKPOINT_CLOSURE_RECEIPT"):
            die("caller-supplied checkpoint closure receipt is forbidden; the rollout executes its checkpoint closure probe")
        checkpoint_closure_receipt = run_checkpoint_closure_probe(
            args.manifest.resolve(), package_dir, out_dir
        )

    platform_provenance = {
        platform: package_provenance(
            platform,
            platform_dirs[platform],
            platform_assets[platform],
            platform_infos[platform],
        )
        for platform in platforms
    }

    plan: dict[str, Any] = {
        "commit": commit,
        "platform_provenance": platform_provenance,
        "out_dir": str(out_dir),
        "readiness_policy": args.readiness_policy,
        "verified_files": verified_files,
        "nodes": [],
    }

    for raw_node in manifest["nodes"]:
        if not isinstance(raw_node, dict):
            die("each node manifest entry must be an object")
        node = raw_node
        platform = str(node.get("platform"))
        name = str(node.get("name") or platform)
        node_plan: dict[str, Any] = {
            "name": name,
            "platform": platform,
            "host": node.get("host"),
            "package_provenance": platform_provenance[platform],
            "commands": [],
            "applied": False,
        }
        version = platform_provenance[platform]["package_version"]
        run_id = platform_provenance[platform]["run_id"]
        if platform == "linux-x64":
            if name in CANONICAL_PROVIDER_NAMES:
                command = linux_command(
                    node,
                    platform_assets[platform],
                    platform_ops_tools_assets[platform],
                    version,
                    commit,
                    run_id,
                    args.readiness_policy,
                )
                node_plan["commands"].extend(
                    linux_plan_commands(
                        node,
                        platform_assets[platform],
                        platform_ops_tools_assets[platform],
                        version,
                        commit,
                        run_id,
                        args.readiness_policy,
                        operator_known_hosts,
                    )
                )
                if name in CANONICAL_PROVIDER_NAMES and node.get("host"):
                    assert operator_known_hosts is not None
                    node_plan["operator_known_hosts"] = str(operator_known_hosts)
            else:
                assert provider_status_urls is not None
                assert checkpoint_closure_receipt is not None
                script_path, commands, command = write_linux_observer_plan(
                    out_dir,
                    node,
                    platform_assets[platform],
                    platform_ops_tools_assets[platform],
                    version,
                    commit,
                    run_id,
                    args.readiness_policy,
                    *provider_status_urls,
                    checkpoint_closure_receipt,
                )
                node_plan["observer_checkpoint_gate_script"] = str(script_path)
                node_plan["checkpoint_closure_receipt"] = checkpoint_closure_receipt
                node_plan["commands"].extend(commands)
            if args.apply_local and not node.get("host"):
                applied = subprocess.run(
                    ["bash", str(node_plan["observer_checkpoint_gate_script"])]
                    if name not in CANONICAL_PROVIDER_NAMES
                    else command,
                    check=True,
                    stdout=subprocess.PIPE,
                    stderr=subprocess.STDOUT,
                    text=True,
                )
                node_plan["apply_output"] = applied.stdout.strip().splitlines()
                node_plan["applied"] = True
        elif platform == "windows-x64":
            if name in CANONICAL_PROVIDER_NAMES:
                die("canonical providers must use Linux rollout entries")
            assert provider_status_urls is not None
            assert checkpoint_closure_receipt is not None
            governed_files = windows_governed_files(
                platform_dirs[platform], verified_files[platform]
            )
            script_path, commands = write_windows_plan(
                out_dir,
                node,
                platform_assets[platform],
                version,
                commit,
                run_id,
                governed_files,
                args.readiness_policy,
                *provider_status_urls,
                checkpoint_closure_receipt,
            )
            node_plan["windows_script"] = str(script_path)
            node_plan["checkpoint_closure_receipt"] = checkpoint_closure_receipt
            node_plan["governed_bundle_path"] = str(node.get("governed_bundle_path") or WINDOWS_GOVERNED_BUNDLE)
            node_plan["governed_genesis_path"] = str(
                node.get("governed_genesis_path") or WINDOWS_GOVERNED_GENESIS
            )
            node_plan["commands"].extend(commands)
        elif platform in {"macos-x64", "macos-arm64"}:
            if name in CANONICAL_PROVIDER_NAMES:
                die("canonical providers must use Linux rollout entries")
            if platform == "macos-arm64":
                assert checkpoint_closure_receipt is not None
                script_path, commands = write_macos_plan(
                    out_dir,
                    node,
                    platform_assets[platform],
                    version,
                    commit,
                    run_id,
                    *(provider_status_urls or ()),
                    checkpoint_closure_receipt,
                )
                node_plan["macos_script"] = str(script_path)
                node_plan["checkpoint_closure_receipt"] = checkpoint_closure_receipt
                node_plan["native_identity"] = "aarch64-apple-darwin"
                node_plan["expected_dmg_sha256"] = sha256_file(platform_assets[platform])
                node_plan["state_sync_escalation"] = "explicit_on_authority_failure"
                node_plan["commands"].extend(commands)
            else:
                node_plan["note"] = (
                    "macos-x64 packages remain verification-only installer artifacts; the "
                    "native observer rollout contract currently applies only to macos-arm64."
                )
        else:
            die(f"unsupported platform in node {name}: {platform}")
        plan["nodes"].append(node_plan)

    plan_path = out_dir / "rollout-plan.json"
    plan_path.write_text(json.dumps(plan, ensure_ascii=True, indent=2, sort_keys=True) + "\n", encoding="utf-8")

    if args.json:
        print(json.dumps(plan, ensure_ascii=False, indent=2, sort_keys=True))
    else:
        print(f"commit={commit}")
        for platform in platforms:
            provenance = platform_provenance[platform]
            print(
                "package_provenance="
                f"platform={platform} package_version={provenance['package_version']} "
                f"run_id={provenance['run_id']} asset_sha256={provenance['asset_sha256']}"
            )
        print(f"rollout_plan={plan_path}")
        for node in plan["nodes"]:
            print(f"node={node['name']} platform={node['platform']} applied={str(node['applied']).lower()}")
            for command in node["commands"]:
                print(f"  {command}")
            if "windows_script" in node:
                print(f"  windows_script={node['windows_script']}")
            if "note" in node:
                print(f"  note={node['note']}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
