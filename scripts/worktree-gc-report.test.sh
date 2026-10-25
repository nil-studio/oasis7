#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
REPO="$TMP/repo with spaces"
mkdir -p "$REPO/scripts" "$TMP/bin"
cp "$ROOT/scripts/"{worktree-gc-report.sh,worktree-harness-lib.sh} "$REPO/scripts/"
git -C "$REPO" init -qb main
git -C "$REPO" config user.email test@example.com
git -C "$REPO" config user.name test
printf 'target\nsecret\n.pm/\n' >"$REPO/.gitignore"
git -C "$REPO" add .
git -C "$REPO" commit -qm initial
git -C "$REPO" update-ref refs/remotes/origin/main HEAD
git -C "$REPO" worktree add -qb codex/facts "$TMP/topic"
NEWLINE_PATH="$TMP/line
break"
git -C "$REPO" worktree add -qb codex/newline "$NEWLINE_PATH"
git -C "$REPO" worktree add -qb codex/missing "$TMP/missing"
rm -rf "$TMP/missing"
git -C "$REPO" worktree lock "$TMP/topic" --reason test
printf 'user' >"$TMP/topic/untracked"
printf 'private' >"$TMP/topic/secret"
mkdir "$TMP/cache"
printf 'build' >"$TMP/cache/data"
ln -s "$TMP/cache" "$TMP/topic/target"
cat >"$TMP/bin/gh" <<'GH'
#!/usr/bin/env bash
echo 'gh must never be invoked' >&2
exit 99
GH
chmod +x "$TMP/bin/gh"
PATH="$TMP/bin:$PATH" "$REPO/scripts/worktree-gc-report.sh" --json >"$TMP/before"
mkdir -p "$REPO/.pm/tasks"
printf 'status: done\nworktree_hint: %s\n' "$TMP/topic" >"$REPO/.pm/tasks/task_old.yaml"
PATH="$TMP/bin:$PATH" "$REPO/scripts/worktree-gc-report.sh" --json >"$TMP/after"

"$REPO/scripts/worktree-gc-report.sh" --json --footprint >"$TMP/footprint"
"$REPO/scripts/worktree-gc-report.sh" >"$TMP/human"
if "$REPO/scripts/worktree-gc-report.sh" --prunable-only >/dev/null 2>&1; then exit 1; fi
python3 - "$TMP" <<'CHECK'
import json,sys
from pathlib import Path
root=Path(sys.argv[1])
p=json.loads((root/'after').read_text())
e={Path(v['path']).name:v for v in p['entries']}
newline=next(v for v in p['entries'] if v['branch']=='codex/newline')
assert newline['exists'] and newline['path']==str((root/'line\nbreak').resolve())
t=e['topic']
before=json.loads((root/'before').read_text())
assert t==next(v for v in before['entries'] if Path(v['path']).name=='topic')
assert t['branch']=='codex/facts' and t['exists'] and t['locked']
assert t['dirty'] and t['untracked_count']==1 and t['ignored_count']==2
assert t['merged_to_main'] and t['merged_to_origin_main']
assert not e['missing']['exists'] and e['missing']['prunable']
assert e['missing']['unreadable_reason']=='path_missing'
for row in p['entries']:
    assert not any('cleanup' in key or 'pm_task' in key or 'delete' in key for key in row)
f=json.loads((root/'footprint').read_text())
assert f['summary']['known_linked_target_bytes']>0
assert 'cargo_cache' not in f
assert 'cleanup' not in (root/'human').read_text()
CHECK
# Use the calling worktree's implementation before it has reached main.
"$TMP/topic/scripts/worktree-gc-report.sh" --json --footprint >"$TMP/topic-footprint"
echo 'worktree-gc-report.test: OK'
