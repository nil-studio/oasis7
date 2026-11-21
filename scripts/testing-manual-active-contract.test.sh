#!/usr/bin/env bash
# Contract for active testing-manual examples and S9B evidence references.
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
MANUAL="$ROOT_DIR/testing-manual.md"
CI_TIER_PRD="$ROOT_DIR/doc/testing/ci/ci-tiered-execution.prd.md"

failed=0
missing_event_examples="$(
  awk '
    /\.\/scripts\/plan-(rust-required-scope|wasm-determinism-scope)\.sh/ && /--changed-path/ && $0 !~ /--event-name/ {
      printf "%d:%s\n", NR, $0
    }
  ' "$MANUAL"
)"
if [[ -n "$missing_event_examples" ]]; then
  echo "testing-manual active planner example is missing required --event-name:" >&2
  printf '%s\n' "$missing_event_examples" >&2
  failed=1
fi

s0_section="$(
  awk '
    /^### S0：/ { active = 1 }
    active { print }
    active && /^### S0\.5：/ { exit }
  ' "$MANUAL"
)"
if grep -Eq 'cargo (check|test|build).*--target|cargo (check|test|build) -p (oasis7_viewer|pixel_world_bridge)' <<<"$s0_section"; then
  echo "testing-manual S0 must not contain Viewer/Bevy or other target-specific compilation:" >&2
  printf '%s\n' "$s0_section" >&2
  failed=1
fi
if ! grep -Fq 'S0 是适用于任何改动的快速静态基线' <<<"$s0_section" \
  || ! grep -Fq 'scoped required（例如 S5）' <<<"$s0_section"; then
  echo "testing-manual S0 must state its universal static boundary and scoped-required handoff:" >&2
  failed=1
fi

if ! grep -Fq 'docs-only 同时执行命中的 contract / planner 样例' "$MANUAL"; then
  echo "testing-manual docs-only selection must include relevant contract/planner evidence:" >&2
  failed=1
fi

# The PRD owns these policies. Check the manual's live pointers instead of
# requiring the PRD to retain a second copy of the manual's wording.
for anchor in ci-fast-pr-periodic-full ci-pr-fast-selection; do
  if ! grep -Fq "doc/testing/ci/ci-tiered-execution.prd.md#$anchor)" "$MANUAL" \
    || ! grep -Fxq "<a id=\"$anchor\"></a>" "$CI_TIER_PRD"; then
    echo "testing-manual must link to the active CI policy anchor: $anchor" >&2
    failed=1
  fi
done

for required_manual_policy in \
  'ordinary PR 的 `required-gate` 是 impact-scoped premerge 最小 blocking set' \
  '`full` 不是 ordinary PR 默认' \
  'planner 的 `scope=full` 仅表示 required tier 内 fail-closed 覆盖扩张' \
  '任何被选择的性能 surface 都必须采集环境、原始复现与样本' \
  '有时限 waiver 生命周期成熟前，结论仅为 report/watch'; do
  if ! grep -Fq "$required_manual_policy" "$MANUAL"; then
    echo "testing-manual missing synchronized CI policy: $required_manual_policy" >&2
    failed=1
  fi
done

# Inspect the active selection rule, not the retired duplicated table rows.
viewer_selection="$(grep -F '3. Viewer / Web / 可见 UI：' "$MANUAL" || true)"
case "$viewer_selection" in
  *"crates/oasis7_viewer/**"*"JS-required"*"S6 JS-browser"*"只有触达"*"crates/pixel_world_bridge/**"*"S5"*) ;;
  *)
    echo "testing-manual Viewer selection must separate JS checks, browser checks, and bridge-only S5:" >&2
    failed=1
    ;;
esac

for required_viewer_contract in \
  '### S5：Pixel World Bridge（Bevy）单测与 wasm 编译套件（L4A 前置）' \
  'env -u RUSTC_WRAPPER cargo test -p pixel_world_bridge --lib' \
  'env -u RUSTC_WRAPPER cargo check -p pixel_world_bridge --target wasm32-unknown-unknown' \
  'JS-required（结构/反馈/Vitest/freshness/build）' \
  'JS-browser（真实浏览器）' \
  '关键交互、console、desktop 与 narrow viewport' \
  '任何 player-visible 改动必须通过此层' \
  'JS-full（release/risk-triggered）' \
  '跨模式、恢复路径、长运行与性能验证'; do
  if ! grep -Fq "$required_viewer_contract" "$MANUAL"; then
    echo "testing-manual missing active Viewer/Bevy tier contract: $required_viewer_contract" >&2
    failed=1
  fi
done

obsolete_viewer_package='cargo (check|test|build).* (-p|--package)[[:space:]=]+oasis7_viewer([[:space:]`]|$)'
if grep -Eq "$obsolete_viewer_package" "$MANUAL"; then
  echo "testing-manual must not name oasis7_viewer as the Rust Viewer/Bevy target:" >&2
  grep -En "$obsolete_viewer_package" "$MANUAL" >&2
  failed=1
fi

s9b_references="$(
  awk '
    /^### S9B：/ { active = 1; next }
    active && /^### / { exit }
    active { print }
  ' "$MANUAL" | grep -Eo 'doc/[A-Za-z0-9_./-]+\.md' | sort -u
)"

missing_s9b_references=()
while IFS= read -r reference; do
  [[ -z "$reference" ]] && continue
  if [[ ! -f "$ROOT_DIR/$reference" ]]; then
    missing_s9b_references+=("$reference")
  fi
done <<<"$s9b_references"

if [[ "${#missing_s9b_references[@]}" -gt 0 ]]; then
  echo "testing-manual S9B evidence reference does not exist:" >&2
  printf '%s\n' "${missing_s9b_references[@]}" >&2
  failed=1
fi

l5_section="$(
  awk '
    /^### L5 / { active = 1 }
    active { print }
    active && /^### / && !/^### L5 / { exit }
  ' "$MANUAL"
)"
if ! grep -Fq '真实人类或受控外部玩家' <<<"$l5_section"; then
  echo "testing-manual L5 must remain limited to real-human or controlled external-player evidence:" >&2
  printf '%s\n' "$l5_section" >&2
  failed=1
fi

technical_l5_titles="$(grep -En '^### S(6\.5|8|9|10)：.*L5' "$MANUAL" || true)"
if [[ -n "$technical_l5_titles" ]]; then
  echo "testing-manual technical suites must not carry L5 labels:" >&2
  printf '%s\n' "$technical_l5_titles" >&2
  failed=1
fi

for required_heading in \
  '### S6.5：Chain Runtime Storage Profile / Gate 技术核验' \
  '### S8：长稳与压力技术套件' \
  '### S9：P2P/存储/共识在线长跑技术套件' \
  '### S10：五节点真实游戏数据在线长跑技术套件'; do
  if ! grep -Fqx "$required_heading" "$MANUAL"; then
    echo "testing-manual missing technical-suite boundary heading: $required_heading" >&2
    failed=1
  fi
done

if grep -Fq 'public_testnet_rehearsal` history may explain benchmark L5 evidence' "$MANUAL"; then
  echo "testing-manual must not treat public_testnet rehearsal history as L5 evidence:" >&2
  failed=1
fi

l5_triage="$(awk '/^## 失败分诊（按层）/ { active = 1 } active && /^6\. L5 / { print; exit }' "$MANUAL")"
if [[ "$l5_triage" != *'真实人类或受控外部玩家样本'* ]] || [[ "$l5_triage" != *'S8/S9/S10'* ]]; then
  echo "testing-manual L5 triage must route technical failures back to S8/S9/S10:" >&2
  printf '%s\n' "$l5_triage" >&2
  failed=1
fi

if [[ "$failed" -ne 0 ]]; then
  exit 1
fi

echo "testing-manual-active-contract.test: OK"
