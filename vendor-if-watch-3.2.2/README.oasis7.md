# Oasis7 if-watch dependency patch

This directory retains the published `if-watch` 3.2.2 Rust sources and upgrades
its Linux netlink dependency requirements in `Cargo.toml`:

| Dependency | Published requirement | Patched requirement |
| --- | --- | --- |
| netlink-sys | 0.8.8 | 0.9.0 |
| netlink-packet-core | 0.8.1 | 0.9.0 |
| netlink-packet-route | 0.28.0 | 0.33.0 |
| netlink-proto | 0.12.0 | 0.13.0 |
| rtnetlink | 0.20.0 | 0.23.0 |

`netlink-packet-core` 0.9 removes the `buffer!` macro and the unmaintained
`paste` dependency. Upgrading the netlink family together preserves a single
set of compatible packet types; changing only packet-core would mix
incompatible crate versions. The public if-watch API and Rust sources stay
unchanged, so the pinned libp2p revision and its existing swarm/rendezvous
patches remain in use.

The same manifest-only dependency upgrade was proposed in upstream
[if-watch PR #55](https://github.com/libp2p/if-watch/pull/55), at immutable head
`b30a9c1a43a1553710ca247fa5707d28ab9dd179`. That proposal was unmerged and had
no maintainer review when this patch was selected; it is a reference for the
change, not an upstream approval. Oasis7 maintains this local patch from the
published crate rather than adding the contributor fork as a Git dependency.
An empty `[workspace]` table keeps standalone vendor tests isolated when a task
worktree is nested inside the primary checkout. `Cargo.toml.orig` preserves
the published manifest. The crate archive identity,
upstream revision and unchanged Rust source hashes are recorded in
`upstream-source-sha256.json`. The upstream declared license remains
`MIT OR Apache-2.0`.

Validate the Linux implementation with
`env -u RUSTC_WRAPPER cargo check --locked --manifest-path ./vendor-if-watch-3.2.2/Cargo.toml --target-dir ./target --target x86_64-unknown-linux-gnu --all-features`.
On Linux, run
`env -u RUSTC_WRAPPER cargo test --locked --manifest-path ./vendor-if-watch-3.2.2/Cargo.toml --target-dir ./target --all-features --lib`
to exercise actual interface enumeration. Restrict the standalone run to unit
tests: the upstream cdylib emits an un-hashed `libif_watch.rlib`, which otherwise
collides with the root workspace dependency build when sharing `target/`. These
runtime tests cannot be replaced by cross-compilation on macOS. Existing
`oasis7_net` TCP/QUIC tests continue to cover the libp2p integration.

Remove this patch when a maintained if-watch release carries compatible
paste-free netlink dependencies; update the lockfile and verify both Linux
interface watching and libp2p TCP/QUIC integration at that time.
