# libp2p swarm manifest compatibility shim

Source: `libp2p/rust-libp2p` revision `2cb3c04346fdcb0ecf6717728664602378edac13`, `swarm` package `0.49.0`.
All `.rs` files under `src/` are 100% byte-identical to upstream `2cb3c04346fdcb0ecf6717728664602378edac13` (verified by `scripts/libp2p-security-source.test.py` against `upstream-source-sha256.json`).

The sole change in `Cargo.toml` relaxes `wasm-bindgen-futures` from `"0.4.80"` (`>=0.4.80`) to `"0.4"` so Cargo's workspace lockfile resolver can unify it with `oasis7_custody_worker` / `oasis7_dev_test_worker`'s Cloudflare `worker = "=0.7.5"` platform pin (`wasm-bindgen = "=0.2.127"`, `wasm-bindgen-futures = "=0.4.77"`). Remove this directory once the workspace WASM platform lock advances to `wasm-bindgen-futures >= 0.4.80`.
