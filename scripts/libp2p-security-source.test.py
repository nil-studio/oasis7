#!/usr/bin/env python3
"""Verify the fixed network source and byte-identical swarm compatibility snapshot."""
import hashlib
import json
from pathlib import Path
import tomllib
import unittest

ROOT = Path(__file__).resolve().parents[1]
REVISION = "2d8497b2615086bd88018ef3a1fb32bc613a07ca"
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
        for name in ("libp2p", "libp2p-core"):
            packages = [p for p in lock["package"] if p["name"] == name]
            self.assertEqual(len(packages), 1)
            self.assertEqual(packages[0]["source"], f"git+{URL}?rev={REVISION}#{REVISION}")
        rendezvous = [p for p in lock["package"] if p["name"] == "libp2p-rendezvous"]
        self.assertEqual(len(rendezvous), 1)
        self.assertEqual(rendezvous[0]["version"], "0.18.1")
        self.assertNotIn("source", rendezvous[0])
        root = tomllib.loads((ROOT / "Cargo.toml").read_text())
        self.assertEqual(root["patch"][URL]["libp2p-rendezvous"]["path"],
                         "vendor-libp2p-rendezvous-0.18.1")
        timers = [p for p in lock["package"] if p["name"] == "futures-timer"]
        self.assertTrue(all(tuple(map(int, p["version"].split("."))) >= (3, 0, 4) for p in timers))

    def test_wasm_builder_includes_workspace_path_patch(self):
        # cargo install resolves root patches even for a non-network tool.
        root = tomllib.loads((ROOT / "Cargo.toml").read_text())
        docker = (ROOT / "docker/wasm-builder/Dockerfile").read_text().splitlines()
        context_policy = (ROOT / ".dockerignore").read_text().splitlines()
        paths = [root["patch"][URL][name]["path"]
                 for name in ("libp2p-swarm", "libp2p-rendezvous")]
        paths.append(root["patch"]["crates-io"]["if-watch"]["path"])
        for patch in paths:
            self.assertIn(f"COPY {patch} /opt/oasis7/{patch}", docker)
            self.assertTrue((ROOT / patch / "Cargo.toml").is_file())
            self.assertIn(f"!{patch}/", context_policy)
            self.assertIn(f"!{patch}/**", context_policy)

    def test_if_watch_patch_preserves_sources_and_removes_paste(self):
        vendor = ROOT / "vendor-if-watch-3.2.2"
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
            self.assertEqual(manifest["patch"]["crates-io"]["if-watch"]["path"],
                             "../../../vendor-if-watch-3.2.2")
            dep = manifest["dependencies"]["libp2p"]
            if mode == "legacy":
                self.assertEqual(dep["version"], "=0.56.0")
            else:
                self.assertEqual((dep["git"], dep["rev"]), (URL, REVISION))
            packages = tomllib.loads((fixture / "Cargo.lock").read_text())["package"]
            self.assertFalse(any(p["name"] == "paste" for p in packages))
            watcher = [p for p in packages if p["name"] == "if-watch"]
            self.assertEqual(len(watcher), 1)
            self.assertNotIn("source", watcher[0])

    def test_rendezvous_has_only_the_reviewed_client_patch(self):
        vendor = ROOT / "vendor-libp2p-rendezvous-0.18.1"
        upstream = json.loads((vendor / "upstream-source-sha256.json").read_text())
        patch = json.loads((vendor / "local-patch-sha256.json").read_text())
        self.assertEqual(set(patch), {"src/client.rs", "src/client/tests.rs"})
        actual = {str(p.relative_to(vendor)): hashlib.sha256(p.read_bytes()).hexdigest()
                  for p in (vendor / "src").rglob("*") if p.is_file()}
        actual["LICENSE"] = hashlib.sha256((vendor / "LICENSE").read_bytes()).hexdigest()
        self.assertEqual(actual, upstream | patch)
        self.assertFalse((vendor / "build.rs").exists())
        self.assertEqual(hashlib.sha256((vendor / "Cargo.toml").read_bytes()).hexdigest(),
                         "7981199de037abcb15b66bbad8c709a7908d9c5d7f265a81a829b3b49d41bd98")
        manifest = tomllib.loads((vendor / "Cargo.toml").read_text())
        self.assertEqual(manifest["package"]["metadata"]["oasis7-compat"]["upstream-revision"], REVISION)
        for name in ("libp2p-core", "libp2p-swarm", "libp2p-request-response", "prost-codec"):
            self.assertEqual((manifest["dependencies"][name]["git"],
                              manifest["dependencies"][name]["rev"]), (URL, REVISION))
        self.assertEqual(manifest["patch"][URL]["libp2p-swarm"]["path"],
                         "../vendor-libp2p-swarm-0.48.0")

    def test_swarm_snapshot_has_no_source_changes(self):
        vendor = ROOT / "vendor-libp2p-swarm-0.48.0"
        hashes = json.loads((vendor / "upstream-source-sha256.json").read_text())
        actual = {str(p.relative_to(vendor)): hashlib.sha256(p.read_bytes()).hexdigest()
                  for p in (vendor / "src").rglob("*.rs")}
        self.assertEqual(actual, hashes)
        manifest = tomllib.loads((vendor / "Cargo.toml").read_text())
        self.assertEqual(manifest["package"]["metadata"]["oasis7-compat"]["upstream-revision"], REVISION)
        self.assertEqual(manifest["dependencies"]["futures-timer"]["version"], "3.0.4")
        self.assertEqual(manifest["target"]['cfg(target_family="wasm")']["dependencies"]["wasm-bindgen-futures"]["version"], "0.4")


if __name__ == "__main__":
    unittest.main()
