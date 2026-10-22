#!/usr/bin/env python3
"""Two local processes verify old/new transports, application protocols and identity."""
import os
from pathlib import Path
import selectors
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
env = dict(os.environ)
env.pop("RUSTC_WRAPPER", None)
# Exercise the shared protocol once, within the legacy graph's already locked dependencies.
subprocess.run(["cargo", "test", "--manifest-path",
                str(ROOT / "tests/libp2p-compat/legacy/Cargo.toml"), "--locked",
                "-p", "oasis7-libp2p-compat-protocol"], env=env, check=True)
binaries = {}
for mode in ("legacy", "candidate"):
    fixture = ROOT / "tests/libp2p-compat" / mode
    subprocess.run(["cargo", "build", "--manifest-path", str(fixture / "Cargo.toml"), "--locked"], env=env, check=True)
    binaries[mode] = fixture / "target/debug/peer"

def output(binary, *args):
    return subprocess.check_output([str(binary), *args], timeout=30, text=True).strip()

assert output(binaries["legacy"], "identity") == output(binaries["candidate"], "identity")
addresses = ["/ip4/127.0.0.1/tcp/4103", "/ip4/127.0.0.1/udp/4103/quic-v1",
             "/dns4/localhost/tcp/4103", "/ip6/::1/tcp/4103"]
assert output(binaries["legacy"], "address", *addresses) == output(binaries["candidate"], "address", *addresses)
for server_mode, client_mode in (("legacy", "candidate"), ("candidate", "legacy")):
    for transport in ("tcp", "quic"):
        with tempfile.TemporaryFile() as errors:
            server = subprocess.Popen([str(binaries[server_mode]), "server", transport], stdout=subprocess.PIPE, stderr=errors, text=True)
            try:
                with selectors.DefaultSelector() as selector:
                    selector.register(server.stdout, selectors.EVENT_READ)
                    assert selector.select(15), "server did not publish local listener"
                ready = server.stdout.readline().strip()
                assert ready.startswith("READY "), ready
                assert output(binaries[client_mode], "client", ready[6:]) == "PASS rpc gossip"
                print(f"ok: {server_mode} -> {client_mode}: {transport}, RPC and gossip")
            finally:
                server.terminate()
                try: server.wait(timeout=5)
                except subprocess.TimeoutExpired: server.kill(); server.wait()
