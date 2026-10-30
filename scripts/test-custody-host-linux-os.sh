#!/usr/bin/env bash
# Root-only execution recipe: canonical Linux artifacts, then disposable UID fixture.
set -euo pipefail
if [[ $# != 4 || "$1" != --image || "$3" != --host-umask || ! "$4" =~ ^(022|077)$ ]]; then
  echo 'usage: test-custody-host-linux-os.sh --image <preloaded-linux-build-image@sha256:digest> --host-umask 022|077' >&2
  exit 2
fi
image="$2"; mask="$4"
[[ "$image" =~ (^sha256:[0-9a-f]{64}$|@sha256:[0-9a-f]{64}$) ]] || { echo 'immutable image digest required' >&2; exit 2; }
repo=$(rtk proxy git rev-parse --show-toplevel)
common=$(rtk proxy git rev-parse --path-format=absolute --git-common-dir)
arch=$(rtk proxy docker image inspect --format '{{.Architecture}}' "$image")
[[ $(rtk proxy docker image inspect --format '{{.Os}}' "$image") == linux ]]
case "$arch" in arm64) target=aarch64-unknown-linux-gnu ;; amd64) target=x86_64-unknown-linux-gnu ;; *) echo 'unsupported fixture Linux architecture' >&2; exit 2 ;; esac
linux_cargo_cache="$repo/.tmp/custody-linux-cargo-5076"
rtk proxy mkdir -p "$repo/target" "$linux_cargo_cache"
rtk proxy chmod 700 "$linux_cargo_cache"
pack=$(rtk proxy mktemp -d "$repo/target/custody-host-linux-os.XXXXXX")
# Explicit Linux triple shares the real worktree target; this is not a lock bypass.
# The image must already contain pinned Rust1.96.0, linker/deps, and Python3.
# Qualify canonical deployment-profile artifacts; retain prior debug failure evidence.
mounts=(--mount "type=bind,src=$repo,dst=$repo,readonly" --mount "type=bind,src=$repo/target,dst=$repo/target" --mount "type=bind,src=$common,dst=$common,readonly" --mount "type=bind,src=$linux_cargo_cache,dst=/tmp/oasis7-linux-cargo")
# Keep Linux dependency cache paths/mtimes stable across these builds. This cache
# is build convenience, never an identity/approval authority or runtime mount.
# Operator UID, worktree target/lock, image/source pins and Cargo flags stay fixed.
build_uid=$(rtk proxy id -u); build_gid=$(rtk proxy id -g)
rtk proxy docker run --rm "${mounts[@]}" --user "$build_uid:$build_gid" --tmpfs /tmp:rw,nosuid,nodev,mode=1777 \
  --env CARGO_HOME=/tmp/oasis7-linux-cargo --env CARGO_BUILD_JOBS=1 --env GIT_CONFIG_COUNT=1 \
  --env GIT_CONFIG_KEY_0=safe.directory --env "GIT_CONFIG_VALUE_0=$repo" \
  --workdir "$repo" --entrypoint python3 "$image" -c '
import os,subprocess,sys
log,target=sys.argv[1:]
env=os.environ.copy();env.pop("RUSTC_WRAPPER",None)
with open(log,"w") as output:
    subprocess.run(["env","-u","RUSTC_WRAPPER","cargo","test","--release","--locked","-p","oasis7_game_client","--lib","--target",target,"--no-run","--message-format=json"],env=env,stdout=output,check=True)
subprocess.run(["env","-u","RUSTC_WRAPPER","cargo","build","--release","--locked","-p","oasis7_game_client","--bin","oasis7_game_client","--target",target],env=env,check=True)
' "$pack/build.jsonl" "$target"
rtk proxy python3 - "$repo" "$target" "$pack" <<'PY_ARTIFACT'
import json,pathlib,shutil,sys
repo,target,pack=map(pathlib.Path,sys.argv[1:])
records=[]
for line in (pack/'build.jsonl').read_text().splitlines():
    try:r=json.loads(line)
    except ValueError:continue
    if r.get('reason')=='compiler-artifact' and r.get('target',{}).get('name')=='oasis7_game_client' and r.get('profile',{}).get('test') and r.get('executable'):records.append(r['executable'])
assert len(records)==1,'one canonical unit-test executable required'
# Packaging contains public compiled artifacts only, never vault/input secrets.
# Runtime root intentionally lacks DAC override; H/M/N must traverse this mount.
pack.chmod(0o755)
for source,name in [(pathlib.Path(records[0]),'host-tests'),(repo/'target'/target/'release/oasis7_game_client','game-client'),(repo/'scripts/fixtures/custody-host-linux-os.py','custody-host-linux-os.py')]:
    shutil.copyfile(source,pack/name);(pack/name).chmod(0o555)
PY_ARTIFACT
printf 'linux_os_artifact_directory=%s\n' "$pack" >&2
# No host accounts, privileged mode, host PID/IPC/network, or Docker socket.
# Supervisor root gets only setup/retirement caps; every H/M/N child drops all.
rtk proxy docker run --rm --network none --read-only --pids-limit 64 --memory 512m --cpus 2 --security-opt no-new-privileges   --cap-drop ALL --cap-add CHOWN --cap-add SETUID --cap-add SETGID --cap-add SETPCAP --cap-add KILL   --tmpfs /run:rw,nosuid,nodev,noexec,size=32m,mode=0755   --tmpfs /var/lib/oasis7-custody:rw,nosuid,nodev,noexec,size=4m,mode=0755   --mount "type=bind,src=$pack,dst=/artifacts,readonly" --entrypoint python3 "$image" /artifacts/custody-host-linux-os.py --host-umask "$mask"
