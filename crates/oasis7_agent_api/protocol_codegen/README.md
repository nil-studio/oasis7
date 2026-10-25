# Signed-read bootstrap protocol generation

This development tool compiles the registered HTTP OpenAPI profile. It is not part
of Cargo builds or request handling. The manual OpenAPI remains the authority;
no Rust DTO, JavaScript signer, World Service numeric-wire codec, or Viewer profile
is generated. There is no previous TypeScript consumer version in this initial
baseline.

Run from any working directory with the Python 3.12 interpreter recorded in
`toolchain.lock.json` (stdlib only):

```sh
python3.12 crates/oasis7_agent_api/protocol_codegen/generate.py write
python3.12 crates/oasis7_agent_api/protocol_codegen/generate.py check
python3.12 -m unittest discover -s crates/oasis7_agent_api/protocol_codegen -p 'test_*.py'
```

`profile.json` registers the sole source, explicit managed output paths, exported
roots, target/features and finite limits. The manifest hashes every generation
input: the source, profile, toolchain lock and compiler. Test/consumer files are
not generation inputs. Generated schema `$defs` retains the source constraints;
`x-operations` maps each actual GET operation to its path and response schemas.
The supported schema subset uses an explicit ECMAScript regex grammar and
rejects recursive component graphs and unsupported constructs. Type expressions
have a cumulative byte budget before publication. Generated TypeScript types
require runtime validation for ranges and patterns.
Dynamic object contents remain unknown. Numeric u64 fields inside the opaque
proof retain their original schema bounds and do not gain a lossless JS codec.

`check` does not create directories or files. It rejects missing, stale, extra,
invalid or mixed artifacts. Consumers must run it before packaging. Diagnostics
identify the affected file; input or structural failures use `invalid:`. `write`
stages the complete set, validates schema semantics and references, holds an
exclusive writer lock, rechecks input/output preimages, and renames the manifest
last. The renames are not an atomic publication: a crash can leave a mixed set
that check rejects. Inspect an interrupted writer before deleting
`.generate.lock`, then restore the complete set with Git or regenerate it. Extra
files and symlinks are never removed automatically. A timeout terminates the
worker and can leave the lock as evidence requiring inspection.
