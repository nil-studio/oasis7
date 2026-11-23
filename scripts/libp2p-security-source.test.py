#!/usr/bin/env python3
"""Verify the fixed network source, consolidated vendor/ shims, and SHA-256 provenance."""
import hashlib
import json
from pathlib import Path
import tomllib
import unittest

ROOT = Path(__file__).resolve().parents[1]
REVISION = "2cb3c04346fdcb0ecf6717728664602378edac13"
URL = "https://github.com/libp2p/rust-libp2p"


class NetworkSourceContract(unittest.TestCase):
    def test_shared_facade_and_lock_sources(self):
        for crate in ("oasis7_net", "oasis7_node"):
            manifest = tomllib.loads((ROOT / "crates" / crate / "Cargo.toml").read_text())
            dep = manifest["dependencies"].get("libp2p") or manifest["target"]['cfg(not(target_arch = "wasm32"))']["dependencies"]["libp2p"]
            self.assertEqual((dep["git"], dep["rev"]), (URL, REVISION))
            self.assertFalse(dep["default-features"])
            self.assertTrue(dep["optional"])
        lock = tomllib.loads((ROOT / "Cargo.lock").read_text())
        for name in ("libp2p", "libp2p-core", "libp2p-rendezvous"):
            packages = [p for p in lock["package"] if p["name"] == name]
            self.assertEqual(len(packages), 1)
            self.assertEqual(packages[0]["source"], f"git+{URL}?rev={REVISION}#{REVISION}")
        root = tomllib.loads((ROOT / "Cargo.toml").read_text())
        self.assertNotIn("libp2p-rendezvous", root["patch"][URL])
        self.assertEqual(list(ROOT.glob("vendor-*")), [])
        self.assertEqual(
            sorted(p.name for p in (ROOT / "vendor").iterdir() if p.is_dir()),
            ["if-watch-3.2.2", "libp2p-quic-0.15.0", "libp2p-swarm-0.49.0"],
        )
        timers = [p for p in lock["package"] if p["name"] == "futures-timer"]
        self.assertTrue(all(tuple(map(int, p["version"].split("."))) >= (3, 0, 4) for p in timers))

    def test_wasm_builder_is_isolated_from_root_workspace_path_patches(self):
        # The WASM builder uses its own standalone workspace under docker/wasm-builder/.
        root = tomllib.loads((ROOT / "Cargo.toml").read_text())
        docker = (ROOT / "docker/wasm-builder/Dockerfile").read_text().splitlines()
        context_policy = (ROOT / ".dockerignore").read_text().splitlines()
        self.assertNotIn("COPY --chmod=0755 Cargo.toml Cargo.lock /opt/oasis7/", docker)
        self.assertNotIn("!Cargo.toml", context_policy)
        self.assertNotIn("!Cargo.lock", context_policy)
        paths = [root["patch"][URL][name]["path"]
                 for name in ("libp2p-swarm", "libp2p-quic")]
        paths.append(root["patch"]["crates-io"]["if-watch"]["path"])
        for patch in paths:
            self.assertTrue((ROOT / patch / "Cargo.toml").is_file())
            self.assertNotIn(f"COPY --chmod=0755 {patch} /opt/oasis7/{patch}", docker)
            self.assertNotIn(f"!{patch}/", context_policy)
            self.assertNotIn(f"!{patch}/Cargo.toml", context_policy)

    def test_if_watch_patch_preserves_sources_and_removes_paste(self):
        vendor = ROOT / "vendor/if-watch-3.2.2"
        provenance = json.loads((vendor / "upstream-source-sha256.json").read_text())
        actual = {str(p.relative_to(vendor)): hashlib.sha256(p.read_bytes()).hexdigest()
                  for p in (vendor / "src").rglob("*.rs")}
        self.assertEqual(actual, provenance["unchanged_source_sha256"])
        self.assertEqual(provenance["crate_archive_sha256"],
                         "71c02a5161c313f0cbdbadc511611893584a10a7b6153cb554bdf83ddce99ec2")
        manifest = tomllib.loads((vendor / "Cargo.toml").read_text())
        deps = manifest["target"]['cfg(target_os = "linux")']["dependencies"]
        expected = {"netlink-sys": "0.9.0", "netlink-packet-core": "0.9.0",
                    "netlink-packet-route": "0.33.0", "netlink-proto": "0.13.0",
                    "rtnetlink": "0.23.0"}
        for name, version in expected.items():
            self.assertEqual(deps[name]["version"], version)
            self.assertFalse(deps[name]["default-features"])
        lock = tomllib.loads((ROOT / "Cargo.lock").read_text())["package"]
        watchers = [p for p in lock if p["name"] == "if-watch"]
        self.assertEqual(len(watchers), 1)
        self.assertNotIn("source", watchers[0])
        self.assertFalse(any(p["name"] == "paste" for p in lock))
        core = [p for p in lock if p["name"] == "netlink-packet-core"]
        self.assertEqual([p["version"] for p in core], ["0.9.0"])

    def test_compat_fixtures_share_only_the_interface_watcher_patch(self):
        for mode in ("legacy", "candidate"):
            fixture = ROOT / "tests/libp2p-compat" / mode
            manifest = tomllib.loads((fixture / "Cargo.toml").read_text())
            self.assertEqual(sorted(manifest["patch"].keys()), ["crates-io"])
            self.assertEqual(manifest["patch"]["crates-io"]["if-watch"]["path"],
                             "../../../vendor/if-watch-3.2.2")
            dep = manifest["dependencies"]["libp2p"]
            packages = tomllib.loads((fixture / "Cargo.lock").read_text())["package"]
            if mode == "legacy":
                self.assertEqual(dep["version"], "=0.56.0")
            else:
                self.assertEqual((dep["git"], dep["rev"]), (URL, REVISION))
                for name in ("libp2p", "libp2p-swarm", "libp2p-quic"):
                    selected = [p for p in packages if p["name"] == name]
                    self.assertEqual(len(selected), 1)
                    self.assertEqual(selected[0]["source"], f"git+{URL}?rev={REVISION}#{REVISION}")
            self.assertFalse(any(p["name"] == "paste" for p in packages))
            watcher = [p for p in packages if p["name"] == "if-watch"]
            self.assertEqual(len(watcher), 1)
            self.assertNotIn("source", watcher[0])

    def test_swarm_snapshot_matches_upstream_sources(self):
        vendor = ROOT / "vendor/libp2p-swarm-0.49.0"
        hashes = json.loads((vendor / "upstream-source-sha256.json").read_text())
        actual = {str(p.relative_to(vendor)): hashlib.sha256(p.read_bytes()).hexdigest()
                  for p in (vendor / "src").rglob("*.rs")}
        self.assertEqual(len(hashes), 31)
        self.assertEqual(actual, hashes)
        self.assertFalse((vendor / "src/transport_attempt.rs").exists())
        self.assertFalse((vendor / "OASIS7-PATCH.md").exists())
        manifest = tomllib.loads((vendor / "Cargo.toml").read_text())
        self.assertIn("workspace", manifest)
        self.assertEqual(manifest["package"]["version"], "0.49.0")
        self.assertEqual(manifest["package"]["metadata"]["oasis7-compat"]["upstream-revision"], REVISION)
        self.assertEqual(manifest["dependencies"]["futures-timer"]["version"], "3.0.4")
        self.assertEqual(manifest["target"]['cfg(target_family="wasm")']["dependencies"]["wasm-bindgen-futures"]["version"], "0.4")
        packages = tomllib.loads((vendor / "Cargo.lock").read_text())["package"]
        self.assertFalse(any(p["name"] == "paste" for p in packages))

    def test_quic_patch_provenance_and_minimal_surface(self):
        vendor = ROOT / "vendor/libp2p-quic-0.15.0"
        upstream = json.loads((vendor / "upstream-source-sha256.json").read_text())
        patched = json.loads((vendor / "local-patch-sha256.json").read_text())
        self.assertTrue(set(upstream).isdisjoint(patched))
        self.assertEqual(set(patched), {"src/connection/connecting.rs", "src/transport.rs"})
        actual = {str(p.relative_to(vendor)): hashlib.sha256(p.read_bytes()).hexdigest()
                  for p in (vendor / "src").rglob("*.rs")}
        self.assertEqual(actual, upstream | patched)
        manifest = tomllib.loads((vendor / "Cargo.toml").read_text())
        self.assertIn("workspace", manifest)
        self.assertEqual(manifest["patch"]["crates-io"]["if-watch"]["path"], "../if-watch-3.2.2")
        self.assertEqual(manifest["package"]["version"], "0.15.0")
        self.assertEqual(manifest["package"]["metadata"]["oasis7-compat"]["upstream-revision"], REVISION)
        for dep_name in ("libp2p-core", "libp2p-tls"):
            dep = manifest["dependencies"][dep_name]
            self.assertEqual((dep["git"], dep["rev"]), (URL, REVISION))
        packages = tomllib.loads((vendor / "Cargo.lock").read_text())["package"]
        self.assertFalse(any(p["name"] == "paste" for p in packages))
        watchers = [p for p in packages if p["name"] == "if-watch"]
        self.assertEqual(len(watchers), 1)
        self.assertNotIn("source", watchers[0])


if __name__ == "__main__":
    unittest.main()

