"""Strict Preview release protocol and RAM-authenticated OSS transport.

Manifests describe compatibility contracts; only host administrator policy authorizes them.
"""
import hashlib
import json
import os
import re
from pathlib import Path
from urllib.request import Request, urlopen
from urllib.parse import urlparse

SHA = re.compile(r"[0-9a-f]{40}")
DIGEST = re.compile(r"[0-9a-f]{64}")
RELEASE = re.compile(r"[0-9a-f]{40}/[1-9][0-9]*-[1-9][0-9]*")


def canonical(value):
    return (json.dumps(value, sort_keys=True, separators=(",", ":")) + "\n").encode()


def _fields(value, fields):
    if not isinstance(value, dict) or set(value) != set(fields.split()):
        raise ValueError("unexpected or missing protocol fields")
    if type(value.get("schema_version")) is not int or value["schema_version"] != 1:
        raise ValueError("unsupported schema version")


def object_key(key, prefix):
    if not isinstance(prefix, str) or not re.fullmatch(r"[A-Za-z0-9_-]+(?:/[A-Za-z0-9_-]+)*", prefix):
        raise ValueError("invalid configured prefix")
    if not isinstance(key, str) or not key.startswith(prefix + "/") or any(p in ("", ".", "..") for p in key.split("/")) or not re.fullmatch(r"[A-Za-z0-9_./-]+", key):
        raise ValueError("object outside configured prefix")
    return key


def validate_manifest(value, prefix):
    _fields(value, "schema_version release_id source_sha archive_key archive_sha256 platform components compatibility")
    if not isinstance(value["source_sha"], str) or not SHA.fullmatch(value["source_sha"]):
        raise ValueError("invalid source SHA")
    if not isinstance(value["release_id"], str) or not RELEASE.fullmatch(value["release_id"]) or not value["release_id"].startswith(value["source_sha"] + "/"):
        raise ValueError("invalid release identity")
    expected = f"{prefix}/releases/{value['release_id']}/bundle.tar.gz"
    if object_key(value["archive_key"], prefix) != expected or not isinstance(value["archive_sha256"], str) or not DIGEST.fullmatch(value["archive_sha256"]):
        raise ValueError("invalid archive identity")
    if value["platform"] != "linux-x64" or not isinstance(value["components"], list) or not value["components"] or any(not isinstance(c, str) or c not in ("chain", "player") for c in value["components"]) or len(set(value["components"])) != len(value["components"]):
        raise ValueError("invalid platform/components")
    c = value["compatibility"]
    if not isinstance(c, dict) or set(c) != {"program_contract", "persistence_contract", "identity_contract"} or any(not isinstance(s, str) or not re.fullmatch(r"[A-Za-z0-9_.:-]{1,128}", s) for s in c.values()):
        raise ValueError("invalid compatibility contracts")
    return value


def validate_channel(value, prefix):
    _fields(value, "schema_version intent sequence release_id manifest_key source_sha rollback")
    if not isinstance(value["intent"], str) or not re.fullmatch(r"[1-9][0-9]*", value["intent"]) or type(value["sequence"]) is not int or value["sequence"] < 1 or type(value["rollback"]) is not bool:
        raise ValueError("invalid release intent")
    if not isinstance(value["release_id"], str) or not RELEASE.fullmatch(value["release_id"]) or not isinstance(value["source_sha"], str) or not SHA.fullmatch(value["source_sha"]) or not value["release_id"].startswith(value["source_sha"] + "/"):
        raise ValueError("invalid release source")
    if object_key(value["manifest_key"], prefix) != f"{prefix}/releases/{value['release_id']}/manifest.json":
        raise ValueError("invalid manifest key")
    return value


class OssStore:
    def __init__(self, endpoint, bucket, prefix, auth=None):
        u = urlparse(endpoint)
        if u.scheme != "https" or not u.hostname or not u.hostname.endswith(".aliyuncs.com") or u.path not in ("", "/") or u.query or u.fragment or u.username or u.password or u.port or not re.fullmatch(r"[a-z0-9][a-z0-9-]{1,61}[a-z0-9]", bucket):
            raise ValueError("invalid OSS endpoint/bucket")
        object_key(prefix + "/probe", prefix)
        import oss2
        if auth is None:
            if os.environ.get("ALIBABA_CLOUD_ACCESS_KEY_ID"):
                # Only STS credentials from the OIDC action; long-lived keys are rejected.
                token = os.environ["ALIBABA_CLOUD_SECURITY_TOKEN"]
                if not token:
                    raise ValueError("short-lived STS token required")
                auth = oss2.StsAuth(os.environ["ALIBABA_CLOUD_ACCESS_KEY_ID"], os.environ["ALIBABA_CLOUD_ACCESS_KEY_SECRET"], token)
            else:
                auth = oss2.ProviderAuth(InstanceRoleProvider())
        self.bucket = oss2.Bucket(auth, endpoint, bucket)
        self.prefix = prefix

    def assert_immutable_bucket(self):
        # OSS explicitly ignores forbid-overwrite for enabled/suspended versioning.
        if self.bucket.get_bucket_versioning().status in ("Enabled", "Suspended"):
            raise ValueError("release bucket versioning must never have been enabled")

    def get_with_etag(self, key):
        result = self.bucket.get_object(object_key(key, self.prefix))
        try:
            data = result.read(1024 * 1024 + 1)
            if len(data) > 1024 * 1024:
                raise ValueError("protocol object exceeds 1 MiB")
            return data, result.etag
        finally:
            result.close()

    def get(self, key):
        return self.get_with_etag(key)[0]

    def download_to(self, key, path, max_bytes):
        result = self.bucket.get_object(object_key(key, self.prefix))
        total = 0
        try:
            with open(path, "xb") as output:
                while True:
                    block = result.read(1024 * 1024)
                    if not block:
                        break
                    total += len(block)
                    if total > max_bytes:
                        raise ValueError("OSS object exceeds configured download limit")
                    output.write(block)
        finally:
            result.close()
        return total

    def verify_archive(self, key, expected):
        result = self.bucket.get_object(object_key(key, self.prefix))
        digest = hashlib.sha256()
        try:
            while True:
                block = result.read(1024 * 1024)
                if not block:
                    break
                digest.update(block)
        finally:
            result.close()
        if digest.hexdigest() != expected:
            raise ValueError("uploaded archive verification failed")

    def put_immutable_file(self, key, path, expected):
        try:
            with open(path, "rb") as data:
                self.bucket.put_object(object_key(key, self.prefix), data, headers={"x-oss-forbid-overwrite": "true"})
        except Exception as exc:
            if getattr(exc, "status", None) != 409:
                raise
        self.verify_archive(key, expected)

    def put_immutable(self, key, data):
        try:
            self.bucket.put_object(object_key(key, self.prefix), data, headers={"x-oss-forbid-overwrite": "true"})
        except Exception as exc:
            if getattr(exc, "status", None) != 409:
                raise
        if self.get(key) != data:
            raise ValueError("uploaded object verification failed")

    def put_channel(self, key, data, etag=None):
        # OSS PutObject has no destination If-Match operation. The publisher role
        # has exactly one workflow writer, serialized with job concurrency. This
        # last read catches unexpected external writes but is not a claimed CAS.
        try:
            _, actual = self.get_with_etag(key)
        except Exception as exc:
            if getattr(exc, "status", None) != 404:
                raise
            actual = None
        if actual != etag:
            raise ValueError("channel changed during serialized publication")
        self.bucket.put_object(object_key(key, self.prefix), data)


class InstanceRoleProvider:
    """Fetch the attached ECS role through IMDSv2; never read local key files."""
    def get_credentials(self):
        from oss2.credentials import Credentials
        base = "http://100.100.100.200/latest/"
        with urlopen(Request(base + "api/token", method="PUT", headers={"X-aliyun-ecs-metadata-token-ttl-seconds": "60"}), timeout=5) as response:
            token = response.read(4096).decode()
        headers = {"X-aliyun-ecs-metadata-token": token}
        with urlopen(Request(base + "meta-data/ram/security-credentials/", headers=headers), timeout=5) as response:
            role = response.read(4096).decode().strip()
        if not re.fullmatch(r"[A-Za-z0-9_-]{1,128}", role):
            raise ValueError("expected exactly one attached ECS RAM role")
        with urlopen(Request(base + "meta-data/ram/security-credentials/" + role, headers=headers), timeout=5) as response:
            value = json.load(response)
        if value.get("Code") != "Success":
            raise ValueError("ECS RAM credential acquisition failed")
        return Credentials(value["AccessKeyId"], value["AccessKeySecret"], value["SecurityToken"])


def sha256_file(path):
    digest = hashlib.sha256()
    with open(path, "rb") as stream:
        while True:
            block = stream.read(1024 * 1024)
            if not block:
                break
            digest.update(block)
    return digest.hexdigest()


def publish(store, manifest, archive, intent, sequence, is_ancestor, rollback=False):
    prefix = store.prefix
    validate_manifest(manifest, prefix)
    key = prefix + "/channels/preview.json"
    try:
        raw, etag = store.get_with_etag(key)
    except Exception as exc:
        if getattr(exc, "status", None) != 404:
            raise
        current, etag = None, None
    else:
        current = validate_channel(json.loads(raw), prefix)
    if current:
        if sequence == current["sequence"] and intent == current["intent"] and manifest["release_id"] == current["release_id"] and rollback == current["rollback"]:
            # A successful re-run of the exact same intent is already complete.
            return current
        if sequence <= current["sequence"]:
            raise ValueError("stale or repeated publication intent")
        if not rollback and not is_ancestor(current["source_sha"], manifest["source_sha"]):
            raise ValueError("normal publication would reverse/diverge source ancestry")
    manifest_key = f"{prefix}/releases/{manifest['release_id']}/manifest.json"
    if archive is not None:
        digest = sha256_file(archive) if isinstance(archive, Path) else hashlib.sha256(archive).hexdigest()
        if digest != manifest["archive_sha256"]:
            raise ValueError("archive digest mismatch")
        if isinstance(archive, Path):
            store.put_immutable_file(manifest["archive_key"], archive, digest)
        else:
            store.put_immutable(manifest["archive_key"], archive)
        store.put_immutable(manifest["archive_key"] + ".sha256", (manifest["archive_sha256"] + "  bundle.tar.gz\n").encode())
        store.put_immutable(manifest_key, canonical(manifest))
    else:
        if store.get(manifest_key) != canonical(manifest):
            raise ValueError("retained release verification failed")
        store.verify_archive(manifest["archive_key"], manifest["archive_sha256"])
    channel = validate_channel(dict(schema_version=1, intent=intent, sequence=sequence, release_id=manifest["release_id"], manifest_key=manifest_key, source_sha=manifest["source_sha"], rollback=rollback), prefix)
    store.put_channel(key, canonical(channel), etag)
    return channel
