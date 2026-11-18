#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")"

target=${1:-}
case "$target" in
  runtime|build) ;;
  *) echo "usage: $0 runtime|build" >&2; exit 2 ;;
esac

base=ubuntu:24.04
digest=$(docker buildx imagetools inspect "$base" --format "{{json .Manifest}}" | jq -er .digest)
[[ "$digest" =~ ^sha256:[0-9a-f]{64}$ ]]
image="oasis7-ci-${target}:candidate"
docker build --platform linux/amd64 --target "$target" \
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
