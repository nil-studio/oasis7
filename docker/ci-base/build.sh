#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")"

target=${1:-}
case "$target" in
  runtime|build|browser) ;;
  *) echo "usage: $0 runtime|build|browser" >&2; exit 2 ;;
esac

base=ubuntu:24.04
digest=$(docker buildx imagetools inspect "$base" --format "{{json .Manifest}}" | jq -er .digest)
[[ "$digest" =~ ^sha256:[0-9a-f]{64}$ ]]
image="oasis7-ci-${target}:candidate"
options=()
if [[ "$target" == browser ]]; then
  node_base=node:24-bookworm-slim
  node_digest=$(docker buildx imagetools inspect "$node_base" --format "{{json .Manifest}}" | jq -er .digest)
  [[ "$node_digest" =~ ^sha256:[0-9a-f]{64}$ ]]
  options+=(--build-arg "NODE_BASE=${node_base}@${node_digest}"
            --build-context browser-tools=../../scripts/ci-browser-tools
            --label "org.oasis7.ci.node-base.digest=$node_digest")
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
  ldconfig -p | grep libwayland-client.so.0
  dpkg-query -W
'
if [[ "$target" == build ]]; then
  docker run --rm --platform linux/amd64 "$image" bash -ec '
    gcc --version
    pkg-config --exists wayland-client x11 alsa libudev
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
