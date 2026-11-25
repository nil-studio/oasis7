#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")"

target=${1:-}
case "$target" in
  runtime|build|browser|rust-build|rust-wasm-build|codeql-rust|api-runtime) ;;
  *) echo "usage: $0 runtime|build|browser|rust-build|rust-wasm-build|codeql-rust|api-runtime" >&2; exit 2 ;;
esac

base=ubuntu:24.04
digest=$(docker buildx imagetools inspect "$base" --format "{{json .Manifest}}" | jq -er .digest)
[[ "$digest" =~ ^sha256:[0-9a-f]{64}$ ]]
image="oasis7-ci-${target}:candidate"
node_base=node:24-bookworm-slim
node_digest=$(docker buildx imagetools inspect "$node_base" --format "{{json .Manifest}}" | jq -er .digest)
[[ "$node_digest" =~ ^sha256:[0-9a-f]{64}$ ]] || {
  echo "Invalid Node base digest" >&2
  exit 1
}
options=(--build-arg "NODE_BASE=${node_base}@${node_digest}"
         --label "org.oasis7.ci.node-base.digest=$node_digest")
if [[ "$target" == browser ]]; then
  options+=(--build-context browser-tools=../../scripts/ci-browser-tools)
fi
if [[ "$target" == rust-build || "$target" == rust-wasm-build ]]; then
  workflow=../../.github/workflows/rust.yml
  stable=$(sed -n 's/^  RUST_TOOLCHAIN: //p' "$workflow")
  nightly=$(sed -n 's/^  OASIS7_WASM_TOOLCHAIN: //p' "$workflow")
  wasm_target=$(sed -n 's/^  OASIS7_WASM_TARGET: //p' "$workflow")
  [[ "$stable" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]
  [[ "$nightly" =~ ^nightly-[0-9]{4}-[0-9]{2}-[0-9]{2}$ ]]
  [[ "$wasm_target" == wasm32-unknown-unknown ]]
  options+=(--build-arg "RUST_TOOLCHAIN=$stable" --build-arg "WASM_TOOLCHAIN=$nightly"
            --build-arg "WASM_TARGET=$wasm_target")
elif [[ "$target" == codeql-rust ]]; then
  stable=$(sed -n "s/^      RUSTUP_TOOLCHAIN: '\([0-9]*\.[0-9]*\.[0-9]*\)'$/\1/p" ../../.github/workflows/codeql.yml)
  [[ "$stable" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]
  options+=(--build-arg "CODEQL_RUST_TOOLCHAIN=$stable")
fi
if [[ "$target" == rust-build || "$target" == rust-wasm-build || "$target" == codeql-rust ]]; then
  rust_base="rust:${stable}-bookworm"
  rust_digest=$(docker buildx imagetools inspect "$rust_base" --format "{{json .Manifest}}" | jq -er .digest)
  [[ "$rust_digest" =~ ^sha256:[0-9a-f]{64}$ ]] || {
    echo "Invalid Rust base digest" >&2
    exit 1
  }
  options+=(--build-arg "RUST_BASE=${rust_base}@${rust_digest}"
            --label "org.oasis7.ci.rust-base.digest=$rust_digest")
fi
docker build "${options[@]}" --platform linux/amd64 --target "$target" \
  --build-arg "UBUNTU_BASE=${base}@${digest}" \
  --label "org.opencontainers.image.base.digest=$digest" \
  --label "org.opencontainers.image.revision=$(git rev-parse HEAD)" \
  --label "org.opencontainers.image.source=https://github.com/nil-studio/oasis7" \
  --tag "$image" .
docker run --rm --platform linux/amd64 "$image" bash -ec '
  git --version
  gdb --version
  file --version
  zstd --version
  smoke=$(mktemp -d)
  printf "cache archive smoke\n" >"$smoke/input"
  tar --posix --zstd -cf "$smoke/cache.tar.zst" -C "$smoke" input
  mkdir "$smoke/restored"
  tar --zstd -xf "$smoke/cache.tar.zst" -C "$smoke/restored"
  cmp "$smoke/input" "$smoke/restored/input"
  rm -rf "$smoke"
  ldconfig -p | grep libwayland-client.so.0
  dpkg-query -W
'
# Verify the inherited executable without runner toolcache or network access.
docker run --rm --platform linux/amd64 --network none --user 1001:1001 \
  --env HOME=/tmp "$image" bash -ec '
    test "$(command -v node)" = /usr/local/bin/node
    node --version
    smoke=$(mktemp --suffix=.ts)
    trap "rm -- \"$smoke\"" EXIT
    cat > "$smoke" <<EOF
const expectedMajor: number = 24;
if (Number(process.versions.node.split(".")[0]) !== expectedMajor || process.getuid() !== 1001) {
  process.exit(1);
}
EOF
    node "$smoke"
  '

if [[ "$target" == runtime ]]; then
  docker run --rm --platform linux/amd64 --network none --user 65534:65534 \
    "$image" bash -ec 'jq -e ".ready == true" <<< "{\"ready\":true}" >/dev/null
    if command -v npm >/dev/null; then
      echo "npm belongs to the browser image" >&2
      exit 1
    fi
  '
fi

if [[ "$target" == build ]]; then
  docker run --rm --platform linux/amd64 "$image" bash -ec '
    gcc --version
    pkg-config --exists wayland-client x11 alsa libudev openssl
    ps -o pid= -p "$$"
  '
fi

if [[ "$target" == build || "$target" == rust-build || "$target" == rust-wasm-build ]]; then
  docker run --rm --platform linux/amd64 --network none --user 65534:65534 \
    "$image" bash -ec '
      report=$(mktemp)
      trap "rm -- \"$report\"" EXIT
      LC_ALL=C /usr/bin/time -v -o "$report" /usr/bin/true
      grep -Eq "Maximum resident set size \(kbytes\): [0-9]+$" "$report"
      grep -Eq "Exit status: 0$" "$report"
    '
fi

if [[ "$target" == browser ]]; then
  docker run --rm --platform linux/amd64 --network none --user 65534:65534 \
    --env HOME=/tmp "$image" node -e '
      (async () => {
        const {chromium} = require(process.env.OASIS7_PLAYWRIGHT_MODULE);
        const browser = await chromium.launch({headless: true});
        try {
          const page = await browser.newPage();
          await page.setContent("<title>CI browser smoke</title>");
          if (await page.title() !== "CI browser smoke") throw new Error("Unexpected browser result");
          console.log(browser.version());
        } finally {
          await browser.close();
        }
      })().catch(error => { console.error(error); process.exitCode = 1; });
    '
fi

if [[ "$target" == rust-build || "$target" == rust-wasm-build ]]; then
  docker run --rm --platform linux/amd64 --network none --user 65534:65534 \
    --env HOME=/tmp "$image" bash -ec '
      /usr/local/bin/git --no-lazy-fetch --no-replace-objects --no-pager --no-optional-locks --version
      test -x "$(/usr/local/bin/git --exec-path)/git-remote-https"
      rustc +"$RUST_TOOLCHAIN" -vV
      rustc +"$OASIS7_WASM_TOOLCHAIN" -vV
      cargo +"$RUST_TOOLCHAIN" clippy --version
      rustfmt +"$RUST_TOOLCHAIN" --version
      node -e "if (!process.version.startsWith(\"v24.\")) process.exit(1)"
      jq -e ".ready == true" <<< "{\"ready\":true}" >/dev/null
      python3 - <<PY
import socket
import subprocess

with socket.socket() as listener:
    listener.bind(("127.0.0.1", 0))
    listener.listen()
    port = listener.getsockname()[1]
    subprocess.run(
        ["lsof", "-nP", f"-iTCP:{port}", "-sTCP:LISTEN"],
        check=True,
        stdout=subprocess.DEVNULL,
    )
PY
      test "$(ruby -ryaml -rjson -e "puts JSON.generate(YAML.safe_load(File.read(%q{/dev/stdin})))" <<< "ready: true")" = "{\"ready\":true}"
      rustup component list --installed --toolchain "$OASIS7_WASM_TOOLCHAIN" | grep -Fx rust-src
      printf "pub fn smoke() {}\n" >/tmp/smoke.rs
      for toolchain in "$RUST_TOOLCHAIN" "$OASIS7_WASM_TOOLCHAIN"; do
        rustc +"$toolchain" --target wasm32-unknown-unknown --crate-type lib /tmp/smoke.rs -o /tmp/smoke.rlib
      done
    '
fi

if [[ "$target" == rust-wasm-build ]]; then
  docker run --rm --platform linux/amd64 --network none --user 65534:65534 \
    --env HOME=/tmp "$image" bash -ec '
      clang --print-targets | grep -w wasm32
      printf "int smoke(void) { return 42; }\n" >/tmp/wasm-smoke.c
      clang --target=wasm32-unknown-unknown -c /tmp/wasm-smoke.c -o /tmp/wasm-smoke.o
      llvm-ar rcs /tmp/wasm-smoke.a /tmp/wasm-smoke.o
      test "$(llvm-ar t /tmp/wasm-smoke.a)" = wasm-smoke.o
    '
fi

if [[ "$target" == codeql-rust ]]; then
  repository=$(git rev-parse --show-toplevel)
  docker run --rm --platform linux/amd64 --network none --user 1001:1001 \
    --env HOME=/tmp --env "CODEQL_EXPECTED_RUST=$stable" \
    --mount "type=bind,src=$repository/scripts/security,dst=/codeql-check,readonly" \
    "$image" bash -ec '
      test "$(id -u)" = 1001
      test "$(rustc --version | cut -d" " -f2)" = "$CODEQL_EXPECTED_RUST"
      test "$(cargo --version | cut -d" " -f2)" = "$CODEQL_EXPECTED_RUST"
      test "$(rustup toolchain list | wc -l)" = 1
      scratch=$(mktemp -d)
      trap "rm -rf -- \"$scratch\"" EXIT
      printf "fn main() {}\n" > "$scratch/probe.rs"
      rustc --crate-name probe "$scratch/probe.rs" -o "$scratch/probe"
      "$scratch/probe"
      PYTHONPATH=/codeql-check python3 -B - <<PY
import importlib.util
import subprocess

spec = importlib.util.spec_from_file_location("native", "/codeql-check/codeql-rust-cargo-check.py")
native = importlib.util.module_from_spec(spec)
spec.loader.exec_module(native)
subprocess.run(["dpkg-query", "-W", *native.NATIVE_PACKAGES], check=True)
subprocess.run(["pkg-config", "--print-errors", "--modversion", *native.NATIVE_MODULES], check=True)
PY
    '
  docker image inspect --format 'CodeQL Rust image local bytes: {{.Size}}' "$image"
fi

if [[ "$target" == api-runtime ]]; then
  docker run --rm --platform linux/amd64 --network none --user 1001:1001 \
    --env HOME=/tmp --env GH_PROMPT_DISABLED=1 --env GH_NO_UPDATE_NOTIFIER=1 \
    "$image" bash -ec '
      test "$(id -u)" = 1001
      test "$(command -v gh)" = /usr/local/bin/gh
      test ! -w /usr/local/bin/gh
      version=$(gh --version)
      grep -Eq "^gh version 2[.]105[.]0( |$)" <<< "$version"
      gh api --help >/dev/null
    '
fi
