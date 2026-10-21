import copy
import importlib.util
from pathlib import Path
import unittest
import tempfile

SPEC = importlib.util.spec_from_file_location("peer", Path(__file__).with_name("codeql-rust-peer-inputs.py"))
P = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(P)


def metadata(root_id, manifest, workspace):
    return {"packages": [{"id": root_id, "name": "peer", "manifest_path": manifest,
                          "targets": [{"src_path": "/snapshot/tests/libp2p-compat/peer.rs"}],
                          "features": {}, "dependencies": [{"name": "libp2p", "source": "git+exact", "features": ["macros"]}]},
                         {"id": "git+exact#libp2p", "name": "libp2p", "manifest_path": "/owned/cargo/libp2p/Cargo.toml"}],
            "workspace_members": [root_id], "workspace_default_members": [root_id],
            "workspace_root": workspace, "target_directory": workspace + "/target",
            "resolve": {"root": root_id, "nodes": [{"id": root_id, "dependencies": ["git+exact#libp2p"],
                         "deps": [{"name": "libp2p", "pkg": "git+exact#libp2p", "dep_kinds": [{"kind": None, "target": None}]}], "features": []}]}}


class ProjectionMetadata(unittest.TestCase):
    def test_only_root_identity_and_relocated_fields_are_aliased(self):
        a = metadata("path+candidate#peer", "/candidate/Cargo.toml", "/candidate")
        b = metadata("path+parent#peer", "/parent/Cargo.toml", "/parent")
        self.assertEqual(P.normalized_metadata(a), P.normalized_metadata(b))

    def test_complete_requires_actual_resolve_graph(self):
        packet = metadata("root", "/root/Cargo.toml", "/root")
        packet["resolve"] = None
        with self.assertRaisesRegex(RuntimeError, "real resolved root"):
            P.verify_metadata(packet, packet, Path("/root"), "candidate", True)

    def test_build_directory_must_follow_target_directory(self):
        packet = metadata("root", "/root/Cargo.toml", "/root")
        packet["build_directory"] = "/foreign/build"
        with self.assertRaisesRegex(RuntimeError, "independent Cargo build"):
            P.normalized_metadata(packet)

    def test_only_original_peer_physical_target_is_canonicalized(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            (root / "candidate").mkdir()
            peer = root / "peer.rs"
            peer.write_text("fn main() {}")
            packet = metadata("root", "/root/Cargo.toml", "/root")
            packet["packages"][0]["targets"] = [{"kind": ["bin"], "name": "peer", "src_path": str(root / "candidate/../peer.rs")}]
            normalized = P.normalized_metadata(packet, peer)
            self.assertEqual(normalized["packages"][0]["targets"][0]["src_path"], str(peer.resolve()))
            wrapper = root / "wrapper.rs"
            wrapper.write_text(peer.read_text())
            packet["packages"][0]["targets"][0]["src_path"] = str(wrapper)
            with self.assertRaisesRegex(RuntimeError, "same original"):
                P.normalized_metadata(packet, peer)

    def test_dependency_source_features_and_edges_cannot_be_aliased(self):
        base = metadata("path+candidate#peer", "/candidate/Cargo.toml", "/candidate")
        for mutation in ("source", "version", "features", "edge", "target"):
            changed = copy.deepcopy(base)
            if mutation == "source":
                changed["packages"][0]["dependencies"][0]["source"] = "registry+different"
            elif mutation == "version":
                changed["packages"][1]["version"] = "999.0.0"
            elif mutation == "features":
                changed["packages"][0]["dependencies"][0]["features"] = []
            elif mutation == "edge":
                changed["resolve"]["nodes"][0]["deps"][0]["dep_kinds"][0]["target"] = "cfg(windows)"
            else:
                changed["packages"][0]["targets"][0]["src_path"] = "/generated/wrapper.rs"
            self.assertNotEqual(P.normalized_metadata(base), P.normalized_metadata(changed), mutation)

class RealSourceModelControls(unittest.TestCase):
    def tables(self, root):
        import csv
        contexts = {kind: {"libp2p": {"root": f"/owned/{kind}/libp2p"}} for kind in ("candidate", "legacy")}
        for kind in ("baseline", "candidate", "legacy"):
            macros = [["src/lib.rs", "3", "oracle", "4", "1"], ["orphan.rs", "1", "oracle", "0", "0"],
                      [P.PEER, "22", "assert", "0" if kind == "baseline" else "20", "0" if kind == "baseline" else "4"]]
            calls = [] if kind == "baseline" else [[P.PEER, "47", "with_existing_identity", f"/owned/{kind}/libp2p/src/builder.rs"]]
            data = {"Macros": macros, "Calls": calls, "Derives": [[P.PEER, "10", "1", "20", "4"]], "Flow": [["run-args-to-line45-path", "1"]]}
            for query, rows in data.items():
                with (root / f"{kind}-{query}.csv").open("w") as stream:
                    writer = csv.writer(stream); writer.writerow(["header"]); writer.writerows(rows)
        return contexts

    def test_distinct_original_source_contexts(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp); contexts = self.tables(root)
            P.check_results(root, contexts)

    def test_wrapper_or_other_dependency_origin_is_rejected(self):
        for replacement in ("generated-wrapper.rs", "/foreign/libp2p"):
            with tempfile.TemporaryDirectory() as temp:
                root = Path(temp); contexts = self.tables(root)
                path = root / "candidate-Calls.csv"
                if replacement == "generated-wrapper.rs":
                    path.write_text(path.read_text().replace(P.PEER, replacement))
                else:
                    path.write_text(path.read_text().replace("/owned/candidate/libp2p", replacement))
                with self.assertRaisesRegex(RuntimeError, "target provenance"):
                    P.check_results(root, contexts)

    def test_missing_derive_or_exact_flow_is_rejected(self):
        for query in ("Derives", "Flow"):
            with tempfile.TemporaryDirectory() as temp:
                root = Path(temp); contexts = self.tables(root)
                path = root / f"candidate-{query}.csv"
                path.write_text("header\n" + (f"{P.PEER},10,0,0,0\n" if query == "Derives" else "run-args-to-line45-path,0\n"))
                with self.assertRaises(RuntimeError):
                    P.check_results(root, contexts)


if __name__ == "__main__":
    unittest.main()
