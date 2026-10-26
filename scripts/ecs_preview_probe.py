"""Bounded, signed DevLocal participant-policy read used by ECS deployment."""

from __future__ import annotations

import http.client
import importlib
import ipaddress
import json
import os
import re
import secrets
import site
import stat
import sys
import sysconfig
import time
from collections.abc import Mapping
from pathlib import Path
from urllib.parse import urlsplit


PATH = "/v1/world/participant-policy"
SIGNING_DOMAIN = "oasis7.world-service.v1"
MAX_RESPONSE_BYTES = 65_536
HTTP_TIMEOUT_SECONDS = 15
MAX_U64 = (1 << 64) - 1
_HEX_32 = re.compile(r"^[0-9a-fA-F]{64}$")
_HEX_64 = re.compile(r"^[0-9a-fA-F]{128}$")
_DECIMAL_U64 = re.compile(r"^(?:0|[1-9][0-9]*)$")
_CONFIG_KEYS = {
    "expected_world",
    "service_public_key",
    "reader_public_key",
    "reader_key_file",
    "world_service_url",
    "python_path",
    "reader_uid",
}


class ProbeError(RuntimeError):
    """An intentionally bounded error safe to put in a deployment report."""


def _fail(code: str) -> None:
    raise ProbeError(code)


def _string(value: object, *, code: str, maximum: int = 256) -> str:
    if not isinstance(value, str) or not value or value != value.strip() or len(value) > maximum:
        _fail(code)
    if any(ord(character) < 0x20 or ord(character) == 0x7F for character in value):
        _fail(code)
    return value


def _decode_public_key(value: object, code: str) -> bytes:
    if not isinstance(value, str) or not _HEX_32.fullmatch(value):
        _fail(code)
    return bytes.fromhex(value)


def _config(config: object) -> tuple[dict[str, str], bytes, bytes, Path, tuple[str, int], int]:
    if not isinstance(config, Mapping) or set(config) - _CONFIG_KEYS:
        _fail("probe_config_invalid")

    world = config.get("expected_world")
    if not isinstance(world, Mapping) or set(world) != {"world_id", "genesis_digest"}:
        _fail("probe_config_invalid")
    expected_world = {
        "world_id": _string(world.get("world_id"), code="probe_config_invalid"),
        "genesis_digest": _string(world.get("genesis_digest"), code="probe_config_invalid"),
    }

    service_public_key = _decode_public_key(config.get("service_public_key"), "probe_service_key_invalid")
    reader_public_key = _decode_public_key(config.get("reader_public_key"), "probe_reader_key_pin_invalid")
    reader_key_file = config.get("reader_key_file")
    if not isinstance(reader_key_file, str) or not reader_key_file or not os.path.isabs(reader_key_file):
        _fail("probe_config_invalid")
    key_path = Path(reader_key_file)
    if ".." in key_path.parts:
        _fail("probe_config_invalid")

    reader_uid = config.get("reader_uid", 999)
    if isinstance(reader_uid, bool) or not isinstance(reader_uid, int) or not 0 <= reader_uid <= 0xFFFFFFFF:
        _fail("probe_config_invalid")

    endpoint = _loopback_endpoint(config.get("world_service_url"))
    return expected_world, service_public_key, reader_public_key, key_path, endpoint, reader_uid


def _loopback_endpoint(value: object) -> tuple[str, int]:
    if not isinstance(value, str) or len(value) > 512:
        _fail("probe_loopback_url_invalid")
    try:
        parsed = urlsplit(value)
        port = parsed.port
        host = parsed.hostname
    except ValueError:
        _fail("probe_loopback_url_invalid")
    if (
        parsed.scheme != "http"
        or not host
        or parsed.path not in ("", "/")
        or parsed.query
        or parsed.fragment
        or parsed.username is not None
        or parsed.password is not None
        or (port is not None and not 1 <= port <= 65535)
    ):
        _fail("probe_loopback_url_invalid")
    if host.lower() != "localhost":
        try:
            if not ipaddress.ip_address(host).is_loopback:
                _fail("probe_loopback_url_invalid")
        except ValueError:
            _fail("probe_loopback_url_invalid")
    return host, port or 80


def _key_file_metadata(path: Path, reader_uid: int) -> os.stat_result:
    try:
        before = os.lstat(path)
    except OSError:
        _fail("probe_reader_key_unavailable")
    if stat.S_ISLNK(before.st_mode) or not stat.S_ISREG(before.st_mode) or before.st_nlink != 1:
        _fail("probe_reader_key_file_invalid")
    if before.st_uid not in {0, reader_uid}:
        _fail("probe_reader_key_owner_invalid")
    if stat.S_IMODE(before.st_mode) != 0o600:
        _fail("probe_reader_key_permissions")
    return before


def _read_reader_seed(path: Path, reader_uid: int) -> bytes:
    before = _key_file_metadata(path, reader_uid)
    flags = os.O_RDONLY | getattr(os, "O_CLOEXEC", 0) | getattr(os, "O_NOFOLLOW", 0)
    try:
        descriptor = os.open(path, flags)
    except OSError:
        _fail("probe_reader_key_file_invalid")
    try:
        after = os.fstat(descriptor)
        if (
            not stat.S_ISREG(after.st_mode)
            or after.st_nlink != 1
            or after.st_uid not in {0, reader_uid}
            or stat.S_IMODE(after.st_mode) != 0o600
            or (after.st_dev, after.st_ino) != (before.st_dev, before.st_ino)
        ):
            _fail("probe_reader_key_file_invalid")
        content = os.read(descriptor, 66)
        if len(content) == 32:
            return content
        if len(content) in (64, 65) and _HEX_32.fullmatch(content[:64].decode("ascii", errors="ignore")):
            if len(content) == 65 and content[64:] != b"\n":
                _fail("probe_reader_key_invalid")
            return bytes.fromhex(content[:64].decode("ascii"))
        _fail("probe_reader_key_invalid")
    except OSError:
        _fail("probe_reader_key_file_invalid")
    finally:
        os.close(descriptor)


def _no_symlink_absolute_path(path: Path) -> bool:
    if not path.is_absolute() or ".." in path.parts:
        return False
    current = Path(path.anchor)
    try:
        for part in path.parts[1:]:
            current = current / part
            metadata = os.lstat(current)
            if stat.S_ISLNK(metadata.st_mode):
                return False
    except OSError:
        return False
    return True


def _trusted_python_path(value: object) -> list[str]:
    if value is None:
        return []
    if not isinstance(value, list) or len(value) > 16:
        _fail("probe_python_path_invalid")
    paths: list[str] = []
    for item in value:
        if not isinstance(item, str):
            _fail("probe_python_path_invalid")
        path = Path(item)
        if not _no_symlink_absolute_path(path):
            _fail("probe_python_path_invalid")
        try:
            metadata = os.stat(path)
        except OSError:
            _fail("probe_python_path_invalid")
        if not stat.S_ISDIR(metadata.st_mode) or metadata.st_uid != 0 or metadata.st_mode & 0o022:
            _fail("probe_python_path_untrusted")
        normalized = str(path)
        if normalized not in paths:
            paths.append(normalized)
    return paths


def _root_trusted_default_paths() -> list[str]:
    """Retain only root-managed interpreter paths, excluding cwd/PYTHONPATH/user-site."""
    candidates: list[str] = []
    try:
        configured = sysconfig.get_paths()
        candidates.extend(configured.get(name, "") for name in ("stdlib", "platstdlib", "purelib", "platlib"))
        candidates.extend(site.getsitepackages())
        candidates.extend(entry for entry in sys.path if entry and os.path.isabs(entry))
    except (OSError, TypeError):
        _fail("probe_dependency_missing")

    current_directory = Path.cwd().resolve()
    try:
        configured_user_sites = site.getusersitepackages()
        if isinstance(configured_user_sites, str):
            configured_user_sites = [configured_user_sites]
        user_sites = {str(Path(entry).resolve()) for entry in configured_user_sites if entry}
    except (OSError, TypeError):
        user_sites = set()
    safe: list[str] = []
    for item in candidates:
        if not item:
            continue
        path = Path(item)
        try:
            resolved = path.resolve(strict=True)
            if str(resolved) in user_sites or resolved == current_directory:
                continue
            metadata = os.stat(resolved)
        except OSError:
            continue
        if stat.S_ISDIR(metadata.st_mode) and metadata.st_uid == 0 and not metadata.st_mode & 0o022:
            value = str(resolved)
            if value not in safe:
                safe.append(value)
    return safe


def _load_dependencies(config: Mapping[str, object]) -> tuple[object, object, object, object, object]:
    optional_paths = _trusted_python_path(config.get("python_path"))
    search_paths = optional_paths + [path for path in _root_trusted_default_paths() if path not in optional_paths]
    if not search_paths:
        _fail("probe_dependency_missing")
    original_path = sys.path[:]
    try:
        sys.path[:] = search_paths
        cbor2 = importlib.import_module("cbor2")
        blake3 = importlib.import_module("blake3")
        ed25519 = importlib.import_module("cryptography.hazmat.primitives.asymmetric.ed25519")
        serialization = importlib.import_module("cryptography.hazmat.primitives.serialization")
        for name, module in tuple(sys.modules.items()):
            if (name == "cbor2" or name.startswith("cbor2.") or name == "blake3" or name.startswith("blake3.")
                    or name == "cryptography" or name.startswith("cryptography.")):
                if module is not None and not _module_origin_is_trusted(module, search_paths):
                    _fail("probe_dependency_untrusted")
        return cbor2, blake3, ed25519, serialization, search_paths
    except ProbeError:
        raise
    except Exception:
        _fail("probe_dependency_missing")
    finally:
        sys.path[:] = original_path


def _module_origin_is_trusted(module: object, search_paths: list[str]) -> bool:
    origin = getattr(module, "__file__", None)
    if not isinstance(origin, str):
        return False
    try:
        resolved_origin = Path(origin).resolve(strict=True)
        for root in search_paths:
            resolved_root = Path(root).resolve(strict=True)
            if resolved_root.is_dir() and resolved_origin.is_relative_to(resolved_root):
                return True
    except OSError:
        return False
    return False


def _prepare(config: object) -> tuple[dict[str, str], tuple[str, int], object, str, object, object, object]:
    if not isinstance(config, Mapping):
        _fail("probe_config_invalid")
    expected_world, service_key_bytes, reader_pin_bytes, key_path, endpoint, reader_uid = _config(config)
    cbor2, blake3, ed25519, serialization, search_paths = _load_dependencies(config)
    seed = _read_reader_seed(key_path, reader_uid)
    try:
        private_key = ed25519.Ed25519PrivateKey.from_private_bytes(seed)
        public_key = private_key.public_key()
        reader_public_key = public_key.public_bytes(serialization.Encoding.Raw, serialization.PublicFormat.Raw)
    except Exception:
        _fail("probe_reader_key_invalid")
    try:
        service_verifier = ed25519.Ed25519PublicKey.from_public_bytes(service_key_bytes)
    except Exception:
        _fail("probe_service_key_invalid")
    if not secrets.compare_digest(reader_public_key, reader_pin_bytes):
        _fail("probe_reader_key_pin_mismatch")
    return expected_world, endpoint, private_key, reader_public_key.hex(), service_verifier, cbor2, (blake3, search_paths)


def preflight(config: Mapping[str, object]) -> str:
    """Validate pins, the private reader file, and crypto modules without HTTP."""
    _, _, _, reader_public_key, _, _, _ = _prepare(config)
    return reader_public_key


def _canonical_bytes(cbor2: object, value: object) -> bytes:
    try:
        return cbor2.dumps(value, canonical=True)
    except Exception:
        _fail("probe_canonical_encoding_failed")


def _unique_object(pairs: list[tuple[str, object]]) -> dict[str, object]:
    result: dict[str, object] = {}
    for key, value in pairs:
        if key in result:
            raise ValueError("duplicate JSON key")
        result[key] = value
    return result


def _read_response(endpoint: tuple[str, int], request_body: bytes) -> bytes:
    host, port = endpoint
    connection = http.client.HTTPConnection(host, port, timeout=HTTP_TIMEOUT_SECONDS)
    deadline = time.monotonic() + HTTP_TIMEOUT_SECONDS
    try:
        connection.request(
            "POST",
            PATH,
            body=request_body,
            headers={
                "Accept": "application/json",
                "Content-Type": "application/json",
                "Connection": "close",
            },
        )
        response = connection.getresponse()
        if response.status != 200:
            _fail("probe_http_failed")
        content_length = response.getheader("Content-Length")
        transfer_encoding = response.getheader("Transfer-Encoding")
        if content_length is not None:
            if transfer_encoding is not None or not content_length.isascii() or not content_length.isdecimal():
                _fail("probe_response_invalid")
            if int(content_length) > MAX_RESPONSE_BYTES:
                _fail("probe_response_too_large")
        content_type = response.getheader("Content-Type", "").split(";", 1)[0].strip().lower()
        if content_type != "application/json":
            _fail("probe_response_invalid")

        chunks = bytearray()
        while True:
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                _fail("probe_http_timeout")
            if connection.sock is not None:
                connection.sock.settimeout(remaining)
            part = response.read1(min(8192, MAX_RESPONSE_BYTES + 1 - len(chunks)))
            if not part:
                break
            chunks.extend(part)
            if len(chunks) > MAX_RESPONSE_BYTES:
                _fail("probe_response_too_large")
        if content_length is not None and len(chunks) != int(content_length):
            _fail("probe_response_invalid")
        return bytes(chunks)
    except ProbeError:
        raise
    except TimeoutError:
        _fail("probe_http_timeout")
    except (OSError, http.client.HTTPException, ValueError):
        _fail("probe_http_failed")
    finally:
        connection.close()


def read_world(config: Mapping[str, object]) -> dict[str, object]:
    """Make one bounded signed policy read and return its verified world identity."""
    expected_world, endpoint, private_key, reader_public_key, service_verifier, cbor2, extra = _prepare(config)
    blake3, _search_paths = extra
    request = {
        "contract_version": 1,
        "expected_world": expected_world,
        "trust_config_ref": str(config["service_public_key"]).lower(),
        "observation_nonce": secrets.token_hex(32),
    }
    try:
        request_signature = private_key.sign(_canonical_bytes(cbor2, (SIGNING_DOMAIN, PATH, request))).hex()
    except ProbeError:
        raise
    except Exception:
        _fail("probe_reader_sign_failed")
    signed_request = {
        "request": request,
        "subject_public_key": reader_public_key,
        "signature_hex": request_signature,
    }
    try:
        request_signing_bytes = _canonical_bytes(cbor2, (SIGNING_DOMAIN, PATH, signed_request))
        request_digest = "blake3:" + blake3.blake3(request_signing_bytes).hexdigest()
        request_body = json.dumps(signed_request, ensure_ascii=False, separators=(",", ":"), allow_nan=False).encode("utf-8")
    except ProbeError:
        raise
    except Exception:
        _fail("probe_request_encoding_failed")

    raw = _read_response(endpoint, request_body)
    try:
        response = json.loads(raw.decode("utf-8"), object_pairs_hook=_unique_object,
                              parse_constant=lambda _value: (_ for _ in ()).throw(ValueError("invalid JSON number")))
    except (UnicodeDecodeError, json.JSONDecodeError, ValueError, RecursionError):
        _fail("probe_response_invalid")
    if not isinstance(response, dict) or set(response) != {"request_digest", "payload", "signature_hex"}:
        _fail("probe_response_invalid")
    response_digest = response.get("request_digest")
    payload = response.get("payload")
    signature_hex = response.get("signature_hex")
    if response_digest != request_digest or not isinstance(payload, dict) or not isinstance(signature_hex, str) or not _HEX_64.fullmatch(signature_hex):
        _fail("probe_response_invalid")
    try:
        service_verifier.verify(bytes.fromhex(signature_hex), _canonical_bytes(cbor2, (SIGNING_DOMAIN, PATH, (response_digest, payload))))
    except ProbeError:
        raise
    except Exception:
        _fail("probe_service_signature_invalid")

    if payload.get("world_id") != expected_world["world_id"] or payload.get("genesis_digest") != expected_world["genesis_digest"]:
        _fail("probe_world_identity_mismatch")
    if payload.get("available") is not True or payload.get("authority_source") != "dev_local_operator":
        _fail("probe_devlocal_policy_unavailable")
    logical_tick = payload.get("logical_tick")
    if not isinstance(logical_tick, str) or not _DECIMAL_U64.fullmatch(logical_tick):
        _fail("probe_logical_tick_invalid")
    tick = int(logical_tick)
    if tick > MAX_U64:
        _fail("probe_logical_tick_invalid")
    return {
        "world_id": expected_world["world_id"],
        "genesis_digest": expected_world["genesis_digest"],
        "logical_tick": tick,
        "service_public_key": str(config["service_public_key"]).lower(),
        "signature_verified": True,
    }
