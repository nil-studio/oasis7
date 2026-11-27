#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_root"
source "$repo_root/scripts/cargo-dev-lib.sh"

print_help() {
  cat <<'USAGE'
Usage: ./scripts/llm-longrun-stress.sh [options]

Options:
  --scenario <name>            Scenario for oasis7_llm_agent_demo (repeatable)
  --scenarios <a,b,c>          Comma-separated scenario list
  --jobs <n>                   Max parallel scenarios in multi-scenario mode (default: 1)
  --ticks <n>                  Number of ticks to run (default: 240)
  --out-dir <path>             Output directory (default: .tmp/llm_stress)
  --report-json <path>         Report json path (default: <out-dir>/report.json)
  --log-file <path>            Raw command log path (default: <out-dir>/run.log)
  --summary-file <path>        Summary text path (default: <out-dir>/summary.txt)
  --llm-system-prompt <text>   Override LLM system prompt for this run
  --llm-short-goal <text>      Override LLM short-term goal for this run
  --llm-long-goal <text>       Override LLM long-term goal for this run
  --prompt-pack <name>         Apply built-in gameplay development test prompt pack
  --prompt-switch-tick <n>     Apply switch prompt overrides at tick n (1-based)
  --switch-llm-system-prompt <text>  Switch system prompt after --prompt-switch-tick
  --switch-llm-short-goal <text>     Switch short-term goal after --prompt-switch-tick
  --switch-llm-long-goal <text>      Switch long-term goal after --prompt-switch-tick
  --prompt-switches-json <json>      Multi-stage switch plan JSON (array of {"tick":n,"llm_*":...})
  --llm-execute-until-auto-reenter-ticks <n>  Override OASIS7_LLM_EXECUTE_UNTIL_AUTO_REENTER_TICKS
  --runtime-gameplay-bridge          Enable runtime gameplay bridge in demo (default: on)
  --no-runtime-gameplay-bridge       Disable runtime gameplay bridge in demo
  --runtime-gameplay-preset <name>   Seed runtime gameplay events before loop (none|civic_hotspot_v1)
  --load-state-dir <path>            Load simulator state dir before run (snapshot/journal)
  --save-state-dir <path>            Save simulator state dir after run (snapshot/journal)
  --max-llm-errors <n>         Fail if llm_errors > n (default: 0)
  --max-parse-errors <n>       Fail if parse_errors > n (default: 0)
  --max-repair-rounds-max <n>  Fail if repair_rounds_max > n (default: 2)
  --min-active-ticks <n>       Fail if active_ticks < n (default: ticks)
  --min-action-kinds <n>       Fail if distinct action kinds < n (default: 0)
  --require-action-kind <k:n>  Require action kind k count >= n (repeatable)
  --release-gate               Enable release defaults (min kinds + core actions)
  --release-gate-profile <p>   Release gate profile: industrial | gameplay | hybrid (default: hybrid)
  --coverage-bootstrap-profile <p>  Deterministic bootstrap before LLM loop: none | industrial | gameplay | hybrid
  --local-provider-bind <host:port>
                              Local provider bridge reuse address (default: 127.0.0.1:5841)
  --reuse-local-provider-bridge
                              Reuse an already-listening local provider bridge
  --proxy <url>                HTTP/HTTPS proxy for local provider bridge (or $OASIS7_LOCAL_TEST_PROXY_URL; unset by default)
  --socks-proxy <url>          all_proxy value for local provider bridge (or $OASIS7_LOCAL_TEST_SOCKS_PROXY_URL; unset by default)
  --no-default-proxy           Do not set local proxy defaults for local provider bridge
  --agent-provider-connect-timeout-ms <n>
                              Provider bridge connect timeout passed to demo (default: 60000)
  --agent-provider-decision-timeout-ms <n>
                              Provider decision budget passed to demo (default: 60000)
  --agent-provider-profile <name>
                              Provider agent profile passed to demo (default: oasis7_p0_low_freq_npc)
  --no-llm-io                  Disable LLM input/output logging in run.log
  --llm-io-max-chars <n>       Truncate each LLM input/output block to n chars
  --keep-out-dir               Keep existing out dir content
  -h, --help                   Show help

Notes:
  - If no scenario is provided, default is llm_bootstrap.
  - Single-scenario mode keeps legacy output behavior.
  - Multi-scenario mode supports parallel runs via --jobs.
  - Multi-scenario mode writes per-scenario outputs to:
      <out-dir>/scenarios/<scenario>/{report.json,run.log,summary.txt}
    and writes aggregate outputs to report-json/log-file/summary-file.
  - --release-gate applies profile-based gameplay coverage:
      industrial -> harvest_radiation,mine_compound,refine_compound,build_factory,schedule_recipe
      gameplay   -> open_governance_proposal,cast_governance_vote,resolve_crisis,grant_meta_progress
      hybrid     -> industrial + gameplay
  - --release-gate 默认同时启用 coverage bootstrap profile（可通过 --coverage-bootstrap-profile 显式覆盖）
  - --prompt-pack options:
      story_balanced (recommended) -> staged growth (stability -> production -> governance/resilience)
      frontier_builder             -> exploration + production expansion
      industrial_baseline          -> industrial-first baseline build (rules -> mine/refine -> factory/schedule)
      civic_operator               -> governance + collaboration cadence
      resilience_drill             -> crisis/economic contract pressure test
  - story_balanced 会在 ticks 较长时自动注入多阶段切换计划（通过 --prompt-switches-json 透传）
  - industrial_baseline 默认设置 OASIS7_LLM_EXECUTE_UNTIL_AUTO_REENTER_TICKS=24（可通过参数覆盖）
  - runtime gameplay bridge 默认开启：将 simulator 的 runtime-only gameplay/economic 动作接入 runtime World，降低非预期拒绝
  - runtime gameplay preset 可注入可续跑的治理事件句柄（待投票提案/活跃危机/待结算合约）
  - state dir 参数仅支持单场景模式（便于构建/复用同一阶段基线）
  - --no-llm-io only disables raw LLM I/O logging; this stress runner still needs an active LLM provider.

Output:
  - report json: detailed run metrics emitted by oasis7_llm_agent_demo
  - run log: cargo run stdout/stderr output (includes LLM I/O by default)
  - summary: flattened key metrics for quick comparison
USAGE
}

run() {
  echo "+ $*"
  "$@"
}

local_provider_base_url() {
  printf 'http://%s' "$local_provider_bind"
}

local_provider_info_url() {
  printf '%s/v1/provider/info' "$(local_provider_base_url)"
}

local_provider_ready() {
  curl -fsS "$(local_provider_info_url)" >/dev/null 2>&1
}

cleanup_local_provider_bridge() {
  if [[ -n "${local_provider_bridge_pid:-}" ]] && kill -0 "$local_provider_bridge_pid" >/dev/null 2>&1; then
    kill "$local_provider_bridge_pid" >/dev/null 2>&1 || true
    wait "$local_provider_bridge_pid" >/dev/null 2>&1 || true
  fi
}

configure_local_provider_proxy_env() {
  if (( with_local_provider_bridge == 0 || use_default_proxy == 0 )); then
    return 0
  fi

  if [[ -n "$proxy_url" && -z "${https_proxy:-${HTTPS_PROXY:-}}" ]]; then
    export https_proxy="$proxy_url"
  fi
  if [[ -n "$proxy_url" && -z "${http_proxy:-${HTTP_PROXY:-}}" ]]; then
    export http_proxy="$proxy_url"
  fi
  if [[ -n "$socks_proxy_url" && -z "${all_proxy:-${ALL_PROXY:-}}" ]]; then
    export all_proxy="$socks_proxy_url"
  fi

  local loopback_no_proxy="127.0.0.1,localhost,::1"
  if [[ -n "${no_proxy:-}" ]]; then
    case ",$no_proxy," in
      *",127.0.0.1,"*) ;;
      *) export no_proxy="$loopback_no_proxy,$no_proxy" ;;
    esac
  else
    export no_proxy="$loopback_no_proxy"
  fi
  if [[ -n "${NO_PROXY:-}" ]]; then
    case ",$NO_PROXY," in
      *",127.0.0.1,"*) ;;
      *) export NO_PROXY="$loopback_no_proxy,$NO_PROXY" ;;
    esac
  else
    export NO_PROXY="$loopback_no_proxy"
  fi
}

start_or_reuse_local_provider_bridge() {
  if (( with_local_provider_bridge == 0 )); then
    return 0
  fi

  local provider_url
  provider_url=$(local_provider_base_url)
  configure_local_provider_proxy_env
  if local_provider_ready; then
    echo "local provider bridge: reusing $provider_url"
    return 0
  fi

  echo "error: local provider bridge is not listening at $provider_url" >&2
  echo "hint: start oasis7_provider_local_bridge at $local_provider_bind before passing --reuse-local-provider-bridge." >&2
  exit 2
}

ensure_positive_int() {
  local name=$1
  local value=$2
  if [[ ! "$value" =~ ^[0-9]+$ ]]; then
    echo "invalid integer for $name: $value" >&2
    exit 2
  fi
}

trim_whitespace() {
  local value=$1
  value="${value#"${value%%[![:space:]]*}"}"
  value="${value%"${value##*[![:space:]]}"}"
  echo "$value"
}

append_scenario() {
  local raw=$1
  local value
  value=$(trim_whitespace "$raw")
  if [[ -z "$value" ]]; then
    echo "invalid empty scenario" >&2
    exit 2
  fi
  scenario_inputs+=("$value")
}

append_scenarios_from_csv() {
  local csv=$1
  local part
  IFS=',' read -r -a csv_parts <<<"$csv"
  for part in "${csv_parts[@]}"; do
    append_scenario "$part"
  done
}

upsert_required_action_kind() {
  local kind=$1
  local min_count=$2
  local idx

  for idx in "${!required_action_kinds[@]}"; do
    if [[ "${required_action_kinds[$idx]}" == "$kind" ]]; then
      if (( min_count > required_action_min_counts[$idx] )); then
        required_action_min_counts[$idx]=$min_count
      fi
      return 0
    fi
  done

  required_action_kinds+=("$kind")
  required_action_min_counts+=("$min_count")
}

append_required_action_kind() {
  local raw=$1
  local value
  local kind
  local min_count

  value=$(trim_whitespace "$raw")
  if [[ -z "$value" || "$value" != *:* ]]; then
    echo "invalid --require-action-kind value: $raw (expected <kind>:<min_count>)" >&2
    exit 2
  fi
  kind=$(trim_whitespace "${value%%:*}")
  min_count=$(trim_whitespace "${value##*:}")
  if [[ -z "$kind" ]]; then
    echo "invalid --require-action-kind kind: $raw" >&2
    exit 2
  fi
  ensure_positive_int "--require-action-kind min_count" "$min_count"
  upsert_required_action_kind "$kind" "$min_count"
}

format_required_action_kind_requirements() {
  if (( ${#required_action_kinds[@]} == 0 )); then
    echo "none"
    return 0
  fi

  local idx
  local out=""
  for idx in "${!required_action_kinds[@]}"; do
    if [[ -n "$out" ]]; then
      out+=","
    fi
    out+="${required_action_kinds[$idx]}:${required_action_min_counts[$idx]}"
  done
  echo "$out"
}

apply_release_gate_defaults() {
  if (( release_gate == 0 )); then
    return 0
  fi

  local profile
  profile=$(printf '%s' "$release_gate_profile" | tr '[:upper:]' '[:lower:]')
  case "$profile" in
    industrial)
      if (( min_action_kinds < 5 )); then
        min_action_kinds=5
      fi
      upsert_required_action_kind "harvest_radiation" 1
      upsert_required_action_kind "mine_compound" 1
      upsert_required_action_kind "refine_compound" 1
      upsert_required_action_kind "build_factory" 1
      upsert_required_action_kind "schedule_recipe" 1
      ;;
    gameplay)
      if (( min_action_kinds < 4 )); then
        min_action_kinds=4
      fi
      upsert_required_action_kind "open_governance_proposal" 1
      upsert_required_action_kind "cast_governance_vote" 1
      upsert_required_action_kind "resolve_crisis" 1
      upsert_required_action_kind "grant_meta_progress" 1
      ;;
    hybrid)
      if (( min_action_kinds < 9 )); then
        min_action_kinds=9
      fi
      upsert_required_action_kind "harvest_radiation" 1
      upsert_required_action_kind "mine_compound" 1
      upsert_required_action_kind "refine_compound" 1
      upsert_required_action_kind "build_factory" 1
      upsert_required_action_kind "schedule_recipe" 1
      upsert_required_action_kind "open_governance_proposal" 1
      upsert_required_action_kind "cast_governance_vote" 1
      upsert_required_action_kind "resolve_crisis" 1
      upsert_required_action_kind "grant_meta_progress" 1
      ;;
    *)
      echo "invalid --release-gate-profile: $release_gate_profile (expected industrial|gameplay|hybrid)" >&2
      exit 2
      ;;
  esac

  release_gate_profile="$profile"
  if (( coverage_bootstrap_profile_explicit == 0 )); then
    coverage_bootstrap_profile="$profile"
  fi
  if (( max_parse_errors_explicit == 0 )); then
    local adaptive_parse_errors
    adaptive_parse_errors=$(((ticks + 39) / 40))
    if (( adaptive_parse_errors < 2 )); then
      adaptive_parse_errors=2
    fi
    if (( max_parse_errors < adaptive_parse_errors )); then
      max_parse_errors=$adaptive_parse_errors
    fi
  fi
}

apply_prompt_pack_defaults() {
  if [[ -z "$prompt_pack" ]]; then
    return 0
  fi

  local pack
  pack=$(printf '%s' "$prompt_pack" | tr '[:upper:]' '[:lower:]')
  case "$pack" in
    story_balanced)
      if [[ -z "$llm_system_prompt" ]]; then
        llm_system_prompt="你是硅基文明叙事导演兼发展代理。按阶段推进文明：开局稳态（能源与位置）-> 产业组织（生产与协作）-> 公共治理（提案与投票）-> 韧性维护（危机处置与成长结算）；避免长期重复同一动作。每次只输出一个严格合法的决策 JSON 对象，不附加解释文本。"
      fi
      if [[ -z "$llm_short_goal" ]]; then
        llm_short_goal="把本轮 tick 视为一局完整剧情：前段稳住能源与位置，中段扩展生产与协作，后段转向治理议题与风险处置；根据局势自然切换动作，避免无意义重复。"
      fi
      if [[ -z "$llm_long_goal" ]]; then
        llm_long_goal="形成可持续文明闭环：资源可用、组织可治理、危机可恢复，让行动呈现阶段递进而不是单一刷动作。"
      fi
      if [[ -z "$prompt_switches_json" && -z "$prompt_switch_tick" && -z "$switch_llm_system_prompt" && -z "$switch_llm_short_goal" && -z "$switch_llm_long_goal" ]]; then
        prompt_switches_json=$(build_story_balanced_prompt_switches_json "$ticks")
      fi
      ;;
    frontier_builder)
      if [[ -z "$llm_system_prompt" ]]; then
        llm_system_prompt="你是边疆建设型文明代理。重视探索、定居与生产链扩展，目标是在不失稳的前提下持续开疆与建设。"
      fi
      if [[ -z "$llm_short_goal" ]]; then
        llm_short_goal="优先处理当前瓶颈并扩展基础设施，保持探索与建设节奏，避免在单一动作上过度停留。"
      fi
      if [[ -z "$llm_long_goal" ]]; then
        llm_long_goal="构建高可扩展的生产与迁移网络，为后续治理和风险对抗提供冗余。"
      fi
      ;;
    industrial_baseline)
      if [[ -z "$llm_system_prompt" ]]; then
        llm_system_prompt="你是硅基文明的总务官。你的职责是让文明像真实游戏进程一样推进：先掌握规则和地形，再建立最小工业闭环，然后才进入治理扩展。每回合只提交一个合法 decision JSON。遇到规则不清或动作失败，先查 world.rules.guide 与 environment.current_observation，再调整前置条件。"
      fi
      if [[ -z "$llm_short_goal" ]]; then
        llm_short_goal="当前章节目标是工业建基线：先完成一次可持续资源链（mine_compound -> refine_compound），再在合适地点建造 assembler 工厂并至少安排一次配方生产。动作要自然衔接，不重复空转。"
      fi
      if [[ -z "$llm_long_goal" ]]; then
        llm_long_goal="在同一局内形成可复用的工业起点：能源稳定、资源转换可持续、工厂可生产，为后续治理和社会事件留下空间。"
      fi
      if [[ -z "$llm_execute_until_auto_reenter_ticks" ]]; then
        llm_execute_until_auto_reenter_ticks="24"
      fi
      ;;
    civic_operator)
      if [[ -z "$runtime_gameplay_preset" ]]; then
        runtime_gameplay_preset="civic_hotspot_v1"
      fi
      if [[ -z "$llm_system_prompt" ]]; then
        llm_system_prompt="你是文明治理运营代理。通过提案、投票、协作与秩序维护推动系统长期稳定，而非只追求短期资源增量。"
      fi
      if [[ -z "$llm_short_goal" ]]; then
        llm_short_goal="在保障基本运转的前提下，优先推进治理议题、协调行为冲突并减少无效重复决策。若已注入 preset 句柄，优先围绕 preset.governance.civic_hotspot_v1、crisis.auto.8、preset.contract.civic_hotspot_v1 推进治理闭环。"
      fi
      if [[ -z "$llm_long_goal" ]]; then
        llm_long_goal="形成可迭代的治理机制，使文明在扩张中保持可协调与可持续。"
      fi
      ;;
    resilience_drill)
      if [[ -z "$runtime_gameplay_preset" ]]; then
        runtime_gameplay_preset="civic_hotspot_v1"
      fi
      if [[ -z "$llm_system_prompt" ]]; then
        llm_system_prompt="你是韧性演练代理。关注危机感知、恢复路径、经济协作与失败后重构能力，优先验证文明抗压上限。"
      fi
      if [[ -z "$llm_short_goal" ]]; then
        llm_short_goal="主动寻找并处理系统脆弱点，结合治理与经济协作策略降低风险扩散，避免单一操作循环。若已注入 preset 句柄，优先围绕 crisis.auto.8 与 preset.contract.civic_hotspot_v1 完成一次危机与协作闭环。"
      fi
      if [[ -z "$llm_long_goal" ]]; then
        llm_long_goal="建立可恢复、可再平衡的文明机制，使危机后能快速重回发展轨道。"
      fi
      ;;
    *)
      echo "invalid --prompt-pack: $prompt_pack (expected story_balanced|frontier_builder|industrial_baseline|civic_operator|resilience_drill)" >&2
      exit 2
      ;;
  esac

  prompt_pack="$pack"
}

build_story_balanced_prompt_switches_json() {
  local ticks_value=$1
  python3 - "$ticks_value" <<'PY'
import json
import sys

ticks = max(1, int(sys.argv[1]))

def clamp_tick(value: int) -> int:
    return max(12, min(ticks, value))

if ticks >= 960:
    raw_stages = [
        {
            "tick": clamp_tick(ticks // 4),
            "llm_system_prompt": "你是中期文明治理运营代理。重点把生产扩张转化为协作秩序：围绕治理议题、经济合约与危机预防推进叙事，避免无意义重复动作。",
            "llm_short_term_goal": "进入中期剧情：在保持基础产能的同时，优先推动一次治理协商或经济协作落地，并对潜在危机做前置处置。",
            "llm_long_term_goal": "让文明从资源扩张过渡到制度化协作，使治理、协作与风险处置形成常态闭环。",
        },
        {
            "tick": clamp_tick(ticks // 2),
            "llm_system_prompt": "你是中后期文明策略代理。围绕联盟关系、治理投票、经济结算与危机处理进行节奏调度，确保玩法事件持续演进。",
            "llm_short_term_goal": "进入中后段剧情：根据当前世界状态，优先选择能推进公共秩序与韧性的动作，避免在单一治理动作中循环。",
            "llm_long_term_goal": "保持制度韧性与资源效率并进，使文明在冲突与协作中持续可恢复。",
        },
        {
            "tick": clamp_tick((ticks * 3) // 4),
            "llm_system_prompt": "你是晚期文明稳态代理。关注长期平衡与风险回收：持续推进治理收敛、危机恢复与成长沉淀，不追求短期刷动作。",
            "llm_short_term_goal": "进入晚期剧情：优先处理未闭环的治理议题与风险尾项，并完成阶段性成长反馈。",
            "llm_long_term_goal": "形成可长期运行的文明稳态：制度可执行、风险可恢复、成长可累积。",
        },
    ]
elif ticks >= 360:
    raw_stages = [
        {
            "tick": clamp_tick(ticks // 3),
            "llm_system_prompt": "你是中后期文明运营代理。让世界从资源扩张转向制度建设与韧性治理：围绕正在发生的议题推进提案、协商、危机处置与成长结算，避免陷入单一采集或单一治理动作循环。",
            "llm_short_term_goal": "进入中后期剧情节点：先识别最紧迫的公共议题，再选择最合适的治理或韧性动作推进一格；若同类治理动作连续出现，主动切换到危机处置、成长结算或经济协作。",
            "llm_long_term_goal": "让文明从资源驱动转向制度与韧性驱动，使治理讨论、风险处理与成长反馈形成持续演化。",
        },
        {
            "tick": clamp_tick((ticks * 2) // 3),
            "llm_system_prompt": "你是后段文明韧性代理。重点收敛治理与危机尾项，维持系统稳定并推动成长结算，避免动作模式固化。",
            "llm_short_term_goal": "进入后段剧情：优先完成未闭环议题与风险处置，再推进成长沉淀。",
            "llm_long_term_goal": "保证文明后半程的治理执行力与恢复力，避免单一动作导致演化停滞。",
        },
    ]
else:
    raw_stages = [
        {
            "tick": clamp_tick(ticks // 2),
            "llm_system_prompt": "你是中后期文明运营代理。让世界从资源扩张转向制度建设与韧性治理：围绕正在发生的议题推进提案、协商、危机处置与成长结算，避免陷入单一采集或单一治理动作循环。决策输出必须严格符合 JSON schema。",
            "llm_short_term_goal": "进入中后期剧情节点：先识别最紧迫的公共议题，再选择最合适的治理或韧性动作推进一格；若同类治理动作连续出现，主动切换到危机处置、成长结算或经济协作。",
            "llm_long_term_goal": "让文明从资源驱动转向制度与韧性驱动，使治理讨论、风险处理与成长反馈形成持续演化。",
        }
    ]

stages = []
last_tick = 0
for stage in raw_stages:
    tick = max(stage["tick"], last_tick + 1)
    tick = min(tick, ticks)
    if tick <= last_tick:
        continue
    normalized = dict(stage)
    normalized["tick"] = tick
    stages.append(normalized)
    last_tick = tick

if not stages:
    stages = [raw_stages[-1]]
    stages[0]["tick"] = min(max(1, stages[0]["tick"]), ticks)

print(json.dumps(stages, ensure_ascii=False))
PY
}

extract_metric_from_log() {
  local key=$1
  local log_path=$2
  local line
  line=$(grep -E "^${key}: " "$log_path" | tail -n1 || true)
  if [[ -z "$line" ]]; then
    return 1
  fi
  echo "${line##*: }"
}

active_ticks=0
total_decisions=0
total_actions=0
action_success=0
action_failure=0
llm_skipped_ticks=0
llm_skipped_tick_ratio_ppm=0
llm_errors=0
parse_errors=0
repair_rounds_total=0
repair_rounds_max=0
llm_input_chars_total=0
llm_input_chars_avg=0
llm_input_chars_max=0
clipped_sections=0
decision_wait=0
decision_wait_ticks=0
decision_act=0
module_call_count=0
plan_count=0
execute_until_continue_count=0
action_kinds_total=0
action_kind_pairs=""
action_kind_counts_inline="none"
runtime_perf_health="unknown"
runtime_perf_bottleneck="none"
runtime_perf_tick_p95_ms="0"
runtime_perf_tick_over_budget_ratio_ppm=0

assign_metrics_from_report_lines() {
  local report_metrics=$1
  local metric_index=0
  local metric_value=""

  while IFS= read -r metric_value; do
    metric_index=$((metric_index + 1))
    case "$metric_index" in
      1) active_ticks=$metric_value ;;
      2) total_decisions=$metric_value ;;
      3) total_actions=$metric_value ;;
      4) action_success=$metric_value ;;
      5) action_failure=$metric_value ;;
      6) llm_skipped_ticks=$metric_value ;;
      7) llm_skipped_tick_ratio_ppm=$metric_value ;;
      8) llm_errors=$metric_value ;;
      9) parse_errors=$metric_value ;;
      10) repair_rounds_total=$metric_value ;;
      11) repair_rounds_max=$metric_value ;;
      12) llm_input_chars_total=$metric_value ;;
      13) llm_input_chars_avg=$metric_value ;;
      14) llm_input_chars_max=$metric_value ;;
      15) clipped_sections=$metric_value ;;
      16) decision_wait=$metric_value ;;
      17) decision_wait_ticks=$metric_value ;;
      18) decision_act=$metric_value ;;
      19) module_call_count=$metric_value ;;
      20) plan_count=$metric_value ;;
      21) execute_until_continue_count=$metric_value ;;
      22) runtime_perf_health=$metric_value ;;
      23) runtime_perf_bottleneck=$metric_value ;;
      24) runtime_perf_tick_p95_ms=$metric_value ;;
      25) runtime_perf_tick_over_budget_ratio_ppm=$metric_value ;;
    esac
  done <<<"$report_metrics"

  active_ticks=${active_ticks:-0}
  total_decisions=${total_decisions:-0}
  total_actions=${total_actions:-0}
  action_success=${action_success:-0}
  action_failure=${action_failure:-0}
  llm_skipped_ticks=${llm_skipped_ticks:-0}
  llm_skipped_tick_ratio_ppm=${llm_skipped_tick_ratio_ppm:-0}
  llm_errors=${llm_errors:-0}
  parse_errors=${parse_errors:-0}
  repair_rounds_total=${repair_rounds_total:-0}
  repair_rounds_max=${repair_rounds_max:-0}
  llm_input_chars_total=${llm_input_chars_total:-0}
  llm_input_chars_avg=${llm_input_chars_avg:-0}
  llm_input_chars_max=${llm_input_chars_max:-0}
  clipped_sections=${clipped_sections:-0}
  decision_wait=${decision_wait:-0}
  decision_wait_ticks=${decision_wait_ticks:-0}
  decision_act=${decision_act:-0}
  module_call_count=${module_call_count:-0}
  plan_count=${plan_count:-0}
  execute_until_continue_count=${execute_until_continue_count:-0}
  runtime_perf_health=${runtime_perf_health:-unknown}
  runtime_perf_bottleneck=${runtime_perf_bottleneck:-none}
  runtime_perf_tick_p95_ms=${runtime_perf_tick_p95_ms:-0}
  runtime_perf_tick_over_budget_ratio_ppm=${runtime_perf_tick_over_budget_ratio_ppm:-0}
}

load_metrics_from_report() {
  local report_path=$1
  local log_path=$2
  local report_metrics=""

  if command -v jq >/dev/null 2>&1; then
    report_metrics=$(jq -r '[
      (.active_ticks // 0),
      (.total_decisions // 0),
      (.total_actions // 0),
      (.action_success // 0),
      (.action_failure // 0),
      (.trace_counts.llm_skipped_ticks // 0),
      (.trace_counts.llm_skipped_tick_ratio_ppm // 0),
      (.trace_counts.llm_errors // 0),
      (.trace_counts.parse_errors // 0),
      (.trace_counts.repair_rounds_total // 0),
      (.trace_counts.repair_rounds_max // 0),
      (.trace_counts.llm_input_chars_total // 0),
      (.trace_counts.llm_input_chars_avg // 0),
      (.trace_counts.llm_input_chars_max // 0),
      (.trace_counts.prompt_section_clipped // 0),
      (.decision_counts.wait // 0),
      (.decision_counts.wait_ticks // 0),
      (.decision_counts.act // 0),
      (.trace_counts.step_type_counts.module_call // 0),
      (.trace_counts.step_type_counts.plan // 0),
      (.trace_counts.step_type_counts.execute_until_continue // 0),
      (.runtime_perf.health // "unknown"),
      (.runtime_perf.bottleneck // "none"),
      (.runtime_perf.tick.p95_ms // 0),
      (.runtime_perf.tick.over_budget_ratio_ppm // 0)
    ] | .[]' "$report_path")
    assign_metrics_from_report_lines "$report_metrics"
  elif command -v python3 >/dev/null 2>&1; then
    report_metrics=$(python3 - "$report_path" <<'__PYJSON__'
import json
import sys

path = sys.argv[1]
with open(path, "r", encoding="utf-8") as fh:
    report = json.load(fh)

def get(path, default=0):
    current = report
    for key in path.split('.'):
        if not isinstance(current, dict):
            return default
        current = current.get(key)
        if current is None:
            return default
    return current

keys = [
    "active_ticks",
    "total_decisions",
    "total_actions",
    "action_success",
    "action_failure",
    "trace_counts.llm_skipped_ticks",
    "trace_counts.llm_skipped_tick_ratio_ppm",
    "trace_counts.llm_errors",
    "trace_counts.parse_errors",
    "trace_counts.repair_rounds_total",
    "trace_counts.repair_rounds_max",
    "trace_counts.llm_input_chars_total",
    "trace_counts.llm_input_chars_avg",
    "trace_counts.llm_input_chars_max",
    "trace_counts.prompt_section_clipped",
    "decision_counts.wait",
    "decision_counts.wait_ticks",
    "decision_counts.act",
    "trace_counts.step_type_counts.module_call",
    "trace_counts.step_type_counts.plan",
    "trace_counts.step_type_counts.execute_until_continue",
    "runtime_perf.health",
    "runtime_perf.bottleneck",
    "runtime_perf.tick.p95_ms",
    "runtime_perf.tick.over_budget_ratio_ppm",
]
for key in keys:
    print(get(key, 0))
__PYJSON__
)
    assign_metrics_from_report_lines "$report_metrics"
  else
    active_ticks=$(extract_metric_from_log "active_ticks" "$log_path" || echo 0)
    total_decisions=$(extract_metric_from_log "total_decisions" "$log_path" || echo 0)
    total_actions=$(extract_metric_from_log "total_actions" "$log_path" || echo 0)
    action_success=$(extract_metric_from_log "action_success" "$log_path" || echo 0)
    action_failure=$(extract_metric_from_log "action_failure" "$log_path" || echo 0)
    llm_skipped_ticks=$(extract_metric_from_log "llm_skipped_ticks" "$log_path" || echo 0)
    llm_skipped_tick_ratio_ppm=$(extract_metric_from_log "llm_skipped_tick_ratio_ppm" "$log_path" || echo 0)
    llm_errors=$(extract_metric_from_log "llm_errors" "$log_path" || echo 0)
    parse_errors=$(extract_metric_from_log "parse_errors" "$log_path" || echo 0)
    repair_rounds_total=$(extract_metric_from_log "repair_rounds_total" "$log_path" || echo 0)
    repair_rounds_max=$(extract_metric_from_log "repair_rounds_max" "$log_path" || echo 0)
    llm_input_chars_total=$(extract_metric_from_log "llm_input_chars_total" "$log_path" || echo 0)
    llm_input_chars_avg=$(extract_metric_from_log "llm_input_chars_avg" "$log_path" || echo 0)
    llm_input_chars_max=$(extract_metric_from_log "llm_input_chars_max" "$log_path" || echo 0)
    clipped_sections=0
    decision_wait=$(extract_metric_from_log "decision_wait" "$log_path" || echo 0)
    decision_wait_ticks=$(extract_metric_from_log "decision_wait_ticks" "$log_path" || echo 0)
    decision_act=$(extract_metric_from_log "decision_act" "$log_path" || echo 0)
    module_call_count=0
    plan_count=0
    execute_until_continue_count=0
    runtime_perf_health=$(extract_metric_from_log "runtime_perf_health" "$log_path" || echo "unknown")
    runtime_perf_bottleneck=$(extract_metric_from_log "runtime_perf_bottleneck" "$log_path" || echo "none")
    runtime_perf_tick_p95_ms=$(extract_metric_from_log "runtime_perf_tick_p95_ms" "$log_path" || echo 0)
    runtime_perf_tick_over_budget_ratio_ppm=$(extract_metric_from_log "runtime_perf_tick_over_budget_ratio_ppm" "$log_path" || echo 0)
  fi
}

extract_action_kind_pairs_from_log() {
  local log_path=$1
  grep -E '^action_kind_[^:]+: [0-9]+$' "$log_path" \
    | sed -E 's/^action_kind_([^:]+): ([0-9]+)$/\1\t\2/' || true
}

load_action_kind_counts() {
  local report_path=$1
  local log_path=$2
  local parsed_pairs=""

  action_kinds_total=0
  action_kind_pairs=""
  action_kind_counts_inline="none"

  if command -v jq >/dev/null 2>&1; then
    parsed_pairs=$(jq -r '.action_kind_counts // {} | to_entries[]? | "\(.key)\t\(.value)"' "$report_path")
  elif command -v python3 >/dev/null 2>&1; then
    parsed_pairs=$(python3 - "$report_path" <<'__PYACTIONKIND__'
import json
import sys

path = sys.argv[1]
with open(path, "r", encoding="utf-8") as fh:
    report = json.load(fh)

counts = report.get("action_kind_counts") or {}
for kind in sorted(counts.keys()):
    print(f"{kind}\t{int(counts[kind] or 0)}")
__PYACTIONKIND__
)
  else
    parsed_pairs=$(extract_action_kind_pairs_from_log "$log_path")
  fi

  if [[ -z "$parsed_pairs" ]]; then
    return 0
  fi

  action_kind_pairs=$(printf '%s\n' "$parsed_pairs" | sed '/^[[:space:]]*$/d')
  if [[ -z "$action_kind_pairs" ]]; then
    return 0
  fi

  action_kinds_total=$(printf '%s\n' "$action_kind_pairs" | wc -l | tr -d ' ')
  action_kind_counts_inline=$(printf '%s\n' "$action_kind_pairs" | awk -F'\t' '
    BEGIN { sep="" }
    NF >= 2 {
      printf "%s%s:%s", sep, $1, $2
      sep=","
    }
  ')
  if [[ -z "$action_kind_counts_inline" ]]; then
    action_kind_counts_inline="none"
  fi
}

action_kind_count_for() {
  local target_kind=$1
  local kind
  local count

  if [[ -z "$action_kind_pairs" ]]; then
    echo 0
    return 0
  fi

  while IFS=$'\t' read -r kind count; do
    if [[ "$kind" == "$target_kind" ]]; then
      echo "${count:-0}"
      return 0
    fi
  done <<<"$action_kind_pairs"

  echo 0
}

write_summary_file() {
  local summary_path=$1
  local scenario_name=$2
  local llm_system_prompt_set=0
  local llm_short_goal_set=0
  local llm_long_goal_set=0
  local switch_llm_system_prompt_set=0
  local switch_llm_short_goal_set=0
  local switch_llm_long_goal_set=0
  local prompt_switches_json_set=0

  [[ -n "$llm_system_prompt" ]] && llm_system_prompt_set=1
  [[ -n "$llm_short_goal" ]] && llm_short_goal_set=1
  [[ -n "$llm_long_goal" ]] && llm_long_goal_set=1
  [[ -n "$switch_llm_system_prompt" ]] && switch_llm_system_prompt_set=1
  [[ -n "$switch_llm_short_goal" ]] && switch_llm_short_goal_set=1
  [[ -n "$switch_llm_long_goal" ]] && switch_llm_long_goal_set=1
  [[ -n "$prompt_switches_json" ]] && prompt_switches_json_set=1

  {
    echo "scenario=$scenario_name"
    echo "ticks=$ticks"
    echo "active_ticks=$active_ticks"
    echo "total_decisions=$total_decisions"
    echo "total_actions=$total_actions"
    echo "action_success=$action_success"
    echo "action_failure=$action_failure"
    echo "llm_skipped_ticks=$llm_skipped_ticks"
    echo "llm_skipped_tick_ratio_ppm=$llm_skipped_tick_ratio_ppm"
    echo "llm_errors=$llm_errors"
    echo "parse_errors=$parse_errors"
    echo "repair_rounds_total=$repair_rounds_total"
    echo "repair_rounds_max=$repair_rounds_max"
    echo "llm_input_chars_total=$llm_input_chars_total"
    echo "llm_input_chars_avg=$llm_input_chars_avg"
    echo "llm_input_chars_max=$llm_input_chars_max"
    echo "prompt_section_clipped=$clipped_sections"
    echo "decision_wait=$decision_wait"
    echo "decision_wait_ticks=$decision_wait_ticks"
    echo "decision_act=$decision_act"
    echo "module_call=$module_call_count"
    echo "plan=$plan_count"
    echo "execute_until_continue=$execute_until_continue_count"
    echo "release_gate=$release_gate"
    echo "release_gate_profile=$release_gate_profile"
    echo "min_action_kinds=$min_action_kinds"
    echo "required_action_kinds=$required_action_kinds_config"
    echo "action_kinds_total=$action_kinds_total"
    echo "action_kind_counts=$action_kind_counts_inline"
    echo "runtime_perf_health=$runtime_perf_health"
    echo "runtime_perf_bottleneck=$runtime_perf_bottleneck"
    echo "runtime_perf_tick_p95_ms=$runtime_perf_tick_p95_ms"
    echo "runtime_perf_tick_over_budget_ratio_ppm=$runtime_perf_tick_over_budget_ratio_ppm"
    echo "llm_io_logged=$print_llm_io"
    echo "llm_io_max_chars=${llm_io_max_chars:-none}"
    echo "llm_execute_until_auto_reenter_ticks=${llm_execute_until_auto_reenter_ticks:-none}"
    echo "runtime_gameplay_bridge=$runtime_gameplay_bridge"
    echo "runtime_gameplay_preset=${runtime_gameplay_preset:-none}"
    echo "coverage_bootstrap_profile=${coverage_bootstrap_profile:-none}"
    echo "local_provider_bridge=$with_local_provider_bridge"
    echo "local_provider_url=$([[ "$with_local_provider_bridge" == "1" ]] && local_provider_base_url || echo none)"
    echo "agent_provider_profile=$([[ "$with_local_provider_bridge" == "1" ]] && echo "$agent_provider_profile" || echo none)"
    echo "load_state_dir=${load_state_dir:-none}"
    echo "save_state_dir=${save_state_dir:-none}"
    echo "prompt_pack=${prompt_pack:-none}"
    echo "llm_system_prompt_set=$llm_system_prompt_set"
    echo "llm_short_goal_set=$llm_short_goal_set"
    echo "llm_long_goal_set=$llm_long_goal_set"
    echo "prompt_switch_tick=${prompt_switch_tick:-none}"
    echo "switch_llm_system_prompt_set=$switch_llm_system_prompt_set"
    echo "switch_llm_short_goal_set=$switch_llm_short_goal_set"
    echo "switch_llm_long_goal_set=$switch_llm_long_goal_set"
    echo "prompt_switches_json_set=$prompt_switches_json_set"
    echo "report_json=$scenario_report_json"
    echo "run_log=$scenario_log_file"
  } >"$summary_path"
}

run_scenario_to_log() {
  local scenario_name=$1
  local scenario_report_path=$2
  local scenario_run_log_path=$3
  local -a cmd=(
    oasis7_cargo_dev run -p oasis7_provider_tools --bin oasis7_llm_agent_demo --
    "$scenario_name"
    --ticks "$ticks"
    --report-json "$scenario_report_path"
  )
  if (( with_local_provider_bridge == 1 )); then
    cmd+=(
      --decision-source provider_loopback_http
      --agent-provider-url "$(local_provider_base_url)"
      --agent-provider-connect-timeout-ms "$agent_provider_connect_timeout_ms"
      --agent-provider-decision-timeout-ms "$agent_provider_decision_timeout_ms"
      --agent-provider-profile "$agent_provider_profile"
    )
  fi
  if [[ $runtime_gameplay_bridge -eq 1 ]]; then
    cmd+=(--runtime-gameplay-bridge)
  else
    cmd+=(--no-runtime-gameplay-bridge)
  fi
  if [[ -n "$runtime_gameplay_preset" ]]; then
    cmd+=(--runtime-gameplay-preset "$runtime_gameplay_preset")
  fi
  if [[ -n "$coverage_bootstrap_profile" ]]; then
    cmd+=(--coverage-bootstrap-profile "$coverage_bootstrap_profile")
  fi
  if [[ -n "$load_state_dir" ]]; then
    cmd+=(--load-state-dir "$load_state_dir")
  fi
  if [[ -n "$save_state_dir" ]]; then
    cmd+=(--save-state-dir "$save_state_dir")
  fi
  if [[ $print_llm_io -eq 1 ]]; then
    cmd+=(--print-llm-io)
    if [[ -n "$llm_io_max_chars" ]]; then
      cmd+=(--llm-io-max-chars "$llm_io_max_chars")
    fi
  fi
  if [[ -n "$llm_system_prompt" ]]; then
    cmd+=(--llm-system-prompt "$llm_system_prompt")
  fi
  if [[ -n "$llm_short_goal" ]]; then
    cmd+=(--llm-short-term-goal "$llm_short_goal")
  fi
  if [[ -n "$llm_long_goal" ]]; then
    cmd+=(--llm-long-term-goal "$llm_long_goal")
  fi
  if [[ -n "$prompt_switch_tick" ]]; then
    cmd+=(--prompt-switch-tick "$prompt_switch_tick")
  fi
  if [[ -n "$switch_llm_system_prompt" ]]; then
    cmd+=(--switch-llm-system-prompt "$switch_llm_system_prompt")
  fi
  if [[ -n "$switch_llm_short_goal" ]]; then
    cmd+=(--switch-llm-short-term-goal "$switch_llm_short_goal")
  fi
  if [[ -n "$switch_llm_long_goal" ]]; then
    cmd+=(--switch-llm-long-term-goal "$switch_llm_long_goal")
  fi
  if [[ -n "$prompt_switches_json" ]]; then
    cmd+=(--prompt-switches-json "$prompt_switches_json")
  fi

  {
    echo "==== scenario: $scenario_name ===="
    echo "+ ${cmd[*]}"
  } >"$scenario_run_log_path"
  set +e
  if [[ -n "$llm_execute_until_auto_reenter_ticks" ]]; then
    OASIS7_LLM_EXECUTE_UNTIL_AUTO_REENTER_TICKS="$llm_execute_until_auto_reenter_ticks" "${cmd[@]}" >>"$scenario_run_log_path" 2>&1
  else
    "${cmd[@]}" >>"$scenario_run_log_path" 2>&1
  fi
  local run_exit=$?
  set -e
  return "$run_exit"
}

wait_parallel_head_job() {
  local pid=${parallel_pids[0]:-}
  local scenario_name=${parallel_scenarios[0]:-unknown}
  if [[ -z "$pid" ]]; then
    return 0
  fi
  local run_exit=0
  set +e
  wait "$pid"
  run_exit=$?
  set -e

  if (( run_exit != 0 )); then
    echo "pressure run failed for scenario=$scenario_name with exit code $run_exit" >&2
    if (( parallel_failed == 0 )); then
      parallel_failed=1
      parallel_failed_exit=$run_exit
    fi
  fi

  if (( ${#parallel_pids[@]} > 1 )); then
    parallel_pids=("${parallel_pids[@]:1}")
    parallel_scenarios=("${parallel_scenarios[@]:1}")
  else
    parallel_pids=()
    parallel_scenarios=()
  fi
}

validate_gameplay_coverage_for_scenario() {
  local scenario_name=$1

  if (( action_kinds_total < min_action_kinds )); then
    echo "failed: scenario=$scenario_name action_kinds_total($action_kinds_total) < min_action_kinds($min_action_kinds); action_kind_counts=$action_kind_counts_inline" >&2
    exit 14
  fi

  if (( ${#required_action_kinds[@]} == 0 )); then
    return 0
  fi

  local idx
  local kind
  local required_count
  local actual_count
  local -a unmet=()
  for idx in "${!required_action_kinds[@]}"; do
    kind=${required_action_kinds[$idx]}
    required_count=${required_action_min_counts[$idx]}
    actual_count=$(action_kind_count_for "$kind")
    if (( actual_count < required_count )); then
      unmet+=("${kind}:${actual_count}/${required_count}")
    fi
  done

  if (( ${#unmet[@]} > 0 )); then
    echo "failed: scenario=$scenario_name gameplay coverage unmet [${unmet[*]}]; action_kind_counts=$action_kind_counts_inline" >&2
    exit 15
  fi
}

declare -a scenario_inputs=()
ticks="240"
out_dir=".tmp/llm_stress"
report_json=""
log_file=""
summary_file=""
max_llm_errors="0"
max_parse_errors="0"
max_parse_errors_explicit=0
max_repair_rounds_max="2"
min_active_ticks=""
min_action_kinds="0"
print_llm_io=1
llm_io_max_chars=""
keep_out_dir=0
jobs="1"
release_gate=0
release_gate_profile="hybrid"
coverage_bootstrap_profile=""
coverage_bootstrap_profile_explicit=0
llm_system_prompt=""
llm_short_goal=""
llm_long_goal=""
prompt_pack=""
prompt_switch_tick=""
switch_llm_system_prompt=""
switch_llm_short_goal=""
switch_llm_long_goal=""
prompt_switches_json=""
llm_execute_until_auto_reenter_ticks=""
runtime_gameplay_bridge=1
runtime_gameplay_preset=""
load_state_dir=""
save_state_dir=""
with_local_provider_bridge=0
reuse_local_provider_bridge=0
local_provider_bind="127.0.0.1:5841"
local_provider_config=""
local_provider_model=""
local_provider_auto_topup_usd="0.1"
ensure_token_config=1
proxy_url="${OASIS7_LOCAL_TEST_PROXY_URL:-}"
socks_proxy_url="${OASIS7_LOCAL_TEST_SOCKS_PROXY_URL:-}"
use_default_proxy=1
agent_provider_connect_timeout_ms="60000"
agent_provider_decision_timeout_ms="60000"
agent_provider_profile="oasis7_p0_low_freq_npc"
local_provider_bridge_pid=""
local_provider_bridge_log=""
declare -a required_action_kinds=()
declare -a required_action_min_counts=()
required_action_kinds_config="none"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --scenario)
      append_scenario "${2:-}"
      shift 2
      ;;
    --scenarios)
      append_scenarios_from_csv "${2:-}"
      shift 2
      ;;
    --ticks)
      ticks=${2:-}
      shift 2
      ;;
    --jobs)
      jobs=${2:-}
      shift 2
      ;;
    --out-dir)
      out_dir=${2:-}
      shift 2
      ;;
    --report-json)
      report_json=${2:-}
      shift 2
      ;;
    --log-file)
      log_file=${2:-}
      shift 2
      ;;
    --summary-file)
      summary_file=${2:-}
      shift 2
      ;;
    --llm-system-prompt)
      llm_system_prompt=${2:-}
      shift 2
      ;;
    --llm-short-goal)
      llm_short_goal=${2:-}
      shift 2
      ;;
    --llm-long-goal)
      llm_long_goal=${2:-}
      shift 2
      ;;
    --prompt-pack)
      prompt_pack=${2:-}
      shift 2
      ;;
    --prompt-switch-tick)
      prompt_switch_tick=${2:-}
      shift 2
      ;;
    --switch-llm-system-prompt)
      switch_llm_system_prompt=${2:-}
      shift 2
      ;;
    --switch-llm-short-goal)
      switch_llm_short_goal=${2:-}
      shift 2
      ;;
    --switch-llm-long-goal)
      switch_llm_long_goal=${2:-}
      shift 2
      ;;
    --prompt-switches-json)
      prompt_switches_json=${2:-}
      shift 2
      ;;
    --llm-execute-until-auto-reenter-ticks)
      llm_execute_until_auto_reenter_ticks=${2:-}
      shift 2
      ;;
    --runtime-gameplay-bridge)
      runtime_gameplay_bridge=1
      shift
      ;;
    --no-runtime-gameplay-bridge)
      runtime_gameplay_bridge=0
      shift
      ;;
    --runtime-gameplay-preset)
      runtime_gameplay_preset=${2:-}
      shift 2
      ;;
    --load-state-dir)
      load_state_dir=${2:-}
      shift 2
      ;;
    --save-state-dir)
      save_state_dir=${2:-}
      shift 2
      ;;
    --max-llm-errors)
      max_llm_errors=${2:-}
      shift 2
      ;;
    --max-parse-errors)
      max_parse_errors=${2:-}
      max_parse_errors_explicit=1
      shift 2
      ;;
    --max-repair-rounds-max)
      max_repair_rounds_max=${2:-}
      shift 2
      ;;
    --min-active-ticks)
      min_active_ticks=${2:-}
      shift 2
      ;;
    --min-action-kinds)
      min_action_kinds=${2:-}
      shift 2
      ;;
    --require-action-kind)
      append_required_action_kind "${2:-}"
      shift 2
      ;;
    --release-gate)
      release_gate=1
      shift
      ;;
    --release-gate-profile)
      release_gate_profile=${2:-}
      shift 2
      ;;
    --coverage-bootstrap-profile)
      coverage_bootstrap_profile=${2:-}
      coverage_bootstrap_profile_explicit=1
      shift 2
      ;;
    --local-provider-bind)
      with_local_provider_bridge=1
      local_provider_bind=${2:-}
      shift 2
      ;;
    --reuse-local-provider-bridge)
      with_local_provider_bridge=1
      reuse_local_provider_bridge=1
      shift
      ;;
    --proxy)
      proxy_url=${2:-}
      shift 2
      ;;
    --socks-proxy)
      socks_proxy_url=${2:-}
      shift 2
      ;;
    --no-default-proxy)
      use_default_proxy=0
      shift
      ;;
    --agent-provider-connect-timeout-ms)
      agent_provider_connect_timeout_ms=${2:-}
      shift 2
      ;;
    --agent-provider-decision-timeout-ms)
      agent_provider_decision_timeout_ms=${2:-}
      shift 2
      ;;
    --agent-provider-profile)
      agent_provider_profile=${2:-}
      shift 2
      ;;
    --no-llm-io)
      print_llm_io=0
      shift
      ;;
    --llm-io-max-chars)
      llm_io_max_chars=${2:-}
      shift 2
      ;;
    --keep-out-dir)
      keep_out_dir=1
      shift
      ;;
    -h|--help)
      print_help
      exit 0
      ;;
    *)
      echo "unknown option: $1" >&2
      print_help
      exit 2
      ;;
  esac
done

if [[ ${#scenario_inputs[@]} -eq 0 ]]; then
  scenario_inputs=("llm_bootstrap")
fi

declare -a scenarios=()
for candidate in "${scenario_inputs[@]}"; do
  seen=0
  if (( ${#scenarios[@]} > 0 )); then
    for existed in "${scenarios[@]}"; do
      if [[ "$existed" == "$candidate" ]]; then
        seen=1
        break
      fi
    done
  fi
  if (( seen == 0 )); then
    scenarios+=("$candidate")
  fi
done

scenario_count=${#scenarios[@]}
multi_mode=0
if (( scenario_count > 1 )); then
  multi_mode=1
fi
if (( multi_mode == 0 )); then
  jobs=1
elif (( jobs > scenario_count )); then
  jobs=$scenario_count
fi

if (( multi_mode == 1 )) && { [[ -n "$load_state_dir" ]] || [[ -n "$save_state_dir" ]]; }; then
  echo "--load-state-dir/--save-state-dir only support single-scenario mode" >&2
  exit 2
fi

ensure_positive_int "--ticks" "$ticks"
ensure_positive_int "--jobs" "$jobs"
ensure_positive_int "--max-llm-errors" "$max_llm_errors"
ensure_positive_int "--max-parse-errors" "$max_parse_errors"
ensure_positive_int "--max-repair-rounds-max" "$max_repair_rounds_max"
ensure_positive_int "--min-action-kinds" "$min_action_kinds"
ensure_positive_int "--agent-provider-connect-timeout-ms" "$agent_provider_connect_timeout_ms"
ensure_positive_int "--agent-provider-decision-timeout-ms" "$agent_provider_decision_timeout_ms"
if [[ -n "$llm_io_max_chars" ]]; then
  ensure_positive_int "--llm-io-max-chars" "$llm_io_max_chars"
fi
if (( with_local_provider_bridge == 1 )) && [[ -z "$local_provider_bind" || -z "$local_provider_auto_topup_usd" || -z "$agent_provider_profile" ]]; then
  echo "--local-provider-bind, --local-provider-auto-topup-usd, and --agent-provider-profile cannot be empty" >&2
  exit 2
fi
if [[ -n "$prompt_switch_tick" ]]; then
  ensure_positive_int "--prompt-switch-tick" "$prompt_switch_tick"
fi
if [[ -z "$min_active_ticks" ]]; then
  min_active_ticks="$ticks"
fi
ensure_positive_int "--min-active-ticks" "$min_active_ticks"
apply_prompt_pack_defaults
if [[ -n "$runtime_gameplay_preset" ]]; then
  runtime_gameplay_preset=$(printf '%s' "$runtime_gameplay_preset" | tr '[:upper:]' '[:lower:]' | tr '-' '_')
  case "$runtime_gameplay_preset" in
    none)
      runtime_gameplay_preset=""
      ;;
    civic_hotspot_v1)
      ;;
    *)
      echo "invalid --runtime-gameplay-preset: $runtime_gameplay_preset (expected none|civic_hotspot_v1)" >&2
      exit 2
      ;;
  esac
fi
if [[ $runtime_gameplay_bridge -eq 0 && -n "$runtime_gameplay_preset" ]]; then
  echo "--runtime-gameplay-preset requires runtime gameplay bridge to be enabled" >&2
  exit 2
fi
if [[ -n "$llm_execute_until_auto_reenter_ticks" ]]; then
  ensure_positive_int "--llm-execute-until-auto-reenter-ticks" "$llm_execute_until_auto_reenter_ticks"
fi
if [[ -n "$prompt_switches_json" ]]; then
  if [[ -n "$prompt_switch_tick" || -n "$switch_llm_system_prompt" || -n "$switch_llm_short_goal" || -n "$switch_llm_long_goal" ]]; then
    echo "--prompt-switches-json cannot be mixed with --prompt-switch-tick/--switch-llm-*" >&2
    exit 2
  fi
else
  if [[ -n "$switch_llm_system_prompt" || -n "$switch_llm_short_goal" || -n "$switch_llm_long_goal" ]]; then
    if [[ -z "$prompt_switch_tick" ]]; then
      echo "--prompt-switch-tick is required when any --switch-llm-* option is set" >&2
      exit 2
    fi
  fi
  if [[ -n "$prompt_switch_tick" && -z "$switch_llm_system_prompt" && -z "$switch_llm_short_goal" && -z "$switch_llm_long_goal" ]]; then
    echo "--prompt-switch-tick requires at least one --switch-llm-* option" >&2
    exit 2
  fi
fi
apply_release_gate_defaults
if [[ -n "$coverage_bootstrap_profile" ]]; then
  coverage_bootstrap_profile=$(printf '%s' "$coverage_bootstrap_profile" | tr '[:upper:]' '[:lower:]' | tr '-' '_')
  case "$coverage_bootstrap_profile" in
    none)
      coverage_bootstrap_profile=""
      ;;
    industrial|gameplay|hybrid)
      ;;
    *)
      echo "invalid --coverage-bootstrap-profile: $coverage_bootstrap_profile (expected none|industrial|gameplay|hybrid)" >&2
      exit 2
      ;;
  esac
fi
if [[ $runtime_gameplay_bridge -eq 0 ]]; then
  if [[ "$coverage_bootstrap_profile" == "gameplay" || "$coverage_bootstrap_profile" == "hybrid" ]]; then
    echo "--coverage-bootstrap-profile $coverage_bootstrap_profile requires runtime gameplay bridge to be enabled" >&2
    exit 2
  fi
fi
required_action_kinds_config=$(format_required_action_kind_requirements)
if (( release_gate == 1 )) || (( min_action_kinds > 0 )) || (( ${#required_action_kinds[@]} > 0 )); then
  echo "gameplay coverage gate: release_gate=$release_gate profile=$release_gate_profile min_action_kinds=$min_action_kinds required_action_kinds=$required_action_kinds_config coverage_bootstrap_profile=${coverage_bootstrap_profile:-none}"
fi

if [[ -z "$report_json" ]]; then
  report_json="$out_dir/report.json"
fi
if [[ -z "$log_file" ]]; then
  log_file="$out_dir/run.log"
fi
if [[ -z "$summary_file" ]]; then
  summary_file="$out_dir/summary.txt"
fi

if [[ $keep_out_dir -eq 0 ]]; then
  run rm -rf "$out_dir"
fi
run mkdir -p "$out_dir"
run mkdir -p "$(dirname "$report_json")" "$(dirname "$log_file")" "$(dirname "$summary_file")"
start_or_reuse_local_provider_bridge

metrics_tsv="$out_dir/scenario_metrics.tsv"
if [[ $multi_mode -eq 1 ]]; then
  : >"$log_file"
  {
    printf '%s\t' scenario
    printf '%s\t' report_json run_log summary_file
    printf '%s\t' active_ticks total_decisions total_actions action_success action_failure
    printf '%s\t' llm_skipped_ticks llm_skipped_tick_ratio_ppm
    printf '%s\t' llm_errors parse_errors repair_rounds_total repair_rounds_max
    printf '%s\t' llm_input_chars_total llm_input_chars_avg llm_input_chars_max
    printf '%s\t' prompt_section_clipped decision_wait decision_wait_ticks decision_act
    printf '%s\t' module_call plan execute_until_continue action_kinds_total
    printf '%s\t' runtime_perf_health runtime_perf_bottleneck runtime_perf_tick_p95_ms runtime_perf_tick_over_budget_ratio_ppm
    printf '%s\t' llm_io_logged
    printf '%s\n' action_kind_counts
  } >"$metrics_tsv"
fi

parallel_mode=0
if (( multi_mode == 1 && jobs > 1 )); then
  parallel_mode=1
  echo "parallel scenario run enabled: jobs=$jobs"
fi

if (( parallel_mode == 1 )); then
  declare -a parallel_pids=()
  declare -a parallel_scenarios=()
  parallel_failed=0
  parallel_failed_exit=0

  for scenario in "${scenarios[@]}"; do
    scenario_dir="$out_dir/scenarios/$scenario"
    run mkdir -p "$scenario_dir"
    scenario_report_json="$scenario_dir/report.json"
    scenario_log_file="$scenario_dir/run.log"
    run_scenario_to_log "$scenario" "$scenario_report_json" "$scenario_log_file" &
    parallel_pids+=("$!")
    parallel_scenarios+=("$scenario")
    if (( ${#parallel_pids[@]} >= jobs )); then
      wait_parallel_head_job
    fi
  done

  while (( ${#parallel_pids[@]} > 0 )); do
    wait_parallel_head_job
  done

  if (( parallel_failed != 0 )); then
    exit "$parallel_failed_exit"
  fi

  : >"$log_file"
  for scenario in "${scenarios[@]}"; do
    scenario_log_file="$out_dir/scenarios/$scenario/run.log"
    cat "$scenario_log_file" >>"$log_file"
  done
fi

agg_active_ticks=0
agg_total_decisions=0
agg_total_actions=0
agg_action_success=0
agg_action_failure=0
agg_llm_skipped_ticks=0
agg_llm_errors=0
agg_parse_errors=0
agg_repair_rounds_total=0
agg_repair_rounds_max_peak=0
agg_llm_input_chars_total=0
agg_llm_input_chars_avg_sum=0
agg_llm_input_chars_max_peak=0
agg_prompt_section_clipped=0
agg_decision_wait=0
agg_decision_wait_ticks=0
agg_decision_act=0
agg_module_call=0
agg_plan=0
agg_execute_until_continue=0
agg_action_kinds_total=0
agg_action_kinds_peak=0
agg_runtime_perf_tick_p95_ms_peak="0"
agg_runtime_perf_tick_over_budget_ratio_ppm_peak=0

for scenario in "${scenarios[@]}"; do
  if [[ $multi_mode -eq 1 ]]; then
    scenario_dir="$out_dir/scenarios/$scenario"
    run mkdir -p "$scenario_dir"
    scenario_report_json="$scenario_dir/report.json"
    scenario_log_file="$scenario_dir/run.log"
    scenario_summary_file="$scenario_dir/summary.txt"
  else
    scenario_report_json="$report_json"
    scenario_log_file="$log_file"
    scenario_summary_file="$summary_file"
  fi

  if (( parallel_mode == 0 )); then
    cmd=(
      oasis7_cargo_dev run -p oasis7_provider_tools --bin oasis7_llm_agent_demo --
      "$scenario"
      --ticks "$ticks"
      --report-json "$scenario_report_json"
    )
    if (( with_local_provider_bridge == 1 )); then
      cmd+=(
        --decision-source provider_loopback_http
        --agent-provider-url "$(local_provider_base_url)"
        --agent-provider-connect-timeout-ms "$agent_provider_connect_timeout_ms"
        --agent-provider-decision-timeout-ms "$agent_provider_decision_timeout_ms"
        --agent-provider-profile "$agent_provider_profile"
      )
    fi
    if [[ $runtime_gameplay_bridge -eq 1 ]]; then
      cmd+=(--runtime-gameplay-bridge)
    else
      cmd+=(--no-runtime-gameplay-bridge)
    fi
    if [[ -n "$runtime_gameplay_preset" ]]; then
      cmd+=(--runtime-gameplay-preset "$runtime_gameplay_preset")
    fi
    if [[ -n "$coverage_bootstrap_profile" ]]; then
      cmd+=(--coverage-bootstrap-profile "$coverage_bootstrap_profile")
    fi
    if [[ -n "$load_state_dir" ]]; then
      cmd+=(--load-state-dir "$load_state_dir")
    fi
    if [[ -n "$save_state_dir" ]]; then
      cmd+=(--save-state-dir "$save_state_dir")
    fi
    if [[ $print_llm_io -eq 1 ]]; then
      cmd+=(--print-llm-io)
      if [[ -n "$llm_io_max_chars" ]]; then
        cmd+=(--llm-io-max-chars "$llm_io_max_chars")
      fi
    fi
    if [[ -n "$llm_system_prompt" ]]; then
      cmd+=(--llm-system-prompt "$llm_system_prompt")
    fi
    if [[ -n "$llm_short_goal" ]]; then
      cmd+=(--llm-short-term-goal "$llm_short_goal")
    fi
    if [[ -n "$llm_long_goal" ]]; then
      cmd+=(--llm-long-term-goal "$llm_long_goal")
    fi
    if [[ -n "$prompt_switch_tick" ]]; then
      cmd+=(--prompt-switch-tick "$prompt_switch_tick")
    fi
    if [[ -n "$switch_llm_system_prompt" ]]; then
      cmd+=(--switch-llm-system-prompt "$switch_llm_system_prompt")
    fi
    if [[ -n "$switch_llm_short_goal" ]]; then
      cmd+=(--switch-llm-short-term-goal "$switch_llm_short_goal")
    fi
    if [[ -n "$switch_llm_long_goal" ]]; then
      cmd+=(--switch-llm-long-term-goal "$switch_llm_long_goal")
    fi
    if [[ -n "$prompt_switches_json" ]]; then
      cmd+=(--prompt-switches-json "$prompt_switches_json")
    fi

    if [[ $multi_mode -eq 1 ]]; then
      {
        echo "==== scenario: $scenario ===="
        echo "+ ${cmd[*]}"
      } | tee -a "$log_file"
      set +e
      if [[ -n "$llm_execute_until_auto_reenter_ticks" ]]; then
        OASIS7_LLM_EXECUTE_UNTIL_AUTO_REENTER_TICKS="$llm_execute_until_auto_reenter_ticks" "${cmd[@]}" 2>&1 | tee "$scenario_log_file" | tee -a "$log_file"
      else
        "${cmd[@]}" 2>&1 | tee "$scenario_log_file" | tee -a "$log_file"
      fi
      run_exit=${PIPESTATUS[0]}
      set -e
    else
      echo "+ ${cmd[*]} | tee $scenario_log_file"
      set +e
      if [[ -n "$llm_execute_until_auto_reenter_ticks" ]]; then
        OASIS7_LLM_EXECUTE_UNTIL_AUTO_REENTER_TICKS="$llm_execute_until_auto_reenter_ticks" "${cmd[@]}" 2>&1 | tee "$scenario_log_file"
      else
        "${cmd[@]}" 2>&1 | tee "$scenario_log_file"
      fi
      run_exit=${PIPESTATUS[0]}
      set -e
    fi

    if [[ $run_exit -ne 0 ]]; then
      echo "pressure run failed for scenario=$scenario with exit code $run_exit" >&2
      exit $run_exit
    fi
  fi

  if [[ ! -s "$scenario_report_json" ]]; then
    echo "missing report json for scenario=$scenario: $scenario_report_json" >&2
    exit 3
  fi

  load_metrics_from_report "$scenario_report_json" "$scenario_log_file"
  load_action_kind_counts "$scenario_report_json" "$scenario_log_file"
  write_summary_file "$scenario_summary_file" "$scenario"

  echo "pressure summary [$scenario]:"
  cat "$scenario_summary_file"

  if (( active_ticks < min_active_ticks )); then
    echo "failed: scenario=$scenario active_ticks($active_ticks) < min_active_ticks($min_active_ticks)" >&2
    exit 10
  fi
  if (( llm_errors > max_llm_errors )); then
    echo "failed: scenario=$scenario llm_errors($llm_errors) > max_llm_errors($max_llm_errors)" >&2
    exit 11
  fi
  if (( parse_errors > max_parse_errors )); then
    echo "failed: scenario=$scenario parse_errors($parse_errors) > max_parse_errors($max_parse_errors)" >&2
    exit 12
  fi
  if (( repair_rounds_max > max_repair_rounds_max )); then
    echo "failed: scenario=$scenario repair_rounds_max($repair_rounds_max) > max_repair_rounds_max($max_repair_rounds_max)" >&2
    exit 13
  fi
  validate_gameplay_coverage_for_scenario "$scenario"

  agg_active_ticks=$((agg_active_ticks + active_ticks))
  agg_total_decisions=$((agg_total_decisions + total_decisions))
  agg_total_actions=$((agg_total_actions + total_actions))
  agg_action_success=$((agg_action_success + action_success))
  agg_action_failure=$((agg_action_failure + action_failure))
  agg_llm_skipped_ticks=$((agg_llm_skipped_ticks + llm_skipped_ticks))
  agg_llm_errors=$((agg_llm_errors + llm_errors))
  agg_parse_errors=$((agg_parse_errors + parse_errors))
  agg_repair_rounds_total=$((agg_repair_rounds_total + repair_rounds_total))
  agg_llm_input_chars_total=$((agg_llm_input_chars_total + llm_input_chars_total))
  agg_llm_input_chars_avg_sum=$((agg_llm_input_chars_avg_sum + llm_input_chars_avg))
  agg_prompt_section_clipped=$((agg_prompt_section_clipped + clipped_sections))
  agg_decision_wait=$((agg_decision_wait + decision_wait))
  agg_decision_wait_ticks=$((agg_decision_wait_ticks + decision_wait_ticks))
  agg_decision_act=$((agg_decision_act + decision_act))
  agg_module_call=$((agg_module_call + module_call_count))
  agg_plan=$((agg_plan + plan_count))
  agg_execute_until_continue=$((agg_execute_until_continue + execute_until_continue_count))
  agg_action_kinds_total=$((agg_action_kinds_total + action_kinds_total))
  if (( repair_rounds_max > agg_repair_rounds_max_peak )); then
    agg_repair_rounds_max_peak=$repair_rounds_max
  fi
  if (( llm_input_chars_max > agg_llm_input_chars_max_peak )); then
    agg_llm_input_chars_max_peak=$llm_input_chars_max
  fi
  if (( action_kinds_total > agg_action_kinds_peak )); then
    agg_action_kinds_peak=$action_kinds_total
  fi
  if awk -v current="$runtime_perf_tick_p95_ms" -v peak="$agg_runtime_perf_tick_p95_ms_peak" 'BEGIN { exit (current > peak) ? 0 : 1 }'; then
    agg_runtime_perf_tick_p95_ms_peak="$runtime_perf_tick_p95_ms"
  fi
  if (( runtime_perf_tick_over_budget_ratio_ppm > agg_runtime_perf_tick_over_budget_ratio_ppm_peak )); then
    agg_runtime_perf_tick_over_budget_ratio_ppm_peak=$runtime_perf_tick_over_budget_ratio_ppm
  fi

  if [[ $multi_mode -eq 1 ]]; then
    {
      printf '%s\t' "$scenario"
      printf '%s\t' "$scenario_report_json" "$scenario_log_file" "$scenario_summary_file"
      printf '%s\t' "$active_ticks" "$total_decisions" "$total_actions" "$action_success" "$action_failure"
      printf '%s\t' "$llm_skipped_ticks" "$llm_skipped_tick_ratio_ppm"
      printf '%s\t' "$llm_errors" "$parse_errors" "$repair_rounds_total" "$repair_rounds_max"
      printf '%s\t' "$llm_input_chars_total" "$llm_input_chars_avg" "$llm_input_chars_max"
      printf '%s\t' "$clipped_sections" "$decision_wait" "$decision_wait_ticks" "$decision_act"
      printf '%s\t' "$module_call_count" "$plan_count" "$execute_until_continue_count" "$action_kinds_total"
      printf '%s\t' "$runtime_perf_health" "$runtime_perf_bottleneck" "$runtime_perf_tick_p95_ms" "$runtime_perf_tick_over_budget_ratio_ppm"
      printf '%s\t' "$print_llm_io"
      printf '%s\n' "$action_kind_counts_inline"
    } >>"$metrics_tsv"
  fi
done

if [[ $multi_mode -eq 1 ]]; then
  agg_llm_input_chars_avg_mean=$((agg_llm_input_chars_avg_sum / scenario_count))
  agg_llm_skipped_tick_ratio_ppm=0
  if (( agg_active_ticks > 0 )); then
    agg_llm_skipped_tick_ratio_ppm=$((agg_llm_skipped_ticks * 1000000 / agg_active_ticks))
  fi
  scenarios_csv=$(IFS=,; echo "${scenarios[*]}")
  {
    echo "mode=multi_scenario"
    echo "jobs=$jobs"
    echo "scenario_count=$scenario_count"
    echo "scenarios=$scenarios_csv"
    echo "ticks=$ticks"
    echo "active_ticks_total=$agg_active_ticks"
    echo "total_decisions_total=$agg_total_decisions"
    echo "total_actions_total=$agg_total_actions"
    echo "action_success_total=$agg_action_success"
    echo "action_failure_total=$agg_action_failure"
    echo "llm_skipped_ticks_total=$agg_llm_skipped_ticks"
    echo "llm_skipped_tick_ratio_ppm=$agg_llm_skipped_tick_ratio_ppm"
    echo "llm_errors_total=$agg_llm_errors"
    echo "parse_errors_total=$agg_parse_errors"
    echo "repair_rounds_total=$agg_repair_rounds_total"
    echo "repair_rounds_max_peak=$agg_repair_rounds_max_peak"
    echo "llm_input_chars_total=$agg_llm_input_chars_total"
    echo "llm_input_chars_avg_mean=$agg_llm_input_chars_avg_mean"
    echo "llm_input_chars_max_peak=$agg_llm_input_chars_max_peak"
    echo "prompt_section_clipped_total=$agg_prompt_section_clipped"
    echo "decision_wait_total=$agg_decision_wait"
    echo "decision_wait_ticks_total=$agg_decision_wait_ticks"
    echo "decision_act_total=$agg_decision_act"
    echo "module_call_total=$agg_module_call"
    echo "plan_total=$agg_plan"
    echo "execute_until_continue_total=$agg_execute_until_continue"
    echo "action_kinds_total=$agg_action_kinds_total"
    echo "action_kinds_peak=$agg_action_kinds_peak"
    echo "runtime_perf_tick_p95_ms_peak=$agg_runtime_perf_tick_p95_ms_peak"
    echo "runtime_perf_tick_over_budget_ratio_ppm_peak=$agg_runtime_perf_tick_over_budget_ratio_ppm_peak"
    echo "release_gate=$release_gate"
    echo "min_action_kinds=$min_action_kinds"
    echo "required_action_kinds=$required_action_kinds_config"
    echo "llm_io_logged=$print_llm_io"
    echo "llm_io_max_chars=${llm_io_max_chars:-none}"
    echo "runtime_gameplay_bridge=$runtime_gameplay_bridge"
    echo "coverage_bootstrap_profile=${coverage_bootstrap_profile:-none}"
    echo "load_state_dir=${load_state_dir:-none}"
    echo "save_state_dir=${save_state_dir:-none}"
    echo "report_json=$report_json"
    echo "run_log=$log_file"
    echo "per_scenario_dir=$out_dir/scenarios"
  } >"$summary_file"

  if command -v python3 >/dev/null 2>&1; then
    python3 - "$metrics_tsv" "$report_json" "$ticks" "$scenario_count" "$jobs" "$print_llm_io" "${llm_io_max_chars:-}" "$release_gate" "$min_action_kinds" "$required_action_kinds_config" "${coverage_bootstrap_profile:-none}" <<'__PYAGG__'
import csv
import json
import sys
from collections import Counter

(
    metrics_tsv,
    output_path,
    ticks,
    scenario_count,
    jobs,
    llm_io_logged,
    llm_io_max_chars,
    release_gate,
    min_action_kinds,
    required_action_kinds,
    coverage_bootstrap_profile,
) = sys.argv[1:]

int_fields = [
    "active_ticks",
    "total_decisions",
    "total_actions",
    "action_success",
    "action_failure",
    "llm_skipped_ticks",
    "llm_skipped_tick_ratio_ppm",
    "llm_errors",
    "parse_errors",
    "repair_rounds_total",
    "repair_rounds_max",
    "llm_input_chars_total",
    "llm_input_chars_avg",
    "llm_input_chars_max",
    "prompt_section_clipped",
    "decision_wait",
    "decision_wait_ticks",
    "decision_act",
    "module_call",
    "plan",
    "execute_until_continue",
    "action_kinds_total",
    "runtime_perf_tick_over_budget_ratio_ppm",
    "llm_io_logged",
]
float_fields = [
    "runtime_perf_tick_p95_ms",
]

rows = []
with open(metrics_tsv, "r", encoding="utf-8") as fh:
    reader = csv.DictReader(fh, delimiter="\t")
    for row in reader:
        normalized = dict(row)
        for key in int_fields:
            normalized[key] = int(row.get(key, 0) or 0)
        for key in float_fields:
            normalized[key] = float(row.get(key, 0) or 0)
        rows.append(normalized)

def sum_of(key):
    return sum(item[key] for item in rows)

def peak_of(key):
    return max((item[key] for item in rows), default=0)

def peak_float_of(key):
    return max((item[key] for item in rows), default=0.0)

scenario_names = [item["scenario"] for item in rows]
avg_mean = int(sum_of("llm_input_chars_avg") / max(len(rows), 1))
health_counts = Counter(item.get("runtime_perf_health", "unknown") for item in rows)

report = {
    "mode": "multi_scenario",
    "ticks_requested": int(ticks),
    "scenario_count": int(scenario_count),
    "jobs": int(jobs),
    "scenarios": scenario_names,
    "llm_io_logged": int(llm_io_logged),
    "llm_io_max_chars": llm_io_max_chars or "none",
    "coverage_gate": {
        "release_gate": int(release_gate),
        "min_action_kinds": int(min_action_kinds),
        "required_action_kinds": required_action_kinds or "none",
        "coverage_bootstrap_profile": coverage_bootstrap_profile or "none",
    },
    "totals": {
        "active_ticks": sum_of("active_ticks"),
        "total_decisions": sum_of("total_decisions"),
        "total_actions": sum_of("total_actions"),
        "action_success": sum_of("action_success"),
        "action_failure": sum_of("action_failure"),
        "llm_skipped_ticks": sum_of("llm_skipped_ticks"),
        "llm_skipped_tick_ratio_ppm": int(
            (sum_of("llm_skipped_ticks") * 1_000_000) / max(sum_of("active_ticks"), 1)
        ),
        "llm_errors": sum_of("llm_errors"),
        "parse_errors": sum_of("parse_errors"),
        "repair_rounds_total": sum_of("repair_rounds_total"),
        "llm_input_chars_total": sum_of("llm_input_chars_total"),
        "prompt_section_clipped": sum_of("prompt_section_clipped"),
        "decision_wait": sum_of("decision_wait"),
        "decision_wait_ticks": sum_of("decision_wait_ticks"),
        "decision_act": sum_of("decision_act"),
        "module_call": sum_of("module_call"),
        "plan": sum_of("plan"),
        "execute_until_continue": sum_of("execute_until_continue"),
        "action_kinds_total": sum_of("action_kinds_total"),
    },
    "peaks": {
        "repair_rounds_max": peak_of("repair_rounds_max"),
        "llm_input_chars_max": peak_of("llm_input_chars_max"),
        "action_kinds_total": peak_of("action_kinds_total"),
    },
    "runtime_perf": {
        "tick_p95_ms_peak": peak_float_of("runtime_perf_tick_p95_ms"),
        "tick_over_budget_ratio_ppm_peak": peak_of("runtime_perf_tick_over_budget_ratio_ppm"),
        "health_counts": dict(health_counts),
    },
    "means": {
        "llm_input_chars_avg": avg_mean,
    },
    "per_scenario": rows,
}

with open(output_path, "w", encoding="utf-8") as fh:
    json.dump(report, fh, ensure_ascii=False, indent=2)
__PYAGG__
  else
    cat >"$report_json" <<EOF
{
  "mode": "multi_scenario",
  "ticks_requested": $ticks,
  "scenario_count": $scenario_count,
  "jobs": $jobs,
  "coverage_gate": {
    "release_gate": $release_gate,
    "min_action_kinds": $min_action_kinds,
    "required_action_kinds": "$required_action_kinds_config",
    "coverage_bootstrap_profile": "${coverage_bootstrap_profile:-none}"
  }
}
EOF
  fi
fi

echo "pressure summary:"
cat "$summary_file"
echo "pressure run passed"
